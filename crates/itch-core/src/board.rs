//! The Rig's view of its hardware. Adapters: the real board (firmware),
//! `SimBoard` (itch-sim) and `FakeBoard` (tests).

pub trait Board {
    fn info(&self) -> BoardInfo;

    /// Drives the Channels to `state`, bit 0 = CH1.
    fn write_outputs(&mut self, state: u16);

    /// Milliseconds since boot, monotonic.
    fn now(&self) -> u64;

    /// A snapshot of the counters other tasks keep.
    fn counters(&self) -> Counters;
}

/// Facts fixed for the life of a boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardInfo {
    /// The factory MAC address.
    pub hardware_id: [u8; 6],
    pub firmware_version: &'static str,
    /// Built from uncommitted code.
    pub dirty: bool,
    /// `wire::FEATURE_*` bits.
    pub features: u8,
    pub board_revision: &'static str,
    /// The raw `SocResetReason` of this boot, or 0 if unknown.
    pub reset_reason: u8,
    /// Where the previous boot panicked, at most 64 bytes. Empty = it didn't.
    pub last_panic: &'static str,
}

/// Counters kept outside the Rig, by the transports. Each counts since boot
/// and wraps. See "Counters" in `docs/protocol/errors.md`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counters {
    pub crc_failures: u32,
    pub oversized_frames: u32,
    pub dropped_logs: u32,
    pub usb_tx_dropped: u32,
}
