use std::time::Instant;

use itch_core::board::{Board, BoardInfo, Counters};

/// The simulated Rig's hardware: everything lives in memory.
pub struct SimBoard {
    info: BoardInfo,
    booted: Instant,
    /// Bumped by the connections, as the firmware's transports would.
    pub counters: Counters,
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
            counters: Counters::default(),
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
        self.counters
    }
}
