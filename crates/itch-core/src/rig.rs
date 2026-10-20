use crate::board::{Board, Network, StorageFault};
use crate::frame::{FrameError, MAX_PAYLOAD};
use crate::holder::Holder;
use crate::label::Label;
use crate::wifi::{Addressing, Credentials};
use crate::wire::{
    CHANNEL_COUNT, DecodeError, ErrorCode, HelloReply, Lock, LogLevel, PROTOCOL, Reply, Request,
    StatusReply, decode_request,
};

/// How long the transport waits after a Channel write before replying, so
/// the contacts have settled. An initial value, pending bench measurement.
pub const SETTLE_MS: u64 = 20;

/// What the transport does after [`Rig::receive`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handled {
    /// Send nothing: the frame was dropped, or its payload had no readable
    /// `request_id`.
    NoReply,
    /// Send the first `len` bytes of the reply buffer, after waiting
    /// [`SETTLE_MS`] if `settle` is set.
    Reply { len: usize, settle: bool },
}

/// Controls which Log Events the calling stream receives.
pub trait LogControl {
    /// Sends the calling stream the Log Events at `level` and below from now
    /// on, and none of those queued before.
    fn set_log_level(&self, level: LogLevel);
}

/// The Rig's protocol logic. It does no I/O: the transport feeds it decoded
/// frames and sends back what it writes.
pub struct Rig {
    /// The label `Hello` reports.
    label: Label,
    /// The Wi-Fi credentials saved. `None` = none.
    wifi: Option<Credentials>,
    /// How the Rig is set to take its IPv4 address.
    addressing: Addressing,
    /// Who holds the Lock. `None` = unlocked. Never persisted, so a reset
    /// unlocks the Rig.
    holder: Option<Holder>,
    /// The commanded Channel state, bit 0 = CH1.
    channels: u16,
    /// Frames dropped for a bad CRC, truncated COBS or a wrong length.
    crc_failures: u32,
    /// Frames dropped for a payload over [`MAX_PAYLOAD`].
    oversized_frames: u32,
    /// Payloads dropped for being under 5 bytes.
    malformed_frames: u32,
    /// Times the expander was found faulty and re-initialised.
    expander_faults: u32,
    /// The expander didn't come back up, so it is held in reset with every
    /// Channel off.
    held_in_reset: bool,
}

impl Rig {
    /// Boots the Rig on `board`, loading the saved label.
    pub fn new<B: Board>(board: &mut B) -> Self {
        let label = board
            .load_label()
            .filter(|label| !label.as_str().is_empty())
            .unwrap_or_else(|| Label::default_for(board.info().hardware_id));
        let wifi = board.load_wifi();
        let addressing = board.load_addressing();
        Self {
            label,
            wifi,
            addressing,
            holder: None,
            channels: 0,
            crc_failures: 0,
            oversized_frames: 0,
            malformed_frames: 0,
            expander_faults: 0,
            held_in_reset: false,
        }
    }

    /// Answers one frame from the stream controlled by `logs`, writing its
    /// reply payload into `reply`. A frame the decoder dropped is counted.
    pub fn receive<B: Board>(
        &mut self,
        frame: Result<&[u8], FrameError>,
        board: &mut B,
        logs: &impl LogControl,
        reply: &mut [u8; MAX_PAYLOAD],
    ) -> Handled {
        let request = match frame {
            Ok(request) => request,
            Err(FrameError::Corrupt) => {
                log::warn!("dropped a corrupt frame");
                self.crc_failures = self.crc_failures.wrapping_add(1);
                return Handled::NoReply;
            }
            Err(FrameError::Oversized) => {
                log::warn!("dropped a frame over {MAX_PAYLOAD} bytes");
                self.oversized_frames = self.oversized_frames.wrapping_add(1);
                return Handled::NoReply;
            }
        };
        let Some((header, decoded)) = decode_request(request) else {
            self.malformed_frames = self.malformed_frames.wrapping_add(1);
            return Handled::NoReply;
        };
        if let Ok(Request::SetLogLevel { level }) = decoded {
            logs.set_log_level(level);
        }
        let result = match decoded {
            Ok(request) => self.apply(request, board),
            Err(error) => {
                let error = match error {
                    DecodeError::Malformed => ErrorCode::Malformed,
                    DecodeError::UnknownType => ErrorCode::UnknownRequest,
                };
                log::warn!(
                    "answered request {} (type {:#04x}) with {error:?}",
                    header.request_id,
                    header.request_type
                );
                Err(error)
            }
        };
        let settle = matches!(result, Ok(Reply::WriteChannels { .. } | Reply::Clear { .. }));
        let response = match result {
            Ok(reply) => header.reply(reply),
            Err(ErrorCode::Locked) => header.error(ErrorCode::Locked, self.holder()),
            Err(error) => header.error(error, ""),
        };
        Handled::Reply { len: response.encode(reply), settle }
    }

