#![cfg_attr(any(target_os = "none", wp2_sim), no_main, cfg_attr(target_os = "none", no_std))]

//! Watch Remote: a remote for a PC or a Google TV, paired to your phone (the Watch Menu app), which acts as a
//! Bluetooth keyboard + media remote. Two pages, flipped by turning the crown (the SDK gives apps no swipe):
//!   page 1: four arrow wedges and a centre ring (OK)
//!   page 2: Back, Home, Prev, Play/Pause, Next, Mute and a Volume icon (tap it, then the crown changes the volume;
//!           it switches itself off after a moment without input)
//! Crown press / right swipe leaves the app. Every button just sends its id to the phone, which decides what it
//! does (so every button can be re-programmed on the phone).
//!
//! Watch -> phone messages (all start with `c1`):
//!   `c1h` ver            hello: the app is open
//!   `c1f`                heartbeat while open (the phone treats 9 s of silence as closed)
//!   `c1z`                the app is closing
//!   `c1p` button         page-1 key down / still held (repeated every 250 ms while the finger stays down)
//!   `c1r` button         page-1 key released (the phone also lets go by itself after ~1 s of silence)
//!                        ids: 1 up 2 down 3 left 4 right 5 centre
//!   `c1k` button 0       a page-2 tap: 10 back 11 home 12 play/pause 13 prev 14 next 15 mute
//!   `c1v` n              n crown volume steps (signed byte, + = up), batched per 60 ms

/// Screen geometry and hit testing, pure integer maths (host-tested).
pub mod logic {
    pub const SIZE: i32 = 466;
    pub const C: i32 = SIZE / 2;
    /// Radius of the grey disc on page 1.
    pub const R: i32 = 228;
    /// Radius of the centre ring.
    pub const RC: i32 = 78;
    /// Page dots at the bottom: centre y and spacing.
    pub const DOT_Y: i32 = SIZE - 28;
    pub const DOT_GAP: i32 = 28;

    pub const UP: u8 = 1;
    pub const DOWN: u8 = 2;
    pub const LEFT: u8 = 3;
    pub const RIGHT: u8 = 4;
    pub const CENTRE: u8 = 5;
    pub const BACK: u8 = 10;
    pub const HOME: u8 = 11;
    pub const PLAY: u8 = 12;
    pub const PREV: u8 = 13;
    pub const NEXT: u8 = 14;
    pub const MUTE: u8 = 15;
    /// The volume icon: not sent to the phone, it starts the crown volume mode.
    pub const VOLUME: u8 = 16;
    pub const VOL_UP: u8 = 20;
    pub const VOL_DOWN: u8 = 21;
    /// A tap on the page dots flips the page.
    pub const DOTS: u8 = 99;

    /// Page 2 buttons: (id, centre x, centre y, radius).
    pub const BUTTONS: [(u8, i32, i32, i32); 7] = [
        (BACK, C - 90, C - 125, 52),
        (HOME, C + 90, C - 125, 52),
        (PREV, C - 135, C, 52),
        (PLAY, C, C, 62),
        (NEXT, C + 135, C, 52),
        (MUTE, C - 90, C + 125, 52),
        (VOLUME, C + 90, C + 125, 52),
    ];

    pub fn isqrt(v: i32) -> i32 {
        if v <= 0 {
            return 0;
        }
        let mut r = 0;
        while (r + 1) * (r + 1) <= v {
            r += 1;
        }
        r
    }

    fn on_dots(x: i32, y: i32) -> bool {
        y >= DOT_Y - 18 && y <= DOT_Y + 18 && (x - C).abs() <= DOT_GAP + 14
    }

    /// Page 1: which control a touch at (x, y) hits. Wedges are found by angle: the diagonals split the disc.
    pub fn hit_page1(x: i32, y: i32) -> Option<u8> {
        if on_dots(x, y) {
            return Some(DOTS);
        }
        let (dx, dy) = (x - C, y - C);
        let d2 = dx * dx + dy * dy;
        if d2 <= RC * RC {
            return Some(CENTRE);
        }
        if d2 > R * R {
            return None;
        }
        Some(if dx.abs() > dy.abs() {
            if dx > 0 { RIGHT } else { LEFT }
        } else if dy > 0 {
            DOWN
        } else {
            UP
        })
    }

