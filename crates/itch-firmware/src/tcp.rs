//! The development-only WiFi transport (ADR 0003), built with the `wifi`
//! feature. `wifi` keeps the Rig on the network named at build time, `net`
//! runs the IP stack, and `tcp` serves one connection at a time on [`PORT`]:
//! `tcp_rx` feeds the request queue and `tcp_tx`, the only TCP writer, sends
//! replies before Log Events.

use core::cell::RefCell;

use embassy_executor::Spawner;
use embassy_futures::select::select;
use embassy_net::tcp::{TcpReader, TcpSocket, TcpWriter};
use embassy_net::{Runner, Stack, StackResources};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer, with_timeout};
use esp_hal::peripherals::WIFI;
use esp_hal::rng::Rng;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config, ControllerConfig, Interface, WifiController,
};
use itch_core::frame::{FrameDecoder, MAX_FRAME, MAX_PAYLOAD, encode_frame};
use itch_core::logs::LogQueue;
use itch_core::wire::LogLevel;
use static_cell::StaticCell;

use crate::queue::{self, Payload, Replies, Transport};

/// The network to join, from the build environment. Development only: a
/// production build has no credentials.
const SSID: &str = env!("ITCH_WIFI_SSID", "set ITCH_WIFI_SSID to build with `wifi`");
const PASSWORD: &str = env!("ITCH_WIFI_PASSWORD", "set ITCH_WIFI_PASSWORD to build with `wifi`");
const _: () = assert!(!SSID.is_empty() && SSID.len() <= 32, "ITCH_WIFI_SSID must be 1 to 32 bytes");
const _: () = assert!(
    PASSWORD.len() >= 8 && PASSWORD.len() <= 63,
    "ITCH_WIFI_PASSWORD must be a WPA2 passphrase of 8 to 63 characters"
);

/// The port clients connect to: `tcp://<ip>:7878`.
pub const PORT: u16 = 7878;

/// How long one frame, or one reply waiting for room, may take. After that
/// it is dropped, so a client that stops reading never stalls the Rig.
const WRITE_TIMEOUT: Duration = Duration::from_millis(100);

/// How often an idle connection is probed, and how long the Rig waits for
/// an answer before dropping it, so a client that vanished frees the port.
const KEEP_ALIVE: Duration = Duration::from_secs(5);
const TIMEOUT: Duration = Duration::from_secs(15);

/// How long to wait before joining the network again.
const REJOIN: Duration = Duration::from_secs(5);

/// Room for one full-length Log Event or many short ones.
const LOG_BYTES: usize = 4 * 1024;

/// Replies waiting for `tcp_tx`.
pub static REPLIES: Replies = Channel::new();

/// Log Events waiting for `tcp_tx`, filtered by the connection's log level.
/// Logs are off between connections.
pub static LOGS: Mutex<CriticalSectionRawMutex, RefCell<LogQueue<LOG_BYTES>>> =
    Mutex::new(RefCell::new(LogQueue::new()));

/// Wakes `tcp_tx` after a log line is queued.
pub static LOGGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// DHCP and the TCP listener.
static RESOURCES: StaticCell<StackResources<2>> = StaticCell::new();

pub fn set_log_level(level: LogLevel) {
    LOGS.lock(|logs| logs.borrow_mut().set_level(level));
}

/// Queues a reply for the connection. Dropped if `tcp_tx` can't take it in
/// time, as when the client has gone, so the `rig` task never stalls.
pub async fn reply(reply: Payload) {
    let _ = with_timeout(WRITE_TIMEOUT, REPLIES.send(reply)).await;
}

/// Brings up the radio and spawns the WiFi tasks. If the radio doesn't
/// start, the Rig carries on over USB alone.
pub fn start(spawner: Spawner, wifi: WIFI<'static>) {
    // The radio needs about 57 KiB at its peak, mostly from RAM the
    // bootloader no longer uses.
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 36 * 1024);
    let station = StationConfig::default()
        .with_ssid(SSID.try_into().expect("checked at build time"))
        .with_authentication(AuthenticationMethodConfig::Wpa2Personal(
            PASSWORD.try_into().expect("checked at build time"),
        ));
    let config = ControllerConfig::default().with_initial_config(Config::Station(station));
    let Ok(controller) = WifiController::new(wifi, config) else {
        log::error!("the radio didn't start; serving USB only");
        return;
    };
    let rng = Rng::new();
    let seed = (u64::from(rng.random()) << 32) | u64::from(rng.random());
    let (stack, runner) = embassy_net::new(
        Interface::station(),
        embassy_net::Config::dhcpv4(Default::default()),
        RESOURCES.init(StackResources::new()),
        seed,
    );
    spawner.spawn(join(controller).unwrap());
    spawner.spawn(net(runner).unwrap());
    spawner.spawn(tcp(stack).unwrap());
}

/// Keeps the Rig on the network, rejoining after it drops.
#[embassy_executor::task]
async fn join(mut controller: WifiController<'static>) {
    loop {
        match controller.connect_async().await {
            Ok(_) => {
                log::info!("joined {SSID}");
                let _ = controller.wait_for_disconnect_async().await;
                log::warn!("left {SSID}");
            }
            Err(_) => log::warn!("couldn't join {SSID}"),
        }
        Timer::after(REJOIN).await;
    }
}

#[embassy_executor::task]
async fn net(mut runner: Runner<'static, Interface>) {
    runner.run().await
}

/// Serves one connection at a time. A new connection starts with logs off
/// and none of the last one's replies.
#[embassy_executor::task]
async fn tcp(stack: Stack<'static>) {
    let mut rx_buffer = [0; 2 * 1024];
    let mut tx_buffer = [0; 4 * 1024];
    loop {
        stack.wait_config_up().await;
        if let Some(config) = stack.config_v4() {
            log::info!("listening on tcp://{}:{PORT}", config.address.address());
        }
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_keep_alive(Some(KEEP_ALIVE));
        socket.set_timeout(Some(TIMEOUT));
        if socket.accept(PORT).await.is_ok() {
            REPLIES.clear();
            set_log_level(LogLevel::Off);
            let (rx, tx) = socket.split();
            select(tcp_rx(rx), tcp_tx(tx)).await;
            set_log_level(LogLevel::Off);
            socket.close();
            let _ = with_timeout(WRITE_TIMEOUT, socket.flush()).await;
        }
        socket.abort();
        let _ = with_timeout(WRITE_TIMEOUT, socket.flush()).await;
    }
}

/// Turns the bytes from the client into queued requests, until it hangs up.
async fn tcp_rx(mut rx: TcpReader<'_>) {
    let mut decoder = FrameDecoder::new();
    let mut bytes = [0; 256];
    loop {
        let read = match embedded_io_async::Read::read(&mut rx, &mut bytes).await {
            Ok(0) | Err(_) => return,
            Ok(read) => read,
        };
        for &byte in &bytes[..read] {
            if let Some(frame) = decoder.push(byte) {
                queue::receive(Transport::Tcp, frame, &REPLIES);
            }
        }
    }
}

/// Sends replies, then Log Events, one frame at a time, until the
/// connection fails.
async fn tcp_tx(mut tx: TcpWriter<'_>) {
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
