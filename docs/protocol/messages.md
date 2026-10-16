# Messages

Each message lists its request body and its ok result body. The header (`type`, `request_id`, `status`) and the field types are in the [README](README.md). Error replies carry `detail: text` in place of the result body.

| Type | Request |
| --- | --- |
| `0x01` | [`Hello`](#hello) |

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
