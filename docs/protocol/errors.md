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
| `malformed_frames` | Payloads dropped for being under 5 bytes |
| `dropped_logs` | Log Events dropped because a connection's queue was full |
| `expander_faults` | Times the Channel expander was found faulty and re-initialised |
| `usb_tx_dropped` | Frames the USB transport cut short after its write timeout |
