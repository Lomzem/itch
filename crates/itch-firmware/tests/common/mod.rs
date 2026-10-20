#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::convert::Infallible;
use std::rc::Rc;
use std::task::Poll;

use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_futures::block_on;
use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{ErrorType as PinErrorType, OutputPin};
use embedded_hal::i2c::{self, ErrorKind, I2c, NoAcknowledgeSource, Operation};
use embedded_storage::nor_flash::{
    ErrorType as FlashErrorType, NorFlash, NorFlashErrorKind, ReadNorFlash,
};
use itch_core::board::{Board, Counters};
use itch_core::frame::MAX_PAYLOAD;
use itch_core::rig::{Handled, LogControl, Rig};
use itch_core::wire::{
    Link, LogLevel, Message, Reply, Request, Response, SetLabel, SetWifi, WriteChannels,
};
use itch_firmware::board::{Chip, RevA};
use itch_firmware::serve::Clock;
use itch_firmware::settings::PARTITION;
use itch_firmware::supervisor::{Check, Failsafe, Latch, ResetLine, Supervisor};
use itch_firmware::wifi::Wifi;
use sequential_storage::cache::Cache;
use sequential_storage::map::{MapConfig, MapStorage};

const ADDRESS: u8 = 0x20;
pub const IODIRA: u8 = 0x00;
pub const OLATA: u8 = 0x14;
const REGISTERS: usize = 0x16;

/// Something the firmware did to the expander, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// RESET driven low (`true`) or released (`false`).
    Reset(bool),
    /// An I2C write: the register, then the bytes written from it.
    Write(Vec<u8>),
    Read {
        register: u8,
        len: usize,
    },
}

/// A simulated MCP23017 at `0x20`, in its power-on BANK = 0 layout. It
/// ignores the bus while RESET is low, and RESET puts every pin back to an
/// input.
pub struct Expander {
    registers: [u8; REGISTERS],
    pointer: u8,
    in_reset: bool,
    /// Never acknowledges, as if missing or dead.
    pub absent: bool,
    /// Acknowledges writes but ignores them, as if wedged.
    pub stuck: bool,
    /// Writes that set any latch break off after their first data byte, as
    /// when the bus glitches mid-transfer. Clearing writes still go through.
    pub torn: bool,
    /// Every operation since power-on, oldest first.
    pub log: Vec<Op>,
}

impl Expander {
    fn power_on() -> Self {
        let mut expander = Self {
            registers: [0; REGISTERS],
            pointer: 0,
            in_reset: true,
            absent: false,
            stuck: false,
            torn: false,
            log: vec![],
        };
        expander.reset();
        expander
    }

    fn reset(&mut self) {
        self.registers = [0; REGISTERS];
        self.registers[usize::from(IODIRA)] = 0xFF;
        self.registers[usize::from(IODIRA) + 1] = 0xFF;
    }

    /// Loses its configuration as after a brownout: every pin back to an
    /// input and every latch cleared, while RESET stays released.
    pub fn brown_out(&mut self) {
        self.reset();
    }

    /// RESET driven low behind the board's back, as the supervisor does
    /// straight through the GPIO registers.
    pub fn pull_reset(&mut self) {
        self.log.push(Op::Reset(true));
        self.in_reset = true;
        self.reset();
    }

    pub fn in_reset(&self) -> bool {
        self.in_reset
    }

    /// The pins driven high, as `(GPA, GPB)`: outputs whose latch is set.
    pub fn driven(&self) -> (u8, u8) {
        let pins = |port: usize| {
            self.registers[usize::from(OLATA) + port] & !self.registers[usize::from(IODIRA) + port]
        };
        (pins(0), pins(1))
    }

    fn next(&mut self) -> usize {
        let at = usize::from(self.pointer);
        self.pointer = (self.pointer + 1) % REGISTERS as u8;
        at
    }
}

pub struct Bus(Rc<RefCell<Expander>>);

impl i2c::ErrorType for Bus {
    type Error = ErrorKind;
}

