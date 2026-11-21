Shows the drive that **Vela** on your phone is navigating, on a live map that fills the whole screen. The route is a
line of blue dots, the streets around you are thin lines (main roads a little brighter), and your position is a
light disc with an arrow. Turn the crown to zoom.

At the bottom, in a black dome, are the next turn, the distance to it and the time left. Hold the screen to switch
between two ways of looking: the arrow turns to where you are heading and the map stays (north up), or the map turns
with you and the arrow stays up. When you arrive, the screen says so. The screen stays on while a route is active.

What goes over Bluetooth is the turn, the road name, and the route and your position in metres from the start of
the route: never latitude and longitude, and never the destination's name. The watch's app channel can be read by
any device in Bluetooth range, so a nearby listener could still see the shape of the drive and the road names.

## Requirements

- The [patched firmware](/firmware) (`fw-wp2.bin` from 2026-11-13 or later) and the [App Loader](/firmware)
  (2026-11-13 or later).
- Vela for Android with the watch link, [APK on GitHub](https://github.com/arunk140/esp32-vela/releases/latest)
  ([source](https://github.com/arunk140/esp32-vela)), on the phone that is paired with the watch in Nothing X. In
  Vela: Settings > Nav device > CMF watch test, pick the watch, turn on **Show navigation on the watch**.

Open it from the **Apps** row at the end of the watch's app list before you start driving.
