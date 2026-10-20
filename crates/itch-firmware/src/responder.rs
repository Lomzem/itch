//! The mDNS responder: while the Rig is connected it announces the
//! [`Advertisement`] and answers queries for it, on whatever address the
//! network stack has, from DHCP or static. Whenever the Advertisement
//! changes it starts again, announcing the new one; after a rename it first
//! withdraws the old instance, so browsers drop the old Label at once.

use core::net::Ipv4Addr;

use edge_mdns::buf::VecBufAccess;
use edge_mdns::io::{self, IPV4_DEFAULT_SOCKET, Mdns};
use edge_mdns::{HostAnswersMdnsHandler, MdnsHandler, MdnsRequest, MdnsResponse};
use edge_nal::UdpSplit;
use edge_nal_embassy::{Udp, UdpBuffers};
use embassy_futures::select::{Either, select};
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use esp_hal::rng::Rng;

use crate::mdns::{Advertisement, Answers, Identity};
use crate::wifi::WIFI;

/// How long the responder waits before starting again after a failure.
const RETRY: Duration = Duration::from_secs(5);

/// The largest mDNS message the responder takes or sends: a whole
/// Ethernet frame's payload.
const MESSAGE: usize = 1500;

/// The socket's room for messages each way.
const SOCKET: usize = 1472;

/// Advertises the Rig, whose Hardware ID is `hardware_id`, whenever it is
/// connected. It needs one of the network stack's socket slots.
#[embassy_executor::task]
pub async fn responder(stack: Stack<'static>, hardware_id: [u8; 6]) {
    let identity = Identity::new(hardware_id);
    let sockets = UdpBuffers::<1, SOCKET, SOCKET, 2>::new();
    let udp = Udp::new(stack, &sockets);
    let buffers = Buffers { receive: VecBufAccess::new(), send: VecBufAccess::new() };
    let mut advertisement = WIFI.advertisement();
    loop {
        advertisement = match advertisement {
            None => WIFI.changed_from(None).await,
            Some(advertisement) => advertise(&udp, &buffers, &identity, advertisement).await,
        };
    }
}

/// The responder's working space for one message each way.
struct Buffers {
    receive: VecBufAccess<NoopRawMutex, MESSAGE>,
    send: VecBufAccess<NoopRawMutex, MESSAGE>,
}

/// Announces `advertisement` and answers queries for it until what the Rig
/// advertises changes. Returns the new advertisement.
async fn advertise(
    udp: &Udp<'_>,
    buffers: &Buffers,
    identity: &Identity,
    advertisement: Advertisement,
) -> Option<Advertisement> {
    let any = Some(Ipv4Addr::UNSPECIFIED);
    let mut socket = match io::bind(udp, IPV4_DEFAULT_SOCKET, any, None).await {
        Ok(socket) => socket,
        Err(error) => {
            log::warn!("mDNS didn't start ({error:?}); trying again in {} s", RETRY.as_secs());
            return retry(advertisement).await;
        }
    };
    let (receive, send) = socket.split();
    // The answers change by starting again, never through this.
    let unchanged = Signal::new();
    let mdns = Mdns::<_, _, _, _, _, NoopRawMutex>::new(
        any,
        None,
        receive,
        send,
        &buffers.receive,
        &buffers.send,
        Rng::new(),
        &unchanged,
    );
    log::info!(
        "advertising {} as {}.local at {}",
        advertisement.instance(),
        identity.hostname(),
        Ipv4Addr::from(advertisement.address())
    );
    let answers = HostAnswersMdnsHandler::new(Answers::announce(identity, &advertisement));
    match select(mdns.run(answers), WIFI.changed_from(Some(advertisement))).await {
        Either::First(error) => {
            log::warn!("mDNS stopped ({error:?}); starting again in {} s", RETRY.as_secs());
            retry(advertisement).await
        }
        Either::Second(new) => {
            if new.is_some_and(|new| new.instance() != advertisement.instance()) {
                let mut goodbye =
                    HostAnswersMdnsHandler::new(Answers::goodbye(identity, &advertisement));
                // Best effort: without it the old Label lingers in caches
                // until its records expire.
                let _ = mdns
                    .query(|buffer| match goodbye.handle(MdnsRequest::None, buffer)? {
                        MdnsResponse::Reply { data, .. } => Ok(data.len()),
                        MdnsResponse::None => Ok(0),
                    })
                    .await;
            }
            new
        }
    }
}

/// Waits [`RETRY`], or less if what the Rig advertises changes first.
/// Returns what it advertises then.
async fn retry(advertisement: Advertisement) -> Option<Advertisement> {
    match select(Timer::after(RETRY), WIFI.changed_from(Some(advertisement))).await {
        Either::First(()) => Some(advertisement),
        Either::Second(new) => new,
    }
}
