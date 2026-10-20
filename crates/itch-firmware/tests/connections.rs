//! The connection core: the Rig task serving several connections through the
//! library's scheduling loop, on the bench.

mod common;

use std::future::Future;
use std::pin::{Pin, pin};
use std::task::Poll;

use common::{Bench, BenchClock, STATIC, advance};
use embassy_futures::poll_once;
use itch_core::frame::{FrameDecoder, MAX_FRAME, MAX_PAYLOAD};
use itch_core::rig::SETTLE_MS;
use itch_core::wifi::{Addressing, Credentials, StaticAddress};
use itch_core::wire::{
    Event, LogLevel, Message, Request, Response, SetAddress, SetLabel, SetWifi, WriteChannels,
};
use itch_firmware::connections::{Connection, Connections, DEPTH};
use itch_firmware::serve::{REPLY_TIMEOUT_MS, TICK_MS, serve};
use itch_firmware::wifi::Wifi;
use log::Level;

// Links the host's critical section, which the connections lock with.
use critical_section as _;

/// A connection core of its own, with `N` connections.
fn connections<const N: usize>() -> &'static Connections<N> {
    Box::leak(Box::new(Connections::new()))
}

/// Sends `request` from `connection`, as its transport's reader does.
fn send(connection: Connection, request_id: u32, request: Request) {
    let mut payload = [0; MAX_PAYLOAD];
    let len = Message::Request { request_id, request }.encode(&mut payload);
    connection.receive(Ok(&payload[..len]));
}

/// Lets the Rig task run until it waits.
fn run(rig: Pin<&mut impl Future>) {
    assert!(poll_once(rig).is_pending(), "the Rig task never returns");
}

/// Every frame waiting for `connection`'s writer, in order: `reply <id>`,
/// `<error> <id>` or `log <text>`.
fn drain(connection: Connection) -> Vec<String> {
    let mut out = vec![];
    let mut frame = [0; MAX_FRAME];
    while let Poll::Ready(len) = poll_once(connection.next_frame(&mut frame)) {
        let mut decoder = FrameDecoder::new();
        let payload: Vec<u8> = frame[..len]
            .iter()
            .find_map(|&byte| decoder.push(byte)?.ok().map(<[u8]>::to_vec))
            .expect("a whole frame");
        out.push(match Message::decode(&payload).expect("a valid message") {
            Message::Response { request_id, response: Response::Ok(_) } => {
                format!("reply {request_id}")
            }
            Message::Response { request_id, response: Response::Err { error, .. } } => {
                format!("{error:?} {request_id}")
            }
            Message::Event(Event::Log(log)) => format!("log {}", log.text),
            message => panic!("not for a client: {message:?}"),
        });
    }
    out
}

#[test]
fn each_reply_goes_only_to_the_connection_that_sent_the_request() {
    let mut bench = Bench::boot();
    let connections = connections::<2>();
    let (a, b) = (connections.connect(0), connections.connect(1));
    let mut rig = pin!(serve(&mut bench.rig, &mut bench.board, connections, &BenchClock));

    send(a, 1, Request::Hello);
    send(b, 2, Request::GetChannels);
    send(a, 3, Request::GetStatus);
    run(rig.as_mut());

    assert_eq!(drain(a), ["reply 1", "reply 3"]);
    assert_eq!(drain(b), ["reply 2"]);
}

#[test]
fn each_connection_gets_log_events_at_its_own_level_and_keeps_its_own_backlog() {
    let mut bench = Bench::boot();
    let connections = connections::<2>();
    let (a, b) = (connections.connect(0), connections.connect(1));
    let mut rig = pin!(serve(&mut bench.rig, &mut bench.board, connections, &BenchClock));

    send(a, 1, Request::SetLogLevel { level: LogLevel::Debug });
    run(rig.as_mut());
    assert_eq!(drain(a), ["reply 1"]);
    connections.log(Level::Info, format_args!("info"));
    assert!(drain(b).is_empty(), "logs start off");

    // B choosing a level touches neither A's level nor its backlog.
    send(b, 2, Request::SetLogLevel { level: LogLevel::Warn });
    run(rig.as_mut());
    connections.log(Level::Info, format_args!("info for A only"));
    connections.log(Level::Warn, format_args!("warning"));
    assert_eq!(drain(a), ["log info", "log info for A only", "log warning"]);
    assert_eq!(drain(b), ["reply 2", "log warning"]);
}

