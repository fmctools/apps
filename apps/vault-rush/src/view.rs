//! Steel safe, super-dark dial wheel, bank entrance fly-in, police siren, jail bars scene, and 4-bpp fonts.

use crate::font::{FONT_LG, FONT_MD, FONT_SM};
use crate::game::{
    DIAL_STEPS, Dir, Flash, Game, HEIST_TIMES_S, Mode, OVER_ANIM_MS, OVER_SLAM_MS, Records, State,
    Zone, heist_pins_for_idx,
};
use cmf_wp2_ui::draw::{Band, Paint, disc, gradient, line, radial_n, ring, rrect, stroke};
use cmf_wp2_ui::motion::{cos, ease_out, lerp, sin};
use cmf_wp2_ui::text::int;
use cmf_wp2_ui::{C, ONE, Rgb, SIZE, Surface};

const BG: Rgb = Rgb(4, 5, 8);
const SAFE_BODY: Rgb = Rgb(76, 82, 94);
const SAFE_BODY_EDGE: Rgb = Rgb(118, 126, 142);
const DOOR_TOP: Rgb = Rgb(118, 126, 140);
const DOOR_BOT: Rgb = Rgb(84, 92, 106);
const DOOR_PANEL: Rgb = Rgb(104, 112, 126);
const DOOR_SEAM: Rgb = Rgb(30, 34, 42);
const HINGE: Rgb = Rgb(148, 156, 170);
const BOLT: Rgb = Rgb(192, 200, 214);

// Whole dial wheel rendered in deep dark charcoal so the entire wheel contrasts sharply with the steel door.
const WHEEL_DARK: Rgb = Rgb(8, 10, 14);
const WHEEL_RIM: Rgb = Rgb(18, 21, 27);
const WHEEL_SPOKE: Rgb = Rgb(18, 21, 27);
const WHEEL_HUB: Rgb = Rgb(24, 28, 36);
const WHEEL_CAP: Rgb = Rgb(34, 39, 48);
const WHEEL_PEG: Rgb = Rgb(36, 41, 50);

const CAVITY_BG: Rgb = Rgb(18, 15, 12);
const CAVITY_WALL: Rgb = Rgb(38, 32, 26);
const GOLD_DARK: Rgb = Rgb(188, 124, 8);
const GOLD_MID: Rgb = Rgb(245, 186, 28);
const GOLD_BRIGHT: Rgb = Rgb(255, 232, 118);
const GOLD_SHINE: Rgb = Rgb(255, 250, 214);

// Bank facade colours.
const BANK_WALL: Rgb = Rgb(62, 68, 82);
const BANK_STONE: Rgb = Rgb(92, 100, 118);
const BANK_COL: Rgb = Rgb(118, 126, 146);
const BANK_DOOR: Rgb = Rgb(22, 24, 30);
const BANK_TRIM: Rgb = Rgb(186, 146, 52);

// Jail / busted scene colours.
const CELL_BG: Rgb = Rgb(14, 16, 24);
const SKIN: Rgb = Rgb(212, 174, 142);
const PRISON_A: Rgb = Rgb(168, 174, 186);
const PRISON_B: Rgb = Rgb(56, 62, 74);
const BAR_METAL: Rgb = Rgb(134, 144, 162);
const BAR_SHINE: Rgb = Rgb(196, 206, 222);

const SIREN_RED: Rgb = Rgb(255, 36, 42);
const SIREN_BLUE: Rgb = Rgb(32, 124, 255);

const HUD_BG: Rgb = Rgb(14, 17, 22);
const HUD_RIM: Rgb = Rgb(62, 68, 80);
const TRACK: Rgb = Rgb(38, 43, 52);
const CARD: Rgb = Rgb(26, 30, 38);
const CARD_SEL: Rgb = Rgb(56, 38, 24);
const ACCENT: Rgb = Rgb(255, 92, 0);
const GOLD: Rgb = Rgb(255, 196, 44);
const GREEN: Rgb = Rgb(60, 220, 110);
const RED: Rgb = Rgb(255, 72, 54);
const CYAN: Rgb = Rgb(48, 196, 255);
const WHITE_RGB: Rgb = Rgb(255, 255, 255);
const GREY_RGB: Rgb = Rgb(150, 156, 170);

pub const WHEEL_CY: i32 = 264;
pub const TICK_R0: i32 = 114;
pub const TICK_R1: i32 = 125;
pub const WHEEL_R1: i32 = 98;
#[cfg(test)]
pub const WHEEL_BOX: [i32; 4] = [C - 133, WHEEL_CY - 133, 266, 266];
pub const HUD_BOX: [i32; 4] = [C - 94, 64, 188, 58];
pub const SIREN_R0: i32 = 214;
pub const SIREN_R1: i32 = 233;

// Main menu buttons.
pub const MENU_BTN_X: i32 = C - 104;
pub const MENU_BTN_W: i32 = 208;
pub const MENU_BTN_H: i32 = 52;
pub const MENU_CLASSIC_Y: i32 = 148;
pub const MENU_HEIST_Y: i32 = 210;
pub const MENU_BLITZ_Y: i32 = 272;

// Heist duration grid (2 rows x 3 cols: 10S..60S) + Back button.
pub const HEIST_TILE_W: i32 = 88;
pub const HEIST_TILE_H: i32 = 60;
pub const HEIST_COL_PITCH: i32 = 98;
pub const HEIST_ROW_PITCH: i32 = 72;
pub const HEIST_GRID_X0: i32 = C - HEIST_COL_PITCH - HEIST_TILE_W / 2;
pub const HEIST_GRID_Y0: i32 = 142;
pub const HEIST_BACK_X: i32 = C - 72;
pub const HEIST_BACK_Y: i32 = 336;
pub const HEIST_BACK_W: i32 = 144;
pub const HEIST_BACK_H: i32 = 44;

// End screen buttons.
pub const OVER_BTN_X: i32 = C - 84;
pub const OVER_BTN_W: i32 = 168;
pub const OVER_BTN_H: i32 = 46;
pub const OVER_RETRY_Y: i32 = 282;
pub const OVER_MENU_Y: i32 = 338;

pub fn heist_tile_rect(i: usize) -> [i32; 4] {
    let col = (i % 3) as i32;
    let row = (i / 3) as i32;
    [
        HEIST_GRID_X0 + col * HEIST_COL_PITCH,
        HEIST_GRID_Y0 + row * HEIST_ROW_PITCH,
        HEIST_TILE_W,
        HEIST_TILE_H,
    ]
}

pub fn in_rect(x: i32, y: i32, rx: i32, ry: i32, rw: i32, rh: i32) -> bool {
    x >= rx && x < rx + rw && y >= ry && y < ry + rh
}

pub fn tick_angle(i: i32) -> u16 {
    let i = i.rem_euclid(DIAL_STEPS);
    ((49152i32 + (i * 65536 + DIAL_STEPS / 2) / DIAL_STEPS) & 0xFFFF) as u16
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Seen {
    state: State,
    mode: Mode,
    menu_sel: usize,
    heist_idx: usize,
    over_sel: usize,
    dial: i32,
    zone: Zone,
    armed: bool,
    expected_dir: Dir,
    pin_idx: usize,
    pin_count: usize,
    lives: u8,
    score: u32,
    tenths: i32,
    combo: u32,
    flash: Flash,
    siren: u8,
    anim_step: u32,
    anim_t: i32,
}

fn seen(g: &Game) -> Seen {
    let (anim_step, anim_t) = match g.state {
        State::EnterBank | State::ZoomIn | State::Unlocking | State::Opening => (g.anim_ms / 33, g.anim_t()),
        State::Over => {
            let step = if g.anim_ms < OVER_SLAM_MS + 40 {
                g.anim_ms / 33
            } else if g.anim_ms < OVER_ANIM_MS {
                100 + ((g.anim_ms / 180) & 1)
            } else {
                200
            };
            (step, 0)
        }
        State::ClassicGold | State::HeistWin => (g.anim_ms / 240, 0),
        _ => (0, 0),
    };
    Seen {
        state: g.state,
        mode: g.mode,
        menu_sel: g.menu_sel,
        heist_idx: g.heist_idx,
        over_sel: g.over_sel,
        dial: g.dial,
        zone: g.zone(),
        armed: g.armed(),
        expected_dir: g.expected_dir,
        pin_idx: g.pin_idx,
        pin_count: g.pin_count,
        lives: g.lives,
        score: g.score,
        tenths: (g.time_ms.max(0) + 99) / 100,
        combo: g.combo,
        flash: g.flash,
        siren: g.siren_phase(),
        anim_step,
        anim_t,
    }
}

fn zone_led_color(z: Zone, flash: Flash) -> Rgb {
    match flash {
        Flash::Miss | Flash::WrongDir => RED,
        Flash::PinLock | Flash::ExactBonus | Flash::GoodBonus => GREEN,
        Flash::None => match z {
            Zone::Exact => GREEN,
            Zone::Hot => ACCENT,
            Zone::Warm => GOLD,
            Zone::Cold => Rgb(62, 68, 80),
        },
    }
}

fn format_two_digits(v: i32, out: &mut [u8; 2]) -> &str {
    let v = v.rem_euclid(100);
    out[0] = b'0' + (v / 10) as u8;
    out[1] = b'0' + (v % 10) as u8;
    core::str::from_utf8(out).unwrap_or("00")
}

fn format_tenths<'a>(ms: i32, buf: &'a mut [u8; 16]) -> &'a str {
    let tenths = (ms.max(0) + 99) / 100;
    let secs = tenths / 10;
    let dec = tenths % 10;
    let mut db = [0u8; 11];
    let sb = int(secs, &mut db).as_bytes();
    let sn = sb.len().min(6);
    buf[..sn].copy_from_slice(&sb[..sn]);
    buf[sn] = b'.';
    buf[sn + 1] = b'0' + dec as u8;
    buf[sn + 2] = b'S';
    core::str::from_utf8(&buf[..sn + 3]).unwrap_or("0.0S")
}

fn format_best<'a>(prefix: &[u8], n: u32, buf: &'a mut [u8; 20]) -> &'a str {
    let pn = prefix.len().min(12);
    buf[..pn].copy_from_slice(&prefix[..pn]);
    let mut db = [0u8; 11];
    let d = int(n as i32, &mut db).as_bytes();
    let dn = d.len().min(buf.len() - pn);
    buf[pn..pn + dn].copy_from_slice(&d[..dn]);
    core::str::from_utf8(&buf[..pn + dn]).unwrap_or("")
}

fn format_heist_best<'a>(ms: u32, buf: &'a mut [u8; 20]) -> &'a str {
    if ms == 0 {
        return "BEST --";
    }
    buf[..5].copy_from_slice(b"BEST ");
    let mut tb = [0u8; 16];
    let t = format_tenths(ms as i32, &mut tb).as_bytes();
    let n = t.len().min(buf.len() - 5);
    buf[5..5 + n].copy_from_slice(&t[..n]);
    core::str::from_utf8(&buf[..5 + n]).unwrap_or("BEST")
}