impl I2c for Bus {
    fn transaction(&mut self, address: u8, operations: &mut [Operation]) -> Result<(), ErrorKind> {
        let mut expander = self.0.borrow_mut();
        if address != ADDRESS || expander.absent || expander.in_reset {
            return Err(ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address));
        }
        for operation in operations {
            match operation {
                Operation::Write(bytes) => {
                    expander.log.push(Op::Write(bytes.to_vec()));
                    let Some((&register, data)) = bytes.split_first() else { continue };
                    expander.pointer = register;
                    if expander.stuck {
                        continue;
                    }
                    let torn = expander.torn && data.iter().any(|&byte| byte != 0);
                    let data = if torn { &data[..1] } else { data };
                    for &byte in data {
                        let at = expander.next();
                        expander.registers[at] = byte;
                    }
                    if torn {
                        return Err(ErrorKind::NoAcknowledge(NoAcknowledgeSource::Data));
                    }
                }
                Operation::Read(buffer) => {
                    let register = expander.pointer;
                    expander.log.push(Op::Read { register, len: buffer.len() });
                    for byte in buffer.iter_mut() {
                        let at = expander.next();
                        *byte = expander.registers[at];
                    }
                }
            }
        }
        Ok(())
    }
}

pub struct ResetPin(Rc<RefCell<Expander>>);

impl PinErrorType for ResetPin {
    type Error = Infallible;
}

impl OutputPin for ResetPin {
    fn set_low(&mut self) -> Result<(), Infallible> {
        let mut expander = self.0.borrow_mut();
        expander.log.push(Op::Reset(true));
        expander.in_reset = true;
        expander.reset();
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Infallible> {
        let mut expander = self.0.borrow_mut();
        expander.log.push(Op::Reset(false));
        expander.in_reset = false;
        Ok(())
    }
}

pub struct Led(Rc<Cell<bool>>);

impl PinErrorType for Led {
    type Error = Infallible;
}

impl OutputPin for Led {
    fn set_low(&mut self) -> Result<(), Infallible> {
        self.0.set(false);
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Infallible> {
        self.0.set(true);
        Ok(())
    }
}

pub struct NoDelay;

impl DelayNs for NoDelay {
    fn delay_ns(&mut self, _ns: u32) {}
}

thread_local! {
    /// The bench clock in ms since boot, one per test. It only moves when
    /// a test or a flash stall moves it.
    static CLOCK: Cell<u64> = const { Cell::new(0) };
}

/// The bench clock, as the board reads it.
pub fn now() -> u64 {
    CLOCK.with(Cell::get)
}

pub fn advance(ms: u64) {
    CLOCK.with(|clock| clock.set(clock.get() + ms));
}

/// Waits on the bench clock. A wait ends once the test has advanced the
/// clock far enough and polls again.
pub struct BenchClock;

impl Clock for BenchClock {
    async fn until(&self, at: u64) {
        std::future::poll_fn(|_| if now() >= at { Poll::Ready(()) } else { Poll::Pending }).await
    }
}

thread_local! {
    /// The supervisor of the bench's `rig`, one per test, replaced at every
    /// power-on.
    static SUPERVISOR: RefCell<Option<Rc<Supervisor<'static, BenchFailsafe>>>> =
        const { RefCell::new(None) };
    /// When the supervisor last fed the watchdog, by the bench clock.
    static FED: Cell<Option<u64>> = const { Cell::new(None) };
}

/// What the bench's supervisor acts through: a watchdog that only notes
/// when it was fed, and the simulated expander's RESET.
pub struct BenchFailsafe(Rc<RefCell<Expander>>);

impl Failsafe for BenchFailsafe {
    fn feed(&self) {
        FED.with(|fed| fed.set(Some(now())));
    }

