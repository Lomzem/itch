//! What the Rig advertises over mDNS: its Label, while it is connected, at
//! the address the network stack reports.

mod common;

use std::pin::{Pin, pin};
use std::task::Poll;

use common::Bench;
use embassy_futures::poll_once;
use itch_firmware::mdns::Advertisement;
use itch_firmware::wifi::Radio;

/// The default label for the bench's Hardware ID, 02:00:00:AC:E0:01.
const DEFAULT: &str = "itch-ACE001";

/// Made-up addresses from the documentation range.
const ADDRESS: [u8; 4] = [192, 0, 2, 10];
const OTHER_ADDRESS: [u8; 4] = [192, 0, 2, 20];

/// The instance name and address the Rig advertises. `None` = nothing.
fn advertised(bench: &Bench) -> Option<(String, [u8; 4])> {
    let advertisement = bench.wifi.advertisement()?;
    Some((advertisement.instance().to_owned(), advertisement.address()))
}

fn connect(bench: &Bench, address: [u8; 4]) {
    bench.wifi.set_radio(Radio::Joined);
    bench.wifi.set_address(Some(address));
}

#[test]
fn a_connected_rig_advertises_its_label_at_its_address() {
    let bench = Bench::boot();
    connect(&bench, ADDRESS);
    assert_eq!(advertised(&bench), Some((DEFAULT.into(), ADDRESS)));
}

#[test]
fn nothing_is_advertised_until_the_rig_is_connected() {
    let bench = Bench::boot();
    assert_eq!(advertised(&bench), None);

    // A static address is configured whether or not the Rig is joined.
    bench.wifi.set_address(Some(ADDRESS));
    assert_eq!(advertised(&bench), None);

    bench.wifi.set_radio(Radio::Joining);
    assert_eq!(advertised(&bench), None);
}

#[test]
fn nothing_is_advertised_once_the_link_drops() {
    let bench = Bench::boot();
    connect(&bench, ADDRESS);
    bench.wifi.set_radio(Radio::Joining);
    assert_eq!(advertised(&bench), None);
}

#[test]
fn a_new_address_is_advertised() {
    let bench = Bench::boot();
    connect(&bench, ADDRESS);
    bench.wifi.set_address(Some(OTHER_ADDRESS));
    assert_eq!(advertised(&bench), Some((DEFAULT.into(), OTHER_ADDRESS)));
}

#[test]
fn the_saved_label_is_advertised_after_a_power_cycle() {
    let mut bench = Bench::boot();
    bench.set_label("bench-3").unwrap();

    let bench = bench.power_cycle();
    connect(&bench, ADDRESS);
    assert_eq!(advertised(&bench), Some(("bench-3".into(), ADDRESS)));
}

#[test]
fn a_label_the_flash_fails_to_save_is_not_advertised() {
    let mut bench = Bench::boot();
    connect(&bench, ADDRESS);
    bench.set_label("bench-3").unwrap();
    bench.flash.borrow_mut().broken = true;
    assert_eq!(bench.set_label("bench-4"), Err("StorageFault".into()));
    assert_eq!(advertised(&bench), Some(("bench-3".into(), ADDRESS)));
}

#[test]
fn renaming_the_rig_advertises_it_under_the_new_label() {
    let mut bench = Bench::boot();
    connect(&bench, ADDRESS);
    bench.set_label("bench-3").unwrap();
    assert_eq!(advertised(&bench), Some(("bench-3".into(), ADDRESS)));

    bench.set_label("").unwrap();
    assert_eq!(advertised(&bench), Some((DEFAULT.into(), ADDRESS)));
}

/// The instance name and address `changed` ends with, or `Pending` while
/// it waits.
fn woken(changed: Pin<&mut impl Future<Output = Option<Advertisement>>>) -> Poll<Option<String>> {
    poll_once(changed).map(|advertisement| advertisement.map(|it| it.instance().to_owned()))
}

#[test]
fn a_rename_wakes_the_responder_with_the_new_advertisement() {
    let mut bench = Bench::boot();
    connect(&bench, ADDRESS);
    let wifi = bench.wifi;
    let mut changed = pin!(wifi.changed_from(wifi.advertisement()));
    assert_eq!(woken(changed.as_mut()), Poll::Pending);

    bench.set_label("bench-3").unwrap();
    assert_eq!(woken(changed.as_mut()), Poll::Ready(Some("bench-3".into())));
}

#[test]
fn a_dropped_link_wakes_the_responder_with_nothing_to_advertise() {
    let bench = Bench::boot();
    connect(&bench, ADDRESS);
    let mut changed = pin!(bench.wifi.changed_from(bench.wifi.advertisement()));
    bench.wifi.set_radio(Radio::Joining);
    assert_eq!(woken(changed.as_mut()), Poll::Ready(None));
}

#[test]
fn a_connection_wakes_the_responder_with_the_advertisement() {
    let bench = Bench::boot();
    let mut changed = pin!(bench.wifi.changed_from(None));
    bench.wifi.set_radio(Radio::Joined);
    assert_eq!(woken(changed.as_mut()), Poll::Pending, "no address yet");

    bench.wifi.set_address(Some(ADDRESS));
    assert_eq!(woken(changed.as_mut()), Poll::Ready(Some(DEFAULT.into())));
}

#[test]
fn changes_that_leave_the_advertisement_as_it_was_let_the_responder_be() {
    let mut bench = Bench::boot();
    connect(&bench, ADDRESS);
    bench.set_label("bench-3").unwrap();
    let wifi = bench.wifi;
    let mut changed = pin!(wifi.changed_from(wifi.advertisement()));

    bench.set_label("bench-3").unwrap();
    connect(&bench, ADDRESS);
    assert_eq!(woken(changed.as_mut()), Poll::Pending);
}