fn fastest_heist_ms(best: Records) -> u32 {
    best.heist_best_ms.iter().copied().filter(|&v| v > 0).min().unwrap_or(0)
}

fn tint(c: Rgb, k: i32) -> Rgb {
    c.scale(k)
}

fn siren_wash(c: Rgb, siren: u8, amount_q: i32) -> Rgb {
    match siren {
        1 => c.mix(SIREN_RED, amount_q),
        2 => c.mix(SIREN_BLUE, amount_q),
        _ => c,
    }
}

/// Semi-transparent red/blue police light wash over screen surfaces.
fn wash(c: Rgb, siren: u8) -> Rgb {
    siren_wash(c, siren, 260)
}

/// Lighter translucent tint so bright LEDs and HUD digits stay crisp inside the wash.
fn wash_fg(c: Rgb, siren: u8) -> Rgb {
    siren_wash(c, siren, 150)
}

/// Restricts drawing to `[x0..x1, y0..y1)`.
struct Clip<'a, S: Surface> {
    s: &'a mut S,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

impl<S: Surface> Surface for Clip<'_, S> {
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u16) {
        let ax = x.max(self.x0);
        let ay = y.max(self.y0);
        let bx = (x + w).min(self.x1);
        let by = (y + h).min(self.y1);
        if bx > ax && by > ay {
            self.s.rect(ax, ay, bx - ax, by - ay, c);
        }
    }

    fn text(&mut self, x: i32, y: i32, t: &str, c: u16) {
        self.s.text(x, y, t, c);
    }
}

/// Skips drawing over `[skip[0]..skip[2], skip[1]..skip[3])` so static foreground elements never flicker.
struct Exclude<'a, S: Surface> {
    s: &'a mut S,
    skip: [i32; 4],
}

impl<S: Surface> Surface for Exclude<'_, S> {
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u16) {
        if w <= 0 || h <= 0 {
            return;
        }
        let (x0, y0, x1, y1) = (x, y, x + w, y + h);
        let [sx0, sy0, sx1, sy1] = self.skip;
        if x1 <= sx0 || x0 >= sx1 || y1 <= sy0 || y0 >= sy1 {
            self.s.rect(x, y, w, h, c);
            return;
        }
        if y0 < sy0 {
            self.s.rect(x0, y0, w, sy0 - y0, c);
        }
        if y1 > sy1 {
            self.s.rect(x0, sy1, w, y1 - sy1, c);
        }
        let my0 = y0.max(sy0);
        let my1 = y1.min(sy1);
        if my1 > my0 {
            if x0 < sx0 {
                self.s.rect(x0, my0, sx0 - x0, my1 - my0, c);
            }
            if x1 > sx1 {
                self.s.rect(sx1, my0, x1 - sx1, my1 - my0, c);
            }
        }
    }

    fn text(&mut self, x: i32, y: i32, t: &str, c: u16) {
        self.s.text(x, y, t, c);
    }
}

/// Draws the classical bank building centered at `(C, cy)` scaled by `scale_q` and lit by `bright_k`.
fn draw_bank_scene(s: &mut impl Surface, cy: i32, scale_q: i32, bright_k: i32, show_sign: bool, clear_bg: bool) {
    let bg565 = BG.rgb565();
    if bright_k <= 20 {
        s.rect(0, 0, SIZE, SIZE, bg565);
        return;
    }
    if clear_bg {
        s.rect(0, 0, SIZE, SIZE, bg565);
    }

    let mut clip = Clip { s, x0: 0, y0: 0, x1: SIZE, y1: SIZE };
    let wall = tint(BANK_WALL, bright_k);
    let wall565 = wall.rgb565();
    let stone = tint(BANK_STONE, bright_k);
    let stone565 = stone.rgb565();
    let col_c = tint(BANK_COL, bright_k);
    let col565 = col_c.rgb565();
    let door_c = tint(BANK_DOOR, bright_k);
    let trim_c = tint(BANK_TRIM, bright_k);

    let sc = |v: i32| (v * scale_q) / ONE;

    let body_w = sc(248);
    let body_h = sc(126);
    let body_x = C - body_w / 2;
    let body_y = cy - sc(38);
    let body_bot = body_y + body_h;
    let corn_y = body_y - sc(12);
    let corn_h = sc(14).max(2);

    // Triangular pediment roof (`clear_bg = false` skips already-black sky because the zoom strictly expands).
    let roof_w = sc(272);
    let roof_h = sc(46);
    let roof_top = body_y - roof_h - sc(10);
    let in_h = sc(28);
    let in_top = roof_top + sc(12);
    let r0 = (-roof_top).max(0);
    let r1 = roof_h.min(corn_y - roof_top).min(SIZE - roof_top);
    for row in r0..r1 {
        let half = ((row + 1) * (roof_w / 2)) / roof_h.max(1);
        let y = roof_top + row;
        let x0 = (C - half).max(0);
        let x1 = (C + half).min(SIZE);
        if x1 > x0 {
            if y >= in_top && y < in_top + in_h {
                let ihalf = ((y - in_top + 1) * sc(88)) / in_h.max(1);
                let ix0 = (C - ihalf).max(x0);
                let ix1 = (C + ihalf).min(x1);
                clip.rect(x0, y, (ix0 - x0).max(0), 1, stone565);
                clip.rect(ix0, y, (ix1 - ix0).max(0), 1, wall565);
                clip.rect(ix1, y, (x1 - ix1).max(0), 1, stone565);
            } else {
                clip.rect(x0, y, x1 - x0, 1, stone565);
            }
        }
    }

    // Architrave cornice bar.
    let kx0 = C - sc(136);
    let kw = sc(272);
    clip.rect(kx0, corn_y, kw, corn_h, col565);

    let dw = sc(68);
    let dh = sc(84);
    let dx = C - dw / 2;
    let dy = body_bot - dh;
    let trim_pad = sc(4);
    let tx0 = dx - trim_pad;
    let tx1 = dx + dw + trim_pad;
    let ty0 = dy - trim_pad;

    let cw = sc(22).max(4);
    let cap_pad = sc(3);
    let cap_h = sc(8).max(2);
    let mid_y = body_y + cap_h;
    let mid_h = (body_bot - cap_h - mid_y).max(0);
    let offs = [-102, -58, 58, 102];
    let cols = offs.map(|off| {
        let cx0 = C + sc(off) - cw / 2;
        (cx0, cx0 + cw, cx0 - cap_pad, cx0 + cw + cap_pad)
    });

    // Paint facade wall ONLY in the non-column bays so columns never flash wall colour underneath.
    for (gx0, gx1) in [
        (body_x, cols[0].2),
        (cols[0].3, cols[1].2),
        (cols[1].3, tx0),
        (tx1, cols[2].2),
        (cols[2].3, cols[3].2),
        (cols[3].3, body_x + body_w),
    ] {
        if gx1 > gx0 {
            clip.rect(gx0, body_y, gx1 - gx0, body_h, wall565);
        }
    }
    for &(cx0, cx1, cap_x0, cap_x1) in &cols {
        clip.rect(cx0, mid_y, cx1 - cx0, mid_h, col565);
        clip.rect(cap_x0, body_y, cap_x1 - cap_x0, cap_h, stone565);
        clip.rect(cap_x0, body_bot - cap_h, cap_x1 - cap_x0, cap_h, stone565);
        if mid_h > 0 {
            clip.rect(cap_x0, mid_y, cx0 - cap_x0, mid_h, wall565);
            clip.rect(cx1, mid_y, cap_x1 - cx1, mid_h, wall565);
        }
    }

    let sign_w = sc(68);
    let sign_h = sc(16);
    let sign_active = show_sign && scale_q < ONE * 2 && sign_h >= 12;
    let sign_box = if sign_active {
        [C - sign_w / 2, body_y + sc(6), C + sign_w / 2, body_y + sc(6) + sign_h]
    } else {
        [0, 0, 0, 0]
    };
    let trim_r = sc(6).max(2);
    if ty0 + trim_r > body_y && tx1 > tx0 {
        let mut ex_sign = Exclude { s: &mut clip, skip: sign_box };
        ex_sign.rect(tx0, body_y, tx1 - tx0, ty0 - body_y, wall565);
        // Backfill outer rounded corners of the door trim box.
        ex_sign.rect(tx0, ty0, trim_r, trim_r, wall565);
        ex_sign.rect(tx1 - trim_r, ty0, trim_r, trim_r, wall565);
        ex_sign.rect(tx0, body_bot - trim_r, trim_r, trim_r, wall565);
        ex_sign.rect(tx1 - trim_r, body_bot - trim_r, trim_r, trim_r, wall565);
    }
    if sign_active {
        clip.rect(C - sign_w / 2, body_y + sc(6), sign_w, sign_h, door_c.rgb565());
        FONT_SM.centre(&mut clip, C, body_y + sc(6), "BANK", trim_c, door_c);
    }

    // Door trim frame + central double doors (zero wall/column overlap).
    {
        let mut ex_door = Exclude { s: &mut clip, skip: [dx, dy, dx + dw, dy + dh] };
        rrect(&mut ex_door, tx0, ty0, dw + 2 * trim_pad, dh + trim_pad, trim_r, trim_c, wall);
    }
    clip.rect(dx, dy, dw, dh, door_c.rgb565());
    clip.rect(C - sc(1).max(1), dy + sc(4), sc(2).max(1), dh - sc(6), trim_c.rgb565());
    disc(&mut clip, C - sc(10), dy + dh / 2, sc(3).max(1), trim_c, door_c);
    disc(&mut clip, C + sc(10), dy + dh / 2, sc(3).max(1), trim_c, door_c);

    // Base steps + side sky strip around step 1 (catches shrinking corner overlap when step 2 shifts down).
    let step1_y = body_bot;
    let step2_y = step1_y + sc(10);
    let sx1 = C - sc(136);
    let sw1 = sc(272);
    let sx2 = C - sc(148);
    let sw2 = sc(296);
    // Side sky around facade (below projecting cornice eaves) + step 1 sides.
    if !clear_bg {
        let facade_sky_y = (corn_y + corn_h).max(body_y);
        let facade_sky_h = (body_bot - facade_sky_y).max(0);
        if facade_sky_h > 0 {
            if body_x > 0 {
                clip.rect(0, facade_sky_y, body_x, facade_sky_h, bg565);
            }
            if body_x + body_w < SIZE {
                clip.rect(body_x + body_w, facade_sky_y, SIZE - (body_x + body_w), facade_sky_h, bg565);
            }
        }
        if sx1 > 0 {
            clip.rect(0, step1_y, sx1, step2_y - step1_y, bg565);
        }
        if sx1 + sw1 < SIZE {
            clip.rect(sx1 + sw1, step1_y, SIZE - (sx1 + sw1), step2_y - step1_y, bg565);
        }
    }
    clip.rect(sx1, step1_y, sw1, sc(10).max(2), stone565);
    clip.rect(sx2, step2_y, sw2, sc(10).max(2), col565);
}

