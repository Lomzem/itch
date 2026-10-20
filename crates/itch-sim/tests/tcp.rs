use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use itch_core::frame::{FrameDecoder, MAX_FRAME, MAX_PAYLOAD, encode_frame};
use itch_core::rig::SETTLE_MS;
use itch_core::wire::{
    AddressMode, ErrorCode, Event, HelloReply, Link, Lock, LogEvent, LogLevel, Message, Reply,
    Request, Response, SetAddress, SetLabel, SetWifi, StatusReply, WriteChannels,
};

#[test]
fn answers_hello_over_tcp() {
    let sim = Sim::start();
    let hello = hello(&mut sim.connect(), 7);
    assert_eq!(hello.protocol, 3);
    assert_eq!(hello.channel_count, 16);
    assert_eq!(hello.board_revision, "sim");
    assert!(hello.label.starts_with("itch-"), "{}", hello.label);
}

#[test]
fn channel_state_survives_disconnects_and_is_shared_between_clients() {
    let sim = Sim::start();
    let mut first = sim.connect();
    let mut other = sim.connect();
    assert_eq!(
        exchange(
            &mut first,
            1,
            Request::WriteChannels(WriteChannels { holder: "", mask: 0x0001, values: 0x0001 }),
        ),
        Response::Ok(Reply::WriteChannels { state: 0x0001 })
    );
    assert_eq!(
        exchange(
            &mut other,
            1,
            Request::WriteChannels(WriteChannels { holder: "", mask: 0x8000, values: 0x8000 }),
        ),
        Response::Ok(Reply::WriteChannels { state: 0x8001 })
    );
    assert_eq!(
        exchange(&mut first, 2, Request::GetChannels),
        Response::Ok(Reply::GetChannels { state: 0x8001 })
    );
    disconnect(first);
    assert_eq!(
        exchange(&mut other, 2, Request::GetChannels),
        Response::Ok(Reply::GetChannels { state: 0x8001 })
    );
    disconnect(other);

    let mut reconnected = sim.connect();
    assert_eq!(
        exchange(&mut reconnected, 1, Request::GetChannels),
        Response::Ok(Reply::GetChannels { state: 0x8001 })
    );
    assert_eq!(
        exchange(&mut reconnected, 2, Request::Clear { holder: "" }),
        Response::Ok(Reply::Clear { state: 0 })
    );
    assert_eq!(
        exchange(&mut sim.connect(), 1, Request::GetChannels),
        Response::Ok(Reply::GetChannels { state: 0 })
    );
}

#[test]
fn a_fifth_connection_is_closed_at_once_until_another_ends() {
    let sim = Sim::start();
    let mut open: Vec<TcpStream> = (0..4).map(|_| sim.connect()).collect();
    for (request_id, stream) in (1..).zip(&mut open) {
        hello(stream, request_id);
    }

    let mut fifth = sim.connect();
    // The sim may close it before or after this write arrives.
    let _ = fifth.write_all(&request(5, Request::Hello));
    match fifth.read(&mut [0; MAX_FRAME]) {
        Ok(0) => {}
        Err(err)
            if matches!(err.kind(), ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted) => {}
        other => panic!("the fifth connection was served: {other:?}"),
    }

    disconnect(open.pop().unwrap());
    hello(&mut sim.connect(), 6);
}

#[test]
fn sends_logs_to_a_connection_that_opted_in() {
    let sim = Sim::start();
    let mut watcher = sim.connect();
    set_log_level(&mut watcher, LogLevel::Info);
    let mut other = sim.connect();
    hello(&mut other, 1);
    let log = next_log(&mut watcher);
    assert_eq!(log.level, LogLevel::Info);
    assert!(log.text.contains("connected"), "{}", log.text);
}

#[test]
fn logs_errors() {
    let sim = Sim::start();
    let mut watcher = sim.connect();
    set_log_level(&mut watcher, LogLevel::Warn);
    let mut other = sim.connect();
    other.write_all(&frame(&[0x3F, 1, 0, 0, 0])).unwrap();
    let log = next_log(&mut watcher);
    assert_eq!(log.level, LogLevel::Warn);
    assert!(log.text.contains("UnknownRequest"), "{}", log.text);
}

