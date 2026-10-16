use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use itch_core::frame::{FrameDecoder, MAX_FRAME, MAX_PAYLOAD, encode_frame};
use itch_core::wire::{HelloReply, Message, Reply, Request, Response};

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
    let mut payload = [0; MAX_PAYLOAD];
    let len = Message::Request { request_id, request: Request::Hello }.encode(&mut payload);
    let mut frame = [0; MAX_FRAME];
    let len = encode_frame(&payload[..len], &mut frame);
    stream.write_all(&frame[..len]).unwrap();

    let mut decoder = FrameDecoder::new();
    let reply = loop {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        if let Some(Ok(reply)) = decoder.push(byte[0]) {
            break reply.to_vec().leak();
        }
    };
    match Message::decode(reply) {
        Ok(Message::Response { request_id: id, response: Response::Ok(Reply::Hello(hello)) })
            if id == request_id =>
        {
            hello
        }
        other => panic!("not a Hello reply: {other:?}"),
    }
}