fn polar_at(cx: i32, cy: i32, r: i32, a: u16) -> (i32, i32) {
    (cx + ((r * cos(a) + (1 << 13)) >> 14), cy + ((r * sin(a) + (1 << 13)) >> 14))
}

fn draw_safe_bolts(s: &mut impl Surface, bolt_retract_t: i32, bright_k: i32, siren: u8, restore_bg: bool) {
    let body_c = wash(tint(SAFE_BODY, bright_k), siren);
    let door_c = wash(tint(DOOR_PANEL, bright_k), siren);
    let seam_c = wash(tint(DOOR_SEAM, bright_k), siren);
    let bolt_c = wash(tint(BOLT, bright_k), siren);
    let bolt_x = SIZE - 34 + (20 * bolt_retract_t) / ONE;

    if restore_bg {
        for by in [152, 286] {
            s.rect(SIZE - 35, by - 1, 4, 30, door_c.rgb565());
            s.rect(SIZE - 31, by - 1, 5, 30, seam_c.rgb565());
            s.rect(SIZE - 26, by - 1, 26, 30, body_c.rgb565());
        }
    }
    rrect(s, bolt_x, 152, 22, 28, 4, bolt_c, seam_c);
    rrect(s, bolt_x, 286, 22, 28, 4, bolt_c, seam_c);
}

fn safe_corner_r(scale_q: i32) -> i32 {
    (6 * scale_q / ONE).max(3)
}

fn safe_door_core(scale_q: i32) -> [i32; 4] {
    let half = (96 * scale_q) / ONE;
    if half >= C {
        [31, 60, SIZE - 31, SIZE - 52]
    } else {
        let x0 = C - half;
        let y0 = C - half;
        let w = half * 2;
        let r = safe_corner_r(scale_q);
        let dpad = (20 * scale_q) / ONE;
        let ix = x0 + dpad + 3;
        let iy = y0 + dpad + 3;
        let iw = w - 2 * dpad - 6;
        let dr = (r - 6).max(2);
        let hw = (12 * scale_q / ONE).max(4);
        // Leave room on the left for the hinge inner edge so hinges never overlap the skipped core.
        [ix + hw / 2 + 2, iy + dr, ix + iw, iy + iw - dr]
    }
}

fn draw_safe_backdrop(
    s: &mut impl Surface,
    scale_q: i32,
    bolt_retract_t: i32,
    bright_k: i32,
    siren: u8,
    clear_bg: bool,
) {
    let half = (96 * scale_q) / ONE;
    let x0 = C - half;
    let y0 = C - half;
    let w = half * 2;
    let r = safe_corner_r(scale_q);

    let body_c = wash(tint(SAFE_BODY, bright_k), siren);
    let edge_c = wash(tint(SAFE_BODY_EDGE, bright_k), siren);
    let door_c = wash(tint(DOOR_PANEL, bright_k), siren);
    let seam_c = wash(tint(DOOR_SEAM, bright_k), siren);
    let hinge_c = wash(tint(HINGE, bright_k), siren);

    if half < C {
        let leg_w = (24 * scale_q) / ONE;
        let leg_h = (16 * scale_q) / ONE;
        let leg_off = (62 * scale_q) / ONE;
        let bg565 = BG.rgb565();
        if clear_bg {
            s.rect(0, 0, SIZE, SIZE, bg565);
        } else if y0 + w < SIZE {
            let clear_h = (leg_h + 6).min(SIZE - (y0 + w));
            s.rect(C - leg_off - 4, y0 + w, leg_off * 2 + 8, clear_h, bg565);
        }
        rrect(s, C - leg_off, y0 + w - 4, leg_w, leg_h, 4, seam_c, BG);
        rrect(s, C + leg_off - leg_w, y0 + w - 4, leg_w, leg_h, 4, seam_c, BG);

        let pad = (10 * scale_q) / ONE;
        let bx = x0 + pad;
        let by = y0 + pad;
        let bw = w - 2 * pad;
        let br = (r - 2).max(2);
        let dpad = (20 * scale_q) / ONE;
        let dx = x0 + dpad;
        let dy = y0 + dpad;
        let dw = w - 2 * dpad;
        let sr = (r - 4).max(2);
        let ix = dx + 3;
        let iy = dy + 3;
        let iw = dw - 6;
        let dr = (r - 6).max(2);

        // Skip each layer's solid interior covered by the layer inside it so no frame paints the centre four times.
        {
            let mut ex_v = Exclude { s, skip: [bx + br, by, bx + bw - br, by + bw] };
            let mut ex_edge = Exclude { s: &mut ex_v, skip: [bx, by + br, bx + bw, by + bw - br] };
            rrect(&mut ex_edge, x0, y0, w, w, r, edge_c, BG);
        }
        {
            let mut ex_v = Exclude { s, skip: [dx + sr, dy, dx + dw - sr, dy + dw] };
            let mut ex_body = Exclude { s: &mut ex_v, skip: [dx, dy + sr, dx + dw, dy + dw - sr] };
            rrect(&mut ex_body, bx, by, bw, bw, br, body_c, edge_c);
        }
        {
            let mut ex_v = Exclude { s, skip: [ix + dr, iy, ix + iw - dr, iy + iw] };
            let mut ex_seam = Exclude { s: &mut ex_v, skip: [ix, iy + dr, ix + iw, iy + iw - dr] };
            rrect(&mut ex_seam, dx, dy, dw, dw, sr, seam_c, body_c);
        }
        rrect(s, ix, iy, iw, iw, dr, door_c, seam_c);

        let hw = (12 * scale_q / ONE).max(4);
        let hh = (26 * scale_q / ONE).max(8);
        rrect(s, dx - hw / 2, C - half / 2 - hh / 2, hw, hh, 3, hinge_c, body_c);
        rrect(s, dx - hw / 2, C + half / 2 - hh / 2, hw, hh, 3, hinge_c, body_c);
        return;
    }

    s.rect(31, 60, SIZE - 62, SIZE - 112, door_c.rgb565());
    gradient(s, 31, 0, SIZE - 62, 60, wash(tint(DOOR_TOP, bright_k), siren), door_c, true);
    gradient(s, 31, SIZE - 52, SIZE - 62, 52, door_c, wash(tint(DOOR_BOT, bright_k), siren), true);
    s.rect(0, 0, 26, SIZE, body_c.rgb565());
    s.rect(26, 0, 5, SIZE, seam_c.rgb565());
    s.rect(SIZE - 26, 0, 26, SIZE, body_c.rgb565());
    s.rect(SIZE - 31, 0, 5, SIZE, seam_c.rgb565());
    rrect(s, 14, 132, 22, 46, 5, hinge_c, body_c);
    rrect(s, 14, 288, 22, 46, 5, hinge_c, body_c);
    draw_safe_bolts(s, bolt_retract_t, bright_k, siren, false);
}

fn draw_single_tick(s: &mut impl Surface, cx: i32, cy: i32, i: i32, active: bool, active_c: Rgb, bg: Rgb, siren: u8) {
    let a = tick_angle(i);
    let (r0, r1, w, col) = if active {
        (TICK_R0 - 3, TICK_R1 + 2, 4, active_c)
    } else if i % 3 == 0 {
        (TICK_R0 + 2, TICK_R1, 3, wash(Rgb(218, 224, 234), siren))
    } else {
        (TICK_R0 + 6, TICK_R1 - 1, 2, wash(Rgb(62, 68, 80), siren))
    };
    stroke(s, cx, cy, cos(a), sin(a), r0, r1, w, 0, |_| col, bg);
}

fn wheel_metrics(r_outer: i32) -> (i32, i32, i32, i32, i32, i32) {
    let r_inner = (r_outer * 74) / 100;
    let r_hub = (r_outer * 30) / 100;
    let r_knob = ((r_outer - r_inner) / 2 - 4).max(3);
    let spoke_w = (r_outer / 11).max(3);
    let spoke_r0 = r_hub + 3;
    let spoke_r1 = r_inner - 5;
    (r_inner, r_hub, r_knob, spoke_w, spoke_r0, spoke_r1)
}

fn draw_wheel_spokes(
    s: &mut impl Surface,
    cx: i32,
    cy: i32,
    r0: i32,
    r1: i32,
    w: i32,
    dial: i32,
    col: Rgb,
    bg: Rgb,
) {
    if r1 <= r0 + 1 {
        return;
    }
    let base_a = tick_angle(dial);
    for k in 0..3 {
        let a = base_a.wrapping_add((k * 21845) as u16);
        stroke(s, cx, cy, cos(a), sin(a), r0, r1, w, 0, |_| col, bg);
    }
}

fn solid_disc(s: &mut impl Surface, cx: i32, cy: i32, r: i32, c: Rgb) {
    radial_n::<1>(s, cx, cy, &[Band { r, paint: Paint::Solid(c), soft: 0 }], None, |_, _| c);
}

fn solid_ring(s: &mut impl Surface, cx: i32, cy: i32, r0: i32, r1: i32, c: Rgb) {
    let bands = [Band { r: r0, paint: Paint::Skip, soft: 0 }, Band { r: r1, paint: Paint::Solid(c), soft: 0 }];
    radial_n::<2>(s, cx, cy, &bands, None, |_, _| c);
}

fn draw_wheel_grips(
    s: &mut impl Surface,
    cx: i32,
    cy: i32,
    r_mid: i32,
    r_knob: i32,
    dial: i32,
    peg_c: Rgb,
    acc_c: Rgb,
    rim_c: Rgb,
    erase: bool,
    crisp: bool,
) {
    let base_a = tick_angle(dial);
    for k in 0..3 {
        let a = base_a.wrapping_add((k * 21845) as u16);
        let (px, py) = polar_at(cx, cy, r_mid, a);
        if erase {
            disc(s, px, py, r_knob + 1, rim_c, rim_c);
        } else if crisp {
            disc(s, px, py, r_knob, peg_c, rim_c);
            if k == 0 {
                disc(s, px, py, (r_knob - 3).max(2), acc_c, peg_c);
            }
        } else {
            solid_disc(s, px, py, r_knob, peg_c);
            if k == 0 {
                solid_disc(s, px, py, (r_knob - 3).max(2), acc_c);
            }
        }
    }
}

