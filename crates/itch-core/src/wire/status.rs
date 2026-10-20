use super::codec::{Reader, Writer};
use super::{DecodeError, MAX_LAST_PANIC};

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
        })
    }
}
