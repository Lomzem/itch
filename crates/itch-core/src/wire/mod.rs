//! The payload codec: the message envelope and every message body, as laid
//! out in `docs/protocol/`. Decoding is strict: exact sizes, no trailing
//! bytes, valid bools, enums and UTF-8.

mod channels;
mod codec;
mod hello;
mod label;
mod lock;
mod log;
mod status;
mod wifi;

pub use channels::WriteChannels;
use codec::{Reader, Writer};
pub use hello::HelloReply;
pub use label::SetLabel;
pub use lock::Lock;
pub(crate) use log::encode_header as encode_log_header;
pub use log::{LogEvent, LogLevel, MAX_LOG_TEXT};
pub use status::StatusReply;
pub use wifi::{Link, SetWifi};

/// Bumped on any message change. Clients require an exact match.
pub const PROTOCOL: u16 = 3;

pub const CHANNEL_COUNT: u8 = 16;

/// The longest Label text, in bytes.
pub const MAX_LABEL: usize = 64;

/// The longest Holder name, in bytes.
pub const MAX_HOLDER: usize = 64;

/// The longest Wi-Fi network name (SSID), in bytes.
pub const MAX_SSID: usize = 32;

/// The shortest and longest Wi-Fi passphrase, in bytes, as WPA2-Personal
/// allows.
pub const MIN_PASSPHRASE: usize = 8;
pub const MAX_PASSPHRASE: usize = 63;

/// The longest `last_panic` text, in bytes.
pub const MAX_LAST_PANIC: usize = 64;

const RESPONSE_BIT: u8 = 0x80;
const FIRST_EVENT_TYPE: u8 = 0x40;
const STATUS_OK: u8 = 0;

/// The `type` byte of a request; its response's is `type | 0x80`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RequestType {
    Hello = 0x01,
    GetChannels = 0x02,
    WriteChannels = 0x03,
    Clear = 0x04,
    GetStatus = 0x05,
    SetLabel = 0x06,
    SetLogLevel = 0x07,
    Lock = 0x0B,
    ReleaseLock = 0x0C,
    SetWifi = 0x0D,
}

impl RequestType {
    pub fn from_u8(value: u8) -> Option<Self> {
        use RequestType::*;
        Some(match value {
            0x01 => Hello,
            0x02 => GetChannels,
            0x03 => WriteChannels,
            0x04 => Clear,
            0x05 => GetStatus,
            0x06 => SetLabel,
            0x07 => SetLogLevel,
            0x0B => Lock,
            0x0C => ReleaseLock,
            0x0D => SetWifi,
            _ => return None,
        })
    }
}

/// A non-zero response `status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ErrorCode {
    UnknownRequest = 1,
    Malformed = 3,
    Busy = 7,
    HardwareFault = 8,
    StorageFault = 9,
    Internal = 10,
    Locked = 11,
}

