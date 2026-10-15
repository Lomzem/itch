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

## Control

**Lease**:
Exclusive permission for one client to change a Rig's Channels, which lapses unless the holder keeps it alive.
_Avoid_: lock, session, ownership

**Holder**:
The client currently owning a Rig's Lease, identified by a human-readable description (user, host, script).
_Avoid_: owner, master

**Takeover**:
Forcibly moving a Lease from its Holder to another client.
_Avoid_: steal, break lock

## Identity

**Label**:
A Rig's changeable, human-readable name, stored on the Rig and used to pick it from a script; defaults to `itch-` plus the last three bytes of its Hardware ID.
_Avoid_: name, hostname, alias

**Hardware ID**:
A Rig's fixed factory MAC address, also reported as its USB serial number (e.g. `60:55:F9:C0:27:18`).
_Avoid_: serial, MAC, UID

## Semantics

**Profile**:
The per-DUT-model translation between meaningful settings and Channel states.
_Avoid_: translator, device map, semantic layer

**Field**:
One named setting within a Profile (e.g. `resolution`), occupying a fixed set of Channels.
_Avoid_: parameter, option

**Value**:
One named state of a Field (e.g. `1080p`), corresponding to a specific on/off pattern on that Field's Channels.
_Avoid_: preset, mode
