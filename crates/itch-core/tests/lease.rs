mod common;

use common::TestRig;
use itch_core::wire::{AcquireLease, ErrorCode, RenewLease, Reply, Request, WriteChannels};

const TOKEN: u64 = 0x0123_4567_89AB_CDEF;

fn acquire(token: u64) -> Request<'static> {
    Request::AcquireLease(AcquireLease { token, takeover: false, holder: "qa@bench-pc test.py" })
}

fn write(token: u64, mask: u16, values: u16) -> Request<'static> {
    Request::WriteChannels(WriteChannels { token, mask, values })
}

#[test]
fn acquire_reports_a_10_s_ttl() {
    assert_eq!(TestRig::new().ok(acquire(TOKEN)), Reply::AcquireLease { ttl_ms: 10_000 });
}

#[test]
fn a_retried_acquire_with_the_same_token_succeeds() {
    let mut rig = TestRig::new();
    rig.ok(acquire(TOKEN));
    assert_eq!(rig.ok(acquire(TOKEN)), Reply::AcquireLease { ttl_ms: 10_000 });
}

#[test]
fn a_token_bearing_request_claims_a_free_rig() {
    let mut rig = TestRig::new();
    assert_eq!(rig.ok(write(TOKEN, 1, 1)), Reply::WriteChannels { state: 1 });
}

#[test]
fn renew_keeps_the_lease() {
    let mut rig = TestRig::new();
    rig.ok(acquire(TOKEN));
    let renew = Request::RenewLease(RenewLease { token: TOKEN, holder: "qa@bench-pc test.py" });
    assert_eq!(rig.ok(renew), Reply::RenewLease);
    assert_eq!(rig.ok(write(TOKEN, 1, 1)), Reply::WriteChannels { state: 1 });
}

#[test]
fn lease_requests_with_token_0_get_lease_required() {
    let mut rig = TestRig::new();
    assert_eq!(rig.refused(acquire(0)), ErrorCode::LeaseRequired);
    let renew = Request::RenewLease(RenewLease { token: 0, holder: "" });
    assert_eq!(rig.refused(renew), ErrorCode::LeaseRequired);
}

#[test]
fn release_is_idempotent_and_leaves_the_channels_unchanged() {
    let mut rig = TestRig::new();
    rig.ok(acquire(TOKEN));
    rig.ok(write(TOKEN, 0x8001, 0x8001));
    assert_eq!(rig.ok(Request::ReleaseLease { token: TOKEN }), Reply::ReleaseLease);
    assert_eq!(rig.ok(Request::ReleaseLease { token: TOKEN }), Reply::ReleaseLease);
    assert_eq!(rig.ok(Request::ReleaseLease { token: 0 }), Reply::ReleaseLease);
    assert_eq!(rig.ok(Request::GetChannels), Reply::GetChannels { state: 0x8001 });
    assert_eq!(rig.board.writes, [0x8001]);
}