#[test]
fn sends_no_logs_by_default() {
    let sim = Sim::start();
    let mut quiet = sim.connect();
    let mut other = sim.connect();
    hello(&mut other, 1);
    // Any queued Log Event would arrive before this reply.
    hello(&mut quiet, 2);
}

#[test]
fn a_write_replies_after_the_contacts_settle() {
    let sim = Sim::start();
    let mut stream = sim.connect();
    let write = WriteChannels { holder: "", mask: 0x8001, values: 0x8001 };
    let started = Instant::now();
    let reply = exchange(&mut stream, 3, Request::WriteChannels(write));
    assert!(started.elapsed() >= Duration::from_millis(SETTLE_MS));
    assert_eq!(reply, Response::Ok(Reply::WriteChannels { state: 0x8001 }));
}

#[test]
fn status_counts_bad_frames_from_every_connection() {
    let sim = Sim::start();
    let mut garbled = sim.connect();
    // A Hello frame with a bad CRC, then a frame whose payload is under 5 bytes.
    garbled
        .write_all(&[0x00, 0x02, 0x05, 0x08, 0x01, 0x78, 0x56, 0x34, 0x12, 0xc2, 0xf3, 0x00])
        .unwrap();
    let mut short = [0; MAX_FRAME];
    let len = encode_frame(&[0x05, 0x78, 0x56, 0x34], &mut short);
    garbled.write_all(&short[..len]).unwrap();
    hello(&mut garbled, 1);

    let Response::Ok(Reply::GetStatus(status)) =
        exchange(&mut sim.connect(), 2, Request::GetStatus)
    else {
        panic!("not a GetStatus reply");
    };
    assert_eq!(
        StatusReply { uptime_ms: 0, ..status },
        StatusReply {
            uptime_ms: 0,
            reset_reason: 0x01,
            last_panic: "",
            crc_failures: 1,
            oversized_frames: 0,
            malformed_frames: 1,
            dropped_logs: 0,
            expander_faults: 0,
            usb_tx_dropped: 0,
            holder: "",
            wifi_configured: false,
            ssid: "",
            link: Link::Off,
            current_address: [0; 4],
            mode: AddressMode::Dhcp,
            address: [0; 4],
            prefix: 0,
            gateway: [0; 4],
        }
    );
}

#[test]
fn a_hardware_fault_holds_every_channel_off_and_refuses_writes() {
    let sim = Sim::start_with(&["--hardware-fault"]);
    let mut stream = sim.connect();
    let write = WriteChannels { holder: "", mask: 0x8001, values: 0x8001 };
    let reply = exchange(&mut stream, 1, Request::WriteChannels(write));
    assert!(matches!(reply, Response::Err { error: ErrorCode::HardwareFault, .. }), "{reply:?}");
    let reply = exchange(&mut stream, 2, Request::GetChannels);
    assert_eq!(reply, Response::Ok(Reply::GetChannels { state: 0 }));
}

