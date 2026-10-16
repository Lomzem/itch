mod common;

use common::{cases, hex, vectors};
use itch_core::frame::MAX_PAYLOAD;
use itch_core::wire::{
    ErrorCode, Event, HelloReply, LogEvent, LogLevel, Message, Reply, Request, RequestType,
    Response,
};
use serde_json::Value;

#[test]
fn encoding_a_message_produces_the_vector_payload() {
    for case in cases(&vectors(), "messages") {
        let mut out = [0; MAX_PAYLOAD];
        let len = message(&case["message"]).encode(&mut out);
        assert_eq!(out[..len], hex(case["payload"].as_str().unwrap()), "{}", case["name"]);
    }
}

#[test]
fn decoding_a_vector_payload_produces_the_message() {
    for case in cases(&vectors(), "messages") {
        let payload = hex(case["payload"].as_str().unwrap());
        assert_eq!(Message::decode(&payload), Ok(message(&case["message"])), "{}", case["name"]);
    }
}

/// Builds the message a vector describes. See "Conformance vectors" in
/// `docs/protocol/README.md` for the JSON shape.
fn message(json: &Value) -> Message<'_> {
    if json["kind"] == "event" {
        return Message::Event(event(json));
    }
    let request_id = u32::try_from(json["request_id"].as_u64().unwrap()).unwrap();
    let request_type = request_type(json["type"].as_str().unwrap());
    match (json["kind"].as_str().unwrap(), json["status"].as_str()) {
        ("request", None) => Message::Request { request_id, request: request(request_type, json) },
        ("response", Some("Ok")) => {
            Message::Response { request_id, response: Response::Ok(reply(request_type, json)) }
        }
        ("response", Some(code)) => Message::Response {
            request_id,
            response: Response::Err {
                request_type: request_type as u8,
                error: error_code(code),
                detail: json["detail"].as_str().unwrap(),
            },
        },
        other => panic!("unknown message shape {other:?}"),
    }
}

fn request_type(name: &str) -> RequestType {
    match name {
        "Hello" => RequestType::Hello,
        "SetLogLevel" => RequestType::SetLogLevel,
        _ => panic!("unknown request type {name}"),
    }
}

fn request(request_type: RequestType, json: &Value) -> Request {
    match request_type {
        RequestType::Hello => Request::Hello,
        RequestType::SetLogLevel => Request::SetLogLevel { level: log_level(&json["level"]) },
    }
}

fn reply(request_type: RequestType, json: &Value) -> Reply<'_> {
    match request_type {
        RequestType::Hello => Reply::Hello(HelloReply {
            protocol: int(&json["protocol"]),
            firmware_version: json["firmware_version"].as_str().unwrap(),
            dirty: json["dirty"].as_bool().unwrap(),
            features: int(&json["features"]),
            channel_count: int(&json["channel_count"]),
            board_revision: json["board_revision"].as_str().unwrap(),
            hardware_id: hex(json["hardware_id"].as_str().unwrap()).try_into().unwrap(),
            label: json["label"].as_str().unwrap(),
        }),
        RequestType::SetLogLevel => Reply::SetLogLevel,
    }
}

fn event(json: &Value) -> Event<'_> {
    match json["type"].as_str().unwrap() {
        "Log" => Event::Log(LogEvent {
            level: log_level(&json["level"]),
            dropped: int(&json["dropped"]),
            text: json["text"].as_str().unwrap(),
        }),
        name => panic!("unknown event type {name}"),
    }
}

fn log_level(json: &Value) -> LogLevel {
    LogLevel::from_u8(int(json)).unwrap()
}

fn error_code(name: &str) -> ErrorCode {
    use ErrorCode::*;
    [
        UnknownRequest,
        UnsupportedVersion,
        Malformed,
        LeaseRequired,
        LeaseHeld,
        LeaseLost,
        Busy,
        HardwareFault,
        StorageFault,
        Internal,
    ]
    .into_iter()
    .find(|code| format!("{code:?}") == name)
    .unwrap_or_else(|| panic!("unknown error code {name}"))
}

fn int<T: TryFrom<u64>>(json: &Value) -> T {
    T::try_from(json.as_u64().unwrap()).ok().unwrap()
}
