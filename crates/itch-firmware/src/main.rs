//! The itch Rig firmware. Build it with `cargo +esp build-firmware` and
//! flash it with `cargo +esp flash-firmware`.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, Ordering};

use embassy_time::{Instant, Timer};
use esp_rtos::embassy::Executor;
use itch_core::board::Board;
use itch_core::rig::Rig;
use itch_firmware::board::{self, RealBoard};
use itch_firmware::connections::{CONNECTIONS, USB};
use itch_firmware::serve::{Clock, serve};
use itch_firmware::{logger, radio, supervisor, usb};
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

static EXECUTOR: StaticCell<Executor> = StaticCell::new();

/// Set by the first panic.
static PANICKED: AtomicBool = AtomicBool::new(false);

#[esp_hal::main]
fn main() -> ! {
    let board::Parts { mut board, watchdog, usb_rx, usb_tx, radio } = board::boot();
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
        let credentials = board.load_wifi();
        let hardware_id = board.info().hardware_id;
        spawner.spawn(supervisor::supervisor(watchdog).unwrap());
        let usb = CONNECTIONS.connect(USB);
        spawner.spawn(rig(rig_logic, board).unwrap());
        spawner.spawn(usb::usb_rx(usb_rx, usb).unwrap());
        spawner.spawn(usb::usb_tx(usb_tx, usb).unwrap());
        // After USB, which keeps working whatever the radio does.
        radio::start(spawner, radio, credentials, hardware_id);
    })
}

/// The sole owner of the Rig, the board and I2C.
#[embassy_executor::task]
async fn rig(mut rig: Rig, mut board: RealBoard) {
    serve(&mut rig, &mut board, &CONNECTIONS, &EmbassyClock).await
}

/// The embassy timer, which the board's clock reads too.
struct EmbassyClock;

impl Clock for EmbassyClock {
    async fn until(&self, at: u64) {
        Timer::at(Instant::from_millis(at)).await
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
