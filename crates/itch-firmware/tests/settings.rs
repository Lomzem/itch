mod common;

use common::{Bench, DHCP, Flash, STATIC};

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

const SSID: &str = "ExampleRigNet";
const PASSPHRASE: &str = "example-passphrase";

fn credentials(ssid: &str, passphrase: &str) -> Option<(String, String)> {
    Some((ssid.into(), passphrase.into()))
}

#[test]
fn wifi_credentials_survive_a_power_cycle() {
    let mut bench = Bench::boot();
    assert_eq!(bench.wifi(), None);
    assert_eq!(bench.set_wifi(SSID, PASSPHRASE), Ok(()));

    let mut bench = bench.power_cycle();
    assert_eq!(bench.wifi(), Some(SSID.into()));
    assert_eq!(bench.credentials(), credentials(SSID, PASSPHRASE));
}

#[test]
fn cleared_wifi_credentials_stay_cleared_after_a_power_cycle() {
    let mut bench = Bench::boot();
    bench.set_wifi(SSID, PASSPHRASE).unwrap();
    assert_eq!(bench.set_wifi("", ""), Ok(()));

    let mut bench = bench.power_cycle();
    assert_eq!(bench.wifi(), None);
    assert_eq!(bench.credentials(), None);
}

#[test]
fn changing_the_label_or_the_credentials_keeps_the_other() {
    let mut bench = Bench::boot();
    bench.set_wifi(SSID, PASSPHRASE).unwrap();
    bench.set_label("bench-3").unwrap();
    let mut bench = bench.power_cycle();
    assert_eq!((bench.label(), bench.wifi()), ("bench-3".into(), Some(SSID.into())));

    bench.set_wifi("OtherExampleNet", "another-passphrase").unwrap();
    let mut bench = bench.power_cycle();
    assert_eq!(bench.label(), "bench-3");
    assert_eq!(bench.credentials(), credentials("OtherExampleNet", "another-passphrase"));
}

#[test]
fn a_version_1_record_loads_as_its_label_with_no_credentials_and_takes_them() {
    let mut bench = Bench::boot_with(Flash::holding(b"\x01\x00\x07bench-3"));
    assert_eq!((bench.label(), bench.wifi()), ("bench-3".into(), None));

    bench.set_wifi(SSID, PASSPHRASE).unwrap();
    let mut bench = bench.power_cycle();
    assert_eq!((bench.label(), bench.wifi()), ("bench-3".into(), Some(SSID.into())));
}

#[test]
fn a_scrambled_partition_boots_with_no_credentials_and_takes_new_ones() {
    let flash = Flash::erased();
    flash.borrow_mut().scramble();
    let mut bench = Bench::boot_with(flash);
    assert_eq!(bench.wifi(), None);

    assert_eq!(bench.set_wifi(SSID, PASSPHRASE), Ok(()));
    let mut bench = bench.power_cycle();
    assert_eq!(bench.credentials(), credentials(SSID, PASSPHRASE));
}

#[test]
fn credentials_the_flash_fails_to_save_are_refused_and_the_old_ones_kept() {
    let mut bench = Bench::boot();
    bench.set_wifi(SSID, PASSPHRASE).unwrap();
    bench.flash.borrow_mut().broken = true;
    assert_eq!(bench.set_wifi("OtherExampleNet", PASSPHRASE), Err("StorageFault".into()));
    assert_eq!(bench.set_wifi("", ""), Err("StorageFault".into()));
    assert_eq!(bench.wifi(), Some(SSID.into()));
    // A later label change saves the credentials the Rig still has.
    bench.flash.borrow_mut().broken = false;
    bench.set_label("bench-3").unwrap();

    let mut bench = bench.power_cycle();
    assert_eq!(bench.credentials(), credentials(SSID, PASSPHRASE));
}

#[test]
fn a_static_address_survives_a_power_cycle() {
    let mut bench = Bench::boot();
    assert_eq!(bench.addressing(), DHCP);
    assert_eq!(bench.set_address(STATIC), Ok(()));

    let mut bench = bench.power_cycle();
    assert_eq!(bench.addressing(), STATIC);
}

#[test]
fn switching_back_to_dhcp_survives_a_power_cycle() {
    let mut bench = Bench::boot();
    bench.set_address(STATIC).unwrap();
    assert_eq!(bench.set_address(DHCP), Ok(()));

    let mut bench = bench.power_cycle();
    assert_eq!(bench.addressing(), DHCP);
}

#[test]
fn changing_the_address_keeps_the_label_and_credentials_and_they_keep_it() {
    let mut bench = Bench::boot();
    bench.set_label("bench-3").unwrap();
    bench.set_wifi(SSID, PASSPHRASE).unwrap();
    bench.set_address(STATIC).unwrap();
    let mut bench = bench.power_cycle();
    assert_eq!(bench.label(), "bench-3");
    assert_eq!(bench.credentials(), credentials(SSID, PASSPHRASE));

    bench.set_label("bench-4").unwrap();
    bench.set_wifi("OtherExampleNet", "another-passphrase").unwrap();
    let mut bench = bench.power_cycle();
    assert_eq!(bench.addressing(), STATIC);
}

#[test]
fn a_version_1_record_loads_with_dhcp() {
    let mut bench = Bench::boot_with(Flash::holding(b"\x01\x00\x07bench-3"));
    assert_eq!(bench.addressing(), DHCP);
}

/// A saved address the network stack would panic on, such as the multicast
/// 224.0.0.1/24 an older firmware accepted, must not stop the Rig booting.
#[test]
fn a_saved_address_the_rig_cannot_use_loads_as_dhcp_and_keeps_the_rest() {
    let record = b"\x02\x00\x07bench-3\x0dExampleRigNet\x12example-passphrase\
        \x01\xe0\x00\x00\x01\x18\xc0\x00\x02\x01";
    let mut bench = Bench::boot_with(Flash::holding(record));
    assert_eq!(bench.addressing(), DHCP);
    assert_eq!(bench.label(), "bench-3");
    assert_eq!(bench.credentials(), credentials(SSID, PASSPHRASE));
}

#[test]
fn an_address_the_flash_fails_to_save_is_refused_and_the_old_one_kept() {
    let mut bench = Bench::boot();
    bench.set_address(STATIC).unwrap();
    bench.flash.borrow_mut().broken = true;
    assert_eq!(bench.set_address(DHCP), Err("StorageFault".into()));
    assert_eq!(bench.addressing(), STATIC);
    // A later label change saves the address the Rig still has.
    bench.flash.borrow_mut().broken = false;
    bench.set_label("bench-3").unwrap();

    let mut bench = bench.power_cycle();
    assert_eq!(bench.addressing(), STATIC);
}
