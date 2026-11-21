//! Antialiased text in the app's own fonts (`fonts.rs`, Noto Sans), drawn as runs of equal coverage blended
//! over a background colour the caller names, as every `cmf-wp2-ui` shape is.

use cmf_wp2_ui::color::blend565;
use cmf_wp2_ui::{ONE, Surface};

pub struct Glyph {
    pub c: char,
    pub adv: u8,
    /// Ink box from the pen position on the baseline.
    pub x: i8,
    pub y: i8,
    pub w: u8,
    pub h: u8,
    /// Byte offset of the 4-bit coverage, two pixels a byte, rows packed.
    pub off: u16,
}

pub struct Font {
    pub ascent: i32,
    pub descent: i32,
    /// Height of a capital above the baseline.
    pub cap: i32,
    pub glyphs: &'static [Glyph],
    pub bits: &'static [u8],
}

impl Font {
    fn glyph(&self, c: char) -> Option<&Glyph> {
        self.glyphs.iter().find(|g| g.c == c)
    }

    /// Every character of `t` is in the font.
    pub fn covers(&self, t: &str) -> bool {
        t.chars().all(|c| self.glyph(c).is_some())
    }

    pub fn width(&self, t: &str) -> i32 {
        t.chars().map(|c| self.glyph(c).map_or(0, |g| g.adv as i32)).sum()
    }

    /// Draws `t` with its baseline at `y`, from `x`; returns the pen position after it. Characters the font
    /// lacks are skipped.
    pub fn draw(&self, s: &mut impl Surface, x: i32, y: i32, t: &str, fg: u16, bg: u16) -> i32 {
        let mut shade = [0u16; 16];
        for (a, c) in shade.iter_mut().enumerate() {
            *c = blend565(bg, fg, a as i32 * ONE / 15);
        }
        let mut pen = x;
        for ch in t.chars() {
            let Some(g) = self.glyph(ch) else { continue };
            let (gx, gy, w) = (pen + g.x as i32, y + g.y as i32, g.w as usize);
            let at = |i: usize| {
                let b = self.bits[g.off as usize + i / 2];
                if i.is_multiple_of(2) { b >> 4 } else { b & 15 }
            };
            for row in 0..g.h as usize {
                let base = row * w;
                let mut col = 0;
                while col < w {
                    let a = at(base + col);
                    let start = col;
                    while col < w && at(base + col) == a {
                        col += 1;
                    }
                    if a != 0 {
                        s.rect(gx + start as i32, gy + row as i32, (col - start) as i32, 1, shade[a as usize]);
                    }
                }
            }
            pen += g.adv as i32;
        }
        pen
    }

    /// The longest start of `t` that fits `max` px, and whether it was cut (then an ellipsis follows it).
    pub fn fit<'a>(&self, t: &'a str, max: i32) -> (&'a str, bool) {
        if self.width(t) <= max {
            return (t, false);
        }
        let room = max - self.width("…");
        let mut w = 0;
        for (i, c) in t.char_indices() {
            w += self.glyph(c).map_or(0, |g| g.adv as i32);
            if w > room {
                return (t[..i].trim_end(), true);
            }
        }
        (t, false)
    }

    /// `t` centred on `cx`, cut with an ellipsis to `max` px.
    #[allow(clippy::too_many_arguments)]
    pub fn centre(&self, s: &mut impl Surface, cx: i32, y: i32, t: &str, max: i32, fg: u16, bg: u16) {
        let (t, cut) = self.fit(t, max);
        let w = self.width(t) + if cut { self.width("…") } else { 0 };
        let end = self.draw(s, cx - w / 2, y, t, fg, bg);
        if cut {
            self.draw(s, end, y, "…", fg, bg);
        }
    }
}

/// A filled polygon (any shape, even-odd), antialiased: exact horizontal coverage on 4 sub-rows a pixel.
/// `paint(x)` picks the colour per column, so a shape can have two flat faces. Corners on whole pixels.
pub fn poly(s: &mut impl Surface, pts: &[(i32, i32)], bg: u16, paint: impl Fn(i32) -> u16) {
    if pts.len() > 8 {
        return; // icons and the car only
    }
    let mut h = [(0, 0); 8];
    for (d, p) in h.iter_mut().zip(pts) {
        *d = (p.0 * 2, p.1 * 2);
    }
    poly_half(s, &h[..pts.len()], bg, paint)
}

