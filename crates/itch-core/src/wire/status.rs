use super::codec::{Reader, Writer};
use super::{AddressMode, DecodeError, Link, MAX_HOLDER, MAX_LAST_PANIC, MAX_SSID};

/// The `GetStatus` result. Counters count since boot and wrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusReply<'a> {
    pub uptime_ms: u64,
    /// The raw `SocResetReason`, or 0 if unknown.
    pub reset_reason: u8,
    /// Empty = no panic.
    pub last_panic: &'a str,
    pub crc_failures: u32,
    pub oversized_frames: u32,
    pub malformed_frames: u32,
    pub dropped_logs: u32,
    pub expander_faults: u32,
    pub usb_tx_dropped: u32,
    /// Who holds the Lock. Empty = unlocked.
    pub holder: &'a str,
    /// The Rig has Wi-Fi credentials.
    pub wifi_configured: bool,
    /// The network the credentials are for. Empty = none. The passphrase is
    /// never reported.
    pub ssid: &'a str,
    pub link: Link,
    /// The Rig's IPv4 address on the network. Zero when not connected.
    pub current_address: [u8; 4],
    /// How the Rig is configured to take its address, whether or not it
    /// has one. The static `address`, `prefix` and `gateway` are zero for
    /// DHCP; a zero `gateway` means none.
    pub mode: AddressMode,
    pub address: [u8; 4],
    pub prefix: u8,
    pub gateway: [u8; 4],
}

impl<'a> StatusReply<'a> {
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.u64(self.uptime_ms);
        w.u8(self.reset_reason);
        w.text(self.last_panic);
        w.u32(self.crc_failures);
        w.u32(self.oversized_frames);
        w.u32(self.malformed_frames);
        w.u32(self.dropped_logs);
        w.u32(self.expander_faults);
        w.u32(self.usb_tx_dropped);
        w.text(self.holder);
        w.bool(self.wifi_configured);
        w.text(self.ssid);
        w.u8(self.link as u8);
        w.bytes(&self.current_address);
        w.u8(self.mode as u8);
        w.bytes(&self.address);
        w.u8(self.prefix);
        w.bytes(&self.gateway);
    }

    pub(crate) fn decode(r: &mut Reader<'a>) -> Result<Self, DecodeError> {
        Ok(Self {
            uptime_ms: r.u64()?,
            reset_reason: r.u8()?,
            last_panic: r.text(MAX_LAST_PANIC)?,
            crc_failures: r.u32()?,
            oversized_frames: r.u32()?,
            malformed_frames: r.u32()?,
            dropped_logs: r.u32()?,
            expander_faults: r.u32()?,
            usb_tx_dropped: r.u32()?,
            holder: r.text(MAX_HOLDER)?,
            wifi_configured: r.bool()?,
            ssid: r.text(MAX_SSID)?,
            link: Link::decode(r)?,
            current_address: r.array()?,
            mode: AddressMode::decode(r)?,
            address: r.array()?,
            prefix: r.u8()?,
            gateway: r.array()?,
        })
    }
}
