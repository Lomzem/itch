use itch_firmware::crash::{self, RECORD_LEN};

#[test]
fn a_record_gives_back_where_the_panic_happened() {
    let record = crash::record("src/main.rs", 88, 5);
    assert_eq!(crash::location(&record), Some("src/main.rs:88:5"));
}

#[test]
fn a_location_over_64_bytes_keeps_its_end() {
    let file = "/home/dev/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/esp-hal-1.2.2/src/i2c/master/mod.rs";
    let record = crash::record(file, 1234, 17);
    assert_eq!(
        crash::location(&record),
        Some(".io-1949cf8c6b5b557f/esp-hal-1.2.2/src/i2c/master/mod.rs:1234:17"),
    );
}

#[test]
fn a_location_is_cut_between_characters() {
    let file = format!("src/{}/main.rs", "é".repeat(40));
    let record = crash::record(&file, 7, 10);
    let expected = format!("{}/main.rs:7:10", "é".repeat(25));
    assert_eq!(crash::location(&record), Some(expected.as_str()));
}

#[test]
fn zeroed_ram_holds_no_record() {
    assert_eq!(crash::location(&[0; RECORD_LEN]), None);
}

#[test]
fn a_record_with_any_byte_corrupted_is_ignored() {
    let record = crash::record("src/main.rs", 88, 5);
    for at in 0..RECORD_LEN {
        let mut corrupted = record;
        corrupted[at] ^= 0x10;
        assert_eq!(crash::location(&corrupted), None, "byte {at} corrupted");
    }
}

#[test]
fn a_record_cut_short_over_cleared_ram_is_ignored() {
    let record = crash::record("src/main.rs", 88, 5);
    for written in 0..RECORD_LEN {
        let mut torn = [0; RECORD_LEN];
        torn[..written].copy_from_slice(&record[..written]);
        assert_eq!(crash::location(&torn), None, "{written} bytes written");
    }
}
