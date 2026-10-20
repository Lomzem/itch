//! How the Rig joins its network: the Wi-Fi credentials, and how it takes
//! its IPv4 address there.

use core::fmt;

use crate::wire::{AddressMode, MAX_PASSPHRASE, MAX_SSID, MIN_PASSPHRASE};

/// A network name (SSID) and its WPA2-Personal passphrase, held inline so
/// the core needs no allocator.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Credentials {
    ssid: [u8; MAX_SSID],
    ssid_len: usize,
    passphrase: [u8; MAX_PASSPHRASE],
    passphrase_len: usize,
}

impl Credentials {
    /// The credentials for `ssid`, or `None` unless it is 1 to 32 bytes and
    /// `passphrase` 8 to 63.
    pub fn new(ssid: &str, passphrase: &str) -> Option<Self> {
        if ssid.is_empty() || passphrase.len() < MIN_PASSPHRASE {
            return None;
        }
        let mut credentials = Self {
            ssid: [0; MAX_SSID],
            ssid_len: ssid.len(),
            passphrase: [0; MAX_PASSPHRASE],
            passphrase_len: passphrase.len(),
        };
        credentials.ssid.get_mut(..ssid.len())?.copy_from_slice(ssid.as_bytes());
        credentials.passphrase.get_mut(..passphrase.len())?.copy_from_slice(passphrase.as_bytes());
        Some(credentials)
    }

    pub fn ssid(&self) -> &str {
        core::str::from_utf8(&self.ssid[..self.ssid_len]).expect("SSIDs are UTF-8")
    }

    pub fn passphrase(&self) -> &str {
        core::str::from_utf8(&self.passphrase[..self.passphrase_len])
            .expect("passphrases are UTF-8")
    }
}

/// Shows the network only, so the passphrase never reaches a log.
impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Credentials").field("ssid", &self.ssid()).finish_non_exhaustive()
    }
}

/// How the Rig takes its IPv4 address on the network.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Addressing {
    /// From the network's DHCP server.
    #[default]
    Dhcp,
    /// Exactly the address given, with no conflict check and no fallback.
    Static(StaticAddress),
}

impl Addressing {
    /// The configuration the protocol's `mode`, `address`, `prefix` and
    /// `gateway` describe, or `None` unless DHCP has them all zero, or
    /// static has a unicast address, a prefix of 1 to 32, and a zero or
    /// unicast gateway.
    pub fn new(mode: AddressMode, address: [u8; 4], prefix: u8, gateway: [u8; 4]) -> Option<Self> {
        match mode {
            AddressMode::Dhcp => {
                (address == [0; 4] && prefix == 0 && gateway == [0; 4]).then_some(Self::Dhcp)
            }
            AddressMode::Static => StaticAddress::new(address, prefix, gateway).map(Self::Static),
        }
    }

    pub fn mode(&self) -> AddressMode {
        match self {
            Self::Dhcp => AddressMode::Dhcp,
            Self::Static(_) => AddressMode::Static,
        }
    }

    /// The static address. Zero for DHCP.
    pub fn address(&self) -> [u8; 4] {
        self.as_static().map_or([0; 4], StaticAddress::address)
    }

    /// The static prefix length. Zero for DHCP.
    pub fn prefix(&self) -> u8 {
        self.as_static().map_or(0, StaticAddress::prefix)
    }

    /// The static gateway. Zero for DHCP or no gateway.
    pub fn gateway(&self) -> [u8; 4] {
        self.as_static().and_then(StaticAddress::gateway).unwrap_or_default()
    }

    fn as_static(&self) -> Option<&StaticAddress> {
        match self {
            Self::Dhcp => None,
            Self::Static(address) => Some(address),
        }
    }
}

/// A static IPv4 address: a unicast address, a prefix length of 1 to 32,
/// and an optional unicast gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaticAddress {
    address: [u8; 4],
    prefix: u8,
    /// Zero = none.
    gateway: [u8; 4],
}

impl StaticAddress {
    /// `None` unless `address` is unicast and `prefix` 1 to 32, and
    /// `gateway` is zero, meaning none, or unicast. The network stack
    /// panics on any other address or gateway.
    pub fn new(address: [u8; 4], prefix: u8, gateway: [u8; 4]) -> Option<Self> {
        let valid = is_unicast(address)
            && (1..=32).contains(&prefix)
            && (gateway == [0; 4] || is_unicast(gateway));
        valid.then_some(Self { address, prefix, gateway })
    }

    pub fn address(&self) -> [u8; 4] {
        self.address
    }

    pub fn prefix(&self) -> u8 {
        self.prefix
    }

    /// `None` = no gateway.
    pub fn gateway(&self) -> Option<[u8; 4]> {
        (self.gateway != [0; 4]).then_some(self.gateway)
    }
}

/// Whether `address` is unicast: not zero, not the broadcast address
/// 255.255.255.255, and not multicast (224.0.0.0/4).
fn is_unicast(address: [u8; 4]) -> bool {
    address != [0; 4] && address != [255; 4] && !(224..=239).contains(&address[0])
}
