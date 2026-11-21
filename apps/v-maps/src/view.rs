//! Drawing, against `Surface`: the watch through `&Api`, a host framebuffer in tests.
//!
//! Layout on the round 466 px panel, black like the stock faces: the time left, big, at the top; a heading-up map
//! below it: the route in blue, the streets around you in grey (enough to see which turn to take), the car; the
//! next turn big at the bottom (arrow, distance) over an arc that fills as the drive goes. Text is Noto
//! Sans (`font`, `fonts`), with the loader's pixel font for a road name in a script those fonts lack.

use crate::font::{poly, poly_half};
use crate::fonts::{L, S, XL};
use crate::proto::{Model, ROADS_LINES_MAX, man};
use crate::sign::{Sign, head};
use cmf_wp2_ui::draw::{disc, line, ring};
use cmf_wp2_ui::motion::{Q14, cos, sin};
use cmf_wp2_ui::text::fitted;
use cmf_wp2_ui::{C, CHAR, Rgb, SIZE, Surface, geom};
use core::fmt::Write;

const BLACK: Rgb = Rgb(0, 0, 0);
const WHITE: Rgb = Rgb(255, 255, 255);
const SHADE: Rgb = Rgb(178, 180, 186);
const GREY: Rgb = Rgb(140, 140, 146);
const BLUE: Rgb = Rgb(52, 138, 255);
/// The map's background: a dark slate blue (the dome is black).
const MAP_BG: Rgb = Rgb(22, 30, 42);
const STREET: Rgb = Rgb(46, 58, 76);
const MAJOR: Rgb = Rgb(78, 94, 118); // main roads: dimmer than the text, which is 178 and 255
const ACCENT: Rgb = Rgb(255, 160, 0);
const RED: Rgb = Rgb(228, 36, 36);

/// The map fills the whole face; the time left and the next turn are drawn over it.
pub const TOP: i32 = 0;
pub const BOTTOM: i32 = SIZE;
/// The car, heading up.
pub const CAR: (i32, i32) = (C, 278);
/// The marker's disc.
const CAR_R: i32 = 27;
/// Metres between the route's dots (about 3.4 times the radius of an ahead dot).
const ROUTE_STEP: i32 = 24;
/// Map scales the crown steps through, in metres per pixel.
pub const ZOOMS: [i32; 6] = [1, 2, 3, 5, 8, 13];
#[cfg(test)]
const ZOOM_DEFAULT: usize = 2;

/// What the screen is about, besides the model itself.
pub struct Ctx {
    /// The phone has been heard from recently.
    pub live: bool,
    /// Local time of day (hours, minutes), when the watch has it.
    pub zoom: usize,
    /// The map is being zoomed or turned: the grey streets are drawn with hard edges (cheaper) until it settles.
    pub fast: bool,
    /// Where the phone is looking (a turn, 65536 = 360 degrees clockwise from north): the arrow points that way
    /// while the map stays put. `None`: the map is turned to the heading instead, and the arrow points up.
    pub look: Option<u16>,
    /// Only the arrow changed: repaint the patch of map round it, not the whole screen.
    pub arrow_only: bool,
    /// The car is moving: repaint the whole screen
    /// in the cheap hard-edged style.
    pub light: bool,
    /// Only the text in the dome changed: repaint the dome and nothing else.
    pub dome: bool,
}

/// A small text buffer for `write!` without a heap.
pub struct Buf {
    b: [u8; 32],
    n: usize,
}

impl Buf {
    pub fn new() -> Buf {
        Buf { b: [0; 32], n: 0 }
    }
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.b[..self.n]).unwrap_or("")
    }
}

impl Default for Buf {
    fn default() -> Self {
        Buf::new()
    }
}

impl Write for Buf {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let k = s.len().min(self.b.len() - self.n);
        self.b[self.n..self.n + k].copy_from_slice(&s.as_bytes()[..k]);
        self.n += k;
        Ok(())
    }
}

/// "350 m", "1.2 km", "14 km"; "500 ft", "0.3 mi", "12 mi".
pub fn distance(m: u32, imperial: bool) -> Buf {
    let mut b = Buf::new();
    let _ = if imperial {
        if m < 161 {
            write!(b, "{} ft", (m * 3281 / 1000 + 25) / 50 * 50)
        } else {
            let tenths = (m * 100 + 8047) / 16093; // m / 160.93, rounded
            if tenths < 100 {
                write!(b, "{}.{} mi", tenths / 10, tenths % 10)
            } else {
                write!(b, "{} mi", (tenths + 5) / 10)
            }
        }
    } else if m < 1000 {
        write!(b, "{} m", if m < 100 { m / 5 * 5 } else { m / 10 * 10 })
    } else if m < 10_000 {
        let h = (m + 50) / 100;
        write!(b, "{}.{} km", h / 10, h % 10)
    } else {
        write!(b, "{} km", (m + 500) / 1000)
    };
    b
}

/// "14 min", "1 h 05".
pub fn duration(s: u32) -> Buf {
    let mut b = Buf::new();
    let min = (s + 30) / 60;
    let _ = if min < 60 { write!(b, "{} min", min) } else { write!(b, "{} h {:02}", min / 60, min % 60) };
    b
}

/// One map-frame transform. Heading sine/cosine and the metre scale are calculated once, then reused for
/// every route and street point in that frame. This removes two fixed-point trig calls per projected point.
#[derive(Clone, Copy, Debug)]
pub struct Projector {
    car: [i32; 2],
    s: i32,
    c: i32,
    mpp: i32,
}

impl Projector {
    #[inline]
    pub fn new(car: [i32; 2], course: u16, mpp: i32) -> Self {
        Self { car, s: sin(course) as i32, c: cos(course) as i32, mpp: mpp.max(1) }
    }

    #[inline(always)]
    pub fn project(&self, p: [i32; 2]) -> (i32, i32) {
        const LIM: i64 = 1 << 20;
        let (dx, dy) = (p[0] as i64 - self.car[0] as i64, p[1] as i64 - self.car[1] as i64);
        if dx.abs().max(dy.abs()) <= 60_000 {
            // 32 bits and the hardware divide for nearby points.
            let (dx, dy) = (dx as i32, dy as i32);
            let d = Q14 as i32 * self.mpp;
            let (right, ahead) = ((dx * self.c - dy * self.s) / d, (dx * self.s + dy * self.c) / d);
            return (CAR.0 + right, CAR.1 - ahead);
        }
        let (s, c) = (self.s as i64, self.c as i64);
        let d = Q14 as i64 * self.mpp as i64;
        let right = (dx * c - dy * s) / d;
        let ahead = (dx * s + dy * c) / d;
        ((CAR.0 as i64 + right.clamp(-LIM, LIM)) as i32, (CAR.1 as i64 - ahead.clamp(-LIM, LIM)) as i32)
    }
}

/// Convenience projection for tests and one-off geometry. Render frames should share a `Projector`.
#[cfg(test)]
#[inline]
pub fn project(p: [i32; 2], car: [i32; 2], course: u16, mpp: i32) -> (i32, i32) {
    Projector::new(car, course, mpp).project(p)
}

/// Which sides of the box `reach` metres round the car a point lies outside, as bits: two points that share a bit
/// have a segment between them that is wholly outside. Nothing farther than the screen can show is turned or drawn.
fn world_code(p: [i32; 2], car: [i32; 2], reach: i32) -> u8 {
    let (dx, dy) = (p[0].saturating_sub(car[0]), p[1].saturating_sub(car[1]));
    (dx < -reach) as u8 | ((dx > reach) as u8) << 1 | ((dy < -reach) as u8) << 2 | ((dy > reach) as u8) << 3
}

/// How far from the car, in metres, anything can still be on the screen (its farthest edge is `CAR.1` px away).
fn reach(mpp: i32) -> i32 {
    (CAR.1 + 24) * mpp
}

/// Cohen–Sutherland: the part of a segment inside `[lo, hi]²`, or `None`.
pub fn clip(mut a: (i32, i32), mut b: (i32, i32), lo: i32, hi: i32) -> Option<((i32, i32), (i32, i32))> {
    let code =
        |p: (i32, i32)| (p.0 < lo) as u8 | ((p.0 > hi) as u8) << 1 | ((p.1 < lo) as u8) << 2 | ((p.1 > hi) as u8) << 3;
    let (mut ca, mut cb) = (code(a), code(b));
    for _ in 0..8 {
        if ca | cb == 0 {
            return Some((a, b));
        }
        if ca & cb != 0 {
            return None;
        }
        let out = if ca != 0 { ca } else { cb };
        let (x0, y0, x1, y1) = (a.0 as i64, a.1 as i64, b.0 as i64, b.1 as i64);
        let p = if out & 8 != 0 {
            (x0 + (x1 - x0) * (hi as i64 - y0) / (y1 - y0), hi as i64)
        } else if out & 4 != 0 {
            (x0 + (x1 - x0) * (lo as i64 - y0) / (y1 - y0), lo as i64)
        } else if out & 2 != 0 {
            (hi as i64, y0 + (y1 - y0) * (hi as i64 - x0) / (x1 - x0))
        } else {
            (lo as i64, y0 + (y1 - y0) * (lo as i64 - x0) / (x1 - x0))
        };
        let p = (p.0 as i32, p.1 as i32);
        if out == ca {
            (a, ca) = (p, code(p));
        } else {
            (b, cb) = (p, code(p));
        }
    }
    None
}

/// Rounds the corners of a polyline as it streams by: each vertex becomes a short curve between the midpoints of
/// its two segments (a quadratic through two extra points), so the bends of a road are not spikes. No storage:
/// `emit(from, to)` gets the straight pieces. Fewer than 3 points is just the segment.
#[derive(Default)]
struct Curve {
    a: (i32, i32),
    b: (i32, i32),
    pen: (i32, i32),
    n: u8,
    /// A bend of less than about 20 degrees is left as it is (it already looks smooth): in the cheap frames only.
    gentle_is_straight: bool,
}

