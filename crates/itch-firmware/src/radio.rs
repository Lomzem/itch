//! The Wi-Fi radio and the network stack under the TCP connections. The
//! Rig joins the network its credentials name as a WPA2-Personal station,
//! takes its address by DHCP or as configured, and joins again after any
//! failure or drop. New credentials from the board make it leave and join
//! the new network; without credentials the radio is off. A new address
//! configuration applies at once, without leaving. If the radio doesn't
//! start, the Rig carries on over USB alone.

use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_net::{ConfigV4, Ipv4Address, Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4};
use embassy_time::{Duration, Timer};
use esp_hal::peripherals;
use esp_hal::rng::Rng;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config, ControllerConfig, Interface, WifiController,
};
use itch_core::wifi::{Addressing, Credentials};
use static_cell::StaticCell;

use crate::connections::{TCP_CONNECTIONS, USB};
use crate::tcp::{self, PORT};
use crate::wifi::{Radio, WIFI};

/// How long the Rig waits before joining again after a failure or a drop.
const RETRY: Duration = Duration::from_secs(5);

/// How long the Rig stays on its network, at its address, after the board
/// hands over new credentials or a new address configuration, so the reply
/// to the change reaches its client first.
const LEAVE_DELAY: Duration = Duration::from_secs(1);

/// The network stack's socket slots: DHCP, the TCP connections, and one
/// kept for an mDNS responder.
const SOCKETS: usize = 1 + TCP_CONNECTIONS + 1;

static RESOURCES: StaticCell<StackResources<SOCKETS>> = StaticCell::new();

/// Sets up the heap, the radio and the network stack, and spawns their
/// tasks and the TCP connections' tasks. The radio joins with
/// `credentials`, and the stack takes its address as `addressing` says,
/// both as saved at boot.
pub fn start(
    spawner: Spawner,
    wifi: peripherals::WIFI<'static>,
    credentials: Option<Credentials>,
    addressing: Addressing,
) {
    // The radio's buffers and tasks. It was seen to need about 57 KiB at
    // its peak with one connection; the TCP connections' buffers are their
    // tasks' own RAM, not heap. Most of the heap is RAM the bootloader no
    // longer uses.
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 64 * 1024);

    let rng = Rng::new();
    let seed = (u64::from(rng.random()) << 32) | u64::from(rng.random());
    let mut config = embassy_net::Config::dhcpv4(Default::default());
    config.ipv4 = config_v4(addressing);
    let (stack, runner) =
        embassy_net::new(Interface::station(), config, RESOURCES.init(StackResources::new()), seed);
    spawner.spawn(net(runner).unwrap());
    spawner.spawn(address(stack).unwrap());
    spawner.spawn(readdress(stack).unwrap());
    spawner.spawn(join(wifi, credentials).unwrap());
    for slot in USB + 1..=TCP_CONNECTIONS {
        spawner.spawn(tcp::tcp(stack, slot).unwrap());
    }
}

/// Logs how much of the heap is in use, for checking its headroom on the
/// bench.
pub fn log_heap() {
    let heap = esp_alloc::HEAP.stats();
    log::debug!(
        "heap: {} of {} bytes in use, at most {}",
        heap.current_usage,
        heap.size,
        heap.max_usage
    );
}

/// Runs the network stack.
#[embassy_executor::task]
async fn net(mut runner: Runner<'static, Interface>) {
    runner.run().await
}

/// Tells `WIFI` the stack's IPv4 address as it comes and goes.
#[embassy_executor::task]
async fn address(stack: Stack<'static>) {
    loop {
        let config = stack.config_v4();
        let address = config.as_ref().map(|config| config.address.address());
        WIFI.set_address(address.map(|address| address.octets()));
        match address {
            Some(address) => {
                log::info!("serving tcp://{address}:{PORT}");
                stack.wait_config_down().await;
            }
            None => stack.wait_config_up().await,
        }
    }
}

