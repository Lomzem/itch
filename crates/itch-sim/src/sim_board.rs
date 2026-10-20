use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use itch_core::board::{Board, BoardInfo, Counters, HwFault, StorageFault};
use itch_core::label::Label;

use crate::logs::DROPPED_LOGS;

/// The simulated Rig's hardware: everything lives in memory.
pub struct SimBoard {
    info: BoardInfo,
    booted: Instant,
    /// How many times faster than real time the clock runs.
    clock_rate: u32,
    /// What the simulated expander drives, bit 0 = CH1.
    channels: u16,
    /// The expander is unreachable, so every operation on it fails.
    hardware_fault: bool,
    /// The saved label, for this run only.
    label: Option<Label>,
    /// Makes every label save fail.
    storage_fault: bool,
}

impl SimBoard {
    pub fn new(clock_rate: u32, hardware_fault: bool, storage_fault: bool) -> Self {
        Self {
            info: BoardInfo {
                // A locally administered MAC, so it can't clash with a real Rig.
                hardware_id: [0x02, 0x00, 0x00, 0x51, 0x4D, 0x01],
                firmware_version: env!("CARGO_PKG_VERSION"),
                dirty: env!("ITCH_DIRTY") == "true",
                features: 0,
                board_revision: "sim",
                // `ChipPowerOn`: every sim run is a fresh boot.
                reset_reason: 0x01,
                last_panic: "",
            },
            booted: Instant::now(),
            clock_rate,
            channels: 0,
            hardware_fault,
            label: None,
            storage_fault,
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

    fn write_channels(&mut self, state: u16) -> Result<(), HwFault> {
        self.expander()?;
        self.channels = state;
        Ok(())
    }

    fn read_channels(&mut self) -> Result<u16, HwFault> {
        self.expander()?;
        Ok(self.channels)
    }

    fn reinit(&mut self) -> Result<(), HwFault> {
        self.channels = 0;
        self.expander()
    }

    fn hold_in_reset(&mut self) {
        self.channels = 0;
    }

    fn now(&self) -> u64 {
        let elapsed = self.booted.elapsed().checked_mul(self.clock_rate).unwrap_or(Duration::MAX);
        u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
    }

    fn counters(&self) -> Counters {
        Counters { dropped_logs: DROPPED_LOGS.load(Ordering::Relaxed), usb_tx_dropped: 0 }
    }

    fn load_label(&mut self) -> Option<Label> {
        self.label
    }

    fn save_label(&mut self, label: &str) -> Result<(), StorageFault> {
        if self.storage_fault {
            return Err(StorageFault);
        }
        self.label = Label::new(label);
        Ok(())
    }
}