    /// Page 2: which round button a touch hits.
    pub fn hit_page2(x: i32, y: i32) -> Option<u8> {
        if on_dots(x, y) {
            return Some(DOTS);
        }
        BUTTONS.iter().find(|&&(_, cx, cy, r)| (x - cx) * (x - cx) + (y - cy) * (y - cy) <= r * r).map(|b| b.0)
    }

    /// Row span of a wedge at `dy` rows from the centre, `gap` px kept free for the black separators.
    /// Returns (from, to) x offsets from the centre, inclusive. `dir`: 0 up, 1 down, 2 left, 3 right.
    pub fn wedge_span(dir: u8, dy: i32, gap: i32) -> Option<(i32, i32)> {
        let hw = isqrt(R * R - dy * dy);
        if hw <= 0 {
            return None;
        }
        let a = dy.abs();
        match dir {
            0 | 1 => {
                if (dir == 0) != (dy < 0) {
                    return None;
                }
                let w = (a - gap).min(hw);
                if w <= 0 { None } else { Some((-w, w)) }
            }
            _ => {
                let from = a + gap;
                if from >= hw {
                    return None;
                }
                if dir == 3 { Some((from, hw)) } else { Some((-hw, -from)) }
            }
        }
    }

    /// Page-2 icons as vector shapes, drawn with 4x4 supersampling so the edges are smooth.
    /// Coordinates are in quarter pixels relative to the button centre; every icon is designed in a 48 px box
    /// and offset so its *visual* centre sits on the button centre (no stretching, no off-centre glyphs).
    pub mod icons {
        use super::*;
        const Q: i32 = 4;

        fn tri(x: i32, y: i32, p: [(i32, i32); 3]) -> bool {
            let e = |a: (i32, i32), b: (i32, i32)| (b.0 - a.0) * (y - a.1 * Q) - (b.1 - a.1) * (x - a.0 * Q);
            let (d0, d1, d2) = (e(p[0], p[1]), e(p[1], p[2]), e(p[2], p[0]));
            (d0 >= 0 && d1 >= 0 && d2 >= 0) || (d0 <= 0 && d1 <= 0 && d2 <= 0)
        }
        fn rect(x: i32, y: i32, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
            x >= x0 * Q && x < x1 * Q && y >= y0 * Q && y < y1 * Q
        }
        /// A thick line with round caps: distance from (x, y) to segment a-b at most r.
        fn cap(x: i32, y: i32, a: (i32, i32), b: (i32, i32), r: i32) -> bool {
            let (ax, ay, bx, by) = (a.0 * Q, a.1 * Q, b.0 * Q, b.1 * Q);
            let (vx, vy, wx, wy) = (bx - ax, by - ay, x - ax, y - ay);
            let l2 = vx * vx + vy * vy;
            let t = if l2 == 0 { 0 } else { ((wx * vx + wy * vy) * 1024 / l2).clamp(0, 1024) };
            let (px, py) = (ax + vx * t / 1024, ay + vy * t / 1024);
            let (dx, dy) = (x - px, y - py);
            dx * dx + dy * dy <= r * r * Q * Q
        }
        /// A sound-wave arc: ring around (cx, 0), radii r0..r1, opening to the right within about +-50 degrees.
        fn wave(x: i32, y: i32, cx: i32, r0: i32, r1: i32) -> bool {
            let dx = x - cx * Q;
            let d2 = dx * dx + y * y;
            dx > 0 && y.abs() * 5 <= dx * 6 && d2 >= r0 * r0 * Q * Q && d2 <= r1 * r1 * Q * Q
        }
        fn speaker(x: i32, y: i32, ox: i32) -> bool {
            rect(x, y, ox - 20, -7, ox - 9, 8) || tri(x, y, [(ox - 10, -7), (ox + 2, -18), (ox + 2, 18)])
                || tri(x, y, [(ox - 10, -7), (ox + 2, 18), (ox - 10, 8)])
        }