#[test]
fn a_reply_goes_out_before_log_events_queued_ahead_of_it() {
    let mut bench = Bench::boot();
    let connections = connections::<1>();
    let usb = connections.connect(0);
    let mut rig = pin!(serve(&mut bench.rig, &mut bench.board, connections, &BenchClock));

    send(usb, 1, Request::SetLogLevel { level: LogLevel::Info });
    run(rig.as_mut());
    assert_eq!(drain(usb), ["reply 1"]);
    connections.log(Level::Info, format_args!("first"));
    send(usb, 2, Request::Hello);
    run(rig.as_mut());
    assert_eq!(drain(usb), ["reply 2", "log first"]);
}

#[test]
fn a_level_change_drops_the_backlog_and_filters_later_events() {
    let mut bench = Bench::boot();
    let connections = connections::<1>();
    let usb = connections.connect(0);
    let mut rig = pin!(serve(&mut bench.rig, &mut bench.board, connections, &BenchClock));

    send(usb, 1, Request::SetLogLevel { level: LogLevel::Debug });
    run(rig.as_mut());
    connections.log(Level::Info, format_args!("backlog"));
    send(usb, 2, Request::SetLogLevel { level: LogLevel::Warn });
    run(rig.as_mut());
    connections.log(Level::Info, format_args!("filtered"));
    connections.log(Level::Error, format_args!("current"));
    assert_eq!(drain(usb), ["reply 1", "reply 2", "log current"]);
}

#[test]
fn a_disconnect_drops_that_connections_replies_and_log_events_only() {
    let mut bench = Bench::boot();
    let connections = connections::<2>();
    let (a, b) = (connections.connect(0), connections.connect(1));
    let mut rig = pin!(serve(&mut bench.rig, &mut bench.board, connections, &BenchClock));
    send(a, 1, Request::SetLogLevel { level: LogLevel::Info });
    send(b, 2, Request::SetLogLevel { level: LogLevel::Info });
    run(rig.as_mut());
    connections.log(Level::Info, format_args!("pending"));
    // One request answered but not yet sent, one still queued for the Rig.
    send(a, 3, Request::Hello);
    run(rig.as_mut());
    send(a, 4, Request::Hello);

    a.disconnect();
    run(rig.as_mut());
    let next = connections.connect(0);
    connections.log(Level::Error, format_args!("after"));

    assert!(drain(next).is_empty(), "nothing meant for the earlier connection");
    assert_eq!(drain(b), ["reply 2", "log pending", "log after"]);
}

#[test]
fn a_reply_waiting_for_room_is_dropped_when_its_connection_ends() {
    let mut bench = Bench::boot();
    let connections = connections::<1>();
    let usb = connections.connect(0);
    let mut rig = pin!(serve(&mut bench.rig, &mut bench.board, connections, &BenchClock));
    for request_id in 1..=4 {
        send(usb, request_id, Request::Hello);
    }
    run(rig.as_mut());

    usb.disconnect();
    let next = connections.connect(0);
    run(rig.as_mut());
    send(next, 5, Request::Hello);
    run(rig.as_mut());
    assert_eq!(drain(next), ["reply 5"]);
}

#[test]
fn a_reply_waits_for_room_until_the_writer_takes_one() {
    let mut bench = Bench::boot();
    let connections = connections::<1>();
    let usb = connections.connect(0);
    let mut rig = pin!(serve(&mut bench.rig, &mut bench.board, connections, &BenchClock));
    for request_id in 1..=3 {
        send(usb, request_id, Request::Hello);
    }
    run(rig.as_mut());
    assert_eq!(drain(usb), ["reply 1", "reply 2"]);

    run(rig.as_mut());
    assert_eq!(drain(usb), ["reply 3"]);
}

#[test]
fn a_connection_that_stops_reading_holds_up_the_others_only_briefly() {
    let mut bench = Bench::boot();
    let connections = connections::<2>();
    let (stalled, b) = (connections.connect(0), connections.connect(1));
    let mut rig = pin!(serve(&mut bench.rig, &mut bench.board, connections, &BenchClock));
    for request_id in 1..=3 {
        send(stalled, request_id, Request::Hello);
    }
    send(b, 4, Request::Hello);
    run(rig.as_mut());
    assert!(drain(b).is_empty());

    advance(REPLY_TIMEOUT_MS);
    run(rig.as_mut());
    assert_eq!(drain(b), ["reply 4"]);
    assert_eq!(drain(stalled), ["reply 1", "reply 2"], "the third reply is dropped");
}

