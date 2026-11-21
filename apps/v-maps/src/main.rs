#![cfg_attr(any(target_os = "none", wp2_sim), no_main, cfg_attr(target_os = "none", no_std))]

//! V-maps: shows the drive that the Vela maps app on the phone is navigating. Protocol: `proto.rs`.

// the host build (tests) has no device loop, so some of the protocol is unused there
mod font;
mod gesture;
#[rustfmt::skip]
mod fonts;
mod proto;
mod sign;
mod view;

#[cfg(not(any(target_os = "none", wp2_sim)))]
fn main() {}

#[cfg(any(target_os = "none", wp2_sim))]
mod device {
    use crate::gesture::{Gesture, classify};
    use crate::proto::{ASK, Effect, HOME, Model, WORK};
    use crate::view::{self, Choice, Ctx, ZOOMS};
    use cmf_wp2_app::{Api, BUTTON, MSG_MAX, PAUSE, RESUME, TOUCH_DOWN, TOUCH_UP};

    cmf_wp2_app::entry!(main);
    cmf_wp2_app::app_icon!();

    /// Ask for a snapshot again this often while none comes.
    const ASK_MS: u32 = 3000;
    /// The phone sends the drive every second; after this long without one it is out of reach.
    const STALE_MS: u32 = 6000;
    /// Minimum gap between frames. The renderer now reuses one map transform per frame, making a 20 ms target
    /// more realistic; actual frame rate remains bounded by the drawing time and loader.
    const GAP_MS: u32 = 20;
    /// Position glides over the GPS reporting interval. Kept independent from heading updates.
    const POSITION_ANIM_MS: u32 = 1000;
    /// Heading catches up between phone reports instead of sharing/restarting the position animation.
    const HEADING_ANIM_MS: u32 = 240;
    /// After the last frame of zooming or turning (drawn with cheaper hard-edged streets), this long later the
    /// screen is drawn once more in full quality.
    const SETTLE_MS: u32 = 300;
    /// After Home or Work was pressed, this long for the phone's route to arrive before the watch gives up waiting.
    const GOING_MS: u32 = 12_000;
    /// A hint on the menu ("Open Vela on the phone") stays this long.
    const NOTE_MS: u32 = 5000;
    /// Between full frames the car moves on a cheap band-only frame; the text and the arc are refreshed this
    /// often at most (they change in tens of metres).
    const FULL_MS: u32 = 1000;
    /// The phone already smooths its compass. Keep only a small watch-side deadband (about 0.35°) for jitter.
    const HEADING_DEAD: i32 = 64;
    /// The arrow is repainted only when it has turned this much (1.5 degrees).
    const REPAINT: i32 = 273;
    /// The most the heading on screen turns between two frames (22.5 degrees): after a slow frame the glide
    /// catches up over several frames instead of jumping.
    const MAX_TURN: i32 = 4096;

    /// What the text shows, to the precision they show it.
    fn text_key(s: &crate::proto::State) -> [u32; 2] {
        [s.dist_m as u32 / 10, s.remaining_s as u32 / 60]
    }

    // 4 KiB of route: too big for the 8 KiB stack
    static mut MODEL: Model = Model::new();

    // true on RESUME, false on BUTTON
    fn paused(api: &Api) -> bool {
        loop {
            match api.event(u32::MAX).map(|e| e.kind) {
                Some(RESUME) => return true,
                Some(BUTTON) => return false,
                _ => {}
            }
        }
    }

