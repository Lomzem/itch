use std::sync::atomic::AtomicU32;

use itch_firmware::supervisor::{Check, Latch, Supervisor};

/// A supervisor of its own, started at `now`, and its latch.
fn supervisor(now: u64) -> (Supervisor<'static>, &'static Latch) {
    let latch = Box::leak(Box::new(Latch::new()));
    let supervisor = Supervisor::new(latch, Box::leak(Box::new(AtomicU32::new(0))));
    supervisor.start(now);
    (supervisor, latch)
}

#[test]
fn a_rig_checking_in_on_every_tick_stays_fresh() {
    let (supervisor, latch) = supervisor(0);
    for tick in 1..=200 {
        let now = tick * 50;
        assert_eq!(supervisor.check_in(now), Check::Fresh, "tick {tick}");
        assert_eq!(supervisor.check(now + 25), Check::Fresh, "tick {tick}");
    }
    assert!(!latch.tripped());
}

#[test]
fn a_rig_that_stops_checking_in_misses_its_deadline_1_s_later() {
    let (supervisor, latch) = supervisor(0);
    assert_eq!(supervisor.check_in(500), Check::Fresh);
    assert_eq!(supervisor.check(1499), Check::Fresh);
    assert!(!latch.tripped());
    assert_eq!(supervisor.check(1500), Check::Missed);
    assert!(latch.tripped());
}

#[test]
fn a_rig_that_never_checks_in_misses_its_deadline_1_s_after_the_start() {
    let (supervisor, _) = supervisor(200);
    assert_eq!(supervisor.check(1199), Check::Fresh);
    assert_eq!(supervisor.check(1200), Check::Missed);
}

#[test]
fn a_missed_deadline_stays_missed_when_the_rig_comes_back() {
    let (supervisor, latch) = supervisor(0);
    assert_eq!(supervisor.check(1000), Check::Missed);
    assert_eq!(supervisor.check_in(1050), Check::Missed);
    assert_eq!(supervisor.check(1100), Check::Missed);
    assert!(latch.tripped());
}

#[test]
fn a_rig_that_checks_in_late_has_missed_its_deadline_before_the_supervisor_looks() {
    // `rig` blocked the executor for over 1 s without checking in, so the
    // supervisor never ran in between.
    let (supervisor, latch) = supervisor(0);
    assert_eq!(supervisor.check_in(50), Check::Fresh);
    assert_eq!(supervisor.check_in(1050), Check::Missed);
    assert!(latch.tripped());
    assert_eq!(supervisor.check(1060), Check::Missed);
}

#[test]
fn the_deadline_holds_across_the_clock_wrapping_after_49_days() {
    let wrap = 1 << 32;
    let (supervisor, _) = supervisor(wrap - 100);
    assert_eq!(supervisor.check_in(wrap + 400), Check::Fresh);
    assert_eq!(supervisor.check(wrap + 1399), Check::Fresh);
    assert_eq!(supervisor.check(wrap + 1400), Check::Missed);
}
