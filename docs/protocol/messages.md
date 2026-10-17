# Messages

Each message lists its request body and its ok result body. The header (`type`, `request_id`, `status`) and the field types are in the [README](README.md). Error replies carry `detail: text` in place of the result body.

| Type | Request |
| --- | --- |
| `0x01` | [`Hello`](#hello) |
| `0x02` | [`GetChannels`](#getchannels) |
| `0x03` | [`WriteChannels`](#writechannels) |
| `0x04` | [`Clear`](#clear) |
| `0x07` | [`SetLogLevel`](#setloglevel) |
| `0x08` | [`AcquireLease`](#acquirelease) |
| `0x09` | [`RenewLease`](#renewlease) |
| `0x0A` | [`ReleaseLease`](#releaselease) |

| Type | Event |
| --- | --- |
| `0x40` | [`Log`](#log) |

Channel states are a `u16` with bit 0 = CH1 and bit 15 = CH16. A set bit means on (contact closed).

The examples use `request_id` `0x12345678`, Lease token `0x0123456789ABCDEF` and Holder `qa@bench-pc acme_test.py`.

## Hello

`0x01`. Identifies the Rig and its protocol. It needs no Lease. `itch-py` sends it first on every connection and refuses a Rig whose `protocol` differs from its own.

Request body: empty.

Result body:

| Field | Type | Notes |
| --- | --- | --- |
| `protocol` | `u16` | 1 |
| `firmware_version` | `text` | For example `1.0.0` |
| `dirty` | `bool` | Built from uncommitted code |
| `features` | `u8` | Bit 0 = `wifi`. Other bits are 0 |
| `channel_count` | `u8` | 16 |
| `board_revision` | `text` | For example `A` |
| `hardware_id` | `[u8; 6]` | The factory MAC |
| `label` | `text` | At most 64 bytes. Defaults to `itch-` plus the last three Hardware ID bytes in uppercase hex |

A client should warn when `dirty` is set or `features` has `wifi`: the Rig runs a development build.

Request with `request_id` `0x12345678`:

```hex
01 78 56 34 12
```

Reply from Rig `60:55:F9:C0:27:18`, firmware `1.0.0`, clean, no features, board revision `A`, default label `itch-C02718`:

```hex
81 78 56 34 12 00
01 00
05 31 2e 30 2e 30
00
00
10
01 41
60 55 f9 c0 27 18
0b 69 74 63 68 2d 43 30 32 37 31 38
```

## GetChannels

`0x02`. Reads the commanded Channel state. It needs no Lease.

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

`0x03`. A Mask write: each Channel whose `mask` bit is set takes its `values` bit, and every other Channel stays as it was. It needs the Lease (see [Lease](README.md#lease)). The reply goes out once the contacts have settled.

Request body:

| Field | Type | Notes |
| --- | --- | --- |
| `token` | `u64` | The Lease token |
| `mask` | `u16` | The Channels to change |
| `values` | `u16` | Their new states. Bits outside `mask` are ignored |

Result body:

| Field | Type | Notes |
| --- | --- | --- |
| `state` | `u16` | The state after settling |

Request turning CH1 on and CH3 off:

```hex
03 78 56 34 12
ef cd ab 89 67 45 23 01
05 00
01 00
```

Reply, with CH16 already on:

```hex
83 78 56 34 12 00
01 80
```

Refused with `LeaseRequired` because the request carried token 0:

```hex
83 78 56 34 12 04 00
```

## Clear

`0x04`. Turns every Channel off. It needs the Lease, and the reply goes out once the contacts have settled.

Request body:

| Field | Type | Notes |
| --- | --- | --- |
| `token` | `u64` | The Lease token |

Result body:

| Field | Type | Notes |
| --- | --- | --- |
| `state` | `u16` | The state after settling, always 0 |

Request:

```hex
04 78 56 34 12
ef cd ab 89 67 45 23 01
```

Reply:

```hex
84 78 56 34 12 00
00 00
```

## SetLogLevel

`0x07`. Sets which Log Events the Rig sends on this connection. It needs no Lease. The level is per connection and starts at off; over USB it lasts until the Rig reboots or the level changes. Setting it discards Log Events still queued for this connection, so a new script never receives an earlier one's backlog. `itch-py` sends it right after `Hello`, at off unless the user opted in.

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

## AcquireLease

`0x08`. Takes the Lease. A retried acquire with the current token succeeds.

Request body:

| Field | Type | Notes |
| --- | --- | --- |
| `token` | `u64` | A random non-zero token chosen by the client |
| `takeover` | `bool` | Replace a live Holder at once |
| `holder` | `text` | At most 64 bytes. Describes the client to other users, such as `user@host script.py` |

Result body:

| Field | Type | Notes |
| --- | --- | --- |
| `ttl_ms` | `u32` | How long the Lease lives without a renewal: 10000 |

Request:

```hex
08 78 56 34 12
ef cd ab 89 67 45 23 01
00
18 71 61 40 62 65 6e 63 68 2d 70 63 20 61 63 6d 65 5f 74 65 73 74 2e 70 79
```

Reply:

```hex
88 78 56 34 12 00
10 27 00 00
```

## RenewLease

`0x09`. Keeps the Lease alive. Clients send it every third of `ttl_ms`.

Request body:

| Field | Type | Notes |
| --- | --- | --- |
| `token` | `u64` | The Lease token |
| `holder` | `text` | At most 64 bytes, as in `AcquireLease` |

Result body: empty.

Request:

```hex
09 78 56 34 12
ef cd ab 89 67 45 23 01
18 71 61 40 62 65 6e 63 68 2d 70 63 20 61 63 6d 65 5f 74 65 73 74 2e 70 79
```

Reply:

```hex
89 78 56 34 12 00
```

## ReleaseLease

`0x0A`. Frees the Rig and leaves the Channels as they are. A token that doesn't hold the Lease makes it a no-op, so releasing twice is safe.

Request body:

| Field | Type | Notes |
| --- | --- | --- |
| `token` | `u64` | The Lease token |

Result body: empty.

Request:

```hex
0a 78 56 34 12
ef cd ab 89 67 45 23 01
```

Reply:

```hex
8a 78 56 34 12 00
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
