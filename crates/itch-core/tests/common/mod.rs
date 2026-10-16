#![allow(dead_code)]

mod fake_board;

#[allow(unused_imports)]
pub use fake_board::FakeBoard;

use std::path::PathBuf;

use serde_json::Value;

pub fn protocol_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/protocol")
}

pub fn vectors() -> Value {
    let text = std::fs::read_to_string(protocol_dir().join("vectors-v1.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

pub fn cases<'a>(vectors: &'a Value, section: &str) -> &'a [Value] {
    vectors[section].as_array().unwrap()
}

/// Parses a vector hex string: case-insensitive, whitespace ignored.
pub fn hex(text: &str) -> Vec<u8> {
    let digits: String = text.split_whitespace().collect();
    (0..digits.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&digits[i..i + 2], 16).unwrap())
        .collect()
}
