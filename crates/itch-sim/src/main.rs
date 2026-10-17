//! A simulated Rig serving the itch protocol over TCP.

mod logs;
mod sim_board;

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use itch_core::frame::{FrameDecoder, FrameError, MAX_FRAME, MAX_PAYLOAD, encode_frame};
use itch_core::rig::{Handled, Rig, SETTLE_MS};
use logs::ConnectionLogs;
use sim_board::{CRC_FAILURES, OVERSIZED_FRAMES, SimBoard};

const USAGE: &str = "usage: itch-sim [--bind ADDR] [--clock-rate N] [--hardware-fault]

Serves a simulated Rig over TCP, by default on 127.0.0.1 with a free port.
Once listening it prints one line: `itch-sim listening on tcp://HOST:PORT`.

--clock-rate N runs the Rig's clock N times faster than real time, so a test
can let a Lease lapse without waiting 10 s.
--hardware-fault simulates a Channel expander that never answers.";

/// How often the Rig does its idle work.
const TICK: Duration = Duration::from_millis(50);

/// The one simulated Rig, shared by every connection.
struct Sim {
    rig: Rig,
    board: SimBoard,
}

fn main() -> ExitCode {
    let mut bind = String::from("127.0.0.1:0");
    let mut clock_rate = 1;
    let mut hardware_fault = false;
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
            "--clock-rate" => match args.next().map(|rate| rate.parse()) {
                Some(Ok(rate)) if rate > 0 => clock_rate = rate,
                _ => return usage(),
            },
            "--hardware-fault" => hardware_fault = true,
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
    println!("itch-sim listening on tcp://{}", listener.local_addr().unwrap());
    logs::init();

    let sim = Sim { rig: Rig::new(), board: SimBoard::new(clock_rate, hardware_fault) };
    let sim = Arc::new(Mutex::new(sim));
    thread::spawn({
        let sim = Arc::clone(&sim);
        move || tick(&sim)
    });
    for stream in listener.incoming().flatten() {
        let sim = Arc::clone(&sim);
        thread::spawn(move || serve(stream, &sim));
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
fn serve(stream: TcpStream, sim: &Mutex<Sim>) {
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
        if let Err(err) = answer(&stream, &peer, sim, &logs, &writer) {
            log::warn!("{peer}: {err}");
        }
        logs.close();
    });
    log::info!("{peer} disconnected");
}

/// Answers the connection's requests until it closes.
fn answer(
    mut stream: &TcpStream,
    peer: &str,
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
            let request = match decoder.push(byte) {
                None => continue,
                Some(Ok(request)) => request,
                Some(Err(FrameError::Corrupt)) => {
                    log::warn!("{peer}: dropped a corrupt frame");
                    CRC_FAILURES.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                Some(Err(FrameError::Oversized)) => {
                    log::warn!("{peer}: dropped a frame over {MAX_PAYLOAD} bytes");
                    OVERSIZED_FRAMES.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
            };
            let handled = {
                let Sim { rig, board } = &mut *sim.lock().unwrap();
                rig.handle(request, board, &mut reply)
            };
            let len = match handled {
                Handled::NoReply => continue,
                Handled::Reply { len, settle } => {
                    if settle {
                        // Outside the lock, so other connections aren't held up.
                        thread::sleep(Duration::from_millis(SETTLE_MS));
                    }
                    len
                }
                Handled::SetLogLevel { len, level } => {
                    logs.set_level(level);
                    len
                }
            };
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
