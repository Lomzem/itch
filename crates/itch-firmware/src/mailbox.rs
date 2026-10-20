//! The USB exchange with the `rig` task: bounded requests, replies and
//! Log Events, with replies sent first.

use core::cell::RefCell;
use core::fmt;

use embassy_futures::select::select;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, TrySendError};
use embassy_sync::signal::Signal;
use itch_core::frame::{FrameError, MAX_FRAME, MAX_PAYLOAD, encode_frame};
use itch_core::logs::{LogDropped, LogQueue};
use itch_core::rig::LogControl;
use itch_core::wire::{ErrorCode, LogLevel, decode_request};

/// How many frames may wait for the `rig` task. Additional requests get
/// `Busy` without waiting for the task.
const DEPTH: usize = 4;

/// Room for one full-length Log Event or many short ones.
const LOG_BYTES: usize = 4 * 1024;

/// Requests waiting for the Rig and replies and Log Events waiting for
/// the USB writer. Log filtering lasts until changed or the Rig reboots.
pub struct Mailbox {
    requests: Channel<CriticalSectionRawMutex, Result<Payload, FrameError>, DEPTH>,
    replies: Channel<CriticalSectionRawMutex, Payload, 2>,
    /// Filtered by the selected log level, initially off.
    logs: Mutex<CriticalSectionRawMutex, RefCell<LogQueue<LOG_BYTES>>>,
    /// Wakes the writer after a log line is queued.
    logged: Signal<CriticalSectionRawMutex, ()>,
}

impl Mailbox {
    pub const fn new() -> Self {
        Self {
            requests: Channel::new(),
            replies: Channel::new(),
            logs: Mutex::new(RefCell::new(LogQueue::new())),
            logged: Signal::new(),
        }
    }

    /// Queues a decoded frame for the Rig, payload or error. A request
    /// that finds the queue full gets `Busy` at once. An error takes a
    /// place too, and one that finds it full goes uncounted (ADR 0004).
    pub fn submit(&self, frame: Result<&[u8], FrameError>) {
        if let Err(TrySendError::Full(Ok(payload))) =
            self.requests.try_send(frame.map(Payload::new))
            && let Some(busy) = busy(payload.as_bytes())
        {
            // With replies backed up too, the client times out and retries.
            let _ = self.replies.try_send(busy);
        }
    }

    /// Waits for the next decoded frame, in arrival order.
    pub async fn next_request(&self) -> Result<Payload, FrameError> {
        self.requests.receive().await
    }

    /// Queues a reply once there is room. The Rig task bounds its wait
    /// so a host flooding requests cannot stall hardware supervision.
    pub async fn reply(&self, reply: Payload) {
        self.replies.send(reply).await;
    }

    /// Queues one log line if the selected level lets it
    /// through. `LogDropped` when the queue is full: count it.
    pub fn log(&self, level: log::Level, args: fmt::Arguments) -> Result<(), LogDropped> {
        let pushed = self.logs.lock(|logs| logs.borrow_mut().push(level, args));
        self.logged.signal(());
        pushed
    }

    /// Waits for the next outgoing frame, a reply before any Log Event, and
    /// encodes it into `frame`. Returns the frame's length.
    pub async fn next_frame(&self, frame: &mut [u8; MAX_FRAME]) -> usize {
        let mut log = [0; MAX_PAYLOAD];
        loop {
            if let Ok(reply) = self.replies.try_receive() {
                return encode_frame(reply.as_bytes(), frame);
            }
            if let Some(len) = self.logs.lock(|logs| logs.borrow_mut().pop(&mut log)) {
                return encode_frame(&log[..len], frame);
            }
            select(self.replies.ready_to_receive(), self.logged.wait()).await;
        }
    }
}

impl Default for Mailbox {
    fn default() -> Self {
        Self::new()
    }
}

impl LogControl for Mailbox {
    fn set_log_level(&self, level: LogLevel) {
        self.logs.lock(|logs| logs.borrow_mut().set_level(level));
    }
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
fn busy(request: &[u8]) -> Option<Payload> {
    let (header, _) = decode_request(request)?;
    let mut reply = Payload { len: 0, bytes: [0; MAX_PAYLOAD] };
    reply.len = header.error(ErrorCode::Busy, "").encode(&mut reply.bytes);
    Some(reply)
}
