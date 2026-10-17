mod common;

use common::{Bench, Flash};

/// The default label for the bench's Hardware ID, 02:00:00:AC:E0:01.
const DEFAULT: &str = "itch-ACE001";

#[test]
fn a_label_survives_a_power_cycle() {
    let mut bench = Bench::boot();
    assert_eq!(bench.set_label("bench-3"), Ok("bench-3".into()));

    let mut bench = bench.power_cycle();
    assert_eq!(bench.label(), "bench-3");
}

#[test]
fn the_last_of_thousands_of_labels_survives_a_power_cycle() {
    let mut bench = Bench::boot();
    // Enough 64-byte records to fill the partition several times over.
    let label = |n: usize| format!("{n:064}");
    for n in 0..3000 {
        assert_eq!(bench.set_label(&label(n)), Ok(label(n)));
    }

    let mut bench = bench.power_cycle();
    assert_eq!(bench.label(), label(2999));
}

#[test]
fn a_cleared_label_stays_the_default_after_a_power_cycle() {
    let mut bench = Bench::boot();
    bench.set_label("bench-3").unwrap();
    assert_eq!(bench.set_label(""), Ok(DEFAULT.into()));

    let mut bench = bench.power_cycle();
    assert_eq!(bench.label(), DEFAULT);
}

#[test]
fn a_new_module_reports_the_default_label_without_writing_it() {
    let mut bench = Bench::boot();
    assert_eq!(bench.label(), DEFAULT);
    assert_eq!(bench.flash.borrow().writes, []);
}

#[test]
fn a_scrambled_partition_boots_with_the_default_label_and_takes_a_new_one() {
    let flash = Flash::erased();
    flash.borrow_mut().scramble();
    let mut bench = Bench::power_on_with(flash);
    assert_eq!(bench.label(), DEFAULT);

    assert_eq!(bench.set_label("bench-3"), Ok("bench-3".into()));
    let mut bench = bench.power_cycle();
    assert_eq!(bench.label(), "bench-3");
}

#[test]
fn a_label_the_flash_fails_to_save_is_refused_and_the_old_one_kept() {
    let mut bench = Bench::boot();
    bench.set_label("bench-3").unwrap();
    bench.flash.borrow_mut().broken = true;
    assert_eq!(bench.set_label("bench-4"), Err("StorageFault".into()));
    assert_eq!(bench.label(), "bench-3");

    bench.flash.borrow_mut().broken = false;
    let mut bench = bench.power_cycle();
    assert_eq!(bench.label(), "bench-3");
}