impl Curve {
    fn push(&mut self, p: (i32, i32), smooth: bool, emit: &mut impl FnMut((i32, i32), (i32, i32))) {
        match self.n {
            0 => (self.a, self.pen, self.n) = (p, p, 1),
            1 => (self.b, self.n) = (p, 2),
            _ => {
                if smooth && !(self.gentle_is_straight && self.gentle(p)) {
                    let (a, b) = (self.a, self.b);
                    let m1 = ((a.0 + b.0) / 2, (a.1 + b.1) / 2);
                    let m2 = ((b.0 + p.0) / 2, (b.1 + p.1) / 2);
                    let q = ((m1.0 + 2 * b.0 + m2.0) / 4, (m1.1 + 2 * b.1 + m2.1) / 4);
                    emit(self.pen, m1);
                    emit(m1, q);
                    emit(q, m2);
                    self.pen = m2;
                } else {
                    emit(self.pen, self.b);
                    self.pen = self.b;
                }
                (self.a, self.b) = (self.b, p);
            }
        }
    }

    /// Whether the bend at `b` towards `p` is under about 20 degrees.
    fn gentle(&self, p: (i32, i32)) -> bool {
        let (ux, uy) = ((self.b.0 - self.a.0) as i64, (self.b.1 - self.a.1) as i64);
        let (vx, vy) = ((p.0 - self.b.0) as i64, (p.1 - self.b.1) as i64);
        let dot = ux * vx + uy * vy;
        // cos^2 >= 0.88 (cos 20 degrees is 0.94), going on
        dot > 0 && dot * dot * 100 >= 88 * (ux * ux + uy * uy) * (vx * vx + vy * vy)
    }

    fn finish(&mut self, emit: &mut impl FnMut((i32, i32), (i32, i32))) {
        if self.n == 2 {
            emit(self.pen, self.b);
        }
    }
}

/// Integer square root.
fn isqrt64(n: u64) -> u64 {
    if n <= u32::MAX as u64 {
        // the usual case (a segment under 46 km): shifts and adds, no divide
        let (mut n, mut r, mut b) = (n as u32, 0u32, 1u32 << 30);
        while b > n {
            b >>= 2;
        }
        while b != 0 {
            if n >= r + b {
                n -= r + b;
                r = (r >> 1) + b;
            } else {
                r >>= 1;
            }
            b >>= 2;
        }
        return r as u64;
    }
    let mut x = 1u64 << ((64 - n.leading_zeros()).div_ceil(2));
    loop {
        let y = (x + n / x) / 2;
        if y >= x {
            return x;
        }
        x = y;
    }
}

/// A hard-edged disc: one rect a row, equal rows joined.
fn flat_disc(s: &mut impl Surface, cx: i32, cy: i32, r: i32, c: Rgb) {
    let k = c.rgb565();
    let mut run: Option<(i32, i32)> = None; // first row, half width
    for dy in -r..=r + 1 {
        let hw = if dy <= r { cmf_wp2_ui::motion::isqrt(r * r + r - dy * dy) } else { -1 };
        match run {
            Some((_, h)) if h == hw => {}
            _ => {
                if let Some((y0, h)) = run && h >= 0 {
                    s.rect(cx - h, cy + y0, 2 * h + 1, dy - y0, k);
                }
                run = Some((dy, hw));
            }
        }
    }
}

/// The route as a line of round dots, `step` metres apart, from vertex `from` to vertex `to`. The dots sit at fixed
/// distances along the whole route (counted from its start), so they stay where they are on the map as it moves.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn dots(s: &mut impl Surface, m: &Model, from: usize, to: usize, r: i32, step: i32, c: Rgb, mpp: i32, fast: bool) {
    let projector = Projector::new([m.state.x, m.state.y], m.state.course, mpp);
    dots_projected(s, m, from, to, r, step, c, &projector, fast);
}

#[allow(clippy::too_many_arguments)]
fn dots_projected(s: &mut impl Surface, m: &Model, from: usize, to: usize, r: i32, step: i32, c: Rgb, projector: &Projector, fast: bool) {
    let car = projector.car;
    let mpp = projector.mpp;
    let step = step as i64 * mpp as i64;
    let reach = reach(mpp);
    let mut cum = 0i64;
    for i in 0..to.min(m.route_len).saturating_sub(1) {
        let (a, b) = (m.route[i], m.route[i + 1]);
        let (dx, dy) = (b[0] as i64 - a[0] as i64, b[1] as i64 - a[1] as i64);
        let len = isqrt64((dx * dx + dy * dy) as u64) as i64;
        // a segment wholly beyond the reach only adds its length; its dots are not even turned
        if i >= from && len > 0 && world_code(a, car, reach) & world_code(b, car, reach) == 0 {
            // a dot on screen is within 1.5 reach (along the segment) of the point of it nearest the car
            let near = ((car[0] as i64 - a[0] as i64) * dx + (car[1] as i64 - a[1] as i64) * dy) / len;
            let (lo, hi) = ((near - reach as i64 * 3 / 2).max(0), near + reach as i64 * 3 / 2);
            let mut d = (cum + lo + step - 1) / step * step; // the first dot at or after the start of what can show
            while d < cum + len && d <= cum + hi {
                let t = d - cum;
                let p = projector.project([(a[0] as i64 + dx * t / len) as i32, (a[1] as i64 + dy * t / len) as i32]);
                if geom::inside(p.0, p.1) {
                    if fast {
                        flat_disc(s, p.0, p.1, r, c);
                    } else {
                        disc(s, p.0, p.1, r, c, MAP_BG);
                    }
                }
                d += step;
            }
        }
        cum += len;
    }
}

/// Whether a segment can touch the round screen (with `w` to spare): its distance from the centre is under the
/// radius. Pieces entirely in the corners of the square are never drawn.
fn near_screen(a: (i32, i32), b: (i32, i32), w: i32) -> bool {
    let (ax, ay) = (a.0 as i64 - C as i64, a.1 as i64 - C as i64);
    let (dx, dy) = (b.0 as i64 - a.0 as i64, b.1 as i64 - a.1 as i64);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0 { 0 } else { (-(ax * dx + ay * dy)).clamp(0, len2) };
    // the closest point is a + d * t / len2; compare its squared distance with (C + w + 2)^2
    let (px, py) = (ax * len2 + dx * t, ay * len2 + dy * t);
    let r = (C + w + 2) as i128;
    (px as i128 * px as i128 + py as i128 * py as i128) <= r * r * len2 as i128 * len2 as i128
}

/// A hard-edged line of width `w`: one rect per row (or per run of columns), no antialiasing, no round ends. About
/// a third of the rects of `line`; for the frames while the map is being zoomed or turned.
fn fast_line(s: &mut impl Surface, a: (i32, i32), b: (i32, i32), w: i32, c: Rgb) {
    let k = c.rgb565();
    let (dx, dy) = ((b.0 - a.0) as i64, (b.1 - a.1) as i64);
    let len = cmf_wp2_ui::motion::isqrt((dx * dx + dy * dy).min(i32::MAX as i64) as i32) as i64;
    if len == 0 {
        return;
    }
    if dy.abs() >= dx.abs() {
        // steep: a rect across the line on every row, equal neighbours joined
        let th = ((w as i64 * len + dy.abs() / 2) / dy.abs()).max(w as i64) as i32;
        let (y0, y1) = (a.1.min(b.1).max(0), a.1.max(b.1).min(SIZE - 1));
        let mut run: Option<(i32, i32, i32)> = None; // y, left, right
        for y in y0..=y1 {
            let xc = a.0 as i64 + dx * (y - a.1) as i64 / dy;
            let h = geom::half(y);
            let (l, r) = ((xc as i32 - th / 2).max(C - h), (xc as i32 - th / 2 + th).min(C + h));
            match run {
                Some((_, rl, rr)) if rl == l && rr == r => {}
                _ => {
                    if let Some((ry, rl, rr)) = run && rr > rl {
                        s.rect(rl, ry, rr - rl, y - ry, k);
                    }
                    run = Some((y, l, r));
                }
            }
        }
        if let Some((ry, rl, rr)) = run && rr > rl {
            s.rect(rl, ry, rr - rl, y1 + 1 - ry, k);
        }
    } else {
        // flat: a rect up and down on every column, equal neighbours joined
        let th = ((w as i64 * len + dx.abs() / 2) / dx.abs()).max(w as i64) as i32;
        let (x0, x1) = (a.0.min(b.0).max(0), a.0.max(b.0).min(SIZE - 1));
        let mut run: Option<(i32, i32, i32)> = None; // x, top, bottom
        for x in x0..=x1 {
            let yc = a.1 as i64 + dy * (x - a.0) as i64 / dx;
            let h = geom::half(x);
            let (t, bt) = ((yc as i32 - th / 2).max(C - h), (yc as i32 - th / 2 + th).min(C + h));
            match run {
                Some((rx, rt, rb)) if rt == t && rb == bt => run = Some((rx, rt, rb)),
                _ => {
                    if let Some((rx, rt, rb)) = run && rb > rt {
                        s.rect(rx, rt, x - rx, rb - rt, k);
                    }
                    run = Some((x, t, bt));
                }
            }
        }
        if let Some((rx, rt, rb)) = run && rb > rt {
            s.rect(rx, rt, x1 + 1 - rx, rb - rt, k);
        }
    }
}

/// The streets around the car, thin and grey, under the route: enough to see which turns not to take.
#[cfg(test)]
fn streets(s: &mut impl Surface, m: &Model, mpp: i32, fast: bool) {
    let projector = Projector::new([m.state.x, m.state.y], m.state.course, mpp);
    streets_projected(s, m, &projector, fast);
}

/// Whether bends are rounded in the cheap frames (while the map moves) as they are in the full one. A rounded bend is
/// three short lines instead of one; straight streets are not affected. `false`: the old sharp corners.
const ROUND_WHILE_MOVING: bool = true;

