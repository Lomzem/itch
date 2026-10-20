//! The settings: what the Rig keeps through power cycles and reflashes,
//! which is its label, Wi-Fi credentials and address configuration. They
//! are one versioned record, replaced whole on every save, in a
//! sequential-storage map over the settings partition.

use core::ops::Range;

use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_futures::block_on;
use embedded_storage::nor_flash::{ErrorType, NorFlash, ReadNorFlash};
use itch_core::board::StorageFault;
use itch_core::label::Label;
use itch_core::wifi::{Addressing, Credentials};
use itch_core::wire::{AddressMode, MAX_LABEL, MAX_PASSPHRASE, MAX_SSID};
use sequential_storage::Error;
use sequential_storage::cache::{Cache, Uncached};
use sequential_storage::map::{MapConfig, MapStorage};

/// The settings partition in `partitions.csv`, as flash addresses. It sits
/// after the app partition, so a merged image without padding stops short
/// of it.
pub const PARTITION: Range<u32> = 0x40_0000..0x41_0000;

/// The record layout this firmware writes. It also reads version 1, which
/// held only the label. A record of any other version reads as no record.
pub const VERSION: u16 = 2;
const VERSION_1: u16 = 1;

/// The record's layout: the version (u16 little-endian), then the label,
/// the SSID and the passphrase, each as a `u8` length and its UTF-8 bytes,
/// then the address configuration. Version 1 stopped after the label.
const VERSION_LEN: usize = 2;
/// The IPv4 address configuration, as `SetAddress` carries it: mode (`u8`,
/// 0 = DHCP, 1 = static), address (`[u8; 4]`), prefix (`u8`) and gateway
/// (`[u8; 4]`), all zero for DHCP.
const ADDRESS_LEN: usize = 10;
pub const MAX_RECORD: usize =
    VERSION_LEN + 1 + MAX_LABEL + 1 + MAX_SSID + 1 + MAX_PASSPHRASE + ADDRESS_LEN;

/// Everything the settings hold, saved and loaded whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    /// Empty = the default label.
    pub label: Label,
    /// `None` = no credentials.
    pub wifi: Option<Credentials>,
    pub addressing: Addressing,
}

impl Default for Snapshot {
    /// What a Rig with no record has: the default label, no credentials and
    /// DHCP.
    fn default() -> Self {
        Self {
            label: Label::new("").expect("empty fits"),
            wifi: None,
            addressing: Addressing::Dhcp,
        }
    }
}

/// A settings record.
pub struct Record {
    bytes: [u8; MAX_RECORD],
    len: usize,
}

impl Record {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    fn push(&mut self, bytes: &[u8]) {
        self.bytes[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
    }

    fn push_text(&mut self, text: &str) {
        self.push(&[text.len() as u8]);
        self.push(text.as_bytes());
    }
}

/// The record that saves `snapshot`.
pub fn encode(snapshot: &Snapshot) -> Record {
    let wifi = snapshot.wifi.as_ref();
    let mut record = Record { bytes: [0; MAX_RECORD], len: 0 };
    record.push(&VERSION.to_le_bytes());
    record.push_text(snapshot.label.as_str());
    record.push_text(wifi.map_or("", Credentials::ssid));
    record.push_text(wifi.map_or("", Credentials::passphrase));
    let addressing = &snapshot.addressing;
    record.push(&[addressing.mode() as u8]);
    record.push(&addressing.address());
    record.push(&[addressing.prefix()]);
    record.push(&addressing.gateway());
    record
}

/// The settings `record` saves, or `None` unless it is a whole record of
/// version 1 or 2. A version 1 record has no credentials and DHCP.
pub fn decode(record: &[u8]) -> Option<Snapshot> {
    let (&version, mut rest) = record.split_first_chunk::<VERSION_LEN>()?;
    let label = Label::new(take_text(&mut rest, MAX_LABEL)?)?;
    let (wifi, addressing) = match u16::from_le_bytes(version) {
        VERSION_1 => (None, Addressing::Dhcp),
        VERSION => {
            let ssid = take_text(&mut rest, MAX_SSID)?;
            let passphrase = take_text(&mut rest, MAX_PASSPHRASE)?;
            let wifi = match Credentials::new(ssid, passphrase) {
                None if !(ssid.is_empty() && passphrase.is_empty()) => return None,
                wifi => wifi,
            };
            let (address, tail) = rest.split_first_chunk::<ADDRESS_LEN>()?;
            rest = tail;
            (wifi, take_addressing(address)?)
        }
        _ => return None,
    };
    rest.is_empty().then_some(Snapshot { label, wifi, addressing })
}

/// The address configuration in `bytes`, or `None` unless `SetAddress`
/// could have given it.
fn take_addressing(
    &[mode, a, b, c, d, prefix, e, f, g, h]: &[u8; ADDRESS_LEN],
) -> Option<Addressing> {
    Addressing::new(AddressMode::from_u8(mode)?, [a, b, c, d], prefix, [e, f, g, h])
}

/// Takes a `u8` length and that many bytes of UTF-8, at most `max`, off
/// the front of `bytes`.
fn take_text<'a>(bytes: &mut &'a [u8], max: usize) -> Option<&'a str> {
    let (&len, rest) = bytes.split_first()?;
    let len = usize::from(len);
    if len > max {
        return None;
    }
    let (text, rest) = rest.split_at_checked(len)?;
    *bytes = rest;
    core::str::from_utf8(text).ok()
}