        /// Is the point (x, y), in quarter pixels from the button centre, inside icon `id`?
        pub fn inside(id: u8, x: i32, y: i32) -> bool {
            match id {
                BACK => {
                    // arrow pointing left: shaft and two head strokes
                    let x = x + 2 * Q;
                    cap(x, y, (-16, 0), (20, 0), 4) || cap(x, y, (-16, 0), (-2, -14), 4) || cap(x, y, (-16, 0), (-2, 14), 4)
                }
                HOME => {
                    // roof + walls with a door, bottom-heavy box centred vertically (-20..20)
                    let y = y - 2 * Q;
                    let roof = tri(x, y, [(0, -22), (23, -1), (-23, -1)]);
                    let walls = rect(x, y, -15, -2, 15, 18) && !rect(x, y, -5, 6, 5, 18);
                    roof || walls
                }
                PLAY => {
                    // play triangle + pause bars; whole group is 44 px wide, centred
                    tri(x, y, [(-22, -15), (-22, 15), (2, 0)]) || rect(x, y, 8, -15, 14, 15) || rect(x, y, 17, -15, 23, 15)
                }
                PREV => rect(x, y, -16, -14, -10, 14) || tri(x, y, [(16, -14), (16, 14), (-9, 0)]),
                NEXT => rect(x, y, 10, -14, 16, 14) || tri(x, y, [(-16, -14), (-16, 14), (9, 0)]),
                MUTE => speaker(x, y, -3) || cap(x, y, (8, -8), (21, 8), 3) || cap(x, y, (8, 8), (21, -8), 3),
                VOLUME => {
                    let x = x - 2 * Q;
                    speaker(x, y, -3) || wave(x, y, -1, 8, 12) || wave(x, y, -1, 16, 20)
                }
                _ => false,
            }
        }

        /// Coverage 0..=16 of the pixel whose top-left is (px, py) (whole pixels from the centre).
        pub fn coverage(id: u8, px: i32, py: i32) -> i32 {
            let (x0, y0) = (px * Q, py * Q);
            let corners = [(0, 0), (Q - 1, 0), (0, Q - 1), (Q - 1, Q - 1)];
            let n = corners.iter().filter(|&&(a, b)| inside(id, x0 + a, y0 + b)).count();
            if n == 0 && !inside(id, x0 + 2, y0 + 2) {
                return 0;
            }
            if n == 4 && inside(id, x0 + 2, y0 + 2) {
                return 16;
            }
            let mut c = 0;
            for j in 0..Q {
                for i in 0..Q {
                    if inside(id, x0 + i, y0 + j) {
                        c += 1;
                    }
                }
            }
            c
        }

        /// Half the side of the square every icon fits in.
        pub const HALF: i32 = 28;
    }

    /// Row spans of a triangle pointing `dir` (0 up, 1 down, 2 left, 3 right), centre (cx, cy), half-size s:
    /// calls `f(y, x0, x1)` per row (x1 exclusive).
    pub fn triangle(dir: u8, cx: i32, cy: i32, s: i32, mut f: impl FnMut(i32, i32, i32)) {
        match dir {
            0 | 1 => {
                for i in 0..=s * 2 {
                    let w = i / 2; // i = 0 is the tip
                    let y = if dir == 0 { cy - s + i } else { cy + s - i };
                    f(y, cx - w, cx + w + 1);
                }
            }
            _ => {
                for r in -s..=s {
                    let w = s * 2 - r.abs() * 2;
                    let (x0, x1) = if dir == 3 { (cx - s, cx - s + w + 1) } else { (cx + s - w, cx + s + 1) };
                    f(cy + r, x0, x1);
                }
            }
        }
    }
}

#[cfg(not(any(target_os = "none", wp2_sim)))]
fn main() {}

#[cfg(any(target_os = "none", wp2_sim))]
mod device {
    use super::logic::*;
    use cmf_wp2_app::{Api, BUTTON, CROWN, PAUSE, RESUME, TOUCH_DOWN, TOUCH_UP};
    use cmf_wp2_ui::color::BLACK;
    use cmf_wp2_ui::draw::{disc, ring};
    use cmf_wp2_ui::{Rgb, Surface};

    cmf_wp2_app::entry!(main);
    cmf_wp2_app::app_icon!();

    const HEARTBEAT_MS: u32 = 2500;
    /// While a page-1 key is held the watch repeats `c1p` this often; the phone lets go if it hears nothing for ~1 s.
    const HOLD_PING_MS: u32 = 250;
    /// Crown volume steps within this window go out as one message.
    const VOL_BATCH_MS: u32 = 60;
    /// Sent in hello: 5 = hold (`c1p`/`c1r`) and batched volume (`c1v`).
    const PROTOCOL: u8 = 5;
    /// Volume mode ends after this long without a crown step.
    const VOL_MS: u32 = 1200;

