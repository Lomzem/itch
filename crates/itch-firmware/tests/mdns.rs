//! The mDNS records a Rig advertises, as edge-mdns puts them on the wire.

use edge_mdns::domain::base::name::ToLabelIter;
use edge_mdns::domain::base::{Message, MessageBuilder, ParsedName, Rtype};
use edge_mdns::domain::rdata::AllRecordData;
use edge_mdns::{Buf, HostAnswersMdnsHandler, MdnsHandler, MdnsRequest, MdnsResponse, NameSlice};
use itch_core::label::Label;
use itch_firmware::mdns::{Advertisement, Answers, Identity};

/// The made-up Rig of the docs, at an address from the documentation range.
const HARDWARE_ID: [u8; 6] = [0x60, 0x55, 0xF9, 0xC0, 0x27, 0x18];
const ADDRESS: [u8; 4] = [192, 0, 2, 10];

type Record<'a> = edge_mdns::domain::base::Record<
    ParsedName<&'a [u8]>,
    AllRecordData<&'a [u8], ParsedName<&'a [u8]>>,
>;

/// What the responder sends for `request` with `answers`. Empty if it
/// sends nothing.
fn respond(answers: Answers, request: MdnsRequest) -> Vec<u8> {
    let mut buffer = [0; 1500];
    let mut handler = HostAnswersMdnsHandler::new(answers);
    match handler.handle(request, &mut buffer).expect("the records fit") {
        MdnsResponse::Reply { data, .. } => data.to_vec(),
        MdnsResponse::None => vec![],
    }
}

/// The DNS message that announces `answers`.
fn announcement(answers: Answers) -> Vec<u8> {
    respond(answers, MdnsRequest::None)
}

/// Reads each record in the answer section of `message`, then each in
/// its additional section.
fn read<T>(message: &[u8], mut read: impl FnMut(Record) -> Option<T>) -> Vec<T> {
    let message = Message::from_octets(message).expect("a DNS message");
    let mut records = vec![];
    for section in [message.answer(), message.additional()] {
        let section = section.expect("a section").limit_to::<AllRecordData<_, ParsedName<_>>>();
        records.extend(section.filter_map(|record| read(record.expect("a record"))));
    }
    records
}

/// The records `answers` announces, one per line in DNS presentation
/// format.
fn announced(answers: Answers) -> Vec<String> {
    read(&announcement(answers), |record| Some(record.to_string()))
}

/// The instance name `answers` announces: the first label of its SRV
/// record's owner.
fn instance(answers: Answers) -> String {
    let instances = read(&announcement(answers), |record| {
        let AllRecordData::Srv(_) = record.data() else { return None };
        let label = record.owner().iter_labels().next().expect("an instance label");
        Some(String::from_utf8(label.as_slice().to_vec()).expect("UTF-8"))
    });
    instances.into_iter().next().expect("an SRV record")
}

/// An mDNS query from port 5353 for `name`'s `rtype` records.
fn query(name: &[&str], rtype: Rtype) -> Vec<u8> {
    let mut buffer = [0; 512];
    let mut question = MessageBuilder::from_target(Buf::new(&mut buffer)).expect("room").question();
    question.push((NameSlice::new(name), rtype)).expect("room");
    let Buf(bytes, len) = question.finish();
    bytes[..len].to_vec()
}

/// What the responder answers `query` with, one record per line.
fn answered(answers: Answers, query: &[u8]) -> Vec<String> {
    let request = MdnsRequest::Request { legacy: false, multicast: true, data: query };
    read(&respond(answers, request), |record| Some(record.to_string()))
}

fn advertisement(label: &str) -> Advertisement {
    Advertisement::new(Label::new(label).expect("a valid label"), ADDRESS)
}

#[test]
fn a_rig_announces_its_label_hardware_id_hostname_and_address() {
    let identity = Identity::new(HARDWARE_ID);
    let records = announced(Answers::announce(&identity, &advertisement("bench-3")));
    assert_eq!(
        records,
        [
            "itch-c02718.local. 120 IN A 192.0.2.10",
            "bench-3._itch._tcp.local. 120 IN SRV 0 0 7878 itch-c02718.local",
            "bench-3._itch._tcp.local. 120 IN TXT \"hwid=60:55:F9:C0:27:18\"",
            "_services._dns-sd._udp.local. 120 IN PTR _itch._tcp.local.",
            "_itch._tcp.local. 120 IN PTR bench-3._itch._tcp.local.",
        ]
    );
}

#[test]
fn browsing_for_itch_finds_the_rig_with_everything_needed_to_connect() {
    let identity = Identity::new(HARDWARE_ID);
    let advertisement = advertisement("bench-3");
    let browse = query(&["_itch", "_tcp", "local"], Rtype::PTR);
    assert_eq!(
        answered(Answers::announce(&identity, &advertisement), &browse),
        [
            "_itch._tcp.local. 120 IN PTR bench-3._itch._tcp.local.",
            "itch-c02718.local. 120 IN A 192.0.2.10",
            "bench-3._itch._tcp.local. 120 IN SRV 0 0 7878 itch-c02718.local",
            "bench-3._itch._tcp.local. 120 IN TXT \"hwid=60:55:F9:C0:27:18\"",
        ]
    );
}

#[test]
fn the_hostname_resolves_to_the_rigs_address() {
    let identity = Identity::new(HARDWARE_ID);
    let advertisement = advertisement("bench-3");
    let lookup = query(&["itch-c02718", "local"], Rtype::A);
    // edge-mdns adds the service's records after the answer.
    let records = answered(Answers::announce(&identity, &advertisement), &lookup);
    assert_eq!(records.first().map(String::as_str), Some("itch-c02718.local. 120 IN A 192.0.2.10"));
}

#[test]
fn a_64_byte_label_is_announced_cut_to_the_63_bytes_a_dns_label_holds() {
    let identity = Identity::new(HARDWARE_ID);
    let label = "a".repeat(64);
    assert_eq!(instance(Answers::announce(&identity, &advertisement(&label))), "a".repeat(63));
}

#[test]
fn a_long_label_is_cut_before_a_character_that_would_straddle_byte_63() {
    let identity = Identity::new(HARDWARE_ID);
    // 62 bytes, then a 2-byte character in bytes 63 and 64.
    let label = format!("{}é", "a".repeat(62));
    assert_eq!(instance(Answers::announce(&identity, &advertisement(&label))), "a".repeat(62));
}

#[test]
fn a_63_byte_label_is_announced_whole() {
    let identity = Identity::new(HARDWARE_ID);
    let label = format!("{}é", "a".repeat(61));
    assert_eq!(instance(Answers::announce(&identity, &advertisement(&label))), label);
}

#[test]
fn a_goodbye_withdraws_the_service_records_but_keeps_the_hostname() {
    let identity = Identity::new(HARDWARE_ID);
    let records = announced(Answers::goodbye(&identity, &advertisement("bench-3")));
    assert_eq!(
        records,
        [
            "bench-3._itch._tcp.local. 0 IN SRV 0 0 7878 itch-c02718.local",
            "bench-3._itch._tcp.local. 0 IN TXT \"hwid=60:55:F9:C0:27:18\"",
            "_services._dns-sd._udp.local. 0 IN PTR _itch._tcp.local.",
            "_itch._tcp.local. 0 IN PTR bench-3._itch._tcp.local.",
        ]
    );
}
