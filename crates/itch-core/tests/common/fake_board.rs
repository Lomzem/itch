use itch_core::board::{Board, BoardInfo, Counters};

/// A Board for host tests. Its facts match the `hello-ok` vector.
pub struct FakeBoard {
    pub info: BoardInfo,
    /// Every output write, oldest first. Bit 0 = CH1.
    pub writes: Vec<u16>,
    /// The clock in milliseconds, moved by hand.
    pub now: u64,
    pub counters: Counters,
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
                reset_reason: 0x01,
                last_panic: "",
            },
            writes: Vec::new(),
            now: 0,
            counters: Counters::default(),
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

    fn now(&self) -> u64 {
        self.now
    }

    fn counters(&self) -> Counters {
        self.counters
    }
}
