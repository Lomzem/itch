# Rev A bring-up checklist

Bench checks for an itch Rig: an ESP32-S3 driving 16 Channels (CH1–16) through an I2C I/O expander, with the expander's RESET on GPIO6. Code that a host test can reach is tested by `cargo test`; this list covers what only real hardware shows.

- **Full run:** every item below, before firmware ships and again after any hardware, esp-hal or esp-rtos change.
- **Per-Rig run:** only the items marked **[per-Rig]**, before each new Rig goes to QA. They take a few minutes.

> [!WARNING]
> **Never erase the whole chip** (`espflash erase-flash`, or esptool's `erase_flash`). The Rig's label lives in the `settings` partition, and a full erase loses it. Reflash with `cargo +esp flash-firmware`, or with a merged image saved with `--skip-padding` (see the [README](../../README.md#firmware)). Both leave the label alone.

The examples use a made-up Rig with Hardware ID `60:55:F9:C0:27:18`, so its default label is `itch-C02718`. No DUT is needed: check the contacts themselves with a meter or a logic analyser.

Use a client adapted to [protocol 3](../protocol/README.md); older released clients use the former wire layout. Client interface examples below illustrate the bench actions, while the wire contract lives in `docs/protocol/`.

## Tools

- A Linux PC and a Windows 11 PC, each with Wireshark: usbmon on Linux, USBPcap on Windows.
- itch-py installed on both PCs, plus `espflash` and the `esp` toolchain on the PC that flashes.
- A continuity meter, and a scope or a logic analyser with enough inputs for GPIO6 and all 16 contacts. Use a pull-up on each contact.
- A jumper wire to short SDA (GPIO5) or the expander RESET (GPIO6) to GND.
- A second Rig, for the discovery checks.
- For mDNS, `avahi-utils` on the Linux PC (`avahi-browse`, `avahi-resolve`), and Bonjour's `dns-sd` on the Windows PC.
- For the Wi-Fi items: a WPA2-Personal access point you can reboot and switch off, running DHCP on a network the Linux PC can reach, and a second network to move a Rig to (a second SSID on the same access point will do). The examples call them `ExampleRigNet` and `ExampleRigNet2` and use addresses from `192.0.2.0/24`; use your own.

## Where the tunable figures live

| Figure | Constant | File |
| --- | --- | --- |
| Relay settle delay, 20 ms | `SETTLE_MS` | `crates/itch-core/src/rig.rs` |
| Supervisor deadline, 1 s | `DEADLINE_MS` | `crates/itch-firmware/src/supervisor.rs` |
| RTC watchdog (RWDT), 2 s | `WATCHDOG_TIMEOUT` | `crates/itch-firmware/src/board.rs` |
| I2C transaction timeout, 10 ms | `I2C_TIMEOUT` | `crates/itch-firmware/src/board.rs` |
| Idle tick (readback, check-in), 50 ms | `TICK_MS` | `crates/itch-firmware/src/serve.rs` |
| USB write timeout, 100 ms | `WRITE_TIMEOUT` | `crates/itch-firmware/src/usb.rs` |
| Wi-Fi join retry, 5 s | `RETRY` | `crates/itch-firmware/src/radio.rs` |
| Stay on the old network, or at the old address, after a change, 1 s | `LEAVE_DELAY` | `crates/itch-firmware/src/radio.rs` |
| Heap, 64 KiB reclaimed + 64 KiB | `heap_allocator!` in `start` | `crates/itch-firmware/src/radio.rs` |
| TCP connections at once, 4 | `TCP_CONNECTIONS` | `crates/itch-firmware/src/connections.rs` |
| TCP keepalive 5 s, timeout 15 s | `KEEP_ALIVE`, `TIMEOUT` | `crates/itch-firmware/src/tcp.rs` |
| TCP write timeout, 100 ms | `WRITE_TIMEOUT` | `crates/itch-firmware/src/tcp.rs` |
| mDNS record TTL, 120 s | `TTL` | `crates/itch-firmware/src/mdns.rs` |
| mDNS restart after a failure, 5 s | `RETRY` | `crates/itch-firmware/src/responder.rs` |
| DTR/RTS on open: both up, or both down on Windows | `lines` in `UsbTransport.__init__` | itch-py `src/itch/_usb.py` |

`SETTLE_MS` also paces `itch-sim`, so changing it keeps the simulator honest. If you raise `DEADLINE_MS` or `WATCHDOG_TIMEOUT`, keep the RWDT above the supervisor deadline. Both must stay above the longest single flash erase or write: `rig` checks in around each one, and a check-in in time feeds the RWDT, but nothing runs during one. Raising either one also lengthens the 2 s fault-to-open bound, so update the spec to match.

## 1. Module **[per-Rig]**

- **Method:** read the module's can marking, then run `espflash board-info`.
- **Pass:** the marking reads `ESP32-S3-WROOM-1-N8`, and `board-info` reports an 8 MB flash. The flash runner in `.cargo/config.toml` writes `--flash-size 8mb` into the image header.
- **If it fails:** don't use the module. `crates/itch-firmware/partitions.csv` places `settings` at 0x400000, past the end of a 4 MB flash.

## 2. Hardware ID and default label **[per-Rig]**

- **Method:** flash with `cargo +esp flash-firmware` from the workspace root. Note the MAC that `espflash board-info` prints, then run `itch list` and `Rig.connect().hardware_id`.
- **Pass:** all three show the same MAC. A Rig that has never had a label set shows the default label `itch-` plus the last three bytes, such as `itch-C02718`.
- **If it fails:** the firmware reads the Hardware ID from `esp_hal::efuse::base_mac_address()` in `board::boot` (`crates/itch-firmware/src/board.rs`). itch-py takes the USB serial number in `usb_ports()` (`src/itch/_usb.py`) and refuses a mismatch in `src/itch/_rig.py`. The default label comes from `Label::default_for` (`crates/itch-core/src/label.rs`).

## 3. Boot order

- **Method:** scope GPIO6 and a few contacts while you power the Rig up. Do it once with Channels on before the power-off.
- **Pass:** GPIO6 stays low until the expander is initialised. No contact closes during boot, and `get_channels()` reports every Channel off.
- **If it fails:** `board::boot` must drive GPIO6 low first, as a plain GPIO. `bring_up` must write OLAT = 0 before IODIR = out, then read the registers back (`crates/itch-firmware/src/board.rs`).

## 4. Each Channel on and off **[per-Rig]**

- **Method:** call `rig.set_channel(n, "on")` for n = 1…16, measure the contact, then turn it off and measure again.
- **Pass:** only CH*n*'s contact closes, then it opens again. The status LED (GPIO15) is lit while any Channel is on.
- **If it fails:** a wrong or swapped contact means the Channel map `CHANNELS` in `crates/itch-firmware/src/board.rs` is wrong (CH1–8 → GPB7…GPB0, CH9–16 → GPA7…GPA0). LED behaviour is in `show_status`, in the same file.

## 5. Label persists **[per-Rig]**

- **Method:** run `itch label itch-C02718 bench3`. Power-cycle and check `itch list`. Reflash with `cargo +esp flash-firmware` and check again. *Full run only:* also flash a merged image (README) and check again. Afterwards, `itch label bench3 ""` restores the default.
- **Pass:** `bench3` survives the power cycle, the reflash and the merged image.
- **If it fails:** check that the partition table in `crates/itch-firmware/partitions.csv` was flashed, and that the merged image was saved with `--skip-padding`; padding overwrites `settings`. The storage code is in `crates/itch-firmware/src/settings.rs` (`PARTITION`, `VERSION`).

The Wi-Fi credentials and address configuration are saved with the label; item 28 checks they survive the same reflashes.

## 6. USB open/close leaves the Rig alone

Setup: turn CH1 and CH16 on and every other Channel off, and put the meter or logic analyser on all 16 contacts. This is the test pattern.

On both Linux and Windows 11, capture USB (usbmon / USBPcap) while a script runs 1,000 cycles of `Rig.connect("60:55:F9:C0:27:18")` → `rig.status()` → `get_channels()` → `close()`, logging `uptime`, `reset_reason` and the Channels each time. The script only reads; it never sets Channels. Then repeat a few cycles after each of these, with the Rig powered throughout:

- the script killed mid-call (`kill -KILL` / `taskkill /F`);
- the port closed and reopened by a fresh script;
- a PC reboot;
- another serial tool having used the port.

- **Pass:** the Wireshark filter `usb.setup.bRequest == 0x22 && (usb.setup.wValue == 1 || usb.setup.wValue == 2)` (SET_CONTROL_LINE_STATE with only DTR, or only RTS) matches nothing. `uptime` keeps rising and `reset_reason` never changes. `get_channels()` reports the test pattern every time, and only the CH1 and CH16 contacts measure closed throughout, including while no script has the port open.
- **Power loss or reset is a different case.** A USB unplug and replug powers a USB-powered Rig off, and a PC reboot may too if the port drops power. Either shows as `uptime` restarting and a new `reset_reason`. The Rig then boots Clear, as item 10 checks, so expect every Channel off and every contact open; the Channel state is not retained. Restore the test pattern before the next cycles. A PC reboot passes the powered checks above only if `uptime` shows the Rig stayed up.
- **If it fails:** a lone RTS resets the S3, and a lone DTR sets its download flag. The policy is the `lines` value in `UsbTransport.__init__` (itch-py `src/itch/_usb.py`): True/True on Linux, False/False on Windows. If Windows fails with False/False, try True/True there. Never change the lines after `open()`. If `uptime` keeps rising but Channels changed, something Cleared or wrote them without a reset: check that the script and the other serial tool send no Channel writes.

## 7. Finding Rigs over USB

- `itch list` and `Rig.connect("<hardware id>")` agree with each other on both OSes.
- With two Rigs, and one held by a running script: `itch list` shows that one as `in use`, and `Rig.connect("<other label>")` opens the free one without disturbing the script.
- Unplugging during a call raises `Rig disconnected; run the script again.`
- On Linux, a user outside the port's group (`dialout`/`uucp`) gets the hint to join it.
- **If it fails:** port matching and the in-use test are in itch-py `src/itch/_usb.py` (`VID, PID`, `PortInUse`). Selection is in `src/itch/_rig.py`, and the messages are in `src/itch/_errors.py`.

## 8. Packet boundaries

USB full-speed bulk packets are 64 B, so frames whose size falls on or around a multiple of 64 are the risky ones.

- **Method:** with itch-py's `itch._frame.encode_frame`, build frames of exactly 63, 64, 65, 128 and 192 B, delimiters included, and write them raw to the port.
  - **Host → Rig:** a payload with an unknown request type can be any size. The Rig answers it with an `UnknownRequest` error.
  - **Rig → host:** vary the Label length and read `Hello` to move the reply around 64 B. For longer frames, use a throwaway scratch build that emits controlled-length Log Events, covering 128 and 192 B as well. Decode everything received with `itch._frame.FrameDecoder`; do not commit the scratch logging.
- **Pass:** every host → Rig frame gets an `UnknownRequest` reply carrying its request ID, and `crc_failures`, `oversized_frames` and `malformed_frames` stay unchanged. Every Rig → host frame decodes, with no request timing out.
- **If it fails:** the receive path is `usb_rx` in `crates/itch-firmware/src/usb.rs`, which reads 64 B at a time into a persistent `FrameDecoder`. On the send side, `usb_tx` must `flush()` after each frame; that flush sends the zero-length packet a multiple-of-64 transfer needs.

## 9. Host stops reading

- **Method:** turn a Channel on, then from a raw serial handle write a burst of `GetStatus` frames, or turn on debug logs, and stop reading for 30 s. Resume, then reconnect with itch-py.
- **Pass:** `uptime` has kept rising and `reset_reason` is unchanged. The Channel is still on, so there was no reboot or Clear. `usb_tx_dropped` (or `dropped_logs`) has risen, and the next frames decode normally.
- **If it fails:** `usb_tx` in `crates/itch-firmware/src/usb.rs` drops the rest of a frame after `WRITE_TIMEOUT` and counts it in `TX_DROPPED`. Request back-pressure is `DEPTH` in `crates/itch-firmware/src/connections.rs`.

## 10. Unplug and every reset type with Channels on

For each reset type, turn on all 16 Channels, watch GPIO6 and every contact on the logic analyser, cause the reset, then read `rig.status()`:

| Reset | Expected `reset_reason` |
| --- | --- |
| USB unplug and replug (power loss) | `ChipPowerOn` |
| EN/reset button, if fitted | record it |
| `cargo +esp flash-firmware` / `espflash reset` | record it |
| Panic (item 11) | `CoreSw` |
| Hang or busy loop (item 11) | `SysRtcWdt` |

- **Pass:** every contact opens and none pulses on during the reset or the boot. The Rig boots Clear: `get_channels()` is all off and the contacts measure open.
- **If it fails:** GPIO6 is pulled low on the board and `board::boot` drives it low first; a pulse here is most likely a hardware matter. The panic path calls `board::force_reset_low()` before anything else (`#[panic_handler]` in `crates/itch-firmware/src/main.rs`).

## 11. Fault-to-open within 2 s

Each case needs a throwaway build from a scratch branch; don't commit it. Make the `rig` task (`crates/itch-firmware/src/main.rs`) misbehave some seconds after boot, with Channels on:

- **Hang:** `core::future::pending::<()>().await`. The task stops checking in, the supervisor latches RESET low after `DEADLINE_MS`, and the RWDT resets the chip.
- **Busy loop:** `loop {}`.
- **Panic:** `panic!("bench")`.
- **Stuck I2C:** no build needed. Short SDA (GPIO5) to GND on the normal firmware.

- **Pass:** for each case, every contact opens within 2 s of the fault, and `reset_reason` after the reboot matches the table in item 10. For stuck I2C, the contacts open and the LED blinks fast, normally with no reboot. Changes raise `HardwareFaultError`. Once the short is removed, the commanded Channels come back and `expander_faults` has risen by one.
- **If it fails:** check the order in the `#[panic_handler]`, and the timings `DEADLINE_MS`, `WATCHDOG_TIMEOUT`, `I2C_TIMEOUT` and `TICK` (see the table above). Expander recovery is `Rig::tick` / `recover` in `crates/itch-core/src/rig.rs`.

## 12. Crash marker

- **Method:** after the panic case in item 11, read `rig.status()`, then reset the Rig another way (EN button, or `espflash reset`) and read it again. Power the Rig off and on several times.
- **Pass:** the boot right after the panic reports `last_panic` as the panic's `file:line` (it survived the reset). The next boot reports `None` (it was cleared). Cold power-ons, which leave noise in RTC RAM, always report `None` (an invalid record is ignored).
- **If it fails:** the record is `CRASH` in RTC fast RAM, taken once at boot by `take_crash` (`crates/itch-firmware/src/board.rs`). Its format and validity checks (`VALID`, checksum, `MAX_LOCATION`) are in `crates/itch-firmware/src/crash.rs`.

## 13. Expander recovery

- **Method:** with Channels on, hold the expander RESET (GPIO6) to GND, then release it. Separately, short SDA to GND for well under a tick (an I2C glitch). If the board lets you, also briefly dip the expander's supply (a brownout).
- **Pass:** while RESET is held, the contacts are open and the LED blinks fast. After release, or after the glitch, the commanded Channels come back without a reboot, and `expander_faults` counts each fault once.
- **If it fails:** see `Rig::tick` and `recover` in `crates/itch-core/src/rig.rs`, and `reinit` / `bring_up` / `hold_in_reset` in `crates/itch-firmware/src/board.rs`.

## 14. Flash stall

- **Method:** time 100 back-to-back `set_label` calls, each with a different 64-byte label. Then carry on, about 2,000 writes in all, until the 64 KiB `settings` partition has wrapped and erased sectors. Subtract a typical `status()` round trip from the longest `set_label` call; that difference is the stall. Then corrupt the partition so the next save erases all 16 sectors: write 64 KiB of random bytes to it (`head -c 65536 /dev/urandom > noise.bin`, then `espflash write-bin 0x400000 noise.bin`), boot, and time one `set_label`. Restore the label afterwards.
- **Pass:** the longest stall of a normal save is under 400 ms. The save after the corruption succeeds, however long it takes, even past `DEADLINE_MS`, and the label survives a power cycle. No call is followed by a watchdog reset: `uptime` keeps counting and `reset_reason` is unchanged.
- **If it fails:** every flash operation goes through `Supervised` in `crates/itch-firmware/src/settings.rs`, which checks in before and after each one, with erases split per sector. Each check-in carries its time and feeds the RWDT, so only the gap between two check-ins counts, never the whole save. If a single erase or write still stalls too long, raise `DEADLINE_MS` and `WATCHDOG_TIMEOUT` (see the note under the table).

## 15. Relay settling

- **Method:** scope one contact against the expander output that drives it. For each Channel, measure from the output edge to the contact's last bounce, both on and off. Repeat with all 16 switching together.
- **Pass:** the worst value observed is below `SETTLE_MS`, with margin.
- **If it fails:** raise `SETTLE_MS` in `crates/itch-core/src/rig.rs` above the worst value observed. Firmware and simulator both wait that long before replying to `WriteChannels` and `Clear`.

## Wi-Fi

Items 16–28 need the access point from [Tools](#tools). Keep a USB connection open throughout, with its log level at debug, to read the Rig's Wi-Fi log lines (`joined …`, `couldn't join …`, `lost …`, `serving tcp://…`, `advertising …`, `heap: …`). `GetStatus` gives `link` (off, joining or connected) and `current_address`, and the configured `mode`, `address`, `prefix` and `gateway`.

## 16. Joining the network **[per-Rig]**

- **Method:** on a Rig with no credentials, read `GetStatus`. Send `SetWifi` over USB with `ExampleRigNet` and its passphrase, then read `GetStatus` every second. Once connected, connect a client to `tcp://<current_address>:7878` and send `Hello`. Power-cycle the Rig and watch it again.
- **Pass:** with no credentials, `link` is off and `current_address` is zero. After `SetWifi`, `link` goes from joining to connected within about 10 s, and `current_address` is the address the access point's DHCP lease table shows for the Rig's Hardware ID. `Hello` over TCP reports the same Hardware ID and label as over USB. After the power cycle the Rig joins again by itself.
- **If it fails:** the warning `couldn't join ExampleRigNet (…)` names the radio's reason. Joining is `join` and `stay_joined`, and the address is the `address` task, in `crates/itch-firmware/src/radio.rs`. `link` and `current_address` come from `Wifi::network` in `crates/itch-firmware/src/wifi.rs`. Credentials that don't survive the power cycle are a settings matter (item 5).

## 17. Wrong credentials, drops and rejoining

Setup: the test pattern from item 6 (CH1 and CH16 on), with the meter or logic analyser on all 16 contacts.

- **Method:**
  1. Send `SetWifi` with a wrong passphrase and watch for 30 s. Then send the right one.
  2. With the Rig connected, reboot the access point.
  3. Switch the access point off for 2 minutes, then on again.
  4. With a client connected over TCP, leave it idle through 2 and 3.
- **Pass:**
  - With the wrong passphrase, `link` stays joining and a `couldn't join` warning comes about every 5 s. USB answers normally throughout. With the right one the Rig connects.
  - After the reboot and the outage, `link` shows joining while the access point is down and connected again within about 15 s of it coming back, with no Rig reboot: `uptime` keeps rising and `reset_reason` is unchanged.
  - The contacts keep the test pattern throughout, and `get_channels()` over USB reports it.
  - The idle TCP client's connection is dropped. It reconnects to the Rig's address and carries on.
- **If it fails:** the retry interval is `RETRY` in `crates/itch-firmware/src/radio.rs`. Nothing in the Wi-Fi code touches the Channels; a pattern change means something wrote or Cleared them, as in item 6.

## 18. Changing networks

- **Method:** over TCP, send `SetWifi` with `ExampleRigNet2` and its passphrase. Then, over the new network, send `SetWifi` with an empty SSID and passphrase. Finally restore `ExampleRigNet` over USB.
- **Pass:** each reply arrives over the TCP connection before that connection drops. The Rig joins `ExampleRigNet2` and reports its new address. After clearing, `wifi_configured` is false, `link` is off and the radio is off: the access point no longer lists the Rig. USB works throughout, and the last `SetWifi` brings the Rig back.
- **If it fails:** the board hands new credentials to the radio only after the reply is queued (`apply_settings` in `crates/itch-firmware/src/board.rs`, called from `serve`), and the radio waits `LEAVE_DELAY` before leaving. A reply lost on a slow network needs a longer `LEAVE_DELAY`.

## 19. USB and Wi-Fi together

- **Method:** run two scripts at once for 10 minutes, one over USB and one over TCP. Each sets a different log level (debug and warn), then loops: write a Channel, read the Channels, read `GetStatus`. Halfway through, open a third, raw TCP connection, write a burst of 50 `GetStatus` frames and never read. Lock the Rig from the USB script, and try a write from the TCP script under another Holder.
- **Pass:** each script only ever receives replies to its own requests, and Log Events at its own level. While the raw connection's burst is answered, the scripts may wait or get `Busy`, for at most about 100 ms per frame in the burst; then they carry on at full speed. The TCP script's write is refused with `Locked` naming the USB script's Holder, and it may still read the Rig.
- **If it fails:** routing is in `crates/itch-firmware/src/connections.rs` and `serve.rs`. The TCP writer is `tcp_tx` in `crates/itch-firmware/src/tcp.rs`, bounded by its `WRITE_TIMEOUT`.

## 20. Connection limit

- **Method:** open four TCP connections and send `Hello` on each. Open a fifth. Close one of the four and open another. Then open a fourth again from a laptop and take the laptop off the network without closing the connection, and keep trying a new connection from the PC.
- **Pass:** the fifth connection is refused at once (connection refused or reset), and the four and USB keep working. After one closes, the next connection is served. The laptop's place frees up within about 20 s, after which a new connection is served.
- **If it fails:** each `tcp` task in `crates/itch-firmware/src/tcp.rs` listens only while its slot is free, so with all four in use the network stack refuses the fifth. A vanished client is dropped by `KEEP_ALIVE` and `TIMEOUT`, in the same file.

## 21. RAM headroom

- **Method:** with four TCP connections open, each at log level debug and polling `GetStatus` as fast as it can, run for 10 minutes. Read the `heap:` debug log lines, which come after every join and every new TCP connection.
- **Pass:** no reboot (`uptime` keeps rising and `reset_reason` is unchanged), and the heap's `at most` figure stays under 96 KiB of its 128 KiB.
- **If it fails:** the heap is set in `start` in `crates/itch-firmware/src/radio.rs`: 64 KiB of the 72 KiB the bootloader leaves free, and 64 KiB of ordinary RAM. Taking more ordinary RAM shrinks the main stack, which `xtensa-esp32s3-elf-size -A` on the firmware shows as `.stack`.

## 22. Static address

The examples use `192.0.2.10/24` with gateway `192.0.2.1`: use a free address on the access point's network, outside its DHCP pool, and its gateway.

- **Method:**
  1. On a connected Rig, note its DHCP address. Over TCP, send `SetAddress` with mode static, `192.0.2.10`, prefix 24 and gateway `192.0.2.1`. Read `GetStatus` over USB every second.
  2. Connect to `tcp://192.0.2.10:7878`, send `Hello`, and ping the address from the PC.
  3. Over that connection, send `SetAddress` again with `192.0.2.11` and no gateway, then connect to `tcp://192.0.2.11:7878`.
  4. Power-cycle the Rig.
- **Pass:**
  - Each reply arrives over the TCP connection that sent it before that connection drops.
  - About 1 s after each reply, `GetStatus` shows mode static with the address, prefix and gateway as sent, and `current_address` is that address. `link` stays connected throughout: the Rig doesn't leave the network, and the access point lists it all along.
  - `Hello` and ping answer at each new address.
  - After the power cycle the Rig joins again at `192.0.2.11` by itself. Before it has joined, `GetStatus` shows mode static with `current_address` zero.
- **If it fails:** the board hands the configuration to the network stack only after the reply is queued (`apply_settings` in `crates/itch-firmware/src/board.rs`), and the `address` task in `crates/itch-firmware/src/radio.rs` applies it `LEAVE_DELAY` later. A `current_address` that keeps the old address after a static-to-static change means `address` didn't report the new one to `WIFI`: it looks every `ADDRESS_POLL`. A configuration that doesn't survive the power cycle is a settings matter (item 5).

## 23. Back to DHCP

- **Method:** on the Rig at its static address from item 22, send `SetAddress` over TCP with mode DHCP and every other field zero. Read `GetStatus` over USB, then power-cycle the Rig.
- **Pass:** the reply arrives over TCP before that connection drops. `GetStatus` shows mode DHCP with address, prefix and gateway zero, and within about 10 s `current_address` is the address the access point's DHCP lease table shows for the Rig's Hardware ID. `wifi_configured` and `ssid` are unchanged: the passphrase was never re-sent. The Rig takes a DHCP address again after the power cycle.
- **If it fails:** as item 22. A Rig that never gets a lease after the switch didn't recreate its DHCP client: check `config_v4` in `crates/itch-firmware/src/radio.rs`.

## 24. Wrong static address recovered over USB

Setup: the test pattern from item 6 (CH1 and CH16 on), with the meter or logic analyser on all 16 contacts.

- **Method:** over TCP, send `SetAddress` with a static address on no network the access point serves, such as `198.51.100.10/24` with gateway `198.51.100.1`. Try to reach the Rig over TCP at that address and at its old one for 1 minute. Then over USB, read `GetStatus` and send `SetAddress` with mode DHCP.
- **Pass:** the Rig is unreachable over Wi-Fi, but `link` stays connected and `current_address` is `198.51.100.10`: it doesn't fall back to DHCP or check for a conflict. USB answers normally throughout, and the contacts keep the test pattern. After the USB `SetAddress` the Rig takes a DHCP address and is reachable over TCP again, with no reboot: `uptime` keeps rising and `reset_reason` is unchanged.
- **If it fails:** USB is served apart from the network stack (`crates/itch-firmware/src/usb.rs`), so a USB stall here is a scheduling matter, as in item 19.

## 25. Discovery **[per-Rig]**

- **Method:** label the Rig `bench-3` and connect it to `ExampleRigNet` by DHCP. On the Linux PC run `avahi-browse -rt _itch._tcp` and `avahi-resolve -n itch-c02718.local`; on the Windows PC run `dns-sd -B _itch._tcp` and `dns-sd -L bench-3 _itch._tcp`. Connect a client to the address and port found and send `Hello`. Then give the Rig a static address with `SetAddress` and repeat.
- **Pass:** both PCs find one `bench-3` instance on port 7878 with host `itch-c02718.local` and TXT `hwid=60:55:F9:C0:27:18`, matching the Rig's Hardware ID. The hostname resolves to the Rig's `current_address`, the DHCP lease, then the static address. `Hello` over TCP reports the same Hardware ID and label as over USB.
- **If it fails:** each time the responder starts, the USB log says `advertising bench-3 as itch-c02718.local at …`; `mDNS didn't start` and `mDNS stopped` warnings give its error. The responder is `crates/itch-firmware/src/responder.rs` and its records are `Answers` in `crates/itch-firmware/src/mdns.rs`. If the Rig says it is advertising but no PC sees it, capture `udp port 5353` in Wireshark: an access point that isolates wireless clients or filters multicast drops the Rig's packets. Use `tcp://<current_address>:7878` meanwhile.

## 26. Renaming re-advertises

- **Method:** keep `avahi-browse -r _itch._tcp` running, so it follows changes. Over USB, rename the Rig `bench-4`, then a 64-byte label, then `bench-3`. Lock the Rig under another Holder and try renaming it again.
- **Pass:** within a few seconds of each rename, the browser removes the old instance and adds the new one, with the same host, address and TXT. The 64-byte label appears cut to its first 63 bytes. The refused rename changes nothing.
- **If it fails:** the board hands the label to mDNS at boot and after each saved rename (`RevA::advertise` in `crates/itch-firmware/src/board.rs`), and the responder starts again when `Wifi::changed_from` in `crates/itch-firmware/src/wifi.rs` wakes it, sending a goodbye for the old instance first. An old instance that stays listed until its 120 s TTL runs out means the goodbye was lost.

## 27. Re-advertising after a rejoin

- **Method:** keep `avahi-browse -r _itch._tcp` running. Reboot the access point. Then move the Rig to `ExampleRigNet2` and back, as in item 18, and finally power-cycle it. On a Rig using DHCP, also reserve a different address for its Hardware ID on the access point, with a short lease time, and wait for the lease to renew without the Rig leaving.
- **Pass:** each time the Rig is connected again, the browser lists it within a few seconds of `link` showing connected, at its new `current_address`, with nothing queried by hand. While it is away, its instance drops out within about 2 minutes. After the power cycle it is advertised under its saved label. Within a few seconds of the renewal, `current_address` is the reserved address, the USB log says `serving tcp://…` at it, and the browser lists the Rig there.
- **If it fails:** the responder only runs while `Wifi::network` reports connected, and the address it advertises comes from the `address` task in `crates/itch-firmware/src/radio.rs`. A renewal at a new address reaches it only by polling, every `ADDRESS_POLL`.

## 28. Wi-Fi settings survive a reflash

- **Method:** over USB, give the Rig credentials for `ExampleRigNet` with `SetWifi`, and the static address `192.0.2.10/24` with gateway `192.0.2.1` with `SetAddress`, as in item 22. Reflash with `cargo +esp flash-firmware` and read `GetStatus` over USB. Then flash a merged image saved with `--skip-padding` (README) and read it again. Afterwards, `SetAddress` with mode DHCP restores DHCP.
- **Pass:** after each reflash, `wifi_configured` is true, `ssid` is `ExampleRigNet`, and `mode`, `address`, `prefix` and `gateway` are as sent. The Rig joins `ExampleRigNet` again by itself at `192.0.2.10`, with no passphrase re-sent.
- **If it fails:** as item 5: the credentials and address configuration live in the same settings record as the label.
