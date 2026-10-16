# Messages

Each message lists its request body and its ok result body. The header (`type`, `request_id`, `status`) and the field types are in the [README](README.md). Error replies carry `detail: text` in place of the result body.

| Type | Request |
| --- | --- |
| `0x01` | [`Hello`](#hello) |
| `0x07` | [`SetLogLevel`](#setloglevel) |

| Type | Event |
| --- | --- |
| `0x40` | [`Log`](#log) |

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
