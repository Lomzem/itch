use super::codec::{Reader, Writer};
use super::{DecodeError, MAX_HOLDER};

/// A Mask write: Channels whose `mask` bit is set take their `values` bit.
/// Bit 0 is CH1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteChannels<'a> {
    /// Empty = no Holder.
    pub holder: &'a str,
    pub mask: u16,
    pub values: u16,
}

impl<'a> WriteChannels<'a> {
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.text(self.holder);
        w.u16(self.mask);
        w.u16(self.values);
    }

    pub(crate) fn decode(r: &mut Reader<'a>) -> Result<Self, DecodeError> {
        Ok(Self { holder: r.text(MAX_HOLDER)?, mask: r.u16()?, values: r.u16()? })
    }
}
