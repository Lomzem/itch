use crate::board::{Board, StorageFault};
use crate::frame::MAX_PAYLOAD;
use crate::label::Label;
use crate::lease::{self, Lease};
use crate::wire::{
    CHANNEL_COUNT, DecodeError, ErrorCode, HelloReply, LogLevel, Message, PROTOCOL, Reply, Request,
    RequestType, Response, StatusReply,
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
    /// Set the calling connection's log level to `level`, then send the
    /// first `len` bytes of the reply buffer.
    SetLogLevel { len: usize, level: LogLevel },
}

/// The Rig's protocol logic. It does no I/O: the transport feeds it decoded
/// payloads and sends back what it writes.
pub struct Rig {
    lease: Lease,
    /// The label `Hello` reports.
    label: Label,
    /// The commanded Channel state, bit 0 = CH1.
    channels: u16,
    /// Payloads dropped for being under 5 bytes.
    malformed_frames: u32,
    /// Times the expander was found faulty and re-initialised.
    expander_faults: u32,
    /// The expander didn't come back up, so it is held in reset with every
    /// Channel off.
    held: bool,
}

impl Rig {
    /// Boots the Rig on `board`, loading the saved label.
    pub fn new<B: Board>(board: &mut B) -> Self {
        let label = board
            .load_label()
            .filter(|label| !label.as_str().is_empty())
            .unwrap_or_else(|| Label::default_for(board.info().hardware_id));
        Self {
            lease: Lease::default(),
            label,
            channels: 0,
            malformed_frames: 0,
            expander_faults: 0,
            held: false,
        }
    }

    /// Answers one request payload, writing the reply payload into `reply`.
    pub fn handle<B: Board>(
        &mut self,
        request: &[u8],
        board: &mut B,
        reply: &mut [u8; MAX_PAYLOAD],
    ) -> Handled {
        let Some((&request_type, rest)) = request.split_first() else {
            return self.drop_malformed();
        };
        let Some((request_id, body)) = rest.split_first_chunk() else {
            return self.drop_malformed();
        };
        let request_id = u32::from_le_bytes(*request_id);
        let decoded = RequestType::from_u8(request_type)
            .ok_or(DecodeError::UnknownType)
            .and_then(|kind| Request::decode(kind, body));
        let log_level = match decoded {
            Ok(Request::SetLogLevel { level }) => Some(level),
            _ => None,
        };
        let result = match decoded {
            Ok(request) => self.apply(request, board),
            Err(error) => {
                let error = match error {
                    DecodeError::Malformed => ErrorCode::Malformed,
                    DecodeError::UnknownType => ErrorCode::UnknownRequest,
                };
                log::warn!(
                    "answered request {request_id} (type {request_type:#04x}) with {error:?}"
                );
                Err(error)
            }
        };
        let settle = matches!(result, Ok(Reply::WriteChannels { .. } | Reply::Clear { .. }));
        let response = match result {
            Ok(reply) => Response::Ok(reply),
            Err(error) => Response::Err { request_type, error, detail: self.detail(error) },
        };
        let len = Message::Response { request_id, response }.encode(reply);
        match log_level {
            Some(level) => Handled::SetLogLevel { len, level },
            None => Handled::Reply { len, settle },
        }
    }

    /// The idle tick, every ~50 ms. Re-initialises the expander if it no
    /// longer drives the commanded Channels, and retries one held in reset.
    pub fn tick<B: Board>(&mut self, board: &mut B) {
        if !self.held {
            if board.read_outputs() == Ok(self.channels) {
                return;
            }
            self.count_fault();
        }
        let _ = self.recover(board, self.channels);
    }

    /// The `detail` of an error reply: who holds the Rig, for the Lease errors.
    fn detail(&self, error: ErrorCode) -> &str {
        match error {
            ErrorCode::LeaseHeld | ErrorCode::LeaseLost => self.lease.holder(),
            _ => "",
        }
    }

    fn drop_malformed(&mut self) -> Handled {
        self.malformed_frames = self.malformed_frames.wrapping_add(1);
        Handled::NoReply
    }

