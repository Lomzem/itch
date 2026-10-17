//! Rev A board support: every chip and board specific lives here. Above
//! this module Channels are canonical, bit 0 = CH1.

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;
use embedded_hal::i2c::I2c;
use itch_core::board::{Board, BoardInfo, Counters, StorageFault};
use itch_core::label::Label;

/// The MCP23017's I2C address: A0-A2 are tied to GND.
const EXPANDER: u8 = 0x20;

/// MCP23017 registers in the power-on BANK = 0 layout. Each A register is
/// followed by its B register, so one write sets both ports.
const IODIRA: u8 = 0x00;
const OLATA: u8 = 0x14;

#[derive(Clone, Copy)]
enum Port {
    A,
    B,
}

/// The expander pin behind each Channel, CH1 first. The board wires them in
/// reverse: CH1-8 = GPB7..GPB0, CH9-16 = GPA7..GPA0.
const CHANNELS: [(Port, u8); 16] = [
    (Port::B, 7),
    (Port::B, 6),
    (Port::B, 5),
    (Port::B, 4),
    (Port::B, 3),
    (Port::B, 2),
    (Port::B, 1),
    (Port::B, 0),
    (Port::A, 7),
    (Port::A, 6),
    (Port::A, 5),
    (Port::A, 4),
    (Port::A, 3),
    (Port::A, 2),
    (Port::A, 1),
    (Port::A, 0),
];

/// What the board reports from the chip and the rest of the firmware.
#[derive(Debug, Clone, Copy)]
pub struct Chip {
    /// The factory MAC address.
    pub hardware_id: [u8; 6],
    /// The raw `SocResetReason` of this boot, or 0 if unknown.
    pub reset_reason: u8,
    /// Milliseconds since boot.
    pub now: fn() -> u64,
    /// The counters the transports keep.
    pub counters: fn() -> Counters,
}

/// The Rev A board: an MCP23017 on I2C driving the 16 Channels, with its
/// RESET on a GPIO, and the status LED.
pub struct RevA<I2C, RESET, LED, DELAY> {
    i2c: I2C,
    reset: RESET,
    led: LED,
    delay: DELAY,
    info: BoardInfo,
    chip: Chip,
    /// The Channels last written, bit 0 = CH1.
    outputs: u16,
    /// The last expander operation failed.
    faulted: bool,
    /// Where a fault blink is: lit or not.
    blink: bool,
    /// The saved label. An in-RAM stand-in until ticket 15 adds the
    /// flash-backed settings record, so a label lasts until the next reset.
    label: Option<Label>,
}

impl<I2C: I2c, RESET: OutputPin, LED: OutputPin, DELAY: DelayNs> RevA<I2C, RESET, LED, DELAY> {
    /// Takes over a board whose expander RESET is already driven low.
    pub fn new(i2c: I2C, reset: RESET, led: LED, delay: DELAY, chip: Chip) -> Self {
        let info = BoardInfo {
            hardware_id: chip.hardware_id,
            firmware_version: env!("CARGO_PKG_VERSION"),
            dirty: false,
            features: 0,
            board_revision: "A",
            reset_reason: chip.reset_reason,
            last_panic: "",
        };
        Self {
            i2c,
            reset,
            led,
            delay,
            info,
            chip,
            outputs: 0,
            faulted: false,
            blink: false,
            label: None,
        }
    }

    /// Pulses RESET and brings the expander back up with every Channel off.
    /// If it doesn't come up, it is held in reset, which keeps every
    /// contact open.
    pub fn reinit(&mut self) -> Result<(), ExpanderFault> {
        let result = self.bring_up();
        if result.is_err() {
            let _ = self.reset.set_low();
        }
        self.faulted = result.is_err();
        result
    }

    /// Runs on every idle tick: blinks the status LED, once per tick, while
    /// the expander is faulted.
    pub fn tick(&mut self) {
        self.blink = !self.blink;
        self.show_status();
    }

    /// The status LED is on while any Channel is on, and blinks while the
    /// expander is faulted.
    fn show_status(&mut self) {
        let lit = if self.faulted { self.blink } else { self.outputs != 0 };
        let _ = self.led.set_state(lit.into());
    }

    /// Clears the latches before the pins become outputs, so no contact
    /// closes on the way, then reads the configuration back.
    fn bring_up(&mut self) -> Result<(), ExpanderFault> {
        let _ = self.reset.set_low();
        // The MCP23017 needs a RESET pulse of at least 1 µs.
        self.delay.delay_us(10);
        let _ = self.reset.set_high();
        self.delay.delay_us(10);
        self.write_pair(OLATA, [0x00, 0x00])?;
        self.write_pair(IODIRA, [0x00, 0x00])?;
        let mut iodir = [0xFF; 2];
        self.i2c.write_read(EXPANDER, &[IODIRA], &mut iodir).map_err(|_| ExpanderFault)?;
        if iodir != [0x00, 0x00] {
            return Err(ExpanderFault);
        }
        Ok(())
    }

    /// Writes an A register and the B register after it.
    fn write_pair(&mut self, register: u8, [a, b]: [u8; 2]) -> Result<(), ExpanderFault> {
        self.i2c.write(EXPANDER, &[register, a, b]).map_err(|_| ExpanderFault)
    }
}

