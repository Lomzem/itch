//! The USB Serial/JTAG transport: `usb_rx` feeds the request queue and
//! `usb_tx`, the only USB writer, sends replies before Log Events.

use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, Ordering};

use embassy_futures::select::select;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, TrySendError};
use embassy_sync::signal::Signal;
use embassy_time::{Duration, with_timeout};
use itch_core::frame::{FrameDecoder, MAX_FRAME, MAX_PAYLOAD, encode_frame};
use itch_core::logs::LogQueue;
use itch_core::wire::LogLevel;

use crate::board::{UsbRx, UsbTx};
use crate::queue::{self, Payload, Queued, Transport};

/// How long one frame may take to go out. After that the rest of it is
/// dropped, so a host that stops reading never stalls the Rig.
const WRITE_TIMEOUT: Duration = Duration::from_millis(100);

/// Room for one full-length Log Event or many short ones.
const LOG_BYTES: usize = 4 * 1024;

/// Frames that didn't fully go out: the Rig's `usb_tx_dropped` counter.
pub static TX_DROPPED: AtomicU32 = AtomicU32::new(0);

/// Replies waiting for `usb_tx`.
pub static REPLIES: Channel<CriticalSectionRawMutex, Payload, 2> = Channel::new();

/// Log Events waiting for `usb_tx`, filtered by the USB log level.
pub static LOGS: Mutex<CriticalSectionRawMutex, RefCell<LogQueue<LOG_BYTES>>> =
    Mutex::new(RefCell::new(LogQueue::new()));

/// Wakes `usb_tx` after a log line is queued.
pub static LOGGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

pub fn set_log_level(level: LogLevel) {
    LOGS.lock(|logs| logs.borrow_mut().set_level(level));
}

/// Turns the bytes from the host into queued requests. A request that finds
/// the queue full is answered `Busy` at once.
#[embassy_executor::task]
pub async fn usb_rx(mut rx: UsbRx) {
    let mut decoder = FrameDecoder::new();
    let mut bytes = [0; 64];
    loop {
        let Ok(read) = embedded_io_async::Read::read(&mut rx, &mut bytes).await;
        for &byte in &bytes[..read] {
            let Some(Ok(payload)) = decoder.push(byte) else {
                continue;
            };
            let request = Queued { transport: Transport::Usb, payload: Payload::new(payload) };
            if let Err(TrySendError::Full(request)) = queue::REQUESTS.try_send(request)
                && let Some(busy) = queue::busy(request.payload.as_bytes())
                && REPLIES.try_send(busy).is_err()
            {
                TX_DROPPED.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// Sends replies, then Log Events, one frame at a time.
#[embassy_executor::task]
pub async fn usb_tx(mut tx: UsbTx) {
    let mut log = [0; MAX_PAYLOAD];
    let mut frame = [0; MAX_FRAME];
    loop {
        let len = if let Ok(reply) = REPLIES.try_receive() {
            encode_frame(reply.as_bytes(), &mut frame)
        } else if let Some(len) = LOGS.lock(|logs| logs.borrow_mut().pop(&mut log)) {
            encode_frame(&log[..len], &mut frame)
        } else {
            select(REPLIES.ready_to_receive(), LOGGED.wait()).await;
            continue;
        };
        let sent = with_timeout(WRITE_TIMEOUT, async {
            embedded_io_async::Write::write_all(&mut tx, &frame[..len]).await?;
            embedded_io_async::Write::flush(&mut tx).await
        })
        .await;
        if !matches!(sent, Ok(Ok(()))) {
            // The host sees a cut-off frame and resyncs on the next one's
            // leading delimiter.
            TX_DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    }
}