#[test]
fn a_hardware_fault_is_found_by_the_idle_tick() {
    let sim = Sim::start_with(&["--hardware-fault"]);
    let mut stream = sim.connect();
    let started = Instant::now();
    loop {
        let Response::Ok(Reply::GetStatus(status)) = exchange(&mut stream, 1, Request::GetStatus)
        else {
            panic!("not a GetStatus reply");
        };
        if status.expander_faults == 1 {
            return;
        }
        assert!(started.elapsed() < Duration::from_secs(2), "no tick found the fault");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_label_lasts_for_the_run_unless_storage_faults() {
    let sim = Sim::start();
    let set = |label| Request::SetLabel(SetLabel { holder: "", label });
    exchange(&mut sim.connect(), 1, set("bench3"));
    assert_eq!(hello(&mut sim.connect(), 2).label, "bench3");

    let faulty = Sim::start_with(&["--storage-fault"]);
    let Response::Err { error, .. } = exchange(&mut faulty.connect(), 1, set("bench3")) else {
        panic!("the label was saved");
    };
    assert_eq!(error, ErrorCode::StorageFault);
}

#[test]
fn a_lock_keeps_other_connections_from_changing_the_rig_until_taken_over() {
    let sim = Sim::start();
    let mut alice = sim.connect();
    let mut bob = sim.connect();
    let write =
        |holder, values| Request::WriteChannels(WriteChannels { holder, mask: 0x0003, values });
    let lock = |holder, takeover| Request::Lock(Lock { holder, takeover });
    let locked = |detail| Response::Err { request_type: 0x03, error: ErrorCode::Locked, detail };

    assert_eq!(exchange(&mut alice, 1, lock("alice", false)), Response::Ok(Reply::Lock));
    assert_eq!(exchange(&mut bob, 1, write("bob", 0x0002)), locked("alice"));
    assert_eq!(exchange(&mut bob, 2, lock("bob", true)), Response::Ok(Reply::Lock));
    assert_eq!(exchange(&mut alice, 2, write("alice", 0x0001)), locked("bob"));
    let release = Request::ReleaseLock { holder: "bob" };
    assert_eq!(exchange(&mut bob, 3, release), Response::Ok(Reply::ReleaseLock));
    assert_eq!(
        exchange(&mut alice, 3, write("alice", 0x0001)),
        Response::Ok(Reply::WriteChannels { state: 0x0001 })
    );
}

const SSID: &str = "ExampleRigNet";

fn set_wifi<'a>(holder: &'a str, ssid: &'a str, passphrase: &'a str) -> Request<'a> {
    Request::SetWifi(SetWifi { holder, ssid, passphrase })
}

/// What `GetStatus` reports of the network: credentials, SSID, link and
/// current address.
fn network(stream: &mut TcpStream) -> (bool, &'static str, Link, [u8; 4]) {
    match exchange(stream, 9, Request::GetStatus) {
        Response::Ok(Reply::GetStatus(status)) => {
            (status.wifi_configured, status.ssid, status.link, status.current_address)
        }
        other => panic!("not a GetStatus reply: {other:?}"),
    }
}

#[test]
fn wifi_credentials_last_for_the_run_and_connect_at_the_bound_address() {
    let sim = Sim::start();
    let off = (false, "", Link::Off, [0; 4]);
    assert_eq!(network(&mut sim.connect()), off);

    let set = set_wifi("", SSID, "example-passphrase");
    assert_eq!(exchange(&mut sim.connect(), 1, set), Response::Ok(Reply::SetWifi));
    assert_eq!(network(&mut sim.connect()), (true, SSID, Link::Connected, [127, 0, 0, 1]));

    assert_eq!(exchange(&mut sim.connect(), 2, set_wifi("", "", "")), Response::Ok(Reply::SetWifi));
    assert_eq!(network(&mut sim.connect()), off);
}

/// Bound to every address, the sim has no one address to report, so it is
/// like a Rig still waiting for its DHCP lease.
#[test]
fn a_sim_bound_to_no_one_address_reports_joining_with_dhcp() {
    let sim = Sim::start_with(&["--bind", "0.0.0.0:0"]);
    let set = set_wifi("", SSID, "example-passphrase");
    assert_eq!(exchange(&mut sim.connect(), 1, set), Response::Ok(Reply::SetWifi));
    assert_eq!(network(&mut sim.connect()), (true, SSID, Link::Joining, [0; 4]));
}

#[test]
fn wifi_credentials_out_of_range_are_malformed() {
    let sim = Sim::start();
    let mut stream = sim.connect();
    for (ssid, passphrase) in [(SSID, "short"), ("", "example-passphrase")] {
        let Response::Err { error, .. } = exchange(&mut stream, 1, set_wifi("", ssid, passphrase))
        else {
            panic!("{ssid:?} with {passphrase:?} was accepted");
        };
        assert_eq!(error, ErrorCode::Malformed);
    }
    assert_eq!(network(&mut stream), (false, "", Link::Off, [0; 4]));
}