fn streets_projected(s: &mut impl Surface, m: &Model, projector: &Projector, fast: bool) {
    let car = projector.car;
    let world = |i: usize| [m.roads_center[0] + m.roads[i][0] as i32, m.roads_center[1] + m.roads[i][1] as i32];
    let at = |i: usize| projector.project(world(i));
    let reach = reach(projector.mpp);
    let code = |i: usize| world_code(world(i), car, reach);
    for pass in 0..2 {
    for l in 0..m.roads_count.min(ROADS_LINES_MAX) {
        let start = m.roads_starts[l] as usize;
        let end = (start + m.roads_sizes[l] as usize).min(m.roads_len);
        let mut len = 0i32;
        for i in start + 1..end {
            len += (m.roads[i][0] as i32 - m.roads[i - 1][0] as i32).abs() + (m.roads[i][1] as i32 - m.roads[i - 1][1] as i32).abs();
        }
        let major = len > 500;
        if major != (pass == 1) {
            continue;
        }
        let w = if major { 10 } else { 6 };
        let col = if major { MAJOR } else { STREET };
        // a point in the same pixel as the one before it adds nothing at this zoom: left out (never a line's end)
        let near = if fast { 2 } else { 1 };
        let same = |q: (i32, i32), p: (i32, i32), i: usize| i + 1 < end && (p.0 - q.0).abs() <= near && (p.1 - q.1).abs() <= near;
        for pass in 0..(if fast { 1 } else { 2 }) {
            let mut emit = |q: (i32, i32), p: (i32, i32)| {
                if pass == 0 {
                    if !(q.1 < TOP - w && p.1 < TOP - w || q.1 > BOTTOM + w && p.1 > BOTTOM + w) && near_screen(q, p, w) {
                        if fast {
                            fast_line(s, q, p, w, col);
                        } else if let Some((a, b)) = clip(q, p, -w, SIZE + w) {
                            line(s, a.0, a.1, b.0, b.1, w, col, MAP_BG);
                        }
                    }
                } else if geom::inside(q.0, q.1) && q.1 >= TOP - w && q.1 <= BOTTOM + w {
                    disc(s, q.0, q.1, w / 2 - 1, col, col);
                }
            };
            let mut k = Curve { gentle_is_straight: fast, ..Curve::default() };
            let mut last: Option<(i32, i32)> = None;
            let mut cut = false;
            for i in start..end {
                // a segment along the route is not drawn (the route covers it): the line is cut there
                if i > start && m.under_route(i - 1) {
                    cut = true;
                }
                // a point whose segments both lie beyond the reach is not turned at all; the line is cut there
                let seen_before = i > start && code(i - 1) & code(i) == 0;
                let seen_after = i + 1 < end && code(i) & code(i + 1) == 0;
                if !seen_before && !seen_after {
                    cut = true;
                    continue;
                }
                if !seen_before {
                    cut = true;
                }
                let p = at(i);
                if !cut && last.is_some_and(|q| same(q, p, i)) {
                    continue;
                }
                if cut {
                    k.finish(&mut emit);
                    (k, cut) = (Curve { gentle_is_straight: fast, ..Curve::default() }, false);
                }
                last = Some(p);
                k.push(p, ROUND_WHILE_MOVING || !fast, &mut emit);
            }
            k.finish(&mut emit);
        }
    }
    }
}

/// Direction `a` (turns, clockwise from up) as a screen vector of length `r`.
fn dir(a: u16, r: i32) -> (i32, i32) {
    ((r * sin(a)) >> 14, -(r * cos(a)) >> 14)
}

/// A maneuver icon in a box of radius `r` around (cx, cy): thick strokes with round ends, like a road sign.
pub fn icon(s: &mut impl Surface, code: u8, cx: i32, cy: i32, r: i32, c: Rgb) {
    icon_sign(s, code, cx, cy, r, c, 0)
}

fn icon_sign(s: &mut impl Surface, code: u8, cx: i32, cy: i32, r: i32, c: Rgb, pad: i32) {
    let w = (r * 2 / 7).max(4) + pad;
    let mut g = Sign::new(w);
    let hl = r * 3 / 5; // arrowhead length
    // a stem up from the bottom that bends towards `a` at `elbow` below the centre
    // (the stem leans away from the turn by `lean` so the whole sign sits centred)
    let turn = |g: &mut Sign, a: u16, elbow: i32, len: i32, lean: i32, hl: i32| {
        let sh = dir(a, len - hl / 2);
        let x = cx - lean;
        let e = (x, cy + elbow);
        let tip = (x + dir(a, len).0, e.1 + dir(a, len).1);
        g.seg((x, cy + r), e).seg(e, (x + sh.0, e.1 + sh.1)).tri(head(tip, a, hl));
    };
    let gentle = |g: &mut Sign, a: u16, elbow: i32| turn(g, a, elbow, r, dir(a, r * 2 / 5).0 / 2, hl);
    let sharp = |g: &mut Sign, a: u16| turn(g, a, -r * 3 / 4, r * 13 / 10, dir(a, r).0 * 7 / 10, r / 2);
    // a right angle with a round bend: up, then `sign` (1 right, -1 left)
    let square = |g: &mut Sign, sign: i32| {
        let rho = r * 3 / 5;
        let (x, y) = (cx - sign * r / 2, cy - r / 3);
        let tip = (cx + sign * r, y);
        let start = if sign > 0 { 0x8000 } else { 0 };
        g.seg((x, cy + r), (x, y + rho))
            .arc((x + sign * rho, y + rho), rho, start, sign * 0x4000, 6)
            .seg((x + sign * rho, y), (tip.0 - sign * hl / 2, y))
            .tri(head(tip, if sign > 0 { 0x4000 } else { 0xC000 }, hl));
    };
    match code {
        man::CONTINUE => {
            g.seg((cx, cy + r), (cx, cy - r + hl / 2)).tri(head((cx, cy - r), 0, hl));
        }
        man::SLIGHT_LEFT => gentle(&mut g, 0xE000, 0),
        man::SLIGHT_RIGHT => gentle(&mut g, 0x2000, 0),
        man::LEFT => square(&mut g, -1),
        man::RIGHT => square(&mut g, 1),
        man::SHARP_LEFT => sharp(&mut g, 0xA000),
        man::SHARP_RIGHT => sharp(&mut g, 0x6000),
        man::KEEP_LEFT => gentle(&mut g, 0xF000, r / 4),
        man::KEEP_RIGHT => gentle(&mut g, 0x1000, r / 4),
        man::UTURN => {
            let (rx, top) = (r / 2, cy - r / 4);
            g.seg((cx + rx, cy + r), (cx + rx, top))
                .arc((cx, top), rx, 0, -0x8000, 10)
                .seg((cx - rx, top), (cx - rx, cy + r - hl / 2))
                .tri(head((cx - rx, cy + r), 0x8000, hl));
        }
        man::ROUNDABOUT => {
            let rr = r * 2 / 5;
            g.ring((cx, cy), rr)
                .seg((cx, cy + r), (cx, cy + rr))
                .seg((cx, cy - rr), (cx, cy - r - 4 + r / 4))
                .tri(head((cx, cy - r - 4), 0, r / 2));
        }
        man::ARRIVE if pad == 0 => {
            let pin = cy - r / 4;
            let k = RED.rgb565();
            poly(s, &[(cx - r / 2 + 1, pin + r / 6), (cx + r / 2 - 1, pin + r / 6), (cx, cy + r)], 0, |_| k);
            disc(s, cx, pin, r / 2, RED, BLACK);
            disc(s, cx, pin, r / 5, WHITE, RED);
        }
        man::OFF_ROUTE if pad == 0 => {
            ring(s, cx, cy, r - w, r, ACCENT, BLACK);
            let k = ACCENT.rgb565();
            s.rect(cx - w / 2, cy - r / 2, w, r * 2 / 3, k);
            s.rect(cx - w / 2, cy + r / 3, w, w, k);
        }
        _ => {}
    }
    let m = w + 6;
    g.draw(s, cx - r - m, cy - r - m, cx + r + m, cy + r + m, c.rgb565(), 0);
}

/// The marker: a light disc with the arrow cut out of it in dark, the arrow turned to `angle` (a turn, clockwise
/// from up). No fix: a dimmer disc. `soft`: smooth edges; a moving frame (`soft` false) draws it hard-edged, a
/// third of the rects.
fn car(s: &mut impl Surface, fix: bool, angle: u16, soft: bool) {
    let (x, y) = CAR;
    let (sn, cs) = (sin(angle) as i64, cos(angle) as i64);
    // The disc is drawn on whole pixels, centred on the middle of pixel (x, y). The arrow is built in half pixels
    // round that same point (2x+1, 2y+1), so it is turned about its own middle and sits in the disc's middle.
    let rot = |dx: i32, dy: i32| {
        let rx = (dx as i64 * cs - dy as i64 * sn) >> 14;
        let ry = (dx as i64 * sn + dy as i64 * cs) >> 14;
        (2 * x + 1 + rx as i32, 2 * y + 1 + ry as i32)
    };
    let (face, ink) = if fix { (Rgb(214, 218, 228), Rgb(30, 38, 54)) } else { (Rgb(120, 124, 134), Rgb(52, 58, 72)) };
    let arrow = [rot(0, -31), rot(26, 31), rot(0, 15), rot(-26, 31)]; // 26 px wide, 31 tall
    if soft {
        disc(s, x, y, CAR_R, face, MAP_BG);
        poly_half(s, &arrow, face.rgb565(), |_| ink.rgb565());
    } else {
        for dy in -CAR_R..=CAR_R {
            let hw = cmf_wp2_ui::motion::isqrt(CAR_R * CAR_R - dy * dy);
            s.rect(x - hw, y + dy, 2 * hw + 1, 1, face.rgb565());
        }
        fill_hard(s, &arrow, ink.rgb565());
    }
}

