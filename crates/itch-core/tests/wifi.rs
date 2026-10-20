mod common;

use common::{FakeBoard, TestRig};
use itch_core::board::Network;
use itch_core::frame::MAX_PAYLOAD;
use itch_core::wire::{ErrorCode, Link, Message, Reply, Request, SetWifi, StatusReply};

const SSID: &str = "ExampleRigNet";
const PASSPHRASE: &str = "example-passphrase";

fn set_wifi<'a>(ssid: &'a str, passphrase: &'a str) -> Request<'a> {
    Request::SetWifi(SetWifi { holder: "", ssid, passphrase })
}

fn status(rig: &mut TestRig) -> StatusReply<'static> {
    match rig.ok(Request::GetStatus) {
        Reply::GetStatus(status) => status,
        other => panic!("not a GetStatus reply: {other:?}"),
    }
}

/// What `GetStatus` says of the credentials: whether there are any, and
/// for which network.
fn reported(rig: &mut TestRig) -> (bool, &'static str) {
    let status = status(rig);
    (status.wifi_configured, status.ssid)
}

fn saved(ssid: &str, passphrase: &str) -> Option<(String, String)> {
    Some((ssid.into(), passphrase.into()))
}

#[test]
fn set_wifi_saves_the_credentials_and_status_reports_the_network() {
    let mut rig = TestRig::new();
    assert_eq!(reported(&mut rig), (false, ""));
    assert_eq!(rig.ok(set_wifi(SSID, PASSPHRASE)), Reply::SetWifi);
    assert_eq!(reported(&mut rig), (true, SSID));
    assert_eq!(rig.board.saved_wifi, [saved(SSID, PASSPHRASE)]);
}

#[test]
fn an_empty_ssid_and_passphrase_clear_the_credentials() {
    let mut rig = TestRig::new();
    rig.ok(set_wifi(SSID, PASSPHRASE));
    assert_eq!(rig.ok(set_wifi("", "")), Reply::SetWifi);
    assert_eq!(reported(&mut rig), (false, ""));
    assert_eq!(rig.board.saved_wifi, [saved(SSID, PASSPHRASE), None]);
}

#[test]
fn ssids_of_1_to_32_bytes_and_passphrases_of_8_to_63_bytes_are_accepted() {
    let mut rig = TestRig::new();
    for (ssid, passphrase) in [("a", "8 bytes!"), (&*"é".repeat(16), &*"p".repeat(63))] {
        assert_eq!(rig.ok(set_wifi(ssid, passphrase)), Reply::SetWifi);
        assert_eq!(rig.board.saved_wifi.last(), Some(&saved(ssid, passphrase)));
    }
}

#[test]
fn credentials_out_of_range_are_malformed_and_change_nothing() {
    let mut rig = TestRig::new();
    rig.ok(set_wifi(SSID, PASSPHRASE));
    let long_ssid = "s".repeat(33);
    let long_passphrase = "p".repeat(64);
    for (ssid, passphrase) in [
        (&*long_ssid, PASSPHRASE),
        (SSID, "1234567"),
        (SSID, &*long_passphrase),
        (SSID, ""),
        ("", PASSPHRASE),
    ] {
        assert_eq!(rig.refused(set_wifi(ssid, passphrase)), ErrorCode::Malformed, "{ssid}");
    }
    assert_eq!(reported(&mut rig), (true, SSID));
    assert_eq!(rig.board.saved_wifi.len(), 1);
}

#[test]
fn a_save_failure_is_a_storage_fault_and_keeps_the_old_credentials() {
    let mut rig = TestRig::new();
    rig.ok(set_wifi(SSID, PASSPHRASE));
    rig.board.storage_fault = true;
    assert_eq!(rig.refused(set_wifi("OtherExampleNet", PASSPHRASE)), ErrorCode::StorageFault);
    assert_eq!(rig.refused(set_wifi("", "")), ErrorCode::StorageFault);
    assert_eq!(reported(&mut rig), (true, SSID));
}

#[test]
fn the_rig_reports_the_credentials_stored_at_boot() {
    let mut board = FakeBoard::new();
    board.wifi = saved(SSID, PASSPHRASE);
    assert_eq!(reported(&mut TestRig::on(board)), (true, SSID));
}

#[test]
fn status_reports_the_link_and_address_the_board_has() {
    let mut rig = TestRig::new();
    let status_now = status(&mut rig);
    assert_eq!((status_now.link, status_now.current_address), (Link::Off, [0; 4]));

    rig.board.network = Network { link: Link::Connected, address: [192, 0, 2, 10] };
    let status_now = status(&mut rig);
    assert_eq!((status_now.link, status_now.current_address), (Link::Connected, [192, 0, 2, 10]));
}

#[test]
fn status_never_reports_the_passphrase() {
    let mut rig = TestRig::new();
    rig.ok(set_wifi(SSID, PASSPHRASE));
    let mut payload = [0; MAX_PAYLOAD];
    let len = Message::Request { request_id: 7, request: Request::GetStatus }.encode(&mut payload);
    let reply = rig.raw(&payload[..len]).expect("a reply");
    assert!(!reply.windows(PASSPHRASE.len()).any(|bytes| bytes == PASSPHRASE.as_bytes()));
    assert!(reply.windows(SSID.len()).any(|bytes| bytes == SSID.as_bytes()));
}
