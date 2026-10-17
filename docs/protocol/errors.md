# Errors, log levels and counters

## Error codes

A response's `status` is 0 on success, otherwise one of these codes. `detail` is free text for people, never for parsing. Clients treat an unknown code as a generic failure.

| Code | Name | Meaning |
| --- | --- | --- |
| 1 | `UnknownRequest` | The request `type` is unknown |
| 2 | `UnsupportedVersion` | The request needs a protocol version the Rig doesn't speak |
| 3 | `Malformed` | The request body breaks its byte table (see Strict decoding in the README) |
| 4 | `LeaseRequired` | A change request carried token 0 |
| 5 | `LeaseHeld` | `AcquireLease` without `takeover` while another client holds a live Lease. `detail` names the Holder |
| 6 | `LeaseLost` | A token-bearing request while another client holds a live Lease. `detail` names the Holder |
| 7 | `Busy` | The Rig's request queue is full. Sent at once, so it may overtake queued replies |
| 8 | `HardwareFault` | The Channel expander is unreachable or held in reset |
| 9 | `StorageFault` | The Rig couldn't persist a setting |
| 10 | `Internal` | A Rig bug |

## Expander faults

The Rig checks its Channel expander on every idle tick, about every 50 ms. If the expander doesn't answer, or no longer drives the commanded Channels, the Rig re-initialises it, drives the commanded Channels again and counts one `expander_faults`. A write that fails is recovered the same way before it is answered.

If the expander doesn't come back, the Rig holds it in reset, which turns every Channel off, and retries on every tick. Meanwhile `WriteChannels` and `Clear` are refused with `HardwareFault`, and `GetChannels` reports every Channel off. A refused request changes nothing, so once the expander is back, the Rig restores the Channels last commanded.

## Log levels

| Level | Name |
| --- | --- |
| 0 | off |
| 1 | error |
| 2 | warn |
| 3 | info |
| 4 | debug |

The level is per connection and defaults to off. Over USB it lasts until the Rig reboots or the level changes. [`SetLogLevel`](messages.md#setloglevel) sets it, and a connection receives [`Log`](messages.md#log) Events at its level and below. The Rig has no trace level; its trace output goes out as debug.

## Counters

`GetStatus` reports these `u32` counters, each counting since boot and wrapping at 2^32:

| Counter | Counts |
| --- | --- |
| `crc_failures` | Frames dropped for a bad CRC, truncated COBS or a wrong `length` |
| `oversized_frames` | Frames dropped for a payload over 1024 bytes |
| `malformed_frames` | Payloads dropped for being under 5 bytes, too short to hold a `request_id` |
| `dropped_logs` | Log Events dropped because a connection's queue was full |
| `expander_faults` | Times the Channel expander was found faulty and re-initialised |
| `usb_tx_dropped` | Frames the USB transport cut short after its write timeout |

## Reset reasons

`GetStatus` reports the ESP32-S3's raw reset reason (`SocResetReason` in esp-hal). Clients should show the name, and the number for a value not listed here.

| Value | Name | Cause |
| --- | --- | --- |
| 0 | — | Unknown |
| `0x01` | `ChipPowerOn` | Power-on (also brown-out or super watchdog on some chips) |
| `0x03` | `CoreSw` | Software reset of the digital core, such as after a panic |
| `0x05` | `CoreDeepSleep` | Wake from deep sleep |
| `0x07` | `CoreMwdt0` | Main watchdog 0 reset the digital core |
| `0x08` | `CoreMwdt1` | Main watchdog 1 reset the digital core |
| `0x09` | `CoreRtcWdt` | RTC watchdog reset the digital core |
| `0x0B` | `CpuMwdt0` | Main watchdog 0 reset a CPU |
| `0x0C` | `CpuSw` | Software reset of a CPU |
| `0x0D` | `CpuRtcWdt` | RTC watchdog reset a CPU |
| `0x0F` | `SysBrownOut` | Supply voltage dropped |
| `0x10` | `SysRtcWdt` | RTC watchdog reset the digital core and RTC |
| `0x11` | `CpuMwdt1` | Main watchdog 1 reset a CPU |
| `0x12` | `SysSuperWdt` | Super watchdog reset the digital core and RTC |
| `0x13` | `SysClkGlitch` | Clock glitch |
| `0x14` | `CoreEfuseCrc` | eFuse CRC error |
| `0x15` | `CoreUsbUart` | USB UART reset the digital core |
| `0x16` | `CoreUsbJtag` | USB Serial/JTAG reset the digital core, as espflash does |
| `0x17` | `CorePwrGlitch` | Power glitch |

`itch-sim` always reports `ChipPowerOn`.