    fn apply<B: Board>(&mut self, request: Request, board: &mut B) -> Result<Reply<'_>, ErrorCode> {
        let now = board.now();
        self.lease.expire(now);
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
                    label: self.label.as_str(),
                })
            }
            // The transport applies the level; see `Handled::SetLogLevel`.
            Request::SetLogLevel { .. } => Reply::SetLogLevel,
            Request::GetChannels => {
                Reply::GetChannels { state: if self.held { 0 } else { self.channels } }
            }
            Request::GetStatus => {
                let info = board.info();
                let counters = board.counters();
                Reply::GetStatus(StatusReply {
                    uptime_ms: now,
                    reset_reason: info.reset_reason,
                    last_panic: info.last_panic,
                    holder: self.lease.holder(),
                    crc_failures: counters.crc_failures,
                    oversized_frames: counters.oversized_frames,
                    malformed_frames: self.malformed_frames,
                    dropped_logs: counters.dropped_logs,
                    expander_faults: self.expander_faults,
                    usb_tx_dropped: counters.usb_tx_dropped,
                })
            }
            Request::WriteChannels(write) => {
                self.lease.hold(write.token, None, now)?;
                let state = self.channels & !write.mask | write.values & write.mask;
                Reply::WriteChannels { state: self.write(board, state)? }
            }
            Request::Clear { token } => {
                self.lease.hold(token, None, now)?;
                Reply::Clear { state: self.write(board, 0)? }
            }
            Request::SetLabel(set) => {
                self.lease.hold(set.token, None, now)?;
                self.set_label(board, set.label)?;
                Reply::SetLabel { label: self.label.as_str() }
            }
            Request::AcquireLease(acquire) => {
                self.lease.acquire(acquire.token, acquire.takeover, acquire.holder, now)?;
                Reply::AcquireLease { ttl_ms: lease::TTL_MS }
            }
            Request::RenewLease(renew) => {
                self.lease.hold(renew.token, Some(renew.holder), now)?;
                Reply::RenewLease
            }
            Request::ReleaseLease { token } => {
                self.lease.release(token);
                Reply::ReleaseLease
            }
        })
    }

    /// Saves `label` and reports it from now on. Empty, or the default by
    /// name, restores the default, which is saved as empty.
    fn set_label<B: Board>(&mut self, board: &mut B, label: &str) -> Result<(), ErrorCode> {
        let default = Label::default_for(board.info().hardware_id);
        let custom = if label == default.as_str() { "" } else { label };
        board.save_label(custom).map_err(|StorageFault| ErrorCode::StorageFault)?;
        self.label = match custom {
            "" => default,
            custom => Label::new(custom).expect("decoding caps labels at 64 bytes"),
        };
        Ok(())
    }

    /// Drives the Channels to `state` and makes it the commanded state. A
    /// failed write re-initialises the expander and tries again.
    fn write<B: Board>(&mut self, board: &mut B, state: u16) -> Result<u16, ErrorCode> {
        if self.held {
            return Err(ErrorCode::HardwareFault);
        }
        if board.write_outputs(state).is_err() {
            self.count_fault();
            self.recover(board, state)?;
        }
        self.channels = state;
        Ok(state)
    }

    fn count_fault(&mut self) {
        self.expander_faults = self.expander_faults.wrapping_add(1);
        log::warn!("the expander faulted; re-initialising it");
    }

    /// Re-initialises the expander and drives `state`. If that fails, the
    /// expander is held in reset until a later tick brings it back.
    fn recover<B: Board>(&mut self, board: &mut B, state: u16) -> Result<(), ErrorCode> {
        let recovered = board.reinit().and_then(|()| board.write_outputs(state)).is_ok();
        if recovered && self.held {
            log::info!("the expander is back; Channels restored");
        } else if !recovered && !self.held {
            log::error!("the expander didn't come back; holding every Channel off");
        }
        self.held = !recovered;
        if recovered { Ok(()) } else { Err(ErrorCode::HardwareFault) }
    }
}
