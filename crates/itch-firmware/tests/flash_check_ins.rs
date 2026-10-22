mod common;

use common::{Bench, Flash, advance, now, since_fed};
use itch_firmware::supervisor::{Check, DEADLINE_MS};

#[test]
fn rig_stays_in_time_through_a_save_that_blocks_for_seconds_until_it_stops_checking_in() {
    // Every erase and write stalls for 300 ms. A scrambled partition gets
    // erased whole, 16 sectors, then 3000 saves of labels and Wi-Fi
    // credentials, in turn, fill it over.
    let flash = Flash::erased();
    flash.borrow_mut().scramble();
    flash.borrow_mut().stall_ms = 300;
    let mut bench = Bench::boot_with(flash);
    for n in 0..3000 {
        let started = now();
        bench.flash.borrow_mut().writes.clear();
        if n % 2 == 0 {
            bench.set_label(&format!("{n:064}")).unwrap();
        } else {
            bench.set_wifi(&format!("{n:032}"), &format!("{n:063}")).unwrap();
        }
        let took = now() - started;
        assert!(n > 0 || took > DEADLINE_MS, "the first save took {took} ms");

        let writes = bench.flash.borrow().writes.clone();
        assert!(!writes.is_empty());
        // Each erase or write comes straight after a check-in that fed the
        // watchdog, and is followed by another.
        assert!(writes.iter().all(|&since| since == Some(0)), "save {n}: {writes:?}");
        assert_eq!(since_fed(), Some(0), "save {n}");
        assert_eq!(bench.supervise(), Check::Fresh, "save {n}");
    }
    assert!(!bench.expander.borrow().in_reset());

    advance(DEADLINE_MS);
    assert_eq!(bench.supervise(), Check::Missed);
    assert!(bench.expander.borrow().in_reset());
}
