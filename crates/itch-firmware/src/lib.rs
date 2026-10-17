//! The itch Rig firmware for the ESP32-S3 Rev A board.
//!
//! Without the `firmware` feature only the chip-independent modules build,
//! so host `cargo test` covers them.

#![no_std]

pub mod board;
pub mod queue;