/// Draws the entire 3-spoke dial wheel in deep dark charcoal (including rim, spokes, hub, and side grips).
#[allow(clippy::too_many_arguments)]
fn draw_wheel(
    s: &mut impl Surface,
    cx: i32,
    cy: i32,
    r_outer: i32,
    dial: i32,
    led: Rgb,
    with_ticks: bool,
    bg: Rgb,
    bright_k: i32,
    siren: u8,
    crisp: bool,
) {
    if with_ticks {
        let active_c = wash_fg(led, siren);
        for i in 0..DIAL_STEPS {
            draw_single_tick(s, cx, cy, i, i == dial, active_c, bg, siren);
        }
    }

    let w_dark = wash(tint(WHEEL_DARK, bright_k), siren);
    let w_rim = wash(tint(WHEEL_RIM, bright_k), siren);
    let w_spoke = wash(tint(WHEEL_SPOKE, bright_k), siren);
    let w_hub = wash(tint(WHEEL_HUB, bright_k), siren);
    let w_cap = wash(tint(WHEEL_CAP, bright_k), siren);
    let w_peg = wash(tint(WHEEL_PEG, bright_k), siren);
    let w_acc = wash_fg(tint(ACCENT, bright_k), siren);

    let aa = i32::from(crisp);
    let (r_inner, r_hub, r_knob, spoke_w, spoke_r0, spoke_r1) = wheel_metrics(r_outer);
    draw_wheel_spokes(s, cx, cy, spoke_r0, spoke_r1, spoke_w, dial, w_spoke, bg);

    let rim_bands = [
        Band { r: (r_inner - 2).max(1), paint: Paint::Skip, soft: aa },
        Band { r: r_inner, paint: Paint::Solid(w_dark), soft: 0 },
        Band { r: (r_outer - 2).max(r_inner + 1), paint: Paint::Solid(w_rim), soft: 0 },
        Band { r: r_outer + 1, paint: Paint::Solid(w_dark), soft: aa },
    ];
    radial_n::<4>(s, cx, cy, &rim_bands, None, |_, _| bg);

    draw_wheel_grips(s, cx, cy, (r_inner + r_outer - 2) / 2, r_knob, dial, w_peg, w_acc, w_rim, false, crisp);

    let bands = [
        Band { r: (r_hub * 45 / 100).max(2), paint: Paint::Solid(w_cap), soft: 0 },
        Band { r: (r_hub * 78 / 100).max(3), paint: Paint::Solid(w_hub), soft: 0 },
        Band { r: r_hub.max(4), paint: Paint::Solid(w_dark), soft: aa },
    ];
    radial_n::<3>(s, cx, cy, &bands, None, |_, _| bg);
}

/// Incrementally updates only the changed ticks, spokes, and rim grips without clearing `WHEEL_BOX`.
fn draw_wheel_diff(
    s: &mut impl Surface,
    prev_dial: i32,
    now_dial: i32,
    led: Rgb,
    bg: Rgb,
    siren: u8,
) {
    let active_c = wash_fg(led, siren);
    if prev_dial != now_dial {
        let prev_a = tick_angle(prev_dial);
        stroke(s, C, WHEEL_CY, cos(prev_a), sin(prev_a), TICK_R0 - 3, TICK_R1 + 2, 6, 0, |_| bg, bg);
        draw_single_tick(s, C, WHEEL_CY, prev_dial, false, active_c, bg, siren);

        let now_a = tick_angle(now_dial);
        stroke(s, C, WHEEL_CY, cos(now_a), sin(now_a), TICK_R0, TICK_R1 + 1, 5, 0, |_| bg, bg);
        draw_single_tick(s, C, WHEEL_CY, now_dial, true, active_c, bg, siren);

        let w_rim = wash(WHEEL_RIM, siren);
        let w_spoke = wash(WHEEL_SPOKE, siren);
        let w_peg = wash(WHEEL_PEG, siren);
        let w_acc = wash_fg(ACCENT, siren);
        let (r_inner, _, r_knob, spoke_w, spoke_r0, spoke_r1) = wheel_metrics(WHEEL_R1);

        draw_wheel_spokes(s, C, WHEEL_CY, spoke_r0, spoke_r1, spoke_w + 2, prev_dial, bg, bg);
        draw_wheel_spokes(s, C, WHEEL_CY, spoke_r0, spoke_r1, spoke_w, now_dial, w_spoke, bg);

        let r_mid = (r_inner + WHEEL_R1 - 2) / 2;
        draw_wheel_grips(s, C, WHEEL_CY, r_mid, r_knob, prev_dial, w_peg, w_acc, w_rim, true, true);
        draw_wheel_grips(s, C, WHEEL_CY, r_mid, r_knob, now_dial, w_peg, w_acc, w_rim, false, true);
    } else {
        draw_single_tick(s, C, WHEEL_CY, now_dial, true, active_c, bg, siren);
    }
}

/// Draws a stepped semi-transparent red/blue perimeter glow around the screen during the final 5 seconds.
fn draw_siren_ring(s: &mut impl Surface, siren: u8) {
    if siren == 0 {
        return;
    }
    let base = wash(DOOR_PANEL, siren);
    let mid = siren_wash(DOOR_PANEL, siren, 440);
    let outer = siren_wash(DOOR_PANEL, siren, 670);
    ring(s, C, C, SIREN_R0, 222, mid, base);
    ring(s, C, C, 222, SIREN_R1, outer, mid);
}

fn draw_hud_frame(s: &mut impl Surface, siren: u8) {
    let [hx, hy, hw, hh] = HUD_BOX;
    let outer = wash(DOOR_PANEL, siren);
    let rim = wash(HUD_RIM, siren);
    let bg = wash(HUD_BG, siren);
    rrect(s, hx, hy, hw, hh, 12, rim, outer);
    rrect(s, hx + 2, hy + 2, hw - 4, hh - 4, 10, bg, rim);
}

fn draw_hud_timer(s: &mut impl Surface, g: &Game, siren: u8, clear: bool) {
    let [hx, hy, hw, _] = HUD_BOX;
    let bg = wash(HUD_BG, siren);
    if clear {
        s.rect(hx + hw - 76, hy + 7, 66, 17, bg.rgb565());
    }
    let mut tb = [0u8; 16];
    let tstr = format_tenths(g.time_ms, &mut tb);
    let tcol = wash_fg(if g.time_ms <= 5_000 { RED } else { CYAN }, siren);
    FONT_SM.draw(s, hx + hw - 12 - FONT_SM.width(tstr), hy + 8, tstr, tcol, bg);
}

fn draw_hud_top(s: &mut impl Surface, g: &Game, siren: u8, clear: bool) {
    let [hx, hy, hw, _] = HUD_BOX;
    let bg = wash(HUD_BG, siren);
    let track = wash(TRACK, siren);
    let green = wash_fg(GREEN, siren);
    let accent = wash_fg(ACCENT, siren);
    let red = wash_fg(RED, siren);
    let gold = wash_fg(GOLD, siren);
    let white = wash_fg(WHITE_RGB, siren);

    if clear {
        s.rect(hx + 10, hy + 6, hw - 20, 20, bg.rgb565());
    }
    match g.mode {
        Mode::Classic => {
            let px0 = hx + 16;
            for i in 0..g.pin_count {
                let col = if i < g.pin_idx { green } else if i == g.pin_idx { accent } else { track };
                disc(s, px0 + i as i32 * 18, hy + 15, 5, col, bg);
            }
            for i in 0..crate::game::CLASSIC_LIVES {
                let col = if i < g.lives { red } else { track };
                disc(s, hx + hw - 48 + i as i32 * 14, hy + 15, 4, col, bg);
            }
        }
        Mode::Heist => {
            let px0 = hx + 14;
            for i in 0..g.pin_count {
                let col = if i < g.pin_idx { green } else if i == g.pin_idx { accent } else { track };
                disc(s, px0 + i as i32 * 16, hy + 15, 5, col, bg);
            }
            draw_hud_timer(s, g, siren, false);
        }
        Mode::Blitz => {
            let mut sb = [0u8; 16];
            let mut db = [0u8; 11];
            let d = int(g.score as i32, &mut db).as_bytes();
            sb[0] = b'#';
            let n = d.len().min(6);
            sb[1..1 + n].copy_from_slice(&d[..n]);
            let sstr = core::str::from_utf8(&sb[..1 + n]).unwrap_or("#0");
            FONT_SM.draw(s, hx + 14, hy + 8, sstr, if g.combo >= 2 { gold } else { white }, bg);
            draw_hud_timer(s, g, siren, false);
        }
    }
}

fn draw_hud_dir(s: &mut impl Surface, g: &Game, siren: u8, clear: bool) {
    let [hx, hy, _, _] = HUD_BOX;
    let bg = wash(HUD_BG, siren);
    if clear {
        s.rect(hx + 14, hy + 26, 38, 24, bg.rgb565());
    }
    let accent = wash_fg(ACCENT, siren);
    let dir_off = wash(Rgb(100, 108, 122), siren);
    let dir_str = if g.expected_dir == Dir::Cw { ">>" } else { "<<" };
    let dir_col = if !g.armed() { accent } else { dir_off };
    FONT_MD.draw(s, hx + 16, hy + 27, dir_str, dir_col, bg);
}

fn draw_hud_dial(s: &mut impl Surface, g: &Game, siren: u8, clear: bool) {
    let [_, hy, _, _] = HUD_BOX;
    let bg = wash(HUD_BG, siren);
    if clear {
        s.rect(C - 26, hy + 26, 52, 24, bg.rgb565());
    }
    let green = wash_fg(GREEN, siren);
    let red = wash_fg(RED, siren);
    let white = wash_fg(WHITE_RGB, siren);
    let mut db = [0u8; 2];
    let num = format_two_digits(g.dial, &mut db);
    let num_col = match g.flash {
        Flash::Miss => red,
        Flash::ExactBonus | Flash::GoodBonus => green,
        _ if g.zone() == Zone::Exact => green,
        _ => white,
    };
    FONT_MD.centre(s, C, hy + 27, num, num_col, bg);
}

fn draw_hud_led(s: &mut impl Surface, g: &Game, siren: u8) {
    let [hx, hy, hw, _] = HUD_BOX;
    let bg = wash(HUD_BG, siren);
    let led = wash_fg(zone_led_color(g.zone(), g.flash), siren);
    disc(s, hx + hw - 22, hy + 38, 7, led, bg);
}

fn draw_hud(s: &mut impl Surface, g: &Game, siren: u8) {
    draw_hud_frame(s, siren);
    draw_hud_top(s, g, siren, false);
    if g.mode != Mode::Blitz {
        draw_hud_dir(s, g, siren, false);
    }
    draw_hud_dial(s, g, siren, false);
    draw_hud_led(s, g, siren);
}

