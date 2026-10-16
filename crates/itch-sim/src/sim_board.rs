use itch_core::board::{Board, BoardInfo};

/// The simulated Rig's hardware: everything lives in memory.
pub struct SimBoard {
    info: BoardInfo,
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
            },
        }
    }
}

impl Board for SimBoard {
    fn info(&self) -> BoardInfo {
        self.info
    }
}
