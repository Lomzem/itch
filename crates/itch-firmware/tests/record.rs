use itch_core::label::Label;
use itch_core::wifi::{Addressing, Credentials, StaticAddress};
use itch_firmware::settings::{Snapshot, decode, encode};

/// DHCP's address configuration: mode, address, prefix and gateway are all
/// zero.
const DHCP: [u8; 10] = [0; 10];

/// `192.0.2.10/24` through `192.0.2.1`: mode 1, the address, the prefix,
/// the gateway.
const STATIC: [u8; 10] = [1, 192, 0, 2, 10, 24, 192, 0, 2, 1];

/// A version 1 record: the version (u16 LE), the label's length, the label.
const V1_BENCH_3: &[u8] = b"\x01\x00\x07bench-3";

fn snapshot(label: &str, wifi: Option<(&str, &str)>) -> Snapshot {
    Snapshot {
        label: Label::new(label).unwrap(),
        wifi: wifi.map(|(ssid, passphrase)| Credentials::new(ssid, passphrase).unwrap()),
        addressing: Addressing::Dhcp,
    }
}

fn static_address(address: [u8; 4], prefix: u8, gateway: [u8; 4]) -> Addressing {
    Addressing::Static(StaticAddress::new(address, prefix, gateway).unwrap())
}

/// A version 2 record: the version (u16 LE), then the label, SSID and
/// passphrase, each as a length and its bytes, then the address
/// configuration.
fn v2(label: &[u8], ssid: &[u8], passphrase: &[u8], address: &[u8]) -> Vec<u8> {
    let mut record = vec![0x02, 0x00];
    for text in [label, ssid, passphrase] {
        record.push(u8::try_from(text.len()).unwrap());
        record.extend_from_slice(text);
    }
    record.extend_from_slice(address);
    record
}

#[test]
fn settings_are_saved_as_a_version_2_record() {
    let saved = snapshot("bench-3", Some(("ExampleRigNet", "example-passphrase")));
    let record =
        [&b"\x02\x00\x07bench-3\x0dExampleRigNet\x12example-passphrase"[..], &DHCP].concat();
    assert_eq!(encode(&saved).as_bytes(), record);
}

#[test]
fn settings_without_credentials_save_empty_ones() {
    let record = [&b"\x02\x00\x07bench-3\x00\x00"[..], &DHCP].concat();
    assert_eq!(encode(&snapshot("bench-3", None)).as_bytes(), record);
}

#[test]
fn a_static_address_is_saved_after_the_credentials() {
    let saved = Snapshot {
        addressing: static_address([192, 0, 2, 10], 24, [192, 0, 2, 1]),
        ..snapshot("bench-3", None)
    };
    let record = [&b"\x02\x00\x07bench-3\x00\x00"[..], &STATIC].concat();
    assert_eq!(encode(&saved).as_bytes(), record);
}

#[test]
fn a_version_2_record_gives_back_its_settings() {
    for saved in [
        snapshot("bench-3", Some(("ExampleRigNet", "example-passphrase"))),
        snapshot("", None),
        snapshot(&"é".repeat(32), Some((&"é".repeat(16), &"p".repeat(63)))),
        Snapshot {
            addressing: static_address([192, 0, 2, 10], 24, [192, 0, 2, 1]),
            ..snapshot("bench-3", Some(("ExampleRigNet", "example-passphrase")))
        },
        Snapshot { addressing: static_address([192, 0, 2, 10], 32, [0; 4]), ..snapshot("", None) },
    ] {
        assert_eq!(decode(encode(&saved).as_bytes()), Some(saved));
    }
}

#[test]
fn a_version_1_record_gives_back_its_label_with_no_credentials() {
    assert_eq!(decode(V1_BENCH_3), Some(snapshot("bench-3", None)));
    assert_eq!(decode(b"\x01\x00\x00"), Some(snapshot("", None)));
}

#[test]
fn a_record_of_another_version_is_ignored() {
    assert_eq!(decode(b"\x00\x00\x07bench-3"), None);
    assert_eq!(decode(b"\x01\x01\x07bench-3"), None);
    let v3 = [&b"\x03\x00\x07bench-3\x00\x00"[..], &DHCP].concat();
    assert_eq!(decode(&v3), None);
}

#[test]
fn a_malformed_version_1_record_is_ignored() {
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

#[test]
fn a_malformed_version_2_record_is_ignored() {
    let ssid = b"ExampleRigNet";
    let passphrase = b"example-passphrase";
    for record in [
        // The address configuration is cut short, or has a byte left over.
        v2(b"bench-3", ssid, passphrase, &DHCP[..9]),
        v2(b"bench-3", ssid, passphrase, &[0; 11]),
        // Text that isn't UTF-8, or is too long.
        v2(b"\xC3\x28", ssid, passphrase, &DHCP),
        v2(&[b'a'; 65], ssid, passphrase, &DHCP),
        v2(b"bench-3", &[b's'; 33], passphrase, &DHCP),
        v2(b"bench-3", ssid, &[b'p'; 64], &DHCP),
        // Credentials no SetWifi could have given.
        v2(b"bench-3", ssid, b"1234567", &DHCP),
        v2(b"bench-3", ssid, b"", &DHCP),
        v2(b"bench-3", b"", passphrase, &DHCP),
        // An address configuration no SetAddress could have given.
        v2(b"bench-3", ssid, passphrase, &[2, 192, 0, 2, 10, 24, 192, 0, 2, 1]),
        v2(b"bench-3", ssid, passphrase, &[0, 192, 0, 2, 10, 0, 0, 0, 0, 0]),
        v2(b"bench-3", ssid, passphrase, &[1, 0, 0, 0, 0, 24, 0, 0, 0, 0]),
        v2(b"bench-3", ssid, passphrase, &[1, 192, 0, 2, 10, 33, 0, 0, 0, 0]),
    ] {
        assert_eq!(decode(&record), None, "{record:02X?}");
    }
}