impl<I2C: I2c, RESET: OutputPin, LED: OutputPin, DELAY: DelayNs> Board
    for RevA<I2C, RESET, LED, DELAY>
{
    fn info(&self) -> BoardInfo {
        self.info
    }

    fn write_outputs(&mut self, state: u16) {
        self.outputs = state;
        self.faulted = self.write_pair(OLATA, ports(state)).is_err();
        self.show_status();
    }

    fn now(&self) -> u64 {
        (self.chip.now)()
    }

    fn counters(&self) -> Counters {
        (self.chip.counters)()
    }

    fn load_label(&mut self) -> Option<Label> {
        self.label
    }

    fn save_label(&mut self, label: &str) -> Result<(), StorageFault> {
        self.label = Label::new(label);
        Ok(())
    }
}

/// The expander didn't answer or didn't take its configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpanderFault;

/// Maps a canonical Channel state onto the expander's `[GPA, GPB]` pins.
fn ports(state: u16) -> [u8; 2] {
    let mut ports = [0; 2];
    for (channel, &(port, pin)) in CHANNELS.iter().enumerate() {
        if state & 1 << channel != 0 {
            ports[port as usize] |= 1 << pin;
        }
    }
    ports
}

#[cfg(feature = "firmware")]
pub use esp32s3::{Parts, RealBoard, UsbRx, UsbTx, boot, force_reset_low};

/// The ESP32-S3 side of the board: pins, peripherals and boot.
#[cfg(feature = "firmware")]
mod esp32s3 {
    use core::sync::atomic::{AtomicU32, Ordering};

    use esp_hal::delay::Delay;
    use esp_hal::gpio::{Level, Output, OutputConfig};
    use esp_hal::i2c::master::{Config, I2c, SoftwareTimeout};
    use esp_hal::peripherals::{GPIO, GPIO6};
    use esp_hal::time::Duration;
    use esp_hal::timer::timg::TimerGroup;
    use esp_hal::usb::usb_serial_jtag::{UsbSerialJtag, UsbSerialJtagRx, UsbSerialJtagTx};
    use esp_hal::{Async, Blocking};
    use itch_core::board::Counters;

    use super::{Chip, RevA};
    use crate::{logger, usb};

    /// The expander's RESET is GPIO6, pulled low on the board.
    const RESET_GPIO: u8 = 6;

    /// The longest one I2C transaction may take before it fails. esp-hal
    /// sets no timeout by default.
    const I2C_TIMEOUT: Duration = Duration::from_millis(10);

    pub type RealBoard = RevA<I2c<'static, Blocking>, Output<'static>, Output<'static>, Delay>;
    pub type UsbRx = UsbSerialJtagRx<'static, Async>;
    pub type UsbTx = UsbSerialJtagTx<'static, Async>;

    /// What the firmware runs on once the chip is up.
    pub struct Parts {
        /// The board, with the expander still held in reset.
        pub board: RealBoard,
        /// USB Serial/JTAG, which keeps espflash's auto-reset on the same cable.
        pub usb_rx: UsbRx,
        pub usb_tx: UsbTx,
    }

    /// Boots the chip. The expander RESET is driven low before anything
    /// else, so every contact stays open until the expander is brought up.
    pub fn boot() -> Parts {
        // A plain GPIO with no pad hold. This Output is GPIO6's only owner:
        // nothing may use `GPIO6` from the peripherals below.
        let reset = Output::new(unsafe { GPIO6::steal() }, Level::Low, OutputConfig::default());
        let p = esp_hal::init(esp_hal::Config::default());
        esp_rtos::start(TimerGroup::new(p.TIMG0).timer0, p.FROM_CPU_INTR0);

        let config =
            Config::default().with_software_timeout(SoftwareTimeout::Transaction(I2C_TIMEOUT));
        let i2c = I2c::new(p.I2C0, config)
            .expect("a valid I2C config")
            .with_sda(p.GPIO5)
            .with_scl(p.GPIO4);
        let led = Output::new(p.GPIO15, Level::Low, OutputConfig::default());
        let chip = Chip {
            hardware_id: esp_hal::efuse::base_mac_address()
                .as_bytes()
                .try_into()
                .expect("a 6-byte MAC address"),
            reset_reason: esp_hal::system::reset_reason().map_or(0, |reason| reason as u8),
            now: || embassy_time::Instant::now().as_millis(),
            counters,
        };
        let (usb_rx, usb_tx) = UsbSerialJtag::new(p.USB_DEVICE).into_async().split();
        Parts { board: RevA::new(i2c, reset, led, Delay::new(), chip), usb_rx, usb_tx }
    }

    fn counters() -> Counters {
        let load = |counter: &AtomicU32| counter.load(Ordering::Relaxed);
        Counters {
            crc_failures: load(&usb::CRC_FAILURES),
            oversized_frames: load(&usb::OVERSIZED_FRAMES),
            dropped_logs: load(&logger::DROPPED_LOGS),
            usb_tx_dropped: load(&usb::TX_DROPPED),
        }
    }

    /// Drives the expander RESET low straight through the GPIO registers,
    /// for the panic path, which may run while its owner is mid-call.
    pub fn force_reset_low() {
        GPIO::regs().out_w1tc().write(|w| unsafe { w.bits(1 << RESET_GPIO) });
    }
}
