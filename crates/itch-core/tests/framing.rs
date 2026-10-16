mod common;

use common::{cases, hex, vectors};
use itch_core::frame::{FrameDecoder, MAX_FRAME, encode_frame};

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
        let payloads = feed(&mut FrameDecoder::new(), &hex(case["frame"].as_str().unwrap()));
        assert_eq!(payloads, [hex(case["payload"].as_str().unwrap())], "{}", case["name"]);
    }
}

#[test]
fn decoder_resyncs_drops_bad_frames_and_counts_them() {
    for case in cases(&vectors(), "streams") {
        let name = &case["name"];
        let mut decoder = FrameDecoder::new();
        let payloads = feed(&mut decoder, &hex(case["stream"].as_str().unwrap()));
        let expected: Vec<_> =
            case["payloads"].as_array().unwrap().iter().map(|p| hex(p.as_str().unwrap())).collect();
        assert_eq!(payloads, expected, "{name}");
        let counters = decoder.counters();
        assert_eq!(counters.crc_failures, case["crc_failures"], "{name}");
        assert_eq!(counters.oversized_frames, case["oversized_frames"], "{name}");
    }
}

fn feed(decoder: &mut FrameDecoder, bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut payloads = Vec::new();
    for &byte in bytes {
        if let Some(Ok(payload)) = decoder.push(byte) {
            payloads.push(payload.to_vec());
        }
    }
    payloads
}
