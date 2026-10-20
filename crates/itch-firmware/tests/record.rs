use itch_core::label::Label;
use itch_firmware::settings::{decode, encode};

/// A version 1 record: the version (u16 LE), the label's length, the label.
const BENCH_3: &[u8] = b"\x01\x00\x07bench-3";

#[test]
fn a_label_is_saved_as_a_version_1_record() {
    let label = Label::new("bench-3").unwrap();
    assert_eq!(encode(&label).as_bytes(), BENCH_3);
}

#[test]
fn a_version_1_record_gives_back_its_label() {
    assert_eq!(decode(BENCH_3).as_ref().map(Label::as_str), Some("bench-3"));
    assert_eq!(decode(b"\x01\x00\x00").as_ref().map(Label::as_str), Some(""));
}

#[test]
fn a_longest_label_round_trips() {
    let label = Label::new(&"é".repeat(32)).unwrap();
    assert_eq!(decode(encode(&label).as_bytes()), Some(label));
}

#[test]
fn a_record_of_another_version_is_ignored() {
    assert_eq!(decode(b"\x00\x00\x07bench-3"), None);
    assert_eq!(decode(b"\x02\x00\x07bench-3"), None);
    assert_eq!(decode(b"\x01\x01\x07bench-3"), None);
}

#[test]
fn a_malformed_record_is_ignored() {
    for record in [
        &b""[..],
        b"\x01\x00",
        b"\x01\x00\x08bench-3",
        b"\x01\x00\x06bench-3",
        b"\x01\x00\x02\xC3\x28",
    ] {
        assert_eq!(decode(record), None, "{record:02X?}");
    }
    let over_64 = [&b"\x01\x00\x41"[..], &[b'a'; 65]].concat();
    assert_eq!(decode(&over_64), None);
}
