#![allow(dead_code)]

mod fake_board;

#[allow(unused_imports)]
pub use fake_board::FakeBoard;

use std::path::PathBuf;

use std::cell::Cell;

use itch_core::frame::{FrameError, MAX_PAYLOAD};
use itch_core::rig::{Handled, LogControl, Rig};
use itch_core::wire::{ErrorCode, LogLevel, Message, Reply, Request, Response};
use serde_json::Value;

/// A Rig on a `FakeBoard`, kept across requests, with one log control.
pub struct TestRig {
    pub rig: Rig,
    pub board: FakeBoard,
    pub logs: TestLogControl,
    /// Whether the Rig had the last reply wait for the contacts to settle.
    pub settle: bool,
}

/// A log control that only remembers its log level.
pub struct TestLogControl {
    pub log_level: Cell<LogLevel>,
}

impl LogControl for TestLogControl {
    fn set_log_level(&self, level: LogLevel) {
        self.log_level.set(level);
    }
}

impl TestRig {
    pub fn new() -> Self {
        Self::on(FakeBoard::new())
    }

    /// Boots a Rig on `board`.
    pub fn on(mut board: FakeBoard) -> Self {
        let logs = TestLogControl { log_level: Cell::new(LogLevel::Off) };
        Self { rig: Rig::new(&mut board), board, logs, settle: false }
    }

    /// Runs one idle tick.
    pub fn tick(&mut self) {
        self.rig.tick(&mut self.board);
    }

    /// Sends `request` and returns its response.
    pub fn send(&mut self, request: Request) -> Response<'static> {
        let mut payload = [0; MAX_PAYLOAD];
        let len = Message::Request { request_id: 7, request }.encode(&mut payload);
        let reply = self.raw(&payload[..len]).expect("no reply");
        match Message::decode(reply.leak()) {
            Ok(Message::Response { request_id: 7, response }) => response,
            other => panic!("not a response to the request: {other:?}"),
        }
    }

    /// Sends a raw payload and returns the reply payload, if any.
    pub fn raw(&mut self, payload: &[u8]) -> Option<Vec<u8>> {
        self.receive(Ok(payload))
    }

    /// Hands the Rig a frame as the decoder gave it, payload or error, and
    /// returns the reply payload, if any.
    pub fn receive(&mut self, frame: Result<&[u8], FrameError>) -> Option<Vec<u8>> {
        let mut reply = [0; MAX_PAYLOAD];
        match self.rig.receive(frame, &mut self.board, &self.logs, &mut reply) {
            Handled::NoReply => None,
            Handled::Reply { len, settle } => {
                self.settle = settle;
                Some(reply[..len].to_vec())
            }
        }
    }

    /// Sends `request` and returns its ok reply.
    pub fn ok(&mut self, request: Request) -> Reply<'static> {
        match self.send(request) {
            Response::Ok(reply) => reply,
            Response::Err { error, detail, .. } => panic!("refused with {error:?}: {detail}"),
        }
    }

    /// Sends `request` and returns the error it was refused with.
    pub fn refused(&mut self, request: Request) -> ErrorCode {
        self.refusal(request).0
    }

    /// Sends `request` and returns the error and `detail` it was refused with.
    pub fn refusal(&mut self, request: Request) -> (ErrorCode, &'static str) {
        match self.send(request) {
            Response::Ok(reply) => panic!("accepted: {reply:?}"),
            Response::Err { error, detail, .. } => (error, detail),
        }
    }
}

pub fn protocol_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/protocol")
}

pub fn vectors() -> Value {
    let text = std::fs::read_to_string(protocol_dir().join("vectors-v2.json")).unwrap();
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