    fn force_reset_low(&self) {
        self.0.borrow_mut().pull_reset();
    }
}

fn supervisor() -> Rc<Supervisor<'static, BenchFailsafe>> {
    SUPERVISOR.with(|supervisor| supervisor.borrow().clone().expect("a powered-on bench"))
}

/// `rig` checking in, as the board and the settings do.
fn check_in(now: u64) {
    supervisor().check_in(now);
}

/// How long since the supervisor last fed the watchdog, which it does on
/// every check-in in time. `None` if it never has.
pub fn since_fed() -> Option<u64> {
    FED.with(Cell::get).map(|fed| now() - fed)
}

const SECTOR: usize = 4096;

/// A simulated NOR flash holding the settings partition, which keeps its
/// contents through a power cycle. Erasing sets a 4 KiB sector to `0xFF`
/// and writing can only clear bits, as on the ESP32-S3's flash.
pub struct Flash {
    bytes: Vec<u8>,
    /// Fails every erase and write.
    pub broken: bool,
    /// How long each sector erase and each write stalls the chip.
    pub stall_ms: u64,
    /// How long since the watchdog was last fed, in ms, at the start of
    /// each sector erase and each write, oldest first.
    pub writes: Vec<Option<u64>>,
}

impl Flash {
    /// A partition never written, as on a new module.
    pub fn erased() -> Rc<RefCell<Self>> {
        let len = (PARTITION.end - PARTITION.start) as usize;
        Rc::new(RefCell::new(Self {
            bytes: vec![0xFF; len],
            broken: false,
            stall_ms: 0,
            writes: vec![],
        }))
    }

    /// A partition holding `record` as its settings, the way every firmware
    /// saves them, such as a version 1 record an older firmware left.
    pub fn holding(record: &[u8]) -> Rc<RefCell<Self>> {
        #[repr(align(4))]
        struct Scratch([u8; 256]);
        let flash = Self::erased();
        let mut map = MapStorage::new(
            BlockingAsync::new(FlashPort(Rc::clone(&flash))),
            MapConfig::new(PARTITION),
            Cache::new_uncached(),
        );
        block_on(map.store_item(&mut Scratch([0; 256]).0, &(), &record)).expect("stored");
        flash.borrow_mut().writes.clear();
        flash
    }

    /// Overwrites the whole partition with noise.
    pub fn scramble(&mut self) {
        let mut seed = 0x2545_F491_u32;
        for byte in &mut self.bytes {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            *byte = seed as u8;
        }
    }

    /// The partition offsets of `len` bytes at the physical address `from`.
    fn span(
        &self,
        from: u32,
        len: usize,
        align: usize,
    ) -> Result<std::ops::Range<usize>, NorFlashErrorKind> {
        let start =
            from.checked_sub(PARTITION.start).ok_or(NorFlashErrorKind::OutOfBounds)? as usize;
        if !start.is_multiple_of(align) || !len.is_multiple_of(align) {
            return Err(NorFlashErrorKind::NotAligned);
        }
        if start + len > self.bytes.len() {
            return Err(NorFlashErrorKind::OutOfBounds);
        }
        Ok(start..start + len)
    }

    fn change(&mut self) -> Result<(), NorFlashErrorKind> {
        self.writes.push(since_fed());
        advance(self.stall_ms);
        if self.broken { Err(NorFlashErrorKind::Other) } else { Ok(()) }
    }
}

pub struct FlashPort(Rc<RefCell<Flash>>);

impl FlashErrorType for FlashPort {
    type Error = NorFlashErrorKind;
}

impl ReadNorFlash for FlashPort {
    const READ_SIZE: usize = 4;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), NorFlashErrorKind> {
        let flash = self.0.borrow();
        let span = flash.span(offset, bytes.len(), Self::READ_SIZE)?;
        bytes.copy_from_slice(&flash.bytes[span]);
        Ok(())
    }

    fn capacity(&self) -> usize {
        PARTITION.end as usize
    }
}

