use super::codec::{Reader, Writer};
use super::{DecodeError, MAX_LABEL};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcquireLease<'a> {
    pub token: u64,
    pub takeover: bool,
    pub holder: &'a str,
}

impl<'a> AcquireLease<'a> {
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.u64(self.token);
        w.bool(self.takeover);
        w.text(self.holder);
    }

    pub(crate) fn decode(r: &mut Reader<'a>) -> Result<Self, DecodeError> {
        Ok(Self { token: r.u64()?, takeover: r.bool()?, holder: r.text(MAX_LABEL)? })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenewLease<'a> {
    pub token: u64,
    pub holder: &'a str,
}

impl<'a> RenewLease<'a> {
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.u64(self.token);
        w.text(self.holder);
    }

    pub(crate) fn decode(r: &mut Reader<'a>) -> Result<Self, DecodeError> {
        Ok(Self { token: r.u64()?, holder: r.text(MAX_LABEL)? })
    }
}
