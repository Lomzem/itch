//! Routes `log` facade records to every connection, each at its own level.
//! There is no other log output: anything else on USB would corrupt the
//! protocol.

use core::sync::atomic::{AtomicU32, Ordering};

use log::{LevelFilter, Log, Metadata, Record};

use crate::connections::CONNECTIONS;

/// Log Events dropped because a connection's queue was full: the Rig's
/// `dropped_logs` counter.
pub static DROPPED_LOGS: AtomicU32 = AtomicU32::new(0);

static LOGGER: Logger = Logger;

/// Installs the logger. Each connection's log level filters what it
/// receives.
pub fn init() {
    // Only fails if a logger is already set, and nothing else sets one.
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(LevelFilter::Debug);
}

struct Logger;

impl Log for Logger {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let dropped = CONNECTIONS.log(record.level(), *record.args());
        DROPPED_LOGS.fetch_add(dropped, Ordering::Relaxed);
    }

    fn flush(&self) {}
}
