use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::Ordering;
use std::time::Instant;

use itch_core::board::{Board, BoardInfo, Counters, HwFault, Network, StorageFault};
use itch_core::label::Label;
use itch_core::wifi::Credentials;
use itch_core::wire::Link;

use crate::logs::DROPPED_LOGS;

/// The simulated Rig's hardware: everything lives in memory.
pub struct SimBoard {
    info: BoardInfo,
    booted: Instant,
    /// What the simulated expander drives, bit 0 = CH1.
    channels: u16,
    /// The expander is unreachable, so every operation on it fails.
    hardware_fault: bool,
    /// The saved label, for this run only.
    label: Option<Label>,
    /// The saved Wi-Fi credentials, for this run only.
    wifi: Option<Credentials>,
    /// Makes every save fail.
    storage_fault: bool,
    /// The IPv4 address the sim is bound to, reported as the Rig's address
    /// while it has credentials. Zero if it isn't bound to one.
    address: [u8; 4],
}

impl SimBoard {
    /// `bound` is the address the sim listens on.
    pub fn new(hardware_fault: bool, storage_fault: bool, bound: SocketAddr) -> Self {
        Self {
            info: BoardInfo {
                // A locally administered MAC, so it can't clash with a real Rig.
                hardware_id: [0x02, 0x00, 0x00, 0x51, 0x4D, 0x01],
                firmware_version: env!("CARGO_PKG_VERSION"),
                dirty: env!("ITCH_DIRTY") == "true",
                board_revision: "sim",
                // `ChipPowerOn`: every sim run is a fresh boot.
                reset_reason: 0x01,
                last_panic: "",
            },
            booted: Instant::now(),
            channels: 0,
            hardware_fault,
            label: None,
            wifi: None,
            storage_fault,
            address: match bound.ip() {
                IpAddr::V4(address) => address.octets(),
                IpAddr::V6(_) => [0; 4],
            },
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
        u64::try_from(self.booted.elapsed().as_millis()).unwrap_or(u64::MAX)
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

    fn load_wifi(&mut self) -> Option<Credentials> {
        self.wifi
    }

    fn save_wifi(&mut self, wifi: Option<&Credentials>) -> Result<(), StorageFault> {
        if self.storage_fault {
            return Err(StorageFault);
        }
        self.wifi = wifi.copied();
        Ok(())
    }

    /// With credentials the sim is "connected" at the address it is bound
    /// to; it joins no real network.
    fn network(&self) -> Network {
        match self.wifi {
            Some(_) => Network { link: Link::Connected, address: self.address },
            None => Network::default(),
        }
    }
}
