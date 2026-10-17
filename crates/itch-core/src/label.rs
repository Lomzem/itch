//! The Rig's name, as `Hello` reports it.

use crate::wire::MAX_LABEL;

/// A Label, held inline so the core needs no allocator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Label {
    bytes: [u8; MAX_LABEL],
    len: usize,
}

impl Label {
    /// `label`, or `None` if it is over 64 bytes.
    pub fn new(label: &str) -> Option<Self> {
        let mut bytes = [0; MAX_LABEL];
        bytes.get_mut(..label.len())?.copy_from_slice(label.as_bytes());
        Some(Self { bytes, len: label.len() })
    }

    /// `itch-` plus the last three bytes of the Hardware ID, e.g. `itch-C02718`.
    pub fn default_for(hardware_id: [u8; 6]) -> Self {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut label = Self { bytes: [0; MAX_LABEL], len: 5 };
        label.bytes[..5].copy_from_slice(b"itch-");
        for byte in &hardware_id[3..] {
            label.bytes[label.len] = HEX[usize::from(byte >> 4)];
            label.bytes[label.len + 1] = HEX[usize::from(byte & 0xF)];
            label.len += 2;
        }
        label
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).expect("labels are UTF-8")
    }
}