    /// The idle tick, every ~50 ms. Re-initialises the expander if it no
    /// longer drives the commanded Channels, and retries one held in reset.
    pub fn tick<B: Board>(&mut self, board: &mut B) {
        if !self.held_in_reset {
            if board.read_channels() == Ok(self.channels) {
                return;
            }
            self.count_fault();
        }
        let _ = self.recover(board, self.channels);
    }

    fn apply<B: Board>(&mut self, request: Request, board: &mut B) -> Result<Reply<'_>, ErrorCode> {
        Ok(match request {
            Request::Hello => {
                let info = board.info();
                Reply::Hello(HelloReply {
                    protocol: PROTOCOL,
                    firmware_version: info.firmware_version,
                    dirty: info.dirty,
                    channel_count: CHANNEL_COUNT,
                    board_revision: info.board_revision,
                    hardware_id: info.hardware_id,
                    label: self.label.as_str(),
                })
            }
            // Applied to the log control in `receive`.
            Request::SetLogLevel { .. } => Reply::SetLogLevel,
            Request::GetChannels => {
                Reply::GetChannels { state: if self.held_in_reset { 0 } else { self.channels } }
            }
            Request::GetStatus => {
                let info = board.info();
                let counters = board.counters();
                // Without credentials the link is off, even while the radio
                // takes a moment to leave the network it was on.
                let network = match self.wifi {
                    Some(_) => board.network(),
                    None => Network::default(),
                };
                Reply::GetStatus(StatusReply {
                    uptime_ms: board.now(),
                    reset_reason: info.reset_reason,
                    last_panic: info.last_panic,
                    crc_failures: self.crc_failures,
                    oversized_frames: self.oversized_frames,
                    malformed_frames: self.malformed_frames,
                    dropped_logs: counters.dropped_logs,
                    expander_faults: self.expander_faults,
                    usb_tx_dropped: counters.usb_tx_dropped,
                    holder: self.holder(),
                    wifi_configured: self.wifi.is_some(),
                    ssid: self.wifi.as_ref().map_or("", Credentials::ssid),
                    link: network.link,
                    current_address: network.address,
                    mode: self.addressing.mode(),
                    address: self.addressing.address(),
                    prefix: self.addressing.prefix(),
                    gateway: self.addressing.gateway(),
                })
            }
            Request::WriteChannels(write) => {
                self.check_lock(write.holder)?;
                let state = self.channels & !write.mask | write.values & write.mask;
                Reply::WriteChannels { state: self.write(board, state)? }
            }
            Request::Clear { holder } => {
                self.check_lock(holder)?;
                Reply::Clear { state: self.write(board, 0)? }
            }
            Request::SetLabel(set) => {
                self.check_lock(set.holder)?;
                self.set_label(board, set.label)?;
                Reply::SetLabel { label: self.label.as_str() }
            }
            Request::SetWifi(set) => {
                self.check_lock(set.holder)?;
                self.set_wifi(board, set.credentials())?;
                Reply::SetWifi
            }
            Request::SetAddress(set) => {
                self.check_lock(set.holder)?;
                let addressing = set.addressing().ok_or(ErrorCode::Malformed)?;
                self.set_addressing(board, addressing)?;
                Reply::SetAddress
            }
            Request::Lock(lock) => {
                self.lock(lock)?;
                Reply::Lock
            }
            Request::ReleaseLock { holder } => {
                self.release_lock(holder)?;
                Reply::ReleaseLock
            }
        })
    }

    /// The current Holder. Empty = unlocked.
    fn holder(&self) -> &str {
        self.holder.as_ref().map_or("", Holder::as_str)
    }

    /// Refuses a change from `holder` while someone else holds the Lock.
    fn check_lock(&self, holder: &str) -> Result<(), ErrorCode> {
        match &self.holder {
            Some(current) if current.as_str() != holder => Err(ErrorCode::Locked),
            _ => Ok(()),
        }
    }

    /// Locks the Rig under `lock.holder`. Another Holder's Lock is refused
    /// unless this is a Takeover.
    fn lock(&mut self, lock: Lock) -> Result<(), ErrorCode> {
        if !lock.takeover {
            self.check_lock(lock.holder)?;
        }
        match self.holder() {
            current if current == lock.holder => return Ok(()),
            "" => log::info!("{} locked the Rig", lock.holder),
            current => log::info!("{} took over the Lock from {current}", lock.holder),
        }
        self.holder = Some(Holder::new(lock.holder).expect("decoding caps holders at 64 bytes"));
        Ok(())
    }

    /// Unlocks the Rig if `holder` holds the Lock. Only the Holder may
    /// release it; releasing an unlocked Rig changes nothing.
    fn release_lock(&mut self, holder: &str) -> Result<(), ErrorCode> {
        self.check_lock(holder)?;
        if self.holder.take().is_some() {
            log::info!("{holder} released the Lock");
        }
        Ok(())
    }

    /// Saves `label` and reports it from now on. Empty, or the default by
    /// name, restores the default, which is saved as empty.
    fn set_label<B: Board>(&mut self, board: &mut B, label: &str) -> Result<(), ErrorCode> {
        let default = Label::default_for(board.info().hardware_id);
        let custom = if label == default.as_str() { "" } else { label };
        board.save_label(custom).map_err(|StorageFault| ErrorCode::StorageFault)?;
        self.label = match custom {
            "" => default,
            custom => Label::new(custom).expect("decoding caps labels at 64 bytes"),
        };
        Ok(())
    }

    /// Saves `wifi` and reports it from now on. `None` clears the
    /// credentials.
    fn set_wifi<B: Board>(
        &mut self,
        board: &mut B,
        wifi: Option<Credentials>,
    ) -> Result<(), ErrorCode> {
        board.save_wifi(wifi.as_ref()).map_err(|StorageFault| ErrorCode::StorageFault)?;
        match &wifi {
            Some(wifi) => log::info!("Wi-Fi credentials set for {}", wifi.ssid()),
            None => log::info!("Wi-Fi credentials cleared"),
        }
        self.wifi = wifi;
        Ok(())
    }

    /// Saves `addressing` and reports it from now on.
    fn set_addressing<B: Board>(
        &mut self,
        board: &mut B,
        addressing: Addressing,
    ) -> Result<(), ErrorCode> {
        board.save_addressing(&addressing).map_err(|StorageFault| ErrorCode::StorageFault)?;
        match &addressing {
            Addressing::Dhcp => log::info!("address set to DHCP"),
            Addressing::Static(set) => {
                let [a, b, c, d] = set.address();
                log::info!("address set to {a}.{b}.{c}.{d}/{}", set.prefix());
            }
        }
        self.addressing = addressing;
        Ok(())
    }

    /// Drives the Channels to `state` and makes it the commanded state. A
    /// failed write re-initialises the expander and tries again.
    fn write<B: Board>(&mut self, board: &mut B, state: u16) -> Result<u16, ErrorCode> {
        if self.held_in_reset {
            return Err(ErrorCode::HardwareFault);
        }
        if board.write_channels(state).is_err() {
            self.count_fault();
            self.recover(board, state)?;
        }
        self.channels = state;
        Ok(state)
    }

    fn count_fault(&mut self) {
        self.expander_faults = self.expander_faults.wrapping_add(1);
        log::warn!("the expander faulted; re-initialising it");
    }

    /// Re-initialises the expander and drives `state`. If that fails, the
    /// expander is held in reset until a later tick brings it back.
    fn recover<B: Board>(&mut self, board: &mut B, state: u16) -> Result<(), ErrorCode> {
        let recovered = board.reinit().and_then(|()| board.write_channels(state)).is_ok();
        if !recovered {
            board.hold_in_reset();
        }
        if recovered && self.held_in_reset {
            log::info!("the expander is back; Channels restored");
        } else if !recovered && !self.held_in_reset {
            log::error!("the expander didn't come back; holding every Channel off");
        }
        self.held_in_reset = !recovered;
        if recovered { Ok(()) } else { Err(ErrorCode::HardwareFault) }
    }
}