/// Has the network stack take its address as the board says, once the
/// reply to the change has had time to go out. A static address applies
/// whether or not the Rig is joined, so it is the Rig's address from its
/// next join.
#[embassy_executor::task]
async fn readdress(stack: Stack<'static>) {
    loop {
        let addressing = WIFI.addressing().await;
        Timer::after(LEAVE_DELAY).await;
        stack.set_config_v4(config_v4(addressing));
        // `address` hears of an address coming or going, but not of one
        // address replacing another.
        let address = stack.config_v4().map(|config| config.address.address());
        WIFI.set_address(address.map(|address| address.octets()));
        if let Some(address) = address {
            log::info!("serving tcp://{address}:{PORT}");
        }
    }
}

/// The network stack's IPv4 configuration for `addressing`. The Rig
/// resolves no names, so it has no DNS servers.
fn config_v4(addressing: Addressing) -> ConfigV4 {
    match addressing {
        Addressing::Dhcp => ConfigV4::Dhcp(Default::default()),
        Addressing::Static(set) => ConfigV4::Static(StaticConfigV4 {
            address: Ipv4Cidr::new(Ipv4Address::from(set.address()), set.prefix()),
            gateway: set.gateway().map(Ipv4Address::from),
            dns_servers: Default::default(),
        }),
    }
}

/// Keeps the Rig on the network its credentials name, starting with
/// `credentials`, and turns the radio on and off as they come and go.
#[embassy_executor::task]
async fn join(mut wifi: peripherals::WIFI<'static>, mut credentials: Option<Credentials>) {
    loop {
        let Some(network) = credentials else {
            WIFI.set_radio(Radio::Off);
            credentials = WIFI.credentials().await;
            continue;
        };
        let config = ControllerConfig::default().with_initial_config(station(&network));
        match WifiController::new(wifi.reborrow(), config) {
            // Dropping the controller turns the radio off.
            Ok(controller) => credentials = stay_joined(controller, &network).await,
            Err(error) => {
                WIFI.set_radio(Radio::Off);
                let retry = RETRY.as_secs();
                log::error!("the radio didn't start ({error:?}); trying again in {retry} s");
                if let Either::Second(new) = select(Timer::after(RETRY), WIFI.credentials()).await {
                    credentials = new;
                }
            }
        }
    }
}

/// Joins `network` and stays joined, joining again every [`RETRY`] after
/// a failure or a drop, until the board hands over new credentials.
/// Returns them once the Rig has left.
async fn stay_joined(
    mut controller: WifiController<'_>,
    network: &Credentials,
) -> Option<Credentials> {
    let ssid = network.ssid();
    let retry = RETRY.as_secs();
    loop {
        WIFI.set_radio(Radio::Joining);
        let joined = select(controller.connect_async(), WIFI.credentials()).await;
        match joined {
            Either::First(Ok(_)) => {
                WIFI.set_radio(Radio::Joined);
                log::info!("joined {ssid}");
                log_heap();
                let dropped =
                    select(controller.wait_for_disconnect_async(), WIFI.credentials()).await;
                match dropped {
                    Either::First(_) => log::warn!("lost {ssid}; joining again in {retry} s"),
                    Either::Second(new) => return leave(controller, new).await,
                }
            }
            Either::First(Err(error)) => {
                log::warn!("couldn't join {ssid} ({error:?}); trying again in {retry} s");
            }
            Either::Second(new) => return leave(controller, new).await,
        }
        WIFI.set_radio(Radio::Joining);
        if let Either::Second(new) = select(Timer::after(RETRY), WIFI.credentials()).await {
            return leave(controller, new).await;
        }
    }
}

/// Leaves the network once the reply to the change has had time to go
/// out. Returns `credentials`, the new ones.
async fn leave(
    mut controller: WifiController<'_>,
    credentials: Option<Credentials>,
) -> Option<Credentials> {
    Timer::after(LEAVE_DELAY).await;
    log::info!("leaving the network for new credentials");
    // Fails only when the Rig isn't on a network or joining one.
    let _ = controller.disconnect_async().await;
    credentials
}

/// The station configuration that joins `network` with WPA2-Personal.
fn station(network: &Credentials) -> Config {
    let ssid = network.ssid().try_into().expect("SSIDs are 1 to 32 bytes");
    let passphrase = network.passphrase().try_into().expect("passphrases are 8 to 63 bytes");
    Config::Station(
        StationConfig::default()
            .with_ssid(ssid)
            .with_authentication(AuthenticationMethodConfig::Wpa2Personal(passphrase)),
    )
}
