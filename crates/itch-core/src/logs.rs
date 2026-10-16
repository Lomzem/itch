//! A connection's bounded queue of Log Events, filtered by that connection's
//! log level.
//!
//! The queue does no locking. Each adapter wraps one queue per connection in
//! its own lock and fans `log` facade records out to them: the firmware keeps
//! one static queue per transport, the simulator one per TCP connection. A
//! writer drains the queue with [`LogQueue::pop`], after any pending replies.

use core::fmt::{self, Write as _};

use crate::frame::MAX_PAYLOAD;
use crate::wire::{LogLevel, MAX_LOG_TEXT, encode_log_header};

/// Bytes stored before each line: its level and its `u16` length.
const LINE_HEADER: usize = 3;

/// The queue was full, so the line was dropped. Count it in `dropped_logs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogDropped;

/// Lines waiting to go out as Log Events, held in a ring of `BYTES` bytes.
///
/// `BYTES` must fit at least one full-length line. Short lines take only
/// their own length, so a queue holds many more short lines than long ones.
pub struct LogQueue<const BYTES: usize> {
    level: LogLevel,
    buf: [u8; BYTES],
    start: usize,
    len: usize,
    /// Lines dropped since the last Log Event popped.
    dropped: u32,
}

impl<const BYTES: usize> LogQueue<BYTES> {
    /// An empty queue with logs off.
    pub const fn new() -> Self {
        const { assert!(BYTES >= LINE_HEADER + MAX_LOG_TEXT, "a LogQueue must fit a full line") };
        Self { level: LogLevel::Off, buf: [0; BYTES], start: 0, len: 0, dropped: 0 }
    }

    pub fn level(&self) -> LogLevel {
        self.level
    }

    /// Sets the level and discards queued lines and the dropped count, so a
    /// new client never receives an earlier client's backlog.
    pub fn set_level(&mut self, level: LogLevel) {
        self.level = level;
        self.start = 0;
        self.len = 0;
        self.dropped = 0;
    }

    /// Queues one line if the level lets it through. A line longer than a Log
    /// Event can carry is truncated on a character boundary.
    pub fn push(&mut self, level: log::Level, args: fmt::Arguments) -> Result<(), LogDropped> {
        let level = LogLevel::from(level);
        if level > self.level {
            return Ok(());
        }
        let free = BYTES - self.len;
        let end = self.start + self.len;
        let mut line = Line {
            buf: &mut self.buf,
            text_start: end + LINE_HEADER,
            len: 0,
            room: free.saturating_sub(LINE_HEADER).min(MAX_LOG_TEXT),
            truncated: false,
            full: free < LINE_HEADER,
        };
        let _ = line.write_fmt(args);
        if line.full {
            self.dropped = self.dropped.wrapping_add(1);
            return Err(LogDropped);
        }
        let text_len = line.len;
        let [lo, hi] = (text_len as u16).to_le_bytes();
        copy_in(&mut self.buf, end, &[level as u8, lo, hi]);
        self.len += LINE_HEADER + text_len;
        Ok(())
    }

    /// Writes the oldest line into `out` as a Log Event payload and returns
    /// its length, or `None` if the queue is empty.
    pub fn pop(&mut self, out: &mut [u8; MAX_PAYLOAD]) -> Option<usize> {
        if self.len == 0 {
            return None;
        }
        let mut header = [0; LINE_HEADER];
        copy_out(&self.buf, self.start, &mut header);
        let level = LogLevel::from_u8(header[0]).expect("queued lines have a level");
        let text_len = usize::from(u16::from_le_bytes([header[1], header[2]]));
        let at = encode_log_header(out, level, self.dropped);
        copy_out(&self.buf, self.start + LINE_HEADER, &mut out[at..at + text_len]);
        self.start = (self.start + LINE_HEADER + text_len) % BYTES;
        self.len -= LINE_HEADER + text_len;
        self.dropped = 0;
        Some(at + text_len)
    }
}

impl<const BYTES: usize> Default for LogQueue<BYTES> {
    fn default() -> Self {
        Self::new()
    }
}

/// Formats one line straight into the ring, after the space for its header.
struct Line<'a> {
    buf: &'a mut [u8],
    /// Unwrapped index where the text starts.
    text_start: usize,
    len: usize,
    /// The most text that fits: the free space, capped at a full line.
    room: usize,
    truncated: bool,
    /// The line doesn't fit in the free space.
    full: bool,
}

impl fmt::Write for Line<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.truncated || self.full {
            return Err(fmt::Error);
        }
        let mut take = s.len();
        if self.len + take > MAX_LOG_TEXT {
            take = s.floor_char_boundary(MAX_LOG_TEXT - self.len);
            self.truncated = true;
        }
        if self.len + take > self.room {
            self.full = true;
            return Err(fmt::Error);
        }
        copy_in(self.buf, self.text_start + self.len, &s.as_bytes()[..take]);
        self.len += take;
        Ok(())
    }
}

/// Copies `bytes` into the ring at the unwrapped index `at`.
fn copy_in(buf: &mut [u8], at: usize, bytes: &[u8]) {
    let at = at % buf.len();
    let first = bytes.len().min(buf.len() - at);
    buf[at..at + first].copy_from_slice(&bytes[..first]);
    buf[..bytes.len() - first].copy_from_slice(&bytes[first..]);
}

/// Fills `out` from the ring at the unwrapped index `at`.
fn copy_out(buf: &[u8], at: usize, out: &mut [u8]) {
    let at = at % buf.len();
    let first = out.len().min(buf.len() - at);
    out[..first].copy_from_slice(&buf[at..at + first]);
    let rest = out.len() - first;
    out[first..].copy_from_slice(&buf[..rest]);
}
