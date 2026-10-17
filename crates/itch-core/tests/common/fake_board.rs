use itch_core::board::{Board, BoardInfo};

/// A Board for host tests. Its facts match the `hello-ok` vector.
pub struct FakeBoard {
    pub info: BoardInfo,
    /// Every output write, oldest first. Bit 0 = CH1.
    pub writes: Vec<u16>,
}

impl FakeBoard {
    pub fn new() -> Self {
        Self {
            info: BoardInfo {
                hardware_id: [0x60, 0x55, 0xF9, 0xC0, 0x27, 0x18],
                firmware_version: "1.0.0",
                dirty: false,
                features: 0,
                board_revision: "A",
            },
            writes: Vec::new(),
        }
    }
}

impl Board for FakeBoard {
    fn info(&self) -> BoardInfo {
        self.info
    }

    fn write_outputs(&mut self, state: u16) {
        self.writes.push(state);
    }
}
