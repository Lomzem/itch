//! The Wi-Fi credentials the Rig joins its network with.

use core::fmt;

use crate::wire::{MAX_PASSPHRASE, MAX_SSID, MIN_PASSPHRASE};

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