/// A hard-edged filled polygon (even-odd), corners in half pixels: a pixel is filled when its middle is inside.
/// A rect for every run of every row.
fn fill_hard(s: &mut impl Surface, p: &[(i32, i32)], c: u16) {
    let y0 = p.iter().map(|q| q.1).min().unwrap_or(0).div_euclid(2);
    let y1 = (p.iter().map(|q| q.1).max().unwrap_or(0) + 1).div_euclid(2);
    for y in y0..y1 {
        let yc = 2 * y + 1; // the middle of the row
        let mut xs = [0i32; 8];
        let mut n = 0;
        for k in 0..p.len() {
            let (a, b) = (p[k], p[(k + 1) % p.len()]);
            if (a.1 <= yc && b.1 > yc || b.1 <= yc && a.1 > yc) && n < xs.len() {
                xs[n] = a.0 + (b.0 - a.0) * (yc - a.1) / (b.1 - a.1);
                n += 1;
            }
        }
        xs[..n].sort_unstable();
        for pair in xs[..n].chunks_exact(2) {
            // pixel px has its middle at 2px+1: filled from pair[0].div_euclid(2) up to pair[1].div_euclid(2)
            let (l, r) = (pair[0].div_euclid(2), pair[1].div_euclid(2));
            if r > l {
                s.rect(l, y, r - l, 1, c);
            }
        }
    }
}

/// A surface that only takes what falls inside a box: the arrow's patch of the map is repainted through it.
struct Clip<'a, S: Surface> {
    s: &'a mut S,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

impl<S: Surface> Surface for Clip<'_, S> {
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u16) {
        let (xa, ya) = (x.max(self.x0), y.max(self.y0));
        let (xb, yb) = ((x + w).min(self.x1), (y + h).min(self.y1));
        if xb > xa && yb > ya {
            self.s.rect(xa, ya, xb - xa, yb - ya, c);
        }
    }

    fn text(&mut self, _x: i32, _y: i32, _s: &str, _c: u16) {}
}

/// How wide a text line with its baseline at `y` may be, from what the circle leaves there.
fn room(f: &crate::font::Font, y: i32, margin: i32) -> i32 {
    let (x0, x1) = geom::chord(y - f.cap, y + f.descent / 2, margin);
    x1 - x0
}

/// A line of small text centred at baseline `y`; a road name in a script the font lacks falls back to the
/// loader's font (ASCII and Cyrillic).
fn small(s: &mut impl Surface, y: i32, t: &str, c: Rgb) {
    if S.covers(t) {
        S.centre(s, C, y, t, room(&S, y, 14), c.rgb565(), 0);
    } else {
        let (_, cells) = geom::text_box(y - S.cap, 14);
        let n = cmf_wp2_ui::text::chars(t).min(cells);
        fitted(s, C - n as i32 * CHAR / 2, y - S.cap, t, n, c.rgb565());
    }
}

fn message(s: &mut impl Surface, title: &str, sub: &str) {
    s.rect(0, 0, SIZE, SIZE, 0);
    // the big font has no hyphen: a title like "V-maps" is drawn in its parts with a small bar between them
    const BAR: i32 = 16;
    let parts = title.split('-');
    let width: i32 = parts.clone().map(|p| L.width(p)).sum::<i32>() + BAR * (title.matches('-').count() as i32);
    let mut x = C - width / 2;
    for (i, p) in parts.enumerate() {
        if i > 0 {
            s.rect(x + 3, C - 4 - L.cap / 3, BAR - 6, 4, WHITE.rgb565());
            x += BAR;
        }
        L.draw(s, x, C - 4, p, WHITE.rgb565(), 0);
        x += L.width(p);
    }
    small(s, C + 38, sub, GREY);
}

/// A button on the menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Choice {
    Home,
    Work,
}

const BTN_W: i32 = 300;
const BTN_H: i32 = 91;
const BTN_X: i32 = C - BTN_W / 2;
/// Top edges of the Home and the Work button.
const BTN_Y: [i32; 2] = [150, 258];
const BTN_FACE: Rgb = Rgb(38, 46, 64);
const BTN_DOWN: Rgb = Rgb(48, 104, 200);
/// The slack round a button that still counts as on it.
const BTN_SLACK: i32 = 8;

/// The button under a touch.
pub fn button_at(x: i32, y: i32) -> Option<Choice> {
    [Choice::Home, Choice::Work].into_iter().zip(BTN_Y).find_map(|(c, top)| {
        (x >= BTN_X - BTN_SLACK && x < BTN_X + BTN_W + BTN_SLACK && y >= top - BTN_SLACK && y < top + BTN_H + BTN_SLACK)
            .then_some(c)
    })
}

/// The menu: a line of text above two big rounded buttons, Home and Work, and a hint under them. `down` is the
/// button a finger is on.
pub fn menu(s: &mut impl Surface, title: &str, hint: &str, down: Option<Choice>) {
    s.rect(0, 0, SIZE, SIZE, 0);
    small(s, 120, title, WHITE);
    for (c, top) in [Choice::Home, Choice::Work].into_iter().zip(BTN_Y) {
        let face = if down == Some(c) { BTN_DOWN } else { BTN_FACE };
        // a bar with a half disc at each end
        let r = (BTN_H - 1) / 2;
        disc(s, BTN_X + r, top + r, r, face, BLACK);
        disc(s, BTN_X + BTN_W - 1 - r, top + r, r, face, BLACK);
        s.rect(BTN_X + r, top, BTN_W - 2 * r, BTN_H, face.rgb565());
        // the sign and the word, side by side, in the middle of the button
        let label = if c == Choice::Home { "Home" } else { "Work" };
        let width = ICON_W + ICON_GAP + L.width(label);
        let (x0, cy) = (C - width / 2, top + r);
        let (ix, f) = (x0 + ICON_W / 2, face.rgb565());
        let white = WHITE.rgb565();
        if c == Choice::Home {
            let roof = [(0, -19), (21, 0), (13, 0), (13, 18), (-13, 18), (-13, 0), (-21, 0)];
            let pts = roof.map(|(dx, dy)| (ix + dx, cy + dy));
            poly(s, &pts, f, |_| white);
            s.rect(ix - 4, cy + 6, 8, 12, f);
        } else {
            s.rect(ix - 20, cy - 8, 40, 28, white);
            s.rect(ix - 9, cy - 18, 18, 11, white);
            s.rect(ix - 5, cy - 14, 10, 7, f);
            s.rect(ix - 20, cy + 3, 40, 3, f);
        }
        L.draw(s, x0 + ICON_W + ICON_GAP, cy + L.cap / 2, label, white, f);
    }
    if !hint.is_empty() {
        small(s, 412, hint, GREY);
    }
}

const ICON_W: i32 = 44;
const ICON_GAP: i32 = 18;

/// After a button was pressed, until the phone's route arrives.
pub fn going(s: &mut impl Surface, c: Choice) {
    message(s, if c == Choice::Home { "Home" } else { "Work" }, "Getting the route…");
}

/// `new` as a heading, but at most `max` away from `from` (going the short way round).
pub fn limit_turn(from: u16, new: u16, max: i32) -> u16 {
    let d = new.wrapping_sub(from) as i16 as i32;
    from.wrapping_add(d.clamp(-max, max) as i16 as u16)
}

/// Interpolate a heading along the shortest clockwise/counter-clockwise path, including across north/zero.
#[inline]
pub fn interpolate_heading(from: u16, to: u16, elapsed_ms: u32, duration_ms: u32) -> u16 {
    if duration_ms == 0 || elapsed_ms >= duration_ms {
        return to;
    }
    let delta = to.wrapping_sub(from) as i16 as i32;
    from.wrapping_add((delta * elapsed_ms as i32 / duration_ms as i32) as i16 as u16)
}

/// Linear interpolation for the car position, independent of the heading animation clock.
#[inline]
pub fn interpolate_position(from: (i32, i32), to: (i32, i32), elapsed_ms: u32, duration_ms: u32) -> (i32, i32) {
    if duration_ms == 0 || elapsed_ms >= duration_ms {
        return to;
    }
    let t = elapsed_ms as i64;
    let d = duration_ms as i64;
    (
        (from.0 as i64 + (to.0 as i64 - from.0 as i64) * t / d) as i32,
        (from.1 as i64 + (to.1 as i64 - from.1 as i64) * t / d) as i32,
    )
}

/// The turn bubble: the bottom of the round screen below a shallow arc (a dome, highest in the middle), all black.
/// The map is never drawn into it (see `Cut`), so only a full frame paints it and a moving map costs nothing for it.
const BUB_TOP: i32 = 318;
const BUB_RD: i32 = 300; // the dome is an arc of a circle of this radius
const BUB_CY: i32 = BUB_TOP + BUB_RD; // its centre, below the screen

/// The bubble's left and right edge on row `y`, or `None` where it has no width.
fn bubble_span(y: i32) -> Option<(i32, i32)> {
    if !(BUB_TOP..SIZE).contains(&y) {
        return None;
    }
    let d = BUB_CY - y;
    let w = cmf_wp2_ui::motion::isqrt(BUB_RD * BUB_RD - d * d).min(geom::half(y));
    (w > 0).then_some((C - w, C + w))
}

/// A surface that leaves the bubble alone: whatever falls inside it is not drawn.
struct Cut<'a, S: Surface> {
    s: &'a mut S,
}

impl<S: Surface> Surface for Cut<'_, S> {
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u16) {
        if y + h <= BUB_TOP {
            return self.s.rect(x, y, w, h, c);
        }
        let end = y + h;
        let mut r = y;
        while r < end {
            let span = bubble_span(r);
            // the rows from `r` that have the same edges
            let mut e = r + 1;
            if r < BUB_TOP {
                e = end.min(BUB_TOP);
            } else {
                while e < end && bubble_span(e) == span {
                    e += 1;
                }
            }
            match span {
                None => self.s.rect(x, r, w, e - r, c),
                Some((a, b)) => {
                    if a > x {
                        self.s.rect(x, r, (a - x).min(w), e - r, c);
                    }
                    if b < x + w {
                        let l = b.max(x);
                        self.s.rect(l, r, x + w - l, e - r, c);
                    }
                }
            }
            r = e;
        }
    }

    fn text(&mut self, _x: i32, _y: i32, _s: &str, _c: u16) {}
}