/// A request that finds the shared queue full is answered `Busy` at once,
/// unless it has no `request_id` to answer by.
#[test]
fn a_request_that_finds_the_queue_full_is_answered_busy() {
    let mut bench = Bench::boot();
    let connections = connections::<3>();
    let (a, b, c) = (connections.connect(0), connections.connect(1), connections.connect(2));
    let mut rig = pin!(serve(&mut bench.rig, &mut bench.board, connections, &BenchClock));
    for request_id in 1..=DEPTH as u32 {
        send(if request_id % 2 == 1 { a } else { b }, request_id, Request::Hello);
    }

    send(c, 0x1234_5678, Request::GetChannels);
    // GetChannels cut short before the end of its `request_id`.
    c.receive(Ok(&[0x02, 0x78, 0x56, 0x34]));
    assert_eq!(drain(c), ["Busy 305419896"]);

    run(rig.as_mut());
    assert_eq!(drain(a), ["reply 1", "reply 3"]);
    assert_eq!(drain(b), ["reply 2", "reply 4"]);
    assert!(drain(c).is_empty());
}

#[test]
fn a_channel_write_is_answered_once_the_contacts_have_settled() {
    let mut bench = Bench::boot();
    let connections = connections::<1>();
    let usb = connections.connect(0);
    let mut rig = pin!(serve(&mut bench.rig, &mut bench.board, connections, &BenchClock));

    send(
        usb,
        1,
        Request::WriteChannels(WriteChannels { holder: "", mask: 0x0001, values: 0x0001 }),
    );
    run(rig.as_mut());
    assert_eq!(bench.expander.borrow().driven(), (0x00, 0x80), "CH1 on");
    advance(SETTLE_MS - 1);
    run(rig.as_mut());
    assert!(drain(usb).is_empty());

    advance(1);
    run(rig.as_mut());
    assert_eq!(drain(usb), ["reply 1"]);
}

#[test]
fn the_idle_tick_restores_channels_an_expander_lost() {
    let mut bench = Bench::boot();
    let connections = connections::<1>();
    let usb = connections.connect(0);
    let mut rig = pin!(serve(&mut bench.rig, &mut bench.board, connections, &BenchClock));
    send(
        usb,
        1,
        Request::WriteChannels(WriteChannels { holder: "", mask: 0x0001, values: 0x0001 }),
    );
    run(rig.as_mut());
    advance(SETTLE_MS);
    run(rig.as_mut());

    bench.expander.borrow_mut().brown_out();
    advance(TICK_MS - SETTLE_MS - 1);
    run(rig.as_mut());
    assert_eq!(bench.expander.borrow().driven(), (0x00, 0x00), "not ticked yet");

    advance(1);
    run(rig.as_mut());
    assert_eq!(bench.expander.borrow().driven(), (0x00, 0x80), "CH1 on again");
}

const SSID: &str = "ExampleRigNet";
const PASSPHRASE: &str = "example-passphrase";

fn set_wifi<'a>(ssid: &'a str, passphrase: &'a str) -> Request<'a> {
    Request::SetWifi(SetWifi { holder: "", ssid, passphrase })
}

/// The credentials handed to the radio since it last looked: `None` if
/// none were, `Some(None)` if they were cleared.
fn handed_to_radio(wifi: &Wifi) -> Option<Option<Credentials>> {
    match poll_once(wifi.credentials()) {
        Poll::Ready(credentials) => Some(credentials),
        Poll::Pending => None,
    }
}

