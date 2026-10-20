mod common;

use common::{FakeBoard, TestRig};
use itch_core::board::Network;
use itch_core::wire::{AddressMode, ErrorCode, Link, Reply, Request, SetAddress, StatusReply};

/// Made-up addresses from the documentation range.
const ADDRESS: [u8; 4] = [192, 0, 2, 10];
const GATEWAY: [u8; 4] = [192, 0, 2, 1];

/// DHCP as status reports it: every field zero.
const DHCP: (AddressMode, [u8; 4], u8, [u8; 4]) = (AddressMode::Dhcp, [0; 4], 0, [0; 4]);

fn set_static(address: [u8; 4], prefix: u8, gateway: [u8; 4]) -> Request<'static> {
    Request::SetAddress(SetAddress {
        holder: "",
        mode: AddressMode::Static,
        address,
        prefix,
        gateway,
    })
}

fn set_dhcp() -> Request<'static> {
    let (mode, address, prefix, gateway) = DHCP;
    Request::SetAddress(SetAddress { holder: "", mode, address, prefix, gateway })
}

fn status(rig: &mut TestRig) -> StatusReply<'static> {
    match rig.ok(Request::GetStatus) {
        Reply::GetStatus(status) => status,
        other => panic!("not a GetStatus reply: {other:?}"),
    }
}

/// The address configuration `GetStatus` reports: mode, address, prefix
/// and gateway.
fn configured(rig: &mut TestRig) -> (AddressMode, [u8; 4], u8, [u8; 4]) {
    let status = status(rig);
    (status.mode, status.address, status.prefix, status.gateway)
}

#[test]
fn a_static_address_is_saved_and_reported_in_status() {
    let mut rig = TestRig::new();
    assert_eq!(configured(&mut rig), DHCP);

    assert_eq!(rig.ok(set_static(ADDRESS, 24, GATEWAY)), Reply::SetAddress);
    assert_eq!(configured(&mut rig), (AddressMode::Static, ADDRESS, 24, GATEWAY));
    assert_eq!(rig.board.saved_addressing, [(AddressMode::Static, ADDRESS, 24, GATEWAY)]);
}

#[test]
fn a_rig_switches_back_to_dhcp() {
    let mut rig = TestRig::new();
    rig.ok(set_static(ADDRESS, 24, GATEWAY));
    assert_eq!(rig.ok(set_dhcp()), Reply::SetAddress);
    assert_eq!(configured(&mut rig), DHCP);
    assert_eq!(rig.board.saved_addressing.last(), Some(&DHCP));
}

#[test]
fn a_static_address_needs_no_gateway_and_takes_any_prefix_from_1_to_32() {
    let mut rig = TestRig::new();
    for prefix in [1, 24, 32] {
        assert_eq!(rig.ok(set_static(ADDRESS, prefix, [0; 4])), Reply::SetAddress);
        assert_eq!(configured(&mut rig), (AddressMode::Static, ADDRESS, prefix, [0; 4]));
    }
}

#[test]
fn an_address_configuration_out_of_range_is_malformed_and_changes_nothing() {
    let mut rig = TestRig::new();
    rig.ok(set_static(ADDRESS, 24, GATEWAY));
    for (mode, address, prefix, gateway) in [
        (AddressMode::Dhcp, ADDRESS, 0, [0; 4]),
        (AddressMode::Dhcp, [0; 4], 24, [0; 4]),
        (AddressMode::Dhcp, [0; 4], 0, GATEWAY),
        (AddressMode::Static, [0; 4], 24, GATEWAY),
        (AddressMode::Static, ADDRESS, 0, GATEWAY),
        (AddressMode::Static, ADDRESS, 33, GATEWAY),
    ] {
        let set = SetAddress { holder: "", mode, address, prefix, gateway };
        assert_eq!(rig.refused(Request::SetAddress(set)), ErrorCode::Malformed, "{set:?}");
    }
    assert_eq!(configured(&mut rig), (AddressMode::Static, ADDRESS, 24, GATEWAY));
    assert_eq!(rig.board.saved_addressing.len(), 1);
}

#[test]
fn a_save_failure_is_a_storage_fault_and_keeps_the_old_configuration() {
    let mut rig = TestRig::new();
    rig.ok(set_static(ADDRESS, 24, GATEWAY));
    rig.board.storage_fault = true;
    assert_eq!(rig.refused(set_static([192, 0, 2, 20], 24, GATEWAY)), ErrorCode::StorageFault);
    assert_eq!(rig.refused(set_dhcp()), ErrorCode::StorageFault);
    assert_eq!(configured(&mut rig), (AddressMode::Static, ADDRESS, 24, GATEWAY));
}

#[test]
fn the_rig_reports_the_configuration_stored_at_boot() {
    let mut board = FakeBoard::new();
    board.addressing = (AddressMode::Static, ADDRESS, 24, GATEWAY);
    let mut rig = TestRig::on(board);
    assert_eq!(configured(&mut rig), (AddressMode::Static, ADDRESS, 24, GATEWAY));
}

/// Status tells "static but not connected" from "connected at this
/// address".
#[test]
fn status_reports_the_configured_address_beside_the_one_the_rig_has() {
    let mut rig = TestRig::new();
    rig.ok(set_static(ADDRESS, 24, GATEWAY));
    assert_eq!(status(&mut rig).current_address, [0; 4]);

    rig.board.network = Network { link: Link::Connected, address: ADDRESS };
    let status = status(&mut rig);
    assert_eq!((status.mode, status.address), (AddressMode::Static, ADDRESS));
    assert_eq!(status.current_address, ADDRESS);
}
