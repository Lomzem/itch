//! The itch Rig firmware for the ESP32-S3 Rev A board.
//!
//! Without the `firmware` feature only the chip-independent modules build,
//! so host `cargo test` covers them.

#![no_std]

pub mod board;
pub mod crash;
#[cfg(feature = "firmware")]
pub mod logger;
pub mod queue;
pub mod settings;
pub mod supervisor;
#[cfg(feature = "wifi")]
pub mod tcp;
#[cfg(feature = "firmware")]
pub mod usb;