impl ErrorCode {
    pub fn from_u8(value: u8) -> Option<Self> {
        use ErrorCode::*;
        Some(match value {
            1 => UnknownRequest,
            3 => Malformed,
            7 => Busy,
            8 => HardwareFault,
            9 => StorageFault,
            10 => Internal,
            11 => Locked,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// The payload breaks its byte table. Answered with `Malformed`.
    Malformed,
    /// The `type` byte names no known request. Answered with `UnknownRequest`.
    UnknownType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message<'a> {
    Request { request_id: u32, request: Request<'a> },
    Response { request_id: u32, response: Response<'a> },
    Event(Event<'a>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request<'a> {
    Hello,
    GetChannels,
    WriteChannels(WriteChannels<'a>),
    Clear { holder: &'a str },
    GetStatus,
    SetLabel(SetLabel<'a>),
    SetLogLevel { level: LogLevel },
    Lock(Lock<'a>),
    ReleaseLock { holder: &'a str },
    SetWifi(SetWifi<'a>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Response<'a> {
    Ok(Reply<'a>),
    /// `request_type` is the raw type byte, so an unknown request can be echoed.
    Err {
        request_type: u8,
        error: ErrorCode,
        detail: &'a str,
    },
}

/// The result body of an ok response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply<'a> {
    Hello(HelloReply<'a>),
    /// The commanded state, bit 0 = CH1.
    GetChannels {
        state: u16,
    },
    /// The state after settling.
    WriteChannels {
        state: u16,
    },
    /// The state after settling.
    Clear {
        state: u16,
    },
    GetStatus(StatusReply<'a>),
    /// The label the Rig now reports.
    SetLabel {
        label: &'a str,
    },
    SetLogLevel,
    Lock,
    ReleaseLock,
    SetWifi,
}

/// What a response must echo from its request: read first, so even a
/// request whose body can't be decoded gets an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestHeader {
    /// The raw `type` byte, which may name no known request.
    pub request_type: u8,
    pub request_id: u32,
}

impl RequestHeader {
    /// The ok response carrying `reply`.
    pub fn reply(self, reply: Reply<'_>) -> Message<'_> {
        Message::Response { request_id: self.request_id, response: Response::Ok(reply) }
    }

    /// The response refusing the request with `error` and its `detail`.
    pub fn error(self, error: ErrorCode, detail: &str) -> Message<'_> {
        let response = Response::Err { request_type: self.request_type, error, detail };
        Message::Response { request_id: self.request_id, response }
    }
}

/// Reads a request payload: its header, and the request or why it couldn't
/// be decoded. Any `type` byte that names no request, a response's or an
/// Event's included, is `UnknownType`. `None` when the payload is too short
/// to hold a `request_id`, so it can't be answered.
pub fn decode_request(payload: &[u8]) -> Option<(RequestHeader, Result<Request<'_>, DecodeError>)> {
    let (&request_type, rest) = payload.split_first()?;
    let (request_id, body) = rest.split_first_chunk()?;
    let header = RequestHeader { request_type, request_id: u32::from_le_bytes(*request_id) };
    let request = RequestType::from_u8(request_type)
        .ok_or(DecodeError::UnknownType)
        .and_then(|request_type| Request::decode(request_type, body));
    Some((header, request))
}

/// An unsolicited message from the Rig. Clients ignore Event types they don't know.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event<'a> {
    Log(LogEvent<'a>),
}

impl<'a> Message<'a> {
    /// Encodes into `out` and returns the payload length.
    pub fn encode(&self, out: &mut [u8]) -> usize {
        let mut w = Writer::new(out);
        match self {
            Message::Request { request_id, request } => {
                w.u8(request.request_type() as u8);
                w.u32(*request_id);
                request.encode_body(&mut w);
            }
            Message::Response { request_id, response: Response::Ok(reply) } => {
                w.u8(reply.request_type() as u8 | RESPONSE_BIT);
                w.u32(*request_id);
                w.u8(STATUS_OK);
                reply.encode_body(&mut w);
            }
            Message::Response {
                request_id,
                response: Response::Err { request_type, error, detail },
            } => {
                w.u8(request_type | RESPONSE_BIT);
                w.u32(*request_id);
                w.u8(*error as u8);
                w.text(detail);
            }
            Message::Event(Event::Log(log)) => log.encode(&mut w),
        }
        w.len()
    }

    pub fn decode(payload: &'a [u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(payload);
        let message_type = r.u8()?;
        if message_type & RESPONSE_BIT == 0 {
            if message_type >= FIRST_EVENT_TYPE {
                if message_type != log::LOG_EVENT_TYPE {
                    return Err(DecodeError::UnknownType);
                }
                return Ok(Message::Event(Event::Log(LogEvent::decode(&mut r)?)));
            }
            let (header, request) = decode_request(payload).ok_or(DecodeError::Malformed)?;
            return Ok(Message::Request { request_id: header.request_id, request: request? });
        }
        let request_id = r.u32()?;
        let request_type = message_type & !RESPONSE_BIT;
        let response = match r.u8()? {
            STATUS_OK => {
                let request_type =
                    RequestType::from_u8(request_type).ok_or(DecodeError::Malformed)?;
                Response::Ok(Reply::decode(request_type, &mut r)?)
            }
            status => Response::Err {
                request_type,
                error: ErrorCode::from_u8(status).ok_or(DecodeError::Malformed)?,
                detail: r.text(255)?,
            },
        };
        r.finish()?;
        Ok(Message::Response { request_id, response })
    }
}

impl<'a> Request<'a> {
    pub fn request_type(&self) -> RequestType {
        match self {
            Request::Hello => RequestType::Hello,
            Request::GetChannels => RequestType::GetChannels,
            Request::WriteChannels(_) => RequestType::WriteChannels,
            Request::Clear { .. } => RequestType::Clear,
            Request::GetStatus => RequestType::GetStatus,
            Request::SetLabel(_) => RequestType::SetLabel,
            Request::SetLogLevel { .. } => RequestType::SetLogLevel,
            Request::Lock(_) => RequestType::Lock,
            Request::ReleaseLock { .. } => RequestType::ReleaseLock,
            Request::SetWifi(_) => RequestType::SetWifi,
        }
    }

    /// Decodes a request body, the bytes after `request_id`.
    pub fn decode(request_type: RequestType, body: &'a [u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(body);
        let request = match request_type {
            RequestType::Hello => Request::Hello,
            RequestType::GetChannels => Request::GetChannels,
            RequestType::WriteChannels => Request::WriteChannels(WriteChannels::decode(&mut r)?),
            RequestType::Clear => Request::Clear { holder: r.text(MAX_HOLDER)? },
            RequestType::GetStatus => Request::GetStatus,
            RequestType::SetLabel => Request::SetLabel(SetLabel::decode(&mut r)?),
            RequestType::SetLogLevel => Request::SetLogLevel { level: LogLevel::decode(&mut r)? },
            RequestType::Lock => Request::Lock(Lock::decode(&mut r)?),
            RequestType::ReleaseLock => Request::ReleaseLock { holder: r.text(MAX_HOLDER)? },
            RequestType::SetWifi => Request::SetWifi(SetWifi::decode(&mut r)?),
        };
        r.finish()?;
        Ok(request)
    }

    fn encode_body(&self, w: &mut Writer) {
        match self {
            Request::Hello | Request::GetChannels | Request::GetStatus => {}
            Request::Clear { holder } | Request::ReleaseLock { holder } => w.text(holder),
            Request::WriteChannels(write) => write.encode(w),
            Request::SetLabel(set) => set.encode(w),
            Request::SetLogLevel { level } => w.u8(*level as u8),
            Request::Lock(lock) => lock.encode(w),
            Request::SetWifi(set) => set.encode(w),
        }
    }
}

impl<'a> Reply<'a> {
    pub fn request_type(&self) -> RequestType {
        match self {
            Reply::Hello(_) => RequestType::Hello,
            Reply::GetChannels { .. } => RequestType::GetChannels,
            Reply::WriteChannels { .. } => RequestType::WriteChannels,
            Reply::Clear { .. } => RequestType::Clear,
            Reply::GetStatus(_) => RequestType::GetStatus,
            Reply::SetLabel { .. } => RequestType::SetLabel,
            Reply::SetLogLevel => RequestType::SetLogLevel,
            Reply::Lock => RequestType::Lock,
            Reply::ReleaseLock => RequestType::ReleaseLock,
            Reply::SetWifi => RequestType::SetWifi,
        }
    }

    fn decode(request_type: RequestType, r: &mut Reader<'a>) -> Result<Self, DecodeError> {
        Ok(match request_type {
            RequestType::Hello => Reply::Hello(HelloReply::decode(r)?),
            RequestType::GetChannels => Reply::GetChannels { state: r.u16()? },
            RequestType::WriteChannels => Reply::WriteChannels { state: r.u16()? },
            RequestType::Clear => Reply::Clear { state: r.u16()? },
            RequestType::GetStatus => Reply::GetStatus(StatusReply::decode(r)?),
            RequestType::SetLabel => Reply::SetLabel { label: r.text(MAX_LABEL)? },
            RequestType::SetLogLevel => Reply::SetLogLevel,
            RequestType::Lock => Reply::Lock,
            RequestType::ReleaseLock => Reply::ReleaseLock,
            RequestType::SetWifi => Reply::SetWifi,
        })
    }

    fn encode_body(&self, w: &mut Writer) {
        match self {
            Reply::Hello(hello) => hello.encode(w),
            Reply::GetChannels { state }
            | Reply::WriteChannels { state }
            | Reply::Clear { state } => w.u16(*state),
            Reply::GetStatus(status) => status.encode(w),
            Reply::SetLabel { label } => w.text(label),
            Reply::SetLogLevel | Reply::Lock | Reply::ReleaseLock | Reply::SetWifi => {}
        }
    }
}
