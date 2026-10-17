//! Primitive field codecs. Everything is little-endian; see "Field types" in
//! `docs/protocol/README.md`.

use super::DecodeError;

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    pub(crate) fn array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let (head, rest) = self.bytes.split_first_chunk().ok_or(DecodeError::Malformed)?;
        self.bytes = rest;
        Ok(*head)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, DecodeError> {
        self.array().map(u8::from_le_bytes)
    }

    pub(crate) fn u16(&mut self) -> Result<u16, DecodeError> {
        self.array().map(u16::from_le_bytes)
    }

    pub(crate) fn u32(&mut self) -> Result<u32, DecodeError> {
        self.array().map(u32::from_le_bytes)
    }

    pub(crate) fn u64(&mut self) -> Result<u64, DecodeError> {
        self.array().map(u64::from_le_bytes)
    }

    pub(crate) fn bool(&mut self) -> Result<bool, DecodeError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DecodeError::Malformed),
        }
    }

    /// A `u8` length, then that many bytes of UTF-8, at most `max` of them.
    pub(crate) fn text(&mut self, max: usize) -> Result<&'a str, DecodeError> {
        let len = usize::from(self.u8()?);
        if len > max || len > self.bytes.len() {
            return Err(DecodeError::Malformed);
        }
        let (text, rest) = self.bytes.split_at(len);
        self.bytes = rest;
        core::str::from_utf8(text).map_err(|_| DecodeError::Malformed)
    }

    pub(crate) fn rest(&mut self) -> &'a [u8] {
        core::mem::take(&mut self.bytes)
    }

    /// Fails on leftover bytes.
    pub(crate) fn finish(self) -> Result<(), DecodeError> {
        if self.bytes.is_empty() { Ok(()) } else { Err(DecodeError::Malformed) }
    }
}

/// Writes into a payload buffer. Overflowing it is a bug: every message is
/// far below the frame cap.
pub(crate) struct Writer<'a> {
    out: &'a mut [u8],
    len: usize,
}

impl<'a> Writer<'a> {
    pub(crate) fn new(out: &'a mut [u8]) -> Self {
        Self { out, len: 0 }
    }

    pub(crate) fn bytes(&mut self, bytes: &[u8]) {
        self.out[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
    }

    pub(crate) fn u8(&mut self, value: u8) {
        self.bytes(&[value]);
    }

    pub(crate) fn u16(&mut self, value: u16) {
        self.bytes(&value.to_le_bytes());
    }

    pub(crate) fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }

    pub(crate) fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }

    pub(crate) fn bool(&mut self, value: bool) {
        self.u8(value.into());
    }

    pub(crate) fn text(&mut self, text: &str) {
        self.u8(u8::try_from(text.len()).expect("text over 255 bytes"));
        self.bytes(text.as_bytes());
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }
}
