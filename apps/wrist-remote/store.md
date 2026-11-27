# Wrist Remote

Turn a CMF Watch Pro 2 into a small universal remote. Pick a device from the list, tap one of four large buttons, or turn the crown for volume. Media buttons talk to the phone directly; TV, PC and custom profiles talk to the companion over the watch's BLE app channel.

The watch only sends short button commands. All device-specific work happens in the companion (Android/PC). No personal data, tokens or keys are sent over the BLE channel.

## Requirements

- CMF Watch Pro 2 with the FMC patched firmware and the App Loader installed.
- For phone media: the phone linked to the watch (Nothing X). Volume +/- needs a recent loader.
- For TV/PC/custom profiles: the Wrist Remote companion on a PC or Android phone, connected to the watch's app channel and to the target device (LAN, local HTTP, IR hardware where present).
- The watch has no Wi-Fi and no IR transmitter; both are provided by the companion only.
