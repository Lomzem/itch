use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use itch_core::board::{Board, BoardInfo, Counters};

use crate::logs::DROPPED_LOGS;

/// Frames any connection dropped for a bad CRC: the Rig's `crc_failures`.
pub static CRC_FAILURES: AtomicU32 = AtomicU32::new(0);

/// Frames any connection dropped for being oversized: `oversized_frames`.
pub static OVERSIZED_FRAMES: AtomicU32 = AtomicU32::new(0);

/// The simulated Rig's hardware: everything lives in memory.
pub struct SimBoard {
    info: BoardInfo,
    booted: Instant,
}

impl SimBoard {
    pub fn new() -> Self {
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
        }
    }
}

impl Board for SimBoard {
    fn info(&self) -> BoardInfo {
        self.info
    }

    fn write_outputs(&mut self, _state: u16) {}

    fn now(&self) -> u64 {
        u64::try_from(self.booted.elapsed().as_millis()).unwrap_or(u64::MAX)
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
