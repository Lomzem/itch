//! The payload codec: the message envelope and every message body, as laid
//! out in `docs/protocol/`. Decoding is strict: exact sizes, no trailing
//! bytes, valid bools, enums and UTF-8.

mod codec;
mod hello;

use codec::{Reader, Writer};
pub use hello::{FEATURE_WIFI, HelloReply};

/// Bumped on any message change. Clients require an exact match.
pub const PROTOCOL: u16 = 1;

pub const CHANNEL_COUNT: u8 = 16;

/// The longest Label or Holder text, in bytes.
pub const MAX_LABEL: usize = 64;

const RESPONSE_BIT: u8 = 0x80;
const FIRST_EVENT_TYPE: u8 = 0x40;
const STATUS_OK: u8 = 0;

/// The `type` byte of a request; its response's is `type | 0x80`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RequestType {
    Hello = 0x01,
}

impl RequestType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Self::Hello),
            _ => None,
        }
    }
}

/// A non-zero response `status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ErrorCode {
    UnknownRequest = 1,
    UnsupportedVersion = 2,
    Malformed = 3,
    LeaseRequired = 4,
    LeaseHeld = 5,
    LeaseLost = 6,
    Busy = 7,
    HardwareFault = 8,
    StorageFault = 9,
    Internal = 10,
}

impl ErrorCode {
    pub fn from_u8(value: u8) -> Option<Self> {
        use ErrorCode::*;
        Some(match value {
            1 => UnknownRequest,
            2 => UnsupportedVersion,
            3 => Malformed,
            4 => LeaseRequired,
            5 => LeaseHeld,
            6 => LeaseLost,
            7 => Busy,
            8 => HardwareFault,
            9 => StorageFault,
            10 => Internal,
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
    Request { request_id: u32, request: Request },
    Response { request_id: u32, response: Response<'a> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Hello,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Response<'a> {
    Ok(Reply<'a>),
    /// `request_type` is the raw type byte, so an unknown request can be echoed.
    Err { request_type: u8, error: ErrorCode, detail: &'a str },
}

/// The result body of an ok response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply<'a> {
    Hello(HelloReply<'a>),
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
        }
        w.len()
    }

    pub fn decode(payload: &'a [u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(payload);
        let message_type = r.u8()?;
        if message_type & RESPONSE_BIT == 0 {
            if message_type >= FIRST_EVENT_TYPE {
                return Err(DecodeError::UnknownType);
            }
            let request_id = r.u32()?;
            let request_type = RequestType::from_u8(message_type).ok_or(DecodeError::UnknownType)?;
            let request = Request::decode(request_type, r.rest())?;
            return Ok(Message::Request { request_id, request });
        }
        let request_id = r.u32()?;
        let request_type = message_type & !RESPONSE_BIT;
        let response = match r.u8()? {
            STATUS_OK => {
                let request_type =
                    RequestType::from_u8(request_type).ok_or(DecodeError::UnknownType)?;
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

impl Request {
    pub fn request_type(&self) -> RequestType {
        match self {
            Request::Hello => RequestType::Hello,
        }
    }

    /// Decodes a request body, the bytes after `request_id`.
    pub fn decode(request_type: RequestType, body: &[u8]) -> Result<Self, DecodeError> {
        let r = Reader::new(body);
        let request = match request_type {
            RequestType::Hello => Request::Hello,
        };
        r.finish()?;
        Ok(request)
    }

    fn encode_body(&self, _w: &mut Writer) {
        match self {
            Request::Hello => {}
        }
    }
}

impl<'a> Reply<'a> {
    pub fn request_type(&self) -> RequestType {
        match self {
            Reply::Hello(_) => RequestType::Hello,
        }
    }

    fn decode(request_type: RequestType, r: &mut Reader<'a>) -> Result<Self, DecodeError> {
        Ok(match request_type {
            RequestType::Hello => Reply::Hello(HelloReply::decode(r)?),
        })
    }

    fn encode_body(&self, w: &mut Writer) {
        match self {
            Reply::Hello(hello) => hello.encode(w),
        }
    }
}
