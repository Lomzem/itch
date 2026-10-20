# itch protocol

This folder is the source of truth for the protocol between a Rig (or `itch-sim`) and its clients. A client can be written from these files alone.

- `README.md` (this file): transports, framing, the message header, field types, ordering, Channel state, Locks and versioning.
- [`messages.md`](messages.md): a byte table and hex example per message.
- [`errors.md`](errors.md): error codes, log levels and counters.
- [`vectors-v3.json`](vectors-v3.json): the conformance vectors that every codec must reproduce. Every ` ```hex ` block in these docs is checked against it.

This is **protocol 3**.

## Transports

The Rig speaks one framed byte stream, the same over every transport:

- **USB:** the ESP32-S3's native USB Serial/JTAG port (VID `0x303A`, PID `0x1001`). The USB serial number is the Rig's Hardware ID. This is how QA reach a Rig.
- **TCP:** `itch-sim`, for development and integration tests without hardware. One TCP connection is one stream; concurrent connections share one simulated Rig. Rig firmware supports USB only (ADR 0006).

The Rig can't see connection boundaries over USB, so `Hello` is optional on the wire. Clients should still send it first on every connection. It identifies the Rig without changing Channels, log filtering or other Rig state.

## Framing

Each message travels as one TAFP frame (ADR 0001):

```text
0x00 | COBS( length u16 | payload | crc u16 ) | 0x00
```

- `length` is the payload length in bytes, little-endian.
- `crc` is CRC-16/CCITT-FALSE over `length | payload`, little-endian: polynomial `0x1021`, initial value `0xFFFF`, no reflection, no final XOR. The CRC of ASCII `123456789` is `0x29B1`.
- COBS is the standard encoding (Cheshire and Baker). When the data ends exactly at the end of a full 254-byte block, no extra `01` code follows.
- A payload is at most **1024 bytes**. Messages are never fragmented.

A `Hello` request with `request_id` `0x12345678` (payload `01 78 56 34 12`) is framed as:

```hex
00 02 05 08 01 78 56 34 12 c2 f2 00
```

A receiver treats every `0x00` as the end of a frame, which resynchronises it after noise or a mid-stream join. An empty frame (`00 00`) is ignored. Any other frame is decoded and then:

| Problem | What the Rig does |
| --- | --- |
| Bad CRC, truncated COBS, or a `length` that disagrees with the payload | Drops it and counts it in `crc_failures` |
| Payload over 1024 bytes | Drops it and counts it in `oversized_frames` |

Noise before a good frame costs one dropped frame. Here three noise bytes precede the `Hello` above, and the Rig counts one CRC failure and then answers the `Hello`:

```hex
ff 13 37 00 02 05 08 01 78 56 34 12 c2 f2 00
```

## Message header

A payload is one message. Its first byte, `type`, sets its kind:

| `type` | Kind |
| --- | --- |
| `0x01`–`0x3F` | Request (client to Rig) |
| `0x40`–`0x7F` | Event (Rig to client, unsolicited) |
| `0x80`–`0xFF` | Response (Rig to client): the request's `type \| 0x80` |

**Request:**

| Field | Type | Notes |
| --- | --- | --- |
| `type` | `u8` | See [`messages.md`](messages.md) |
| `request_id` | `u32` | Chosen by the client and echoed in the response |
| body | | Per message |

**Response:**

| Field | Type | Notes |
| --- | --- | --- |
| `type` | `u8` | The request's `type \| 0x80` |
| `request_id` | `u32` | The request's |
| `status` | `u8` | 0 = ok, otherwise an error code from [`errors.md`](errors.md) |
| body | | On ok, the result body (per message). On error, `detail: text`, which may be empty |

A `Hello` refused with `Busy` and detail `queue full`, an illustrative codec example of a nonempty detail; current firmware sends `Busy` with an empty detail:

```hex
81 78 56 34 12 07 0a 71 75 65 75 65 20 66 75 6c 6c
```

**Event:** `type u8` in `0x40`–`0x7F`, then the body. Events carry no `request_id`. Clients ignore Event types they don't know.

## Field types

| Type | Encoding |
| --- | --- |
| `u8`, `u16`, `u32`, `u64` | Unsigned, little-endian |
| `bool` | `u8`, 0 or 1 |
| `text` | `u8` byte length, then that many bytes of UTF-8 |
| `[u8; N]` | `N` raw bytes |

- Label text and Holder names are at most 64 bytes. A Wi-Fi SSID is at most 32 bytes and its passphrase 8 to 63.
- An IPv4 address is a `[u8; 4]` in network order (`c0 00 02 0a` is `192.0.2.10`).
- Enums are `u8`, counters `u32`, uptime a `u64` in milliseconds.
- The Hardware ID is 6 raw bytes, the factory MAC in transmission order (`60 55 f9 c0 27 18` is `60:55:F9:C0:27:18`).

### Strict decoding

Every message has an exact layout. The Rig answers `Malformed` (with the `request_id` echoed) to a body that is too short, has bytes left over, holds a `bool` other than 0 or 1, an unknown enum value, invalid UTF-8, over-long text or a field outside its range given in [`messages.md`](messages.md). A request `type` it doesn't know gets `UnknownRequest`. A payload under 5 bytes has no readable `request_id`, so the Rig drops it, counts it in `malformed_frames` and the client times out.

A `WriteChannels` with no Holder whose body stops a byte short gets `Malformed`, with its `request_id` echoed:

```hex
03 78 56 34 12 00 01 80 01
```

## Ordering

The Rig processes requests in order and replies in order. The one exception is `Busy`, which the transport sends at once when the Rig's request queue is full, so it may overtake replies still queued. Clients match replies by `request_id`. Clients may pipeline requests; `itch-py` sends one at a time.

Every change sets an absolute state, so retrying a request is safe. The Rig keeps no `request_id` history, and a client retries with a fresh `request_id`.

## Channel state

Any client may read a Rig, and any client may change an unlocked one, without presenting a token or sending a heartbeat. A Mask write is applied as one operation. On an unlocked Rig, separate requests from different clients may interleave; a script that needs the Rig to itself Locks it (see [Locks](#locks)).

A powered Rig retains its commanded Channel state when a script exits, crashes, sleeps or disconnects. Connecting, querying and closing a transport do not Clear it. Use `Clear` to turn every Channel off; power-up and Rig reset also Clear. Hardware supervision can temporarily hold Channels off during an expander fault and restores the commanded state after recovery, as described in [Expander faults](errors.md#expander-faults).

## Locks

A client may Lock a Rig under a Holder name, such as `alice@ws3`, so that nobody else's script can change it (ADR 0009). While a Rig is Locked, only requests carrying its Holder may change it; anyone may still look at it.

- Every changing request (`WriteChannels`, `Clear`, `SetLabel`, `SetWifi`) starts with a `holder` field. Empty means no Holder. The Rig keeps no Lock state per connection, so a client sends its Holder on every change.
- A changing request on a Locked Rig whose `holder` isn't the Holder, including an empty one, is refused with `Locked` and changes nothing. Its `detail` names the Holder for people to read. A client that needs the Holder reads it from `GetStatus`, never from `detail`.
- `Hello`, `GetChannels`, `GetStatus` and `SetLogLevel` carry no Holder and are never refused for a Lock.
- [`Lock`](messages.md#lock) Locks an unlocked Rig. The same Holder may Lock again with no effect, which is how a script that crashed or lost its connection picks its Lock back up. Another Holder is refused with `Locked`, unless it sets `takeover`, which hands the Lock to it: a Takeover. The old Holder's next change is then refused with `Locked` naming the new Holder.
- [`ReleaseLock`](messages.md#releaselock) by the Holder unlocks the Rig. Anyone else is refused with `Locked`; a Takeover is the way to remove someone else's Lock. Releasing an unlocked Rig succeeds and changes nothing.
- A Lock has no token, expiry or heartbeat. It lasts until its Holder releases it, someone takes it over or the Rig resets. It is never saved, so a Rig always boots unlocked.
- Locking, releasing and taking over never change the Channels.

A Holder name is a plain name, not a secret: the Lock guards against accidents, not bad actors. Clients should default it to something like `user@host`.

## Log delivery

`Log` Events are unsolicited and need no client requests or heartbeat. `SetLogLevel` selects which ones the stream receives and discards its queued backlog. USB has one stream for the life of the Rig: its level persists until changed or rebooted, so a new client should explicitly set its level, including off. A simulator TCP connection starts with logs off. `Hello` does not reset this setting.

## Versioning

`Hello` reports one `protocol` number, bumped on any change to any message. Clients require an exact match and refuse anything else. There are no per-frame or per-message versions and no version negotiation. The common response header and the first `u16` of a successful `Hello` result are stable: read the protocol number before decoding the rest of its version-specific body, so a mismatch can be reported without first treating the body as malformed.

The vectors file is named for the protocol it describes (`vectors-v3.json` for protocol 3).

## Conformance vectors

[`vectors-v3.json`](vectors-v3.json) is the one authority for the byte layout. Every codec must encode each message to exactly its payload and decode each payload to exactly its message.

Conventions:

- Hex strings (`payload`, `frame`, `stream`, `hardware_id`) are lowercase byte pairs separated by spaces. Readers should ignore whitespace and case.
- Integer fields are JSON numbers. Consumers must preserve the full range of `u64` uptime values.
- IPv4 addresses (`current_address`) are dotted-quad strings, such as `"192.0.2.10"`.

Top level:

| Key | Contents |
| --- | --- |
| `format` | Version of this JSON layout, currently 1 |
| `protocol` | The protocol the vectors describe |
| `messages` | Payload vectors: a message and its payload |
| `frames` | Frame vectors: a payload and its complete TAFP frame |
| `streams` | Decoder vectors: a byte stream and what a receiver gets from it |
| `malformed` | Strict-decoding vectors: payloads with a valid CRC that break their byte table |

Every case has a `name`, unique within its section.

**`messages`:** `{ "name", "message", "payload" }`. `message` is an object:

- `kind`: `"request"`, `"response"` or `"event"`.
- `type`: the message name from [`messages.md`](messages.md), such as `"Hello"`.
- `request_id`: for requests and responses.
- `status`: for responses, `"Ok"` or an error name from [`errors.md`](errors.md), such as `"Busy"`.
- Body fields, named as in the byte tables. A request carries its request body. An ok response carries its result body. An error response carries `detail`.

**`frames`:** `{ "name", "payload", "frame" }`. Encoding `payload` gives exactly `frame`, and decoding `frame` gives exactly `payload`.

**`streams`:** `{ "name", "stream", "payloads", "crc_failures", "oversized_frames" }`. Feeding `stream` to a fresh decoder yields exactly `payloads`, in order, and drops frames counted as given. These cover bad CRCs, truncated and oversized frames, empty frames and resync.

**`malformed`:** `{ "name", "payload", "error" }`. A strict decoder rejects `payload`, and `error` names how:

- `"Malformed"`: the payload breaks its byte table (too short, bytes left over, a bad `bool` or enum, invalid UTF-8, over-long text, a field out of its range such as an empty Holder in `Lock`), or is an ok reply to an unknown request type.
- `"UnknownRequest"`: a request whose `type` names no request.

A client rejects every case. A Rig, given a request case (`type` below `0x40`), answers with `error` as its `status` and the payload's `request_id` echoed, except that a payload under 5 bytes gets no reply and is counted in `malformed_frames`.
