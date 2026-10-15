# TAFP framing for both transports

The Rig speaks the same framed byte stream over TCP and native USB, so we adopted TAFP's framing (`0x00 | COBS(length + payload + CRC-16/CCITT-FALSE) | 0x00`) rather than inventing our own or using text-based SCPI: COBS gives resynchronisation after noise or a mid-stream join, the CRC catches corruption, and it is already an in-house design. TAFP left byte order open; we pin the length field and CRC as **little-endian**. TAFP specifies nothing above framing, so message semantics, correlation, errors and versioning are ours.
