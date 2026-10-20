//! The itch Rig firmware. Build it with `cargo +esp build-firmware` and
//! flash it with `cargo +esp flash-firmware`.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, Ordering};

use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Ticker, Timer};
use esp_rtos::embassy::Executor;
use itch_core::board::Board;
use itch_core::frame::MAX_PAYLOAD;
use itch_core::rig::{Handled, Rig, SETTLE_MS};
use itch_firmware::board::{self, RealBoard};
use itch_firmware::queue::{self, Payload, Transport};
use itch_firmware::{logger, supervisor, usb};
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

/// How often the `rig` task does its idle work.
const TICK: Duration = Duration::from_millis(50);

static EXECUTOR: StaticCell<Executor> = StaticCell::new();

/// Set by the first panic.
static PANICKED: AtomicBool = AtomicBool::new(false);

#[esp_hal::main]
fn main() -> ! {
    let board::Parts {
        mut board,
        watchdog,
        usb_rx,
        usb_tx,
        #[cfg(feature = "wifi")]
        wifi,
    } = board::boot();
    logger::init();
    let last_panic = board.info().last_panic;
    if !last_panic.is_empty() {
        log::error!("the previous boot panicked at {last_panic}");
    }
    EXECUTOR.init(Executor::new()).run(move |spawner| {
        let rig_logic = Rig::new(&mut board);
        if board.reinit().is_err() {
            log::error!("the expander didn't come up; holding it in reset");
        }
        spawner.spawn(supervisor::supervisor(watchdog).unwrap());
        spawner.spawn(rig(rig_logic, board).unwrap());
        spawner.spawn(usb::usb_rx(usb_rx).unwrap());
        spawner.spawn(usb::usb_tx(usb_tx).unwrap());
        #[cfg(feature = "wifi")]
        itch_firmware::tcp::start(spawner, wifi);
    })
}

/// The sole owner of the Rig, the board and I2C. Answers queued requests
/// in order, each to the transport it came from, and runs the idle tick.
#[embassy_executor::task]
async fn rig(mut rig: Rig, mut board: RealBoard) {
    let mut ticker = Ticker::every(TICK);
    let mut reply = [0; MAX_PAYLOAD];
    loop {
        // The tick goes first, so a stream of requests can't starve it.
        let request = match select(ticker.next(), queue::REQUESTS.receive()).await {
            Either::First(()) => {
                rig.tick(&mut board);
                board.tick();
                supervisor::check_in(board.now());
                continue;
            }
            Either::Second(request) => request,
        };
        let frame = request.frame.as_ref().map(Payload::as_bytes).map_err(|&error| error);
        let Handled::Reply { len, settle } =
            rig.receive(frame, &mut board, &request.transport, &mut reply)
        else {
            continue;
        };
        if settle {
            Timer::after_millis(SETTLE_MS).await;
        }
        send_reply(request.transport, Payload::new(&reply[..len])).await;
    }
}

async fn send_reply(transport: Transport, reply: Payload) {
    match transport {
        Transport::Usb => usb::REPLIES.send(reply).await,
        #[cfg(feature = "wifi")]
        Transport::Tcp(connection) => itch_firmware::tcp::reply(connection, reply).await,
    }
}

/// Opens every contact, leaves where the panic happened for the next boot,
/// then resets the chip's digital core and peripherals. Never a CPU-only
/// reset, which would leave the peripherals running. CPU exceptions end up
/// here too.
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    board::force_reset_low();
    if !PANICKED.swap(true, Ordering::Relaxed) {
        board::record_panic(info.location());
        esp_hal::system::software_reset()
    }
    // A panic on the way to the reset, which takes an esp-hal lock: keep
    // the first panic's record and let the RTC watchdog reset the chip.
    loop {
        core::hint::spin_loop();
    }
}