/// Clears the map's background except the bubble: the part above it in one rect, beside it a row at a time (only
/// as far as the round screen goes, so about 130 rects).
fn clear_round_bubble(s: &mut impl Surface) {
    let bg = MAP_BG.rgb565();
    s.rect(0, TOP, SIZE, BUB_TOP - TOP, bg);
    for r in BUB_TOP..SIZE {
        // only what the round screen has: beside the dome, and where the dome is wider than the screen, nothing
        let h = geom::half(r);
        match bubble_span(r) {
            Some((a, b)) => {
                if a > C - h {
                    s.rect(C - h, r, a - (C - h), 1, bg);
                }
                if b < C + h {
                    s.rect(b, r, C + h - b, 1, bg);
                }
            }
            None => s.rect(C - h, r, 2 * h, 1, bg),
        }
    }
}

/// The bubble: black, with the next turn's sign and the distance to it.
fn turn(s: &mut impl Surface, m: &Model) {
    for y in BUB_TOP..SIZE {
        if let Some((a, b)) = bubble_span(y) {
            s.rect(a, y, b - a, 1, 0);
        }
    }
    let st = &m.state;
    if st.maneuver == man::OFF_ROUTE {
        let y = 402;
        icon(s, man::OFF_ROUTE, C - 100, y - 14, 28, ACCENT);
        L.draw(s, C - 56, y, "Off route", ACCENT.rgb565(), 0);
        S.draw(s, C - 56, y + 28, "Rerouting…", SHADE.rgb565(), 0);
        return;
    }
    let d = distance(st.dist_m as u32, st.imperial());
    let (num, unit) = d.as_str().split_once(' ').unwrap_or((d.as_str(), ""));
    let (iw, gap, ugap) = (60, 14, 8);
    let w = iw + gap + XL.width(num) + ugap + L.width(unit);
    let x = C - w / 2;
    let y = 408; // baseline of the distance
    icon(s, st.maneuver, x + iw / 2, y - XL.cap / 2, 28, WHITE);
    let after = XL.draw(s, x + iw + gap, y, num, WHITE.rgb565(), 0);
    L.draw(s, after + ugap, y, unit, SHADE.rgb565(), 0);
    // the time left, light grey, under it (small when the round screen leaves too little room for the big one)
    let left = duration(st.remaining_s as u32);
    let t = left.as_str();
    let (f, ty) = if L.width(t) <= room(&L, 450, 8) { (&L, 450) } else { (&S, 446) };
    f.draw(s, C - f.width(t) / 2, ty, t, SHADE.rgb565(), 0);
}

/// The whole screen.
pub fn draw(s: &mut impl Surface, m: &Model, ctx: &Ctx) {
    let st = &m.state;
    if !m.has_state || !ctx.live {
        return message(s, "V-maps", if m.has_state { "Phone out of reach" } else { "Waiting for Vela…" });
    }
    if !st.active() {
        return message(s, "V-maps", "Start a route in Vela");
    }
    if st.arrived() {
        s.rect(0, 0, SIZE, SIZE, 0);
        icon(s, man::ARRIVE, C, 168, 46, RED);
        L.draw(s, C - L.width("Arrived") / 2, 268, "Arrived", WHITE.rgb565(), 0);
        small(s, 308, m.text(), GREY);
        return;
    }
    if ctx.dome {
        // only the text changed: the dome paints itself black first, and the map stays as it is
        return turn(s, m);
    }
    let mpp = ZOOMS[ctx.zoom.min(ZOOMS.len() - 1)];
    let angle = ctx.look.unwrap_or(0);
    if ctx.arrow_only {
        // the map has not moved: repaint the patch round the arrow and draw the arrow on it
        let (x, y) = CAR;
        let mut c = Clip { s, x0: x - 34, y0: y - 36, x1: x + 34, y1: y + 32 };
        c.rect(0, 0, SIZE, SIZE, MAP_BG.rgb565());
        layers(&mut c, m, mpp, false);
        car(&mut c, st.fix(), angle, true);
        return;
    }
    if ctx.light {
        // the whole screen in the cheap style, except the bubble: it stays as the last full frame drew it
        {
            clear_round_bubble(s);
            layers(&mut Cut { s: &mut *s }, m, mpp, true);
        }
        car(s, st.fix(), angle, false);
        return;
    }
    {
        clear_round_bubble(s);
        layers(&mut Cut { s: &mut *s }, m, mpp, ctx.fast);
    }
    car(s, st.fix(), angle, true);
    turn(s, m);
}

