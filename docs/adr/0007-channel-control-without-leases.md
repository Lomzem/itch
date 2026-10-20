# Channel control needs no Lease

Partly superseded by [ADR 0009](0009-rigs-can-be-locked.md), which adds an opt-in Lock. Retained Channel state and the absence of tokens, expiry and heartbeats still stand.

The Rig accepts Channel changes and Label changes without a Lease, Holder, Takeover or client heartbeat: one QA engineer normally runs one script per Rig, and accidental competing writes are an accepted cost of a smaller protocol. A powered Rig retains its commanded Channel state after a script exits, crashes or stops communicating; Clear and Rig reset remain explicit, separate causes of state changes. This supersedes ADR 0004's concrete Lease policy while preserving its single-engineer priority; client interface design is a separate concern.
