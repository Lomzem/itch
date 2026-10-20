use itch_core::board::{Board, BoardInfo, Counters, HwFault, Network, StorageFault};
use itch_core::label::Label;
use itch_core::wifi::{Addressing, Credentials};
use itch_core::wire::AddressMode;

/// A Board for host tests. Its facts match the `hello-ok` vector.
pub struct FakeBoard {
    pub info: BoardInfo,
    /// Every Channel write the expander took, oldest first. Bit 0 = CH1.
    pub writes: Vec<u16>,
    /// What the expander drives now, as `read_channels` reports it.
    pub channels: u16,
    /// Times the expander was re-initialised, successfully or not.
    pub reinits: u32,
    /// The next expander operation fails, as after an I2C glitch.
    pub glitch: bool,
    /// Every expander operation fails, as if the expander were dead.
    pub unreachable: bool,
    /// Every Channel write breaks off partway, after reaching some latches,
    /// though the expander still re-initialises.
    pub torn_writes: bool,
    /// The clock in milliseconds, moved by hand.
    pub now: u64,
    /// What the logger and transport writers have counted.
    pub counters: Counters,
    /// The label in storage, loaded at boot. `None` = no record.
    pub stored: Option<String>,
    /// Every saved label, oldest first.
    pub saved: Vec<String>,
    /// The Wi-Fi credentials in storage, loaded at boot, as SSID and
    /// passphrase. `None` = none.
    pub wifi: Option<(String, String)>,
    /// Every saved set of Wi-Fi credentials, oldest first. `None` = cleared.
    pub saved_wifi: Vec<Option<(String, String)>>,
    /// The address configuration in storage, loaded at boot, as mode,
    /// address, prefix and gateway.
    pub addressing: (AddressMode, [u8; 4], u8, [u8; 4]),
    /// Every saved address configuration, oldest first.
    pub saved_addressing: Vec<(AddressMode, [u8; 4], u8, [u8; 4])>,
    /// Makes every save fail.
    pub storage_fault: bool,
    /// The Wi-Fi link and address the board reports.
    pub network: Network,
}

impl FakeBoard {
    pub fn new() -> Self {
        Self {
            info: BoardInfo {
                hardware_id: [0x60, 0x55, 0xF9, 0xC0, 0x27, 0x18],
                firmware_version: "1.0.0",
                dirty: false,
                board_revision: "A",
                reset_reason: 0x01,
                last_panic: "",
            },
            writes: Vec::new(),
            channels: 0,
            reinits: 0,
            glitch: false,
            unreachable: false,
            torn_writes: false,
            now: 0,
            counters: Counters::default(),
            stored: None,
            saved: Vec::new(),
            wifi: None,
            saved_wifi: Vec::new(),
            addressing: (AddressMode::Dhcp, [0; 4], 0, [0; 4]),
            saved_addressing: Vec::new(),
            storage_fault: false,
            network: Network::default(),
        }
    }

    fn reach(&mut self) -> Result<(), HwFault> {
        if self.unreachable || std::mem::take(&mut self.glitch) { Err(HwFault) } else { Ok(()) }
    }
}

impl Board for FakeBoard {
    fn info(&self) -> BoardInfo {
        self.info
    }

    fn write_channels(&mut self, state: u16) -> Result<(), HwFault> {
        self.reach()?;
        if self.torn_writes {
            self.channels = state;
            return Err(HwFault);
        }
        self.writes.push(state);
        self.channels = state;
        Ok(())
    }

    fn read_channels(&mut self) -> Result<u16, HwFault> {
        self.reach()?;
        Ok(self.channels)
    }

    fn reinit(&mut self) -> Result<(), HwFault> {
        self.reinits += 1;
        self.channels = 0;
        self.reach()
    }

    fn hold_in_reset(&mut self) {
        self.channels = 0;
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

    fn load_wifi(&mut self) -> Option<Credentials> {
        let (ssid, passphrase) = self.wifi.as_ref()?;
        Credentials::new(ssid, passphrase)
    }

    fn save_wifi(&mut self, wifi: Option<&Credentials>) -> Result<(), StorageFault> {
        if self.storage_fault {
            return Err(StorageFault);
        }
        self.wifi = wifi.map(|wifi| (wifi.ssid().into(), wifi.passphrase().into()));
        self.saved_wifi.push(self.wifi.clone());
        Ok(())
    }

    fn load_addressing(&mut self) -> Addressing {
        let (mode, address, prefix, gateway) = self.addressing;
        Addressing::new(mode, address, prefix, gateway).expect("a valid address configuration")
    }

    fn save_addressing(&mut self, addressing: &Addressing) -> Result<(), StorageFault> {
        if self.storage_fault {
            return Err(StorageFault);
        }
        self.addressing =
            (addressing.mode(), addressing.address(), addressing.prefix(), addressing.gateway());
        self.saved_addressing.push(self.addressing);
        Ok(())
    }

    fn network(&self) -> Network {
        self.network
    }
}
