use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use itch_core::board::{Board, BoardInfo, Counters, HwFault};

use crate::logs::DROPPED_LOGS;

/// Frames any connection dropped for a bad CRC: the Rig's `crc_failures`.
pub static CRC_FAILURES: AtomicU32 = AtomicU32::new(0);

/// Frames any connection dropped for being oversized: `oversized_frames`.
pub static OVERSIZED_FRAMES: AtomicU32 = AtomicU32::new(0);

/// The simulated Rig's hardware: everything lives in memory.
pub struct SimBoard {
    info: BoardInfo,
    booted: Instant,
    /// How many times faster than real time the clock runs.
    clock_rate: u32,
    /// What the simulated expander drives, bit 0 = CH1.
    outputs: u16,
    /// The expander is unreachable, so every operation on it fails.
    hardware_fault: bool,
}

impl SimBoard {
    pub fn new(clock_rate: u32, hardware_fault: bool) -> Self {
        Self {
            info: BoardInfo {
                // A locally administered MAC, so it can't clash with a real Rig.
                hardware_id: [0x02, 0x00, 0x00, 0x51, 0x4D, 0x01],
                firmware_version: env!("CARGO_PKG_VERSION"),
                dirty: false,
                features: 0,
                board_revision: "sim",
                // `ChipPowerOn`: every sim run is a fresh boot.
                reset_reason: 0x01,
                last_panic: "",
            },
            booted: Instant::now(),
            clock_rate,
            outputs: 0,
            hardware_fault,
        }
    }
}

impl SimBoard {
    fn expander(&self) -> Result<(), HwFault> {
        if self.hardware_fault { Err(HwFault) } else { Ok(()) }
    }
}

impl Board for SimBoard {
    fn info(&self) -> BoardInfo {
        self.info
    }

    fn write_outputs(&mut self, state: u16) -> Result<(), HwFault> {
        self.expander()?;
        self.outputs = state;
        Ok(())
    }

    fn read_outputs(&mut self) -> Result<u16, HwFault> {
        self.expander()?;
        Ok(self.outputs)
    }

    fn reinit(&mut self) -> Result<(), HwFault> {
        self.outputs = 0;
        self.expander()
    }

    fn now(&self) -> u64 {
        let elapsed = self.booted.elapsed().checked_mul(self.clock_rate).unwrap_or(Duration::MAX);
        u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
    }

    fn counters(&self) -> Counters {
        Counters {
            crc_failures: CRC_FAILURES.load(Ordering::Relaxed),
            oversized_frames: OVERSIZED_FRAMES.load(Ordering::Relaxed),
            dropped_logs: DROPPED_LOGS.load(Ordering::Relaxed),
            usb_tx_dropped: 0,
        }
    }
}
