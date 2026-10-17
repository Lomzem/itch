//! The supervisor: watches the `rig` task and feeds the RTC watchdog while
//! `rig` keeps checking in. Once `rig` misses its deadline the supervisor
//! drives the expander RESET low, latches the failure and stops feeding,
//! so the watchdog resets the chip.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use embedded_hal::digital::{ErrorType, OutputPin};

/// How long `rig` may go without checking in.
pub const DEADLINE_MS: u64 = 1000;

/// How many times `rig` has checked in. It wraps.
pub static CHECK_INS: AtomicU32 = AtomicU32::new(0);

/// Set once `rig` misses its deadline.
pub static TRIPPED: Latch = Latch::new();

/// Called by `rig` on every idle tick.
pub fn check_in() {
    CHECK_INS.fetch_add(1, Ordering::Relaxed);
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
    /// `rig` checked in within its deadline: feed the watchdog.
    Fresh,
    /// `rig` missed its deadline, now or before: keep RESET low and let
    /// the watchdog fire.
    Missed,
}

/// Tracks `rig`'s check-ins against its deadline.
pub struct Supervisor<'a> {
    latch: &'a Latch,
    check_ins: u32,
    /// When `check_ins` last changed, in ms since boot.
    since: u64,
}

impl<'a> Supervisor<'a> {
    /// Starts the deadline at `now`, before `rig`'s first check-in.
    pub fn new(latch: &'a Latch, now: u64) -> Self {
        Self { latch, check_ins: 0, since: now }
    }

    /// Checks `rig` given its check-in count, tripping `latch` on a miss.
    pub fn check(&mut self, check_ins: u32, now: u64) -> Check {
        if check_ins != self.check_ins {
            self.check_ins = check_ins;
            self.since = now;
        }
        if now.saturating_sub(self.since) >= DEADLINE_MS {
            self.latch.trip();
        }
        if self.latch.tripped() { Check::Missed } else { Check::Fresh }
    }
}

/// Feeds the RTC watchdog while `rig` keeps checking in. After a miss it
/// holds the expander in reset until the watchdog resets the chip.
#[cfg(feature = "firmware")]
#[embassy_executor::task]
pub async fn supervisor(mut watchdog: esp_hal::rtc_cntl::Rwdt) {
    use embassy_time::{Duration, Instant, Ticker};

    /// How often the supervisor checks on `rig`.
    const PERIOD: Duration = Duration::from_millis(50);

    let now = || Instant::now().as_millis();
    let mut supervisor = Supervisor::new(&TRIPPED, now());
    let mut ticker = Ticker::every(PERIOD);
    loop {
        match supervisor.check(CHECK_INS.load(Ordering::Relaxed), now()) {
            Check::Fresh => watchdog.feed(),
            Check::Missed => crate::board::force_reset_low(),
        }
        ticker.next().await;
    }
}
