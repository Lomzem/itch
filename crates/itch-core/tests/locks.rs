mod common;

use common::TestRig;
use itch_core::wire::{
    ErrorCode, Lock, LogLevel, Reply, Request, SetLabel, SetWifi, WriteChannels,
};

const ALICE: &str = "alice@ws3";
const BOB: &str = "bob@ws7";

fn write(holder: &str, mask: u16, values: u16) -> Request<'_> {
    Request::WriteChannels(WriteChannels { holder, mask, values })
}

fn clear(holder: &str) -> Request<'_> {
    Request::Clear { holder }
}

fn set_label<'a>(holder: &'a str, label: &'a str) -> Request<'a> {
    Request::SetLabel(SetLabel { holder, label })
}

fn set_wifi(holder: &str) -> Request<'_> {
    Request::SetWifi(SetWifi { holder, ssid: "ExampleRigNet", passphrase: "example-passphrase" })
}

fn lock(holder: &str) -> Request<'_> {
    Request::Lock(Lock { holder, takeover: false })
}

fn take_over(holder: &str) -> Request<'_> {
    Request::Lock(Lock { holder, takeover: true })
}

fn release(holder: &str) -> Request<'_> {
    Request::ReleaseLock { holder }
}

/// A Rig Locked by Alice, with CH1 on.
fn locked_by_alice() -> TestRig {
    let mut rig = TestRig::new();
    rig.ok(write("", 0x0001, 0x0001));
    assert_eq!(rig.ok(lock(ALICE)), Reply::Lock);
    rig
}

fn channels(rig: &mut TestRig) -> u16 {
    match rig.ok(Request::GetChannels) {
        Reply::GetChannels { state } => state,
        other => panic!("not a GetChannels reply: {other:?}"),
    }
}

#[test]
fn an_unlocked_rig_accepts_changes_from_any_holder() {
    let mut rig = TestRig::new();
    assert_eq!(rig.ok(write(ALICE, 0x0001, 0x0001)), Reply::WriteChannels { state: 0x0001 });
    assert_eq!(rig.ok(write("", 0x0002, 0x0002)), Reply::WriteChannels { state: 0x0003 });
    assert_eq!(rig.ok(clear(BOB)), Reply::Clear { state: 0 });
    assert_eq!(rig.ok(set_label("", "bench3")), Reply::SetLabel { label: "bench3" });
    assert_eq!(rig.ok(set_wifi(ALICE)), Reply::SetWifi);
}

#[test]
fn a_locked_rig_refuses_changes_from_anyone_else_naming_its_holder() {
    let mut rig = locked_by_alice();
    for holder in [BOB, ""] {
        assert_eq!(rig.refusal(write(holder, 0x0002, 0x0002)), (ErrorCode::Locked, ALICE));
        assert_eq!(rig.refusal(clear(holder)), (ErrorCode::Locked, ALICE));
        assert_eq!(rig.refusal(set_label(holder, "bench3")), (ErrorCode::Locked, ALICE));
        assert_eq!(rig.refusal(set_wifi(holder)), (ErrorCode::Locked, ALICE));
    }
    assert_eq!(channels(&mut rig), 0x0001);
    assert!(rig.board.saved.is_empty());
    assert!(rig.board.saved_wifi.is_empty());
}

#[test]
fn the_holder_changes_a_locked_rig_freely() {
    let mut rig = locked_by_alice();
    assert_eq!(rig.ok(write(ALICE, 0x0002, 0x0002)), Reply::WriteChannels { state: 0x0003 });
    assert_eq!(rig.ok(set_label(ALICE, "bench3")), Reply::SetLabel { label: "bench3" });
    assert_eq!(rig.ok(set_wifi(ALICE)), Reply::SetWifi);
    assert_eq!(rig.ok(clear(ALICE)), Reply::Clear { state: 0 });
}

#[test]
fn the_same_holder_picks_the_lock_back_up_with_no_effect() {
    let mut rig = locked_by_alice();
    assert_eq!(rig.ok(lock(ALICE)), Reply::Lock);
    assert_eq!(rig.refusal(write(BOB, 0x0002, 0x0002)), (ErrorCode::Locked, ALICE));
}

#[test]
fn locking_a_rig_someone_else_holds_is_refused_naming_its_holder() {
    let mut rig = locked_by_alice();
    assert_eq!(rig.refusal(lock(BOB)), (ErrorCode::Locked, ALICE));
    assert_eq!(rig.ok(write(ALICE, 0x0002, 0x0002)), Reply::WriteChannels { state: 0x0003 });
}

#[test]
fn after_a_takeover_the_old_holder_is_refused_naming_the_new_one() {
    let mut rig = locked_by_alice();
    assert_eq!(rig.ok(take_over(BOB)), Reply::Lock);
    assert_eq!(rig.refusal(write(ALICE, 0x0002, 0x0002)), (ErrorCode::Locked, BOB));
    assert_eq!(rig.refusal(lock(ALICE)), (ErrorCode::Locked, BOB));
    assert_eq!(rig.ok(write(BOB, 0x0002, 0x0002)), Reply::WriteChannels { state: 0x0003 });
}

#[test]
fn anyone_may_look_at_a_locked_rig_and_set_their_log_level() {
    let mut rig = locked_by_alice();
    assert!(matches!(rig.ok(Request::Hello), Reply::Hello(_)));
    assert_eq!(channels(&mut rig), 0x0001);
    assert!(matches!(rig.ok(Request::GetStatus), Reply::GetStatus(_)));
    assert_eq!(rig.ok(Request::SetLogLevel { level: LogLevel::Debug }), Reply::SetLogLevel);
    assert_eq!(rig.logs.log_level.get(), LogLevel::Debug);
}

#[test]
fn the_holder_releases_the_lock_and_anyone_may_change_the_rig_again() {
    let mut rig = locked_by_alice();
    assert_eq!(rig.ok(release(ALICE)), Reply::ReleaseLock);
    assert_eq!(rig.ok(write(BOB, 0x0002, 0x0002)), Reply::WriteChannels { state: 0x0003 });
    assert_eq!(rig.ok(lock(BOB)), Reply::Lock);
}

#[test]
fn only_the_holder_may_release_the_lock() {
    let mut rig = locked_by_alice();
    assert_eq!(rig.refusal(release(BOB)), (ErrorCode::Locked, ALICE));
    assert_eq!(rig.refusal(release("")), (ErrorCode::Locked, ALICE));
    assert_eq!(rig.refusal(write(BOB, 0x0002, 0x0002)), (ErrorCode::Locked, ALICE));
}

#[test]
fn releasing_an_unlocked_rig_changes_nothing() {
    let mut rig = TestRig::new();
    assert_eq!(rig.ok(release(ALICE)), Reply::ReleaseLock);
    assert_eq!(rig.ok(write(BOB, 0x0001, 0x0001)), Reply::WriteChannels { state: 0x0001 });
}

#[test]
fn lock_changes_never_move_the_channels() {
    let mut rig = locked_by_alice();
    let writes = rig.board.writes.clone();
    rig.ok(lock(ALICE));
    rig.ok(take_over(BOB));
    rig.ok(release(BOB));
    rig.ok(release(BOB));
    assert_eq!(rig.board.writes, writes);
    assert_eq!(channels(&mut rig), 0x0001);
}

#[test]
fn a_rig_reset_drops_the_lock() {
    let rig = locked_by_alice();
    let mut rebooted = TestRig::on(rig.board);
    assert_eq!(rebooted.ok(write(BOB, 0x0001, 0x0001)), Reply::WriteChannels { state: 0x0001 });
}
