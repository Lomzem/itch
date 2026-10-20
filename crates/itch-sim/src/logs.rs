//! Routes `log` facade output to every open connection's Log Event queue.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use itch_core::frame::MAX_PAYLOAD;
use itch_core::logs::LogQueue;
use itch_core::rig::Connection;
use itch_core::wire::LogLevel;
use log::{LevelFilter, Log, Metadata, Record};

/// Room for one full-length line or many short ones.
const QUEUE_BYTES: usize = 8 * 1024;

/// Log Events dropped because a connection's queue was full: the Rig's
/// `dropped_logs` counter.
pub static DROPPED_LOGS: AtomicU32 = AtomicU32::new(0);

static LOGGER: Logger = Logger { connections: Mutex::new(Vec::new()) };

/// Installs the logger. Call once, before serving.
pub fn init() {
    log::set_logger(&LOGGER).expect("no other logger");
    log::set_max_level(LevelFilter::Debug);
}

struct Logger {
    connections: Mutex<Vec<Arc<ConnectionLogs>>>,
}

impl Log for Logger {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        for connection in self.connections.lock().unwrap().iter() {
            let mut state = connection.state.lock().unwrap();
            if state.queue.push(record.level(), *record.args()).is_err() {
                DROPPED_LOGS.fetch_add(1, Ordering::Relaxed);
            }
            connection.ready.notify_one();
        }
    }

    fn flush(&self) {}
}

/// One connection's Log Events, from the time it opens until it closes.
pub struct ConnectionLogs {
    state: Mutex<State>,
    ready: Condvar,
}

struct State {
    queue: LogQueue<QUEUE_BYTES>,
    closed: bool,
}

impl ConnectionLogs {
    /// Starts collecting logs for a new connection, with logs off.
    pub fn open() -> Arc<Self> {
        let connection = Arc::new(Self {
            state: Mutex::new(State { queue: LogQueue::new(), closed: false }),
            ready: Condvar::new(),
        });
        LOGGER.connections.lock().unwrap().push(Arc::clone(&connection));
        connection
    }

    /// Waits for the next Log Event and writes its payload into `out`.
    /// Returns `None` once the connection is closed.
    pub fn next_event(&self, out: &mut [u8; MAX_PAYLOAD]) -> Option<usize> {
        let mut state = self.state.lock().unwrap();
        loop {
            if state.closed {
                return None;
            }
            if let Some(len) = state.queue.pop(out) {
                return Some(len);
            }
            state = self.ready.wait(state).unwrap();
        }
    }

    /// Stops collecting logs and wakes [`ConnectionLogs::next_event`].
    pub fn close(self: &Arc<Self>) {
        LOGGER.connections.lock().unwrap().retain(|other| !Arc::ptr_eq(other, self));
        self.state.lock().unwrap().closed = true;
        self.ready.notify_one();
    }
}

impl Connection for ConnectionLogs {
    fn set_log_level(&self, level: LogLevel) {
        self.state.lock().unwrap().queue.set_level(level);
    }
}
