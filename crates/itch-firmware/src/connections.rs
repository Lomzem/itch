//! Where the Transports meet the Rig task: one request queue every
//! connection feeds, and each connection's own replies and Log Events.

use core::cell::RefCell;
use core::fmt;
use core::future::poll_fn;
use core::sync::atomic::{AtomicU32, Ordering};

use embassy_futures::select::select;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, TrySendError};
use embassy_sync::signal::Signal;
use itch_core::frame::{FrameError, MAX_FRAME, MAX_PAYLOAD, encode_frame};
use itch_core::logs::LogQueue;
use itch_core::rig::LogControl;
use itch_core::wire::{ErrorCode, LogLevel, decode_request};

/// How many frames may wait for the Rig task, from every connection
/// together. A request that finds the queue full gets `Busy` at once.
pub const DEPTH: usize = 4;

/// Room in each connection's log queue for one full-length Log Event or
/// many short ones.
const LOG_BYTES: usize = 4 * 1024;

/// The Rig's connections: USB alone, for now.
#[cfg(feature = "firmware")]
pub static CONNECTIONS: Connections<1> = Connections::new();

/// USB's slot. It serves one connection for the life of the boot.
pub const USB: usize = 0;

/// The connections the Rig serves: `N` slots, each serving one connection
/// at a time, and the request queue they share.
pub struct Connections<const N: usize> {
    requests: Channel<CriticalSectionRawMutex, Queued, DEPTH>,
    slots: [Slot; N],
}

impl<const N: usize> Connections<N> {
    pub const fn new() -> Self {
        Self { requests: Channel::new(), slots: [const { Slot::new() }; N] }
    }

    /// Starts serving a new connection in `slot`, with logs off and nothing
    /// queued for an earlier one.
    ///
    /// # Panics
    ///
    /// If `slot` isn't below `N`.
    pub fn connect(&'static self, slot: usize) -> Connection {
        let slot = &self.slots[slot];
        slot.reset();
        Connection {
            requests: &self.requests,
            slot,
            generation: slot.generation.load(Ordering::Relaxed),
        }
    }

    /// Queues one log line for every connection whose level lets it
    /// through. Returns how many connections had to drop it, for
    /// `dropped_logs`.
    pub fn log(&self, level: log::Level, args: fmt::Arguments) -> u32 {
        let mut dropped = 0;
        for slot in &self.slots {
            if slot.logs.lock(|logs| logs.borrow_mut().push(level, args)).is_err() {
                dropped += 1;
            }
            slot.logged.signal(());
        }
        dropped
    }

    /// Waits for the next frame from any connection, in arrival order.
    pub(crate) async fn next_request(&self) -> Queued {
        self.requests.receive().await
    }
}

impl<const N: usize> Default for Connections<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// A frame waiting for the Rig task.
pub(crate) struct Queued {
    /// Where its reply goes.
    pub from: Connection,
    pub frame: Result<Payload, FrameError>,
}

/// One connection's outgoing side: replies and Log Events waiting for its
/// Transport's writer, replies first.
struct Slot {
    replies: Channel<CriticalSectionRawMutex, Payload, 2>,
    /// Filtered by the connection's log level, initially off.
    logs: Mutex<CriticalSectionRawMutex, RefCell<LogQueue<LOG_BYTES>>>,
    /// Wakes the writer after a log line is queued.
    logged: Signal<CriticalSectionRawMutex, ()>,
    /// The connection served, bumped whenever one starts or ends, so
    /// nothing meant for one reaches the next.
    generation: AtomicU32,
}

impl Slot {
    const fn new() -> Self {
        Self {
            replies: Channel::new(),
            logs: Mutex::new(RefCell::new(LogQueue::new())),
            logged: Signal::new(),
            generation: AtomicU32::new(0),
        }
    }

    /// Ends the connection served: drops its replies, including one the
    /// Rig task is waiting to queue, and its Log Events, and turns logs off.
    fn reset(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.replies.clear();
        self.logs.lock(|logs| logs.borrow_mut().set_level(LogLevel::Off));
    }
}

/// One connection: where its frames come from and its replies go back to.
/// Its Transport reads with [`receive`](Self::receive) and writes what
/// [`next_frame`](Self::next_frame) returns.
#[derive(Clone, Copy)]
pub struct Connection {
    requests: &'static Channel<CriticalSectionRawMutex, Queued, DEPTH>,
    slot: &'static Slot,
    generation: u32,
}

impl Connection {
    /// Queues a frame the Transport decoded, payload or error, for the Rig,
    /// which counts the errors. A request that finds the queue full is
    /// answered `Busy` at once. An error takes a place in the queue too, and
    /// one that finds it full goes uncounted: corrupt frames come singly, as
    /// from a client joining mid-stream, so neither costs a request (ADR
    /// 0004).
    pub fn receive(self, frame: Result<&[u8], FrameError>) {
        let request = Queued { from: self, frame: frame.map(Payload::new) };
        if let Err(TrySendError::Full(request)) = self.requests.try_send(request)
            && let Ok(payload) = request.frame
            && let Some(busy) = busy(payload.as_bytes())
        {
            // With replies backed up too, the client times out and retries.
            let _ = self.slot.replies.try_send(busy);
        }
    }

    /// Queues `reply` once there is room, or drops it once the connection
    /// has ended. The Rig task bounds its wait, so a writer that stalls
    /// can't hold it up.
    pub(crate) async fn reply(self, mut reply: Payload) {
        let replies = &self.slot.replies;
        // Checked again after every wait: the connection may end meanwhile.
        while self.is_current() {
            match replies.try_send(reply) {
                Ok(()) => return,
                Err(TrySendError::Full(full)) => reply = full,
            }
            poll_fn(|cx| replies.poll_ready_to_send(cx)).await;
        }
    }

    /// Waits for the connection's next outgoing frame, a reply before any
    /// Log Event, and encodes it into `frame`. Returns the frame's length.
    pub async fn next_frame(&self, frame: &mut [u8; MAX_FRAME]) -> usize {
        let slot = self.slot;
        let mut log = [0; MAX_PAYLOAD];
        loop {
            if let Ok(reply) = slot.replies.try_receive() {
                return encode_frame(reply.as_bytes(), frame);
            }
            if let Some(len) = slot.logs.lock(|logs| logs.borrow_mut().pop(&mut log)) {
                return encode_frame(&log[..len], frame);
            }
            select(slot.replies.ready_to_receive(), slot.logged.wait()).await;
        }
    }

    /// Ends the connection: drops its replies, including one the Rig task
    /// is waiting to queue, and its Log Events. Requests it already queued
    /// still run, but their replies go nowhere.
    pub fn disconnect(self) {
        if self.is_current() {
            self.slot.reset();
        }
    }

    fn is_current(self) -> bool {
        self.generation == self.slot.generation.load(Ordering::Relaxed)
    }
}

impl LogControl for Connection {
    fn set_log_level(&self, level: LogLevel) {
        if self.is_current() {
            self.slot.logs.lock(|logs| logs.borrow_mut().set_level(level));
        }
    }
}

/// One frame's payload, owned so it can wait in a queue.
pub(crate) struct Payload {
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
