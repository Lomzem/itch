# WiFi is compiled out of production firmware

Superseded by [ADR 0006](0006-usb-only-rig-firmware.md), which removes WiFi from Rig firmware entirely; ADR 0006 is itself superseded by [ADR 0008](0008-wifi-on-the-rig-network.md), which brings Wi-Fi back in every build.

Rigs serve QA over USB only. The firmware's WiFi + TCP transport sits behind a Cargo feature, `wifi`, which is **off by default**: production/QA builds contain no radio code, no credentials and no TCP listener; development builds opt in with `--features wifi`. The handshake reports the firmware's enabled features so clients can flag a development build on a QA Rig.

We chose a compile-time switch over a runtime setting because QA workstations sit on an internal network that IT forbids bridging to the public WiFi, and a Rig plugged into a workstation over USB while on WiFi would itself straddle both networks. With WiFi absent from the binary, the answer to "can this device join WiFi?" is no, not "only if someone enables it". TCP remains part of the protocol (and the simulated Rig speaks it), so WiFi can return later without redesign.
