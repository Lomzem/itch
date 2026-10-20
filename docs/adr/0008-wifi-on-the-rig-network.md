# Rig firmware supports Wi-Fi on the Rig network

Supersedes [ADR 0006](0006-usb-only-rig-firmware.md).

IT has approved a dedicated Rig network that QA workstations can reach, which removes the network-straddling objection behind ADRs 0003 and 0006. Every firmware build therefore includes Wi-Fi, with no feature flag: a Rig joins the Rig network as a station and serves the same protocol over TCP that it serves over USB. Wi-Fi is a second Transport, not a mode. Both Transports are live at once, behave identically, and a script needs no USB connection to the host. USB is still used for power, flashing and recovery.

Credentials and the IPv4 address (DHCP or static) are set at runtime over any Transport and persisted on the Rig, never compiled in, because the repository and release firmware are public while the network is not. The Rig applies whatever it is told without conflict checks or fallback; a wrong setting is fixed over USB.

The TCP link is deliberately unencrypted and unauthenticated. The Rig network is the access boundary for an internal tool, the traffic (Channel states and logs) is not sensitive, and WPA2 already covers the radio link. Encryption alone would not stop an unwanted client from connecting, and adding a shared key would mean managing a second secret on every Rig and script. Accidental interference between teammates is handled by the Lock ([ADR 0009](0009-rigs-can-be-locked.md)), not by the Transport. Revisit this if people outside QA can reach the Rig network.