#[test]
fn wifi_credentials_are_refused_on_a_storage_fault_or_by_the_lock() {
    let faulty = Sim::start_with(&["--storage-fault"]);
    let mut stream = faulty.connect();
    let Response::Err { error, .. } =
        exchange(&mut stream, 1, set_wifi("", SSID, "example-passphrase"))
    else {
        panic!("the credentials were saved");
    };
    assert_eq!(error, ErrorCode::StorageFault);
    assert!(!network(&mut stream).0);

    let sim = Sim::start();
    let mut alice = sim.connect();
    let lock = Request::Lock(Lock { holder: "alice", takeover: false });
    assert_eq!(exchange(&mut alice, 1, lock), Response::Ok(Reply::Lock));
    assert_eq!(
        exchange(&mut sim.connect(), 1, set_wifi("bob", SSID, "example-passphrase")),
        Response::Err { request_type: 0x0D, error: ErrorCode::Locked, detail: "alice" }
    );
    assert!(!network(&mut alice).0);
}

/// A made-up static address from the documentation range.
const STATIC: [u8; 4] = [192, 0, 2, 10];
const GATEWAY: [u8; 4] = [192, 0, 2, 1];

fn set_address(
    holder: &str,
    mode: AddressMode,
    address: [u8; 4],
    prefix: u8,
    gateway: [u8; 4],
) -> Request<'_> {
    Request::SetAddress(SetAddress { holder, mode, address, prefix, gateway })
}

fn set_static(holder: &str) -> Request<'_> {
    set_address(holder, AddressMode::Static, STATIC, 24, GATEWAY)
}

fn set_dhcp() -> Request<'static> {
    set_address("", AddressMode::Dhcp, [0; 4], 0, [0; 4])
}

/// An address configuration as `GetStatus` reports it: mode, address,
/// prefix and gateway.
type Addressing = (AddressMode, [u8; 4], u8, [u8; 4]);

/// What `GetStatus` reports of the address: the configuration, and the
/// address the Rig has.
fn addressing(stream: &mut TcpStream) -> (Addressing, [u8; 4]) {
    match exchange(stream, 9, Request::GetStatus) {
        Response::Ok(Reply::GetStatus(status)) => {
            ((status.mode, status.address, status.prefix, status.gateway), status.current_address)
        }
        other => panic!("not a GetStatus reply: {other:?}"),
    }
}

#[test]
fn a_static_address_lasts_for_the_run_and_is_the_address_the_sim_reports() {
    let sim = Sim::start();
    let dhcp = (AddressMode::Dhcp, [0; 4], 0, [0; 4]);
    let configured = (AddressMode::Static, STATIC, 24, GATEWAY);
    assert_eq!(addressing(&mut sim.connect()), (dhcp, [0; 4]));

    assert_eq!(exchange(&mut sim.connect(), 1, set_static("")), Response::Ok(Reply::SetAddress));
    // Static but not connected: no credentials yet.
    assert_eq!(addressing(&mut sim.connect()), (configured, [0; 4]));

    exchange(&mut sim.connect(), 2, set_wifi("", SSID, "example-passphrase"));
    assert_eq!(addressing(&mut sim.connect()), (configured, STATIC));

    assert_eq!(exchange(&mut sim.connect(), 3, set_dhcp()), Response::Ok(Reply::SetAddress));
    assert_eq!(addressing(&mut sim.connect()), (dhcp, [127, 0, 0, 1]));
}

#[test]
fn an_address_out_of_range_is_malformed() {
    let sim = Sim::start();
    let mut stream = sim.connect();
    for request in [
        set_address("", AddressMode::Static, STATIC, 33, GATEWAY),
        set_address("", AddressMode::Static, [0; 4], 24, GATEWAY),
        set_address("", AddressMode::Dhcp, STATIC, 0, [0; 4]),
    ] {
        let Response::Err { error, .. } = exchange(&mut stream, 1, request) else {
            panic!("{request:?} was accepted");
        };
        assert_eq!(error, ErrorCode::Malformed);
    }
    assert_eq!(addressing(&mut stream).0.0, AddressMode::Dhcp);
}