impl NorFlash for FlashPort {
    const WRITE_SIZE: usize = 4;
    const ERASE_SIZE: usize = SECTOR;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), NorFlashErrorKind> {
        let mut flash = self.0.borrow_mut();
        let span = flash.span(from, to.saturating_sub(from) as usize, SECTOR)?;
        // The chip erases one sector at a time, each one a stall.
        for _ in span.clone().step_by(SECTOR) {
            flash.change()?;
        }
        flash.bytes[span].fill(0xFF);
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), NorFlashErrorKind> {
        let mut flash = self.0.borrow_mut();
        let span = flash.span(offset, bytes.len(), Self::WRITE_SIZE)?;
        flash.change()?;
        for (old, new) in flash.bytes[span].iter_mut().zip(bytes) {
            *old &= new;
        }
        Ok(())
    }
}

pub type FakeRevA = RevA<Bus, ResetLine<'static, ResetPin>, Led, NoDelay, FlashPort>;

/// The bench's log selection. Nothing here reads its Log Events.
struct Usb;

impl LogControl for Usb {
    fn set_log_level(&self, _: LogLevel) {}
}

/// A Rev A Rig on the bench: the Rig logic over the real board support, with
/// a simulated expander and pins.
pub struct Bench {
    pub expander: Rc<RefCell<Expander>>,
    pub flash: Rc<RefCell<Flash>>,
    led: Rc<Cell<bool>>,
    /// What the radio and the network stack tell the board, and the
    /// credentials the board hands the radio.
    pub wifi: &'static Wifi,
    pub board: FakeRevA,
    pub rig: Rig,
}

impl Bench {
    /// Powers up with RESET held low, as the firmware leaves it at boot.
    pub fn power_on() -> Self {
        Self::power_on_with(Flash::erased())
    }

    /// Powers up on `flash`, as left by an earlier boot.
    pub fn power_on_with(flash: Rc<RefCell<Flash>>) -> Self {
        let expander = Rc::new(RefCell::new(Expander::power_on()));
        let led = Rc::new(Cell::new(false));
        let latch = Box::leak(Box::new(Latch::new()));
        let wifi = Box::leak(Box::new(Wifi::new()));
        let mut board = RevA::new(
            Bus(Rc::clone(&expander)),
            ResetLine::new(ResetPin(Rc::clone(&expander)), latch),
            Led(Rc::clone(&led)),
            NoDelay,
            FlashPort(Rc::clone(&flash)),
            Chip {
                hardware_id: [0x02, 0x00, 0x00, 0xAC, 0xE0, 0x01],
                reset_reason: 0x01,
                last_panic: "",
                now,
                check_in,
                counters: Counters::default,
                wifi,
            },
        );
        let supervisor = Supervisor::new(latch, BenchFailsafe(Rc::clone(&expander)));
        supervisor.start(now());
        SUPERVISOR.with(|cell| cell.replace(Some(Rc::new(supervisor))));
        let rig = Rig::new(&mut board);
        Self { expander, flash, led, wifi, board, rig }
    }

    /// Powers up and brings the expander up, as the firmware does at boot.
    pub fn boot() -> Self {
        Self::boot_with(Flash::erased())
    }

    /// Boots on `flash`, as left by an earlier boot.
    pub fn boot_with(flash: Rc<RefCell<Flash>>) -> Self {
        let mut bench = Self::power_on_with(flash);
        bench.board.reinit().expect("the expander comes up");
        bench
    }

    /// Cuts the power and boots again: only the flash is kept.
    pub fn power_cycle(self) -> Self {
        Self::boot_with(self.flash)
    }

    /// Has the supervisor check on `rig` now, as its task does between
    /// `rig`'s check-ins.
    pub fn supervise(&self) -> Check {
        supervisor().check(now())
    }

    /// Mask-writes the Channels through the Rig, bit 0 = CH1.
    pub fn write(&mut self, mask: u16, values: u16) {
        let request = Request::WriteChannels(WriteChannels { holder: "", mask, values });
        let mut payload = [0; MAX_PAYLOAD];
        let len = Message::Request { request_id: 1, request }.encode(&mut payload);
        let mut reply = [0; MAX_PAYLOAD];
        let handled = self.rig.receive(Ok(&payload[..len]), &mut self.board, &Usb, &mut reply);
        assert!(matches!(handled, Handled::Reply { .. }), "no reply: {handled:?}");
    }

