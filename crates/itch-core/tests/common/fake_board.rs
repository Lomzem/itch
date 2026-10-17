use itch_core::board::{Board, BoardInfo, Counters, StorageFault};
use itch_core::label::Label;

/// A Board for host tests. Its facts match the `hello-ok` vector.
pub struct FakeBoard {
    pub info: BoardInfo,
    /// Every output write, oldest first. Bit 0 = CH1.
    pub writes: Vec<u16>,
    /// The clock in milliseconds, moved by hand.
    pub now: u64,
    pub counters: Counters,
    /// The label in storage, loaded at boot. `None` = no record.
    pub stored: Option<String>,
    /// Every saved label, oldest first.
    pub saved: Vec<String>,
    /// Makes every save fail.
    pub storage_fault: bool,
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
            stored: None,
            saved: Vec::new(),
            storage_fault: false,
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

    fn load_label(&mut self) -> Option<Label> {
        Label::new(self.stored.as_deref()?)
    }

    fn save_label(&mut self, label: &str) -> Result<(), StorageFault> {
        if self.storage_fault {
            return Err(StorageFault);
        }
        self.stored = Some(label.into());
        self.saved.push(label.into());
        Ok(())
    }
}
