# itch

Test equipment that sets a product's DIP switches electronically, so QA engineers can script switch configurations instead of flipping them by hand.

## Hardware

**DUT**:
The device under test: the product whose DIP switches QA would otherwise flip by hand.
_Avoid_: target, product board

**Rig**:
One itch test board (ESP32 plus GPIO expander) wired to a single DUT's switch lines.
_Avoid_: device, board, tester, fixture

## Switch state

**Switch**:
A single on/off position the Rig controls; one bit of state.
_Avoid_: dipswitch, pin, bit, channel

**Bank**:
One physical DIP switch package on the DUT (e.g. "SW3, 8 positions"), grouping several Switches.
_Avoid_: dipswitch, block

**Mask write**:
A write that changes only the Switches selected by a mask and leaves every other Switch as it was.
_Avoid_: partial set, masked set

## Semantics

**Profile**:
The per-DUT-model translation between meaningful settings and Switch states.
_Avoid_: translator, device map, semantic layer

**Field**:
One named setting within a Profile (e.g. `resolution`), occupying a fixed set of Switches.
_Avoid_: parameter, option

**Value**:
One named state of a Field (e.g. `1080p`), corresponding to a specific bit pattern on that Field's Switches.
_Avoid_: preset, mode
