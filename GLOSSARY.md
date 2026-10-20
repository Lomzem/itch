# itch

Test equipment that sets a product's DIP switches electronically, so QA engineers can script switch configurations instead of flipping them by hand.

## Hardware

**DUT**:
The device under test: the product whose DIP switches QA would otherwise flip by hand.
_Avoid_: target, product board

**Rig**:
One itch test board wired to a single DUT in place of its DIP switches.
_Avoid_: device, board, tester, fixture, HAWG

**Channel**:
One of the Rig's 16 isolated contacts, numbered CH1–CH16 as on the board; each stands in for one Switch on the DUT.
_Avoid_: relay, pin, bit, output

**Switch**:
One physical DIP switch position on the DUT (e.g. "SW3 position 4").
_Avoid_: dipswitch, channel

**Bank**:
One physical DIP switch package on the DUT (e.g. "SW3, 8 positions"), grouping several Switches.
_Avoid_: dipswitch, block

## Channel state

**On / Off**:
The two states of a Channel; on means the contact is closed, matching the "ON" marking of a DIP switch.
_Avoid_: closed/open, high/low, set/unset

**Clear**:
Turning every Channel off; also the state a Rig is in after power-up or reset.
_Avoid_: reset, release

**Mask write**:
A write that changes only the Channels selected by a mask and leaves every other Channel as it was.
_Avoid_: partial set, masked set

## Identity

**Label**:
A Rig's changeable, human-readable name, stored on the Rig and used to pick it from a script; defaults to `itch-` plus the last three bytes of its Hardware ID.
_Avoid_: name, hostname, alias

**Hardware ID**:
A Rig's fixed factory MAC address, also reported as its USB serial number (e.g. `60:55:F9:C0:27:18`).
_Avoid_: serial, MAC, UID

## Access

**Transport**:
How a client reaches a Rig: over USB, or over the network via the Rig's Wi-Fi. Every Transport carries the same protocol and allows the same control.
_Avoid_: interface, link, connection type

**Lock**:
A Holder's hold on a Rig that stops anyone else changing it, while still letting anyone look at it; it lasts until the Holder releases it, someone takes it over, or the Rig resets.
_Avoid_: lease, claim, reservation

**Holder**:
The name a Lock is held under (e.g. `alice@ws3`); a client using the same name can pick the Lock back up freely.
_Avoid_: owner, client, session

**Takeover**:
Deliberately taking a Rig's Lock from its current Holder.
_Avoid_: steal, force, override

## Semantics

**Profile**:
The per-DUT-model translation between meaningful settings and Channel states.
_Avoid_: translator, device map, semantic layer

**Field**:
One named setting within a Profile (e.g. `resolution`), occupying a fixed set of Channels.
_Avoid_: parameter, option

**Value**:
One named state of a Field (e.g. `1080p`), corresponding to one or more Encodings on that Field's Channels.
_Avoid_: preset, mode

**Encoding**:
One on/off pattern of a Field's Channels that selects a Value; a Value with several Encodings numbers them from 1, and Encoding 1 is its default.
_Avoid_: pattern, variant, representation

**Alias**:
Another name for an existing Value of the same Field.
_Avoid_: synonym
