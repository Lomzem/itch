//! TAFP framing: `0x00 | COBS(length u16 + payload + CRC-16/CCITT-FALSE) | 0x00`,
//! with the length and CRC little-endian and the CRC taken over length + payload.

/// The largest payload a frame may carry. Larger frames are dropped, never fragmented.
pub const MAX_PAYLOAD: usize = 1024;

const MAX_LOGICAL: usize = 2 + MAX_PAYLOAD + 2;

/// The longest encoded frame, both delimiters included.
pub const MAX_FRAME: usize = 1 + MAX_LOGICAL + MAX_LOGICAL.div_ceil(254) + 1;

/// Frames `payload` into `out` and returns the frame's length.
///
/// # Panics
///
/// If `payload` is longer than [`MAX_PAYLOAD`].
pub fn encode_frame(payload: &[u8], out: &mut [u8; MAX_FRAME]) -> usize {
    assert!(payload.len() <= MAX_PAYLOAD, "payload over {MAX_PAYLOAD} bytes");
    let length = (payload.len() as u16).to_le_bytes();
    let crc = crc16(crc16(0xFFFF, &length), payload).to_le_bytes();
    let mut cobs = CobsEncoder::new(out);
    for &byte in length.iter().chain(payload).chain(&crc) {
        cobs.push(byte);
    }
    cobs.finish()
}

/// Why [`FrameDecoder`] dropped a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    /// Bad CRC, truncated COBS or a length that disagrees with the payload.
    /// Counted in `crc_failures`.
    Corrupt,
    /// Over [`MAX_PAYLOAD`]. Counted in `oversized_frames`.
    Oversized,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameCounters {
    pub crc_failures: u32,
    pub oversized_frames: u32,
}

/// Turns a byte stream into payloads, one byte at a time.
///
/// Every `0x00` ends a frame, so the decoder resyncs after noise or a
/// mid-stream join; the garbage before the next delimiter is dropped as one
/// corrupt frame. Empty frames (`00 00`) are ignored.
pub struct FrameDecoder {
    buf: [u8; MAX_LOGICAL],
    len: usize,
    started: bool,
    oversized: bool,
    block_left: u8,
    zero_pending: bool,
    counters: FrameCounters,
}

impl FrameDecoder {
    pub const fn new() -> Self {
        Self {
            buf: [0; MAX_LOGICAL],
            len: 0,
            started: false,
            oversized: false,
            block_left: 0,
            zero_pending: false,
            counters: FrameCounters { crc_failures: 0, oversized_frames: 0 },
        }
    }

    /// Feeds one byte. Returns the payload or the reason it was dropped when
    /// `byte` ends a frame.
    pub fn push(&mut self, byte: u8) -> Option<Result<&[u8], FrameError>> {
        if byte == 0 {
            return self.end_frame();
        }
        self.started = true;
        if self.block_left == 0 {
            if self.zero_pending {
                self.store(0);
            }
            self.block_left = byte - 1;
            self.zero_pending = byte != 0xFF;
        } else {
            self.store(byte);
            self.block_left -= 1;
        }
        None
    }

    pub fn counters(&self) -> FrameCounters {
        self.counters
    }

    fn store(&mut self, byte: u8) {
        if self.len == MAX_LOGICAL {
            self.oversized = true;
        } else {
            self.buf[self.len] = byte;
            self.len += 1;
        }
    }

    fn end_frame(&mut self) -> Option<Result<&[u8], FrameError>> {
        let (len, started, oversized, complete) =
            (self.len, self.started, self.oversized, self.block_left == 0);
        self.len = 0;
        self.started = false;
        self.oversized = false;
        self.block_left = 0;
        self.zero_pending = false;
        if !started {
            return None;
        }
        if oversized {
            self.counters.oversized_frames = self.counters.oversized_frames.wrapping_add(1);
            return Some(Err(FrameError::Oversized));
        }
        let logical = &self.buf[..len];
        if !complete || !is_valid(logical) {
            self.counters.crc_failures = self.counters.crc_failures.wrapping_add(1);
            return Some(Err(FrameError::Corrupt));
        }
        Some(Ok(&self.buf[2..len - 2]))
    }
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self::new()
    }
}

fn is_valid(logical: &[u8]) -> bool {
    let Some(payload_len) = logical.len().checked_sub(4) else {
        return false;
    };
    let (body, crc) = logical.split_at(logical.len() - 2);
    usize::from(u16::from_le_bytes([body[0], body[1]])) == payload_len
        && crc16(0xFFFF, body) == u16::from_le_bytes([crc[0], crc[1]])
}

/// CRC-16/CCITT-FALSE: poly `0x1021`, no reflection, no final XOR. Start from `0xFFFF`.
fn crc16(mut crc: u16, bytes: &[u8]) -> u16 {
    for &byte in bytes {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

/// Standard COBS, writing the leading delimiter on creation and the trailing
/// one on `finish`. A data run that ends exactly on a full 254-byte block gets
/// no extra `01` code.
struct CobsEncoder<'a> {
    out: &'a mut [u8; MAX_FRAME],
    code_at: usize,
    len: usize,
    full_block: bool,
}

impl<'a> CobsEncoder<'a> {
    fn new(out: &'a mut [u8; MAX_FRAME]) -> Self {
        out[0] = 0;
        Self { out, code_at: 1, len: 2, full_block: false }
    }

    fn push(&mut self, byte: u8) {
        if byte != 0 {
            self.out[self.len] = byte;
            self.len += 1;
        }
        self.full_block = byte != 0 && self.len - self.code_at == 0xFF;
        if byte == 0 || self.full_block {
            self.close_block();
        }
    }

    fn close_block(&mut self) {
        self.out[self.code_at] = (self.len - self.code_at) as u8;
        self.code_at = self.len;
        self.len += 1;
    }

    fn finish(mut self) -> usize {
        if self.full_block {
            self.len -= 1;
        } else {
            self.out[self.code_at] = (self.len - self.code_at) as u8;
        }
        self.out[self.len] = 0;
        self.len + 1
    }
}
