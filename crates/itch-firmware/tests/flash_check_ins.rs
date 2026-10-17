//! On its own, as the check-in count is global: no other test may check in
//! while this one counts.

mod common;

use std::sync::atomic::Ordering;

use common::{Bench, Flash};
use itch_firmware::supervisor::CHECK_INS;

#[test]
fn rig_checks_in_right_before_and_after_every_flash_erase_and_write() {
    // A scrambled partition gets erased whole; 3000 labels fill it over.
    let flash = Flash::erased();
    flash.borrow_mut().scramble();
    let mut bench = Bench::power_on_with(flash);
    for n in 0..3000 {
        let before = CHECK_INS.load(Ordering::Relaxed);
        bench.flash.borrow_mut().writes.clear();
        bench.set_label(&format!("{n:064}")).unwrap();
        let after = CHECK_INS.load(Ordering::Relaxed);

        let writes = bench.flash.borrow().writes.clone();
        assert!(!writes.is_empty());
        // Each erase or write sees one check-in since the last stall ended,
        // and is followed by another.
        let mut last = before;
        for at in writes {
            assert_eq!(at, last + 1, "label {n}");
            last = at + 1;
        }
        assert_eq!(after, last, "label {n}");
    }
}
