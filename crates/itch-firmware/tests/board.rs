mod common;

use common::{Bench, IODIRA, OLATA, Op};
use itch_core::board::{Board, HwFault};
use itch_core::wire::FEATURE_WIFI;

#[test]
fn each_channel_drives_its_contact_on_the_reversed_rev_a_wiring() {
    // (GPA, GPB) driven high for CH1..CH16, from the Rev A schematic:
    // CH1-8 = GPB7..GPB0, CH9-16 = GPA7..GPA0.
    let contacts = [
        (0x00, 0x80),
        (0x00, 0x40),
        (0x00, 0x20),
        (0x00, 0x10),
        (0x00, 0x08),
        (0x00, 0x04),
        (0x00, 0x02),
        (0x00, 0x01),
        (0x80, 0x00),
        (0x40, 0x00),
        (0x20, 0x00),
        (0x10, 0x00),
        (0x08, 0x00),
        (0x04, 0x00),
        (0x02, 0x00),
        (0x01, 0x00),
    ];
    let mut bench = Bench::boot();
    for (index, contact) in contacts.into_iter().enumerate() {
        let channel = 1 << index;
        bench.write(0xFFFF, channel);
        assert_eq!(bench.driven(), contact, "CH{}", index + 1);
    }
}

#[test]
fn boot_clears_the_output_latches_before_the_pins_become_outputs() {
    let bench = Bench::boot();
    let log = &bench.expander.borrow().log;
    let at = |op: Op| {
        log.iter().position(|o| *o == op).unwrap_or_else(|| panic!("no {op:?} in {log:?}"))
    };
    let released = at(Op::Reset(false));
    let latches = at(Op::Write(vec![OLATA, 0x00, 0x00]));
    let outputs = at(Op::Write(vec![IODIRA, 0x00, 0x00]));
    assert!(released < latches && latches < outputs, "{log:?}");
    assert_eq!(bench.driven(), (0x00, 0x00));
}

#[test]
fn boot_holds_an_expander_that_does_not_take_its_configuration_in_reset() {
    let mut bench = Bench::power_on();
    bench.expander.borrow_mut().stuck = true;
    assert_eq!(bench.board.reinit(), Err(HwFault));
    assert!(bench.expander.borrow().in_reset());
}

#[test]
fn boot_holds_an_expander_that_does_not_answer_in_reset() {
    let mut bench = Bench::power_on();
    bench.expander.borrow_mut().absent = true;
    assert_eq!(bench.board.reinit(), Err(HwFault));
    assert!(bench.expander.borrow().in_reset());
}

#[test]
fn the_status_led_is_on_while_any_channel_is_on() {
    let mut bench = Bench::boot();
    assert!(!bench.led());
    bench.write(0xFFFF, 0x0104);
    assert!(bench.led());
    bench.write(0x0004, 0x0000);
    assert!(bench.led());
    bench.write(0x0100, 0x0000);
    assert!(!bench.led());
}

#[test]
fn the_status_led_blinks_on_every_tick_while_the_expander_is_held_in_reset() {
    let mut bench = Bench::power_on();
    bench.expander.borrow_mut().absent = true;
    let _ = bench.board.reinit();
    assert_eq!(bench.led_over(4), [true, false, true, false]);
}

#[test]
fn the_status_led_blinks_after_a_channel_write_fails() {
    let mut bench = Bench::boot();
    bench.expander.borrow_mut().absent = true;
    bench.write(0xFFFF, 0x0001);
    assert_eq!(bench.led_over(4), [true, false, true, false]);
}

#[test]
fn a_tick_restores_the_channels_after_the_expander_browns_out() {
    let mut bench = Bench::boot();
    bench.write(0xFFFF, 0x0181);
    bench.expander.borrow_mut().brown_out();
    bench.tick();
    assert_eq!(bench.driven(), (0x80, 0x81));
    assert!(bench.led());
}

#[test]
fn a_tick_leaves_a_healthy_expander_alone() {
    let mut bench = Bench::boot();
    bench.write(0xFFFF, 0x0181);
    let ops = bench.expander.borrow().log.len();
    bench.tick();
    let log = &bench.expander.borrow().log;
    // Reads only: a write longer than a register address would change something.
    let changes =
        |op: &Op| matches!(op, Op::Reset(_)) || matches!(op, Op::Write(bytes) if bytes.len() > 1);
    assert!(!log[ops..].iter().any(changes), "{log:?}");
}

#[test]
fn reads_back_the_channels_on_the_reversed_rev_a_wiring() {
    let mut bench = Bench::boot();
    bench.write(0xFFFF, 0x0181);
    assert_eq!(bench.board.read_channels(), Ok(0x0181));
}

#[test]
fn an_expander_that_comes_back_gets_its_channels_restored() {
    let mut bench = Bench::boot();
    bench.write(0xFFFF, 0x0181);
    bench.expander.borrow_mut().absent = true;
    bench.tick();
    bench.tick();
    assert!(bench.expander.borrow().in_reset());
    bench.expander.borrow_mut().absent = false;
    bench.tick();
    assert_eq!(bench.driven(), (0x80, 0x81));
}

#[test]
fn once_the_supervisor_trips_the_expander_stays_in_reset() {
    let mut bench = Bench::boot();
    bench.write(0xFFFF, 0x0181);
    bench.trip_supervisor();
    bench.tick();
    bench.tick();
    assert!(bench.expander.borrow().in_reset());
    assert_eq!(bench.driven(), (0x00, 0x00));
}

#[test]
fn a_production_build_does_not_report_wifi() {
    let bench = Bench::boot();
    assert_eq!(bench.board.info().features & FEATURE_WIFI, 0);
}
