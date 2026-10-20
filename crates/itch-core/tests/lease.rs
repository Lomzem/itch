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

const OTHER: u64 = 0xFEDC_BA98_7654_3210;

fn acquire_as(token: u64, holder: &'static str, takeover: bool) -> Request<'static> {
    Request::AcquireLease(AcquireLease { token, takeover, holder })
}

/// A Rig whose Lease went to `TOKEN`, held by `qa@bench-pc test.py`, at time 0.
fn held() -> TestRig {
    let mut rig = TestRig::new();
    rig.ok(acquire(TOKEN));
    rig
}

#[test]
fn acquire_against_a_live_foreign_lease_gets_lease_held_naming_the_holder() {
    let mut rig = held();
    let refusal = rig.refusal(acquire_as(OTHER, "dev@laptop other.py", false));
    assert_eq!(refusal, (ErrorCode::LeaseHeld, "qa@bench-pc test.py"));
    assert_eq!(rig.ok(write(TOKEN, 1, 1)), Reply::WriteChannels { state: 1 });
}

#[test]
fn another_clients_change_to_a_live_lease_gets_lease_lost_naming_the_holder() {
    let mut rig = held();
    rig.board.now = 5_000;
    assert_eq!(rig.refusal(write(OTHER, 1, 1)), (ErrorCode::LeaseLost, "qa@bench-pc test.py"));
    assert_eq!(rig.refused(Request::Clear { token: OTHER }), ErrorCode::LeaseLost);
    let renew = Request::RenewLease(RenewLease { token: OTHER, holder: "other" });
    assert_eq!(rig.refused(renew), ErrorCode::LeaseLost);
    assert!(rig.board.writes.is_empty());
}

#[test]
fn takeover_replaces_a_live_holder_at_once_and_leaves_the_channels() {
    let mut rig = held();
    rig.ok(write(TOKEN, 0x8001, 0x8001));
    let takeover = acquire_as(OTHER, "dev@laptop other.py", true);
    assert_eq!(rig.ok(takeover), Reply::AcquireLease { ttl_ms: 10_000 });
    assert_eq!(rig.refusal(write(TOKEN, 1, 0)), (ErrorCode::LeaseLost, "dev@laptop other.py"));
    assert_eq!(rig.ok(Request::GetChannels), Reply::GetChannels { state: 0x8001 });
    assert_eq!(rig.board.writes, [0x8001]);
}

#[test]
fn the_lease_is_alive_at_ttl_minus_1() {
    let mut rig = held();
    rig.board.now = 9_999;
    assert_eq!(rig.refused(write(OTHER, 1, 1)), ErrorCode::LeaseLost);
}

#[test]
fn the_lease_expires_at_ttl() {
    let mut rig = held();
    rig.board.now = 10_000;
    assert_eq!(rig.ok(write(OTHER, 1, 1)), Reply::WriteChannels { state: 1 });
}

#[test]
fn the_lease_expires_after_ttl() {
    let mut rig = held();
    rig.board.now = 10_001;
    assert_eq!(rig.ok(write(OTHER, 1, 1)), Reply::WriteChannels { state: 1 });
}

#[test]
fn a_write_by_the_holder_renews_the_lease() {
    let mut rig = held();
    rig.board.now = 9_000;
    rig.ok(write(TOKEN, 1, 1));
    rig.board.now = 18_999;
    assert_eq!(rig.refused(write(OTHER, 1, 1)), ErrorCode::LeaseLost);
}

#[test]
fn renew_lease_keeps_the_lease_alive_past_its_first_ttl() {
    let mut rig = held();
    rig.board.now = 9_000;
    rig.ok(Request::RenewLease(RenewLease { token: TOKEN, holder: "qa@bench-pc test.py" }));
    rig.board.now = 18_999;
    assert_eq!(rig.refused(acquire_as(OTHER, "other", false)), ErrorCode::LeaseHeld);
}

#[test]
fn refusals_do_not_renew_the_lease() {
    let mut rig = held();
    rig.board.now = 9_000;
    rig.refused(write(OTHER, 1, 1));
    rig.refused(acquire_as(OTHER, "other", false));
    rig.board.now = 10_000;
    rig.ok(write(OTHER, 1, 1));
}

#[test]
fn acquire_claims_an_expired_lease_without_takeover() {
    let mut rig = held();
    rig.board.now = 10_000;
    assert_eq!(rig.ok(acquire_as(OTHER, "other", false)), Reply::AcquireLease { ttl_ms: 10_000 });
    assert_eq!(rig.refusal(write(TOKEN, 1, 1)), (ErrorCode::LeaseLost, "other"));
}

#[test]
fn expiry_leaves_the_channels_unchanged() {
    let mut rig = held();
    rig.ok(write(TOKEN, 0x00F0, 0x0050));
    rig.board.now = 20_000;
    assert_eq!(rig.ok(Request::GetChannels), Reply::GetChannels { state: 0x0050 });
    rig.ok(acquire_as(OTHER, "other", false));
    assert_eq!(rig.ok(Request::GetChannels), Reply::GetChannels { state: 0x0050 });
    assert_eq!(rig.board.writes, [0x0050]);
}

#[test]
fn a_lapsed_lease_is_reclaimed_by_its_old_token() {
    let mut rig = held();
    rig.board.now = 60_000;
    assert_eq!(rig.ok(write(TOKEN, 1, 1)), Reply::WriteChannels { state: 1 });
    rig.board.now = 69_999;
    assert_eq!(rig.refused(acquire_as(OTHER, "other", false)), ErrorCode::LeaseHeld);
}

#[test]
fn a_write_that_claims_the_rig_names_an_unknown_holder_until_renewed() {
    let mut rig = TestRig::new();
    rig.ok(write(TOKEN, 1, 1));
    assert_eq!(rig.refusal(acquire_as(OTHER, "other", false)), (ErrorCode::LeaseHeld, "unknown"));
    rig.ok(Request::RenewLease(RenewLease { token: TOKEN, holder: "qa@bench-pc test.py" }));
    let refusal = rig.refusal(acquire_as(OTHER, "other", false));
    assert_eq!(refusal, (ErrorCode::LeaseHeld, "qa@bench-pc test.py"));
}

#[test]
fn release_by_another_token_leaves_the_lease_held() {
    let mut rig = held();
    assert_eq!(rig.ok(Request::ReleaseLease { token: OTHER }), Reply::ReleaseLease);
    assert_eq!(rig.refused(acquire_as(OTHER, "other", false)), ErrorCode::LeaseHeld);
}

#[test]
fn a_holder_over_64_bytes_is_malformed() {
    let mut rig = TestRig::new();
    let holder = "h".repeat(65).leak();
    assert_eq!(rig.refused(acquire_as(TOKEN, holder, false)), ErrorCode::Malformed);
    let renew = Request::RenewLease(RenewLease { token: TOKEN, holder });
    assert_eq!(rig.refused(renew), ErrorCode::Malformed);
    rig.ok(acquire_as(TOKEN, "h".repeat(64).leak(), false));
}
