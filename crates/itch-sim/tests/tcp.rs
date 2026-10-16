use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use itch_core::frame::{FrameDecoder, MAX_FRAME, MAX_PAYLOAD, encode_frame};
use itch_core::wire::{Event, HelloReply, LogEvent, LogLevel, Message, Reply, Request, Response};

#[test]
fn answers_hello_over_tcp() {
    let sim = Sim::start();
    let hello = hello(&mut sim.connect(), 7);
    assert_eq!(hello.protocol, 1);
    assert_eq!(hello.channel_count, 16);
    assert_eq!(hello.board_revision, "sim");
    assert!(hello.label.starts_with("itch-"), "{}", hello.label);
}

#[test]
fn serves_several_connections_at_once() {
    let sim = Sim::start();
    let mut idle = sim.connect();
    let mut other = sim.connect();
    hello(&mut other, 1);
    hello(&mut idle, 2);
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

struct Sim {
    child: Child,
    url: String,
}

impl Sim {
    fn start() -> Self {
        let mut child =
            Command::new(env!("CARGO_BIN_EXE_itch-sim")).stdout(Stdio::piped()).spawn().unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        let url = line.trim().strip_prefix("itch-sim listening on ").unwrap().to_owned();
        Self { child, url }
    }

    fn connect(&self) -> TcpStream {
        let stream = TcpStream::connect(self.url.strip_prefix("tcp://").unwrap()).unwrap();
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
    stream.write_all(&request(request_id, Request::Hello)).unwrap();
    match receive(stream) {
        Message::Response { request_id: id, response: Response::Ok(Reply::Hello(hello)) }
            if id == request_id =>
        {
            hello
        }
        other => panic!("not a Hello reply: {other:?}"),
    }
}

fn set_log_level(stream: &mut TcpStream, level: LogLevel) {
    stream.write_all(&request(1, Request::SetLogLevel { level })).unwrap();
    match receive(stream) {
        Message::Response { response: Response::Ok(Reply::SetLogLevel), .. } => {}
        other => panic!("not a SetLogLevel reply: {other:?}"),
    }
}

fn next_log(stream: &mut TcpStream) -> LogEvent<'static> {
    match receive(stream) {
        Message::Event(Event::Log(log)) => log,
        other => panic!("not a Log Event: {other:?}"),
    }
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