fn draw_gold_interior(s: &mut impl Surface, cx: i32, cy: i32, w: i32, h: i32, sparkle_phase: u32) {
    rrect(s, cx - w / 2, cy - h / 2, w, h, 18, SAFE_BODY_EDGE, BG);
    rrect(s, cx - w / 2 + 8, cy - h / 2 + 8, w - 16, h - 16, 12, CAVITY_BG, SAFE_BODY_EDGE);
    s.rect(cx - w / 2 + 12, cy + 44, w - 24, 10, CAVITY_WALL.rgb565());

    let bar_w = 54;
    let bar_h = 22;
    let base_y = cy + 20;
    let draw_ingot = |s: &mut _, x: i32, y: i32| {
        rrect(s, x, y, bar_w, bar_h, 5, GOLD_DARK, CAVITY_BG);
        rrect(s, x + 2, y + 2, bar_w - 4, bar_h - 6, 4, GOLD_MID, GOLD_DARK);
        rrect(s, x + 6, y + 4, bar_w - 14, 5, 2, GOLD_BRIGHT, GOLD_MID);
    };
    draw_ingot(s, cx - 86, base_y);
    draw_ingot(s, cx - 27, base_y);
    draw_ingot(s, cx + 32, base_y);
    draw_ingot(s, cx - 56, base_y - 24);
    draw_ingot(s, cx + 3, base_y - 24);
    draw_ingot(s, cx - 27, base_y - 48);

    for (ox, oy) in [(-72, 38), (-54, 40), (48, 39), (66, 37), (-16, 40)] {
        disc(s, cx + ox, cy + oy, 6, GOLD_BRIGHT, CAVITY_BG);
    }

    let phase = sparkle_phase & 3;
    let glints = [(-38, -22), (24, -18), (-64, 16), (54, 18)];
    for (idx, &(gx, gy)) in glints.iter().enumerate() {
        let sx = cx + gx;
        let sy = cy + gy;
        let r = if idx as u32 == phase { 5 } else { 2 };
        disc(s, sx, sy, r, GOLD_SHINE, GOLD_BRIGHT);
    }
}

fn opening_door_geom(open_t: i32) -> (i32, i32, i32) {
    (lerp(SIZE - 56, 22, open_t), lerp(0, 88, open_t), lerp(SIZE, 240, open_t))
}

fn draw_opening_door(s: &mut impl Surface, door_w: i32, dy: i32, dh: i32) {
    if door_w > 18 {
        let dx = 28;
        rrect(s, dx, dy, door_w, dh, 14, SAFE_BODY_EDGE, BG);
        if door_w > 28 {
            rrect(s, dx + 6, dy + 6, door_w - 12, dh - 12, 10, DOOR_PANEL, SAFE_BODY_EDGE);
        }
        rrect(s, 14, 132, 22, 46, 5, HINGE, BG);
        rrect(s, 14, 288, 22, 46, 5, HINGE, BG);
    }
}

fn draw_opening_scene(s: &mut impl Surface, open_t: i32, sparkle: u32) {
    let (door_w, dy, dh) = opening_door_geom(open_t);
    {
        // Skip painting BG and gold bars underneath the closed central slab of the door.
        let inner = if door_w > 48 && dh > 36 {
            [44, dy + 16, 28 + door_w - 16, dy + dh - 16]
        } else {
            [0, 0, 0, 0]
        };
        let mut ex = Exclude { s, skip: inner };
        ex.rect(0, 0, SIZE, SIZE, BG.rgb565());
        draw_gold_interior(&mut ex, C, 206, 268, 224, sparkle);
    }
    draw_opening_door(s, door_w, dy, dh);
}

fn draw_opening_step(s: &mut impl Surface, prev_t: i32, now_t: i32) {
    let (prev_dw, prev_dy, prev_dh) = opening_door_geom(prev_t);
    let (now_dw, now_dy, now_dh) = opening_door_geom(now_t);
    let prev_right = if prev_dw > 18 { 28 + prev_dw } else { 52 };

    let paint_strip = |s: &mut _, x0: i32, y0: i32, x1: i32, y1: i32| {
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let mut clip = Clip { s, x0, y0, x1, y1 };
        // Paint BG only outside the rectangular gold frame core to avoid gold flashing black.
        let mut ex_gold = Exclude { s: &mut clip, skip: [C - 116, 206 - 94, C + 116, 206 + 94] };
        ex_gold.rect(x0, y0, x1 - x0, y1 - y0, BG.rgb565());
        draw_gold_interior(&mut clip, C, 206, 268, 224, 0);
    };

    if now_dw <= 18 {
        paint_strip(s, 14, prev_dy, prev_right + 1, prev_dy + prev_dh);
        if prev_dh < SIZE {
            s.rect(14, 132, 22, 46, BG.rgb565());
            s.rect(14, 288, 22, 46, BG.rgb565());
        }
        return;
    }

    let now_right = 28 + now_dw;
    let rx0 = (now_right - 14).max(28);
    paint_strip(s, rx0, prev_dy, prev_right + 1, prev_dy + prev_dh);
    if now_dy > prev_dy {
        paint_strip(s, 28, prev_dy, now_right + 1, (now_dy + 14).min(prev_dy + prev_dh));
    }
    if now_dy + now_dh < prev_dy + prev_dh {
        paint_strip(s, 28, (now_dy + now_dh - 14).max(prev_dy), now_right + 1, prev_dy + prev_dh);
    }

    if now_dw > 48 && now_dh > 36 {
        let mut ex = Exclude { s, skip: [44, now_dy + 16, now_right - 16, now_dy + now_dh - 16] };
        draw_opening_door(&mut ex, now_dw, now_dy, now_dh);
    } else {
        draw_opening_door(s, now_dw, now_dy, now_dh);
    }
}

/// Draws the jail bars slamming shut with flashing beacons and the prisoner gripping the bars.
fn draw_jail_scene(s: &mut impl Surface, anim_ms: u32, with_frame: bool) {
    let cx = C;
    let bx = cx - 106;
    let by = 82;
    let bw = 212;
    let bh = 144;
    let iy0 = by + 4;
    let iy1 = by + bh - 4;
    let ih = iy1 - iy0;

    let slam_t = ((anim_ms.min(OVER_SLAM_MS) as i32) * ONE / OVER_SLAM_MS as i32).clamp(0, ONE);
    let slam_q = ease_out(slam_t);
    let bar_h = ((ih * slam_q) / ONE).clamp(16, ih);
    let thud_y = if (340..400).contains(&anim_ms) {
        2
    } else if (400..470).contains(&anim_ms) {
        -1
    } else {
        0
    };

    let beacon = if anim_ms < OVER_ANIM_MS { (((anim_ms / 180) & 1) + 1) as u8 } else { 0 };
    let cell_bg = siren_wash(CELL_BG, beacon, 150);
    let (top_l, top_r) = match beacon {
        1 => (SIREN_RED, Rgb(22, 48, 92)),
        2 => (Rgb(84, 20, 26), SIREN_BLUE),
        _ => (SIREN_RED, SIREN_BLUE),
    };

    // Outer frame only needed on first draw; incremental frames paint cell interior directly.
    if with_frame {
        rrect(s, bx, by, bw, bh, 14, SAFE_BODY_EDGE, BG);
    }
    rrect(s, bx + 4, iy0, bw - 8, ih, 10, cell_bg, SAFE_BODY_EDGE);

    // Ceiling police strobe beacons.
    s.rect(bx + 12, by + 6, 46, 5, top_l.rgb565());
    s.rect(bx + bw - 58, by + 6, 46, 5, top_r.rgb565());

    // Prisoner body (striped shirt) + head inside the cell.
    let body_x = cx - 44;
    let body_y = (by + 84 + thud_y).max(iy0 + 40);
    let body_w = 88;
    let body_h = (iy1 - body_y - 4).max(12);
    rrect(s, body_x, body_y, body_w, body_h, 12, PRISON_A, cell_bg);
    for sy in (body_y + 8..iy1 - 6).step_by(12) {
        s.rect(body_x + 4, sy, body_w - 8, 5, PRISON_B.rgb565());
    }

    // Arms reach outward/up as the steel gate locks down.
    let arm_y = (by + 98 - (bar_h.min(82) * 14) / 82 + thud_y).clamp(iy0 + 20, iy1 - 10);
    line(s, cx - 36, body_y + 16, cx - 46, arm_y, 10, PRISON_A, cell_bg);
    line(s, cx + 36, body_y + 16, cx + 46, arm_y, 10, PRISON_A, cell_bg);

    // Head + startled expression while falling, grim expression once locked behind bars.
    let head_y = by + 56 + thud_y;
    disc(s, cx, head_y, 24, SKIN, cell_bg);
    s.rect(cx - 18, head_y - 23, 36, 8, WHEEL_RIM.rgb565());
    disc(s, cx - 8, head_y - 2, 3, WHEEL_DARK, SKIN);
    disc(s, cx + 8, head_y - 2, 3, WHEEL_DARK, SKIN);
    if bar_h < 82 {
        disc(s, cx, head_y + 10, 4, WHEEL_DARK, SKIN);
    } else {
        s.rect(cx - 7, head_y + 9, 14, 3, WHEEL_DARK.rgb565());
    }

    // Vertical steel jail bars slamming down from top of cell + crossbars.
    let bar_xs = [cx - 78, cx - 44, cx, cx + 44, cx + 78];
    for &bar_x in &bar_xs {
        s.rect(bar_x - 4, iy0, 8, bar_h, BAR_METAL.rgb565());
        s.rect(bar_x - 2, iy0, 2, bar_h, BAR_SHINE.rgb565());
    }
    if bar_h >= 20 {
        s.rect(bx + 8, iy0 + 14, bw - 16, 5, BAR_METAL.rgb565());
    }
    let bot_y = (iy0 + bar_h - 6).min(iy1 - 8);
    s.rect(bx + 8, bot_y, bw - 16, 6, BAR_METAL.rgb565());

    // Hands snap onto the vertical bars (`cx - 44` and `cx + 44`) once the gate drops past grip height.
    if bar_h >= 82 {
        let grip_y = by + 82 + thud_y;
        for &hx in &[cx - 44, cx + 44] {
            rrect(s, hx - 9, grip_y - 8, 18, 17, 6, SKIN, BAR_METAL);
            s.rect(hx - 6, grip_y - 2, 12, 2, Rgb(164, 126, 98).rgb565());
            s.rect(hx - 6, grip_y + 3, 12, 2, Rgb(164, 126, 98).rgb565());
        }
    }
}

fn draw_menu_button(s: &mut impl Surface, y: i32, title: &str, sub: &str, selected: bool, accent: Rgb) {
    let fill = if selected { CARD_SEL } else { CARD };
    rrect(s, MENU_BTN_X, y, MENU_BTN_W, MENU_BTN_H, 14, fill, BANK_WALL);
    if selected {
        rrect(s, MENU_BTN_X + 8, y + 10, 5, MENU_BTN_H - 20, 2, accent, fill);
    }
    FONT_MD.centre(s, C, y + 6, title, if selected { accent } else { WHITE_RGB }, fill);
    FONT_SM.centre(s, C, y + 29, sub, GREY_RGB, fill);
}

