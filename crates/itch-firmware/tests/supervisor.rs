use std::cell::Cell;

use itch_firmware::supervisor::{Check, DEADLINE_MS, Failsafe, Latch, Supervisor};

/// A watchdog and RESET line that only count what was done to them.
#[derive(Default)]
struct Recorder {
    feeds: Cell<u32>,
    reset_forced: Cell<bool>,
}

impl Failsafe for Recorder {
    fn feed(&self) {
        self.feeds.set(self.feeds.get() + 1);
    }

    fn force_reset_low(&self) {
        self.reset_forced.set(true);
    }
}

/// A supervisor of its own, started at `now`, and its latch and failsafe.
fn supervisor(
    now: u64,
) -> (Supervisor<'static, &'static Recorder>, &'static Latch, &'static Recorder) {
    let latch = Box::leak(Box::new(Latch::new()));
    let recorder = Box::leak(Box::new(Recorder::default()));
    let supervisor = Supervisor::new(latch, &*recorder);
    supervisor.start(now);
    (supervisor, latch, recorder)
}

#[test]
fn a_rig_checking_in_on_every_tick_stays_fresh_and_keeps_the_watchdog_fed() {
    let (supervisor, latch, recorder) = supervisor(0);
    for tick in 1..=200 {
        let now = tick * 50;
        assert_eq!(supervisor.check_in(now), Check::Fresh, "tick {tick}");
        assert_eq!(supervisor.check(now + 25), Check::Fresh, "tick {tick}");
    }
    assert!(!latch.tripped());
    assert_eq!(recorder.feeds.get(), 400);
    assert!(!recorder.reset_forced.get());
}

#[test]
fn a_rig_that_stops_checking_in_misses_its_deadline_after_the_last_check_in() {
    let (supervisor, latch, _) = supervisor(0);
    assert_eq!(supervisor.check_in(500), Check::Fresh);
    assert_eq!(supervisor.check(500 + DEADLINE_MS - 1), Check::Fresh);
    assert!(!latch.tripped());
    assert_eq!(supervisor.check(500 + DEADLINE_MS), Check::Missed);
    assert!(latch.tripped());
}

#[test]
fn a_rig_that_never_checks_in_misses_its_deadline_after_the_start() {
    let (supervisor, _, _) = supervisor(200);
    assert_eq!(supervisor.check(200 + DEADLINE_MS - 1), Check::Fresh);
    assert_eq!(supervisor.check(200 + DEADLINE_MS), Check::Missed);
}

#[test]
fn a_missed_deadline_holds_reset_and_starves_the_watchdog_when_the_rig_comes_back() {
    let (supervisor, latch, recorder) = supervisor(0);
    assert_eq!(supervisor.check(DEADLINE_MS), Check::Missed);
    assert!(recorder.reset_forced.get());
    recorder.reset_forced.set(false);
    assert_eq!(supervisor.check_in(DEADLINE_MS + 50), Check::Missed);
    assert_eq!(supervisor.check(DEADLINE_MS + 100), Check::Missed);
    assert!(latch.tripped());
    assert!(recorder.reset_forced.get());
    assert_eq!(recorder.feeds.get(), 0);
}

#[test]
fn a_rig_that_checks_in_late_has_missed_its_deadline_before_the_supervisor_looks() {
    // `rig` blocked the executor for a full deadline without checking in, so the
    // supervisor never ran in between.
    let (supervisor, latch, _) = supervisor(0);
    assert_eq!(supervisor.check_in(50), Check::Fresh);
    assert_eq!(supervisor.check_in(DEADLINE_MS + 50), Check::Missed);
    assert!(latch.tripped());
    assert_eq!(supervisor.check(DEADLINE_MS + 60), Check::Missed);
}

#[test]
fn the_deadline_holds_across_the_clock_wrapping_after_49_days() {
    let wrap = 1 << 32;
    let (supervisor, _, _) = supervisor(wrap - 100);
    assert_eq!(supervisor.check_in(wrap + 400), Check::Fresh);
    assert_eq!(supervisor.check(wrap + 400 + DEADLINE_MS - 1), Check::Fresh);
    assert_eq!(supervisor.check(wrap + 400 + DEADLINE_MS), Check::Missed);
}
