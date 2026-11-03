# Network configuration

A QA script sets Wi-Fi credentials and DHCP or a static IPv4 address, reads the
configuration, then clears it. The simulator models the reported network state.

## Sub-features

- `network-credentials`: fake credentials appear as configured after reconnect.
- `network-dhcp`: simulated connection reports the loopback bind address.
- `network-static`: status reports the supplied static address and gateway.
- `network-clear`: DHCP and no credentials return to the initial off state.

## How to get to it (user POV)

Send `SetWifi`, `SetAddress` and `GetStatus` over the simulator TCP connection.
Real Rig provisioning is also available over USB; that transport is not driven
by this harness.

## Driving it with verify.py

Preconditions: freshly launched healthy simulator, no Lock or storage fault,
no credentials, DHCP, bound to `127.0.0.1` rather than `0.0.0.0`.

Run `python3 .agents/skills/verify-itch/verify.py network --evidence .scratch/verification/network-002`.

- **Baseline.** `GetStatus` (`0x05`) reports no credentials, off and DHCP.
- **Configure.** `SetWifi` (`0x0D`) uses `ExampleRigNet` and the clearly fake
  `example-passphrase`; after reconnect, status reports configured, connected,
  that SSID, and `127.0.0.1` as current address.
- **Static.** `SetAddress` (`0x0E`) supplies `192.0.2.10/24`, gateway `192.0.2.1`;
  status reports static mode, those fields and current address `192.0.2.10`.
- **Clear.** Set DHCP and empty credentials; status again matches the full off
  baseline with zero current address and zero static configuration.

## Gotchas

The process always listens on its original loopback port. Setting a simulated
static address never moves its listener or touches the host network. No radio
association, DHCP exchange, routing, mDNS or real network access is proved.
Never substitute real credentials into this evidence-producing harness.
