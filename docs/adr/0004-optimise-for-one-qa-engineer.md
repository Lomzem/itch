# Optimise for one QA engineer at a time

itch is an internal tool with no external users. In practice one QA engineer drives a Rig with one script at a time. When a design choice trades that engineer's experience against correctness in rare multi-client or failure edge cases, **we choose the engineer's experience** and accept the edge case.

Concretely, the Lease ([Lease semantics](../../.scratch/itch-v1/issues/16-lease-semantics.md)) does not fail a script whose Lease lapsed while nobody else was using the Rig: any request carrying a token claims a free Rig. Accepted cost: after a Rig reboot (which Clears every Channel) or after another client held and released the Lease in between, the script's next write succeeds without telling it that Channels outside its mask may have changed. `GetStatus` (uptime, reset reason) remains available for anyone who needs to check.

Contention is still handled where it is cheap: a live Lease held by someone else yields `LeaseHeld`/`LeaseLost`, and Takeover is explicit. Proposals that add ceremony, client-side bookkeeping or errors to guard against rare multi-user or failure cases should cite this decision and justify the cost to the single-user workflow.
