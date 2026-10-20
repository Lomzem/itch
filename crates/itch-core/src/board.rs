//! The Rig's view of its hardware. Adapters: the real board (firmware),
//! `SimBoard` (itch-sim) and `FakeBoard` (tests).

use crate::label::Label;
use crate::wifi::{Addressing, Credentials};
use crate::wire::Link;

pub trait Board {
    fn info(&self) -> BoardInfo;

    /// Drives the Channels to `state`, bit 0 = CH1.
    fn write_channels(&mut self, state: u16) -> Result<(), HwFault>;

    /// Reads back the Channels the expander drives, bit 0 = CH1.
    fn read_channels(&mut self) -> Result<u16, HwFault>;

    /// Pulses the expander's RESET and brings it back up with every Channel
    /// off. If it doesn't come up, it is held in reset, so every Channel
    /// stays off.
    fn reinit(&mut self) -> Result<(), HwFault>;

    /// Holds the expander in reset, so every Channel is off, until the next
    /// [`reinit`](Board::reinit).
    fn hold_in_reset(&mut self);

    /// Milliseconds since boot, monotonic.
    fn now(&self) -> u64;

    /// A snapshot of the counters other tasks keep.
    fn counters(&self) -> Counters;

    /// The label last saved, read once at boot. `None` for a missing or
    /// corrupt record; it and an empty label mean the default.
    fn load_label(&mut self) -> Option<Label>;

    /// Persists `label` before returning, so it survives a reboot. Empty
    /// means "report the default"; the default itself is never saved.
    fn save_label(&mut self, label: &str) -> Result<(), StorageFault>;

    /// The Wi-Fi credentials last saved, read once at boot. `None` when
    /// there are none, or for a missing or corrupt record.
    fn load_wifi(&mut self) -> Option<Credentials>;

    /// Persists `wifi` before returning, so it survives a reboot, keeping
    /// every other setting. `None` clears the credentials.
    fn save_wifi(&mut self, wifi: Option<&Credentials>) -> Result<(), StorageFault>;

    /// How the Rig was last set to take its IPv4 address, read once at
    /// boot. DHCP when never set, or for a missing or corrupt record.
    fn load_addressing(&mut self) -> Addressing;

    /// Persists `addressing` before returning, so it survives a reboot,
    /// keeping every other setting.
    fn save_addressing(&mut self, addressing: &Addressing) -> Result<(), StorageFault>;

    /// The Wi-Fi link now, and the address the Rig has on it.
    fn network(&self) -> Network;
}

/// The Channel expander didn't answer or didn't take its configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HwFault;

/// A setting couldn't be saved. Answered with `StorageFault`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageFault;

/// The Rig's Wi-Fi link and its IPv4 address on the network.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Network {
    pub link: Link,
    /// Zero when not connected.
    pub address: [u8; 4],
}

/// Facts fixed for the life of a boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardInfo {
    /// The factory MAC address.
    pub hardware_id: [u8; 6],
    pub firmware_version: &'static str,
    /// Built from uncommitted code.
    pub dirty: bool,
    pub board_revision: &'static str,
    /// The raw `SocResetReason` of this boot, or 0 if unknown.
    pub reset_reason: u8,
    /// Where the previous boot panicked, at most 64 bytes. Empty = it didn't.
    pub last_panic: &'static str,
}

/// Counters kept outside the Rig, by the logger and the transport writers.
/// Each counts since boot and wraps. See "Counters" in
/// `docs/protocol/errors.md`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counters {
    pub dropped_logs: u32,
    pub usb_tx_dropped: u32,
}
