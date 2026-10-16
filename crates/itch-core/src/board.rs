//! The Rig's view of its hardware. Adapters: the real board (firmware),
//! `SimBoard` (itch-sim) and `FakeBoard` (tests).

pub trait Board {
    fn info(&self) -> BoardInfo;

    /// Drives the Channels to `state`, bit 0 = CH1.
    fn write_outputs(&mut self, state: u16);
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
}