    /// Renames the Rig through the Rig logic: `Ok` with the label it now
    /// reports, or the error code's name.
    pub fn set_label(&mut self, label: &str) -> Result<String, String> {
        let request = Request::SetLabel(SetLabel { holder: "", label });
        self.request(request, |reply| match reply {
            Reply::SetLabel { label } => label.to_owned(),
            reply => panic!("not a SetLabel reply: {reply:?}"),
        })
    }

    /// Gives the Rig Wi-Fi credentials through the Rig logic, or clears
    /// them with an empty `ssid` and `passphrase`: `Ok`, or the error code's
    /// name.
    pub fn set_wifi(&mut self, ssid: &str, passphrase: &str) -> Result<(), String> {
        let request = Request::SetWifi(SetWifi { holder: "", ssid, passphrase });
        self.request(request, |reply| match reply {
            Reply::SetWifi => (),
            reply => panic!("not a SetWifi reply: {reply:?}"),
        })
    }

    /// The network `GetStatus` reports credentials for. `None` = none.
    pub fn wifi(&mut self) -> Option<String> {
        self.request(Request::GetStatus, |reply| match reply {
            Reply::GetStatus(status) => status.wifi_configured.then(|| status.ssid.to_owned()),
            reply => panic!("not a GetStatus reply: {reply:?}"),
        })
        .expect("GetStatus succeeds")
    }

    /// The Wi-Fi link and current address `GetStatus` reports.
    pub fn network(&mut self) -> (Link, [u8; 4]) {
        self.request(Request::GetStatus, |reply| match reply {
            Reply::GetStatus(status) => (status.link, status.current_address),
            reply => panic!("not a GetStatus reply: {reply:?}"),
        })
        .expect("GetStatus succeeds")
    }

    /// The SSID and passphrase the board would join with. `GetStatus` never
    /// reports the passphrase.
    pub fn credentials(&mut self) -> Option<(String, String)> {
        let wifi = self.board.load_wifi()?;
        Some((wifi.ssid().to_owned(), wifi.passphrase().to_owned()))
    }

    /// The label `Hello` reports.
    pub fn label(&mut self) -> String {
        self.request(Request::Hello, |reply| match reply {
            Reply::Hello(hello) => hello.label.to_owned(),
            reply => panic!("not a Hello reply: {reply:?}"),
        })
        .expect("Hello succeeds")
    }

    fn request<T>(&mut self, request: Request, read: impl FnOnce(Reply) -> T) -> Result<T, String> {
        let mut payload = [0; MAX_PAYLOAD];
        let len = Message::Request { request_id: 1, request }.encode(&mut payload);
        let mut reply = [0; MAX_PAYLOAD];
        let len = match self.rig.receive(Ok(&payload[..len]), &mut self.board, &Usb, &mut reply) {
            Handled::Reply { len, .. } => len,
            handled => panic!("no reply: {handled:?}"),
        };
        match Message::decode(&reply[..len]).expect("a valid reply") {
            Message::Response { response: Response::Ok(reply), .. } => Ok(read(reply)),
            Message::Response { response: Response::Err { error, .. }, .. } => {
                Err(format!("{error:?}"))
            }
            message => panic!("not a response: {message:?}"),
        }
    }

    pub fn driven(&self) -> (u8, u8) {
        self.expander.borrow().driven()
    }

    pub fn led(&self) -> bool {
        self.led.get()
    }

    /// Runs one idle tick, as the firmware's `rig` task does.
    pub fn tick(&mut self) {
        self.rig.tick(&mut self.board);
        self.board.tick();
    }

    /// The LED over the next `ticks` idle ticks.
    pub fn led_over(&mut self, ticks: usize) -> Vec<bool> {
        (0..ticks)
            .map(|_| {
                self.tick();
                self.led()
            })
            .collect()
    }
}
