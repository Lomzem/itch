use crate::board::Board;
use crate::frame::MAX_PAYLOAD;
use crate::label::Label;
use crate::lease::{self, Lease};
use crate::wire::{
    CHANNEL_COUNT, DecodeError, ErrorCode, HelloReply, Message, PROTOCOL, Reply, Request,
    RequestType, Response,
};

/// How long the transport waits after a Channel write before replying, so
/// the contacts have settled. An initial value, pending bench measurement.
pub const SETTLE_MS: u64 = 20;

/// What the transport does after [`Rig::handle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handled {
    /// Send nothing: the payload had no readable `request_id`.
    NoReply,
    /// Send the first `len` bytes of the reply buffer, after waiting
    /// [`SETTLE_MS`] if `settle` is set.
    Reply { len: usize, settle: bool },
}

/// The Rig's protocol logic. It does no I/O: the transport feeds it decoded
/// payloads and sends back what it writes.
#[derive(Default)]
pub struct Rig {
    lease: Lease,
    /// The commanded Channel state, bit 0 = CH1.
    channels: u16,
}

impl Rig {
    pub fn new() -> Self {
        Self::default()
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
        let label = Label::default_for(board.info().hardware_id);
        let result = match decoded {
            Ok(request) => self.apply(request, board, &label),
            Err(DecodeError::Malformed) => Err(ErrorCode::Malformed),
            Err(DecodeError::UnknownType) => Err(ErrorCode::UnknownRequest),
        };
        let settle = matches!(result, Ok(Reply::WriteChannels { .. } | Reply::Clear { .. }));
        let response = match result {
            Ok(reply) => Response::Ok(reply),
            Err(error) => Response::Err { request_type, error, detail: "" },
        };
        let len = Message::Response { request_id, response }.encode(reply);
        Handled::Reply { len, settle }
    }

    fn apply<'a, B: Board>(
        &mut self,
        request: Request,
        board: &mut B,
        label: &'a Label,
    ) -> Result<Reply<'a>, ErrorCode> {
        Ok(match request {
            Request::Hello => {
                let info = board.info();
                Reply::Hello(HelloReply {
                    protocol: PROTOCOL,
                    firmware_version: info.firmware_version,
                    dirty: info.dirty,
                    features: info.features,
                    channel_count: CHANNEL_COUNT,
                    board_revision: info.board_revision,
                    hardware_id: info.hardware_id,
                    label: label.as_str(),
                })
            }
            Request::GetChannels => Reply::GetChannels { state: self.channels },
            Request::WriteChannels(write) => {
                self.lease.hold(write.token)?;
                let state = self.channels & !write.mask | write.values & write.mask;
                Reply::WriteChannels { state: self.write(board, state) }
            }
            Request::Clear { token } => {
                self.lease.hold(token)?;
                Reply::Clear { state: self.write(board, 0) }
            }
            Request::AcquireLease(acquire) => {
                self.lease.acquire(acquire.token)?;
                Reply::AcquireLease { ttl_ms: lease::TTL_MS }
            }
            Request::RenewLease(renew) => {
                self.lease.hold(renew.token)?;
                Reply::RenewLease
            }
            Request::ReleaseLease { token } => {
                self.lease.release(token);
                Reply::ReleaseLease
            }
        })
    }

    fn write<B: Board>(&mut self, board: &mut B, state: u16) -> u16 {
        board.write_outputs(state);
        self.channels = state;
        state
    }
}
