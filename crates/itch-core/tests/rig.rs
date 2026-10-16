mod common;

use common::FakeBoard;
use itch_core::frame::MAX_PAYLOAD;
use itch_core::rig::{Handled, Rig};
use itch_core::wire::{
    ErrorCode, FEATURE_WIFI, HelloReply, LogLevel, Message, Reply, Request, RequestType, Response,
};

#[test]
fn hello_reports_the_rig_identity_and_default_label() {
    let reply = handle(&mut FakeBoard::new(), &request(0x1234_5678, Request::Hello)).unwrap();
    assert_eq!(
        Message::decode(&reply),
        Ok(Message::Response {
            request_id: 0x1234_5678,
            response: Response::Ok(Reply::Hello(HelloReply {
                protocol: 1,
                firmware_version: "1.0.0",
                dirty: false,
                features: 0,
                channel_count: 16,
                board_revision: "A",
                hardware_id: [0x60, 0x55, 0xF9, 0xC0, 0x27, 0x18],
                label: "itch-C02718",
            })),
        })
    );
}

#[test]
fn hello_reports_a_development_build() {
    let mut board = FakeBoard::new();
    board.info.dirty = true;
    board.info.features = FEATURE_WIFI;
    let reply = handle(&mut board, &request(1, Request::Hello)).unwrap();
    let Ok(Message::Response { response: Response::Ok(Reply::Hello(hello)), .. }) =
        Message::decode(&reply)
    else {
        panic!("not a Hello reply");
    };
    assert!(hello.dirty);
    assert_eq!(hello.features, FEATURE_WIFI);
}

#[test]
fn an_unknown_request_type_gets_unknown_request_with_its_id() {
    let reply = handle(&mut FakeBoard::new(), &[0x3F, 0x78, 0x56, 0x34, 0x12]).unwrap();
    assert_eq!(reply[..6], [0xBF, 0x78, 0x56, 0x34, 0x12, ErrorCode::UnknownRequest as u8]);
}

#[test]
fn a_request_with_trailing_bytes_gets_malformed_with_its_id() {
    let mut payload = request(0x1234_5678, Request::Hello);
    payload.push(0);
    let reply = handle(&mut FakeBoard::new(), &payload).unwrap();
    assert_eq!(reply[..6], [0x81, 0x78, 0x56, 0x34, 0x12, ErrorCode::Malformed as u8]);
}

#[test]
fn a_payload_without_a_request_id_gets_no_reply() {
    assert_eq!(handle(&mut FakeBoard::new(), &[RequestType::Hello as u8, 0, 0, 0]), None);
}

#[test]
fn set_log_level_hands_the_level_to_the_calling_connection() {
    let mut reply = [0; MAX_PAYLOAD];
    let request = request(9, Request::SetLogLevel { level: LogLevel::Info });
    let Handled::SetLogLevel { len, level } =
        Rig::new().handle(&request, &mut FakeBoard::new(), &mut reply)
    else {
        panic!("not a SetLogLevel");
    };
    assert_eq!(level, LogLevel::Info);
    assert_eq!(
        Message::decode(&reply[..len]),
        Ok(Message::Response { request_id: 9, response: Response::Ok(Reply::SetLogLevel) })
    );
}

#[test]
fn a_log_level_above_debug_gets_malformed() {
    let reply = handle(&mut FakeBoard::new(), &[0x07, 9, 0, 0, 0, 5]).unwrap();
    assert_eq!(reply[..6], [0x87, 9, 0, 0, 0, ErrorCode::Malformed as u8]);
}

fn request(request_id: u32, request: Request) -> Vec<u8> {
    let mut out = [0; MAX_PAYLOAD];
    let len = Message::Request { request_id, request }.encode(&mut out);
    out[..len].to_vec()
}

fn handle(board: &mut FakeBoard, request: &[u8]) -> Option<Vec<u8>> {
    let mut reply = [0; MAX_PAYLOAD];
    match Rig::new().handle(request, board, &mut reply) {
        Handled::NoReply => None,
        Handled::Reply { len, .. } | Handled::SetLogLevel { len, .. } => {
            Some(reply[..len].to_vec())
        }
    }
}
