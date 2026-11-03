# itch verification map

This directory owns the maintained map of simulator user journeys. Read this
index, then the selected feature file before driving. The primary entry point
is the TCP Rig protocol; physical USB and Wi-Fi paths remain unverified here.

## Baseline preconditions

Build with `cargo build --locked -p itch-sim`. Each helper invocation creates a
fresh loopback instance on an ephemeral port, with all Channels off, no Lock,
default Label, no credentials and DHCP. Doctor requires protocol 3, board `sim`,
16 Channels and the workspace version. No seed data is required.

## Driving conventions

Run `python3 .agents/skills/verify-itch/verify.py FEATURE --evidence NEW_DIRECTORY`
from the repository root. Use a fresh evidence directory per attempt. Each
invocation owns and tears down its own process; never attach it to real hardware
or another user's instance. `all` runs the four features in index order, each
restoring the state needed by the next feature.

## Proof and skip reporting

Retain `wire.jsonl`, `result.json` and `server.log` after teardown. Record the
feature ID, executable, request actions, readback results and cleanup outcome.
Exit 0 and `passed: true` prove only the selected journey on that binary. Report
failures and unmet preconditions; do not replace a failed journey with a more
convenient entry point. Protocol status in the simulator proves in-memory
configuration, not radio association, flash persistence or contact behavior.

## Features

- [Channels](channels.md): Mask writes, retained state after reconnect and Clear.
- [Locks](locks.md): exclusion, reconnect, Takeover and release.
- [Identity](identity.md): rename, reconnect and unchanged Hardware ID.
- [Network configuration](network.md): simulated DHCP, static address and clear.