fn draw_menu(s: &mut impl Surface, g: &Game, best: Records) {
    // Small classical bank on the background, softly dimmed behind the UI cards.
    draw_bank_scene(s, C + 18, ONE, (ONE * 58) / 100, false, true);
    ring(s, C, C, 214, 218, SAFE_BODY, BG);
    FONT_LG.centre(s, C, 56, "VAULT", WHITE_RGB, BG);
    FONT_MD.centre(s, C, 102, "RUSH", GOLD, BG);

    let mut b1 = [0u8; 20];
    let mut b2 = [0u8; 20];
    let mut b3 = [0u8; 20];
    let heist_best = fastest_heist_ms(best);
    let heist_sub = if heist_best == 0 { "10S - 60S" } else { format_heist_best(heist_best, &mut b2) };
    draw_menu_button(s, MENU_CLASSIC_Y, "CLASSIC", format_best(b"BEST ", best.classic, &mut b1), g.menu_sel == 0, ACCENT);
    draw_menu_button(s, MENU_HEIST_Y, "HEIST", heist_sub, g.menu_sel == 1, GOLD);
    draw_menu_button(s, MENU_BLITZ_Y, "BLITZ 60S", format_best(b"BEST ", best.blitz, &mut b3), g.menu_sel == 2, CYAN);

    FONT_SM.centre(s, C, 354, "CROWN OR TAP", GREY_RGB, BG);
}

fn draw_heist_header(s: &mut impl Surface, idx: usize, best: Records) {
    let pins = heist_pins_for_idx(idx);
    let mut buf = [0u8; 28];
    let mut pos = 0usize;
    let push = |buf: &mut [u8], pos: &mut usize, bytes: &[u8]| {
        let n = bytes.len().min(buf.len() - *pos);
        buf[*pos..*pos + n].copy_from_slice(&bytes[..n]);
        *pos += n;
    };
    let mut db = [0u8; 11];
    push(&mut buf, &mut pos, int(pins as i32, &mut db).as_bytes());
    push(&mut buf, &mut pos, b" PINS  ");
    let mut hb = [0u8; 20];
    push(&mut buf, &mut pos, format_heist_best(best.heist_best_ms[idx], &mut hb).as_bytes());
    let sub = core::str::from_utf8(&buf[..pos]).unwrap_or("PINS");
    let bx = C - 108;
    rrect(s, bx, 90, 216, 28, 10, CARD, BG);
    FONT_SM.centre(s, C, 95, sub, GREY_RGB, CARD);
}

fn draw_heist_tile(s: &mut impl Surface, i: usize, selected: bool, cleared: bool) {
    let [x, y, w, h] = heist_tile_rect(i);
    let fill = if selected { CARD_SEL } else { CARD };
    let border = if selected { GOLD } else { HUD_RIM };
    rrect(s, x, y, w, h, 14, border, BANK_WALL);
    rrect(s, x + 2, y + 2, w - 4, h - 4, 12, fill, border);

    let mut lb = [0u8; 6];
    let mut db = [0u8; 11];
    let d = int(HEIST_TIMES_S[i] as i32, &mut db).as_bytes();
    let n = d.len().min(4);
    lb[..n].copy_from_slice(&d[..n]);
    lb[n] = b'S';
    let label = core::str::from_utf8(&lb[..n + 1]).unwrap_or("40S");
    FONT_MD.centre(s, x + w / 2, y + 18, label, if selected { GOLD } else { WHITE_RGB }, fill);

    if cleared {
        disc(s, x + w - 12, y + 12, 4, GREEN, fill);
    }
}

fn draw_heist_select(s: &mut impl Surface, g: &Game, best: Records) {
    draw_bank_scene(s, C + 18, ONE, (ONE * 58) / 100, false, true);
    ring(s, C, C, 214, 218, SAFE_BODY, BG);
    FONT_MD.centre(s, C, 56, "HEIST TIME", GOLD, BG);
    draw_heist_header(s, g.heist_idx, best);

    for i in 0..HEIST_TIMES_S.len() {
        draw_heist_tile(s, i, i == g.heist_idx, best.heist_best_ms[i] > 0);
    }

    FONT_SM.centre(s, C, 294, "TAP TO START", GREY_RGB, BG);
    rrect(s, HEIST_BACK_X, HEIST_BACK_Y, HEIST_BACK_W, HEIST_BACK_H, 22, CARD, BG);
    FONT_MD.centre(s, C, HEIST_BACK_Y + 11, "BACK", WHITE_RGB, CARD);
}

fn draw_end_buttons(s: &mut impl Surface, sel: usize, primary: &str, bg: Rgb) {
    let f0 = if sel == 0 { ACCENT } else { CARD };
    rrect(s, OVER_BTN_X, OVER_RETRY_Y, OVER_BTN_W, OVER_BTN_H, 23, f0, bg);
    FONT_MD.centre(s, C, OVER_RETRY_Y + 12, primary, if sel == 0 { BG } else { WHITE_RGB }, f0);

    let f1 = if sel == 1 { CARD_SEL } else { CARD };
    rrect(s, OVER_BTN_X, OVER_MENU_Y, OVER_BTN_W, OVER_BTN_H, 23, f1, bg);
    FONT_MD.centre(s, C, OVER_MENU_Y + 12, "MENU", WHITE_RGB, f1);
}

fn draw_over_title(s: &mut impl Surface, g: &Game) {
    let title = match g.mode {
        Mode::Classic => "CAUGHT!",
        Mode::Heist | Mode::Blitz => "TOO SLOW!",
    };
    let k = ((g.anim_ms.min(300) as i32) * ONE / 300).clamp(260, ONE);
    s.rect(C - 120, 42, 240, 32, BG.rgb565());
    FONT_MD.centre(s, C, 46, title, tint(RED, k), BG);
}

fn draw_over(s: &mut impl Surface, g: &Game, best: Records) {
    s.rect(0, 0, SIZE, SIZE, BG.rgb565());
    draw_over_title(s, g);

    draw_jail_scene(s, g.anim_ms, true);

    if g.mode == Mode::Heist {
        let mut hb = [0u8; 20];
        FONT_SM.centre(s, C, 240, format_heist_best(best.heist_best_ms[g.heist_idx], &mut hb), GREY_RGB, BG);
    } else {
        let mut sb = [0u8; 20];
        let label = format_best(b"SCORE ", g.score, &mut sb);
        if g.new_best {
            FONT_SM.centre(s, C, 236, label, WHITE_RGB, BG);
            FONT_SM.centre(s, C, 256, "NEW RECORD!", GREEN, BG);
        } else {
            let mut bb = [0u8; 20];
            let b = if g.mode == Mode::Classic { best.classic } else { best.blitz };
            FONT_SM.centre(s, C, 236, label, WHITE_RGB, BG);
            FONT_SM.centre(s, C, 256, format_best(b"BEST ", b, &mut bb), GREY_RGB, BG);
        }
    }

    draw_end_buttons(s, g.over_sel, "RETRY", BG);
}

fn draw_playing_backdrop_masked(s: &mut impl Surface, siren: u8) {
    let [hx, hy, hw, hh] = HUD_BOX;
    let mut ex_hud = Exclude { s, skip: [hx + 12, hy + 4, hx + hw - 12, hy + hh - 4] };
    // Hub disc (`r_hub = 29`) completely covers an inscribed `36x36` centre square (`diag/2 = 25 < 29`).
    let mut ex_both = Exclude { s: &mut ex_hud, skip: [C - 18, WHEEL_CY - 18, C + 18, WHEEL_CY + 18] };
    draw_safe_backdrop(&mut ex_both, 2660, 0, ONE, siren, false);
}

fn bank_bright(t: i32) -> i32 {
    if t < ONE / 2 {
        lerp((ONE * 70) / 100, ONE, t * 2)
    } else {
        lerp(ONE, 0, (t - ONE / 2) * 2)
    }
}

fn zoom_params(t: i32) -> (i32, i32, i32, i32, bool, bool, bool) {
    let bright = (t * 3).min(ONE);
    let scale = lerp(ONE, 2660, t);
    let wheel_t = ((t * 8) / 7).min(ONE);
    let wheel_r = lerp(38, WHEEL_R1, wheel_t);
    // Settle wheel, HUD, ticks, and AA finish on separate frames after backdrop growth stops.
    let wheel_cy = lerp(C, WHEEL_CY, wheel_t);
    let show_hud = t > 910;
    let show_ticks = t > 960;
    let crisp = t > 990;
    (scale, bright, wheel_r, wheel_cy, show_hud, show_ticks, crisp)
}

fn draw_zoom_step(s: &mut impl Surface, g: &Game, pt: i32, nt: i32) {
    let (prev_scale, prev_bright, prev_r, prev_cy, prev_hud, prev_ticks, prev_crisp) = zoom_params(pt);
    let (now_scale, now_bright, now_r, now_cy, now_hud, now_ticks, now_crisp) = zoom_params(nt);
    let led = zone_led_color(g.zone(), g.flash);

    if prev_bright == ONE && now_bright == ONE {
        if (96 * prev_scale) / ONE < C {
            let core = safe_door_core(prev_scale);
            let half = (96 * prev_scale) / ONE;
            let x0 = C - half;
            let y0 = C - half;
            let w = half * 2;
            let dpad = (20 * prev_scale) / ONE;
            let r = safe_corner_r(prev_scale);
            let ix = x0 + dpad + 3;
            let iy = y0 + dpad + 3;
            let iw = w - 2 * dpad - 6;
            let dr = (r - 6).max(2);
            let (top_y, bot_y) = if (96 * now_scale) / ONE >= C { (60, SIZE - 52) } else { (0, SIZE) };
            let mut ex_v = Exclude { s, skip: [core[0].max(ix + dr), iy.max(top_y), ix + iw - dr, (iy + iw).min(bot_y)] };
            let mut ex_h = Exclude { s: &mut ex_v, skip: [core[0], core[1].max(top_y), core[2], core[3].min(bot_y)] };
            draw_safe_backdrop(&mut ex_h, now_scale, 0, ONE, 0, false);
        }
        let wheel_changed = prev_cy != now_cy || prev_r != now_r || prev_crisp != now_crisp;
        if prev_cy != now_cy {
            solid_disc(s, C, prev_cy, prev_r + 2, DOOR_PANEL);
        } else if wheel_changed {
            let prev_hub = (prev_r * 30) / 100;
            let clear_r1 = if now_crisp { now_r + 2 } else { (now_r * 74) / 100 };
            solid_ring(s, C, now_cy, (prev_hub - 2).max(0), clear_r1, DOOR_PANEL);
        }
        if wheel_changed {
            draw_wheel(s, C, now_cy, now_r, g.dial, led, now_ticks && !prev_ticks, DOOR_PANEL, ONE, 0, now_crisp);
        } else if now_ticks && !prev_ticks {
            let active_c = wash_fg(led, 0);
            for i in 0..DIAL_STEPS {
                draw_single_tick(s, C, now_cy, i, i == g.dial, active_c, DOOR_PANEL, 0);
            }
        }
        if now_hud && !prev_hud {
            draw_hud(s, g, 0);
        }
    } else {
        draw_safe_backdrop(s, now_scale, 0, now_bright, 0, false);
        draw_wheel(s, C, now_cy, now_r, g.dial, tint(led, now_bright), now_ticks, tint(DOOR_PANEL, now_bright), now_bright, 0, now_crisp);
        if now_hud {
            draw_hud(s, g, 0);
        }
    }
}

