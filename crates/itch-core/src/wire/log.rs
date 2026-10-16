use super::DecodeError;
use super::codec::{Reader, Writer};
use crate::frame::MAX_PAYLOAD;

pub(crate) const LOG_EVENT_TYPE: u8 = 0x40;

/// The longest Log Event text: the frame cap less `type`, `level` and `dropped`.
pub const MAX_LOG_TEXT: usize = MAX_PAYLOAD - 6;

/// A connection's log level, or the level of one Log Event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(u8)]
pub enum LogLevel {
    #[default]
    Off = 0,
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
}

impl LogLevel {
    pub fn from_u8(value: u8) -> Option<Self> {
        use LogLevel::*;
        Some(match value {
            0 => Off,
            1 => Error,
            2 => Warn,
            3 => Info,
            4 => Debug,
            _ => return None,
        })
    }

    pub(crate) fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Self::from_u8(r.u8()?).ok_or(DecodeError::Malformed)
    }
}

/// The wire has no trace level, so trace records go out as debug.
impl From<log::Level> for LogLevel {
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Error => LogLevel::Error,
            log::Level::Warn => LogLevel::Warn,
            log::Level::Info => LogLevel::Info,
            log::Level::Debug | log::Level::Trace => LogLevel::Debug,
        }
    }
}

/// The `0x40` Log Event. `text` runs to the end of the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogEvent<'a> {
    /// Never [`LogLevel::Off`].
    pub level: LogLevel,
    /// Log Events this connection dropped since the last one sent.
    pub dropped: u32,
    pub text: &'a str,
}

impl<'a> LogEvent<'a> {
    pub(crate) fn encode(&self, w: &mut Writer) {
        encode_header(w, self.level, self.dropped);
        w.bytes(self.text.as_bytes());
    }

    pub(crate) fn decode(r: &mut Reader<'a>) -> Result<Self, DecodeError> {
        let level = LogLevel::decode(r)?;
        if level == LogLevel::Off {
            return Err(DecodeError::Malformed);
        }
        let dropped = r.u32()?;
        let text = core::str::from_utf8(r.rest()).map_err(|_| DecodeError::Malformed)?;
        Ok(Self { level, dropped, text })
    }
}

/// Everything before the text, which callers append themselves.
pub(crate) fn encode_header(w: &mut Writer, level: LogLevel, dropped: u32) {
    w.u8(LOG_EVENT_TYPE);
    w.u8(level as u8);
    w.u32(dropped);
}
