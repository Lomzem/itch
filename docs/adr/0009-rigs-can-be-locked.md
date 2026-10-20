# Rigs can be Locked

Partly supersedes [ADR 0007](0007-channel-control-without-leases.md): Rigs gain an opt-in Lock; everything else in 0007 stands.

Wi-Fi ([ADR 0008](0008-wifi-on-the-rig-network.md)) makes every Rig reachable from anywhere on the Rig network, not just from the workstation plugged into it, so a teammate's script aimed at the wrong Rig can now ruin a test run. A client may Lock a Rig under a Holder name (e.g. `alice@ws3`). While a Rig is Locked, only its Holder may change it; anyone may still look at it and set their own log level. An unlocked Rig accepts changes from anyone, as before.

The Lock deliberately avoids what made the old Lease heavy (ADR 0004): no token, no expiry, no heartbeat. The Holder is a plain name, not a secret, because the Lock guards against accidents, not bad actors. The same name picks the Lock back up freely, so a script that crashed or lost Wi-Fi resumes without ceremony. A Lock ends only on release, Takeover or Rig reset, and none of these changes the Channels. A Holder that is taken over gets an error naming the new Holder on its next change, rather than silently fighting for the Channels. Accepted cost: a crashed script's Lock blocks everyone but its Holder until someone takes over.
