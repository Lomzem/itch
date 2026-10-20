use super::codec::{Reader, Writer};
use super::{DecodeError, MAX_LABEL};

/// Renames the Rig. An empty `label` restores the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetLabel<'a> {
    pub label: &'a str,
}

impl<'a> SetLabel<'a> {
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.text(self.label);
    }

    pub(crate) fn decode(r: &mut Reader<'a>) -> Result<Self, DecodeError> {
        Ok(Self { label: r.text(MAX_LABEL)? })
    }
}
