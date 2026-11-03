# Channels

A QA script changes selected Channels, retains their state after disconnecting,
and explicitly Clears them when finished.

## Sub-features

- `channels-write`: turn CH1 and CH16 on together.
- `channels-mask`: turn CH1 off without disturbing CH16.
- `channels-retain`: reconnect and read the retained state.
- `channels-clear`: Clear and read all Channels off.

## How to get to it (user POV)

Connect to the simulator's announced TCP address; send `GetChannels`,
`WriteChannels` and `Clear`, just as a script sends them to a Rig.

## Driving it with verify.py

Preconditions: freshly launched healthy simulator, unlocked and all Channels off.

Run `python3 .agents/skills/verify-itch/verify.py channels --evidence .scratch/verification/channels-003`.

- **Read.** `GetChannels` (`0x02`) returns state `0x0000`.
- **Write.** `WriteChannels` (`0x03`), empty Holder, mask and values `0x8001`,
  returns `0x8001`.
- **Mask.** Write mask `0x0001`, values `0x0000`; reply is `0x8000`.
- **Reconnect.** Close the stream, connect again, and read `0x8000`.
- **Clear.** `Clear` (`0x04`) with empty Holder returns zero; a separate
  `GetChannels` reads zero. `wire.jsonl` retains every action and response.

## Gotchas

Simulator Channel readback cannot prove the physical contacts settled or the
expander drove a DUT. The Rust TCP suite separately checks reply timing. A new
process resets state; use a reconnect within the same process for retention.
