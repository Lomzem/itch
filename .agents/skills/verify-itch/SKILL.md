---
name: verify-itch
description: Verify itch protocol journeys through a real local itch-sim TCP process when changing Rig behavior, simulator behavior, or protocol handling.
---

# Verify itch

Use the [feature map](features/README.md) to select a journey. This checkout's
primary scriptable surface is the Rig protocol. The harness drives the actual
`itch-sim` executable over TCP, using Python 3.9+ standard-library sockets.
Firmware USB, Wi-Fi association, mDNS and physical Channel contacts require a
Rig and are outside this simulator proof. The Python client belongs to itch-py;
this harness needs no sibling checkout or client package.

## Launch

Run from the repository root with Rust stable, Cargo, Git and Python 3.9+:

```sh
cargo build --locked -p itch-sim
python3 .agents/skills/verify-itch/verify.py channels --evidence .scratch/verification/channels-001
```

Each invocation starts `target/debug/itch-sim --bind 127.0.0.1:0`, waits up to
five seconds for `itch-sim listening on tcp://127.0.0.1:PORT`, then requires a
successful read-only `Hello` before driving. No environment variables, auth,
migrations, data directories or seeds are required. A fresh process starts
unlocked, all Channels off, default Label, no credentials and DHCP. All simulator
settings are in memory. Each run owns a separate process and OS-assigned port;
parallel runs need distinct evidence directories.

The command tears down its child on success or failure. Exit 0 and
`result.json` with `passed: true` mean the selected journey passed. A startup
failure records its exact exception and retains logs. Rebuild after source
changes; `--binary /absolute/path/to/itch-sim` selects another already-built
binary, including a Windows `.exe` or a custom Cargo target directory.

## Doctor

The harness runs doctor against its own child before every journey: the process
must be alive, `Hello` must answer with protocol 3, 16 Channels, board revision
`sim`, and the workspace package version. It captures the returned version,
dirty flag, Hardware ID and Label. It never connects to an existing Rig.

For a standalone read-only readiness check:

```sh
python3 .agents/skills/verify-itch/verify.py doctor --evidence .scratch/verification/doctor-001
```

This starts and checks a fresh owned instance. When anything looks off, run this
first and inspect `result.json`, `server.log` and `wire.jsonl`; a matching package
version alone does not prove source freshness, so rebuild the binary first.

## Drive

```sh
python3 .agents/skills/verify-itch/verify.py channels --evidence .scratch/verification/channels-002
python3 .agents/skills/verify-itch/verify.py locks --evidence .scratch/verification/locks-001
python3 .agents/skills/verify-itch/verify.py identity --evidence .scratch/verification/identity-001
python3 .agents/skills/verify-itch/verify.py network --evidence .scratch/verification/network-001
python3 .agents/skills/verify-itch/verify.py all --evidence .scratch/verification/all-001
```

Each feature file names the exact requests and observable results. The harness
uses the production TAFP stream and protocol-3 messages from `docs/protocol/`.
It checks response IDs, status, lengths and CRCs. No internal setters, test-only
endpoints or mocked protocol responses are used. The existing broader boundary
suite is `cargo test -p itch-sim`; use it for framing counters, fault injection,
log filtering and the four-connection limit beyond this initial feature map.

## Evidence

Choose a new evidence directory for every attempt; existing directories are
refused so earlier proof cannot be overwritten. Evidence survives teardown:

- `wire.jsonl` records each outgoing request (type, ID and payload hex), incoming
  response payload, and completed feature. It flushes before sending, preserving
  failed actions too.
- `result.json` records launch command, child PID, address announcement, doctor
  identity, pass/failure and process cleanup result.
- `server.log` retains stderr; it is normally empty because simulator logs are
  opt-in protocol Events.

Exercise the real user path and read back changed state, including from a new
connection where persistence across disconnects is the promise. Capture actions
and results, not just a final state. For physical changes, additionally observe
contacts and real network behavior on a Rig; these are not proved by simulator
status fields. The network journey uses explicitly fake credentials and opens
only loopback TCP; it does not join a network. This is not a dry-run mode.

## Cleanup

The helper closes each TCP stream, terminates only its own `Popen` child, waits
for exit, and escalates to killing that same child if needed. `result.json` must
report `cleanup.process_exited: true`. Its in-memory state disappears with the
process; the helper deletes no evidence directory. After every attempt, inspect
that field and confirm `wire.jsonl` and `result.json` still exist. An external
hard kill of the harness bypasses its cleanup; inspect the recorded child PID
and identity before manually stopping a stranded child, never kill by name.

## Helpers

`verify.py` is executable; invoke it through `python3` as shown above, or run
`.agents/skills/verify-itch/verify.py --help`. It launches, doctors, drives and
cleans up one owned instance in a single command. It accepts `doctor`, the four
feature IDs, or `all`, plus `--evidence` and optional `--binary`.

Keep the map current with `$maintain-verification-skill` in Codex or
`/maintain-verification-skill` in Claude Code.