/// `poly` with the corners in half pixels (2 = the next pixel's left edge), so a shape can be centred on the
/// middle of a pixel, as the car's disc is.
pub fn poly_half(s: &mut impl Surface, pts: &[(i32, i32)], bg: u16, paint: impl Fn(i32) -> u16) {
    const SUB: i32 = 4; // sub-rows a pixel
    const FX: i32 = 16; // sub-pixel units a pixel, across
    let y0 = pts.iter().map(|p| p.1).min().unwrap_or(0).div_euclid(2);
    let y1 = (pts.iter().map(|p| p.1).max().unwrap_or(0) + 1).div_euclid(2);
    let x0 = pts.iter().map(|p| p.0).min().unwrap_or(0).div_euclid(2);
    let x1 = (pts.iter().map(|p| p.0).max().unwrap_or(0) + 1).div_euclid(2);
    let wide = (x1 - x0 + 2) as usize;
    if wide > 128 || pts.len() > 8 {
        return;
    }
    for y in y0..y1 {
        let mut cov = [0i32; 128];
        for k in 0..SUB {
            // sample at the sub-row's middle, in 1/(2*SUB) px
            let sy2 = y * 2 * SUB + 2 * k + 1;
            let mut xs = [0i32; 8];
            let mut n = 0;
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                let (ay, by) = (a.1 * SUB, b.1 * SUB);
                if (ay <= sy2) != (by <= sy2) {
                    xs[n] = (a.0 * (FX / 2)) + (b.0 - a.0) * (FX / 2) * (sy2 - ay) / (by - ay);
                    n += 1;
                }
            }
            xs[..n].sort_unstable();
            for span in xs[..n].chunks(2) {
                let [l, r] = span else { continue };
                for px in (l.div_euclid(FX))..=(r.div_euclid(FX)) {
                    let lo = (px * FX).max(*l);
                    let hi = ((px + 1) * FX).min(*r);
                    let i = px - x0 + 1;
                    if hi > lo && i >= 0 && (i as usize) < wide {
                        cov[i as usize] += hi - lo;
                    }
                }
            }
        }
        // runs of equal colour
        let full = FX * SUB;
        let mut i = 0;
        while i < wide {
            let c = cov[i];
            let x = x0 - 1 + i as i32;
            let col = if c == 0 { None } else { Some(blend565(bg, paint(x), c * ONE / full)) };
            let start = i;
            while i < wide && cov[i] == c && paint(x0 - 1 + i as i32) == paint(x) {
                i += 1;
            }
            if let Some(col) = col {
                s.rect(x, y, (i - start) as i32, 1, col);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::{L, S, XL};
    use cmf_wp2_ui::{Fb, SIZE};
    use std::vec;

    #[test]
    fn fonts_have_what_the_screens_need() {
        assert!(XL.covers("0123456789."));
        assert!(L.covers("12:05 pm km mi ft V maps Vela Arrived Off route Home Work"));
        assert!(S.covers("Bannerghatta Main Road · 4.7 km · 12 min Rerouting…"));
        assert!(!S.covers("ಹೊಸೂರು ರಸ್ತೆ"));
    }

    #[test]
    fn text_is_antialiased_and_fits() {
        let mut px = vec![0u16; (SIZE * SIZE) as usize];
        let mut fb = Fb::new(&mut px);
        let end = XL.draw(&mut fb, 100, 200, "300", 0xFFFF, 0);
        assert_eq!(end, 100 + XL.width("300"));
        let mut shades = std::collections::BTreeSet::new();
        for y in 100..220 {
            for x in 90..end + 10 {
                shades.insert(fb.at(x, y));
            }
        }
        assert!(shades.len() > 8, "{}", shades.len());
        let (t, cut) = S.fit("Bannerghatta Main Road", 120);
        assert!(cut && S.width(t) + S.width("…") <= 120, "{t}");
        assert_eq!(S.fit("Road", 120), ("Road", false));
    }

    #[test]
    fn a_polygon_has_soft_edges_and_a_solid_middle() {
        let mut px = vec![0u16; (SIZE * SIZE) as usize];
        let mut fb = Fb::new(&mut px);
        poly(&mut fb, &[(100, 100), (140, 160), (60, 160)], 0, |_| 0xFFFF);
        assert_eq!(fb.at(100, 150), 0xFFFF);
        assert_eq!(fb.at(100, 90), 0);
        let edge = fb.at(79, 130); // the left edge crosses it at x 79.7
        assert!(edge != 0 && edge != 0xFFFF, "{edge:x}");
    }
}
