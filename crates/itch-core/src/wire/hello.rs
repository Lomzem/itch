use super::codec::{Reader, Writer};
use super::{DecodeError, MAX_LABEL};

/// `features` bit: the firmware was built with the `wifi` feature.
pub const FEATURE_WIFI: u8 = 1 << 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelloReply<'a> {
    pub protocol: u16,
    pub firmware_version: &'a str,
    pub dirty: bool,
    pub features: u8,
    pub channel_count: u8,
    pub board_revision: &'a str,
    pub hardware_id: [u8; 6],
    pub label: &'a str,
}

impl<'a> HelloReply<'a> {
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.u16(self.protocol);
        w.text(self.firmware_version);
        w.bool(self.dirty);
        w.u8(self.features);
        w.u8(self.channel_count);
        w.text(self.board_revision);
        w.bytes(&self.hardware_id);
        w.text(self.label);
    }

    pub(crate) fn decode(r: &mut Reader<'a>) -> Result<Self, DecodeError> {
        Ok(Self {
            protocol: r.u16()?,
            firmware_version: r.text(255)?,
            dirty: r.bool()?,
            features: r.u8()?,
            channel_count: r.u8()?,
            board_revision: r.text(255)?,
            hardware_id: r.array()?,
            label: r.text(MAX_LABEL)?,
        })
    }
}
