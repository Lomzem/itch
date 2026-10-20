//! What the Rig advertises over mDNS while it is on the network: the
//! `_itch._tcp` service on [`PORT`], under its Label, with its Hardware ID
//! in the TXT record and a hostname derived from the Hardware ID.

use core::net::{Ipv4Addr, Ipv6Addr};

use edge_mdns::domain::base::Ttl;
use edge_mdns::host::{Host, Service, ServiceAnswers};
use edge_mdns::{HostAnswer, HostAnswers, MdnsError};
use itch_core::label::Label;

use crate::wifi::PORT;

/// How long other hosts may cache the records, in seconds.
const TTL: u32 = 120;

/// The longest instance name: one DNS label. A Label may be a byte longer.
const MAX_INSTANCE: usize = 63;

/// The TXT record's key for the Hardware ID.
pub const HARDWARE_ID_KEY: &str = "hwid";

/// What the Rig is found by that never changes: its hostname and its
/// Hardware ID as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    /// `itch-` plus the last three Hardware ID bytes in lowercase hex.
    hostname: [u8; 11],
    /// The Hardware ID as it is written everywhere, e.g. `60:55:F9:C0:27:18`.
    hardware_id: [u8; 17],
}

impl Identity {
    pub fn new(hardware_id: [u8; 6]) -> Self {
        const UPPER: &[u8; 16] = b"0123456789ABCDEF";
        const LOWER: &[u8; 16] = b"0123456789abcdef";
        let mut hostname = *b"itch-000000";
        for (at, byte) in hardware_id[3..].iter().enumerate() {
            hostname[5 + 2 * at] = LOWER[usize::from(byte >> 4)];
            hostname[6 + 2 * at] = LOWER[usize::from(byte & 0xF)];
        }
        let mut text = [b':'; 17];
        for (at, byte) in hardware_id.iter().enumerate() {
            text[3 * at] = UPPER[usize::from(byte >> 4)];
            text[3 * at + 1] = UPPER[usize::from(byte & 0xF)];
        }
        Self { hostname, hardware_id: text }
    }

    /// The hostname, without `.local`. Never the Label, which need not be
    /// a valid hostname.
    pub fn hostname(&self) -> &str {
        core::str::from_utf8(&self.hostname).expect("ASCII")
    }

    pub fn hardware_id(&self) -> &str {
        core::str::from_utf8(&self.hardware_id).expect("ASCII")
    }
}

/// What the Rig advertises that can change: its Label and its address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Advertisement {
    label: Label,
    address: [u8; 4],
}

impl Advertisement {
    pub fn new(label: Label, address: [u8; 4]) -> Self {
        Self { label, address }
    }

    /// The service instance name: the Label, cut to the 63 bytes a DNS
    /// label holds at the last whole character.
    pub fn instance(&self) -> &str {
        let label = self.label.as_str();
        let end = (0..=MAX_INSTANCE.min(label.len()))
            .rev()
            .find(|&end| label.is_char_boundary(end))
            .unwrap_or(0);
        &label[..end]
    }

    pub fn address(&self) -> [u8; 4] {
        self.address
    }
}

/// The records of an [`Advertisement`], for the responder.
pub struct Answers<'a> {
    identity: &'a Identity,
    advertisement: &'a Advertisement,
    /// Withdraws the service records instead.
    goodbye: bool,
}

impl<'a> Answers<'a> {
    /// The records that announce `advertisement` and answer queries for it.
    pub fn announce(identity: &'a Identity, advertisement: &'a Advertisement) -> Self {
        Self { identity, advertisement, goodbye: false }
    }

    /// The records that withdraw `advertisement`'s service instance, for a
    /// rename: the service records with a TTL of 0. Not the hostname's
    /// address, which stays the same.
    pub fn goodbye(identity: &'a Identity, advertisement: &'a Advertisement) -> Self {
        Self { identity, advertisement, goodbye: true }
    }
}

impl HostAnswers for Answers<'_> {
    fn visit<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnMut(HostAnswer) -> Result<(), E>,
        E: From<MdnsError>,
    {
        let host = Host {
            hostname: self.identity.hostname(),
            // An unspecified address leaves the address record out.
            ipv4: if self.goodbye {
                Ipv4Addr::UNSPECIFIED
            } else {
                Ipv4Addr::from(self.advertisement.address)
            },
            ipv6: Ipv6Addr::UNSPECIFIED,
            ttl: Ttl::from_secs(if self.goodbye { 0 } else { TTL }),
        };
        let txt = [(HARDWARE_ID_KEY, self.identity.hardware_id())];
        let service = Service {
            name: self.advertisement.instance(),
            priority: 0,
            weight: 0,
            service: "_itch",
            protocol: "_tcp",
            port: PORT,
            service_subtypes: &[],
            txt_kvs: &txt,
        };
        ServiceAnswers::new(&host, &service).visit(f)
    }
}