#[test]
fn an_address_is_refused_on_a_storage_fault_or_by_the_lock() {
    let faulty = Sim::start_with(&["--storage-fault"]);
    let mut stream = faulty.connect();
    let Response::Err { error, .. } = exchange(&mut stream, 1, set_static("")) else {
        panic!("the address was saved");
    };
    assert_eq!(error, ErrorCode::StorageFault);
    assert_eq!(addressing(&mut stream).0.0, AddressMode::Dhcp);

    let sim = Sim::start();
    let mut alice = sim.connect();
    let lock = Request::Lock(Lock { holder: "alice", takeover: false });
    assert_eq!(exchange(&mut alice, 1, lock), Response::Ok(Reply::Lock));
    assert_eq!(
        exchange(&mut sim.connect(), 1, set_static("bob")),
        Response::Err { request_type: 0x0E, error: ErrorCode::Locked, detail: "alice" }
    );
    assert_eq!(addressing(&mut alice).0.0, AddressMode::Dhcp);
}

struct Sim {
    child: Child,
    url: String,
}

impl Sim {
    fn start() -> Self {
        Self::start_with(&[])
    }

    fn start_with(args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_itch-sim"))
            .args(args)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        let url = line.trim().strip_prefix("itch-sim listening on ").unwrap().to_owned();
        Self { child, url }
    }

    fn connect(&self) -> TcpStream {
        let mut addr: SocketAddr = self.url.strip_prefix("tcp://").unwrap().parse().unwrap();
        // Windows refuses to connect to 0.0.0.0, which other systems take as this host.
        if addr.ip().is_unspecified() {
            addr.set_ip(Ipv4Addr::LOCALHOST.into());
        }
        let stream = TcpStream::connect(addr).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        stream
    }
}

impl Drop for Sim {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn hello(stream: &mut TcpStream, request_id: u32) -> HelloReply<'static> {
    match exchange(stream, request_id, Request::Hello) {
        Response::Ok(Reply::Hello(hello)) => hello,
        other => panic!("not a Hello reply: {other:?}"),
    }
}

/// Sends `request` and returns the response to it.
fn exchange(stream: &mut TcpStream, request_id: u32, request: Request) -> Response<'static> {
    stream.write_all(&self::request(request_id, request)).unwrap();
    match receive(stream) {
        Message::Response { request_id: id, response } if id == request_id => response,
        other => panic!("not a response to {request_id}: {other:?}"),
    }
}

fn set_log_level(stream: &mut TcpStream, level: LogLevel) {
    match exchange(stream, 1, Request::SetLogLevel { level }) {
        Response::Ok(Reply::SetLogLevel) => {}
        other => panic!("not a SetLogLevel reply: {other:?}"),
    }
}

fn next_log(stream: &mut TcpStream) -> LogEvent<'static> {
    match receive(stream) {
        Message::Event(Event::Log(log)) => log,
        other => panic!("not a Log Event: {other:?}"),
    }
}

/// Waits for the server to finish this connection before checking retained state.
fn disconnect(mut stream: TcpStream) {
    stream.shutdown(Shutdown::Write).unwrap();
    assert_eq!(stream.read(&mut [0]).unwrap(), 0);
}

fn request(request_id: u32, request: Request) -> Vec<u8> {
    let mut payload = [0; MAX_PAYLOAD];
    let len = Message::Request { request_id, request }.encode(&mut payload);
    frame(&payload[..len])
}

fn frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = [0; MAX_FRAME];
    let len = encode_frame(payload, &mut frame);
    frame[..len].to_vec()
}

fn receive(stream: &mut TcpStream) -> Message<'static> {
    let mut decoder = FrameDecoder::new();
    let payload = loop {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        if let Some(Ok(payload)) = decoder.push(byte[0]) {
            break payload.to_vec().leak();
        }
    };
    Message::decode(payload).unwrap()
}
