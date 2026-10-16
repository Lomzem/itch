# itch
sw-itch

Firmware, simulator and protocol spec for itch Rigs. Terms are defined in [GLOSSARY.md](GLOSSARY.md).

- `crates/itch-core`: the hardware-free `no_std` core: TAFP framing, the wire codec and Rig logic.
- `crates/itch-sim`: a simulated Rig serving the protocol over TCP.
- `docs/protocol/`: the protocol spec and its conformance vectors.

## Development

```sh
cargo test --workspace
cargo run -p itch-sim    # prints `itch-sim listening on tcp://127.0.0.1:<port>`
```
