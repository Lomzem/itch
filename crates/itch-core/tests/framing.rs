mod common;

use common::{cases, hex, vectors};
use itch_core::frame::{FrameDecoder, FrameError, MAX_FRAME, encode_frame};

#[test]
fn encoding_a_payload_produces_the_vector_frame() {
    for case in cases(&vectors(), "frames") {
        let mut out = [0; MAX_FRAME];
        let len = encode_frame(&hex(case["payload"].as_str().unwrap()), &mut out);
        assert_eq!(out[..len], hex(case["frame"].as_str().unwrap()), "{}", case["name"]);
    }
}

#[test]
fn decoding_a_vector_frame_yields_its_payload() {
    for case in cases(&vectors(), "frames") {
        let frames = decode(&hex(case["frame"].as_str().unwrap()));
        assert_eq!(frames, [Ok(hex(case["payload"].as_str().unwrap()))], "{}", case["name"]);
    }
}

#[test]
fn decoder_resyncs_drops_bad_frames_and_counts_them() {
    for case in cases(&vectors(), "streams") {
        let name = &case["name"];
        let frames = decode(&hex(case["stream"].as_str().unwrap()));
        let payloads: Vec<_> = frames.iter().filter_map(|frame| frame.clone().ok()).collect();
        let expected: Vec<_> =
            case["payloads"].as_array().unwrap().iter().map(|p| hex(p.as_str().unwrap())).collect();
        assert_eq!(payloads, expected, "{name}");
        let dropped = |error| frames.iter().filter(|&frame| *frame == Err(error)).count();
        assert_eq!(dropped(FrameError::Corrupt), case["crc_failures"], "{name}");
        assert_eq!(dropped(FrameError::Oversized), case["oversized_frames"], "{name}");
    }
}

/// Every frame `bytes` holds, as a payload or the reason it was dropped.
fn decode(bytes: &[u8]) -> Vec<Result<Vec<u8>, FrameError>> {
    let mut decoder = FrameDecoder::new();
    let mut frames = Vec::new();
    for &byte in bytes {
        if let Some(frame) = decoder.push(byte) {
            frames.push(frame.map(<[u8]>::to_vec));
        }
    }
    frames
}
