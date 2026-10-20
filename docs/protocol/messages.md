# Messages

Each message lists its request body and its ok result body. The header (`type`, `request_id`, `status`) and the field types are in the [README](README.md). Error replies carry `detail: text` in place of the result body.

| Type | Request |
| --- | --- |
| `0x01` | [`Hello`](#hello) |
| `0x02` | [`GetChannels`](#getchannels) |
| `0x03` | [`WriteChannels`](#writechannels) |
| `0x04` | [`Clear`](#clear) |
| `0x05` | [`GetStatus`](#getstatus) |
| `0x06` | [`SetLabel`](#setlabel) |
| `0x07` | [`SetLogLevel`](#setloglevel) |
| `0x0B` | [`Lock`](#lock) |
| `0x0C` | [`ReleaseLock`](#releaselock) |
| `0x0D` | [`SetWifi`](#setwifi) |
| `0x0E` | [`SetAddress`](#setaddress) |

| Type | Event |
| --- | --- |
| `0x40` | [`Log`](#log) |

Types `0x08` to `0x0A` belonged to protocol 1 and are unused.

Channel states are a `u16` with bit 0 = CH1 and bit 15 = CH16. A set bit means on (contact closed).

The examples use `request_id` `0x12345678`.

## Hello

`0x01`. Identifies the Rig and its protocol without changing Rig state. Clients should send it first and refuse a Rig whose `protocol` differs from their own. Read the protocol number before decoding the rest of the result body (see [Versioning](README.md#versioning)).

Request body: empty.

Result body:

| Field | Type | Notes |
| --- | --- | --- |
| `protocol` | `u16` | 3 |
| `firmware_version` | `text` | For example `1.0.0` |
| `dirty` | `bool` | Built from uncommitted code |
| `channel_count` | `u8` | 16 |
| `board_revision` | `text` | For example `A` |
| `hardware_id` | `[u8; 6]` | The factory MAC |
| `label` | `text` | At most 64 bytes. Defaults to `itch-` plus the last three Hardware ID bytes in uppercase hex |

A client should warn when `dirty` is set: the Rig was built from uncommitted code.

Request with `request_id` `0x12345678`:

```hex
01 78 56 34 12
```

Reply from Rig `60:55:F9:C0:27:18`, firmware `1.0.0`, clean, board revision `A`, default label `itch-C02718`:

```hex
81 78 56 34 12 00
03 00
05 31 2e 30 2e 30
00
10
01 41
60 55 f9 c0 27 18
0b 69 74 63 68 2d 43 30 32 37 31 38
```

## GetChannels

`0x02`. Reads the commanded Channel state. While the expander is held in reset (see [Expander faults](errors.md#expander-faults)), it reports every Channel off.

Request body: empty.

Result body:

| Field | Type | Notes |
| --- | --- | --- |
| `state` | `u16` | The commanded state |

Request:

```hex
02 78 56 34 12
```

Reply with CH1 and CH16 on:

```hex
82 78 56 34 12 00
01 80
```

## WriteChannels

`0x03`. A Mask write: each Channel whose `mask` bit is set takes its `values` bit, and every other Channel stays as it was. The reply goes out once the contacts have settled. While the expander is held in reset it is refused with `HardwareFault`. On a Rig Locked by someone else it is refused with `Locked` (see [Locks](README.md#locks)).

Request body:

| Field | Type | Notes |
| --- | --- | --- |
| `holder` | `text` | At most 64 bytes. Empty = no Holder. See [Locks](README.md#locks) |
| `mask` | `u16` | The Channels to change |
| `values` | `u16` | Their new states. Bits outside `mask` are ignored |

Result body:

| Field | Type | Notes |
| --- | --- | --- |
| `state` | `u16` | The state after settling |

Request from Holder `alice@ws3` turning CH1 on and CH3 off:

```hex
03 78 56 34 12
09 61 6c 69 63 65 40 77 73 33
05 00
01 00
```

The same request with no Holder:

```hex
03 78 56 34 12
00
05 00
01 00
```

Reply, with CH16 already on:

```hex
83 78 56 34 12 00
01 80
```

Refused with `Locked` on a Rig held by `bob@ws7`:

```hex
83 78 56 34 12 0b
07 62 6f 62 40 77 73 37
```

## Clear

`0x04`. Turns every Channel off. The reply goes out once the contacts have settled. While the expander is held in reset it is refused with `HardwareFault`. On a Rig Locked by someone else it is refused with `Locked` (see [Locks](README.md#locks)).

Request body:

| Field | Type | Notes |
| --- | --- | --- |
| `holder` | `text` | At most 64 bytes. Empty = no Holder. See [Locks](README.md#locks) |

Result body:

| Field | Type | Notes |
| --- | --- | --- |
| `state` | `u16` | The state after settling, always 0 |

Request from Holder `alice@ws3`:

```hex
04 78 56 34 12
09 61 6c 69 63 65 40 77 73 33
```

Reply:

```hex
84 78 56 34 12 00
00 00
```

## GetStatus

`0x05`. Reports the Rig's health since boot, who holds its Lock, its Wi-Fi network and its address. It never reports the Wi-Fi passphrase.

Request body: empty.

Result body:

| Field | Type | Notes |
| --- | --- | --- |
| `uptime_ms` | `u64` | Milliseconds since boot |
| `reset_reason` | `u8` | Why the Rig last reset: a raw ESP32-S3 `SocResetReason`, listed in [`errors.md`](errors.md#reset-reasons). 0 = unknown |
| `last_panic` | `text` | Where the previous boot panicked, such as `src/main.rs:88:5`. At most 64 bytes; a longer location keeps its end. Empty = no panic |
| `crc_failures` | `u32` | |
| `oversized_frames` | `u32` | |
| `malformed_frames` | `u32` | |
| `dropped_logs` | `u32` | |
| `expander_faults` | `u32` | |
| `usb_tx_dropped` | `u32` | |
| `holder` | `text` | Who holds the Lock. Empty = unlocked |
| `wifi_configured` | `bool` | The Rig has Wi-Fi credentials |
| `ssid` | `text` | The network the credentials are for. At most 32 bytes. Empty exactly when `wifi_configured` is 0 |
| `link` | `u8` | The Wi-Fi link: 0 off (no credentials, or the radio is off), 1 joining, 2 connected. Always 0 when `wifi_configured` is 0. Anything higher is `Malformed` |
| `current_address` | `[u8; 4]` | The Rig's IPv4 address on the network, in network order (`c0 00 02 0a` is `192.0.2.10`). Non-zero when connected, zero otherwise |
| `mode` | `u8` | How the Rig is set to take its address, whether or not it is connected: 0 DHCP, 1 static. Anything higher is `Malformed` |
| `address` | `[u8; 4]` | The static address. Zero for DHCP |
| `prefix` | `u8` | The static prefix length. Zero for DHCP |
| `gateway` | `[u8; 4]` | The static gateway. Zero for DHCP or no gateway |

A reply whose fields break these rules, such as DHCP with a non-zero `address` or a joining `link` with a non-zero `current_address`, is `Malformed`. `mode`, `address`, `prefix` and `gateway` follow the rules of [`SetAddress`](#setaddress).

The counters are described in [`errors.md`](errors.md#counters). Each counts since boot and wraps at 2^32.

Request:

```hex
05 78 56 34 12
```

Reply from a Rig up for 1 h 2 min 3 s after a software reset, whose previous boot panicked at `src/main.rs:88:5`, with counters 1 to 6 in order, Locked by `alice@ws3`, connected to the network `ExampleRigNet` at its static address `192.0.2.10/24` with gateway `192.0.2.1`:

```hex
85 78 56 34 12 00
f8 ce 38 00 00 00 00 00
03
10 73 72 63 2f 6d 61 69 6e 2e 72 73 3a 38 38 3a 35
01 00 00 00
02 00 00 00
03 00 00 00
04 00 00 00
05 00 00 00
06 00 00 00
09 61 6c 69 63 65 40 77 73 33
01
0d 45 78 61 6d 70 6c 65 52 69 67 4e 65 74
02
c0 00 02 0a
01
c0 00 02 0a
18
c0 00 02 01
```

## SetLabel

`0x06`. Renames the Rig. The Rig saves the label before it replies, so the reply means the label survives a reboot. An empty label, or the default label by name, restores the default `itch-` plus the last three Hardware ID bytes in uppercase hex, such as `itch-C02718`. The default is never saved. A failed save gets `StorageFault` and keeps the old label. On a Rig Locked by someone else it is refused with `Locked` (see [Locks](README.md#locks)).

Request body:

| Field | Type | Notes |
| --- | --- | --- |
| `holder` | `text` | At most 64 bytes. Empty = no Holder. See [Locks](README.md#locks) |
| `label` | `text` | At most 64 bytes. Empty = restore the default |

Result body:

| Field | Type | Notes |
| --- | --- | --- |
| `label` | `text` | The label the Rig now reports, as in `Hello` |

Request from Holder `alice@ws3` to rename the Rig `bench3`:

```hex
06 78 56 34 12
09 61 6c 69 63 65 40 77 73 33
06 62 65 6e 63 68 33
```

Reply:

```hex
86 78 56 34 12 00
06 62 65 6e 63 68 33
```

Request with no Holder to restore the default:

```hex
06 78 56 34 12
00
00
```

Reply on the Rig with Hardware ID `60:55:F9:C0:27:18`:

```hex
86 78 56 34 12 00
0b 69 74 63 68 2d 43 30 32 37 31 38
```

Refused with `StorageFault`:

```hex
86 78 56 34 12 09
00
```

## SetLogLevel

`0x07`. Sets which Log Events the Rig sends on this connection. The level is per connection and starts at off; over USB it lasts until the Rig reboots or the level changes. Setting it discards Log Events still queued for this connection, so a new script never receives an earlier one's backlog. Clients should send it after `Hello`, at off unless the user opted in. Log Events are emitted as they occur, even while the client sends no requests.

Request body:

| Field | Type | Notes |
| --- | --- | --- |
| `level` | `u8` | 0 off to 4 debug, from [`errors.md`](errors.md#log-levels). Anything higher gets `Malformed` |

Result body: empty.

Request with `request_id` `0x12345678`, level info:

```hex
07 78 56 34 12 03
```

Reply:

```hex
87 78 56 34 12 00
```

## Lock

`0x0B`. Locks the Rig under `holder`, so that only requests carrying it may change the Rig (see [Locks](README.md#locks)). Locking an unlocked Rig, or Locking again under the current Holder, succeeds. On a Rig Locked by someone else it is refused with `Locked`, whose `detail` names the Holder, unless `takeover` is set: then the Lock passes to `holder`. Never changes the Channels.

Request body:

| Field | Type | Notes |
| --- | --- | --- |
| `holder` | `text` | 1 to 64 bytes. Empty gets `Malformed` |
| `takeover` | `bool` | Take the Lock from another Holder |

Result body: empty.

Request from `alice@ws3`:

```hex
0b 78 56 34 12
09 61 6c 69 63 65 40 77 73 33
00
```

Request from `bob@ws7` to take the Lock over:

```hex
0b 78 56 34 12
07 62 6f 62 40 77 73 37
01
```

Reply:

```hex
8b 78 56 34 12 00
```

Refused with `Locked` on a Rig held by `alice@ws3`:

```hex
8b 78 56 34 12 0b
09 61 6c 69 63 65 40 77 73 33
```

## ReleaseLock

`0x0C`. Unlocks the Rig if `holder` holds its Lock. Anyone else is refused with `Locked`, whose `detail` names the Holder. Releasing an unlocked Rig succeeds and changes nothing. Never changes the Channels.

Request body:

| Field | Type | Notes |
| --- | --- | --- |
| `holder` | `text` | At most 64 bytes |

Result body: empty.

Request from `alice@ws3`:

```hex
0c 78 56 34 12
09 61 6c 69 63 65 40 77 73 33
```

Reply:

```hex
8c 78 56 34 12 00
```

## SetWifi

`0x0D`. Gives the Rig the Wi-Fi network to join, as an SSID and its WPA2-Personal passphrase, or clears them with an empty SSID and passphrase. The Rig saves the credentials before it replies, so the reply means they survive a reboot; it keeps its label and every other setting. A failed save gets `StorageFault` and keeps the old credentials. The Rig then applies them: it leaves its current network and joins the new one, so a change made over Wi-Fi drops that connection after the reply. A Rig with no credentials keeps its radio off and works over USB alone. `GetStatus` reports the SSID, but never the passphrase. On a Rig Locked by someone else it is refused with `Locked` (see [Locks](README.md#locks)).

Request body:

| Field | Type | Notes |
| --- | --- | --- |
| `holder` | `text` | At most 64 bytes. Empty = no Holder. See [Locks](README.md#locks) |
| `ssid` | `text` | 1 to 32 bytes. Empty = clear the credentials |
| `passphrase` | `text` | 8 to 63 bytes, or empty with an empty `ssid`. Anything else is `Malformed` |

Result body: empty.

Request from Holder `alice@ws3` to join `ExampleRigNet` with passphrase `example-passphrase`:

```hex
0d 78 56 34 12
09 61 6c 69 63 65 40 77 73 33
0d 45 78 61 6d 70 6c 65 52 69 67 4e 65 74
12 65 78 61 6d 70 6c 65 2d 70 61 73 73 70 68 72 61 73 65
```

Request with no Holder to clear the credentials:

```hex
0d 78 56 34 12
00
00
00
```

Reply:

```hex
8d 78 56 34 12 00
```

Refused with `StorageFault`:

```hex
8d 78 56 34 12 09
00
```

## SetAddress

`0x0E`. Sets how the Rig takes its IPv4 address on the network: by DHCP, the default, or a static address, prefix length and optional gateway. It leaves the Wi-Fi credentials alone, so re-addressing a Rig never needs the passphrase. The Rig saves the configuration before it replies, so the reply means it survives a reboot; it keeps its label and every other setting. A failed save gets `StorageFault` and keeps the old configuration. The Rig then applies it, straight away if it is connected and otherwise when it next joins, so a change made over Wi-Fi may drop that connection after the reply. The Rig uses exactly the address given: it checks for no conflict and never falls back to DHCP. It has no DNS servers, since it never resolves names or opens connections. A mis-addressed Rig is fixed over USB (see [Transports](README.md#transports)). On a Rig Locked by someone else it is refused with `Locked` (see [Locks](README.md#locks)).

Request body:

| Field | Type | Notes |
| --- | --- | --- |
| `holder` | `text` | At most 64 bytes. Empty = no Holder. See [Locks](README.md#locks) |
| `mode` | `u8` | 0 DHCP, 1 static. Anything higher is `Malformed` |
| `address` | `[u8; 4]` | Static: a unicast address, so not zero, not `255.255.255.255` and not multicast (`224.0.0.0` to `239.255.255.255`). DHCP: zero |
| `prefix` | `u8` | Static: 1 to 32. DHCP: zero |
| `gateway` | `[u8; 4]` | Static: zero = no gateway, otherwise a unicast address as for `address`. DHCP: zero |

Anything else, such as DHCP with a non-zero field or a multicast static address, is `Malformed`. `GetStatus` reports the configuration as `mode`, `address`, `prefix` and `gateway`, beside the `current_address` the Rig has.

Result body: empty.

Request from Holder `alice@ws3` for the static address `192.0.2.10/24` with gateway `192.0.2.1`:

```hex
0e 78 56 34 12
09 61 6c 69 63 65 40 77 73 33
01
c0 00 02 0a
18
c0 00 02 01
```

Request with no Holder to go back to DHCP:

```hex
0e 78 56 34 12
00
00
00 00 00 00
00
00 00 00 00
```

Reply:

```hex
8e 78 56 34 12 00
```

Refused with `StorageFault`:

```hex
8e 78 56 34 12 09
00
```

## Log

`0x40`, an Event. One line of the Rig's log, sent to each connection whose level lets it through. Each connection has a bounded queue; when it is full, new lines are dropped and counted in `dropped_logs`.

Body:

| Field | Type | Notes |
| --- | --- | --- |
| `level` | `u8` | 1 error to 4 debug |
| `dropped` | `u32` | Log Events this connection dropped since the last one sent |
| `text` | | UTF-8 to the end of the frame, with no length byte. The Rig truncates a longer line on a character boundary to fit the 1024-byte payload, leaving at most 1018 bytes of text |

A warning, after 3 dropped Log Events, with text `label set to café`:

```hex
40 02 03 00 00 00
6c 61 62 65 6c 20 73 65 74 20 74 6f 20 63 61 66 c3 a9
```
