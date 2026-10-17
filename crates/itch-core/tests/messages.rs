mod common;

use common::{cases, hex, vectors};
use itch_core::frame::MAX_PAYLOAD;
use itch_core::wire::{
    AcquireLease, DecodeError, ErrorCode, HelloReply, Message, PROTOCOL, RenewLease, Reply,
    Request, RequestType, Response, StatusReply, WriteChannels,
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

#[test]
fn decoding_a_malformed_vector_fails_with_its_error() {
    for case in cases(&vectors(), "malformed") {
        let payload = hex(case["payload"].as_str().unwrap());
        let expected = match case["error"].as_str().unwrap() {
            "Malformed" => DecodeError::Malformed,
            "UnknownRequest" => DecodeError::UnknownType,
            other => panic!("unknown error {other}"),
        };
        assert_eq!(Message::decode(&payload), Err(expected), "{}", case["name"]);
    }
}

/// The release manifest reads the `protocol` from the vectors.
#[test]
fn the_vectors_are_for_this_protocol() {
    assert_eq!(vectors()["protocol"], PROTOCOL);
}

/// Builds the message a vector describes. See "Conformance vectors" in
/// `docs/protocol/README.md` for the JSON shape.
fn message(json: &Value) -> Message<'_> {
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
        "GetChannels" => RequestType::GetChannels,
        "WriteChannels" => RequestType::WriteChannels,
        "Clear" => RequestType::Clear,
        "GetStatus" => RequestType::GetStatus,
        "AcquireLease" => RequestType::AcquireLease,
        "RenewLease" => RequestType::RenewLease,
        "ReleaseLease" => RequestType::ReleaseLease,
        _ => panic!("unknown request type {name}"),
    }
}

fn request(request_type: RequestType, json: &Value) -> Request<'_> {
    match request_type {
        RequestType::Hello => Request::Hello,
        RequestType::GetChannels => Request::GetChannels,
        RequestType::WriteChannels => Request::WriteChannels(WriteChannels {
            token: token(&json["token"]),
            mask: int(&json["mask"]),
            values: int(&json["values"]),
        }),
        RequestType::Clear => Request::Clear { token: token(&json["token"]) },
        RequestType::GetStatus => Request::GetStatus,
        RequestType::AcquireLease => Request::AcquireLease(AcquireLease {
            token: token(&json["token"]),
            takeover: json["takeover"].as_bool().unwrap(),
            holder: json["holder"].as_str().unwrap(),
        }),
        RequestType::RenewLease => Request::RenewLease(RenewLease {
            token: token(&json["token"]),
            holder: json["holder"].as_str().unwrap(),
        }),
        RequestType::ReleaseLease => Request::ReleaseLease { token: token(&json["token"]) },
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
        RequestType::GetChannels => Reply::GetChannels { state: int(&json["state"]) },
        RequestType::WriteChannels => Reply::WriteChannels { state: int(&json["state"]) },
        RequestType::Clear => Reply::Clear { state: int(&json["state"]) },
        RequestType::GetStatus => Reply::GetStatus(StatusReply {
            uptime_ms: int(&json["uptime_ms"]),
            reset_reason: int(&json["reset_reason"]),
            last_panic: json["last_panic"].as_str().unwrap(),
            holder: json["holder"].as_str().unwrap(),
            crc_failures: int(&json["crc_failures"]),
            oversized_frames: int(&json["oversized_frames"]),
            malformed_frames: int(&json["malformed_frames"]),
            dropped_logs: int(&json["dropped_logs"]),
            expander_faults: int(&json["expander_faults"]),
            usb_tx_dropped: int(&json["usb_tx_dropped"]),
        }),
        RequestType::AcquireLease => Reply::AcquireLease { ttl_ms: int(&json["ttl_ms"]) },
        RequestType::RenewLease => Reply::RenewLease,
        RequestType::ReleaseLease => Reply::ReleaseLease,
    }
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

/// A `u64` token, written as a `0x` hex string.
fn token(json: &Value) -> u64 {
    u64::from_str_radix(json.as_str().unwrap().strip_prefix("0x").unwrap(), 16).unwrap()
}
