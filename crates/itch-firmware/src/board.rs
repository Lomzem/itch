//! Rev A board support: every chip and board specific lives here. Above
//! this module Channels are canonical, bit 0 = CH1.

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;
use embedded_hal::i2c::I2c;
use itch_core::board::{Board, BoardInfo};

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

/// The Rev A board: an MCP23017 on I2C driving the 16 Channels, with its
/// RESET on a GPIO, and the status LED.
pub struct RevA<I2C, RESET, LED, DELAY> {
    i2c: I2C,
    reset: RESET,
    led: LED,
    delay: DELAY,
    info: BoardInfo,
    /// The Channels last written, bit 0 = CH1.
    outputs: u16,
    /// The last expander operation failed.
    faulted: bool,
    /// Where a fault blink is: lit or not.
    blink: bool,
}

impl<I2C: I2c, RESET: OutputPin, LED: OutputPin, DELAY: DelayNs> RevA<I2C, RESET, LED, DELAY> {
    /// Takes over a board whose expander RESET is already driven low.
    pub fn new(i2c: I2C, reset: RESET, led: LED, delay: DELAY, hardware_id: [u8; 6]) -> Self {
        let info = BoardInfo {
            hardware_id,
            firmware_version: env!("CARGO_PKG_VERSION"),
            dirty: false,
            features: 0,
            board_revision: "A",
        };
        Self { i2c, reset, led, delay, info, outputs: 0, faulted: false, blink: false }
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
