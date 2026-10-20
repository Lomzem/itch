//! The USB Serial/JTAG transport: `usb_rx` feeds the request queue and
//! `usb_tx`, the only USB writer, sends replies before Log Events.

use core::sync::atomic::{AtomicU32, Ordering};

use embassy_time::{Duration, with_timeout};
use itch_core::frame::{FrameDecoder, MAX_FRAME};

use crate::board::{UsbRx, UsbTx};
use crate::mailbox::Mailbox;

/// How long one frame may take to go out. After that the rest of it is
/// dropped, so a host that stops reading never stalls the Rig.
const WRITE_TIMEOUT: Duration = Duration::from_millis(100);

/// Frames cut short by the write timeout: `usb_tx_dropped`.
pub static TX_DROPPED: AtomicU32 = AtomicU32::new(0);

/// The exchange between the USB tasks and the Rig task.
pub static MAILBOX: Mailbox = Mailbox::new();

/// Turns the bytes from the host into queued requests.
#[embassy_executor::task]
pub async fn usb_rx(mut rx: UsbRx) {
    let mut decoder = FrameDecoder::new();
    let mut bytes = [0; 64];
    loop {
        let Ok(read) = embedded_io_async::Read::read(&mut rx, &mut bytes).await;
        for &byte in &bytes[..read] {
            if let Some(frame) = decoder.push(byte) {
                MAILBOX.submit(frame);
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
        let len = MAILBOX.next_frame(&mut frame).await;
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