/// The map: nearby streets, the road behind, the route ahead and where it ends.
fn layers(s: &mut impl Surface, m: &Model, mpp: i32, fast: bool) {
    let st = &m.state;
    // Reuse the same heading transform for every road, route dot and destination point in this frame.
    let projector = Projector::new([st.x, st.y], st.course, mpp);
    streets_projected(s, m, &projector, fast);
    if m.route_len >= 2 {
        let p = (st.progress as usize).clamp(1, m.route_len);
        dots_projected(s, m, p - 1, m.route_len, 7, ROUTE_STEP, BLUE, &projector, fast);
        let end = projector.project(m.route[m.route_len - 1]);
        if geom::inside(end.0, end.1) && end.1 > TOP + 12 && end.1 < BOTTOM - 12 {
            disc(s, end.0, end.1, 13, BLACK, WHITE);
            disc(s, end.0, end.1, 9, RED, BLACK);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::enc::{route, state};
    use crate::proto::{F_ACTIVE, F_FIX, State};
    use cmf_wp2_ui::Fb;
    use std::boxed::Box;
    use std::vec;
    use std::vec::Vec;

    #[test]
    fn formats() {
        assert_eq!(distance(347, false).as_str(), "340 m");
        assert_eq!(distance(42, false).as_str(), "40 m");
        assert_eq!(distance(1249, false).as_str(), "1.2 km");
        assert_eq!(distance(9960, false).as_str(), "10.0 km");
        assert_eq!(distance(23_600, false).as_str(), "24 km");
        assert_eq!(distance(100, true).as_str(), "350 ft");
        assert_eq!(distance(500, true).as_str(), "0.3 mi");
        assert_eq!(distance(40_000, true).as_str(), "25 mi");
        assert_eq!(duration(840).as_str(), "14 min");
        assert_eq!(duration(3900).as_str(), "1 h 05");
    }

    #[test]
    fn projector_reuses_one_frame_transform_without_changing_projection() {
        for course in [0u16, 1, 8192, 16384, 24576, 32768, 49152, 65535] {
            let car = [12_345, -67_890];
            let projector = Projector::new(car, course, 1);
            let (sn, cs) = (sin(course) as i64, cos(course) as i64);
            for p in [[12_345, -67_790], [12_245, -67_890], [12_400, -67_940], [50_000, 20_000]] {
                let (dx, dy) = (p[0] as i64 - car[0] as i64, p[1] as i64 - car[1] as i64);
                let d = Q14 as i64;
                let right = (dx * cs - dy * sn) / d;
                let ahead = (dx * sn + dy * cs) / d;
                let lim = 1i64 << 20;
                let expected = (
                    (CAR.0 as i64 + right.clamp(-lim, lim)) as i32,
                    (CAR.1 as i64 - ahead.clamp(-lim, lim)) as i32,
                );
                assert_eq!(projector.project(p), expected, "course {course}, point {p:?}");
            }
        }
    }

    #[test]
    fn heading_interpolation_takes_short_path_through_north() {
        let halfway = interpolate_heading(65_000, 500, 100, 200);
        assert!(halfway < 65_535 && halfway > 65_000);
        assert_eq!(interpolate_heading(65_000, 500, 200, 200), 500);
        assert_eq!(interpolate_heading(500, 65_000, 100, 200), 65_518);
    }

    #[test]
    fn retargeting_heading_starts_from_the_currently_displayed_angle() {
        let displayed = interpolate_heading(1000, 5000, 120, 240);
        // A new phone sample retargets from the value on screen, so t=0 cannot jump.
        assert_eq!(interpolate_heading(displayed, 9000, 0, 240), displayed);
        assert_eq!(interpolate_heading(displayed, 9000, 240, 240), 9000);
    }

    #[test]
    fn position_and_heading_interpolations_finish_independently() {
        assert_eq!(interpolate_position((0, 0), (100, 200), 100, 200), (50, 100));
        assert_eq!(interpolate_heading(0, 16_384, 100, 200), 8_192);
        assert_eq!(interpolate_heading(0, 16_384, 200, 200), 16_384);
    }

    #[test]
    fn heading_up() {
        // heading east (a quarter turn): a point 100 m east is straight ahead, one 100 m south is to the right
        assert_eq!(project([100, 0], [0, 0], 0x4000, 1), (CAR.0, CAR.1 - 100));
        assert_eq!(project([0, -100], [0, 0], 0x4000, 1), (CAR.0 + 100, CAR.1));
        assert_eq!(project([0, 100], [0, 0], 0, 2), (CAR.0, CAR.1 - 50));
        // far away does not overflow
        let far = project([2_000_000_000, -2_000_000_000], [-2_000_000_000, 2_000_000_000], 0x2000, 1);
        assert!(far.0 > SIZE);
    }

    #[test]
    fn clipping() {
        assert_eq!(clip((10, 10), (20, 20), 0, 100), Some(((10, 10), (20, 20))));
        assert_eq!(clip((-50, -50), (-10, -10), 0, 100), None);
        assert_eq!(clip((-100, 50), (200, 50), 0, 100), Some(((0, 50), (100, 50))));
    }

    fn model() -> Box<Model> {
        let mut m = Box::new(Model::new());
        let pts: vec::Vec<[i32; 2]> =
            (0..60).map(|i| if i < 30 { [0, i * 20] } else { [(i - 29) * 20, 580] }).collect();
        let (msgs, seq) = route(0, 1, &pts);
        msgs.iter().for_each(|x| {
            m.apply(x);
        });
        let st = State {
            flags: F_ACTIVE | F_FIX,
            maneuver: man::RIGHT,
            dist_m: 180,
            remaining_10m: 120,
            remaining_s: 300,
            limit: 50,
            x: 0,
            y: 400,
            progress: 21,
            ..State::default()
        };
        m.apply(&state(seq, &st));
        m.apply(&crate::proto::enc::msg(b'T', seq.wrapping_add(1), b"Bannerghatta Main Road"));
        m
    }

    #[test]
    fn the_arrow_turns_and_only_its_patch_is_repainted() {
        let m = model();
        let mut px = vec![0u16; (SIZE * SIZE) as usize];
        let mut fb = Fb::new(&mut px);
        let ctx = |look: Option<u16>, arrow_only| Ctx { live: true, zoom: ZOOM_DEFAULT, fast: false, look, arrow_only, light: false, dome: false };
        // looking east (a quarter turn): the tip points right of the car, the wings trail to the left
        draw(&mut fb, &m, &ctx(Some(16384), false));
        let ink = Rgb(30, 38, 54).rgb565();
        assert_eq!(fb.at(CAR.0 + 8, CAR.1), ink, "tip at the right");
        assert_ne!(fb.at(CAR.0, CAR.1 - 8), ink, "nothing up any more");
        // a heading change alone: pixels outside the patch are left as they were, the arrow follows the new heading
        let mut px2 = vec![0x1234u16; (SIZE * SIZE) as usize];
        let mut fb2 = Fb::new(&mut px2);
        draw(&mut fb2, &m, &ctx(Some(0), true));
        assert_eq!(fb2.at(10, 10), 0x1234);
        assert_eq!(fb2.at(CAR.0 + 60, CAR.1), 0x1234);
        assert_eq!(fb2.at(CAR.0, CAR.1 - 8), Rgb(30, 38, 54).rgb565()); // the arrow up, on the disc
        assert_eq!(fb2.at(CAR.0 - 22, CAR.1), Rgb(214, 218, 228).rgb565());
        assert!(fb2.rects < 1500, "{}", fb2.rects); // a full frame is ~15x that
    }

    #[test]
    fn a_light_frame_repaints_the_screen_but_the_bubble() {
        let m = rich();
        let mut px = vec![0x1234u16; (SIZE * SIZE) as usize];
        let mut fb = Fb::new(&mut px);
        draw(&mut fb, &m, &Ctx { live: true, zoom: ZOOM_DEFAULT, fast: true, look: None, arrow_only: false, light: true, dome: false });
        assert_eq!(fb.at(C, C + 218), 0x1234); // the bubble is left as it was
        assert_ne!(fb.at(C, 100), 0x1234);
        assert_ne!(fb.at(C, 20), 0x1234);
        assert_eq!(fb.at(CAR.0, CAR.1 - 8), Rgb(30, 38, 54).rgb565());
        assert_eq!(fb.at(CAR.0 - 22, CAR.1), Rgb(214, 218, 228).rgb565());
        assert!(fb.rects < 7000, "{}", fb.rects);
    }

    #[test]
    fn draws_a_drive() {
        let m = model();
        let mut px = vec![0u16; (SIZE * SIZE) as usize];
        let mut fb = Fb::new(&mut px);
        draw(&mut fb, &m, &Ctx { live: true, zoom: ZOOM_DEFAULT, fast: false, look: None, arrow_only: false, light: false, dome: false });
        // the route ahead passes above the car, the car is drawn, the turn goes right of the centre
        assert!((CAR.1 - 90..CAR.1 - 35).any(|y| fb.at(CAR.0, y) == BLUE.rgb565()));
        // the car: a light disc with a dark arrow in it
        assert_eq!(fb.at(CAR.0, CAR.1 - 8), Rgb(30, 38, 54).rgb565());
        assert_eq!(fb.at(CAR.0 - 22, CAR.1), Rgb(214, 218, 228).rgb565());
        // no progress arc at the bottom
        assert_eq!(fb.at(C, C + 218), 0);
        assert!(fb.rects < 20_000, "{}", fb.rects);
        // nothing lit outside the circle
        for y in 0..SIZE {
            for x in 0..SIZE {
                // outside the round screen only the background's own colour (its clear is a square)
                if !geom::inside(x, y) {
                    let p = fb.at(x, y);
                    assert!(p == 0 || p == MAP_BG.rgb565(), "lit at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn a_turn_is_limited_per_frame_the_short_way() {
        assert_eq!(limit_turn(100, 500, 4096), 500);
        assert_eq!(limit_turn(0, 20000, 4096), 4096);
        assert_eq!(limit_turn(0, 0u16.wrapping_sub(20000), 4096), 0u16.wrapping_sub(4096));
        // across north: 65000 -> 2000 is a short turn right, not a long one left
        assert_eq!(limit_turn(65000, 2000, 4096), 2000);
        assert_eq!(limit_turn(60000, 8000, 4096), 60000u16.wrapping_add(4096));
    }

    /// The steep branch as it was before equal rows were joined: one rect a row.
    fn steep_rows(s: &mut impl Surface, a: (i32, i32), b: (i32, i32), w: i32, c: Rgb) {
        let k = c.rgb565();
        let (dx, dy) = ((b.0 - a.0) as i64, (b.1 - a.1) as i64);
        let len = cmf_wp2_ui::motion::isqrt((dx * dx + dy * dy).min(i32::MAX as i64) as i32) as i64;
        let th = ((w as i64 * len + dy.abs() / 2) / dy.abs()).max(w as i64) as i32;
        for y in a.1.min(b.1).max(0)..=a.1.max(b.1).min(SIZE - 1) {
            let xc = a.0 as i64 + dx * (y - a.1) as i64 / dy;
            let h = geom::half(y);
            let (x0, x1) = ((xc as i32 - th / 2).max(C - h), (xc as i32 - th / 2 + th).min(C + h));
            if x1 > x0 {
                s.rect(x0, y, x1 - x0, 1, k);
            }
        }
    }

    #[test]
    fn joined_steep_rows_draw_the_same_pixels() {
        let ends = [(C, -40), (C + 3, SIZE + 40), (60, 20), (400, 450), (233, 233), (120, 440), (300, 10), (30, 200), (C + 90, 100)];
        let mut n = 0;
        for &a in &ends {
            for &b in &ends {
                if (b.1 - a.1).abs() < (b.0 - a.0).abs() || a == b {
                    continue; // steep ones only
                }
                for w in [3, 8, 16] {
                    let mut p1 = vec![0u16; (SIZE * SIZE) as usize];
                    let mut p2 = p1.clone();
                    let (mut f1, mut f2) = (Fb::new(&mut p1), Fb::new(&mut p2));
                    fast_line(&mut f1, a, b, w, WHITE);
                    steep_rows(&mut f2, a, b, w, WHITE);
                    let (r1, r2) = (f1.rects, f2.rects);
                    assert!(p1 == p2, "{a:?} {b:?} {w}");
                    assert!(r1 <= r2);
                    n += 1;
                }
            }
        }
        assert!(n > 30);
        // a vertical street is one rect
        let mut px = vec![0u16; (SIZE * SIZE) as usize];
        let mut fb = Fb::new(&mut px);
        fast_line(&mut fb, (C, -10), (C, SIZE + 10), 8, WHITE);
        assert_eq!(fb.rects, 1);
    }

    #[test]
    fn the_route_is_dots_that_stay_put_on_the_map() {
        let mut m = model();
        let snap = |m: &Model, fast: bool| {
            let mut px = vec![0u16; (SIZE * SIZE) as usize];
            let mut fb = Fb::new(&mut px);
            layers(&mut fb, m, 1, fast);
            px
        };
        // the dots are separate: going up the column of the car there are gaps between blue runs
        for fast in [false, true] {
            let px = snap(&m, fast);
            let col: Vec<bool> = (CAR.1 - 150..CAR.1 - 35).map(|y| px[(y * SIZE + CAR.0) as usize] == BLUE.rgb565()).collect();
            let runs = col.windows(2).filter(|w| !w[0] && w[1]).count();
            assert!(runs >= 3, "{fast} {runs}");
            assert!(col.iter().any(|b| !b) && col.iter().any(|b| *b));
        }
        // 24 m on, the dots are exactly where they were (the pattern repeats every 24 px along the route)
        let a = snap(&m, true);
        m.state.y += 24;
        let b = snap(&m, true);
        let blue = |px: &Vec<u16>, y: i32| (CAR.0 - 8..CAR.0 + 8).filter(|x| px[(y * SIZE + x) as usize] == BLUE.rgb565()).count();
        for y in CAR.1 - 150..CAR.1 - 80 {
            assert_eq!(blue(&a, y), blue(&b, y + 24), "row {y}");
        }
    }

    #[test]
    fn the_arrow_is_in_the_middle_of_the_disc() {
        let m = model();
        let mut px = vec![0u16; (SIZE * SIZE) as usize];
        let mut fb = Fb::new(&mut px);
        car(&mut fb, true, 0, true);
        let ink = Rgb(30, 38, 54).rgb565();
        let (mut x0, mut x1, mut y0, mut y1) = (SIZE, 0, SIZE, 0);
        for y in CAR.1 - CAR_R..=CAR.1 + CAR_R {
            for x in CAR.0 - CAR_R..=CAR.0 + CAR_R {
                if fb.at(x, y) != ink {
                    continue;
                }
                (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
            }
        }
        // the disc spans CAR.1 - CAR_R ..= CAR.1 + CAR_R: the arrow leaves as much room above as below
        let (above, below) = (y0 - (CAR.1 - CAR_R), (CAR.1 + CAR_R) - y1);
        assert!((above - below).abs() <= 1, "{above} {below}");
        let (left, right) = (x0 - (CAR.0 - CAR_R), (CAR.0 + CAR_R) - x1);
        assert!((left - right).abs() <= 1, "{left} {right}");
        let _ = m;
    }

    #[test]
    fn every_icon_stays_in_its_box() {
        for code in 0..=13 {
            let mut px = vec![0u16; (SIZE * SIZE) as usize];
            let mut fb = Fb::new(&mut px);
            icon(&mut fb, code, C, C, 32, WHITE);
            for y in 0..SIZE {
                for x in 0..SIZE {
                    if fb.at(x, y) != 0 {
                        assert!((x - C).abs() <= 40 && (y - C).abs() <= 40, "icon {code} at {x},{y}");
                    }
                }
            }
        }
    }

    fn with_streets(mut m: Box<Model>) -> Box<Model> {
        let lines = vec![
            vec![[-300i16, 120], [-200, 120], [-100, 120], [0, 120], [100, 120], [200, 120], [300, 120]],
            vec![[60i16, 100], [90, 150], [150, 200], [220, 230]],
            vec![[-80i16, 0], [-80, 100], [-80, 200], [-80, 300]],
        ];
        let (msgs, _) = crate::proto::enc::roads(10, 4, [0, 400], &lines);
        msgs.iter().for_each(|x| {
            m.apply(x);
        });
        m
    }

    #[test]
    fn nearby_streets_are_grey_and_under_the_route() {
        let m = with_streets(model());
        let mut px = vec![0u16; (SIZE * SIZE) as usize];
        let mut fb = Fb::new(&mut px);
        draw(&mut fb, &m, &Ctx { live: true, zoom: ZOOM_DEFAULT, fast: false, look: None, arrow_only: false, light: false, dome: false });
        // the street left of the road, 80 m west at 3 m/px: 27 px left of the car's column, going up
        assert!((CAR.0 - 31..CAR.0 - 22).any(|x| fb.at(x, CAR.1 - 60) == STREET.rgb565()));
        // the cross street 120 m ahead of (0, 400): y 520 = 40 px above the car
        assert!((CAR.1 - 44..CAR.1 - 35).any(|y| fb.at(CAR.0 - 90, y) == STREET.rgb565() || fb.at(CAR.0 - 90, y) == MAJOR.rgb565()));
        // the route stays on top where they cross
        assert!((CAR.1 - 90..CAR.1 - 35).any(|y| fb.at(CAR.0, y) == BLUE.rgb565()));
        assert!(fb.rects < 25_000, "{}", fb.rects);
    }

    /// A winding route with a grid of streets around the car, for pictures.
    fn rich() -> Box<Model> {
        let mut m = Box::new(Model::new());
        let mut pts: vec::Vec<[i32; 2]> = vec![];
        for i in 0..=20 { pts.push([0, i * 20]); } // north 400 m
        for i in 1..=15 { pts.push([i * 20, 400]); } // east 300 m
        for i in 1..=20 { pts.push([300, 400 + i * 20]); } // north 400 m
        for i in 1..=10 { pts.push([300 - i * 20, 800]); } // west 200 m
        let (msgs, seq) = route(0, 1, &pts);
        msgs.iter().for_each(|x| {
            m.apply(x);
        });
        let st = State {
            flags: F_ACTIVE | F_FIX,
            maneuver: man::RIGHT,
            after: man::LEFT,
            after_m: 240,
            dist_m: 180,
            remaining_10m: 120,
            remaining_s: 300,
            x: 0,
            y: 220,
            progress: 12,
            ..State::default()
        };
        m.apply(&state(seq, &st));
        let mut lines: vec::Vec<vec::Vec<[i16; 2]>> = vec![];
        for k in -2..=5i16 {
            lines.push(vec![[-740, k * 160 + 30], [-370, k * 160 + 8], [0, k * 160], [370, k * 160 - 12], [740, k * 160 - 20]]);
        }
        for k in -4..=4i16 {
            lines.push(vec![[k * 190 + 20, -400], [k * 190 + 10, -150], [k * 190, 150], [k * 190 - 10, 400]]);
        }
        lines.truncate(ROADS_LINES_MAX);
        let (msgs, _) = crate::proto::enc::roads(seq.wrapping_add(1), 4, [0, 400], &lines);
        msgs.iter().for_each(|x| {
            m.apply(x);
        });
        m.apply(&crate::proto::enc::msg(b'T', seq.wrapping_add(40), b"Bannerghatta Main Road"));
        m
    }

    /// `PREVIEW=/path cargo test dump_preview`: raw RGB565 frames of the screen, for pictures.
    #[test]
    fn dump_preview() {
        let Ok(dir) = std::env::var("PREVIEW") else { return };
        let mut m = rich();
        let mut shot = |name: &str, m: &Model, zoom: usize| {
            let mut px = vec![0u16; (SIZE * SIZE) as usize];
            let mut fb = Fb::new(&mut px);
            draw(&mut fb, m, &Ctx { live: true, zoom, fast: false, look: None, arrow_only: false, light: false, dome: false });
            let bytes: Vec<u8> = px.iter().flat_map(|v| v.to_le_bytes()).collect();
            std::fs::write(format!("{dir}/{name}.raw"), bytes).unwrap();
        };
        shot("drive", &m, ZOOM_DEFAULT);
        shot("far", &m, 4);
        shot("near", &m, 1);
        m.state.maneuver = man::OFF_ROUTE;
        shot("off", &m, ZOOM_DEFAULT);
        let mut px = vec![0u16; (SIZE * SIZE) as usize];
        let mut fb = Fb::new(&mut px);
        draw(&mut fb, &m, &Ctx { live: true, zoom: ZOOM_DEFAULT, fast: true, look: None, arrow_only: false, light: false, dome: false });
        std::fs::write(format!("{dir}/fast.raw"), px.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
    }

    /// Winding streets, a vertex every ~15 m, for the cost of rounding bends.
    fn curvy() -> Box<Model> {
        let mut m = rich();
        let mut lines: vec::Vec<vec::Vec<[i16; 2]>> = vec![];
        for k in 0..12i32 {
            let mut l = vec![];
            for i in 0..24i32 {
                let t = i as f32 * 0.9; // a bend every 29 m, 40 to 60 degrees
                let x = -330 + i * 29;
                let y = k * 70 - 330 + (45.0 * (t + k as f32).sin()) as i32;
                l.push([x as i16, y as i16]);
            }
            lines.push(l);
        }
        let (msgs, _) = crate::proto::enc::roads(60, 9, [0, 220], &lines);
        msgs.iter().for_each(|x| {
            m.apply(x);
        });
        m
    }

    #[test]
    fn rounded_bends_in_a_moving_frame_stay_cheap() {
        let m = curvy();
        let mut px = vec![0u16; (SIZE * SIZE) as usize];
        let mut fb = Fb::new(&mut px);
        draw(&mut fb, &m, &Ctx { live: true, zoom: ZOOM_DEFAULT, fast: true, look: None, arrow_only: false, light: true, dome: false });
        eprintln!("curvy moving frame: {} rects", fb.rects);
        assert!(fb.rects < 2700, "{}", fb.rects);
    }

    #[test]
    fn the_bubble_stays_and_a_moving_frame_leaves_it_alone() {
        let m = rich();
        let mut full = vec![0u16; (SIZE * SIZE) as usize];
        draw(&mut Fb::new(&mut full), &m, &Ctx { live: true, zoom: ZOOM_DEFAULT, fast: false, look: None, arrow_only: false, light: false, dome: false });
        // the bubble is black with the sign and the distance on it
        assert!((BUB_TOP..SIZE).any(|y| bubble_span(y).is_some_and(|(a, b)| (a..b).any(|x| full[(y * SIZE + x) as usize] != 0))));
        // a light frame drawn over it leaves every pixel of the bubble as it was, and costs fewer rects
        let mut px = full.clone();
        let mut fb = Fb::new(&mut px);
        draw(&mut fb, &m, &Ctx { live: true, zoom: ZOOM_DEFAULT, fast: true, look: None, arrow_only: false, light: true, dome: false });
        assert!(fb.rects < 3300, "{}", fb.rects);
        for y in BUB_TOP..SIZE {
            let Some((a, b)) = bubble_span(y) else { continue };
            for x in a..b {
                assert_eq!(px[(y * SIZE + x) as usize], full[(y * SIZE + x) as usize], "bubble changed at {x},{y}");
            }
        }
    }

    #[test]
    fn fast_streets_keep_every_street_and_the_layering() {
        let m = with_streets(model());
        let frame = |fast: bool| {
            let mut px = vec![0u16; (SIZE * SIZE) as usize];
            let mut fb = Fb::new(&mut px);
            draw(&mut fb, &m, &Ctx { live: true, zoom: ZOOM_DEFAULT, fast, look: None, arrow_only: false, light: false, dome: false });
            let rects = fb.rects;
            (px, rects)
        };
        let (full, full_rects) = frame(false);
        let (fast, fast_rects) = frame(true);
        // fewer rects in all, and a lot fewer for the streets themselves
        assert!(fast_rects < full_rects, "{fast_rects} vs {full_rects}");
        let cost = |fast: bool, zoom: usize| {
            let mut c = cmf_wp2_ui::Count { rects: 0, px: 0 };
            streets(&mut c, &rich(), ZOOMS[zoom], fast);
            c.rects
        };
        for zoom in [0, 2, 4] {
            let (f, q) = (cost(false, zoom), cost(true, zoom));
            eprintln!("zoom {zoom}: streets {f} rects, fast {q}");
            assert!(q * 2 < f, "zoom {zoom}: {q} vs {f}");
        }
        // the same streets are there: every pixel that is grey in the full frame is grey (some grey) in the fast one
        let grey = |p: u16| p == STREET.rgb565() || p == MAJOR.rgb565();
        let (mut both, mut only_full) = (0, 0);
        for (a, b) in full.iter().zip(&fast) {
            if grey(*a) {
                if grey(*b) { both += 1 } else { only_full += 1 }
            }
        }
        assert!(both > 1500 && only_full * 4 < both, "{both} {only_full}");
        // a main road is over a small street where they cross; the route over both
        let cross = |px: &Vec<u16>, x, y| px[(y * SIZE + x) as usize];
        assert!((CAR.1 - 90..CAR.1 - 35).any(|y| cross(&fast, CAR.0, y) == BLUE.rgb565()));
    }

    #[test]
    fn nothing_is_drawn_for_streets_in_the_corners() {
        // a street in the top left corner of the square, outside the round screen
        assert!(!near_screen((0, 0), (30, 0), 6));
        assert!(near_screen((0, 233), (30, 233), 6));
        assert!(near_screen((-100, 233), (600, 233), 6));
        assert!(!near_screen((-50, 20), (20, -50), 10));
    }

    #[test]
    fn the_menu_has_two_buttons_that_can_be_hit() {
        assert_eq!(button_at(C, BTN_Y[0] + 40), Some(Choice::Home));
        assert_eq!(button_at(C, BTN_Y[1] + 40), Some(Choice::Work));
        assert_eq!(button_at(C, 60), None);
        assert_eq!(button_at(C, 440), None);
        assert_eq!(button_at(5, BTN_Y[0] + 40), None);
        // the buttons sit inside the round screen with room to spare
        for top in BTN_Y {
            for y in [top, top + BTN_H - 1] {
                let (x0, x1) = geom::chord(y, y + 1, 6);
                assert!(BTN_X + BTN_H / 2 >= x0 && BTN_X + BTN_W - BTN_H / 2 <= x1, "{y}");
            }
        }
    }

    #[test]
    fn the_menu_draws_inside_the_circle_and_shows_a_pressed_button() {
        let draw_menu = |down: Option<Choice>, title: &str| {
            let mut px = vec![0u16; (SIZE * SIZE) as usize];
            let mut fb = Fb::new(&mut px);
            menu(&mut fb, title, "Swipe left to go back", down);
            px
        };
        let (a, b) = (draw_menu(None, "Start a destination"), draw_menu(Some(Choice::Work), "Start a destination"));
        for y in 0..SIZE {
            for x in 0..SIZE {
                if !geom::inside(x, y) {
                    assert_eq!(a[(y * SIZE + x) as usize], 0, "lit at {x},{y}");
                }
            }
        }
        let at = |px: &Vec<u16>, x: i32, y: i32| px[(y * SIZE + x) as usize];
        // the buttons have a face; the one under a finger has another
        assert_eq!(at(&a, BTN_X + 20, BTN_Y[0] + 45), BTN_FACE.rgb565());
        assert_eq!(at(&a, BTN_X + 20, BTN_Y[1] + 45), BTN_FACE.rgb565());
        assert_eq!(at(&b, BTN_X + 20, BTN_Y[1] + 45), BTN_DOWN.rgb565());
        assert_eq!(at(&b, BTN_X + 20, BTN_Y[0] + 45), BTN_FACE.rgb565());
        // the word is white on the face: some white inside the button
        let white = (BTN_X..BTN_X + BTN_W).any(|x| (BTN_Y[0]..BTN_Y[0] + BTN_H).any(|y| at(&a, x, y) == 0xFFFF));
        assert!(white);
        // another title draws a different screen
        assert!(a != draw_menu(None, "Waiting for Vela…"));
        // it is a still screen: cheap
        let mut c = cmf_wp2_ui::Count { rects: 0, px: 0 };
        menu(&mut c, "Start a destination", "", None);
        assert!(c.rects < 6000, "{}", c.rects);
        let mut px = vec![0u16; (SIZE * SIZE) as usize];
        going(&mut Fb::new(&mut px), Choice::Home);
        assert!(px.iter().any(|&p| p == 0xFFFF));
    }

    #[test]
    fn points_in_the_same_pixel_cost_nothing() {
        let mut m = model();
        // a street of 200 points a decimetre apart: one pixel at most
        let line: vec::Vec<[i16; 2]> = (0..200).map(|i| [-60i16, 100 + i / 2]).collect();
        let (msgs, _) = crate::proto::enc::roads(10, 4, [0, 400], &[line]);
        msgs.iter().for_each(|x| {
            m.apply(x);
        });
        let mut c = cmf_wp2_ui::Count { rects: 0, px: 0 };
        streets(&mut c, &m, 3, false);
        assert!(c.rects < 400, "{}", c.rects);
    }

    #[test]
    fn a_dome_frame_repaints_only_the_dome() {
        let m = rich();
        let mut px = vec![0x1234u16; (SIZE * SIZE) as usize];
        let mut fb = Fb::new(&mut px);
        draw(&mut fb, &m, &Ctx { live: true, zoom: ZOOM_DEFAULT, fast: false, look: None, arrow_only: false, light: false, dome: true });
        assert_eq!(fb.at(C, 100), 0x1234); // the map is left as it was
        assert_eq!(fb.at(CAR.0, CAR.1), 0x1234);
        assert_ne!(fb.at(C, C + 218), 0x1234); // the dome is painted
        // and it costs well under a whole frame
        let mut full = vec![0u16; (SIZE * SIZE) as usize];
        let mut ffb = Fb::new(&mut full);
        draw(&mut ffb, &m, &Ctx { live: true, zoom: ZOOM_DEFAULT, fast: false, look: None, arrow_only: false, light: false, dome: false });
        assert!(fb.rects * 2 < ffb.rects, "dome {} full {}", fb.rects, ffb.rects);
    }

    #[test]
    fn the_quick_projection_is_the_long_one() {
        let cars = [[0, 0], [3, -4], [-1200, 800]];
        for (p, car, course, mpp) in [
            ([10, 20], cars[0], 0x1234u16, 1),
            ([-250, 300], cars[1], 40000, 3),
            ([299, -280], cars[2], 65535, 13),
            ([59_000, -59_000], cars[0], 12345, 1),
            ([100_000, 90_000], cars[0], 777, 2),
        ] {
            let (dx, dy) = (p[0] as i64 - car[0] as i64, p[1] as i64 - car[1] as i64);
            let (s, c) = (sin(course) as i64, cos(course) as i64);
            let d = Q14 as i64 * mpp as i64;
            let (right, ahead) = ((dx * c - dy * s) / d, (dx * s + dy * c) / d);
            let lim = 1i64 << 20;
            let want = ((CAR.0 as i64 + right.clamp(-lim, lim)) as i32, (CAR.1 as i64 - ahead.clamp(-lim, lim)) as i32);
            assert_eq!(project(p, car, course, mpp), want, "{p:?} {car:?}");
        }
    }

    #[test]
    fn the_square_root_is_exact() {
        for n in [0u64, 1, 2, 3, 4, 15, 16, 17, 99, 100, 1 << 31, u32::MAX as u64 - 1, u32::MAX as u64, u32::MAX as u64 + 1, 1 << 40, u64::MAX >> 1] {
            let r = isqrt64(n) as u128;
            assert!(r * r <= n as u128 && (r + 1) * (r + 1) > n as u128, "{n}");
        }
        for n in 0..5000u64 {
            let r = isqrt64(n * 1_000_003) as u128;
            assert!(r * r <= (n * 1_000_003) as u128 && (r + 1) * (r + 1) > (n * 1_000_003) as u128);
        }
    }

    /// The dots as 1.25 drew them: every dot of the whole route turned, whether or not it could be on screen.
    #[allow(clippy::too_many_arguments)]
    fn dots_of_everything(s: &mut impl Surface, m: &Model, from: usize, to: usize, r: i32, step: i32, c: Rgb, mpp: i32, fast: bool) {
        let st = &m.state;
        let car = [st.x, st.y];
        let step = step as i64 * mpp as i64;
        let mut cum = 0i64;
        for i in 0..to.min(m.route_len).saturating_sub(1) {
            let (a, b) = (m.route[i], m.route[i + 1]);
            let (dx, dy) = (b[0] as i64 - a[0] as i64, b[1] as i64 - a[1] as i64);
            let len = isqrt64((dx * dx + dy * dy) as u64) as i64;
            if i >= from && len > 0 {
                let mut d = (cum + step - 1) / step * step;
                while d < cum + len {
                    let t = d - cum;
                    // This test-only baseline intentionally uses the one-shot projection helper:
                    // it models the pre-Projector renderer for pixel-equivalence comparisons.
                    let p = project(
                        [(a[0] as i64 + dx * t / len) as i32, (a[1] as i64 + dy * t / len) as i32],
                        car,
                        st.course,
                        mpp,
                    );
                    if geom::inside(p.0, p.1) {
                        if fast {
                            flat_disc(s, p.0, p.1, r, c);
                        } else {
                            disc(s, p.0, p.1, r, c, MAP_BG);
                        }
                    }
                    d += step;
                }
            }
            cum += len;
        }
    }

    fn long_route() -> Box<Model> {
        // a route 20 km long, then a turn: the car is half way along the first stretch
        let mut m = model();
        m.route[0] = [0, 0];
        m.route[1] = [0, 20_000];
        m.route[2] = [3000, 20_000];
        m.route_len = 3;
        m.state.x = 3;
        m.state.y = 9000;
        m
    }

    #[test]
    fn leaving_out_what_is_far_away_changes_no_pixel() {
        for which in 0..2 {
            for mpp in [1, 3] {
                for fast in [false, true] {
                    for course in [0u16, 9000, 30000, 50000] {
                        let mut m = if which == 0 { model() } else { long_route() };
                        m.state.course = course;
                        let (mut a, mut b) = (vec![0u16; (SIZE * SIZE) as usize], vec![0u16; (SIZE * SIZE) as usize]);
                        dots(&mut Fb::new(&mut a), &m, 0, m.route_len, 7, ROUTE_STEP, BLUE, mpp, fast);
                        dots_of_everything(&mut Fb::new(&mut b), &m, 0, m.route_len, 7, ROUTE_STEP, BLUE, mpp, fast);
                        assert!(a == b, "route {which} mpp {mpp} fast {fast} course {course}");
                        if mpp == 1 {
                            assert!(a.iter().any(|p| *p == BLUE.rgb565()), "route {which}: nothing drawn");
                        }
                    }
                }
            }
        }
    }
}