pub fn full(s: &mut impl Surface, g: &Game, best: Records) {
    match g.state {
        State::Menu => draw_menu(s, g, best),
        State::HeistSelect => draw_heist_select(s, g, best),
        State::EnterBank => {
            let t = g.anim_t();
            let scale = lerp(ONE, 1850, t);
            let cy = lerp(C + 18, C + 36, t);
            draw_bank_scene(s, cy, scale, bank_bright(t), true, true);
        }
        State::ZoomIn => {
            let t = g.anim_t();
            let (scale, bright, wheel_r, wheel_cy, show_hud, show_ticks, crisp) = zoom_params(t);
            if bright <= 8 {
                s.rect(0, 0, SIZE, SIZE, BG.rgb565());
                return;
            }
            let led = zone_led_color(g.zone(), g.flash);
            draw_safe_backdrop(s, scale, 0, bright, 0, true);
            draw_wheel(s, C, wheel_cy, wheel_r, g.dial, tint(led, bright), show_ticks, tint(DOOR_PANEL, bright), bright, 0, crisp);
            if show_hud {
                draw_hud(s, g, 0);
            }
        }
        State::Playing => {
            let siren = g.siren_phase();
            draw_playing_backdrop_masked(s, siren);
            draw_hud(s, g, siren);
            draw_wheel(s, C, WHEEL_CY, WHEEL_R1, g.dial, zone_led_color(g.zone(), g.flash), true, wash(DOOR_PANEL, siren), ONE, siren, true);
            draw_siren_ring(s, siren);
        }
        State::Unlocking => {
            let t = g.anim_t();
            draw_safe_backdrop(s, 2660, t, ONE, 0, false);
            let r = lerp(WHEEL_R1, 10, t);
            let spin_dial = g.dial + (t * 6) / ONE;
            if r > 14 {
                draw_wheel(s, C, WHEEL_CY, r, spin_dial, GREEN, false, DOOR_PANEL, ONE, 0, true);
            }
            FONT_MD.centre(s, C, 86, "UNLOCKED", GREEN, DOOR_PANEL);
        }
        State::Opening => {
            draw_opening_scene(s, g.anim_t(), 0);
        }
        State::ClassicGold => {
            s.rect(0, 0, SIZE, SIZE, BG.rgb565());
            draw_gold_interior(s, C, 220, 276, 228, g.anim_ms / 240);
            FONT_MD.centre(s, C, 62, "OPEN!", GOLD, BG);
            let mut sb = [0u8; 20];
            FONT_MD.centre(s, C, 354, format_best(b"SAFES ", g.score, &mut sb), WHITE_RGB, BG);
        }
        State::HeistWin => {
            s.rect(0, 0, SIZE, SIZE, BG.rgb565());
            draw_gold_interior(s, C, 182, 256, 184, g.anim_ms / 240);
            let mut tb = [0u8; 16];
            let time_str = format_tenths(g.elapsed_ms as i32, &mut tb);
            if g.new_best {
                FONT_MD.centre(s, C, 32, "NEW RECORD!", GREEN, BG);
            } else {
                FONT_MD.centre(s, C, 32, "CRACKED!", GOLD, BG);
            }
            FONT_MD.centre(s, C, 58, time_str, WHITE_RGB, BG);
            draw_end_buttons(s, g.over_sel, "AGAIN", BG);
        }
        State::Over => draw_over(s, g, best),
    }
}

