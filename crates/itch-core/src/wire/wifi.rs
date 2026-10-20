use super::codec::{Reader, Writer};
use super::{DecodeError, MAX_HOLDER, MAX_PASSPHRASE, MAX_SSID};
use crate::wifi::Credentials;

/// Gives the Rig its Wi-Fi credentials, or clears them with an empty `ssid`
/// and `passphrase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetWifi<'a> {
    /// Empty = no Holder.
    pub holder: &'a str,
    /// 1 to 32 bytes, or empty to clear the credentials.
    pub ssid: &'a str,
    /// 8 to 63 bytes, or empty with an empty `ssid`. Never reported back.
    pub passphrase: &'a str,
}

impl<'a> SetWifi<'a> {
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.text(self.holder);
        w.text(self.ssid);
        w.text(self.passphrase);
    }

    pub(crate) fn decode(r: &mut Reader<'a>) -> Result<Self, DecodeError> {
        let set = Self {
            holder: r.text(MAX_HOLDER)?,
            ssid: r.text(MAX_SSID)?,
            passphrase: r.text(MAX_PASSPHRASE)?,
        };
        let clears = set.ssid.is_empty() && set.passphrase.is_empty();
        if clears || set.credentials().is_some() { Ok(set) } else { Err(DecodeError::Malformed) }
    }

    /// The credentials to save. `None` clears them.
    pub fn credentials(&self) -> Option<Credentials> {
        Credentials::new(self.ssid, self.passphrase)
    }
}

/// The Rig's Wi-Fi link, as `GetStatus` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Link {
    /// The radio is off, or the Rig has no credentials.
    #[default]
    Off = 0,
    /// Joining the network, or waiting to retry.
    Joining = 1,
    Connected = 2,
}

impl Link {
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Link::Off,
            1 => Link::Joining,
            2 => Link::Connected,
            _ => return None,
        })
    }

    pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Self::from_u8(r.u8()?).ok_or(DecodeError::Malformed)
    }
}
