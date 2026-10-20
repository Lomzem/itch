//! Where the Wi-Fi Transport meets the board: the credentials the board
//! hands the radio, the Label it hands the mDNS responder, and what the
//! radio and the network stack report for `GetStatus` and mDNS.

use core::cell::Cell;
use core::fmt;

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use itch_core::board::Network;
use itch_core::label::Label;
use itch_core::wifi::Credentials;
use itch_core::wire::Link;

use crate::mdns::Advertisement;

/// The port clients connect to: `tcp://<address>:7878`.
pub const PORT: u16 = 7878;

/// The Rig's Wi-Fi.
#[cfg(feature = "firmware")]
pub static WIFI: Wifi = Wifi::new();

/// What the radio is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Radio {
    /// Off: there are no credentials, or the radio didn't start.
    Off,
    /// On and trying to join the network, or waiting to try again.
    Joining,
    /// Joined to the network.
    Joined,
}

/// The Rig's Wi-Fi: the credentials the radio joins with, the Label mDNS
/// advertises, and the link as the radio and the network stack report it.
pub struct Wifi {
    /// The newest credentials the board handed over, until the radio takes
    /// them. `None` = none.
    credentials: Signal<CriticalSectionRawMutex, Option<Credentials>>,
    state: Mutex<CriticalSectionRawMutex, Cell<State>>,
    /// The Label the board last handed over. `None` until it has.
    label: Mutex<CriticalSectionRawMutex, Cell<Option<Label>>>,
    /// Something mDNS advertises may have changed.
    changed: Signal<CriticalSectionRawMutex, ()>,
}

#[derive(Clone, Copy)]
struct State {
    radio: Radio,
    /// The network stack's IPv4 address: a DHCP lease, or a static address
    /// whether or not the Rig is joined.
    address: Option<[u8; 4]>,
}

impl Wifi {
    pub const fn new() -> Self {
        Self {
            credentials: Signal::new(),
            state: Mutex::new(Cell::new(State { radio: Radio::Off, address: None })),
            label: Mutex::new(Cell::new(None)),
            changed: Signal::new(),
        }
    }

    /// Has the radio leave the network it is on, if any, and join with
    /// `credentials` from now on. `None` turns the radio off.
    pub fn join(&self, credentials: Option<Credentials>) {
        self.credentials.signal(credentials);
    }

    /// Waits for the radio's next credentials, the newest the board handed
    /// over.
    pub async fn credentials(&self) -> Option<Credentials> {
        self.credentials.wait().await
    }

    /// Has mDNS advertise the Rig under `label` from now on.
    pub fn advertise(&self, label: Label) {
        self.label.lock(|cell| cell.set(Some(label)));
        self.changed.signal(());
    }

    /// What mDNS advertises: the Label, while the Rig is connected, at its
    /// address. `None` = nothing.
    pub fn advertisement(&self) -> Option<Advertisement> {
        let label = self.label.lock(Cell::get)?;
        let Network { link: Link::Connected, address } = self.network() else { return None };
        Some(Advertisement::new(label, address))
    }

    /// Waits until what mDNS advertises is no longer `advertised`, and
    /// returns what it is now. For the one mDNS responder.
    pub async fn changed_from(&self, advertised: Option<Advertisement>) -> Option<Advertisement> {
        loop {
            let advertisement = self.advertisement();
            if advertisement != advertised {
                return advertisement;
            }
            self.changed.wait().await;
        }
    }

    /// The radio reports what it is doing.
    pub fn set_radio(&self, radio: Radio) {
        self.update(|state| state.radio = radio);
    }

    /// The network stack reports its IPv4 address.
    pub fn set_address(&self, address: Option<[u8; 4]>) {
        self.update(|state| state.address = address);
    }

    /// The link `GetStatus` reports: connected once the Rig has joined and
    /// has an address, which it reports only then.
    pub fn network(&self) -> Network {
        let State { radio, address } = self.state.lock(Cell::get);
        match (radio, address) {
            (Radio::Off, _) => Network::default(),
            (Radio::Joined, Some(address)) => Network { link: Link::Connected, address },
            _ => Network { link: Link::Joining, address: [0; 4] },
        }
    }

    fn update(&self, change: impl FnOnce(&mut State)) {
        self.state.lock(|state| {
            let mut new = state.get();
            change(&mut new);
            state.set(new);
        });
        self.changed.signal(());
    }
}

impl Default for Wifi {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Wifi {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("Wifi").field(&self.network()).finish()
    }
}
