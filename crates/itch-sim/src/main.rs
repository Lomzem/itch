//! A simulated Rig serving the itch protocol over TCP.

mod logs;
mod sim_board;

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use itch_core::frame::{FrameDecoder, MAX_FRAME, MAX_PAYLOAD, encode_frame};
use itch_core::rig::{Handled, Rig, SETTLE_MS};
use logs::ConnectionLogs;
use sim_board::SimBoard;

const USAGE: &str = "usage: itch-sim [--bind ADDR] [--hardware-fault]
                [--storage-fault]

Serves a simulated Rig over TCP, by default on 127.0.0.1 with a free port.
Once listening it prints one line: `itch-sim listening on tcp://HOST:PORT`.

--hardware-fault simulates a Channel expander that never answers.
--storage-fault makes every settings save fail with `StorageFault`.

Like a Rig over Wi-Fi, it serves at most 4 connections at once and closes
any more straight away.

The label and Wi-Fi credentials live in memory, so every run starts with
the default label and no credentials. With credentials, `GetStatus` reports
the Wi-Fi link connected at the address the sim listens on.";

/// The most connections served at once, as by a Rig over Wi-Fi.
const MAX_CONNECTIONS: usize = 4;

/// How often the Rig does its idle work.
const TICK: Duration = Duration::from_millis(50);

/// The one simulated Rig, shared by every connection.
struct Sim {
    rig: Rig,
    board: SimBoard,
}

fn main() -> ExitCode {
    let mut bind = String::from("127.0.0.1:0");
    let mut hardware_fault = false;
    let mut storage_fault = false;
    let mut args = std::env::args().skip(1);
    let usage = || {
        eprintln!("{USAGE}");
        ExitCode::from(2)
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => match args.next() {
                Some(addr) => bind = addr,
                None => return usage(),
            },
            "--hardware-fault" => hardware_fault = true,
            "--storage-fault" => storage_fault = true,
            _ => return usage(),
        }
    }

    let listener = match TcpListener::bind(&bind) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("itch-sim: can't listen on {bind}: {err}");
            return ExitCode::FAILURE;
        }
    };
    let bound = listener.local_addr().unwrap();
    println!("itch-sim listening on tcp://{bound}");
    logs::init();

    let mut board = SimBoard::new(hardware_fault, storage_fault, bound);
    let sim = Sim { rig: Rig::new(&mut board), board };
    let sim = Arc::new(Mutex::new(sim));
    thread::spawn({
        let sim = Arc::clone(&sim);
        move || tick(&sim)
    });
    let open = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming().flatten() {
        if open.load(Ordering::Relaxed) == MAX_CONNECTIONS {
            log::warn!("closed a connection: {MAX_CONNECTIONS} are already open");
            continue;
        }
        open.fetch_add(1, Ordering::Relaxed);
        let sim = Arc::clone(&sim);
        let open = Arc::clone(&open);
        thread::spawn(move || {
            serve(&stream, &sim);
            // Before `stream` closes, so a client that sees it close can
            // connect again at once.
            open.fetch_sub(1, Ordering::Relaxed);
        });
    }
    ExitCode::SUCCESS
}

/// Runs the Rig's idle tick forever.
fn tick(sim: &Mutex<Sim>) {
    loop {
        thread::sleep(TICK);
        let Sim { rig, board } = &mut *sim.lock().unwrap();
        rig.tick(board);
    }
}

/// Serves one connection until it closes: its replies, and its Log Events
/// from a second thread.
fn serve(stream: &TcpStream, sim: &Mutex<Sim>) {
    let peer = stream.peer_addr().map_or_else(|_| "a client".to_owned(), |addr| addr.to_string());
    log::info!("{peer} connected");
    let logs = ConnectionLogs::open();
    let writer = match stream.try_clone() {
        Ok(writer) => Mutex::new(writer),
        Err(err) => {
            log::warn!("{peer}: {err}");
            return;
        }
    };
    thread::scope(|scope| {
        scope.spawn(|| send_logs(&logs, &writer));
        if let Err(err) = answer(stream, sim, &logs, &writer) {
            log::warn!("{peer}: {err}");
        }
        logs.close();
    });
    log::info!("{peer} disconnected");
}

/// Answers the connection's requests until it closes.
fn answer(
    mut stream: &TcpStream,
    sim: &Mutex<Sim>,
    logs: &ConnectionLogs,
    writer: &Mutex<TcpStream>,
) -> io::Result<()> {
    let mut decoder = FrameDecoder::new();
    let mut input = [0; 4096];
    let mut reply = [0; MAX_PAYLOAD];
    loop {
        let read = stream.read(&mut input)?;
        if read == 0 {
            return Ok(());
        }
        for &byte in &input[..read] {
            let Some(frame) = decoder.push(byte) else {
                continue;
            };
            let handled = {
                let Sim { rig, board } = &mut *sim.lock().unwrap();
                rig.receive(frame, board, logs, &mut reply)
            };
            let Handled::Reply { len, settle } = handled else {
                continue;
            };
            if settle {
                // Outside the lock, so other connections aren't held up.
                thread::sleep(Duration::from_millis(SETTLE_MS));
            }
            send(writer, &reply[..len])?;
        }
    }
}

/// Sends the connection's Log Events as they come, until it closes.
fn send_logs(logs: &ConnectionLogs, writer: &Mutex<TcpStream>) {
    let mut event = [0; MAX_PAYLOAD];
    while let Some(len) = logs.next_event(&mut event) {
        if send(writer, &event[..len]).is_err() {
            return;
        }
    }
}

/// Frames and sends one payload, whole, between other threads' frames.
fn send(writer: &Mutex<TcpStream>, payload: &[u8]) -> io::Result<()> {
    let mut frame = [0; MAX_FRAME];
    let len = encode_frame(payload, &mut frame);
    writer.lock().unwrap().write_all(&frame[..len])
}
