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

## Releases

Each [release](https://github.com/Lomzem/itch/releases) attaches:

- `itch-sim-linux-x86_64` and `itch-sim-windows-x86_64.exe`: prebuilt simulators, so you can write scripts without a Rig or a Rust toolchain;
- `vectors-v1.json`: the conformance vectors;
- `manifest.json`: the release's tag, commit, `protocol` and the SHA-256 of every other asset.

To cut a release, set `version` in `Cargo.toml` and push a matching `vX.Y.Z` tag. The release workflow runs the tests, builds the sims and publishes the release. Tags never move.
