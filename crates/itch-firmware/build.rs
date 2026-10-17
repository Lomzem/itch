fn main() {
    // Only the firmware binary links against the ESP32-S3 memory map.
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("xtensa") {
        println!("cargo:rustc-link-arg-bins=-nostartfiles");
        // The memory map has a segment that is both writable and
        // executable, which ld would otherwise warn about on every link.
        println!("cargo:rustc-link-arg-bins=-Wl,--no-warn-rwx-segments");
        println!("cargo:rustc-link-arg-bins=-Tlinkall.x");
    }
}
