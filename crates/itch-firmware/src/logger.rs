//! Routes `log` facade records to each transport's mailbox. There
//! is no other log output: anything else on USB would corrupt the protocol.

use core::sync::atomic::{AtomicU32, Ordering};

use log::{LevelFilter, Log, Metadata, Record};

use crate::mailbox::Mailbox;
#[cfg(feature = "wifi")]
use crate::tcp;
use crate::usb;

/// Log Events dropped because a transport's queue was full: the Rig's
/// `dropped_logs` counter.
pub static DROPPED_LOGS: AtomicU32 = AtomicU32::new(0);

static LOGGER: Logger = Logger;

/// Installs the logger. Each transport's level filters what it receives.
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
        let log = |mailbox: &Mailbox| {
            if mailbox.log(record.level(), *record.args()).is_err() {
                DROPPED_LOGS.fetch_add(1, Ordering::Relaxed);
            }
        };
        log(&usb::MAILBOX);
        #[cfg(feature = "wifi")]
        log(&tcp::MAILBOX);
    }

    fn flush(&self) {}
}
