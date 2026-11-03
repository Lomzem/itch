# Identity

A QA engineer labels a Rig for recognition and confirms its fixed Hardware ID
continues identifying the same Rig after a rename.

## Sub-features

- `identity-read`: inspect protocol, version, Hardware ID and Label through Hello.
- `identity-rename`: set a Label and observe it after reconnecting.
- `identity-hardware-id`: rename preserves Hardware ID.

## How to get to it (user POV)

Connect to the announced TCP address and send `Hello` and `SetLabel`, the same
protocol requests a script uses to identify and rename a Rig.

## Driving it with verify.py

Preconditions: freshly launched healthy simulator, no Lock, writable storage.

Run `python3 .agents/skills/verify-itch/verify.py identity --evidence .scratch/verification/identity-002`.

- **Read.** `Hello` (`0x01`) records the starting identity.
- **Rename.** `SetLabel` (`0x06`), empty Holder and `verification-bench`, succeeds.
- **Reconnect and inspect.** A new stream's `Hello` returns Label
  `verification-bench` and the original Hardware ID.
- **Restore.** Set the original Label. Wire responses and doctor identity remain
  in the evidence directory after process cleanup.

## Gotchas

Simulator settings last only for one process, so reconnect tests run-local
retention rather than flash persistence. The simulator does not advertise mDNS;
this journey cannot prove discovery responds to a rename.
