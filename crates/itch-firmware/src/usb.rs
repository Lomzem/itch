//! The USB Serial/JTAG transport: `usb_rx` feeds the request queue and
//! `usb_tx`, the only USB writer, sends replies before Log Events.

use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, Ordering};

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, with_timeout};
use itch_core::frame::{FrameDecoder, MAX_FRAME};
use itch_core::logs::LogQueue;
use itch_core::wire::LogLevel;

use crate::board::{UsbRx, UsbTx};
use crate::queue::{self, Logs, Replies, Transport};

/// How long one frame may take to go out. After that the rest of it is
/// dropped, so a host that stops reading never stalls the Rig.
const WRITE_TIMEOUT: Duration = Duration::from_millis(100);

/// Room for one full-length Log Event or many short ones.
const LOG_BYTES: usize = 4 * 1024;

/// Frames cut short by the write timeout: `usb_tx_dropped`.
pub static TX_DROPPED: AtomicU32 = AtomicU32::new(0);

/// Replies waiting for `usb_tx`.
pub static REPLIES: Replies = Channel::new();

/// Log Events waiting for `usb_tx`, filtered by the USB log level.
pub static LOGS: Logs<LOG_BYTES> = Mutex::new(RefCell::new(LogQueue::new()));

/// Wakes `usb_tx` after a log line is queued.
pub static LOGGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

pub fn set_log_level(level: LogLevel) {
    LOGS.lock(|logs| logs.borrow_mut().set_level(level));
}

/// Turns the bytes from the host into queued requests.
#[embassy_executor::task]
pub async fn usb_rx(mut rx: UsbRx) {
    let mut decoder = FrameDecoder::new();
    let mut bytes = [0; 64];
    loop {
        let Ok(read) = embedded_io_async::Read::read(&mut rx, &mut bytes).await;
        for &byte in &bytes[..read] {
            if let Some(frame) = decoder.push(byte) {
                queue::receive(Transport::Usb, frame, &REPLIES);
            }
        }
    }
}

/// Sends replies, then Log Events, one frame at a time.
#[embassy_executor::task]
pub async fn usb_tx(mut tx: UsbTx) {
    let mut frame = [0; MAX_FRAME];
    let mut timed_out = false;
    loop {
        let len = queue::next_frame(&REPLIES, &LOGS, &LOGGED, &mut frame).await;
        let sent = with_timeout(WRITE_TIMEOUT, async {
            // A timed-out frame can leave a packet the host hasn't read, and
            // esp-hal writes without waiting for room, so wait for the host.
            if timed_out {
                embedded_io_async::Write::flush(&mut tx).await?;
            }
            embedded_io_async::Write::write_all(&mut tx, &frame[..len]).await?;
            embedded_io_async::Write::flush(&mut tx).await
        })
        .await;
        timed_out = sent.is_err();
        if timed_out {
            // The host sees a cut-off frame and resyncs on the next one's
            // leading delimiter.
            TX_DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    }
}
