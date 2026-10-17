use itch_firmware::supervisor::{Check, Latch, Supervisor};

#[test]
fn a_rig_checking_in_on_every_tick_stays_fresh() {
    let latch = Latch::new();
    let mut supervisor = Supervisor::new(&latch, 0);
    for tick in 1..=200 {
        assert_eq!(supervisor.check(tick, u64::from(tick) * 50), Check::Fresh, "tick {tick}");
    }
    assert!(!latch.tripped());
}

#[test]
fn a_rig_that_stops_checking_in_misses_its_deadline_1_s_later() {
    let latch = Latch::new();
    let mut supervisor = Supervisor::new(&latch, 0);
    assert_eq!(supervisor.check(1, 500), Check::Fresh);
    assert_eq!(supervisor.check(1, 1499), Check::Fresh);
    assert!(!latch.tripped());
    assert_eq!(supervisor.check(1, 1500), Check::Missed);
    assert!(latch.tripped());
}

#[test]
fn a_rig_that_never_checks_in_misses_its_deadline_1_s_after_boot() {
    let latch = Latch::new();
    let mut supervisor = Supervisor::new(&latch, 200);
    assert_eq!(supervisor.check(0, 1199), Check::Fresh);
    assert_eq!(supervisor.check(0, 1200), Check::Missed);
}

#[test]
fn a_missed_deadline_stays_missed_when_the_rig_comes_back() {
    let latch = Latch::new();
    let mut supervisor = Supervisor::new(&latch, 0);
    assert_eq!(supervisor.check(0, 1000), Check::Missed);
    assert_eq!(supervisor.check(1, 1050), Check::Missed);
    assert_eq!(supervisor.check(2, 1100), Check::Missed);
    assert!(latch.tripped());
}

#[test]
fn a_rig_that_checks_in_late_has_still_missed_its_deadline() {
    // `rig` blocked the executor, so the supervisor only sees its next
    // check-in once both run again.
    let latch = Latch::new();
    let mut supervisor = Supervisor::new(&latch, 0);
    assert_eq!(supervisor.check(1, 50), Check::Fresh);
    assert_eq!(supervisor.check(2, 1300), Check::Missed);
    assert!(latch.tripped());
}
