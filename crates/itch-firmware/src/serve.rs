//! The Rig task: answers queued requests in order, each to the connection
//! that sent it, and runs the idle tick.

use core::future::Future;

use embassy_futures::select::{Either, select};
use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;
use embedded_hal::i2c::I2c;
use embedded_storage::nor_flash::NorFlash;
use itch_core::board::Board;
use itch_core::frame::MAX_PAYLOAD;
use itch_core::rig::{Handled, Rig, SETTLE_MS};

use crate::board::RevA;
use crate::connections::{Connections, Payload};

/// How often the Rig task does its idle work, in ms.
pub const TICK_MS: u64 = 50;

/// How long a reply may wait for room in its connection's queue, in ms.
/// After that it is dropped and the client retries.
pub const REPLY_TIMEOUT_MS: u64 = 100;

/// Waits on the board's clock, which [`Board::now`] reads.
pub trait Clock {
    /// Waits until `at` ms since boot.
    fn until(&self, at: u64) -> impl Future<Output = ()>;
}

/// The sole owner of the Rig, the board and I2C. Never returns.
pub async fn serve<const N: usize, I2C, RESET, LED, DELAY, FLASH>(
    rig: &mut Rig,
    board: &mut RevA<I2C, RESET, LED, DELAY, FLASH>,
    connections: &Connections<N>,
    clock: &impl Clock,
) -> !
where
    I2C: I2c,
    RESET: OutputPin,
    LED: OutputPin,
    DELAY: DelayNs,
    FLASH: NorFlash,
{
    let mut next_tick = board.now() + TICK_MS;
    let mut reply = [0; MAX_PAYLOAD];
    loop {
        // The tick goes first, so a stream of requests can't starve it.
        let request = match select(clock.until(next_tick), connections.next_request()).await {
            Either::First(()) => {
                next_tick += TICK_MS;
                rig.tick(board);
                board.tick();
                continue;
            }
            Either::Second(request) => request,
        };
        let frame = request.frame.as_ref().map(Payload::as_bytes).map_err(|&error| error);
        let Handled::Reply { len, settle } = rig.receive(frame, board, &request.from, &mut reply)
        else {
            continue;
        };
        if settle {
            clock.until(board.now() + SETTLE_MS).await;
        }
        // A client flooding requests fills its replies with `Busy`, and one
        // that stops reading leaves them full. Either could keep this reply
        // waiting until the Rig task misses its deadline: drop it instead.
        let deadline = board.now() + REPLY_TIMEOUT_MS;
        select(request.from.reply(Payload::new(&reply[..len])), clock.until(deadline)).await;
    }
}
