//! Where the transports meet the `rig` task: the request queue every
//! transport feeds, and each transport's [`Mailbox`] for what goes back out.

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
use itch_core::logs::{LogDropped, LogQueue};
use itch_core::rig::Connection;
use itch_core::wire::{ErrorCode, LogLevel, decode_request};

/// How many frames may wait for the `rig` task. A transport answers `Busy`
/// itself when the queue is full.
pub const DEPTH: usize = 4;

/// Room in each mailbox for one full-length Log Event or many short ones.
const LOG_BYTES: usize = 4 * 1024;

/// Frames waiting for the `rig` task, in arrival order.
pub type Requests = Channel<CriticalSectionRawMutex, Queued, DEPTH>;

/// The request queue every transport feeds.
pub static REQUESTS: Requests = Channel::new();

/// A frame waiting for the `rig` task.
pub struct Queued {
    /// Where its reply goes.
    pub from: Origin,
    pub frame: Result<Payload, FrameError>,
}

/// One transport's mailbox. It serves one connection at a time: the
/// connection's frames go to the request queue, and its replies and Log
/// Events wait here for the transport's writer, replies first.
pub struct Mailbox {
    requests: &'static Requests,
    replies: Channel<CriticalSectionRawMutex, Payload, 2>,
    /// Filtered by the connection's log level, which is off between
    /// connections.
    logs: Mutex<CriticalSectionRawMutex, RefCell<LogQueue<LOG_BYTES>>>,
    /// Wakes the writer after a log line is queued.
    logged: Signal<CriticalSectionRawMutex, ()>,
    /// The generation of the connection served, bumped whenever one starts
    /// or ends, so nothing meant for one reaches the next.
    connection: AtomicU32,
}

impl Mailbox {
    pub const fn new(requests: &'static Requests) -> Self {
        Self {
            requests,
            replies: Channel::new(),
            logs: Mutex::new(RefCell::new(LogQueue::new())),
            logged: Signal::new(),
            connection: AtomicU32::new(0),
        }
    }

    /// Starts serving a new connection, with logs off and none of an
    /// earlier one's replies. Returns where its frames come from.
    pub fn connect(&'static self) -> Origin {
        self.disconnect();
        Origin { mailbox: self, connection: self.connection.load(Ordering::Relaxed) }
    }

    /// Stops serving the connection. Its replies are dropped, including one
    /// the `rig` task is waiting to queue.
    pub fn disconnect(&self) {
        self.connection.fetch_add(1, Ordering::Relaxed);
        self.replies.clear();
        self.logs.lock(|logs| logs.borrow_mut().set_level(LogLevel::Off));
    }

    /// Queues one log line for the connection, if its level lets it
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

/// One connection on a mailbox, where its frames come from and its replies
/// go back to.
#[derive(Clone, Copy)]
pub struct Origin {
    mailbox: &'static Mailbox,
    connection: u32,
}

impl Origin {
    /// Queues a frame the transport decoded, payload or error, for the Rig.
    /// A request that finds the queue full is answered `Busy` at once; a
    /// dropped frame that finds it full goes uncounted.
    pub fn receive(self, frame: Result<&[u8], FrameError>) {
        let request = Queued { from: self, frame: frame.map(Payload::new) };
        if let Err(TrySendError::Full(request)) = self.mailbox.requests.try_send(request)
            && let Ok(payload) = request.frame
            && let Some(busy) = busy(payload.as_bytes())
        {
            // With replies backed up too, the client times out and retries.
            let _ = self.mailbox.replies.try_send(busy);
        }
    }

    /// Queues `reply` once there is room, or drops it once the connection
    /// has ended. The writer drains replies within its write timeout, so
    /// this never stalls the `rig` task for long.
    pub async fn reply(self, mut reply: Payload) {
        let replies = &self.mailbox.replies;
        // Checked again after every wait: the connection may end meanwhile.
        while self.is_current() {
            match replies.try_send(reply) {
                Ok(()) => return,
                Err(TrySendError::Full(full)) => reply = full,
            }
            poll_fn(|cx| replies.poll_ready_to_send(cx)).await;
        }
    }

    fn is_current(self) -> bool {
        self.connection == self.mailbox.connection.load(Ordering::Relaxed)
    }
}

impl Connection for Origin {
    fn set_log_level(&self, level: LogLevel) {
        if self.is_current() {
            self.mailbox.logs.lock(|logs| logs.borrow_mut().set_level(level));
        }
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
