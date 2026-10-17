# itch
sw-itch

Firmware, simulator and protocol spec for itch Rigs. Terms are defined in [GLOSSARY.md](GLOSSARY.md).

- `crates/itch-core`: the hardware-free `no_std` core: TAFP framing, the wire codec and Rig logic.
- `crates/itch-firmware`: the Rig firmware for the ESP32-S3 Rev A board, built on `itch-core`.
- `crates/itch-sim`: a simulated Rig serving the protocol over TCP.
- `docs/protocol/`: the protocol spec and its conformance vectors.

## Development

```sh
cargo test --workspace
cargo run -p itch-sim    # prints `itch-sim listening on tcp://127.0.0.1:<port>`
```

### Firmware

The firmware needs the `esp` Rust toolchain from [espup](https://github.com/esp-rs/espup) (`espup install`) and [espflash](https://github.com/esp-rs/espflash). Host `cargo test` covers the board support that needs no chip; the rest is checked on the bench.

```sh
cargo +esp build-firmware    # target/xtensa-esp32s3-none-elf/firmware/itch-firmware
cargo +esp flash-firmware    # flashes a Rig over its USB-C port
cargo +esp clippy-firmware
```

Run these from the workspace root. Flashing uses the partition table in `crates/itch-firmware/partitions.csv`, which keeps the settings partition (the Rig's label) after the app, so a reflash leaves the label alone. Don't erase the whole chip: that loses the label.

To hand out a single image instead, save a merged one without padding. Padding would fill the flash to its end and overwrite the settings:

```sh
cargo +esp build-firmware
espflash save-image --chip esp32s3 --flash-size 8mb \
  --partition-table crates/itch-firmware/partitions.csv --merge --skip-padding \
  target/xtensa-esp32s3-none-elf/firmware/itch-firmware itch-firmware.bin
espflash write-bin 0x0 itch-firmware.bin    # flashes it; the label survives
```

## Releases

Each [release](https://github.com/Lomzem/itch/releases) attaches:

- `itch-sim-linux-x86_64` and `itch-sim-windows-x86_64.exe`: prebuilt simulators, so you can write scripts without a Rig or a Rust toolchain;
- `vectors-v1.json`: the conformance vectors;
- `manifest.json`: the release's tag, commit, `protocol` and the SHA-256 of every other asset.

To cut a release, set `version` in `Cargo.toml` and push a matching `vX.Y.Z` tag. The release workflow runs the tests, builds the sims and publishes the release. Tags never move.
