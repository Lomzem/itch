mod common;

use common::TestRig;
use itch_core::board::Counters;
use itch_core::frame::FrameError;
use itch_core::wire::{Lock, Reply, Request, StatusReply};

fn status(rig: &mut TestRig) -> StatusReply<'static> {
    match rig.ok(Request::GetStatus) {
        Reply::GetStatus(status) => status,
        other => panic!("not a GetStatus reply: {other:?}"),
    }
}

#[test]
fn status_reports_uptime_reset_reason_last_panic_and_the_counters() {
    let mut rig = TestRig::new();
    rig.board.now = 3_723_000;
    rig.board.info.reset_reason = 0x03;
    rig.board.info.last_panic = "src/main.rs:88:5";
    rig.board.counters = Counters { dropped_logs: 4, usb_tx_dropped: 6 };
    assert_eq!(rig.receive(Err(FrameError::Corrupt)), None);
    assert_eq!(rig.receive(Err(FrameError::Oversized)), None);
    assert_eq!(rig.receive(Err(FrameError::Oversized)), None);
    assert_eq!(
        status(&mut rig),
        StatusReply {
            uptime_ms: 3_723_000,
            reset_reason: 0x03,
            last_panic: "src/main.rs:88:5",
            crc_failures: 1,
            oversized_frames: 2,
            malformed_frames: 0,
            dropped_logs: 4,
            expander_faults: 0,
            usb_tx_dropped: 6,
            holder: "",
        }
    );
}

#[test]
fn status_reports_who_holds_the_lock() {
    let mut rig = TestRig::new();
    assert_eq!(status(&mut rig).holder, "");
    rig.ok(Request::Lock(Lock { holder: "alice@ws3", takeover: false }));
    assert_eq!(status(&mut rig).holder, "alice@ws3");
    rig.ok(Request::Lock(Lock { holder: "bob@ws7", takeover: true }));
    assert_eq!(status(&mut rig).holder, "bob@ws7");
    rig.ok(Request::ReleaseLock { holder: "bob@ws7" });
    assert_eq!(status(&mut rig).holder, "");
}

#[test]
fn a_payload_under_5_bytes_is_counted_in_malformed_frames() {
    let mut rig = TestRig::new();
    assert_eq!(rig.raw(&[0x05, 0x78, 0x56, 0x34]), None);
    assert_eq!(rig.raw(&[]), None);
    assert_eq!(status(&mut rig).malformed_frames, 2);
}
