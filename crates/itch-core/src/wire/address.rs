use super::codec::{Reader, Writer};
use super::{DecodeError, MAX_HOLDER};
use crate::wifi::Addressing;

/// Sets how the Rig takes its IPv4 address: by DHCP, with `address`,
/// `prefix` and `gateway` all zero, or static.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetAddress<'a> {
    /// Empty = no Holder.
    pub holder: &'a str,
    pub mode: AddressMode,
    /// Unicast when static.
    pub address: [u8; 4],
    /// 1 to 32 when static.
    pub prefix: u8,
    /// Zero = no gateway. Otherwise unicast.
    pub gateway: [u8; 4],
}

impl<'a> SetAddress<'a> {
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.text(self.holder);
        w.u8(self.mode as u8);
        w.bytes(&self.address);
        w.u8(self.prefix);
        w.bytes(&self.gateway);
    }

    pub(crate) fn decode(r: &mut Reader<'a>) -> Result<Self, DecodeError> {
        let set = Self {
            holder: r.text(MAX_HOLDER)?,
            mode: AddressMode::decode(r)?,
            address: r.array()?,
            prefix: r.u8()?,
            gateway: r.array()?,
        };
        set.addressing().map(|_| set).ok_or(DecodeError::Malformed)
    }

    /// The configuration to save. `None` if the fields don't make one.
    pub fn addressing(&self) -> Option<Addressing> {
        Addressing::new(self.mode, self.address, self.prefix, self.gateway)
    }
}

/// How the Rig takes its IPv4 address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum AddressMode {
    #[default]
    Dhcp = 0,
    Static = 1,
}

impl AddressMode {
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => AddressMode::Dhcp,
            1 => AddressMode::Static,
            _ => return None,
        })
    }

    pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Self::from_u8(r.u8()?).ok_or(DecodeError::Malformed)
    }
}
