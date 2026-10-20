//! The name a Lock is held under.

use crate::wire::MAX_HOLDER;

/// A Holder name, held inline so the core needs no allocator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Holder {
    bytes: [u8; MAX_HOLDER],
    len: usize,
}

impl Holder {
    /// `holder`, or `None` if it is over 64 bytes.
    pub(crate) fn new(holder: &str) -> Option<Self> {
        let mut bytes = [0; MAX_HOLDER];
        bytes.get_mut(..holder.len())?.copy_from_slice(holder.as_bytes());
        Some(Self { bytes, len: holder.len() })
    }

    pub(crate) fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).expect("holders are UTF-8")
    }
}
