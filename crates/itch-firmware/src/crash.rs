//! The crash record: where a panic happened, kept in RTC RAM through the
//! reset that follows, so the next boot can report it as `last_panic`.

use core::fmt::{self, Write};

/// The longest location kept: `last_panic` is at most 64 bytes.
const MAX_LOCATION: usize = 64;

/// The record's layout: the location's length, the location, a checksum
/// over both, then the marker, last so that a write cut short leaves none.
const LEN: usize = 0;
const LOCATION: usize = LEN + 1;
const CHECKSUM: usize = LOCATION + MAX_LOCATION;
const MARKER: usize = CHECKSUM + 4;
pub const RECORD_LEN: usize = MARKER + 4;

/// Marks a whole record. Change it if the layout changes.
const VALID: [u8; 4] = *b"itch";

pub type Record = [u8; RECORD_LEN];

/// The record of a panic at `file:line:column`. A location over 64 bytes
/// keeps its end, which names the file.
pub fn record(file: &str, line: u32, column: u32) -> Record {
    let mut tail = Tail { bytes: [0; MAX_LOCATION], len: 0 };
    // Tail never fails.
    let _ = write!(tail, "{file}:{line}:{column}");
    // The cut may have split a character: drop what is left of it.
    let start = tail.bytes[..tail.len].iter().take_while(|&&byte| byte & 0xC0 == 0x80).count();
    let location = &tail.bytes[start..tail.len];

    let mut record = [0; RECORD_LEN];
    record[LEN] = location.len() as u8;
    record[LOCATION..LOCATION + location.len()].copy_from_slice(location);
    let checksum = checksum(&record[..CHECKSUM]);
    record[CHECKSUM..MARKER].copy_from_slice(&checksum.to_le_bytes());
    record[MARKER..].copy_from_slice(&VALID);
    record
}

/// Where the panic happened, or `None` unless `record` is a whole record.
pub fn location(record: &Record) -> Option<&str> {
    let len = usize::from(record[LEN]);
    let checksum_ok = record[CHECKSUM..MARKER] == checksum(&record[..CHECKSUM]).to_le_bytes();
    if record[MARKER..] != VALID || len > MAX_LOCATION || !checksum_ok {
        return None;
    }
    core::str::from_utf8(&record[LOCATION..LOCATION + len]).ok()
}

/// FNV-1a: enough to tell a record from noise or a torn write.
fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811C_9DC5, |hash, &byte| (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193))
}

/// Keeps the last [`MAX_LOCATION`] bytes written to it.
struct Tail {
    bytes: [u8; MAX_LOCATION],
    len: usize,
}

impl Write for Tail {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let s = &s.as_bytes()[s.len().saturating_sub(MAX_LOCATION)..];
        let keep = self.len.min(MAX_LOCATION - s.len());
        self.bytes.copy_within(self.len - keep..self.len, 0);
        self.bytes[keep..keep + s.len()].copy_from_slice(s);
        self.len = keep + s.len();
        Ok(())
    }
}
