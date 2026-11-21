//! Turn signs drawn in one pass: the sign is a union of strokes with round ends, rings and triangles, and each
//! pixel's coverage comes from its distance to that union. Shapes drawn one over another would leave a dark
//! seam wherever two antialiased edges meet; this has none.

use cmf_wp2_ui::color::blend565;
use cmf_wp2_ui::motion::{cos, sin};
use cmf_wp2_ui::{ONE, Surface};

/// Sub-pixel units a pixel.
const U: i64 = 16;

#[derive(Clone, Copy)]
enum Part {
    /// A stroke from one point to another, round ends.
    Seg((i32, i32), (i32, i32)),
    /// A circle of a radius, stroked.
    Ring((i32, i32), i32),
    /// A filled triangle.
    Tri([(i32, i32); 3]),
}

pub struct Sign {
    parts: [Part; 24],
    n: usize,
    /// Stroke width in px.
    w: i32,
}

impl Sign {
    pub fn new(w: i32) -> Sign {
        Sign { parts: [Part::Ring((0, 0), 0); 24], n: 0, w }
    }

    fn push(&mut self, p: Part) -> &mut Sign {
        if self.n < self.parts.len() {
            self.parts[self.n] = p;
            self.n += 1;
        }
        self
    }

    pub fn seg(&mut self, a: (i32, i32), b: (i32, i32)) -> &mut Sign {
        self.push(Part::Seg(a, b))
    }

    pub fn ring(&mut self, c: (i32, i32), r: i32) -> &mut Sign {
        self.push(Part::Ring(c, r))
    }

    pub fn tri(&mut self, t: [(i32, i32); 3]) -> &mut Sign {
        self.push(Part::Tri(t))
    }

    /// A stroked arc around `c` of radius `r`, from angle `start` over `sweep` (u16 turns, 0 = 3 o'clock,
    /// clockwise on screen; negative goes the other way), as `steps` straight pieces.
    pub fn arc(&mut self, c: (i32, i32), r: i32, start: u16, sweep: i32, steps: i32) -> &mut Sign {
        let at = |a: u16| (c.0 + ((r * cos(a)) >> 14), c.1 + ((r * sin(a)) >> 14));
        let mut prev = at(start);
        for i in 1..=steps {
            let p = at(start.wrapping_add((sweep * i / steps) as u16));
            self.seg(prev, p);
            prev = p;
        }
        self
    }

    /// Distance from `p` (in 1/U px) to the part's edge, in 1/U px; negative inside.
    fn dist(&self, part: &Part, p: (i64, i64)) -> i64 {
        let half = self.w as i64 * U / 2;
        let q = |v: (i32, i32)| (v.0 as i64 * U + U / 2, v.1 as i64 * U + U / 2);
        match *part {
            Part::Seg(a, b) => {
                let (a, b) = (q(a), q(b));
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let len2 = dx * dx + dy * dy;
                let t = if len2 == 0 { 0 } else { ((p.0 - a.0) * dx + (p.1 - a.1) * dy).clamp(0, len2) };
                let (cx, cy) = if len2 == 0 { a } else { (a.0 + dx * t / len2, a.1 + dy * t / len2) };
                near((p.0 - cx).pow(2) + (p.1 - cy).pow(2), half)
            }
            Part::Ring(c, r) => {
                let c = q(c);
                (isqrt((p.0 - c.0).pow(2) + (p.1 - c.1).pow(2)) - r as i64 * U).abs() - half
            }
            Part::Tri(t) => {
                let v = [q(t[0]), q(t[1]), q(t[2])];
                let area = (v[1].0 - v[0].0) * (v[2].1 - v[0].1) - (v[1].1 - v[0].1) * (v[2].0 - v[0].0);
                let sign = if area > 0 { 1 } else { -1 };
                (0..3)
                    .map(|i| {
                        let (a, b) = (v[i], v[(i + 1) % 3]);
                        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
                        let len = isqrt(ex * ex + ey * ey).max(1);
                        // outward distance from the edge's line
                        -sign * (ex * (p.1 - a.1) - ey * (p.0 - a.0)) / len
                    })
                    .max()
                    .unwrap_or(0)
            }
        }
    }

