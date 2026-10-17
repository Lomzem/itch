#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::convert::Infallible;
use std::rc::Rc;

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{ErrorType as PinErrorType, OutputPin};
use embedded_hal::i2c::{self, ErrorKind, I2c, NoAcknowledgeSource, Operation};
use itch_core::board::Counters;
use itch_core::frame::MAX_PAYLOAD;
use itch_core::rig::{Handled, Rig};
use itch_core::wire::{Message, Request, WriteChannels};
use itch_firmware::board::{Chip, RevA};

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
                    for &byte in data {
                        let at = expander.next();
                        expander.registers[at] = byte;
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

pub type FakeRevA = RevA<Bus, ResetPin, Led, NoDelay>;

const TOKEN: u64 = 0x0123_4567_89AB_CDEF;

/// A Rev A Rig on the bench: the Rig logic over the real board support, with
/// a simulated expander and pins.
pub struct Bench {
    pub expander: Rc<RefCell<Expander>>,
    led: Rc<Cell<bool>>,
    pub board: FakeRevA,
    pub rig: Rig,
}

impl Bench {
    /// Powers up with RESET held low, as the firmware leaves it at boot.
    pub fn power_on() -> Self {
        let expander = Rc::new(RefCell::new(Expander::power_on()));
        let led = Rc::new(Cell::new(false));
        let board = RevA::new(
            Bus(Rc::clone(&expander)),
            ResetPin(Rc::clone(&expander)),
            Led(Rc::clone(&led)),
            NoDelay,
            Chip {
                hardware_id: [0x02, 0x00, 0x00, 0xAC, 0xE0, 0x01],
                reset_reason: 0x01,
                now: || 0,
                counters: Counters::default,
            },
        );
        Self { expander, led, board, rig: Rig::new() }
    }

    /// Powers up and brings the expander up, as the firmware does at boot.
    pub fn boot() -> Self {
        let mut bench = Self::power_on();
        bench.board.reinit().expect("the expander comes up");
        bench
    }

    /// Mask-writes the Channels through the Rig, bit 0 = CH1.
    pub fn write(&mut self, mask: u16, values: u16) {
        let request = Request::WriteChannels(WriteChannels { token: TOKEN, mask, values });
        let mut payload = [0; MAX_PAYLOAD];
        let len = Message::Request { request_id: 1, request }.encode(&mut payload);
        let mut reply = [0; MAX_PAYLOAD];
        let handled = self.rig.handle(&payload[..len], &mut self.board, &mut reply);
        assert!(matches!(handled, Handled::Reply { .. }), "no reply: {handled:?}");
    }

    pub fn driven(&self) -> (u8, u8) {
        self.expander.borrow().driven()
    }

    pub fn led(&self) -> bool {
        self.led.get()
    }

    /// The LED over the next `ticks` idle ticks.
    pub fn led_over(&mut self, ticks: usize) -> Vec<bool> {
        (0..ticks)
            .map(|_| {
                self.board.tick();
                self.led()
            })
            .collect()
    }
}
