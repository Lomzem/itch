# Rig firmware supports USB only

Superseded by [ADR 0008](0008-wifi-on-the-rig-network.md), which brings Wi-Fi back on an IT-approved Rig network.

Rig firmware supports USB only; remove WiFi, radio credentials and the firmware TCP transport, including their opt-in build configuration. Staying within IT's network rules makes WiFi infeasible for a Rig, so keeping development-only support does not justify its implementation and maintenance cost. This supersedes ADR 0003; the protocol remains transport agnostic, and the simulator's TCP adapter is a separate concern.