pub fn frame(s: &mut impl Surface, g: &Game, best: Records, last: Option<Seen>) -> (Seen, bool) {
    let now = seen(g);
    if last.is_some_and(|l| l.state == State::EnterBank && bank_bright(l.anim_t) <= 20 && now.state == State::ZoomIn && now.anim_t <= 2) {
        return (now, false);
    }
    if let Some(prev) = last.filter(|l| l.state == State::ZoomIn && now.state == State::Playing && l.mode == now.mode && now.siren == 0) {
        draw_zoom_step(s, g, prev.anim_t, ONE);
        return (now, true);
    }
    let Some(prev) = last.filter(|l| l.state == now.state && (matches!(now.state, State::Menu | State::HeistSelect) || l.mode == now.mode)) else {
        full(s, g, best);
        return (now, true);
    };
    if prev == now {
        return (now, false);
    }
    match now.state {
        State::Menu => {
            if prev.menu_sel != now.menu_sel {
                let mut b1 = [0u8; 20];
                let mut b2 = [0u8; 20];
                let mut b3 = [0u8; 20];
                let hb = fastest_heist_ms(best);
                let hsub = if hb == 0 { "10S - 60S" } else { format_heist_best(hb, &mut b2) };
                draw_menu_button(s, MENU_CLASSIC_Y, "CLASSIC", format_best(b"BEST ", best.classic, &mut b1), now.menu_sel == 0, ACCENT);
                draw_menu_button(s, MENU_HEIST_Y, "HEIST", hsub, now.menu_sel == 1, GOLD);
                draw_menu_button(s, MENU_BLITZ_Y, "BLITZ 60S", format_best(b"BEST ", best.blitz, &mut b3), now.menu_sel == 2, CYAN);
                return (now, true);
            }
            (now, false)
        }
        State::HeistSelect => {
            if prev.heist_idx != now.heist_idx {
                draw_heist_header(s, now.heist_idx, best);
                draw_heist_tile(s, prev.heist_idx, false, best.heist_best_ms[prev.heist_idx] > 0);
                draw_heist_tile(s, now.heist_idx, true, best.heist_best_ms[now.heist_idx] > 0);
                return (now, true);
            }
            (now, false)
        }
        State::Over => {
            let mut drew = false;
            if prev.over_sel != now.over_sel {
                draw_end_buttons(s, now.over_sel, "RETRY", BG);
                drew = true;
            }
            if prev.anim_step != now.anim_step {
                if prev.anim_step <= 9 {
                    draw_over_title(s, g);
                }
                draw_jail_scene(s, g.anim_ms, false);
                drew = true;
            }
            (now, drew)
        }
        State::HeistWin => {
            if prev.over_sel != now.over_sel {
                draw_end_buttons(s, now.over_sel, "AGAIN", BG);
                return (now, true);
            }
            if prev.anim_step != now.anim_step {
                draw_gold_interior(s, C, 182, 256, 184, g.anim_ms / 240);
                return (now, true);
            }
            (now, false)
        }
        State::ClassicGold => {
            if prev.anim_step != now.anim_step {
                draw_gold_interior(s, C, 220, 276, 228, g.anim_ms / 240);
                return (now, true);
            }
            (now, false)
        }
        State::EnterBank => {
            let pb = bank_bright(prev.anim_t);
            let nb = bank_bright(now.anim_t);
            if pb <= 20 && nb <= 20 {
                return (now, false);
            }
            let t = now.anim_t;
            let scale = lerp(ONE, 1850, t);
            let cy = lerp(C + 18, C + 36, t);
            draw_bank_scene(s, cy, scale, nb, true, false);
            (now, true)
        }
        State::ZoomIn => {
            draw_zoom_step(s, g, prev.anim_t, now.anim_t);
            (now, true)
        }
        State::Unlocking => {
            let pt = prev.anim_t;
            let nt = now.anim_t;
            draw_safe_bolts(s, nt, ONE, 0, true);
            let prev_r = lerp(WHEEL_R1, 10, pt);
            let now_r = lerp(WHEEL_R1, 10, nt);
            let spin_dial = g.dial + (nt * 6) / ONE;
            disc(s, C, WHEEL_CY, prev_r + 2, DOOR_PANEL, DOOR_PANEL);
            if now_r > 14 {
                draw_wheel(s, C, WHEEL_CY, now_r, spin_dial, GREEN, false, DOOR_PANEL, ONE, 0, true);
            }
            (now, true)
        }
        State::Opening => {
            draw_opening_step(s, prev.anim_t, now.anim_t);
            (now, true)
        }
        State::Playing => {
            if prev.siren != now.siren {
                draw_playing_backdrop_masked(s, now.siren);
                draw_hud(s, g, now.siren);
                draw_wheel(s, C, WHEEL_CY, WHEEL_R1, g.dial, zone_led_color(g.zone(), g.flash), true, wash(DOOR_PANEL, now.siren), ONE, now.siren, true);
                draw_siren_ring(s, now.siren);
                return (now, true);
            }
            let mut drew = false;
            let top_changed = (prev.pin_idx, prev.pin_count, prev.lives, prev.score, prev.combo)
                != (now.pin_idx, now.pin_count, now.lives, now.score, now.combo);
            if top_changed {
                draw_hud_top(s, g, now.siren, true);
                drew = true;
            } else if prev.tenths != now.tenths && g.mode != Mode::Classic {
                draw_hud_timer(s, g, now.siren, true);
                drew = true;
            }

            if g.mode != Mode::Blitz && (prev.expected_dir, prev.armed) != (now.expected_dir, now.armed) {
                draw_hud_dir(s, g, now.siren, true);
                drew = true;
            }
            if (prev.dial, prev.zone, prev.flash) != (now.dial, now.zone, now.flash) {
                draw_hud_dial(s, g, now.siren, true);
                let bg = wash(DOOR_PANEL, now.siren);
                draw_wheel_diff(s, prev.dial, now.dial, zone_led_color(g.zone(), g.flash), bg, now.siren);
                drew = true;
            }
            if (prev.zone, prev.flash) != (now.zone, now.flash) {
                draw_hud_led(s, g, now.siren);
                drew = true;
            }
            (now, drew)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{ENTER_BANK_MS, OPEN_MS, UNLOCK_MS, ZOOM_MS};
    use cmf_wp2_ui::Fb;
    use cmf_wp2_ui::motion::isqrt;

    const N: usize = (SIZE * SIZE) as usize;

    fn fresh(g: &Game, best: Records) -> Vec<u16> {
        let mut px = vec![0x1234u16; N];
        full(&mut Fb::new(&mut px), g, best);
        px
    }

    fn first_diff(a: &[u16], b: &[u16]) -> Option<(i32, i32)> {
        a.iter().zip(b).position(|(x, y)| x != y).map(|i| (i as i32 % SIZE, i as i32 / SIZE))
    }

    #[test]
    fn partial_frames_match_full_across_modes_siren_and_animations() {
        let mut best = Records { classic: 2, blitz: 8, heist_best_ms: [0, 0, 12_100, 21_400, 0, 0] };
        let mut g = Game::new(42);
        let mut px = vec![0u16; N];
        let (mut last, _) = frame(&mut Fb::new(&mut px), &g, best, None);
        assert_eq!(first_diff(&px, &fresh(&g, best)), None);

        for _ in 0..3 {
            g.crown(1);
            (last, _) = frame(&mut Fb::new(&mut px), &g, best, Some(last));
            assert_eq!(first_diff(&px, &fresh(&g, best)), None);
        }

        g.open_heist_select();
        (last, _) = frame(&mut Fb::new(&mut px), &g, best, Some(last));
        assert_eq!(first_diff(&px, &fresh(&g, best)), None);
        for _ in 0..3 {
            g.crown(1);
            (last, _) = frame(&mut Fb::new(&mut px), &g, best, Some(last));
            assert_eq!(first_diff(&px, &fresh(&g, best)), None);
        }

        // Start 10s Heist -> walk frame-by-frame through EnterBank -> ZoomIn -> Playing -> siren -> Unlocking -> Opening -> HeistWin.
        g.start_heist(0);
        (last, _) = frame(&mut Fb::new(&mut px), &g, best, Some(last));
        assert_eq!(first_diff(&px, &fresh(&g, best)), None);
        while g.state == State::EnterBank {
            g.tick(33, &mut best);
            (last, _) = frame(&mut Fb::new(&mut px), &g, best, Some(last));
            assert_eq!(first_diff(&px, &fresh(&g, best)), None);
        }
        while g.state == State::ZoomIn {
            g.tick(33, &mut best);
            (last, _) = frame(&mut Fb::new(&mut px), &g, best, Some(last));
            assert_eq!(first_diff(&px, &fresh(&g, best)), None);
        }

        // Verify incremental wheel & HUD updates across single and multi-step jumps.
        for delta in [1i16, 1, 5, -3, 7, -2] {
            g.crown(delta);
            (last, _) = frame(&mut Fb::new(&mut px), &g, best, Some(last));
            assert_eq!(first_diff(&px, &fresh(&g, best)), None);
        }

        // Advance into the 5-second police siren window, rotate dial during strobe, and alternate siren phases.
        g.tick(6_000, &mut best);
        (last, _) = frame(&mut Fb::new(&mut px), &g, best, Some(last));
        assert_eq!(first_diff(&px, &fresh(&g, best)), None);
        g.crown(1);
        (last, _) = frame(&mut Fb::new(&mut px), &g, best, Some(last));
        assert_eq!(first_diff(&px, &fresh(&g, best)), None);
        g.tick(250, &mut best);
        (last, _) = frame(&mut Fb::new(&mut px), &g, best, Some(last));
        assert_eq!(first_diff(&px, &fresh(&g, best)), None);

        for _ in 0..g.pin_count {
            let dir = if g.expected_dir == Dir::Cw { 1 } else { -1 };
            for _ in 0..2 {
                g.crown(dir);
            }
            while g.dial != g.target {
                g.crown(dir);
            }
            g.try_lock(&mut best);
            (last, _) = frame(&mut Fb::new(&mut px), &g, best, Some(last));
            assert_eq!(first_diff(&px, &fresh(&g, best)), None);
        }

        while g.state == State::Unlocking {
            g.tick(33, &mut best);
            (last, _) = frame(&mut Fb::new(&mut px), &g, best, Some(last));
            assert_eq!(first_diff(&px, &fresh(&g, best)), None);
        }
        while g.state == State::Opening {
            g.tick(33, &mut best);
            (last, _) = frame(&mut Fb::new(&mut px), &g, best, Some(last));
            assert_eq!(first_diff(&px, &fresh(&g, best)), None);
        }
        g.crown(1);
        let _ = frame(&mut Fb::new(&mut px), &g, best, Some(last));
        assert_eq!(first_diff(&px, &fresh(&g, best)), None);

        // Verify ClassicGold animation frames match full redraws.
        let mut g_cl = Game::new(42);
        g_cl.start(Mode::Classic);
        g_cl.tick(ENTER_BANK_MS, &mut best);
        g_cl.tick(ZOOM_MS, &mut best);
        for _ in 0..g_cl.pin_count {
            let dir = if g_cl.expected_dir == Dir::Cw { 1 } else { -1 };
            for _ in 0..3 {
                g_cl.crown(dir);
            }
            while g_cl.dial != g_cl.target {
                g_cl.crown(dir);
            }
            g_cl.try_lock(&mut best);
        }
        g_cl.tick(UNLOCK_MS, &mut best);
        g_cl.tick(OPEN_MS, &mut best);
        let (mut last_cl, _) = frame(&mut Fb::new(&mut px), &g_cl, best, Some(last));
        assert_eq!(first_diff(&px, &fresh(&g_cl, best)), None);
        for _ in 0..4 {
            g_cl.tick(240, &mut best);
            (last_cl, _) = frame(&mut Fb::new(&mut px), &g_cl, best, Some(last_cl));
            assert_eq!(first_diff(&px, &fresh(&g_cl, best)), None);
        }

        // Trigger timeout into Over and verify jail-slam animation & selection match full redraws.
        let mut g_over = Game::new(42);
        g_over.start_heist(0);
        g_over.tick(ENTER_BANK_MS, &mut best);
        g_over.tick(ZOOM_MS, &mut best);
        g_over.tick(10_000, &mut best);
        let (mut last_over, _) = frame(&mut Fb::new(&mut px), &g_over, best, Some(last_cl));
        assert_eq!(first_diff(&px, &fresh(&g_over, best)), None);
        for step in [180, 180, 100, 300, 700] {
            g_over.tick(step, &mut best);
            (last_over, _) = frame(&mut Fb::new(&mut px), &g_over, best, Some(last_over));
            assert_eq!(first_diff(&px, &fresh(&g_over, best)), None);
        }
        g_over.crown(1);
        let _ = frame(&mut Fb::new(&mut px), &g_over, best, Some(last_over));
        assert_eq!(first_diff(&px, &fresh(&g_over, best)), None);
    }

    #[test]
    fn wheel_turn_draw_budget_is_compact() {
        let mut best = Records::default();
        let mut g = Game::new(7);
        g.start(Mode::Classic);
        g.tick(ENTER_BANK_MS, &mut best);
        g.tick(ZOOM_MS, &mut best);

        let last = seen(&g);
        g.crown(1);
        let mut c = cmf_wp2_ui::Count::default();
        let (_, drew) = frame(&mut c, &g, best, Some(last));
        assert!(drew);
        assert!(c.rects < 1_050, "too many rects per crown frame: {}", c.rects);
        assert!(c.px < 8_000, "too many px per crown frame: {}", c.px);
    }

    #[test]
    fn intro_animation_draw_budgets() {
        let mut best = Records::default();
        let mut g = Game::new(7);
        g.start(Mode::Classic);
        let mut last = seen(&g);
        let (mut eb_max_r, mut eb_max_px) = (0u32, 0u64);
        let (mut zi_max_r, mut zi_max_px) = (0u32, 0u64);
        while g.state == State::EnterBank {
            g.tick(33, &mut best);
            let mut c = cmf_wp2_ui::Count::default();
            let (next, _) = frame(&mut c, &g, best, Some(last));
            last = next;
            eb_max_r = eb_max_r.max(c.rects);
            eb_max_px = eb_max_px.max(c.px);
        }
        while g.state == State::ZoomIn {
            g.tick(33, &mut best);
            let mut c = cmf_wp2_ui::Count::default();
            let (next, _) = frame(&mut c, &g, best, Some(last));
            last = next;
            zi_max_r = zi_max_r.max(c.rects);
            zi_max_px = zi_max_px.max(c.px);
        }
        let mut play_c = cmf_wp2_ui::Count::default();
        let (_, drew_post) = frame(&mut play_c, &g, best, Some(last));
        assert!(!drew_post);
        assert!(eb_max_r < 750, "too many rects in EnterBank: {eb_max_r}");
        assert!(eb_max_px < 225_000, "too many px in EnterBank: {eb_max_px}");
        assert!(zi_max_r < 3_200, "too many rects in ZoomIn: {zi_max_r}");
        assert!(zi_max_px < 170_000, "too many px in ZoomIn: {zi_max_px}");
        assert_eq!((play_c.rects, play_c.px), (0, 0));
    }

    #[test]
    fn hud_wheel_and_buttons_fit_inside_round_panel() {
        assert!(HUD_BOX[1] + HUD_BOX[3] < WHEEL_BOX[1], "HUD overlaps WHEEL_BOX");
        for [x, y, w, h] in [HUD_BOX, WHEEL_BOX] {
            assert!(x >= 32 && x + w <= SIZE - 32, "box touches door side frame");
            for (cx, cy) in [(x, y), (x + w, y), (x, y + h), (x + w, y + h)] {
                let d2 = (cx - C) * (cx - C) + (cy - C) * (cy - C);
                assert!(d2 <= SIREN_R0 * SIREN_R0, "box corner ({cx},{cy}) overlaps siren ring");
            }
        }
        let mut boxes: [[i32; 4]; 12] = [
            [MENU_BTN_X, MENU_CLASSIC_Y, MENU_BTN_W, MENU_BTN_H],
            [MENU_BTN_X, MENU_HEIST_Y, MENU_BTN_W, MENU_BTN_H],
            [MENU_BTN_X, MENU_BLITZ_Y, MENU_BTN_W, MENU_BTN_H],
            [OVER_BTN_X, OVER_RETRY_Y, OVER_BTN_W, OVER_BTN_H],
            [OVER_BTN_X, OVER_MENU_Y, OVER_BTN_W, OVER_BTN_H],
            [HEIST_BACK_X, HEIST_BACK_Y, HEIST_BACK_W, HEIST_BACK_H],
            [0; 4], [0; 4], [0; 4], [0; 4], [0; 4], [0; 4],
        ];
        for i in 0..6 {
            boxes[6 + i] = heist_tile_rect(i);
        }
        for [x, y, w, h] in boxes {
            for (cx, cy) in [(x, y), (x + w, y), (x, y + h), (x + w, y + h)] {
                let d2 = (cx - C) * (cx - C) + (cy - C) * (cy - C);
                assert!(d2 <= (C - 10) * (C - 10), "button ({cx},{cy}) outside circle");
            }
        }
        assert!(FONT_LG.width("VAULT") / 2 <= isqrt((C - 10).pow(2) - (C - 56).pow(2)));
        assert!(FONT_MD.width("RUSH") / 2 <= isqrt((C - 10).pow(2) - (C - 102).pow(2)));
        assert!(FONT_MD.width("TOO SLOW!") / 2 <= isqrt((C - 10).pow(2) - (C - 46).pow(2)));
        assert!(FONT_MD.width("BLITZ 60S") <= MENU_BTN_W - 24);
        assert!(FONT_SM.width("CROWN OR TAP") / 2 <= isqrt((C - 10).pow(2) - (354 + 16 - C).pow(2)));
    }
}