    fn main(api: &'static Api) -> i32 {
        // SAFETY: the app has one thread and this is the only reference.
        let m = unsafe { &mut *core::ptr::addr_of_mut!(MODEL) };
        let zoom = 0; // one zoom only: the closest (no zooming)
        let mut want;
        let mut asked: Option<u32> = None;
        // asks so far without an answer: the gap doubles (3 s, 6 s ... 60 s) so a phone with no Vela hears almost nothing
        let mut tries = 0u32;
        let mut heard: Option<u32> = None;
        let mut dirty = true;
        let mut drawn_end = 0u32;
        // Position and heading use separate animation clocks. A frequent GPS coordinate update must never
        // restart an unfinished heading transition.
        let mut shown_xy = (0i32, 0i32);
        let (mut pos_from, mut pos_target, mut pos_last_raw) = (shown_xy, shown_xy, shown_xy);
        let mut pos_t0 = 0u32;
        let mut shown_heading = 0u16;
        let (mut heading_from, mut heading_target, mut raw_heading_last) = (shown_heading, shown_heading, shown_heading);
        let mut heading_t0 = 0u32;
        let mut fresh = true;
        let mut settle_at: Option<u32> = None;
        let mut live = false;
        // true: the arrow turns to where the phone looks and the map stays; false: the map turns, the arrow is up
        let mut look = true;
        // the way the map is turned while it is held still (north up at first); holding the screen freezes it where it is
        let mut map_heading = 0u16;
        // a touch: when and where the finger came down
        let mut touch0: Option<(u32, i32, i32)> = None;
        // the menu: swiped open during a drive (it is always there when there is no drive), the button under the finger,
        // the button that was pressed while the phone's route is awaited (with when, and whether the old drive has
        // ended), and a hint that Vela did not answer
        let mut menu_open = false;
        let mut menu_shown = false;
        let mut menu_dirty = false;
        let mut pressed: Option<Choice> = None;
        let mut going: Option<(Choice, u32, bool)> = None;
        let mut note: Option<u32> = None;
        let mut last_drawn_heading = 0u16;
        let mut last_map_heading = 0u16;
        let mut last_full = 0u32;
        let mut stale = false; // the text or arc on screen is out of date
        let mut drawn_at_xy = (i32::MIN, i32::MIN);
        let mut screen_on = false;
        loop {
            let now = api.now();
            want = !m.synced;
            if want && asked.is_none_or(|t| now.wrapping_sub(t) >= (ASK_MS << tries.min(4)).min(60_000)) {
                api.send(&ASK); // -107 while Vela is not listening: asked again in 3 s
                asked = Some(now);
                tries += 1;
            }
            let is_live = heard.is_some_and(|t| now.wrapping_sub(t) < STALE_MS);
            if is_live != live {
                live = is_live;
                dirty = true;
                if !live {
                    (m.synced, asked) = (false, None);
                    m.reset_seq();
                }
            }
            let on = live && m.state.active();
            if on != screen_on {
                screen_on = on;
                api.keep_screen_on(on);
            }
            // The phone reports position and heading together, but they must not share an animation clock.
            // Heading updates arrive more often and now animate independently between reports.
            let raw = (m.state.x, m.state.y, m.state.course);
            if !(live && m.state.active()) {
                shown_xy = (raw.0, raw.1);
                (pos_from, pos_target, pos_last_raw) = (shown_xy, shown_xy, shown_xy);
                pos_t0 = now;
                shown_heading = raw.2;
                (heading_from, heading_target, raw_heading_last) = (raw.2, raw.2, raw.2);
                heading_t0 = now;
                fresh = true;
            } else if fresh {
                shown_xy = (raw.0, raw.1);
                (pos_from, pos_target, pos_last_raw) = (shown_xy, shown_xy, shown_xy);
                pos_t0 = now;
                shown_heading = raw.2;
                (heading_from, heading_target, raw_heading_last) = (raw.2, raw.2, raw.2);
                heading_t0 = now;
                fresh = false;
            } else {
                if (raw.0, raw.1) != pos_last_raw {
                    // Retarget from the current interpolated position, not from a stale frame or the old GPS fix.
                    let current = view::interpolate_position(pos_from, pos_target, now.wrapping_sub(pos_t0), POSITION_ANIM_MS);
                    let far = (raw.0 as i64 - current.0 as i64).abs() > 400
                        || (raw.1 as i64 - current.1 as i64).abs() > 400;
                    pos_from = if far { (raw.0, raw.1) } else { current };
                    pos_target = (raw.0, raw.1);
                    pos_last_raw = (raw.0, raw.1);
                    pos_t0 = now;
                }
                if raw.2 != raw_heading_last {
                    raw_heading_last = raw.2;
                    let d = heading_target.wrapping_sub(raw.2) as i16 as i32;
                    if d.abs() > HEADING_DEAD {
                        // Preserve the currently displayed heading when a newer compass sample arrives.
                        // This prevents the new sample from snapping or restarting the position animation.
                        // Start from the value actually shown on the previous frame. Using the ideal
                        // interpolation endpoint here would jump ahead if MAX_TURN had rate-limited it.
                        heading_from = shown_heading;
                        heading_target = raw.2;
                        heading_t0 = now;
                    }
                }
            }
            shown_xy = view::interpolate_position(pos_from, pos_target, now.wrapping_sub(pos_t0), POSITION_ANIM_MS);
            let before_heading = shown_heading;
            shown_heading = view::interpolate_heading(heading_from, heading_target, now.wrapping_sub(heading_t0), HEADING_ANIM_MS);
            shown_heading = view::limit_turn(before_heading, shown_heading, MAX_TURN);
            let moving = shown_xy != pos_target || shown_heading != heading_target;
            let gap = GAP_MS;
            // the text in the dome is out of date: repaint just the dome, once a second at most
            let dome = stale && !dirty && last_full != 0 && now.wrapping_sub(last_full) >= FULL_MS;
            if settle_at.is_some_and(|t| now.wrapping_sub(t) as i32 >= 0) && !moving {
                (dirty, settle_at) = (true, None);
            }
            // the menu: a still screen that takes the place of the map; nothing of the map is drawn meanwhile
            if let Some((c, t0, mut left)) = going {
                left |= !on;
                going = if on && left {
                    menu_open = false; // the new drive is here
                    None
                } else if now.wrapping_sub(t0) >= GOING_MS {
                    (menu_open, note, menu_dirty) = (on, Some(now), true); // the phone did not answer
                    None
                } else {
                    Some((c, t0, left))
                };
            }
            if note.is_some_and(|t| now.wrapping_sub(t) >= NOTE_MS) {
                (note, menu_dirty) = (None, true);
            }
            let menu_screen = going.is_some() || menu_open || !on;
            if menu_screen != menu_shown {
                menu_shown = menu_screen;
                // back to the map: it is drawn in full first (nothing of it is on the screen)
                if menu_screen { menu_dirty = true } else { dirty = true }
                (settle_at, last_full) = (None, 0);
            }
            let heading_moved = (shown_heading.wrapping_sub(last_drawn_heading) as i16 as i32).abs() >= REPAINT;
            // with the map fixed, a change of heading alone repaints only the patch round the arrow
            // has the map moved a whole pixel (or turned a quarter degree) since it was drawn? Less is the same picture
            let mpp = ZOOMS[zoom.min(ZOOMS.len() - 1)];
            let shifted = (shown_xy.0 - drawn_at_xy.0).abs().max((shown_xy.1 - drawn_at_xy.1).abs()) >= mpp
                || (!look && (shown_heading.wrapping_sub(last_map_heading) as i16 as i32).abs() >= 36);
            let arrow_only = look && !dirty && !dome && !shifted && heading_moved;
            // a moving car repaints the screen in the cheap style, when it would look different
            let light = !dirty && !dome && !arrow_only && shifted && last_full != 0;
            if menu_screen {
                if (dirty || menu_dirty) && now.wrapping_sub(drawn_end) >= gap {
                    let mut screen = api;
                    if let Some((c, _, _)) = going {
                        view::going(&mut screen, c);
                    } else {
                        let title = if note.is_some() {
                            "Open Vela on the phone"
                        } else if !live {
                            "Waiting for Vela…"
                        } else if !on {
                            "Start a destination"
                        } else {
                            "Go to"
                        };
                        view::menu(&mut screen, title, if menu_open && on { "Swipe left to go back" } else { "" }, pressed);
                    }
                    api.show();
                    (dirty, menu_dirty, drawn_end, last_full) = (false, false, api.now(), 0);
                    continue;
                }
            } else if (dirty || dome || arrow_only || light) && now.wrapping_sub(drawn_end) >= gap {
                let mut screen = api;
                (m.state.x, m.state.y, m.state.course) = (shown_xy.0, shown_xy.1, shown_heading);
                if look {
                    m.state.course = map_heading;
                }
                let fast = !arrow_only && !dome && moving;
                let ctx = Ctx { live, zoom, fast, look: look.then_some(shown_heading.wrapping_sub(map_heading)), arrow_only, light, dome };
                view::draw(&mut screen, m, &ctx);
                (m.state.x, m.state.y, m.state.course) = raw;
                api.show();
                let end = api.now();
                (dirty, drawn_end) = (false, end);
                if dome {
                    // the dome is fresh; the map and the arrow were not touched
                    (last_full, stale) = (end.max(1), false);
                    continue;
                }
                last_drawn_heading = shown_heading;
                if !arrow_only {
                    (drawn_at_xy, last_map_heading) = (shown_xy, shown_heading);
                }
                if !arrow_only && !light {
                    (last_full, stale) = (end.max(1), false);
                }
                settle_at = (fast || light).then(|| end.wrapping_add(SETTLE_MS));
                continue;
            }
            let mut wait = if dirty || menu_dirty || moving || stale { gap.saturating_sub(now.wrapping_sub(drawn_end)).max(1) } else { 1000 };
            if let Some(t) = settle_at {
                wait = wait.min((t.wrapping_sub(now) as i32).max(1) as u32);
            }
            let mut ev = api.event(wait);
            // everything that is already waiting is handled before the next frame: several crown clicks are one
            // zoom change, not one frame each
            while let Some(e) = ev {
                match e {
                    e if e.kind == BUTTON => return 0,
                    e if e.kind == TOUCH_DOWN => {
                        touch0 = Some((api.now(), e.x as i32, e.y as i32));
                        if menu_screen && going.is_none() {
                            pressed = view::button_at(e.x as i32, e.y as i32);
                            menu_dirty = pressed.is_some();
                        }
                    }
                    e if e.kind == TOUCH_UP => {
                        let Some((t, x0, y0)) = touch0.take() else { continue };
                        let down = pressed.take();
                        menu_dirty |= down.is_some(); // the button lets go
                        match classify(e.x as i32 - x0, e.y as i32 - y0, api.now().wrapping_sub(t)) {
                            Gesture::SwipeRight if !menu_screen => (menu_open, menu_dirty) = (true, true),
                            Gesture::SwipeLeft if menu_open && going.is_none() => (menu_open, dirty) = (false, true),
                            Gesture::Tap if menu_screen && going.is_none() => {
                                if let Some(c) = down.filter(|&c| view::button_at(e.x as i32, e.y as i32) == Some(c)) {
                                    if api.send(if c == Choice::Home { &HOME } else { &WORK }) < 0 {
                                        (note, menu_dirty) = (Some(api.now()), true); // Vela is not listening
                                    } else {
                                        (going, menu_open, menu_dirty) = (Some((c, api.now(), !on)), false, true);
                                    }
                                }
                            }
                            Gesture::Hold if !menu_screen => {
                                look = !look;
                                if look {
                                    map_heading = shown_heading; // hold the map as it is turned now
                                }
                                dirty = true;
                            }
                            _ => {}
                        }
                    }
                    e if e.kind == PAUSE => {
                        (screen_on, touch0, pressed) = (false, None, None);
                        if !paused(api) {
                            return 0;
                        }
                        // messages were refused meanwhile
                        (m.synced, asked, dirty) = (false, None, true);
                        m.reset_seq();
                    }
                    _ => {}
                }
                ev = api.event(0);
            }
            let mut buf = [0u8; MSG_MAX];
            while let Some(msg) = api.recv(&mut buf) {
                let before = m.state;
                match m.apply(msg) {
                    Effect::Ignored => continue,
                    // a drive state: the car's own movement is glided in the loop above; a new manoeuvre, route
                    // state or the like is drawn at once, a changed distance or time within a second
                    Effect::Changed if msg.get(2) == Some(&b'S') => {
                        let (a, b) = (before, m.state);
                        if (a.flags, a.maneuver) != (b.flags, b.maneuver) {
                            dirty = true;
                        } else if text_key(&a) != text_key(&b) {
                            stale = true;
                        }
                    }
                    Effect::Changed => dirty = true,
                    Effect::Resync => {
                        dirty = true;
                        if !want {
                            asked = None; // ask right away
                        }
                    }
                }
                heard = Some(api.now());
                tries = 0;
                want = !m.synced;
            }
        }
    }
}