    const FACE: Rgb = Rgb(58, 58, 60);
    const FACE_DOWN: Rgb = Rgb(96, 96, 100);
    const ARROW: Rgb = Rgb(200, 200, 204);
    const ACCENT: Rgb = Rgb(138, 180, 248);

    fn span(s: &mut &Api, y: i32, x0: i32, x1: i32, c: Rgb) {
        if x1 > x0 {
            Surface::rect(s, x0, y, x1 - x0, 1, c.rgb565());
        }
    }

    fn tri(s: &mut &Api, dir: u8, cx: i32, cy: i32, size: i32, c: Rgb) {
        triangle(dir, cx, cy, size, |y, x0, x1| span(s, y, x0, x1, c));
    }

    fn dots(s: &mut &Api, page: u8) {
        for i in 0..2i32 {
            let x = C + (i * 2 - 1) * (DOT_GAP / 2);
            disc(s, x, DOT_Y, if i == page as i32 { 6 } else { 4 }, if i == page as i32 { ARROW } else { FACE_DOWN }, BLACK);
        }
    }

    fn wedge(s: &mut &Api, dir: u8, on: bool) {
        let c = if on { FACE_DOWN } else { FACE };
        for y in (C - R)..=(C + R) {
            if let Some((a, b)) = wedge_span(dir, y - C, 5) {
                span(s, y, C + a, C + b + 1, c);
            }
        }
    }

    fn draw_page1(api: &Api, pressed: Option<u8>) {
        let mut s = api;
        Surface::rect(&mut s, 0, 0, SIZE, SIZE, BLACK.rgb565());
        for (dir, id) in [(0u8, UP), (1, DOWN), (2, LEFT), (3, RIGHT)] {
            wedge(&mut s, dir, pressed == Some(id));
        }
        disc(&mut s, C, C, RC + 12, BLACK, FACE);
        let cc = if pressed == Some(CENTRE) { FACE_DOWN } else { FACE };
        disc(&mut s, C, C, RC + 2, cc, BLACK);
        ring(&mut s, C, C, RC - 6, RC + 2, ARROW, cc);
        for (dir, x, y) in [(0u8, C, C - 125), (1, C, C + 125), (2, C - 125, C), (3, C + 125, C)] {
            tri(&mut s, dir, x, y, 20, ARROW);
        }
        dots(&mut s, 0);
        api.show();
    }

    /// Draws icon `id` centred on (cx, cy) over `bg`, antialiased, merging same-colour pixels into runs.
    fn icon(s: &mut &Api, id: u8, cx: i32, cy: i32, c: Rgb, bg: Rgb) {
        use cmf_wp2_ui::draw::Run;
        let h = icons::HALF;
        for py in -h..h {
            let mut run = Run::new(s, cy + py, false);
            for px in -h..h {
                let k = icons::coverage(id, px, py);
                let col = if k == 0 { bg } else if k >= 16 { c } else { bg.mix(c, k * 64) };
                run.put(cx + px, 1, col.rgb565());
            }
        }
    }

    fn draw_page2(api: &Api, pressed: Option<u8>, vol_on: bool) {
        let mut s = api;
        Surface::rect(&mut s, 0, 0, SIZE, SIZE, BLACK.rgb565());
        for &(id, cx, cy, r) in BUTTONS.iter() {
            let active = id == VOLUME && vol_on;
            let fill = if pressed == Some(id) { FACE_DOWN } else { FACE };
            disc(&mut s, cx, cy, r, fill, BLACK);
            if active {
                ring(&mut s, cx, cy, r - 5, r, ACCENT, fill);
            }
            icon(&mut s, id, cx, cy, if active { ACCENT } else { ARROW }, fill);
        }
        dots(&mut s, 1);
        api.show();
    }

    fn redraw(api: &Api, page: u8, pressed: Option<u8>, vol_on: bool) {
        if page == 0 { draw_page1(api, pressed) } else { draw_page2(api, pressed, vol_on) }
    }

