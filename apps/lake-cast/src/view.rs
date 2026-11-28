//! Open-beach Lake Cast renderer matching the user's sketch: full azure lake (`0..SHORE_Y`),
//! warm sandy beach (`SHORE_Y..SIZE`), rod rising from sand into water, single-pass zero-flicker
//! swimming fish rendering, and left vertical bar that appears only during Reeling.

use crate::font::{FONT_LG, FONT_MD, FONT_SM};
use crate::game::{
    format_weight, rod_pose, Difficulty, Game, LakeFish, Mode, Records, RodPose, Species, State,
    C, CAUGHT_BTN_H, CAUGHT_BTN_W, CAUGHT_BTN_X, CAUGHT_BTN_Y, DIFFICULTIES, GAUGE_H, GAUGE_W,
    GAUGE_X, GAUGE_Y, HUD_H, HUD_INFO_W, HUD_INFO_X, HUD_MENU_W, HUD_MENU_X, HUD_Y, MAX_FISH,
    MENU_BTN_H, MENU_BTN_W, MENU_BTN_X, MENU_DERBY_Y, MENU_RELAX_Y, MENU_TROPHY_Y, OVER_BTN_H,
    OVER_BTN_W, OVER_BTN_X, OVER_MENU_Y, OVER_RETRY_Y, PROG_W, PROG_X, SHORE_Y, SIZE, SPECIES_ALL,
    TRACK_H, TRACK_W, TRACK_X, TRACK_Y, TROPHY_NAV_H, TROPHY_NAV_W, TROPHY_NAV_Y, TROPHY_NEXT_X,
    TROPHY_PREV_X,
};
use crate::sprites::draw_fish;
use cmf_wp2_ui::draw::{line, rrect, Run};
use cmf_wp2_ui::motion::isqrt;
use cmf_wp2_ui::text::int;
use cmf_wp2_ui::{ONE, Rgb, Surface};

const WHITE: Rgb = Rgb(255, 255, 255);
const SHADOW_TEXT: Rgb = Rgb(10, 48, 86);
const PILL_BG: Rgb = Rgb(12, 34, 58);
const PILL_BORDER: Rgb = Rgb(86, 178, 218);
const PILL_ACTIVE_BG: Rgb = Rgb(20, 66, 98);
const PILL_ACTIVE_RIM: Rgb = Rgb(255, 206, 86);
const CARD_BG: Rgb = Rgb(11, 32, 54);
const CARD_BORDER: Rgb = Rgb(242, 194, 72);
const DIM_TEXT: Rgb = Rgb(172, 210, 232);
const TROPHY_BG: Rgb = Rgb(12, 34, 56);
const SILHOUETTE: Rgb = Rgb(28, 72, 102);

// Left vertical gauge colors.
const GAUGE_FRAME: Rgb = Rgb(138, 92, 52);
const GAUGE_RIM: Rgb = Rgb(228, 178, 98);
const TRACK_BG: Rgb = Rgb(16, 44, 68);
const BAR_LIT: Rgb = Rgb(84, 226, 114);
const BAR_LIT_EDGE: Rgb = Rgb(172, 252, 186);
const BAR_DIM: Rgb = Rgb(48, 124, 82);
const BAR_DIM_EDGE: Rgb = Rgb(88, 164, 118);

const CAUGHT_X: i32 = 84;
const CAUGHT_Y: i32 = 98;
const CAUGHT_W: i32 = 298;
const CAUGHT_H: i32 = 280;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Rect {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

impl Rect {
    fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect {
            x0: x.max(0),
            y0: y.max(0),
            x1: (x + w).min(SIZE),
            y1: (y + h).min(SIZE),
        }
    }

    fn empty() -> Self {
        Rect { x0: 0, y0: 0, x1: 0, y1: 0 }
    }

    fn is_empty(self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        Rect {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }

    fn intersects(self, other: Self) -> bool {
        !self.is_empty()
            && !other.is_empty()
            && self.x0 < other.x1
            && self.x1 > other.x0
            && self.y0 < other.y1
            && self.y1 > other.y0
    }
}

struct Clip<'a, S: Surface> {
    s: &'a mut S,
    r: Rect,
}

impl<S: Surface> Surface for Clip<'_, S> {
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u16) {
        let ax = x.max(self.r.x0);
        let ay = y.max(self.r.y0);
        let bx = (x + w).min(self.r.x1);
        let by = (y + h).min(self.r.y1);
        if bx > ax && by > ay {
            self.s.rect(ax, ay, bx - ax, by - ay, c);
        }
    }

    fn text(&mut self, x: i32, y: i32, t: &str, c: u16) {
        self.s.text(x, y, t, c);
    }
}

/// Deterministic background matching user sketch: full azure lake (`0..SHORE_Y`) + warm yellow sand (`SHORE_Y..SIZE`).
pub fn scene_rgb(x: i32, y: i32) -> Rgb {
    if y < SHORE_Y {
        if y >= SHORE_Y - 3 {
            return Rgb(188, 242, 248); // shoreline foam highlight
        }
        // Smooth 4px-banded bright azure/ocean water gradient.
        let wy = (y & !3).clamp(0, SHORE_Y - 4);
        let mut wcol = if wy < 140 {
            Rgb(14, 128, 196).mix(Rgb(20, 162, 214), wy * ONE / 140)
        } else {
            Rgb(20, 162, 214).mix(Rgb(46, 194, 228), (wy - 140) * ONE / (SHORE_Y - 144))
        };
        // Subtle ambient water shimmer streaks.
        if (y % 26) == 0 && ((x + y * 3) % 67) < 8 {
            wcol = wcol.mix(Rgb(126, 224, 244), 260);
        }
        return wcol;
    }

    if y < SHORE_Y + 7 {
        return Rgb(194, 160, 88); // wet shoreline sand strip
    }

    // Sunny warm golden sand beach.
    let sy = ((y - SHORE_Y - 7) & !3).clamp(0, SIZE - SHORE_Y - 7);
    let sand = Rgb(246, 206, 102).mix(Rgb(234, 184, 78), sy * ONE / (SIZE - SHORE_Y - 7));

    // Small beach pebbles safely in lower sand (`y = 355..432`).
    const PEBBLES: [(i32, i32, i32); 3] = [(118, 376, 5), (356, 364, 5), (318, 412, 4)];
    for &(px, py, pr) in &PEBBLES {
        let d2 = (x - px) * (x - px) + (y - py) * (y - py);
        if d2 <= pr * pr {
            return if y < py { Rgb(160, 152, 138) } else { Rgb(114, 106, 94) };
        }
    }
    sand
}

fn bg_has_row_variation(y: i32) -> bool {
    if y < SHORE_Y - 3 {
        return (y % 26) == 0;
    }
    if y < SHORE_Y + 7 {
        return false;
    }
    y >= 355 && y <= 420
}

fn draw_bg_box(s: &mut impl Surface, r: Rect) {
    if r.is_empty() {
        return;
    }
    let mut y = r.y0;
    while y < r.y1 {
        if !bg_has_row_variation(y) {
            let c = scene_rgb(r.x0, y).rgb565();
            let mut y_end = y + 1;
            while y_end < r.y1 && !bg_has_row_variation(y_end) && scene_rgb(r.x0, y_end).rgb565() == c
            {
                y_end += 1;
            }
            s.rect(r.x0, y, r.x1 - r.x0, y_end - y, c);
            y = y_end;
            continue;
        }
        let mut run = Run::new(s, y, false);
        for x in r.x0..r.x1 {
            run.put(x, 1, scene_rgb(x, y).rgb565());
        }
        drop(run);
        y += 1;
    }
}

fn fish_bbox(f: &LakeFish) -> Rect {
    let (w, h, _) = f.species.silhouette();
    Rect::new(f.x() - w / 2 - 12, f.y() - h / 2 - 6, w + 24, h + 12)
}

