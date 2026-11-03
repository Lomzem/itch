# Locks

A QA script Locks a Rig under a Holder, blocking accidental changes from other
scripts until release or deliberate Takeover, without altering Channels.

## Sub-features

- `locks-reconnect`: a Lock survives disconnection.
- `locks-refuse`: a different Holder's write is refused without changing Channels.
- `locks-takeover`: a new Holder takes over and the old Holder is refused.
- `locks-release`: release permits unheld operations and preserves Channels.

## How to get to it (user POV)

Send `Lock`, Holder-bearing `WriteChannels`, and `ReleaseLock` over the TCP
connection used by the script. Anyone can still send `GetChannels`.

## Driving it with verify.py

Preconditions: freshly launched healthy simulator, unlocked and all Channels off.

Run `python3 .agents/skills/verify-itch/verify.py locks --evidence .scratch/verification/locks-002`.

- **Lock and reconnect.** `Lock` (`0x0B`) as `alice`, takeover false, succeeds;
  close that connection and open another.
- **Refuse.** `bob` writes CH1 on; status `Locked` (11) and readback zero prove
  refusal without a Channel change.
- **Take over.** `bob` Locks with takeover true; `alice`'s next write gets
  `Locked`. `bob`'s write succeeds and returns state `0x0001`.
- **Release.** `ReleaseLock` (`0x0C`) as `bob` succeeds; readback remains one.
  An empty-Holder `Clear` then succeeds and reads back zero.

## Gotchas

Holder names are public names, not authentication secrets. A new process
removes every Lock. Disconnecting a stream does not release one. Test release
by an unheld change, not by assuming a successful reply removed the Lock.
