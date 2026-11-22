Show the current song, artist, and notification artwork on the watch, then control playback and the phone's music-stream volume from large full-screen controls.

The standalone Android **Music Companion** app sends the current title, artist, and a low-resolution copy of the matching notification artwork to the watch. Album names and notification text stay on the phone. The watch keeps the transferred values in memory only and hides stale artwork while a replacement transfers. The watch's Bluetooth channel is not private: a nearby client that knows the watch pairing key can observe the transferred song information and artwork and can inject messages into the watch-facing channel.

## Requirements

- The [patched firmware](/firmware) with the channel key and the [App Loader](/firmware).
- The standalone [Android Music Companion](https://github.com/boxmol71-gif/music-companion) app. [Download the v0.2.0 companion APK](https://github.com/boxmol71-gif/music-companion/releases/download/v0.2.0/MusicCompanion-0.2.0-debug.apk), then enable its watch bridge and grant notification-listener access. Music Companion 0.1.1 remains a compatibility fallback, but it cannot show track text or artwork.