/// Evaluates whether pixel `(x, y)` belongs to `f` underwater sprite (ZERO-FLICKER single-pass render!).
fn fish_pixel_at(f: &LakeFish, x: i32, y: i32) -> Option<u16> {
    let (w, h, _) = f.species.silhouette();
    let cx = f.x();
    let cy = f.y();
    let rx = (w / 2).max(9);
    let ry = (h / 2).max(4);
    if (y - cy).abs() > ry + 4 || (x - cx).abs() > rx + 10 {
        return None;
    }

    let dir = if f.facing_right() { 1 } else { -1 };
    let lx = (x - cx) * dir; // + toward nose, - toward tail
    let dy = y - cy;
    let wag = (((f.phase_ms as i32 / 90) % 4) - 2).abs() - 1;
    let depth = scene_rgb(x.clamp(0, SIZE - 1), y.clamp(0, SHORE_Y - 1));

    // Palette per species softly blended into water depth so fish stay genuinely submerged.
    let (dorsal_t, flank_t, belly_t, fin_t, mix_w) = match f.species {
        Species::Roach => (Rgb(22, 62, 108), Rgb(118, 188, 224), Rgb(208, 238, 250), Rgb(224, 96, 68), 640),
        Species::Perch => (Rgb(26, 78, 54), Rgb(108, 164, 72), Rgb(198, 222, 138), Rgb(232, 122, 46), 670),
        Species::Trout => (Rgb(20, 84, 108), Rgb(112, 182, 206), Rgb(218, 236, 244), Rgb(228, 118, 154), 650),
        Species::Carp => (Rgb(84, 58, 24), Rgb(178, 132, 56), Rgb(234, 202, 118), Rgb(214, 98, 42), 680),
        Species::Pike => (Rgb(18, 66, 48), Rgb(56, 128, 76), Rgb(168, 212, 166), Rgb(192, 98, 46), 670),
        Species::GoldenKoi => (Rgb(226, 86, 36), Rgb(248, 194, 58), Rgb(248, 244, 228), Rgb(246, 136, 46), 740),
    };
    let shade = |c: Rgb| depth.mix(c, mix_w).rgb565();

    // Organic tapered body contour: rounded snout front, graceful taper to a narrow tail wrist.
    let front_x = match f.species {
        Species::Pike => rx / 6,
        Species::Carp | Species::GoldenKoi => rx / 3,
        _ => rx / 4,
    };
    let tail_wrist_x = -rx + 1;
    let body_half = if lx >= tail_wrist_x && lx <= rx {
        if lx >= front_x {
            let fr = (rx - front_x + 1).max(2);
            let dx_f = lx - front_x;
            ry * isqrt((fr * fr - dx_f * dx_f).max(0)) / fr
        } else {
            let span_b = (front_x - tail_wrist_x).max(1);
            let t = lx - tail_wrist_x;
            let wrist = if ry >= 6 { 2 } else { 1 };
            wrist + (ry - wrist) * (t * (2 * span_b - t)) / (span_b * span_b)
        }
    } else {
        -1
    };

    if body_half > 0 && dy.abs() <= body_half {
        // Eye glint near upper-front head.
        let eye_x = rx - 5;
        let eye_y = -(ry / 3).max(1);
        if lx >= eye_x && lx <= eye_x + 1 && dy >= eye_y && dy <= eye_y + 1 {
            let eye_c = if lx == eye_x && dy == eye_y {
                Rgb(10, 18, 28)
            } else {
                Rgb(242, 250, 255)
            };
            return Some(depth.mix(eye_c, 820).rgb565());
        }

        // Curved gill accent behind head.
        if lx == eye_x - 3 && dy.abs() <= (body_half - 2).max(1) && dy >= -body_half / 2 {
            return Some(shade(dorsal_t));
        }

        // Species-distinct markings & dorsal-to-belly shading.
        let col = match f.species {
            Species::Perch => {
                if dy < body_half - 1 && lx < eye_x - 3 && lx > tail_wrist_x + 2 && (lx + 5).rem_euclid(5) < 2 {
                    dorsal_t
                } else if dy <= -body_half / 3 {
                    dorsal_t
                } else if dy >= body_half * 2 / 3 {
                    belly_t
                } else {
                    flank_t
                }
            }
            Species::Trout => {
                if (dy == 0 || dy == 1) && lx < eye_x - 2 && lx > tail_wrist_x + 1 {
                    fin_t
                } else if dy < 0 && ((lx + dy * 2) & 3) == 0 && lx < eye_x - 2 {
                    Rgb(16, 48, 66)
                } else if dy <= -body_half / 3 {
                    dorsal_t
                } else if dy >= body_half * 2 / 3 {
                    belly_t
                } else {
                    flank_t
                }
            }
            Species::Pike => {
                if dy >= -1 && dy < body_half - 1 && ((lx + dy * 3) & 7) == 0 && lx < eye_x - 2 {
                    Rgb(210, 228, 128)
                } else if dy <= -body_half / 3 {
                    dorsal_t
                } else if dy >= body_half * 2 / 3 {
                    belly_t
                } else {
                    flank_t
                }
            }
            Species::GoldenKoi => {
                if ((lx >= 2 && lx <= 7) || (lx >= -6 && lx <= -2)) && dy <= 1 {
                    dorsal_t // crimson koi spot
                } else if dy >= 2 {
                    belly_t // pearl belly
                } else {
                    flank_t // golden back
                }
            }
            _ => {
                if dy <= -body_half / 3 {
                    dorsal_t
                } else if dy >= body_half * 2 / 3 {
                    belly_t
                } else {
                    flank_t
                }
            }
        };
        return Some(shade(col));
    }

    // Species-shaped dorsal crest on top.
    let (d0, d1, dh) = match f.species {
        Species::Pike => (-rx + 3, -rx + 10, 2),
        Species::Carp | Species::GoldenKoi => (-rx / 2, rx / 4, 2),
        Species::Perch => (-3, rx / 3, 2),
        _ => (-2, rx / 4, 2),
    };
    if lx >= d0 && lx <= d1 && body_half > 0 && dy < -body_half && dy >= -body_half - dh {
        if lx > d0 || dy == -body_half - 1 {
            return Some(shade(fin_t));
        }
    }

    // Lower pectoral/ventral fin.
    if lx >= -3 && lx <= 2 && body_half > 0 && (dy == body_half + 1 || (dy == body_half + 2 && lx <= 0)) {
        return Some(shade(fin_t));
    }

    // Flared forked or veil caudal tail behind the narrow tail wrist (`step >= 0`).
    let tail_len = if f.species == Species::GoldenKoi { 8 } else { 6 };
    let step = tail_wrist_x - lx;
    if (0..=tail_len).contains(&step) {
        let wy = cy + wag * step / tail_len;
        let tdy = (y - wy).abs();
        let half = 1 + step * (ry + 1) / tail_len;
        let forked = !matches!(f.species, Species::Carp | Species::GoldenKoi);
        if forked && step >= 3 {
            let notch = (step - 2).min(half - 1);
            if tdy >= notch && tdy <= half {
                return Some(shade(fin_t));
            }
        } else if tdy <= half {
            return Some(shade(if f.species == Species::GoldenKoi && dy > 0 { belly_t } else { fin_t }));
        }
    }

    None
}

