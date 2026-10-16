# itch protocol

This folder is the source of truth for the protocol between a Rig (or `itch-sim`) and its clients. A client can be written from these files alone.

- `README.md` (this file): transports, framing, the message header, field types, ordering and versioning.
- [`messages.md`](messages.md): a byte table and hex example per message.
- [`errors.md`](errors.md): error codes, log levels and counters.
- [`vectors-v1.json`](vectors-v1.json): the conformance vectors that every codec must reproduce. Every ` ```hex ` block in these docs is checked against it.

This is **protocol 1**.

## Transports

The Rig speaks one framed byte stream, the same over every transport:

- **USB:** the ESP32-S3's native USB Serial/JTAG port (VID `0x303A`, PID `0x1001`). The USB serial number is the Rig's Hardware ID. This is how QA reach a Rig.
- **TCP:** `itch-sim`, and firmware built with the `wifi` feature (development only, see ADR 0003). One TCP connection is one stream.

The Rig can't see connection boundaries over USB, so `Hello` is optional on the wire. Clients should still send it first on every connection.

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

A `Hello` refused with `Busy` and detail `queue full`:

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

- Holder and Label text are at most 64 bytes.
- Enums are `u8`, counters `u32`, uptime a `u64` in milliseconds.
- The Hardware ID is 6 raw bytes, the factory MAC in transmission order (`60 55 f9 c0 27 18` is `60:55:F9:C0:27:18`).
- A Lease token is a `u64`. Token 0 is reserved to mean "no token".

### Strict decoding

Every message has an exact layout. The Rig answers `Malformed` (with the `request_id` echoed) to a body that is too short, has bytes left over, holds a `bool` other than 0 or 1, an unknown enum value, invalid UTF-8 or over-long text. A request `type` it doesn't know gets `UnknownRequest`. A payload under 5 bytes has no readable `request_id`, so the Rig drops it and the client times out.

## Ordering

The Rig processes requests in order and replies in order. The one exception is `Busy`, which the transport sends at once when the Rig's request queue is full, so it may overtake replies still queued. Clients match replies by `request_id`. Clients may pipeline requests; `itch-py` sends one at a time.

Every change sets an absolute state, so retrying a request is safe. The Rig keeps no `request_id` history, and a client retries with a fresh `request_id`.

## Versioning

`Hello` reports one `protocol` number, bumped on any change to any message. Clients require an exact match and refuse anything else. There are no per-frame or per-message versions.

The vectors file is named for the protocol it describes (`vectors-v1.json` for protocol 1).

## Conformance vectors

[`vectors-v1.json`](vectors-v1.json) is the one authority for the byte layout. Every codec must encode each message to exactly its payload and decode each payload to exactly its message.

Conventions:

- Hex strings (`payload`, `frame`, `stream`, `hardware_id`) are lowercase byte pairs separated by spaces. Readers should ignore whitespace and case.
- `u64` values (Lease tokens) are hex strings with a `0x` prefix, since JSON numbers lose precision above 2^53. Other integers are JSON numbers.

Top level:

| Key | Contents |
| --- | --- |
| `format` | Version of this JSON layout, currently 1 |
| `protocol` | The protocol the vectors describe |
| `messages` | Payload vectors: a message and its payload |
| `frames` | Frame vectors: a payload and its complete TAFP frame |
| `streams` | Decoder vectors: a byte stream and what a receiver gets from it |

Every case has a `name`, unique within its section.

**`messages`:** `{ "name", "message", "payload" }`. `message` is an object:

- `kind`: `"request"`, `"response"` or `"event"`.
- `type`: the message name from [`messages.md`](messages.md), such as `"Hello"`.
- `request_id`: for requests and responses.
- `status`: for responses, `"Ok"` or an error name from [`errors.md`](errors.md), such as `"Busy"`.
- Body fields, named as in the byte tables. A request carries its request body. An ok response carries its result body. An error response carries `detail`.

**`frames`:** `{ "name", "payload", "frame" }`. Encoding `payload` gives exactly `frame`, and decoding `frame` gives exactly `payload`.

**`streams`:** `{ "name", "stream", "payloads", "crc_failures", "oversized_frames" }`. Feeding `stream` to a fresh decoder yields exactly `payloads`, in order, and drops frames counted as given. These cover bad CRCs, truncated and oversized frames, empty frames and resync.