/// A client on Wi-Fi learns its change was saved before the Rig leaves
/// the network.
#[test]
fn set_wifi_replies_before_the_radio_hears_of_the_new_credentials() {
    let mut bench = Bench::boot();
    let connections = connections::<1>();
    let tcp = connections.connect(0);
    let mut rig = pin!(serve(&mut bench.rig, &mut bench.board, connections, &BenchClock));
    // Two replies the client hasn't read yet keep the third waiting.
    send(tcp, 1, Request::Hello);
    send(tcp, 2, Request::Hello);
    send(tcp, 3, set_wifi(SSID, PASSPHRASE));
    run(rig.as_mut());
    assert_eq!(handed_to_radio(bench.wifi), None);

    assert_eq!(drain(tcp), ["reply 1", "reply 2"]);
    run(rig.as_mut());
    assert_eq!(drain(tcp), ["reply 3"]);
    assert_eq!(handed_to_radio(bench.wifi), Some(Credentials::new(SSID, PASSPHRASE)));
}

#[test]
fn cleared_wifi_credentials_reach_the_radio_as_none() {
    let mut bench = Bench::boot();
    let connections = connections::<1>();
    let usb = connections.connect(0);
    let mut rig = pin!(serve(&mut bench.rig, &mut bench.board, connections, &BenchClock));
    send(usb, 1, set_wifi(SSID, PASSPHRASE));
    send(usb, 2, set_wifi("", ""));
    run(rig.as_mut());

    assert_eq!(drain(usb), ["reply 1", "reply 2"]);
    assert_eq!(handed_to_radio(bench.wifi), Some(None));
}

#[test]
fn a_rename_or_a_failed_save_leaves_the_radio_alone() {
    let mut bench = Bench::boot();
    let connections = connections::<1>();
    let usb = connections.connect(0);
    bench.flash.borrow_mut().broken = true;
    let mut rig = pin!(serve(&mut bench.rig, &mut bench.board, connections, &BenchClock));
    send(usb, 1, set_wifi(SSID, PASSPHRASE));
    run(rig.as_mut());
    assert_eq!(drain(usb), ["StorageFault 1"]);

    bench.flash.borrow_mut().broken = false;
    send(usb, 2, Request::SetLabel(SetLabel { holder: "", label: "bench-3" }));
    run(rig.as_mut());
    assert_eq!(drain(usb), ["reply 2"]);
    assert_eq!(handed_to_radio(bench.wifi), None);
}

fn set_static() -> Request<'static> {
    let (mode, address, prefix, gateway) = STATIC;
    Request::SetAddress(SetAddress { holder: "", mode, address, prefix, gateway })
}

/// The address configuration handed to the network stack since it last
/// looked. `None` if none was.
fn handed_to_stack(wifi: &Wifi) -> Option<Addressing> {
    match poll_once(wifi.addressing()) {
        Poll::Ready(addressing) => Some(addressing),
        Poll::Pending => None,
    }
}

/// A client on Wi-Fi learns its change was saved before the Rig's address
/// changes.
#[test]
fn set_address_replies_before_the_network_stack_hears_of_it() {
    let mut bench = Bench::boot();
    let connections = connections::<1>();
    let tcp = connections.connect(0);
    let mut rig = pin!(serve(&mut bench.rig, &mut bench.board, connections, &BenchClock));
    // Two replies the client hasn't read yet keep the third waiting.
    send(tcp, 1, Request::Hello);
    send(tcp, 2, Request::Hello);
    send(tcp, 3, set_static());
    run(rig.as_mut());
    assert_eq!(handed_to_stack(bench.wifi), None);

    assert_eq!(drain(tcp), ["reply 1", "reply 2"]);
    run(rig.as_mut());
    assert_eq!(drain(tcp), ["reply 3"]);
    let (_, address, prefix, gateway) = STATIC;
    let expected = Addressing::Static(StaticAddress::new(address, prefix, gateway).unwrap());
    assert_eq!(handed_to_stack(bench.wifi), Some(expected));
    // A change of address leaves the radio on its network.
    assert_eq!(handed_to_radio(bench.wifi), None);
}

#[test]
fn an_address_change_the_flash_fails_to_save_leaves_the_network_stack_alone() {
    let mut bench = Bench::boot();
    let connections = connections::<1>();
    let usb = connections.connect(0);
    bench.flash.borrow_mut().broken = true;
    let mut rig = pin!(serve(&mut bench.rig, &mut bench.board, connections, &BenchClock));
    send(usb, 1, set_static());
    run(rig.as_mut());
    assert_eq!(drain(usb), ["StorageFault 1"]);
    assert_eq!(handed_to_stack(bench.wifi), None);
}
