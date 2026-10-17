//! The itch Rig firmware. Build it with `cargo +esp build-firmware` and
//! flash it with `cargo +esp flash-firmware`.

#![no_std]
#![no_main]

use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Ticker, Timer};
use esp_rtos::embassy::Executor;
use itch_core::board::Board;
use itch_core::frame::MAX_PAYLOAD;
use itch_core::rig::{Handled, Rig, SETTLE_MS};
use itch_core::wire::LogLevel;
use itch_firmware::board::{self, RealBoard};
use itch_firmware::queue::{self, Payload, Transport};
use itch_firmware::{logger, usb};
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

/// How often the `rig` task does its idle work.
const TICK: Duration = Duration::from_millis(50);

static EXECUTOR: StaticCell<Executor> = StaticCell::new();

#[esp_hal::main]
fn main() -> ! {
    let board::Parts { mut board, usb_rx, usb_tx } = board::boot();
    logger::init();
    EXECUTOR.init(Executor::new()).run(move |spawner| {
        if board.reinit().is_err() {
            log::error!("the expander didn't come up; holding it in reset");
        }
        spawner.spawn(rig(board).unwrap());
        spawner.spawn(usb::usb_rx(usb_rx).unwrap());
        spawner.spawn(usb::usb_tx(usb_tx).unwrap());
    })
}

/// The sole owner of the Rig, the board and I2C. Answers queued requests
/// in order, each to the transport it came from, and runs the idle tick.
#[embassy_executor::task]
async fn rig(mut board: RealBoard) {
    let mut rig = Rig::new();
    let mut ticker = Ticker::every(TICK);
    let mut reply = [0; MAX_PAYLOAD];
    loop {
        // The tick goes first, so a stream of requests can't starve it.
        let request = match select(ticker.next(), queue::REQUESTS.receive()).await {
            Either::First(()) => {
                rig.tick(&mut board);
                board.tick();
                continue;
            }
            Either::Second(request) => request,
        };
        let len = match rig.handle(request.payload.as_bytes(), &mut board, &mut reply) {
            Handled::NoReply => continue,
            Handled::Reply { len, settle } => {
                if settle {
                    Timer::after_millis(SETTLE_MS).await;
                }
                len
            }
            Handled::SetLogLevel { len, level } => {
                set_log_level(request.transport, level);
                len
            }
        };
        send_reply(request.transport, Payload::new(&reply[..len])).await;
    }
}

fn set_log_level(transport: Transport, level: LogLevel) {
    match transport {
        Transport::Usb => usb::set_log_level(level),
    }
}

async fn send_reply(transport: Transport, reply: Payload) {
    match transport {
        Transport::Usb => usb::REPLIES.send(reply).await,
    }
}

/// Opens every contact, then resets the whole chip. Never a CPU-only
/// reset, which would leave the expander running.
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    board::force_reset_low();
    esp_hal::system::software_reset()
}