/// sequential-storage's working space: one record, in whole flash words.
#[repr(align(4))]
struct Scratch([u8; MAX_RECORD.next_multiple_of(4)]);

/// The settings on `F`, a NOR flash addressed like the chip's.
pub struct Settings<F: NorFlash> {
    map: MapStorage<(), BlockingAsync<Supervised<F>>, Cache<Uncached, Uncached, Uncached>>,
    scratch: Scratch,
    /// The settings last loaded or saved.
    saved: Snapshot,
}

impl<F: NorFlash> Settings<F> {
    /// Opens the settings on `flash`, reading what is saved. `check_in`
    /// checks `rig` in with the supervisor at a time `now` gave.
    pub fn new(flash: F, now: fn() -> u64, check_in: fn(u64)) -> Self {
        let map = MapStorage::new(
            BlockingAsync::new(Supervised { flash, now, check_in }),
            MapConfig::new(PARTITION),
            Cache::new_uncached(),
        );
        let mut settings = Self {
            map,
            scratch: Scratch([0; MAX_RECORD.next_multiple_of(4)]),
            saved: Snapshot::default(),
        };
        settings.saved = settings.load().unwrap_or_default();
        settings
    }

    /// The settings saved. The defaults if, when the settings were opened,
    /// there was no record or it couldn't be read.
    pub fn saved(&self) -> &Snapshot {
        &self.saved
    }

    /// Saves `snapshot`, returning once it is in flash. A partition too
    /// corrupt to take the record is erased first. On failure the old
    /// settings stay saved.
    pub fn save(&mut self, snapshot: Snapshot) -> Result<(), StorageFault> {
        let record = encode(&snapshot);
        match self.store(&record) {
            Err(Error::Corrupted { .. }) => {
                block_on(self.map.erase_all()).map_err(|_| StorageFault)?;
                self.store(&record)
            }
            saved => saved,
        }
        .map_err(|_| StorageFault)?;
        self.saved = snapshot;
        Ok(())
    }

    fn load(&mut self) -> Option<Snapshot> {
        // `BlockingAsync` never yields, so this finishes without an executor.
        let record = block_on(self.map.fetch_item::<&[u8]>(&mut self.scratch.0, &()));
        record.ok().flatten().and_then(decode)
    }

    fn store(&mut self, record: &Record) -> Result<(), Error<F::Error>> {
        block_on(self.map.store_item(&mut self.scratch.0, &(), &record.as_bytes()))
    }
}

/// A flash on which `rig` checks in with the supervisor right before and
/// after each erase and write, since each one stalls the whole executor.
/// Erases go a sector at a time, so no gap between check-ins is longer
/// than one sector's stall, however long the whole save takes.
struct Supervised<F> {
    flash: F,
    now: fn() -> u64,
    check_in: fn(u64),
}

impl<F: ErrorType> ErrorType for Supervised<F> {
    type Error = F::Error;
}

impl<F: ReadNorFlash> ReadNorFlash for Supervised<F> {
    const READ_SIZE: usize = F::READ_SIZE;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), F::Error> {
        self.flash.read(offset, bytes)
    }

    fn capacity(&self) -> usize {
        self.flash.capacity()
    }
}

impl<F: NorFlash> NorFlash for Supervised<F> {
    const WRITE_SIZE: usize = F::WRITE_SIZE;
    const ERASE_SIZE: usize = F::ERASE_SIZE;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), F::Error> {
        for sector in (from..to).step_by(F::ERASE_SIZE) {
            self.supervised(|flash| flash.erase(sector, sector + F::ERASE_SIZE as u32))?;
        }
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), F::Error> {
        self.supervised(|flash| flash.write(offset, bytes))
    }
}

impl<F> Supervised<F> {
    fn supervised<T>(&mut self, stall: impl FnOnce(&mut F) -> T) -> T {
        (self.check_in)((self.now)());
        let result = stall(&mut self.flash);
        (self.check_in)((self.now)());
        result
    }
}
