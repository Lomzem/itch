use crate::board::Board;
use crate::frame::MAX_PAYLOAD;
use crate::label::Label;
use crate::wire::{
    CHANNEL_COUNT, DecodeError, ErrorCode, HelloReply, LogLevel, Message, PROTOCOL, Reply, Request,
    RequestType, Response,
};

/// What the transport does after [`Rig::handle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handled {
    /// Send nothing: the payload had no readable `request_id`.
    NoReply,
    /// Send the first `len` bytes of the reply buffer, after waiting out relay
    /// settling if `settle` is set.
    Reply { len: usize, settle: bool },
    /// Set the calling connection's log level to `level`, then send the
    /// first `len` bytes of the reply buffer.
    SetLogLevel { len: usize, level: LogLevel },
}

/// The Rig's protocol logic. It does no I/O: the transport feeds it decoded
/// payloads and sends back what it writes.
#[derive(Default)]
pub struct Rig {}

impl Rig {
    pub fn new() -> Self {
        Self {}
    }

    /// Answers one request payload, writing the reply payload into `reply`.
    pub fn handle<B: Board>(
        &mut self,
        request: &[u8],
        board: &mut B,
        reply: &mut [u8; MAX_PAYLOAD],
    ) -> Handled {
        let Some((&request_type, rest)) = request.split_first() else {
            return Handled::NoReply;
        };
        let Some((request_id, body)) = rest.split_first_chunk() else {
            return Handled::NoReply;
        };
        let request_id = u32::from_le_bytes(*request_id);
        let decoded = RequestType::from_u8(request_type)
            .ok_or(DecodeError::UnknownType)
            .and_then(|kind| Request::decode(kind, body));
        let len = match decoded {
            Ok(Request::Hello) => {
                let info = board.info();
                let label = Label::default_for(info.hardware_id);
                let hello = HelloReply {
                    protocol: PROTOCOL,
                    firmware_version: info.firmware_version,
                    dirty: info.dirty,
                    features: info.features,
                    channel_count: CHANNEL_COUNT,
                    board_revision: info.board_revision,
                    hardware_id: info.hardware_id,
                    label: label.as_str(),
                };
                let response = Response::Ok(Reply::Hello(hello));
                Message::Response { request_id, response }.encode(reply)
            }
            Ok(Request::SetLogLevel { level }) => {
                let response = Response::Ok(Reply::SetLogLevel);
                let len = Message::Response { request_id, response }.encode(reply);
                return Handled::SetLogLevel { len, level };
            }
            Err(error) => {
                let error = match error {
                    DecodeError::Malformed => ErrorCode::Malformed,
                    DecodeError::UnknownType => ErrorCode::UnknownRequest,
                };
                log::warn!(
                    "answered request {request_id} (type {request_type:#04x}) with {error:?}"
                );
                let response = Response::Err { request_type, error, detail: "" };
                Message::Response { request_id, response }.encode(reply)
            }
        };
        Handled::Reply { len, settle: false }
    }
}
