# V-maps

## 1.1.0

- A menu with two big rounded buttons, Home and Work. It is the screen you get when there is no drive ("Waiting for
  Vela…" over it while the phone is out of reach, "Start a destination" once it is there), and during a drive you
  swipe right to open it ("Go to", with a hint) and swipe left to close it.
- Pressing Home or Work sends `n1H` / `n1W` to the phone, which ends the drive that is on, if any, and starts one to
  the place saved as Home or Work in Vela (needs the new phone patch). The watch shows "Getting the route…" until the
  route arrives, and "Open Vela on the phone" if nothing comes within 12 seconds.
- Holding the screen still switches the map between turning with you and held, as before; a slow swipe is not a hold.
- No map is drawn while the menu is up, and the map drawing and rotation code are as they were.
- The big font has an H and a W now.

## 1.0.0

First release as its own app, from Vela Nav 1.27.1: a live heading-up or north-up map with the route as blue dots,
the streets around you, a black dome at the bottom with the next turn, its distance and the time left, a light disc
as your position, and a smooth rotation. New: its own name, icon and listing; the title on the waiting screens;
rounded bends in the streets while the map moves.
