//! Where the Wi-Fi Transport meets the board: the credentials the board
//! hands the radio, the address configuration it hands the network stack,
//! and what the radio and the network stack report for `GetStatus`.

use core::cell::Cell;
use core::fmt;

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use itch_core::board::Network;
use itch_core::wifi::{Addressing, Credentials};
use itch_core::wire::Link;

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

/// The Rig's Wi-Fi: the credentials the radio joins with, how the network
/// stack takes its address, and the link as the radio and the network
/// stack report it.
pub struct Wifi {
    /// The newest credentials the board handed over, until the radio takes
    /// them. `None` = none.
    credentials: Signal<CriticalSectionRawMutex, Option<Credentials>>,
    /// The newest address configuration the board handed over, until the
    /// network stack takes it.
    addressing: Signal<CriticalSectionRawMutex, Addressing>,
    state: Mutex<CriticalSectionRawMutex, Cell<State>>,
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
            addressing: Signal::new(),
            state: Mutex::new(Cell::new(State { radio: Radio::Off, address: None })),
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

    /// Has the network stack take its address as `addressing` says from
    /// now on, whether or not the Rig is joined.
    pub fn readdress(&self, addressing: Addressing) {
        self.addressing.signal(addressing);
    }

    /// Waits for the network stack's next address configuration, the
    /// newest the board handed over.
    pub async fn addressing(&self) -> Addressing {
        self.addressing.wait().await
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
