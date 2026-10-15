# Profiles are client-side only

The Rig knows only Channels and bit states; it has no notion of Profiles, Fields or Values. Translating "resolution = 1080p" into Channel states happens entirely in the client library, so supporting a new DUT model never requires a firmware change or a reflash. A firmware-side semantic layer (as in the aja-connect pitch) was rejected for that reason; the cost is that each client language must implement Profile translation itself.