    fn msg(api: &Api, d: &[u8]) {
        let _ = api.send(d);
    }
    fn key(api: &Api, button: u8, gesture: u8) {
        msg(api, &[b'c', b'1', b'k', button, gesture]);
    }
    /// Hold protocol: `c1p id` = pressed / still held (repeated every HOLD_PING_MS), `c1r id` = released.
    fn hold(api: &Api, button: u8, down: bool) {
        msg(api, &[b'c', b'1', if down { b'p' } else { b'r' }, button]);
    }
    /// `c1v n`: n volume steps in one message (signed, + = up), so a fast crown spin is one message, not ten.
    fn volume(api: &Api, steps: i32) {
        msg(api, &[b'c', b'1', b'v', steps.clamp(-100, 100) as i8 as u8]);
    }

    fn holdable(id: u8) -> bool {
        matches!(id, UP | DOWN | LEFT | RIGHT | CENTRE)
    }

    fn main(api: &'static Api) -> i32 {
        api.own_back(true);
        api.keep_screen_on(true);
        msg(api, &[b'c', b'1', b'h', PROTOCOL]);
        let mut beat = api.now();
        let mut pressed: Option<u8> = None;
        let mut page = 0u8;
        let mut vol_until: Option<u32> = None; // crown volume mode, until this time
        let mut hold_next: Option<u32> = None; // next "still held" ping while a page-1 key is down
        let mut vol_pending = 0i32; // crown steps not sent yet
        let mut vol_flush: Option<u32> = None; // when the next batched volume message may go
        draw_page1(api, None);
        loop {
            let now = api.now();
            if now.wrapping_sub(beat) >= HEARTBEAT_MS {
                msg(api, &[b'c', b'1', b'f']);
                beat = now;
            }
            if let (Some(t), Some(id)) = (hold_next, pressed) {
                if now.wrapping_sub(t) as i32 >= 0 {
                    hold(api, id, true);
                    hold_next = Some(now.wrapping_add(HOLD_PING_MS));
                }
            }
            if let Some(t) = vol_flush {
                if now.wrapping_sub(t) as i32 >= 0 {
                    if vol_pending != 0 {
                        volume(api, vol_pending);
                        vol_pending = 0;
                        vol_flush = Some(now.wrapping_add(VOL_BATCH_MS));
                    } else {
                        vol_flush = None;
                    }
                }
            }
            if let Some(t) = vol_until {
                if now.wrapping_sub(t) as i32 >= 0 {
                    vol_until = None;
                    if page == 1 {
                        draw_page2(api, None, false);
                    }
                }
            }
            let mut wait = HEARTBEAT_MS.saturating_sub(now.wrapping_sub(beat)).max(1);
            for t in [vol_until, hold_next, vol_flush].into_iter().flatten() {
                wait = wait.min((t.wrapping_sub(now) as i32).max(1) as u32);
            }
            let Some(e) = api.event(wait) else { continue };
            match e.kind {
                k if k == BUTTON => {
                    if let Some(id) = pressed.filter(|&id| holdable(id)) {
                        hold(api, id, false);
                    }
                    msg(api, &[b'c', b'1', b'z']);
                    return 0;
                }
                k if k == RESUME => redraw(api, page, None, vol_until.is_some()),
                k if k == PAUSE => {
                    if let Some(id) = pressed.take().filter(|&id| holdable(id)) {
                        hold(api, id, false);
                    }
                    hold_next = None;
                    vol_until = None;
                }
                k if k == CROWN => {
                    if vol_until.is_some() {
                        let step = if e.delta > 0 { 1 } else { -1 };
                        if vol_flush.is_none() {
                            // first step goes out at once; the rest of a fast spin is batched
                            volume(api, step);
                            vol_flush = Some(api.now().wrapping_add(VOL_BATCH_MS));
                        } else {
                            vol_pending += step;
                        }
                        vol_until = Some(api.now().wrapping_add(VOL_MS));
                    } else {
                        let new = if e.delta > 0 { 1 } else { 0 };
                        if new != page && pressed.is_none() {
                            page = new;
                            redraw(api, page, None, false);
                        }
                    }
                }
                k if k == TOUCH_DOWN => {
                    let (x, y) = (e.x as i32, e.y as i32);
                    pressed = if page == 0 { hit_page1(x, y) } else { hit_page2(x, y) };
                    // send first, draw after: the press reaches the phone before the screen work starts
                    if let Some(id) = pressed {
                        if page == 0 && holdable(id) {
                            hold(api, id, true);
                            hold_next = Some(api.now().wrapping_add(HOLD_PING_MS));
                        } else if page == 1 && id != DOTS && id != VOLUME {
                            key(api, id, 0);
                        }
                        redraw(api, page, pressed, vol_until.is_some());
                    }
                }
                k if k == TOUCH_UP => {
                    if let Some(id) = pressed.take() {
                        hold_next = None;
                        match id {
                            DOTS => {
                                page = 1 - page;
                                vol_until = None;
                            }
                            VOLUME => {
                                vol_until = if vol_until.is_some() { None } else { Some(api.now().wrapping_add(VOL_MS)) };
                            }
                            _ if holdable(id) => hold(api, id, false),
                            _ => {} // page-2 keys were already sent on touch-down
                        }
                        redraw(api, page, None, vol_until.is_some());
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::logic::*;

    #[test]
    fn page1_wedges_by_angle() {
        assert_eq!(hit_page1(C, C - 150), Some(UP));
        assert_eq!(hit_page1(C, C + 120), Some(DOWN));
        assert_eq!(hit_page1(C - 150, C), Some(LEFT));
        assert_eq!(hit_page1(C + 150, C), Some(RIGHT));
        assert_eq!(hit_page1(C + 120, C - 100), Some(RIGHT));
        assert_eq!(hit_page1(C + 100, C - 120), Some(UP));
        assert_eq!(hit_page1(C + 5, C + 5), Some(CENTRE));
        assert_eq!(hit_page1(2, 2), None);
        assert_eq!(hit_page1(C, DOT_Y), Some(DOTS));
    }

    #[test]
    fn page2_buttons_hit_and_do_not_overlap() {
        for &(id, cx, cy, _) in BUTTONS.iter() {
            assert_eq!(hit_page2(cx, cy), Some(id));
        }
        assert_eq!(hit_page2(C, DOT_Y), Some(DOTS));
        assert_eq!(hit_page2(2, 2), None);
        for (i, a) in BUTTONS.iter().enumerate() {
            for b in BUTTONS.iter().skip(i + 1) {
                let d2 = (a.1 - b.1) * (a.1 - b.1) + (a.2 - b.2) * (a.2 - b.2);
                assert!(d2 > (a.3 + b.3) * (a.3 + b.3), "buttons {} and {} overlap", a.0, b.0);
            }
        }
    }

    #[test]
    fn page2_buttons_are_inside_the_round_screen_and_above_the_dots() {
        for &(id, cx, cy, r) in BUTTONS.iter() {
            let far = isqrt((cx - C) * (cx - C) + (cy - C) * (cy - C)) + r;
            assert!(far <= R - 6, "button {id} too close to the rim");
            assert!(cy + r < DOT_Y - 18, "button {id} touches the page dots");
        }
    }

    #[test]
    fn wedges_do_not_overlap_and_stay_in_disc() {
        for dy in -R..=R {
            let mut taken = vec![false; (2 * R + 2) as usize];
            for dir in 0..4u8 {
                if let Some((a, b)) = wedge_span(dir, dy, 5) {
                    assert!(b * b + dy * dy <= R * R + 2 * R && a * a + dy * dy <= R * R + 2 * R);
                    for x in a..=b {
                        let i = (x + R) as usize;
                        assert!(!taken[i], "overlap at dy {dy} x {x}");
                        taken[i] = true;
                    }
                }
            }
        }
    }

    #[test]
    fn triangles_stay_inside_their_box() {
        for dir in 0..4u8 {
            let mut n = 0;
            triangle(dir, 100, 100, 20, |y, x0, x1| {
                assert!((80..=120).contains(&y) && x0 >= 80 && x1 <= 121 && x1 > x0, "dir {dir} y {y} {x0} {x1}");
                n += 1;
            });
            assert!(n >= 30);
        }
    }

    #[test]
    fn icons_fit_their_box_and_are_centred() {
        let h = icons::HALF;
        for &(id, _, _, r) in BUTTONS.iter() {
            let (mut n, mut sx, mut sy) = (0i64, 0i64, 0i64);
            let (mut x0, mut x1, mut y0, mut y1) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
            for py in -h - 8..h + 8 {
                for px in -h - 8..h + 8 {
                    let k = icons::coverage(id, px, py);
                    if k > 0 {
                        assert!(px >= -h && px < h && py >= -h && py < h, "icon {id} leaves its box at {px},{py}");
                        assert!(px * px + py * py < (r - 6) * (r - 6), "icon {id} touches its button edge");
                        n += k as i64; sx += k as i64 * px as i64; sy += k as i64 * py as i64;
                        x0 = x0.min(px); x1 = x1.max(px); y0 = y0.min(py); y1 = y1.max(py);
                    }
                }
            }
            assert!(n > 16 * 150, "icon {id} is empty");
            // bounding box centred within 2 px (the pixel grid is offset by half a pixel)
            assert!((x0 + x1).abs() <= 3 && (y0 + y1).abs() <= 3, "icon {id} box {x0}..{x1} x {y0}..{y1} is off centre");
            // not stretched: width and height within 1.7x of each other
            let (w, hh) = (x1 - x0 + 1, y1 - y0 + 1);
            assert!(w * 10 <= hh * 17 && hh * 10 <= w * 17, "icon {id} is stretched {w}x{hh}");
        }
    }

    /// `WR_PREVIEW=/dir cargo test preview -- --ignored` writes page1.ppm and page2.ppm (store screenshots).
    /// Same geometry and colours as the device drawing code; the round screen edge is masked.
    #[test]
    #[ignore]
    fn preview() {
        let dir = std::env::var("WR_PREVIEW").unwrap();
        type Px = [f32; 3];
        let (face, down, fg): (Px, Px, Px) = ([58., 58., 60.], [96., 96., 100.], [200., 200., 204.]);
        let mix = |a: Px, b: Px, t: f32| -> Px { core::array::from_fn(|i| a[i] + (b[i] - a[i]) * t) };
        let save = |name: &str, f: &dyn Fn(i32, i32) -> Px| {
            let mut out = format!("P6 {SIZE} {SIZE} 255\n").into_bytes();
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let (dx, dy) = (x - C, y - C);
                    let p = if dx * dx + dy * dy > C * C { [0.; 3] } else { f(x, y) };
                    out.extend(p.iter().map(|v| *v as u8));
                }
            }
            std::fs::write(format!("{dir}/{name}"), out).unwrap();
        };
        let dots = |x: i32, y: i32, page: i32| -> Option<Px> {
            for i in 0..2 {
                let cx = C + (i * 2 - 1) * (DOT_GAP / 2);
                let r = if i == page { 6 } else { 4 };
                if (x - cx) * (x - cx) + (y - DOT_Y) * (y - DOT_Y) <= r * r {
                    return Some(if i == page { fg } else { down });
                }
            }
            None
        };
        save("page1.ppm", &|x, y| {
            if let Some(d) = dots(x, y, 0) { return d; }
            let (dx, dy) = (x - C, y - C);
            let d2 = dx * dx + dy * dy;
            let mut arrow = false;
            for (dir, ax, ay) in [(0u8, C, C - 125), (1, C, C + 125), (2, C - 125, C), (3, C + 125, C)] {
                triangle(dir, ax, ay, 20, |ty, x0, x1| if ty == y && x >= x0 && x < x1 { arrow = true });
            }
            if arrow { return fg; }
            if d2 <= (RC + 2) * (RC + 2) {
                return if d2 >= (RC - 6) * (RC - 6) { fg } else { face };
            }
            if d2 <= (RC + 12) * (RC + 12) { return [0.; 3]; }
            for dir in 0..4u8 {
                if let Some((a, b)) = wedge_span(dir, dy, 5) {
                    if dx >= a && dx <= b { return face; }
                }
            }
            [0.; 3]
        });
        save("page2.ppm", &|x, y| {
            if let Some(d) = dots(x, y, 1) { return d; }
            for &(id, cx, cy, r) in BUTTONS.iter() {
                let (dx, dy) = (x - cx, y - cy);
                if dx * dx + dy * dy <= r * r {
                    return mix(face, fg, icons::coverage(id, dx, dy) as f32 / 16.0);
                }
            }
            [0.; 3]
        });
    }
}
