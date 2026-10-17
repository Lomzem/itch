//! The `rig` task's request queue, fed by every transport.

use itch_core::frame::MAX_PAYLOAD;
use itch_core::wire::{ErrorCode, Message, Response};

/// How many requests may wait for the `rig` task. A transport answers
/// `Busy` itself when the queue is full.
pub const DEPTH: usize = 4;

/// Where a request came from, so its reply goes back there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Usb,
}

/// Requests waiting for the `rig` task, in arrival order.
#[cfg(feature = "firmware")]
pub static REQUESTS: embassy_sync::channel::Channel<
    embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
    Queued,
    DEPTH,
> = embassy_sync::channel::Channel::new();

/// A request waiting for the `rig` task.
pub struct Queued {
    pub transport: Transport,
    pub payload: Payload,
}

/// One frame's payload, owned so it can wait in a queue.
pub struct Payload {
    len: usize,
    bytes: [u8; MAX_PAYLOAD],
}

impl Payload {
    /// # Panics
    ///
    /// If `bytes` is longer than [`MAX_PAYLOAD`].
    pub fn new(bytes: &[u8]) -> Self {
        let mut payload = Self { len: bytes.len(), bytes: [0; MAX_PAYLOAD] };
        payload.bytes[..bytes.len()].copy_from_slice(bytes);
        payload
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// The `Busy` reply to `request`, or `None` if it has no readable
/// `request_id`.
pub fn busy(request: &[u8]) -> Option<Payload> {
    let (&request_type, rest) = request.split_first()?;
    let request_id = u32::from_le_bytes(*rest.first_chunk()?);
    let response = Response::Err { request_type, error: ErrorCode::Busy, detail: "" };
    let mut reply = Payload { len: 0, bytes: [0; MAX_PAYLOAD] };
    reply.len = Message::Response { request_id, response }.encode(&mut reply.bytes);
    Some(reply)
}
