use super::codec::{Reader, Writer};
use super::{DecodeError, MAX_HOLDER};

/// Locks the Rig under `holder`, which is never empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lock<'a> {
    pub holder: &'a str,
    /// Replaces another Holder's Lock rather than being refused.
    pub takeover: bool,
}

impl<'a> Lock<'a> {
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.text(self.holder);
        w.bool(self.takeover);
    }

    pub(crate) fn decode(r: &mut Reader<'a>) -> Result<Self, DecodeError> {
        let holder = r.text(MAX_HOLDER)?;
        if holder.is_empty() {
            return Err(DecodeError::Malformed);
        }
        Ok(Self { holder, takeover: r.bool()? })
    }
}
