//! The Wi-Fi Transport's TCP connections on [`PORT`]. Each `tcp` task
//! serves one connection at a time in its own slot of the connection core:
//! `tcp_rx` feeds the request queue and `tcp_tx`, the connection's only
//! writer, sends replies before Log Events.
//!
//! A task listens only while its slot is free. While all
//! [`TCP_CONNECTIONS`] are open nothing listens, so the network stack
//! refuses another connection straight away with a reset.

use embassy_futures::select::select;
use embassy_net::Stack;
use embassy_net::tcp::{TcpReader, TcpSocket, TcpWriter};
use embassy_time::{Duration, with_timeout};
use itch_core::frame::{FrameDecoder, MAX_FRAME};

use crate::connections::{CONNECTIONS, Connection, TCP_CONNECTIONS};
use crate::radio;

/// The port clients connect to: `tcp://<address>:7878`.
pub const PORT: u16 = 7878;

/// How long one frame may take to go out. After that the rest of it is
/// dropped, so a client that stops reading never stalls the Rig.
const WRITE_TIMEOUT: Duration = Duration::from_millis(100);

/// How often an idle connection is probed, and how long the Rig waits for
/// an answer before dropping it, so a client that vanished, or a network
/// the Rig left, frees the slot.
const KEEP_ALIVE: Duration = Duration::from_secs(5);
const TIMEOUT: Duration = Duration::from_secs(15);

/// Each connection's socket buffers, in RAM the task owns. A request is at
/// most one frame. Replies and Log Events are written a frame at a time,
/// and a stalled client fills the transmit buffer until the write times
/// out.
const RX_BYTES: usize = 1024;
const TX_BYTES: usize = 2048;

/// Serves TCP connections in `slot`, one after another.
#[embassy_executor::task(pool_size = TCP_CONNECTIONS)]
pub async fn tcp(stack: Stack<'static>, slot: usize) {
    let mut rx_buffer = [0; RX_BYTES];
    let mut tx_buffer = [0; TX_BYTES];
    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_keep_alive(Some(KEEP_ALIVE));
        socket.set_timeout(Some(TIMEOUT));
        if socket.accept(PORT).await.is_ok() {
            // Logs start off, with nothing left from the slot's last client.
            let connection = CONNECTIONS.connect(slot);
            let client = socket.remote_endpoint();
            if let Some(client) = client {
                log::info!("{client} connected");
            }
            radio::log_heap();
            let (rx, tx) = socket.split();
            select(tcp_rx(rx, connection), tcp_tx(tx, connection)).await;
            connection.disconnect();
            if let Some(client) = client {
                log::info!("{client} disconnected");
            }
            socket.close();
            let _ = with_timeout(WRITE_TIMEOUT, socket.flush()).await;
        }
        socket.abort();
        let _ = with_timeout(WRITE_TIMEOUT, socket.flush()).await;
    }
}

/// Turns the bytes from the client into queued requests, until it hangs up
/// or the connection fails.
async fn tcp_rx(mut rx: TcpReader<'_>, connection: Connection) {
    let mut decoder = FrameDecoder::new();
    let mut bytes = [0; 256];
    loop {
        let read = match rx.read(&mut bytes).await {
            Ok(0) | Err(_) => return,
            Ok(read) => read,
        };
        for &byte in &bytes[..read] {
            if let Some(frame) = decoder.push(byte) {
                connection.receive(frame);
            }
        }
    }
}

/// Sends replies, then Log Events, one frame at a time, until the
/// connection fails.
async fn tcp_tx(mut tx: TcpWriter<'_>, connection: Connection) {
    let mut frame = [0; MAX_FRAME];
    loop {
        let len = connection.next_frame(&mut frame).await;
        let sent = with_timeout(
            WRITE_TIMEOUT,
            embedded_io_async::Write::write_all(&mut tx, &frame[..len]),
        )
        .await;
        // A frame cut short by the timeout is fine: the client resyncs on
        // the next one's leading delimiter.
        if let Ok(Err(_)) = sent {
            return;
        }
    }
}
