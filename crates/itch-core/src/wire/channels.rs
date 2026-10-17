use super::DecodeError;
use super::codec::{Reader, Writer};

/// A Mask write: Channels whose `mask` bit is set take their `values` bit.
/// Bit 0 is CH1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteChannels {
    pub token: u64,
    pub mask: u16,
    pub values: u16,
}

impl WriteChannels {
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.u64(self.token);
        w.u16(self.mask);
        w.u16(self.values);
    }

    pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(Self { token: r.u64()?, mask: r.u16()?, values: r.u16()? })
    }
}
