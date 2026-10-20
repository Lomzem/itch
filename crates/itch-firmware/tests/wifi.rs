//! The Wi-Fi link `GetStatus` reports, from what the radio and the network
//! stack tell the board.

mod common;

use common::Bench;
use itch_core::wire::Link;
use itch_firmware::wifi::Radio;

/// A made-up address from the documentation range.
const ADDRESS: [u8; 4] = [192, 0, 2, 10];

/// Boots a Rig that has credentials, so its radio is on.
fn with_credentials() -> Bench {
    let mut bench = Bench::boot();
    bench.set_wifi("ExampleRigNet", "example-passphrase").unwrap();
    bench
}

#[test]
fn the_link_is_off_while_the_radio_is_off_whatever_address_is_configured() {
    let mut bench = Bench::boot();
    assert_eq!(bench.network(), (Link::Off, [0; 4]));

    // A static address is configured whether or not the Rig is joined.
    bench.wifi.set_address(Some(ADDRESS));
    assert_eq!(bench.network(), (Link::Off, [0; 4]));
}

#[test]
fn the_link_is_joining_until_the_rig_has_joined_and_has_an_address() {
    let mut bench = with_credentials();
    bench.wifi.set_radio(Radio::Joining);
    assert_eq!(bench.network(), (Link::Joining, [0; 4]));

    bench.wifi.set_radio(Radio::Joined);
    assert_eq!(bench.network(), (Link::Joining, [0; 4]), "no DHCP lease yet");

    bench.wifi.set_address(Some(ADDRESS));
    assert_eq!(bench.network(), (Link::Connected, ADDRESS));
}

#[test]
fn a_dropped_link_reports_joining_with_no_address() {
    let mut bench = with_credentials();
    bench.wifi.set_radio(Radio::Joined);
    bench.wifi.set_address(Some(ADDRESS));

    bench.wifi.set_radio(Radio::Joining);
    assert_eq!(bench.network(), (Link::Joining, [0; 4]));
}
