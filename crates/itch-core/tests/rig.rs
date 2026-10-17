mod common;

use common::{FakeBoard, cases, hex, vectors};
use itch_core::frame::MAX_PAYLOAD;
use itch_core::rig::{Handled, Rig};
use itch_core::wire::{
    ErrorCode, FEATURE_WIFI, HelloReply, LogLevel, Message, Reply, Request, Response,
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

/// Payloads under 5 bytes have no `request_id` and get no reply. The rest
/// get the vector's error with their `request_id` echoed.
#[test]
fn malformed_requests_get_the_error_the_vectors_give() {
    for case in cases(&vectors(), "malformed") {
        let name = &case["name"];
        let payload = hex(case["payload"].as_str().unwrap());
        if payload[0] >= 0x40 {
            continue; // An Event or a reply, which only clients decode.
        }
        let reply = handle(&mut FakeBoard::new(), &payload);
        if payload.len() < 5 {
            assert_eq!(reply, None, "{name}");
            continue;
        }
        let error = match case["error"].as_str().unwrap() {
            "Malformed" => ErrorCode::Malformed,
            "UnknownRequest" => ErrorCode::UnknownRequest,
            other => panic!("unknown error {other}"),
        };
        let header = [&[payload[0] | 0x80], &payload[1..5], &[error as u8]].concat();
        assert_eq!(reply.unwrap()[..6], header, "{name}");
    }
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
