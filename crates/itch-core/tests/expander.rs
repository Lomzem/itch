mod common;

use common::TestRig;
use itch_core::wire::{ErrorCode, Reply, Request, WriteChannels};

const TOKEN: u64 = 0x0123_4567_89AB_CDEF;

fn write(mask: u16, values: u16) -> Request<'static> {
    Request::WriteChannels(WriteChannels { token: TOKEN, mask, values })
}

fn expander_faults(rig: &mut TestRig) -> u32 {
    match rig.ok(Request::GetStatus) {
        Reply::GetStatus(status) => status.expander_faults,
        other => panic!("not a GetStatus reply: {other:?}"),
    }
}

#[test]
fn a_tick_restores_channels_the_expander_lost() {
    let mut rig = TestRig::new();
    rig.ok(write(0xFFFF, 0x8001));
    rig.board.channels = 0x0000;
    rig.tick();
    assert_eq!(rig.board.reinits, 1);
    assert_eq!(rig.board.channels, 0x8001);
    assert_eq!(expander_faults(&mut rig), 1);
}

#[test]
fn a_tick_recovers_from_an_i2c_error() {
    let mut rig = TestRig::new();
    rig.ok(write(0xFFFF, 0x0102));
    rig.board.glitch = true;
    rig.tick();
    assert_eq!(rig.board.channels, 0x0102);
    assert_eq!(expander_faults(&mut rig), 1);
}

#[test]
fn a_tick_leaves_a_healthy_expander_alone() {
    let mut rig = TestRig::new();
    rig.ok(write(0xFFFF, 0x0102));
    rig.tick();
    assert_eq!(rig.board.reinits, 0);
    assert_eq!(rig.board.writes, [0x0102]);
    assert_eq!(expander_faults(&mut rig), 0);
}

#[test]
fn an_expander_that_does_not_come_back_holds_every_channel_off() {
    let mut rig = TestRig::new();
    rig.ok(write(0xFFFF, 0x0102));
    rig.board.unreachable = true;
    rig.tick();
    assert_eq!(rig.refused(write(0x0001, 0x0001)), ErrorCode::HardwareFault);
    assert_eq!(rig.refused(Request::Clear { token: TOKEN }), ErrorCode::HardwareFault);
    assert_eq!(rig.ok(Request::GetChannels), Reply::GetChannels { state: 0 });
    assert!(!rig.settled);
}

#[test]
fn a_held_expander_is_retried_on_every_tick_and_counted_once() {
    let mut rig = TestRig::new();
    rig.board.unreachable = true;
    rig.tick();
    rig.tick();
    rig.tick();
    assert_eq!(rig.board.reinits, 3);
    assert_eq!(expander_faults(&mut rig), 1);
}

#[test]
fn a_later_tick_that_brings_the_expander_back_restores_the_commanded_channels() {
    let mut rig = TestRig::new();
    rig.ok(write(0xFFFF, 0x0102));
    rig.board.unreachable = true;
    rig.tick();
    rig.tick();
    rig.board.unreachable = false;
    rig.tick();
    assert_eq!(rig.board.channels, 0x0102);
    assert_eq!(rig.ok(Request::GetChannels), Reply::GetChannels { state: 0x0102 });
    assert_eq!(rig.ok(write(0x0001, 0x0001)), Reply::WriteChannels { state: 0x0103 });
    assert_eq!(expander_faults(&mut rig), 1);
}

#[test]
fn a_write_that_hits_an_i2c_error_recovers_and_succeeds() {
    let mut rig = TestRig::new();
    rig.ok(write(0xFFFF, 0x0102));
    rig.board.glitch = true;
    assert_eq!(rig.ok(write(0x0001, 0x0001)), Reply::WriteChannels { state: 0x0103 });
    assert!(rig.settled);
    assert_eq!(rig.board.channels, 0x0103);
    assert_eq!(expander_faults(&mut rig), 1);
}

#[test]
fn a_write_refused_for_a_hardware_fault_changes_nothing() {
    let mut rig = TestRig::new();
    rig.ok(write(0xFFFF, 0x0102));
    rig.board.unreachable = true;
    assert_eq!(rig.refused(write(0x0001, 0x0001)), ErrorCode::HardwareFault);
    rig.board.unreachable = false;
    rig.tick();
    assert_eq!(rig.ok(Request::GetChannels), Reply::GetChannels { state: 0x0102 });
    assert_eq!(rig.board.channels, 0x0102);
}
