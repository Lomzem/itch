//! The settings: what the Rig keeps through power cycles and reflashes,
//! which in v1 is the label. They are one versioned record, replaced whole
//! on every save, in a sequential-storage map over the settings partition.

use core::ops::Range;

use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_futures::block_on;
use embedded_storage::nor_flash::{ErrorType, NorFlash, ReadNorFlash};
use itch_core::board::StorageFault;
use itch_core::label::Label;
use itch_core::wire::MAX_LABEL;
use sequential_storage::Error;
use sequential_storage::cache::{Cache, Uncached};
use sequential_storage::map::{MapConfig, MapStorage};

use crate::supervisor;

/// The settings partition in `partitions.csv`, as flash addresses. It sits
/// after the app partition, so a merged image without padding stops short
/// of it.
pub const PARTITION: Range<u32> = 0x40_0000..0x41_0000;

/// The record layout this firmware writes. A record of any other version
/// reads as no record.
pub const VERSION: u16 = 1;

/// The record's layout: the version (little-endian), the label's length,
/// then the label.
const VERSION_LEN: usize = 2;
/// Where the label's length sits, straight after the version.
const LABEL_LEN_AT: usize = VERSION_LEN;
const LABEL_AT: usize = LABEL_LEN_AT + 1;
pub const MAX_RECORD: usize = LABEL_AT + MAX_LABEL;

/// A settings record.
pub struct Record {
    bytes: [u8; MAX_RECORD],
    len: usize,
}

impl Record {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// The record that saves `label`.
pub fn encode(label: &Label) -> Record {
    let label = label.as_str().as_bytes();
    let mut bytes = [0; MAX_RECORD];
    bytes[..VERSION_LEN].copy_from_slice(&VERSION.to_le_bytes());
    bytes[LABEL_LEN_AT] = label.len() as u8;
    bytes[LABEL_AT..LABEL_AT + label.len()].copy_from_slice(label);
    Record { bytes, len: LABEL_AT + label.len() }
}

/// The label `record` saves, or `None` unless it is a whole record of this
/// version.
pub fn decode(record: &[u8]) -> Option<Label> {
    let (version, rest) = record.split_first_chunk::<VERSION_LEN>()?;
    let (&len, label) = rest.split_first()?;
    if u16::from_le_bytes(*version) != VERSION || label.len() != usize::from(len) {
        return None;
    }
    core::str::from_utf8(label).ok().and_then(Label::new)
}

/// sequential-storage's working space: one record, in whole flash words.
#[repr(align(4))]
struct Scratch([u8; MAX_RECORD.next_multiple_of(4)]);

/// The settings on `F`, a NOR flash addressed like the chip's.
pub struct Settings<F: NorFlash> {
    map: MapStorage<(), BlockingAsync<Supervised<F>>, Cache<Uncached, Uncached, Uncached>>,
    scratch: Scratch,
}

impl<F: NorFlash> Settings<F> {
    /// `now` is the time in ms since boot, for `rig`'s check-ins.
    pub fn new(flash: F, now: fn() -> u64) -> Self {
        let map = MapStorage::new(
            BlockingAsync::new(Supervised { flash, now }),
            MapConfig::new(PARTITION),
            Cache::new_uncached(),
        );
        Self { map, scratch: Scratch([0; MAX_RECORD.next_multiple_of(4)]) }
    }

    /// The saved label. `None` if there is no record or it can't be read.
    pub fn load(&mut self) -> Option<Label> {
        // `BlockingAsync` never yields, so this finishes without an executor.
        let record = block_on(self.map.fetch_item::<&[u8]>(&mut self.scratch.0, &()));
        record.ok().flatten().and_then(decode)
    }

    /// Saves `label`, returning once it is in flash. A partition too
    /// corrupt to take the record is erased first.
    pub fn save(&mut self, label: &Label) -> Result<(), StorageFault> {
        let record = encode(label);
        match self.store(&record) {
            Err(Error::Corrupted { .. }) => {
                block_on(self.map.erase_all()).map_err(|_| StorageFault)?;
                self.store(&record)
            }
            saved => saved,
        }
        .map_err(|_| StorageFault)
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
        supervisor::check_in((self.now)());
        let result = stall(&mut self.flash);
        supervisor::check_in((self.now)());
        result
    }
}
