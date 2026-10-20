//! The `rig` task's request queue, fed by every transport.

#[cfg(feature = "firmware")]
use core::cell::RefCell;
#[cfg(feature = "firmware")]
use core::sync::atomic::{AtomicU32, Ordering};

#[cfg(feature = "firmware")]
use embassy_futures::select::select;
#[cfg(feature = "firmware")]
use embassy_sync::blocking_mutex::Mutex;
#[cfg(feature = "firmware")]
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
#[cfg(feature = "firmware")]
use embassy_sync::channel::{Channel, TrySendError};
#[cfg(feature = "firmware")]
use embassy_sync::signal::Signal;
use itch_core::frame::MAX_PAYLOAD;
#[cfg(feature = "firmware")]
use itch_core::frame::{FrameError, MAX_FRAME, encode_frame};
#[cfg(feature = "firmware")]
use itch_core::logs::LogQueue;
use itch_core::wire::{ErrorCode, decode_request};

/// How many requests may wait for the `rig` task. A transport answers
/// `Busy` itself when the queue is full.
pub const DEPTH: usize = 4;

/// Where a request came from, so its reply goes back there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Usb,
    /// The development-only WiFi transport, with the generation of the
    /// connection the request came in on.
    #[cfg(feature = "wifi")]
    Tcp(u32),
}

/// Requests waiting for the `rig` task, in arrival order.
#[cfg(feature = "firmware")]
pub static REQUESTS: Channel<CriticalSectionRawMutex, Queued, DEPTH> = Channel::new();

/// A transport's replies, waiting for its writer.
#[cfg(feature = "firmware")]
pub type Replies = Channel<CriticalSectionRawMutex, Payload, 2>;

/// A transport's Log Events, waiting for its writer.
#[cfg(feature = "firmware")]
pub type Logs<const N: usize> = Mutex<CriticalSectionRawMutex, RefCell<LogQueue<N>>>;

/// Waits for a transport's next outgoing frame, a reply before any Log
/// Event, and encodes it into `frame`. `logged` wakes it after a log line
/// is queued. Returns the frame's length.
#[cfg(feature = "firmware")]
pub async fn next_frame<const N: usize>(
    replies: &Replies,
    logs: &Logs<N>,
    logged: &Signal<CriticalSectionRawMutex, ()>,
    frame: &mut [u8; MAX_FRAME],
) -> usize {
    let mut log = [0; MAX_PAYLOAD];
    loop {
        if let Ok(reply) = replies.try_receive() {
            return encode_frame(reply.as_bytes(), frame);
        }
        if let Some(len) = logs.lock(|logs| logs.borrow_mut().pop(&mut log)) {
            return encode_frame(&log[..len], frame);
        }
        select(replies.ready_to_receive(), logged.wait()).await;
    }
}

/// Frames dropped for a bad CRC, on any transport: the Rig's `crc_failures`
/// counter.
#[cfg(feature = "firmware")]
pub static CRC_FAILURES: AtomicU32 = AtomicU32::new(0);

/// Frames dropped for being oversized, on any transport: `oversized_frames`.
#[cfg(feature = "firmware")]
pub static OVERSIZED_FRAMES: AtomicU32 = AtomicU32::new(0);

/// Queues a frame a transport decoded, or counts why it was dropped. A
/// request that finds the queue full is answered `Busy` at once.
#[cfg(feature = "firmware")]
pub fn receive(transport: Transport, frame: Result<&[u8], FrameError>, replies: &Replies) {
    let payload = match frame {
        Ok(payload) => payload,
        Err(FrameError::Corrupt) => {
            log::warn!("dropped a corrupt frame");
            CRC_FAILURES.fetch_add(1, Ordering::Relaxed);
            return;
        }
        Err(FrameError::Oversized) => {
            log::warn!("dropped a frame over {MAX_PAYLOAD} bytes");
            OVERSIZED_FRAMES.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };
    let request = Queued { transport, payload: Payload::new(payload) };
    if let Err(TrySendError::Full(request)) = REQUESTS.try_send(request)
        && let Some(busy) = busy(request.payload.as_bytes())
    {
        // With replies backed up too, the client times out and retries.
        let _ = replies.try_send(busy);
    }
}

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
    let (header, _) = decode_request(request)?;
    let mut reply = Payload { len: 0, bytes: [0; MAX_PAYLOAD] };
    reply.len = header.error(ErrorCode::Busy, "").encode(&mut reply.bytes);
    Some(reply)
}
