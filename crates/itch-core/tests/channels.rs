mod common;

use common::TestRig;
use itch_core::wire::{ErrorCode, Reply, Request, WriteChannels};

fn write(mask: u16, values: u16) -> Request<'static> {
    Request::WriteChannels(WriteChannels { mask, values })
}

#[test]
fn a_mask_write_changes_only_the_masked_channels() {
    let mut rig = TestRig::new();
    rig.ok(write(0b0111, 0b0101));
    let reply = rig.ok(write(0b0110, 0b0010));
    assert_eq!(reply, Reply::WriteChannels { state: 0b0011 });
    assert_eq!(rig.board.writes, [0b0101, 0b0011]);
}

#[test]
fn a_write_has_its_reply_wait_out_settling() {
    let mut rig = TestRig::new();
    rig.ok(write(0x8001, 0x8001));
    assert!(rig.settle);
}

#[test]
fn clear_turns_every_channel_off() {
    let mut rig = TestRig::new();
    rig.ok(write(0xFFFF, 0xA5A5));
    assert_eq!(rig.ok(Request::Clear), Reply::Clear { state: 0 });
    assert!(rig.settle);
    assert_eq!(rig.board.writes.last(), Some(&0));
}

#[test]
fn get_channels_reports_the_commanded_state_after_idle_time_and_hello() {
    let mut rig = TestRig::new();
    assert_eq!(rig.ok(Request::GetChannels), Reply::GetChannels { state: 0 });
    rig.ok(write(0x8001, 0x8001));
    rig.board.now = 60_000;
    rig.tick();
    rig.ok(Request::Hello);
    assert_eq!(rig.ok(Request::GetChannels), Reply::GetChannels { state: 0x8001 });
    assert!(!rig.settle);
}

#[test]
fn a_write_with_a_short_body_gets_malformed() {
    let mut rig = TestRig::new();
    let reply = rig.raw(&[0x03, 7, 0, 0, 0, 1, 2, 3]).unwrap();
    assert_eq!(reply[..6], [0x83, 7, 0, 0, 0, ErrorCode::Malformed as u8]);
}
