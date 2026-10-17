use itch_firmware::queue::busy;

#[test]
fn busy_echoes_the_request_type_and_id() {
    // GetChannels, request_id 0x12345678.
    let request = [0x02, 0x78, 0x56, 0x34, 0x12];
    let reply = busy(&request).expect("a Busy reply");
    assert_eq!(reply.as_bytes(), [0x82, 0x78, 0x56, 0x34, 0x12, 0x07, 0x00]);
}

#[test]
fn a_payload_without_a_request_id_gets_no_busy_reply() {
    assert!(busy(&[0x02, 0x78, 0x56, 0x34]).is_none());
}