    /// The part's box in px, grown by the stroke and a pixel of soft edge.
    fn bbox(&self, part: &Part) -> (i32, i32, i32, i32) {
        let g = self.w / 2 + 2;
        let (x0, y0, x1, y1) = match *part {
            Part::Seg(a, b) => (a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)),
            Part::Ring(c, r) => (c.0 - r, c.1 - r, c.0 + r, c.1 + r),
            Part::Tri(t) => (
                t[0].0.min(t[1].0).min(t[2].0),
                t[0].1.min(t[1].1).min(t[2].1),
                t[0].0.max(t[1].0).max(t[2].0),
                t[0].1.max(t[1].1).max(t[2].1),
            ),
        };
        (x0 - g, y0 - g, x1 + g, y1 + g)
    }

    /// Draws the sign in colour `c` over `bg`, looking only inside the box `x0..x1`, `y0..y1`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&self, s: &mut impl Surface, x0: i32, y0: i32, x1: i32, y1: i32, c: u16, bg: u16) {
        let mut boxes = [(0, 0, 0, 0); 24];
        for (b, part) in boxes.iter_mut().zip(&self.parts[..self.n]) {
            *b = self.bbox(part);
        }
        let mut shade = [0u16; 17];
        for (k, v) in shade.iter_mut().enumerate() {
            *v = blend565(bg, c, k as i32 * ONE / 16);
        }
        for y in y0..y1 {
            let mut run: Option<(i32, usize)> = None;
            for x in x0..=x1 {
                let k = if x == x1 {
                    usize::MAX // flush
                } else {
                    let p = (x as i64 * U + U / 2, y as i64 * U + U / 2);
                    let d = self.parts[..self.n]
                        .iter()
                        .zip(&boxes)
                        .filter(|(_, b)| x >= b.0 && x <= b.2 && y >= b.1 && y <= b.3)
                        .map(|(part, _)| self.dist(part, p))
                        .min()
                        .unwrap_or(U);
                    (U / 2 - d).clamp(0, U) as usize
                };
                match run {
                    Some((_, rk)) if rk == k => {}
                    _ => {
                        if let Some((rx, rk)) = run
                            && rk > 0
                        {
                            s.rect(rx, y, x - rx, 1, shade[rk]);
                        }
                        run = Some((x, k));
                    }
                }
            }
        }
    }
}

/// The three corners of an arrowhead with its tip at `tip`, pointing along `a` (turns clockwise from up), `h` px
/// long.
pub fn head(tip: (i32, i32), a: u16, h: i32) -> [(i32, i32); 3] {
    let d = |a: u16, r: i32| ((r * sin(a)) >> 14, -(r * cos(a)) >> 14);
    let back = d(a, -h);
    let side = d(a.wrapping_add(0x4000), h * 4 / 5);
    let base = (tip.0 + back.0, tip.1 + back.1);
    [tip, (base.0 + side.0, base.1 + side.1), (base.0 - side.0, base.1 - side.1)]
}

/// The distance from `d2`, a squared distance, to a stroke of half width `half`, with the square root taken only
/// on the soft edge: well inside is `-U`, well outside `U`.
fn near(d2: i64, half: i64) -> i64 {
    let out = half + U;
    if d2 >= out * out {
        return U;
    }
    let inn = (half - U).max(0);
    if d2 <= inn * inn && half >= U {
        return -U;
    }
    isqrt(d2) - half
}

fn isqrt(n: i64) -> i64 {
    if n < 2 {
        return n.max(0);
    }
    let mut x = 1i64 << ((64 - n.leading_zeros()).div_ceil(2));
    loop {
        let y = (x + n / x) / 2;
        if y >= x {
            return x;
        }
        x = y;
    }
}
