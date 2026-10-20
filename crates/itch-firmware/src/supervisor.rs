//! The supervisor: watches the `rig` task and keeps the RTC watchdog fed
//! while `rig` keeps checking in. Once `rig` misses its deadline the
//! supervisor drives the expander RESET low, latches the failure and stops
//! feeding, so the watchdog resets the chip.
//!
//! `rig` and the supervisor share one executor, and a flash operation
//! blocks it, so each check-in carries its own time and feeds the watchdog
//! itself: a save that keeps checking in stays in time however long it
//! takes, with the supervisor shut out.

#[cfg(feature = "firmware")]
use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

#[cfg(feature = "firmware")]
use embassy_sync::blocking_mutex::Mutex;
#[cfg(feature = "firmware")]
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embedded_hal::digital::{ErrorType, OutputPin};
#[cfg(feature = "firmware")]
use esp_hal::rtc_cntl::Rwdt;

/// How long `rig` may go without checking in.
pub const DEADLINE_MS: u64 = 1000;

/// Set once `rig` misses its deadline.
pub static TRIPPED: Latch = Latch::new();

/// The supervisor of the `rig` task, on the chip's watchdog and RESET line.
#[cfg(feature = "firmware")]
pub static SUPERVISOR: Supervisor<'static, ChipFailsafe> = Supervisor::new(&TRIPPED, ChipFailsafe);

/// Called by `rig` on every idle tick and around every flash stall, at
/// `now` ms since boot.
#[cfg(feature = "firmware")]
pub fn check_in(now: u64) {
    SUPERVISOR.check_in(now);
}

/// A failure flag that only a reset clears.
pub struct Latch(AtomicBool);

impl Latch {
    pub const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    pub fn tripped(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    fn trip(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

impl Default for Latch {
    fn default() -> Self {
        Self::new()
    }
}

/// The expander RESET pin, which stays low once `latch` trips, so a `rig`
/// task that runs again before the watchdog fires can't bring the expander
/// back.
pub struct ResetLine<'a, P> {
    pin: P,
    latch: &'a Latch,
}

impl<'a, P: OutputPin> ResetLine<'a, P> {
    pub fn new(pin: P, latch: &'a Latch) -> Self {
        Self { pin, latch }
    }
}

impl<P: OutputPin> ErrorType for ResetLine<'_, P> {
    type Error = P::Error;
}

impl<P: OutputPin> OutputPin for ResetLine<'_, P> {
    fn set_low(&mut self) -> Result<(), P::Error> {
        self.pin.set_low()
    }

    fn set_high(&mut self) -> Result<(), P::Error> {
        if self.latch.tripped() { self.pin.set_low() } else { self.pin.set_high() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// `rig` is within its deadline: the watchdog was fed.
    Fresh,
    /// `rig` missed its deadline, now or before: RESET is held low and the
    /// watchdog will fire.
    Missed,
}

/// What the supervisor acts through.
pub trait Failsafe {
    /// Feeds the watchdog: `rig` is in time.
    fn feed(&self);

    /// Drives the expander RESET low: `rig` missed its deadline. It may run
    /// while the RESET line's owner is mid-call.
    fn force_reset_low(&self);
}

impl<F: Failsafe + ?Sized> Failsafe for &F {
    fn feed(&self) {
        (**self).feed();
    }

    fn force_reset_low(&self) {
        (**self).force_reset_low();
    }
}

/// Tracks `rig`'s check-ins against its deadline and acts on every check:
/// it feeds the watchdog while `rig` is in time, and after a miss holds the
/// expander in reset and stops feeding, so the watchdog resets the chip.
pub struct Supervisor<'a, F> {
    latch: &'a Latch,
    /// When `rig` last checked in, in ms since boot. It wraps after 49
    /// days: the chip has no 64-bit atomics.
    last_check_in: AtomicU32,
    failsafe: F,
}

impl<'a, F: Failsafe> Supervisor<'a, F> {
    pub const fn new(latch: &'a Latch, failsafe: F) -> Self {
        Self { latch, last_check_in: AtomicU32::new(0), failsafe }
    }

    /// Starts the deadline at `now`, before `rig`'s first check-in.
    pub fn start(&self, now: u64) {
        self.last_check_in.store(now as u32, Ordering::Relaxed);
    }

    /// Records `rig` checking in at `now`, tripping `latch` if it comes
    /// [`DEADLINE_MS`] or more after the last one.
    pub fn check_in(&self, now: u64) -> Check {
        self.judge(self.last_check_in.swap(now as u32, Ordering::Relaxed), now)
    }

    /// Checks `rig` at `now`, tripping `latch` once it has gone
    /// [`DEADLINE_MS`] without checking in.
    pub fn check(&self, now: u64) -> Check {
        self.judge(self.last_check_in.load(Ordering::Relaxed), now)
    }

    fn judge(&self, last_check_in: u32, now: u64) -> Check {
        if u64::from((now as u32).wrapping_sub(last_check_in)) >= DEADLINE_MS {
            self.latch.trip();
        }
        if self.latch.tripped() {
            self.failsafe.force_reset_low();
            Check::Missed
        } else {
            self.failsafe.feed();
            Check::Fresh
        }
    }
}

/// The RTC watchdog, fed by both `rig`'s check-ins and the supervisor.
#[cfg(feature = "firmware")]
static WATCHDOG: Mutex<CriticalSectionRawMutex, RefCell<Option<Rwdt>>> =
    Mutex::new(RefCell::new(None));

/// The chip's RTC watchdog, once the supervisor has it, and the expander
/// RESET GPIO.
#[cfg(feature = "firmware")]
pub struct ChipFailsafe;

#[cfg(feature = "firmware")]
impl Failsafe for ChipFailsafe {
    fn feed(&self) {
        WATCHDOG.lock(|watchdog| {
            if let Some(watchdog) = watchdog.borrow_mut().as_mut() {
                watchdog.feed();
            }
        });
    }

    fn force_reset_low(&self) {
        crate::board::force_reset_low();
    }
}

/// Checks on `rig` between its check-ins, which catches a `rig` that
/// stopped checking in at all.
#[cfg(feature = "firmware")]
#[embassy_executor::task]
pub async fn supervisor(watchdog: Rwdt) {
    use embassy_time::{Duration, Instant, Ticker};

    /// How often the supervisor checks on `rig`.
    const PERIOD: Duration = Duration::from_millis(50);

    let now = || Instant::now().as_millis();
    WATCHDOG.lock(|cell| cell.replace(Some(watchdog)));
    SUPERVISOR.start(now());
    let mut ticker = Ticker::every(PERIOD);
    loop {
        SUPERVISOR.check(now());
        ticker.next().await;
    }
}