fn rod_bounds(pose: RodPose, bob_x: i32, bob_y: i32) -> Rect {
    let x0 = pose.grip_x.min(pose.ctrl_x).min(pose.tip_x).min(bob_x) - 16;
    let x1 = pose.grip_x.max(pose.ctrl_x).max(pose.tip_x).max(bob_x) + 16;
    let y0 = pose.grip_y.min(pose.ctrl_y).min(pose.tip_y).min(bob_y) - 10;
    let y1 = pose.grip_y.max(bob_y) + 12;
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

fn draw_rod_and_line(
    s: &mut impl Surface,
    pose: RodPose,
    bob_x: i32,
    bob_y: i32,
    show_line: bool,
    taut: bool,
    clip: Rect,
) {
    let mut c = Clip { s, r: clip };
    let under = |px: i32, py: i32| scene_rgb(px.clamp(0, SIZE - 1), py.clamp(0, SIZE - 1));

    if show_line {
        let line_col = if taut { WHITE } else { Rgb(218, 244, 255) };
        if taut {
            line(&mut c, pose.tip_x, pose.tip_y, bob_x, bob_y, 2, line_col, under(pose.tip_x, pose.tip_y));
        } else {
            let mx = (pose.tip_x + bob_x) / 2;
            let my = (pose.tip_y + bob_y) / 2 + 8;
            line(&mut c, pose.tip_x, pose.tip_y, mx, my, 1, line_col, under(mx, my));
            line(&mut c, mx, my, bob_x, bob_y, 1, line_col, under(bob_x, bob_y));
        }
    }

    // Sleek ivory-white & warm cork rod matching user sketch, with small red tip marker when aiming!
    let mut px = pose.grip_x;
    let mut py = pose.grip_y;
    for step in 1..=5 {
        let t = step * 256 / 5;
        let u = 256 - t;
        let nx = (u * u * pose.grip_x + 2 * u * t * pose.ctrl_x + t * t * pose.tip_x) >> 16;
        let ny = (u * u * pose.grip_y + 2 * u * t * pose.ctrl_y + t * t * pose.tip_y) >> 16;
        let thick = match step {
            1 => 6,
            2 => 4,
            3 | 4 => 3,
            _ => 2,
        };
        let col = if step == 1 {
            Rgb(172, 114, 62) // cork handle on beach sand
        } else {
            Rgb(246, 248, 252) // clean crisp white shaft (as in sketch!)
        };
        line(&mut c, px, py, nx, ny, thick, col, under((px + nx) / 2, (py + ny) / 2));
        px = nx;
        py = ny;
    }

    // Red tip marker when uncast (or reel at bottom).
    if !show_line {
        c.rect(pose.tip_x - 3, pose.tip_y - 3, 6, 5, Rgb(246, 60, 44).rgb565());
    }
    let rx = (pose.grip_x * 3 + pose.ctrl_x) / 4;
    let ry = (pose.grip_y * 3 + pose.ctrl_y) / 4;
    c.rect(rx - 5, ry - 5, 10, 10, Rgb(228, 174, 52).rgb565());
    c.rect(rx - 3, ry - 3, 6, 6, Rgb(36, 42, 52).rgb565());
}

const HOOK_PW: i32 = 74;
const HOOK_PH: i32 = 24;

fn hook_pill_pos(bx: i32, by: i32) -> (i32, i32) {
    (
        (bx - HOOK_PW / 2).clamp(112, SIZE - HOOK_PW - 36),
        (by - 34).max(HUD_Y + HUD_H + 8),
    )
}

fn bobber_bounds(bx: i32, by: i32, state: State, splash_ms: u32) -> Rect {
    if state == State::Banner {
        return Rect::new(
            (bx - 76).clamp(108, SIZE - 170),
            (by - 24).clamp(HUD_Y + HUD_H + 10, SHORE_Y - 40),
            152,
            48,
        );
    }
    if state == State::Bite {
        let (px, hpy) = hook_pill_pos(bx, by);
        return Rect::new(bx - 36, by - 36, 72, 72)
            .union(Rect::new(px - 2, hpy - 2, HOOK_PW + 4, HOOK_PH + 4));
    }
    let r = if splash_ms > 0 { 36 } else { 18 };
    Rect::new(bx - r, by - r - 8, r * 2, r * 2 + 16)
}

fn draw_ring_outline(s: &mut impl Surface, cx: i32, cy: i32, r: i32, col: Rgb) {
    if r <= 2 {
        return;
    }
    let c = col.rgb565();
    let r2_hi = (r + 1) * (r + 1);
    let r2_lo = (r - 1) * (r - 1);
    for dy in -r..=r {
        let oy = dy * 2;
        if oy.abs() > r + 1 {
            continue;
        }
        let mut run = Run::new(s, cy + dy, false);
        for dx in -r..=r {
            let d2 = dx * dx + oy * oy;
            if d2 >= r2_lo && d2 <= r2_hi {
                run.put(cx + dx, 1, c);
            }
        }
    }
}

fn draw_bobber_and_fx(s: &mut impl Surface, g: &Game, clip: Rect) {
    if !g.has_cast && g.state != State::Banner {
        return;
    }
    let (bx, by) = g.bobber_pos();
    let bb = bobber_bounds(bx, by, g.state, g.splash_ms);
    if !bb.intersects(clip) {
        return;
    }
    let mut c = Clip { s, r: clip };

    if g.state == State::Banner {
        let bw = 144;
        let bh = 32;
        let x = (bx - bw / 2).clamp(112, SIZE - bw - 26);
        let y = (by - 16).clamp(HUD_Y + HUD_H + 12, SHORE_Y - bh - 12);
        let border = Rgb(255, 176, 68);
        rrect(&mut c, x, y, bw, bh, 12, border, scene_rgb(x, y));
        rrect(&mut c, x + 2, y + 2, bw - 4, bh - 4, 10, PILL_BG, border);
        FONT_SM.centre(&mut c, x + bw / 2, y + 8, "FISH ESCAPED!", WHITE, PILL_BG);
        return;
    }

    if g.splash_ms > 0 {
        let k = (520 - g.splash_ms.min(520)) as i32;
        let r1 = 8 + k / 20;
        draw_ring_outline(&mut c, bx, by + 2, r1, Rgb(220, 248, 255));
        if r1 > 14 {
            draw_ring_outline(&mut c, bx, by + 2, r1 - 9, Rgb(146, 226, 246));
        }
    }

    if g.state == State::Floating {
        let wave_r = 9 + ((g.lake_ms / 120) % 6) as i32;
        draw_ring_outline(&mut c, bx, by + 2, wave_r, Rgb(176, 236, 250));
    }

    if g.state == State::Reeling {
        let col = if g.fish_in_bar { Rgb(106, 244, 184) } else { Rgb(255, 134, 82) };
        let wave_r = 9 + ((g.reel_phase_ms / 140) % 5) as i32;
        draw_ring_outline(&mut c, bx, by + 2, wave_r, col);
        c.rect(bx - 3, by - 4, 6, 4, Rgb(252, 68, 48).rgb565());
        c.rect(bx - 3, by, 6, 3, WHITE.rgb565());
        return;
    }

    let bob_dy = match g.state {
        State::Floating => (((g.lake_ms / 240) % 4) as i32 - 2).abs() - 1,
        State::Nibble => 3,
        State::Bite => 7,
        _ => 0,
    };
    let py = by + bob_dy;

    if g.state != State::Bite {
        c.rect(bx - 1, py - 9, 2, 4, Rgb(30, 34, 42).rgb565());
        c.rect(bx - 4, py - 5, 8, 5, Rgb(252, 68, 48).rgb565());
        c.rect(bx - 5, py - 4, 10, 3, Rgb(252, 68, 48).rgb565());
        c.rect(bx - 2, py - 4, 2, 2, WHITE.rgb565());
        c.rect(bx - 4, py, 8, 4, WHITE.rgb565());
    } else {
        c.rect(bx - 4, py - 2, 8, 5, Rgb(255, 124, 46).rgb565());
        draw_ring_outline(&mut c, bx, by + 2, 18, Rgb(255, 226, 86));
        let (px, hpy) = hook_pill_pos(bx, by);
        rrect(&mut c, px, hpy, HOOK_PW, HOOK_PH, 10, Rgb(255, 138, 36), scene_rgb(px, hpy));
        FONT_SM.centre(&mut c, px + HOOK_PW / 2, hpy + 5, "HOOK!", WHITE, Rgb(255, 138, 36));
    }
}

/// Draws the wooden left gauge frame ONLY when `Reeling`.
fn draw_gauge_frame(s: &mut impl Surface) {
    rrect(
        s,
        GAUGE_X,
        GAUGE_Y,
        GAUGE_W,
        GAUGE_H,
        10,
        GAUGE_RIM,
        scene_rgb(GAUGE_X, GAUGE_Y),
    );
    rrect(
        s,
        GAUGE_X + 2,
        GAUGE_Y + 2,
        GAUGE_W - 4,
        GAUGE_H - 4,
        8,
        GAUGE_FRAME,
        GAUGE_RIM,
    );
}

fn prog_color(reel_progress: i32) -> (u8, Rgb) {
    if reel_progress >= 720 {
        (2, Rgb(84, 242, 168))
    } else if reel_progress >= 360 {
        (1, Rgb(248, 206, 68))
    } else {
        (0, Rgb(242, 118, 58))
    }
}

fn gauge_row_sig(y: i32, bar_top: i32, bar_bot: i32, fy: i32) -> (u8, i32) {
    let zone = if y >= bar_top && y < bar_bot {
        if y == bar_top || y == bar_bot - 1 { 1 } else { 2 }
    } else {
        0
    };
    let d = y - fy;
    let f_rel = if (-5..=5).contains(&d) { d } else { 99 };
    (zone, f_rel)
}

/// ZERO-FLICKER incremental single-pass vertical gauge interior: skips unchanged scanlines and writes only deltas!
fn draw_gauge_inner(s: &mut impl Surface, g: &Game, prev: Option<Seen>) {
    let bar_top = g.bar_y.clamp(0, TRACK_H);
    let bar_bot = (g.bar_y + g.bar_h).clamp(bar_top, TRACK_H);
    let bar_fill = if g.fish_in_bar { BAR_LIT.rgb565() } else { BAR_DIM.rgb565() };
    let bar_edge = if g.fish_in_bar {
        BAR_LIT_EDGE.rgb565()
    } else {
        BAR_DIM_EDGE.rgb565()
    };

    let fy = g.track_fish_y();
    let track_bg_c = TRACK_BG.rgb565();
    let fish_body_c = Rgb(255, 188, 56).rgb565();
    let fish_out_c = Rgb(168, 66, 22).rgb565();
    let fish_eye_c = WHITE.rgb565();

    let diff_only = prev.filter(|o| o.fish_in_bar == g.fish_in_bar);
    let old_bar_top = diff_only.map_or(0, |o| o.bar_y.clamp(0, TRACK_H));
    let old_bar_bot = diff_only.map_or(0, |o| (o.bar_y + o.bar_h).clamp(old_bar_top, TRACK_H));
    let old_fy = diff_only.map_or(-99, |o| o.track_fish_y);

    let mut y = 0;
    while y < TRACK_H {
        let cur_sig = gauge_row_sig(y, bar_top, bar_bot, fy);
        if diff_only.is_some() && cur_sig == gauge_row_sig(y, old_bar_top, old_bar_bot, old_fy) {
            y += 1;
            continue;
        }

        if cur_sig.1 != 99 {
            let dy = (y - fy).abs();
            let half = match dy {
                0..=1 => 7,
                2..=3 => 5,
                _ => 3,
            };
            let bg_c = match cur_sig.0 {
                1 => bar_edge,
                2 => bar_fill,
                _ => track_bg_c,
            };
            let cx = TRACK_W / 2;
            let fx0 = cx - half;
            let fx1 = cx + half;
            s.rect(TRACK_X, TRACK_Y + y, fx0, 1, bg_c);
            s.rect(TRACK_X + fx0, TRACK_Y + y, 1, 1, fish_out_c);
            if dy == 1 && y < fy {
                s.rect(TRACK_X + fx0 + 1, TRACK_Y + y, 2, 1, fish_eye_c);
                s.rect(TRACK_X + fx0 + 3, TRACK_Y + y, (fx1 - fx0 - 4).max(1), 1, fish_body_c);
            } else {
                s.rect(TRACK_X + fx0 + 1, TRACK_Y + y, (fx1 - fx0 - 2).max(1), 1, fish_body_c);
            }
            s.rect(TRACK_X + fx1 - 1, TRACK_Y + y, 1, 1, fish_out_c);
            s.rect(TRACK_X + fx1, TRACK_Y + y, TRACK_W - fx1, 1, bg_c);
            y += 1;
            continue;
        }

        let col = match cur_sig.0 {
            1 => bar_edge,
            2 => bar_fill,
            _ => track_bg_c,
        };
        let mut y_end = y + 1;
        while y_end < TRACK_H {
            let nsig = gauge_row_sig(y_end, bar_top, bar_bot, fy);
            if nsig != cur_sig {
                break;
            }
            if diff_only.is_some() && nsig == gauge_row_sig(y_end, old_bar_top, old_bar_bot, old_fy) {
                break;
            }
            y_end += 1;
        }
        s.rect(TRACK_X, TRACK_Y + y, TRACK_W, y_end - y, col);
        y = y_end;
    }

    let fill_h = (g.reel_progress.clamp(0, 1000) * TRACK_H / 1000).clamp(0, TRACK_H);
    let empty_h = TRACK_H - fill_h;
    let empty_bg = Rgb(18, 38, 54).rgb565();
    let (bucket, pcol) = prog_color(g.reel_progress);
    let pcol_c = pcol.rgb565();

    match prev {
        Some(old) if old.prog_bucket == bucket => {
            if fill_h > old.prog_h {
                s.rect(PROG_X, TRACK_Y + empty_h, PROG_W, fill_h - old.prog_h, pcol_c);
            } else if fill_h < old.prog_h {
                let old_empty = TRACK_H - old.prog_h;
                s.rect(PROG_X, TRACK_Y + old_empty, PROG_W, old.prog_h - fill_h, empty_bg);
            }
        }
        _ => {
            if empty_h > 0 {
                s.rect(PROG_X, TRACK_Y, PROG_W, empty_h, empty_bg);
            }
            if fill_h > 0 {
                s.rect(PROG_X, TRACK_Y + empty_h, PROG_W, fill_h, pcol_c);
            }
        }
    }
}

fn draw_hud(s: &mut impl Surface, g: &Game, best: Records, clip: Rect) {
    if !matches!(
        g.state,
        State::Aim
            | State::Casting
            | State::Floating
            | State::Nibble
            | State::Bite
            | State::Reeling
            | State::Banner
    ) {
        return;
    }
    let hud_r = Rect::new(HUD_MENU_X, HUD_Y, HUD_INFO_X + HUD_INFO_W - HUD_MENU_X, HUD_H);
    if !hud_r.intersects(clip) {
        return;
    }
    let mut c = Clip { s, r: clip };
    rrect(&mut c, HUD_MENU_X, HUD_Y, HUD_MENU_W, HUD_H, 12, PILL_BORDER, scene_rgb(HUD_MENU_X, HUD_Y));
    rrect(&mut c, HUD_MENU_X + 2, HUD_Y + 2, HUD_MENU_W - 4, HUD_H - 4, 10, PILL_BG, PILL_BORDER);
    FONT_SM.centre(&mut c, HUD_MENU_X + HUD_MENU_W / 2, HUD_Y + 6, "< MENU", WHITE, PILL_BG);

    rrect(&mut c, HUD_INFO_X, HUD_Y, HUD_INFO_W, HUD_H, 12, PILL_BORDER, scene_rgb(HUD_INFO_X, HUD_Y));
    rrect(&mut c, HUD_INFO_X + 2, HUD_Y + 2, HUD_INFO_W - 4, HUD_H - 4, 10, PILL_BG, PILL_BORDER);

    let mut line_buf = [0u8; 24];
    let mut len = 0usize;
    let push = |buf: &mut [u8], len: &mut usize, bytes: &[u8]| {
        let n = bytes.len().min(buf.len() - *len);
        buf[*len..*len + n].copy_from_slice(&bytes[..n]);
        *len += n;
    };
    let mut db = [0u8; 11];
    push(&mut line_buf, &mut len, g.difficulty.short().as_bytes());
    push(&mut line_buf, &mut len, b" ");
    push(&mut line_buf, &mut len, int(g.score as i32, &mut db).as_bytes());
    push(&mut line_buf, &mut len, b"P");
    match g.mode {
        Mode::Relax => {
            push(&mut line_buf, &mut len, b" B:");
            push(&mut line_buf, &mut len, int(best.relax_best as i32, &mut db).as_bytes());
        }
        Mode::Derby => {
            let secs = (g.derby_left_ms + 999) / 1000;
            push(&mut line_buf, &mut len, b" ");
            push(&mut line_buf, &mut len, int(secs as i32, &mut db).as_bytes());
            push(&mut line_buf, &mut len, b"S");
        }
    }
    let text = core::str::from_utf8(&line_buf[..len]).unwrap_or("0P");
    let timer_low = g.mode == Mode::Derby && g.derby_left_ms <= 10_000;
    let fg = if timer_low { Rgb(255, 118, 96) } else { WHITE };
    FONT_SM.centre(&mut c, HUD_INFO_X + HUD_INFO_W / 2, HUD_Y + 6, text, fg, PILL_BG);
}

fn draw_prompt_pill(s: &mut impl Surface, g: &Game, clip: Rect) {
    if !matches!(g.state, State::Aim | State::Reeling) || (g.state == State::Aim && g.has_cast) {
        return;
    }
    let pr = Rect::new(132, 68, 202, 24);
    if !pr.intersects(clip) {
        return;
    }
    let mut c = Clip { s, r: clip };
    let msg = if g.state == State::Reeling {
        "CROWN: MOVE BAR"
    } else {
        "TAP WATER TO CAST"
    };
    let rim = if g.state == State::Reeling { PILL_ACTIVE_RIM } else { PILL_BORDER };
    rrect(&mut c, 132, 68, 202, 24, 10, rim, scene_rgb(132, 68));
    rrect(&mut c, 134, 70, 198, 20, 8, PILL_BG, rim);
    FONT_SM.centre(&mut c, C, 73, msg, WHITE, PILL_BG);
}

fn draw_option_pills(
    s: &mut impl Surface,
    active_idx: usize,
    confirming: bool,
    items: &[(i32, &str, &str, Rgb); 3],
) {
    for (idx, &(y, title, sub, accent)) in items.iter().enumerate() {
        let active = active_idx == idx;
        let lit = active && confirming;
        let fill = if lit {
            PILL_ACTIVE_BG.mix(accent, ONE * 36 / 100)
        } else if active {
            PILL_ACTIVE_BG
        } else {
            PILL_BG
        };
        let rim = if lit {
            accent.mix(WHITE, ONE / 2)
        } else if active {
            accent
        } else {
            PILL_BORDER
        };
        rrect(s, MENU_BTN_X, y, MENU_BTN_W, MENU_BTN_H, 16, rim, scene_rgb(MENU_BTN_X, y));
        rrect(s, MENU_BTN_X + 2, y + 2, MENU_BTN_W - 4, MENU_BTN_H - 4, 14, fill, rim);
        if active {
            let bar_col = if lit { WHITE } else { accent };
            s.rect(MENU_BTN_X + 12, y + 12, 5, MENU_BTN_H - 24, bar_col.rgb565());
        }
        let tcol = if lit || !active { WHITE } else { accent };
        let scol = if lit { WHITE.mix(DIM_TEXT, ONE / 2) } else { DIM_TEXT };
        FONT_MD.centre(s, C, y + 7, title, tcol, fill);
        FONT_SM.centre(s, C, y + 31, sub, scol, fill);
    }
}

fn draw_menu_overlay(s: &mut impl Surface, g: &Game, best: Records) {
    let under = |px: i32, py: i32| scene_rgb(px.clamp(0, SIZE - 1), py.clamp(0, SIZE - 1));
    FONT_LG.centre_over(s, C + 2, 34, "LAKE", SHADOW_TEXT, &under);
    FONT_LG.centre_over(s, C, 32, "LAKE", WHITE, &under);
    FONT_MD.centre_over(s, C + 1, 73, "CAST", SHADOW_TEXT, &under);
    FONT_MD.centre_over(s, C, 72, "CAST", Rgb(255, 224, 108), &under);

    let mut b0 = [0u8; 20];
    let mut b1 = [0u8; 20];
    let mut b2 = [0u8; 20];
    let mut db = [0u8; 11];
    let fmt_sub = |prefix: &[u8], val: i32, suffix: &[u8], out: &mut [u8; 20], db: &mut [u8; 11]| -> usize {
        let mut i = 0;
        let pn = prefix.len().min(out.len());
        out[..pn].copy_from_slice(&prefix[..pn]);
        i += pn;
        let v = int(val, db).as_bytes();
        let vn = v.len().min(out.len() - i);
        out[i..i + vn].copy_from_slice(&v[..vn]);
        i += vn;
        let sn = suffix.len().min(out.len() - i);
        out[i..i + sn].copy_from_slice(&suffix[..sn]);
        i + sn
    };
    let l0 = fmt_sub(b"BEST ", best.relax_best as i32, b" PTS", &mut b0, &mut db);
    let l1 = fmt_sub(b"BEST ", best.derby_best as i32, b" PTS", &mut b1, &mut db);
    let l2 = fmt_sub(b"", best.species_unlocked() as i32, b"/6 SPECIES", &mut b2, &mut db);

    let items = [
        (MENU_RELAX_Y, "RELAX", core::str::from_utf8(&b0[..l0]).unwrap_or(""), PILL_ACTIVE_RIM),
        (MENU_DERBY_Y, "DERBY 90S", core::str::from_utf8(&b1[..l1]).unwrap_or(""), PILL_ACTIVE_RIM),
        (MENU_TROPHY_Y, "TROPHIES", core::str::from_utf8(&b2[..l2]).unwrap_or(""), PILL_ACTIVE_RIM),
    ];

    draw_option_pills(s, g.menu_sel, g.confirm_ms > 0, &items);
    FONT_SM.centre_over(s, C, 372, "CROWN OR TAP", Rgb(62, 42, 20), &under);
}

fn draw_difficulty_overlay(s: &mut impl Surface, g: &Game) {
    let under = |px: i32, py: i32| scene_rgb(px.clamp(0, SIZE - 1), py.clamp(0, SIZE - 1));
    rrect(s, HUD_MENU_X, HUD_Y, HUD_MENU_W, HUD_H, 12, PILL_BORDER, scene_rgb(HUD_MENU_X, HUD_Y));
    rrect(s, HUD_MENU_X + 2, HUD_Y + 2, HUD_MENU_W - 4, HUD_H - 4, 10, PILL_BG, PILL_BORDER);
    FONT_SM.centre(s, HUD_MENU_X + HUD_MENU_W / 2, HUD_Y + 6, "< BACK", WHITE, PILL_BG);

    let mode_label = match g.pending_mode {
        Mode::Relax => "RELAX",
        Mode::Derby => "DERBY 90S",
    };
    rrect(s, HUD_INFO_X, HUD_Y, HUD_INFO_W, HUD_H, 12, PILL_BORDER, scene_rgb(HUD_INFO_X, HUD_Y));
    rrect(s, HUD_INFO_X + 2, HUD_Y + 2, HUD_INFO_W - 4, HUD_H - 4, 10, PILL_BG, PILL_BORDER);
    FONT_SM.centre(s, HUD_INFO_X + HUD_INFO_W / 2, HUD_Y + 6, mode_label, Rgb(255, 224, 108), PILL_BG);

    FONT_MD.centre_over(s, C + 1, 93, "DIFFICULTY", SHADOW_TEXT, &under);
    FONT_MD.centre_over(s, C, 92, "DIFFICULTY", WHITE, &under);

    let ys = [MENU_RELAX_Y, MENU_DERBY_Y, MENU_TROPHY_Y];
    let items = [
        (ys[0], DIFFICULTIES[0].name(), DIFFICULTIES[0].desc(), DIFFICULTIES[0].accent()),
        (ys[1], DIFFICULTIES[1].name(), DIFFICULTIES[1].desc(), DIFFICULTIES[1].accent()),
        (ys[2], DIFFICULTIES[2].name(), DIFFICULTIES[2].desc(), DIFFICULTIES[2].accent()),
    ];
    draw_option_pills(s, g.diff_sel, g.confirm_ms > 0, &items);
    FONT_SM.centre_over(s, C, 372, "CROWN OR TAP", Rgb(62, 42, 20), &under);
}

/// Renders background + swimming fish silhouettes in a SINGLE PASS per pixel (ZERO FLICKER!).
fn paint_water_with_fish_single_pass(s: &mut impl Surface, g: &Game, clip: Rect) {
    if clip.is_empty() {
        return;
    }
    let mut fish_active = [false; MAX_FISH];
    let mut any_fish = false;
    for i in 0..MAX_FISH {
        if fish_bbox(&g.fish[i]).intersects(clip) {
            fish_active[i] = true;
            any_fish = true;
        }
    }
    if !any_fish {
        draw_bg_box(s, clip);
        return;
    }

    for y in clip.y0..clip.y1 {
        let mut run = Run::new(s, y, false);
        for x in clip.x0..clip.x1 {
            let mut px_c = None;
            for i in 0..MAX_FISH {
                if fish_active[i] {
                    if let Some(fc) = fish_pixel_at(&g.fish[i], x, y) {
                        px_c = Some(fc);
                        break;
                    }
                }
            }
            let final_c = px_c.unwrap_or_else(|| scene_rgb(x, y).rgb565());
            run.put(x, 1, final_c);
        }
    }
}

fn paint_pond_region(s: &mut impl Surface, g: &Game, best: Records, clip: Rect) {
    if clip.is_empty() {
        return;
    }
    paint_water_with_fish_single_pass(s, g, clip);

    let (rod_target_x, rod_target_y) = if g.has_cast { (g.bx, g.by) } else { (186, 274) };
    let bend = match g.state {
        State::Reeling => 64,
        State::Bite => 48,
        _ => 0,
    };
    let pose = rod_pose(rod_target_x, rod_target_y, bend);
    let (bob_x, bob_y) = g.bobber_pos();
    if rod_bounds(pose, bob_x, bob_y).intersects(clip) {
        let show_line = g.has_cast && g.state != State::Banner;
        let taut = matches!(g.state, State::Casting | State::Bite | State::Reeling);
        draw_rod_and_line(s, pose, bob_x, bob_y, show_line, taut, clip);
    }
    draw_bobber_and_fx(s, g, clip);
    draw_prompt_pill(s, g, clip);
    draw_hud(s, g, best, clip);
}

fn draw_weight_lg(s: &mut impl Surface, cx: i32, y: i32, hg: u16, fg: Rgb, bg: Rgb) {
    let mut wb = [0u8; 16];
    let w_str = format_weight(hg, &mut wb);
    let Some(dot_pos) = w_str.find('.') else {
        FONT_LG.centre(s, cx, y, w_str, fg, bg);
        return;
    };
    let left = &w_str[..dot_pos];
    let right = &w_str[dot_pos + 1..];
    let lw = FONT_LG.width(left);
    let rw = FONT_LG.width(right);
    const DOT_ADV: i32 = 11;
    let x0 = cx - (lw + DOT_ADV + rw) / 2;
    FONT_LG.draw(s, x0, y, left, fg, bg);
    let dx = x0 + lw + 3;
    let dy = y + 26;
    let fg_c = fg.rgb565();
    let aa_c = bg.mix(fg, ONE / 2).rgb565();
    s.rect(dx + 1, dy, 3, 5, fg_c);
    s.rect(dx, dy + 1, 5, 3, fg_c);
    s.rect(dx, dy, 1, 1, aa_c);
    s.rect(dx + 4, dy, 1, 1, aa_c);
    s.rect(dx, dy + 4, 1, 1, aa_c);
    s.rect(dx + 4, dy + 4, 1, 1, aa_c);
    FONT_LG.draw(s, x0 + lw + DOT_ADV, y, right, fg, bg);
}

fn draw_caught_modal(s: &mut impl Surface, g: &Game) {
    let sp = g.caught_species;
    rrect(s, CAUGHT_X, CAUGHT_Y, CAUGHT_W, CAUGHT_H, 24, CARD_BORDER, scene_rgb(CAUGHT_X, CAUGHT_Y));
    rrect(
        s,
        CAUGHT_X + 3,
        CAUGHT_Y + 3,
        CAUGHT_W - 6,
        CAUGHT_H - 6,
        21,
        CARD_BG,
        CARD_BORDER,
    );

    let top_badge = if g.caught_record { "NEW RECORD!" } else { sp.rarity() };
    let badge_col = if g.caught_record { Rgb(255, 212, 68) } else { DIM_TEXT };
    FONT_SM.centre(s, C, CAUGHT_Y + 16, top_badge, badge_col, CARD_BG);

    let sprite = sp.sprite();
    let sc = 3;
    let fx = C - (sprite.w * sc) / 2;
    let fy = CAUGHT_Y + 52 + (54 - sprite.h * sc) / 2;
    draw_fish(s, sprite, fx, fy, sc, None);

    FONT_MD.centre(s, C, CAUGHT_Y + 124, sp.name(), sp.accent(), CARD_BG);
    draw_weight_lg(s, C, CAUGHT_Y + 158, g.caught_hg, WHITE, CARD_BG);

    let mut pb = [0u8; 16];
    let mut db = [0u8; 11];
    pb[0] = b'+';
    let p_bytes = int(g.caught_pts as i32, &mut db).as_bytes();
    let pn = p_bytes.len().min(8);
    pb[1..1 + pn].copy_from_slice(&p_bytes[..pn]);
    pb[1 + pn..5 + pn].copy_from_slice(b" PTS");
    let pts_str = core::str::from_utf8(&pb[..5 + pn]).unwrap_or("+0 PTS");
    FONT_MD.centre(s, C, CAUGHT_Y + 196, pts_str, Rgb(106, 240, 186), CARD_BG);
    rrect(s, CAUGHT_BTN_X, CAUGHT_BTN_Y, CAUGHT_BTN_W, CAUGHT_BTN_H, 14, CARD_BORDER, CARD_BG);
    rrect(
        s,
        CAUGHT_BTN_X + 2,
        CAUGHT_BTN_Y + 2,
        CAUGHT_BTN_W - 4,
        CAUGHT_BTN_H - 4,
        12,
        PILL_ACTIVE_BG,
        CARD_BORDER,
    );
    FONT_SM.centre(s, C, CAUGHT_BTN_Y + 11, "CONTINUE", WHITE, PILL_ACTIVE_BG);
}

fn draw_trophies_screen(s: &mut impl Surface, g: &Game, best: Records) {
    s.rect(0, 0, SIZE, SIZE, TROPHY_BG.rgb565());
    rrect(s, HUD_MENU_X, HUD_Y, HUD_MENU_W, HUD_H, 12, PILL_BORDER, TROPHY_BG);
    rrect(s, HUD_MENU_X + 2, HUD_Y + 2, HUD_MENU_W - 4, HUD_H - 4, 10, PILL_BG, PILL_BORDER);
    FONT_SM.centre(s, HUD_MENU_X + HUD_MENU_W / 2, HUD_Y + 6, "< MENU", WHITE, PILL_BG);

    let mut hb = [0u8; 16];
    hb[..9].copy_from_slice(b"TROPHIES ");
    hb[9] = b'0' + best.species_unlocked() as u8;
    hb[10..12].copy_from_slice(b"/6");
    let hdr = core::str::from_utf8(&hb[..12]).unwrap_or("TROPHIES");
    FONT_MD.centre(s, 272, HUD_Y + 4, hdr, Rgb(255, 224, 108), TROPHY_BG);

    let idx = g.trophy_idx.min(5);
    let sp = SPECIES_ALL[idx];
    let count = best.species_count[idx];
    let unlocked = count > 0;

    let (cx, cy, cw, ch) = (114, 92, 238, 268);
    let rim = if unlocked { sp.accent() } else { PILL_BORDER };
    rrect(s, cx, cy, cw, ch, 22, rim, TROPHY_BG);
    rrect(s, cx + 2, cy + 2, cw - 4, ch - 4, 20, PILL_BG, rim);

    FONT_SM.centre(s, C, cy + 16, sp.rarity(), if unlocked { sp.accent() } else { DIM_TEXT }, PILL_BG);

    let sprite = sp.sprite();
    let sc = 3;
    let fx = C - (sprite.w * sc) / 2;
    let fy = cy + 54 + (54 - sprite.h * sc) / 2;
    draw_fish(s, sprite, fx, fy, sc, if unlocked { None } else { Some(SILHOUETTE) });

    if unlocked {
        FONT_MD.centre(s, C, cy + 128, sp.name(), WHITE, PILL_BG);
        let mut wb = [0u8; 16];
        let mut rb = [0u8; 22];
        rb[..5].copy_from_slice(b"BEST ");
        let ws = format_weight(best.species_best_hg[idx], &mut wb).as_bytes();
        rb[5..5 + ws.len()].copy_from_slice(ws);
        let r_str = core::str::from_utf8(&rb[..5 + ws.len()]).unwrap_or("BEST 0");
        FONT_MD.centre(s, C, cy + 164, r_str, Rgb(106, 240, 186), PILL_BG);

        let mut cb = [0u8; 18];
        let mut db = [0u8; 11];
        cb[..7].copy_from_slice(b"CAUGHT ");
        let cs = int(count as i32, &mut db).as_bytes();
        let cn = cs.len().min(8);
        cb[7..7 + cn].copy_from_slice(&cs[..cn]);
        let c_str = core::str::from_utf8(&cb[..7 + cn]).unwrap_or("CAUGHT 0");
        FONT_SM.centre(s, C, cy + 206, c_str, DIM_TEXT, PILL_BG);
    } else {
        FONT_MD.centre(s, C, cy + 132, "MYSTERY FISH", SILHOUETTE.mix(WHITE, 420), PILL_BG);
        FONT_SM.centre(s, C, cy + 178, "NOT CAUGHT YET", DIM_TEXT, PILL_BG);
    }

    for (bx, sym) in [(TROPHY_PREV_X, "<"), (TROPHY_NEXT_X, ">")] {
        rrect(s, bx, TROPHY_NAV_Y, TROPHY_NAV_W, TROPHY_NAV_H, 14, PILL_BORDER, TROPHY_BG);
        rrect(
            s,
            bx + 2,
            TROPHY_NAV_Y + 2,
            TROPHY_NAV_W - 4,
            TROPHY_NAV_H - 4,
            12,
            PILL_BG,
            PILL_BORDER,
        );
        FONT_MD.centre(s, bx + TROPHY_NAV_W / 2, TROPHY_NAV_Y + 18, sym, WHITE, PILL_BG);
    }

    let dot_start = C - (5 * 22) / 2;
    for i in 0..6 {
        let dx = dot_start + i as i32 * 22;
        let col = if i == idx {
            Rgb(255, 212, 76)
        } else if best.species_count[i] > 0 {
            Rgb(82, 188, 198)
        } else {
            Rgb(38, 72, 102)
        };
        s.rect(dx - 4, 384, 8, 8, col.rgb565());
    }
    FONT_SM.centre(s, C, 404, "CROWN TO BROWSE", DIM_TEXT, TROPHY_BG);
}

fn draw_derby_over(s: &mut impl Surface, g: &Game) {
    draw_bg_box(s, Rect::new(0, 0, SIZE, SIZE));
    rrect(s, 78, 66, 310, 344, 24, CARD_BORDER, scene_rgb(78, 66));
    rrect(s, 81, 69, 304, 338, 21, CARD_BG, CARD_BORDER);

    FONT_LG.centre(s, C, 84, "TIME UP!", WHITE, CARD_BG);
    if g.new_best_score {
        FONT_SM.centre(s, C, 128, "NEW DERBY BEST!", Rgb(255, 212, 72), CARD_BG);
    }

    let mut sb = [0u8; 18];
    let mut db = [0u8; 11];
    let s_bytes = int(g.score as i32, &mut db).as_bytes();
    let sn = s_bytes.len().min(10);
    sb[..sn].copy_from_slice(&s_bytes[..sn]);
    sb[sn..sn + 4].copy_from_slice(b" PTS");
    let score_str = core::str::from_utf8(&sb[..sn + 4]).unwrap_or("0 PTS");
    FONT_LG.centre(s, C, 154, score_str, Rgb(106, 240, 186), CARD_BG);

    let mut cb = [0u8; 20];
    cb[..5].copy_from_slice(b"FISH ");
    let c_bytes = int(g.session_catches as i32, &mut db).as_bytes();
    let cn = c_bytes.len().min(6);
    cb[5..5 + cn].copy_from_slice(&c_bytes[..cn]);
    let catch_str = core::str::from_utf8(&cb[..5 + cn]).unwrap_or("FISH 0");
    FONT_SM.centre(s, C, 206, catch_str, DIM_TEXT, CARD_BG);

    if let Some(sp) = g.session_best_species {
        let mut wb = [0u8; 16];
        let mut bb = [0u8; 28];
        let name = sp.name().as_bytes();
        let nn = name.len().min(14);
        bb[..nn].copy_from_slice(&name[..nn]);
        bb[nn] = b' ';
        let ws = format_weight(g.session_best_hg, &mut wb).as_bytes();
        bb[nn + 1..nn + 1 + ws.len()].copy_from_slice(ws);
        let best_line = core::str::from_utf8(&bb[..nn + 1 + ws.len()]).unwrap_or("");
        FONT_SM.centre(s, C, 232, best_line, sp.accent(), CARD_BG);
    }

    let btns = [(OVER_RETRY_Y, "RETRY"), (OVER_MENU_Y, "MENU")];
    let confirming = g.confirm_ms > 0;
    for (idx, &(y, label)) in btns.iter().enumerate() {
        let active = g.over_sel == idx;
        let lit = active && confirming;
        let fill = if lit {
            PILL_ACTIVE_BG.mix(PILL_ACTIVE_RIM, ONE * 36 / 100)
        } else if active {
            PILL_ACTIVE_BG
        } else {
            PILL_BG
        };
        let rim = if lit {
            PILL_ACTIVE_RIM.mix(WHITE, ONE / 2)
        } else if active {
            PILL_ACTIVE_RIM
        } else {
            PILL_BORDER
        };
        rrect(s, OVER_BTN_X, y, OVER_BTN_W, OVER_BTN_H, 16, rim, CARD_BG);
        rrect(s, OVER_BTN_X + 2, y + 2, OVER_BTN_W - 4, OVER_BTN_H - 4, 14, fill, rim);
        let tcol = if lit || !active { WHITE } else { PILL_ACTIVE_RIM };
        FONT_MD.centre(s, C, y + 12, label, tcol, fill);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Seen {
    state: State,
    mode: Mode,
    difficulty: Difficulty,
    menu_sel: usize,
    diff_sel: usize,
    trophy_idx: usize,
    over_sel: usize,
    confirming: bool,
    score: u32,
    derby_sec: u32,
    fish_boxes: [Rect; MAX_FISH],
    fish_dirs: [bool; MAX_FISH],
    fish_phases: [u16; MAX_FISH],
    has_cast: bool,
    rod_box: Rect,
    rod_sig: (i32, i32, bool, bool),
    bob_box: Rect,
    bob_x: i32,
    bob_y: i32,
    lake_bucket: u32,
    bar_y: i32,
    bar_h: i32,
    track_fish_y: i32,
    fish_in_bar: bool,
    prog_h: i32,
    prog_bucket: u8,
}

fn snapshot(g: &Game) -> Seen {
    let mut fish_boxes = [Rect::empty(); MAX_FISH];
    let mut fish_dirs = [true; MAX_FISH];
    let mut fish_phases = [0u16; MAX_FISH];
    for i in 0..MAX_FISH {
        fish_boxes[i] = fish_bbox(&g.fish[i]);
        fish_dirs[i] = g.fish[i].facing_right();
        fish_phases[i] = g.fish[i].phase_ms / 90;
    }
    let (bob_x, bob_y) = g.bobber_pos();
    let (rx, ry) = if g.has_cast { (g.bx, g.by) } else { (186, 274) };
    let bend = match g.state {
        State::Reeling => 64,
        State::Bite => 48,
        _ => 0,
    };
    let pose = rod_pose(rx, ry, bend);
    Seen {
        state: g.state,
        mode: g.mode,
        difficulty: g.difficulty,
        menu_sel: g.menu_sel,
        diff_sel: g.diff_sel,
        trophy_idx: g.trophy_idx,
        over_sel: g.over_sel,
        confirming: g.confirm_ms > 0,
        score: g.score,
        derby_sec: (g.derby_left_ms + 999) / 1000,
        fish_boxes,
        fish_dirs,
        fish_phases,
        has_cast: g.has_cast,
        rod_box: rod_bounds(pose, bob_x, bob_y),
        rod_sig: (
            pose.tip_x,
            pose.tip_y,
            g.has_cast && g.state != State::Banner,
            matches!(g.state, State::Casting | State::Bite | State::Reeling),
        ),
        bob_box: if g.has_cast && g.state != State::Menu || g.state == State::Banner {
            bobber_bounds(bob_x, bob_y, g.state, g.splash_ms)
        } else {
            Rect::empty()
        },
        bob_x,
        bob_y,
        lake_bucket: {
            let splash_b = if g.splash_ms > 0 {
                1 + (520 - g.splash_ms.min(520)) / 20
            } else {
                0
            };
            let phase_b = match g.state {
                State::Floating => (g.lake_ms / 120) & 0xFFFF,
                State::Reeling => (g.reel_phase_ms / 140) & 0xFFFF,
                _ => 0,
            };
            (splash_b << 16) | phase_b
        },
        bar_y: g.bar_y,
        bar_h: g.bar_h,
        track_fish_y: g.track_fish_y(),
        fish_in_bar: g.fish_in_bar,
        prog_h: (g.reel_progress.clamp(0, 1000) * TRACK_H / 1000).clamp(0, TRACK_H),
        prog_bucket: prog_color(g.reel_progress).0,
    }
}

fn paint_pond_excluding(s: &mut impl Surface, g: &Game, best: Records, r: Rect, ex: Option<Rect>) {
    let Some(ex) = ex.filter(|e| r.intersects(*e)) else {
        paint_pond_region(s, g, best, r);
        return;
    };
    let my0 = r.y0.max(ex.y0);
    let my1 = r.y1.min(ex.y1);
    let parts = [
        Rect { x0: r.x0, y0: r.y0, x1: r.x1, y1: ex.y0.min(r.y1) },
        Rect { x0: r.x0, y0: ex.y1.max(r.y0), x1: r.x1, y1: r.y1 },
        Rect { x0: r.x0, y0: my0, x1: ex.x0.min(r.x1), y1: my1 },
        Rect { x0: ex.x1.max(r.x0), y0: my0, x1: r.x1, y1: my1 },
    ];
    for p in parts {
        if !p.is_empty() {
            paint_pond_region(s, g, best, p);
        }
    }
}

fn paint_fish_deltas(s: &mut impl Surface, g: &Game, best: Records, old: &Seen, cur: &Seen) -> bool {
    let mut pending = [Rect::empty(); MAX_FISH];
    let mut count = 0usize;
    for i in 0..MAX_FISH {
        if old.fish_boxes[i] != cur.fish_boxes[i]
            || old.fish_dirs[i] != cur.fish_dirs[i]
            || old.fish_phases[i] != cur.fish_phases[i]
        {
            pending[count] = old.fish_boxes[i].union(cur.fish_boxes[i]);
            count += 1;
        }
    }
    if count == 0 {
        return false;
    }
    loop {
        let mut merged = false;
        'outer: for i in 0..count {
            if pending[i].is_empty() {
                continue;
            }
            for j in (i + 1)..count {
                if !pending[j].is_empty() && pending[i].intersects(pending[j]) {
                    pending[i] = pending[i].union(pending[j]);
                    pending[j] = Rect::empty();
                    merged = true;
                    break 'outer;
                }
            }
        }
        if !merged {
            break;
        }
    }
    for r in &pending[..count] {
        if !r.is_empty() {
            paint_pond_region(s, g, best, *r);
        }
    }
    true
}

/// Renders full screen on initial draw or tiny separated single-pass dirty regions!
pub fn frame(s: &mut impl Surface, g: &Game, best: Records, prev: Option<Seen>) -> (Seen, bool) {
    let cur = snapshot(g);
    let Some(old) = prev else {
        match g.state {
            State::Trophies => draw_trophies_screen(s, g, best),
            State::Caught => {
                paint_pond_region(s, g, best, Rect::new(0, 0, SIZE, SIZE));
                draw_caught_modal(s, g);
            }
            State::DerbyOver => draw_derby_over(s, g),
            State::Menu => {
                paint_water_with_fish_single_pass(s, g, Rect::new(0, 0, SIZE, SIZE));
                let pose = rod_pose(186, 274, 0);
                draw_rod_and_line(s, pose, 186, 274, false, false, Rect::new(0, 0, SIZE, SIZE));
                draw_menu_overlay(s, g, best);
            }
            State::Difficulty => {
                paint_water_with_fish_single_pass(s, g, Rect::new(0, 0, SIZE, SIZE));
                let pose = rod_pose(186, 274, 0);
                draw_rod_and_line(s, pose, 186, 274, false, false, Rect::new(0, 0, SIZE, SIZE));
                draw_difficulty_overlay(s, g);
            }
            State::Reeling => {
                paint_pond_region(s, g, best, Rect::new(0, 0, SIZE, SIZE));
                draw_gauge_frame(s);
                draw_gauge_inner(s, g, None);
            }
            _ => {
                paint_pond_region(s, g, best, Rect::new(0, 0, SIZE, SIZE));
            }
        }
        return (cur, true);
    };

    if old.state != cur.state {
        if cur.state == State::Menu
            || old.state == State::Menu
            || cur.state == State::Difficulty
            || old.state == State::Difficulty
            || matches!(cur.state, State::Trophies | State::Caught | State::DerbyOver)
            || matches!(old.state, State::Trophies | State::Caught | State::DerbyOver)
        {
            return frame(s, g, best, None);
        }
    }

    if cur.state == State::Menu {
        if old.menu_sel != cur.menu_sel || old.confirming != cur.confirming {
            draw_menu_overlay(s, g, best);
            return (cur, true);
        }
        return (cur, false);
    }
    if cur.state == State::Difficulty {
        if old.diff_sel != cur.diff_sel || old.confirming != cur.confirming {
            draw_difficulty_overlay(s, g);
            return (cur, true);
        }
        return (cur, false);
    }
    if cur.state == State::Trophies {
        if old.trophy_idx != cur.trophy_idx {
            draw_trophies_screen(s, g, best);
            return (cur, true);
        }
        return (cur, false);
    }
    if cur.state == State::DerbyOver {
        if old.over_sel != cur.over_sel || old.confirming != cur.confirming {
            draw_derby_over(s, g);
            return (cur, true);
        }
        return (cur, false);
    }
    if cur.state == State::Caught {
        return (cur, false);
    }

    let mut drew = false;
    let gauge_rect = Rect::new(GAUGE_X, GAUGE_Y, GAUGE_W, GAUGE_H);
    let avoid_gauge = (cur.state == State::Reeling).then_some(gauge_rect);

    // 1. State transition updates on pond & left gauge visibility (pond cleared BEFORE gauge is drawn!).
    if old.state != cur.state {
        if old.state == State::Aim && !old.has_cast || old.state == State::Reeling || cur.state == State::Reeling {
            paint_pond_excluding(s, g, best, Rect::new(132, 68, 202, 24), avoid_gauge);
        }
        let u = old.rod_box.union(cur.rod_box).union(old.bob_box).union(cur.bob_box);
        paint_pond_excluding(s, g, best, u, avoid_gauge);
        if cur.state == State::Reeling {
            for i in 0..MAX_FISH {
                if old.fish_boxes[i] != cur.fish_boxes[i] {
                    paint_pond_excluding(s, g, best, old.fish_boxes[i].union(cur.fish_boxes[i]), avoid_gauge);
                }
            }
            draw_gauge_frame(s);
            draw_gauge_inner(s, g, None);
        } else if old.state == State::Reeling {
            paint_pond_region(s, g, best, gauge_rect);
        }
        drew = true;
    }

    // 2. Top HUD update (only when score, mode, difficulty, or timer second changes!).
    if old.score != cur.score
        || old.derby_sec != cur.derby_sec
        || old.difficulty != cur.difficulty
        || old.mode != cur.mode
    {
        paint_pond_excluding(
            s,
            g,
            best,
            Rect::new(HUD_MENU_X, HUD_Y, HUD_INFO_X + HUD_INFO_W - HUD_MENU_X, HUD_H),
            avoid_gauge,
        );
        drew = true;
    }

    // 3. Individual pond fish boxes in ZERO-FLICKER disjoint single-pass render!
    if cur.state != State::Reeling && paint_fish_deltas(s, g, best, &old, &cur) {
        drew = true;
    }

    // 4. Rod & bobber updates (strictly outside gauge during Reeling).
    let rod_changed = old.has_cast != cur.has_cast || old.rod_box != cur.rod_box || old.rod_sig != cur.rod_sig;
    let bob_changed = old.bob_box != cur.bob_box
        || old.bob_x != cur.bob_x
        || old.bob_y != cur.bob_y
        || old.lake_bucket != cur.lake_bucket;
    if rod_changed || bob_changed {
        let mut r = Rect::empty();
        if rod_changed {
            r = r.union(old.rod_box.union(cur.rod_box));
        }
        if bob_changed {
            r = r.union(old.bob_box.union(cur.bob_box));
        }
        paint_pond_excluding(s, g, best, r, avoid_gauge);
        drew = true;
    }

    // 5. Zero-flicker incremental single-pass vertical gauge interior update during Reeling!
    if cur.state == State::Reeling
        && old.state == State::Reeling
        && (old.bar_y != cur.bar_y
            || old.bar_h != cur.bar_h
            || old.track_fish_y != cur.track_fish_y
            || old.fish_in_bar != cur.fish_in_bar
            || old.prog_h != cur.prog_h
            || old.prog_bucket != cur.prog_bucket)
    {
        draw_gauge_inner(s, g, Some(old));
        drew = true;
    }

    (cur, drew)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmf_wp2_ui::{Count, Fb};

    #[test]
    fn partial_frames_match_full_render() {
        let mut g = Game::new(77);
        let mut rec = Records::default();
        let mut px_full = vec![0u16; (SIZE * SIZE) as usize];
        let mut px_part = vec![0u16; (SIZE * SIZE) as usize];

        let (mut seen, _) = frame(&mut Fb::new(&mut px_part), &g, rec, None);

        let check_sync = |g: &Game, rec: Records, seen: &mut Seen, px_part: &mut [u16], px_full: &mut [u16]| {
            let (ns, _) = frame(&mut Fb::new(px_part), g, rec, Some(*seen));
            *seen = ns;
            frame(&mut Fb::new(px_full), g, rec, None);
            if px_part != px_full {
                let first = px_part.iter().zip(px_full.iter()).position(|(a, b)| a != b).unwrap();
                panic!(
                    "partial frame drifted at state {:?} pixel ({}, {}): part={:#06x} full={:#06x}",
                    g.state,
                    first as i32 % SIZE,
                    first as i32 / SIZE,
                    px_part[first],
                    px_full[first]
                );
            }
        };

        g.crown(1);
        check_sync(&g, rec, &mut seen, &mut px_part, &mut px_full);
        g.crown(-1);
        check_sync(&g, rec, &mut seen, &mut px_part, &mut px_full);

        g.open_difficulty(Mode::Relax);
        check_sync(&g, rec, &mut seen, &mut px_part, &mut px_full);
        g.crown(1);
        check_sync(&g, rec, &mut seen, &mut px_part, &mut px_full);
        g.crown(-1);
        check_sync(&g, rec, &mut seen, &mut px_part, &mut px_full);

        g.tap_ui(233, MENU_RELAX_Y + 10);
        check_sync(&g, rec, &mut seen, &mut px_part, &mut px_full);
        g.tick(80, &mut rec);
        check_sync(&g, rec, &mut seen, &mut px_part, &mut px_full);
        g.tick(80, &mut rec);
        check_sync(&g, rec, &mut seen, &mut px_part, &mut px_full);

        g.start_with_difficulty(Mode::Relax, Difficulty::Medium);
        check_sync(&g, rec, &mut seen, &mut px_part, &mut px_full);

        // Cast right next to the left gauge (`CAST_X_MIN = 106`) so overlap guards are exercised bit-for-bit.
        g.cast_to(106, 190);
        for _ in 0..160 {
            g.tick(40, &mut rec);
            check_sync(&g, rec, &mut seen, &mut px_part, &mut px_full);
            if g.state == State::Bite {
                break;
            }
        }
        assert_eq!(g.state, State::Bite);
        g.try_hook();
        check_sync(&g, rec, &mut seen, &mut px_part, &mut px_full);

        for _ in 0..45 {
            let fy = g.track_fish_y();
            let center = g.bar_y + g.bar_h / 2;
            if fy < center - 6 {
                g.crown(-1);
            } else if fy > center + 6 {
                g.crown(1);
            }
            if g.state == State::Reeling {
                g.tick(40, &mut rec);
            }
            check_sync(&g, rec, &mut seen, &mut px_part, &mut px_full);
            if g.state == State::Caught {
                break;
            }
        }
    }

    #[test]
    fn active_reeling_and_fish_frames_are_tiny_and_flicker_free() {
        let mut g = Game::new(19);
        let mut rec = Records::default();
        g.start_mode(Mode::Relax);
        g.cast_to(220, 190);
        while g.state != State::Bite {
            g.tick(40, &mut rec);
        }
        g.try_hook();

        let mut dummy = vec![0u16; (SIZE * SIZE) as usize];
        let (mut seen, _) = frame(&mut Fb::new(&mut dummy), &g, rec, None);

        for _ in 0..20 {
            g.crown(-1);
            g.tick(40, &mut rec);
            let mut fc = Count::default();
            let (ns, _) = frame(&mut fc, &g, rec, Some(seen));
            seen = ns;
            if g.state == State::Reeling {
                assert!(
                    fc.rects < 220 && fc.px < 12_000,
                    "reeling frame too heavy: {} rects, {} px",
                    fc.rects,
                    fc.px
                );
            }
        }
    }
}
