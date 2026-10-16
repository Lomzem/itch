//! A simulated Rig serving the itch protocol over TCP.

mod sim_board;

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use itch_core::frame::{FrameDecoder, MAX_FRAME, MAX_PAYLOAD, encode_frame};
use itch_core::rig::{Handled, Rig, SETTLE_MS};
use sim_board::SimBoard;

const USAGE: &str = "usage: itch-sim [--bind ADDR]

Serves a simulated Rig over TCP, by default on 127.0.0.1 with a free port.
Once listening it prints one line: `itch-sim listening on tcp://HOST:PORT`.";

/// The one simulated Rig, shared by every connection.
struct Sim {
    rig: Rig,
    board: SimBoard,
}

fn main() -> ExitCode {
    let mut bind = String::from("127.0.0.1:0");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--bind", Some(addr)) => bind = addr,
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }

    let listener = match TcpListener::bind(&bind) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("itch-sim: can't listen on {bind}: {err}");
            return ExitCode::FAILURE;
        }
    };
    println!("itch-sim listening on tcp://{}", listener.local_addr().unwrap());

    let sim = Arc::new(Mutex::new(Sim { rig: Rig::new(), board: SimBoard::new() }));
    for stream in listener.incoming().flatten() {
        let sim = Arc::clone(&sim);
        thread::spawn(move || serve(stream, &sim));
    }
    ExitCode::SUCCESS
}

/// Answers one connection's requests until it closes.
fn serve(mut stream: TcpStream, sim: &Mutex<Sim>) -> io::Result<()> {
    let mut decoder = FrameDecoder::new();
    let mut input = [0; 4096];
    let mut reply = [0; MAX_PAYLOAD];
    let mut frame = [0; MAX_FRAME];
    loop {
        let read = stream.read(&mut input)?;
        if read == 0 {
            return Ok(());
        }
        for &byte in &input[..read] {
            let Some(Ok(request)) = decoder.push(byte) else {
                continue;
            };
            let handled = {
                let Sim { rig, board } = &mut *sim.lock().unwrap();
                rig.handle(request, board, &mut reply)
            };
            if let Handled::Reply { len, settle } = handled {
                if settle {
                    // Outside the lock, so other connections aren't held up.
                    thread::sleep(Duration::from_millis(SETTLE_MS));
                }
                let len = encode_frame(&reply[..len], &mut frame);
                stream.write_all(&frame[..len])?;
            }
        }
    }
}
