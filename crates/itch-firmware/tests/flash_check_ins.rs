//! On its own, as the supervisor is global: no other test may check in
//! while this one runs.

mod common;

use common::{Bench, Flash, advance, now, since_check_in};
use itch_firmware::supervisor::{Check, DEADLINE_MS, SUPERVISOR, TRIPPED};

#[test]
fn rig_stays_in_time_through_a_save_that_blocks_for_seconds_until_it_stops_checking_in() {
    // Every erase and write stalls for 300 ms. A scrambled partition gets
    // erased whole, 16 sectors, then 3000 labels fill it over.
    let flash = Flash::erased();
    flash.borrow_mut().scramble();
    flash.borrow_mut().stall_ms = 300;
    let mut bench = Bench::power_on_with(flash);
    SUPERVISOR.start(now());
    for n in 0..3000 {
        let started = now();
        bench.flash.borrow_mut().writes.clear();
        bench.set_label(&format!("{n:064}")).unwrap();
        let took = now() - started;
        assert!(n > 0 || took > 4 * DEADLINE_MS, "the first save took {took} ms");

        let writes = bench.flash.borrow().writes.clone();
        assert!(!writes.is_empty());
        // Each erase or write comes straight after a check-in, and is
        // followed by another.
        assert!(writes.iter().all(|&since| since == 0), "label {n}: {writes:?}");
        assert_eq!(since_check_in(), 0, "label {n}");
        assert_eq!(SUPERVISOR.check(now()), Check::Fresh, "label {n}");
    }
    assert!(!TRIPPED.tripped());

    advance(DEADLINE_MS);
    assert_eq!(SUPERVISOR.check(now()), Check::Missed);
    assert!(TRIPPED.tripped());
}
