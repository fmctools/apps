// Drawing, through cmf-wp2-ui's Surface: the watch draws with `&Api`, the tests with `Fb`. One buffer and no double
// buffering, so a frame repaints only the cells and numbers that changed, and never paints a pixel twice in different
// colours: each cell is its own 20 px square, filled and then drawn on once.

use crate::game::{Game, H, SCORE_MAX, State, W, shape};
use cmf_wp2_ui::draw::rrect;
use cmf_wp2_ui::text::{big, big_width, centre, int, width};
use cmf_wp2_ui::{C, Rgb, Surface};

pub const SIZE: i32 = cmf_wp2_ui::SIZE;
/// A cell, and the well's top left corner: 200 x 400 px, centred. Its far corners sit 224 px from the centre, inside
/// the round panel (233).
pub const CELL: i32 = 20;
pub const BX: i32 = C - W as i32 * CELL / 2;
pub const BY: i32 = C - H as i32 * CELL / 2;

const BLACK: u16 = 0;
const WELL: Rgb = Rgb(9, 10, 14);
const DOT: Rgb = Rgb(30, 33, 44);
const FRAME: Rgb = Rgb(58, 63, 82);
const PANEL: Rgb = Rgb(20, 22, 30);
const WHITE: Rgb = Rgb(255, 255, 255);
const GREY: u16 = 0x8C71;
const TEAL: Rgb = Rgb(94, 230, 168);

/// I O T S Z J L
pub const COLORS: [Rgb; 7] = [
    Rgb(62, 214, 230),
    Rgb(255, 201, 60),
    Rgb(160, 108, 240),
    Rgb(94, 230, 168),
    Rgb(242, 87, 95),
    Rgb(60, 130, 240),
    Rgb(255, 148, 64),
];

/// What a cell shows: 0 empty, 1..=7 a block of that kind, GHOST + kind where the piece would land, FLASH a
/// blinking row.
const GHOST: u8 = 8;
const FLASH: u8 = 15;

/// The HUD: numbers right-aligned at LEFT_X on the left of the well, the next piece and the best score on the right.
const LEFT_X: i32 = BX - 12;
const RIGHT_X: i32 = BX + W as i32 * CELL + 14;
const FIELD_W: i32 = 96;
const SCORE_Y: i32 = 140;
const LINES_Y: i32 = 206;
const LEVEL_Y: i32 = 272;
const NEXT_Y: i32 = 140;
const BEST_Y: i32 = 272;

/// What is on screen after a frame.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Seen {
    cells: [[u8; W]; H],
    score: u32,
    lines: u32,
    level: u32,
    next: u8,
    best: u32,
    state: State,
}

fn compose(g: &Game) -> [[u8; W]; H] {
    if g.state == State::Ready {
        return [[0; W]; H];
    }
    let mut c = g.board;
    if g.state == State::Playing && g.flash.is_none() {
        if let Some(gh) = g.ghost() {
            for (x, y) in gh.cells() {
                if y >= 0 && c[y as usize][x as usize] == 0 {
                    c[y as usize][x as usize] = GHOST + gh.kind;
                }
            }
        }
    }
    if let Some(p) = g.cur {
        for (x, y) in p.cells() {
            if y >= 0 {
                c[y as usize][x as usize] = p.kind + 1;
            }
        }
    }
    if let Some(f) = g.flash.filter(|f| f.lit()) {
        for (y, row) in c.iter_mut().enumerate() {
            if f.rows >> y & 1 == 1 {
                *row = [FLASH; W];
            }
        }
    }
    c
}

pub fn seen(g: &Game, best: u32) -> Seen {
    Seen {
        cells: compose(g),
        score: g.score.min(SCORE_MAX),
        lines: g.lines.min(9999),
        level: g.level() + 1,
        next: g.next,
        best: best.min(SCORE_MAX),
        state: g.state,
    }
}

/// A block of `size` px: a darker rim, a lit face and a small sheen, like the icon.
fn tile(s: &mut impl Surface, x: i32, y: i32, size: i32, c: Rgb, bg: Rgb) {
    let r = size / 4;
    let dark = c.scale(680);
    rrect(s, x + 1, y + 1, size - 2, size - 2, r + 1, dark, bg);
    rrect(s, x + 2, y + 2, size - 4, size - 5, r, c, dark);
    rrect(s, x + size / 4 + 1, y + size / 5 + 1, size / 2, 2, 1, c.mix(WHITE, 470), c);
}

/// The outline of where the piece will land.
fn ghost(s: &mut impl Surface, x: i32, y: i32, c: Rgb) {
    let dim = c.scale(420);
    rrect(s, x + 1, y + 1, CELL - 2, CELL - 2, 5, dim, WELL);
    rrect(s, x + 3, y + 3, CELL - 6, CELL - 6, 3, WELL, dim);
}

fn cell(s: &mut impl Surface, cx: usize, cy: usize, v: u8) {
    let (x, y) = (BX + cx as i32 * CELL, BY + cy as i32 * CELL);
    s.rect(x, y, CELL, CELL, WELL.rgb565());
    match v {
        0 => s.rect(x + CELL / 2 - 1, y + CELL / 2 - 1, 2, 2, DOT.rgb565()),
        1..=7 => tile(s, x, y, CELL, COLORS[v as usize - 1], WELL),
        GHOST..=14 => ghost(s, x, y, COLORS[(v - GHOST) as usize]),
        _ => tile(s, x, y, CELL, Rgb(245, 248, 255), WELL),
    }
}

/// A number in a box, right-aligned at `xr`.
fn field(s: &mut impl Surface, xr: i32, y: i32, n: u32, c: u16) {
    let mut buf = [0u8; 11];
    let t = int(n as i32, &mut buf);
    s.rect(xr - FIELD_W, y, FIELD_W, 16, BLACK);
    s.text(xr - width(t), y, t, c);
}

fn label(s: &mut impl Surface, xr: i32, y: i32, t: &str) {
    s.text(xr - width(t), y - 20, t, GREY);
}

fn next_box(s: &mut impl Surface, kind: u8) {
    const BW: i32 = 84;
    const BH: i32 = 64;
    const T: i32 = 14;
    rrect(s, RIGHT_X, NEXT_Y + 2, BW, BH, 14, PANEL, Rgb(0, 0, 0));
    let c = shape(kind, 0);
    let (x0, x1) = (c.iter().map(|p| p.0).min().unwrap_or(0), c.iter().map(|p| p.0).max().unwrap_or(0));
    let (y0, y1) = (c.iter().map(|p| p.1).min().unwrap_or(0), c.iter().map(|p| p.1).max().unwrap_or(0));
    let (w, h) = ((x1 - x0 + 1) * T, (y1 - y0 + 1) * T);
    let (ox, oy) = (RIGHT_X + (BW - w) / 2, NEXT_Y + 2 + (BH - h) / 2);
    for (px, py) in c {
        tile(s, ox + (px - x0) * T, oy + (py - y0) * T, T, COLORS[kind as usize % 7], PANEL);
    }
}

fn hud(s: &mut impl Surface, n: &Seen, p: Option<&Seen>) {
    let changed = |f: fn(&Seen) -> u32| p.is_none_or(|p| f(p) != f(n));
    if p.is_none() {
        label(s, LEFT_X, SCORE_Y, "SCORE");
        label(s, LEFT_X, LINES_Y, "LINES");
        label(s, LEFT_X, LEVEL_Y, "LEVEL");
        s.text(RIGHT_X + 2, NEXT_Y - 20, "NEXT", GREY);
        s.text(RIGHT_X + 2, BEST_Y - 20, "BEST", GREY);
    }
    if changed(|s| s.score) {
        field(s, LEFT_X, SCORE_Y, n.score, WHITE.rgb565());
    }
    if changed(|s| s.lines) {
        field(s, LEFT_X, LINES_Y, n.lines, TEAL.rgb565());
    }
    if changed(|s| s.level) {
        field(s, LEFT_X, LEVEL_Y, n.level, TEAL.rgb565());
    }
    if p.is_none_or(|p| p.next != n.next) {
        next_box(s, n.next);
    }
    if changed(|s| s.best) {
        // left-aligned under its label
        let mut buf = [0u8; 11];
        let t = int(n.best as i32, &mut buf);
        s.rect(RIGHT_X, BEST_Y, FIELD_W, 16, BLACK);
        s.text(RIGHT_X + 2, BEST_Y, t, WHITE.rgb565());
    }
}

fn frame_bars(s: &mut impl Surface) {
    let h = H as i32 * CELL;
    let w = W as i32 * CELL;
    s.rect(BX - 4, BY, 2, h + 4, FRAME.rgb565());
    s.rect(BX + w + 2, BY, 2, h + 4, FRAME.rgb565());
    s.rect(BX - 4, BY + h + 2, w + 8, 2, FRAME.rgb565());
}

/// The big title: each letter in the colour of a piece.
fn title(s: &mut impl Surface, y: i32) {
    const T: &str = "TETRIS";
    let x = C - big_width(T, 5) / 2;
    for (i, ch) in T.char_indices() {
        let mut one = [0u8; 4];
        let t = ch.encode_utf8(&mut one);
        big(s, x + i as i32 * 40, y, t, 5, COLORS[[2, 4, 1, 0, 3, 5][i]].rgb565());
    }
}

fn ready(s: &mut impl Surface, best: u32) {
    s.rect(0, 0, SIZE, SIZE, BLACK);
    title(s, 70);
    // a T piece about to drop into the gap in the row under it
    const P: i32 = 34;
    let (x0, y0) = (C - (3 * P - 2) / 2, 142);
    let black = Rgb(0, 0, 0);
    for (cx, cy, k, dim) in [(0, 0, 2, false), (1, 0, 2, false), (2, 0, 2, false), (1, 1, 2, false), (0, 2, 5, true), (2, 2, 5, true)] {
        let c = COLORS[k];
        tile(s, x0 + cx * P, y0 + cy * P, P - 2, if dim { c.scale(500) } else { c }, black);
    }
    centre(s, C, 268, "TAP TO PLAY", WHITE.rgb565());
    centre(s, C, 312, "TAP: TURN", GREY);
    centre(s, C, 334, "HOLD: DROP", GREY);
    centre(s, C, 356, "CROWN: MOVE", GREY);
    if best > 0 {
        let mut buf = [0u8; 11];
        let mut line = [0u8; 16];
        let n = int(best as i32, &mut buf).as_bytes();
        line[..5].copy_from_slice(b"BEST ");
        line[5..5 + n.len()].copy_from_slice(n);
        centre(s, C, 396, core::str::from_utf8(&line[..5 + n.len()]).unwrap_or("?"), TEAL.rgb565());
    }
}

fn over(s: &mut impl Surface, n: &Seen) {
    let (w, h) = (192, 176);
    let (x, y) = (C - w / 2, 136);
    rrect(s, x, y, w, h, 22, PANEL, WELL);
    let red = COLORS[4].rgb565();
    big(s, C - big_width("GAME", 3) / 2, y + 14, "GAME", 3, red);
    big(s, C - big_width("OVER", 3) / 2, y + 42, "OVER", 3, red);
    let mut buf = [0u8; 11];
    centre(s, C, y + 86, int(n.score as i32, &mut buf), WHITE.rgb565());
    let mut line = [0u8; 16];
    let b = int(n.best as i32, &mut buf).as_bytes();
    line[..5].copy_from_slice(b"BEST ");
    line[5..5 + b.len()].copy_from_slice(b);
    centre(s, C, y + 110, core::str::from_utf8(&line[..5 + b.len()]).unwrap_or("?"), TEAL.rgb565());
    centre(s, C, y + 146, "TAP TO PLAY", GREY);
}

/// Draws the frame for `g`. `prev`: what is on screen now; `None`, or another state, repaints everything.
pub fn frame(s: &mut impl Surface, g: &Game, best: u32, prev: Option<Seen>) -> Seen {
    let now = seen(g, best);
    match prev.filter(|p| p.state == now.state) {
        None => {
            if now.state == State::Ready {
                ready(s, best);
            } else {
                s.rect(0, 0, SIZE, SIZE, BLACK);
                frame_bars(s);
                for (y, row) in now.cells.iter().enumerate() {
                    for (x, &v) in row.iter().enumerate() {
                        cell(s, x, y, v);
                    }
                }
                hud(s, &now, None);
                if now.state == State::Over {
                    over(s, &now);
                }
            }
        }
        Some(p) => {
            if now.state == State::Playing {
                for (y, (a, b)) in p.cells.iter().zip(&now.cells).enumerate() {
                    for (x, (&u, &v)) in a.iter().zip(b).enumerate() {
                        if u != v {
                            cell(s, x, y, v);
                        }
                    }
                }
                hud(s, &now, Some(&p));
            }
        }
    }
    now
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Piece;
    use cmf_wp2_ui::{Count, Fb};
    use std::vec;
    use std::vec::Vec;

    fn screen() -> Vec<u16> {
        vec![0u16; (SIZE * SIZE) as usize]
    }

    fn full(g: &Game, best: u32) -> Vec<u16> {
        let mut px = screen();
        frame(&mut Fb::new(&mut px), g, best, None);
        px
    }

    fn playing(seed: u32) -> Game {
        let mut g = Game::new(seed);
        g.start(0);
        g
    }

    fn inside(x: i32, y: i32) -> bool {
        (x - C) * (x - C) + (y - C) * (y - C) <= C * C
    }

    #[test]
    fn the_well_and_its_frame_lie_inside_the_round_panel() {
        let (w, h) = (W as i32 * CELL, H as i32 * CELL);
        for (x, y) in [(BX - 4, BY), (BX + w + 4, BY), (BX - 4, BY + h + 4), (BX + w + 4, BY + h + 4)] {
            assert!(inside(x, y), "({x}, {y}) is off the panel");
        }
        assert_eq!((BX, BY), (133, 33));
    }

    #[test]
    fn the_hud_lies_inside_the_round_panel() {
        // every text and box: its corners (a label is 5 chars, a number up to 6, the next box 84 x 66)
        let right = RIGHT_X + FIELD_W;
        for (x0, x1, y0, y1) in [
            (LEFT_X - 80, LEFT_X, SCORE_Y - 20, SCORE_Y - 4),
            (LEFT_X - FIELD_W, LEFT_X, SCORE_Y, SCORE_Y + 16),
            (LEFT_X - 80, LEFT_X, LINES_Y - 20, LINES_Y + 16),
            (LEFT_X - 80, LEFT_X, LEVEL_Y - 20, LEVEL_Y + 16),
            (RIGHT_X, RIGHT_X + 64, NEXT_Y - 20, NEXT_Y - 4),
            (RIGHT_X, RIGHT_X + 84, NEXT_Y + 2, NEXT_Y + 66),
            (RIGHT_X, right, BEST_Y - 20, BEST_Y + 16),
        ] {
            for (x, y) in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
                assert!(inside(x, y), "({x}, {y})");
            }
        }
    }

    #[test]
    fn the_title_screens_text_fits_the_panel() {
        let wide = |t: &str, y: i32| {
            let w = width(t);
            assert!(inside(C - w / 2, y) && inside(C + w / 2, y) && inside(C - w / 2, y + 16) && inside(C + w / 2, y + 16), "{t}");
        };
        wide("TAP TO PLAY", 268);
        wide("CROWN: MOVE", 356);
        wide("BEST 999999", 396);
        let tw = big_width("TETRIS", 5);
        assert!(inside(C - tw / 2, 70) && inside(C + tw / 2, 110));
    }

    /// Plays on its own for a while and, now and then, compares what the partial frames made with a full repaint.
    #[test]
    fn partial_frames_match_a_full_repaint() {
        for seed in [1u32, 8, 77] {
            let mut g = playing(seed);
            let mut px = screen();
            let mut prev = None;
            let mut r = seed.wrapping_mul(2_654_435_761) | 1;
            let mut rnd = move || {
                r ^= r << 13;
                r ^= r >> 17;
                r ^= r << 5;
                r
            };
            let mut checked = 0;
            let mut t = 0u32;
            for step in 0..6000u32 {
                t += 16;
                match rnd() % 11 {
                    0 | 1 => {
                        g.shift(-1, t);
                    }
                    2 | 3 => {
                        g.shift(1, t);
                    }
                    4 => {
                        g.turn(t);
                    }
                    5 => g.set_soft(rnd() % 3 != 0, t),
                    _ => {}
                }
                g.tick(t);
                if g.state == State::Over {
                    prev = Some(frame(&mut Fb::new(&mut px), &g, 3, prev));
                    assert_eq!(px, full(&g, 3), "over, seed {seed}");
                    g.start(t);
                    prev = None;
                }
                prev = Some(frame(&mut Fb::new(&mut px), &g, 3, prev));
                if step % 9 == 0 {
                    let want = full(&g, 3);
                    let bad = px.iter().zip(&want).position(|(a, b)| a != b);
                    assert_eq!(bad.map(|i| (i as i32 % SIZE, i as i32 / SIZE)), None, "seed {seed} step {step}");
                    checked += 1;
                }
            }
            assert!(checked > 100);
        }
    }

    #[test]
    fn a_still_frame_draws_nothing() {
        for state in [State::Ready, State::Playing] {
            let mut g = Game::new(4);
            if state == State::Playing {
                g.start(0);
            }
            let mut px = screen();
            let mut fb = Fb::new(&mut px);
            let p = frame(&mut fb, &g, 10, None);
            let rects = fb.rects;
            assert!(rects > 0);
            frame(&mut fb, &g, 10, Some(p));
            assert_eq!(fb.rects, rects, "{state:?}");
        }
    }

    #[test]
    fn a_move_costs_a_handful_of_cells_not_the_screen() {
        let mut g = playing(5);
        let mut px = screen();
        let mut fb = Fb::new(&mut px);
        let p = frame(&mut fb, &g, 0, None);
        let first = fb.rects;
        g.shift(1, 10);
        fb.rects = 0;
        let p = frame(&mut fb, &g, 0, Some(p));
        let move_cost = fb.rects;
        // the piece and its guide change cells: at most 16 of them, a few rects each
        assert!(move_cost > 0 && move_cost < 700, "{move_cost}");
        assert!(move_cost * 8 < first, "{move_cost} vs a full paint of {first}");
        g.turn(20);
        fb.rects = 0;
        frame(&mut fb, &g, 0, Some(p));
        assert!(fb.rects < 900, "{}", fb.rects);
        // and a full repaint is cheap enough to do at any time
        let mut c = Count::default();
        frame(&mut c, &g, 0, None);
        assert!(c.rects < 3000, "{}", c.rects);
    }

    #[test]
    fn cells_stay_inside_their_squares() {
        // draw each kind of cell on a marked background and look for pixels outside the 20 px square
        for v in 0..=FLASH {
            if (9..GHOST).contains(&v) {
                continue;
            }
            let mut px = vec![0xABCDu16; (SIZE * SIZE) as usize];
            cell(&mut Fb::new(&mut px), 3, 4, v);
            let (x0, y0) = (BX + 3 * CELL, BY + 4 * CELL);
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let inside = x >= x0 && x < x0 + CELL && y >= y0 && y < y0 + CELL;
                    if !inside {
                        assert_eq!(px[(y * SIZE + x) as usize], 0xABCD, "value {v} spilled to ({x}, {y})");
                    }
                }
            }
        }
    }

    #[test]
    fn each_kind_has_its_own_colour_on_screen() {
        let mut seen = Vec::new();
        for k in 1..=7u8 {
            let mut px = screen();
            cell(&mut Fb::new(&mut px), 0, 0, k);
            seen.push(px[((BY + 10) * SIZE + BX + 8) as usize]);
        }
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 7);
    }

    #[test]
    fn the_ghost_shows_under_the_piece_and_not_over_blocks() {
        let mut g = playing(6);
        g.cur = Some(Piece { kind: 2, rot: 0, x: 3, y: 0 });
        let c = compose(&g);
        let gh = g.ghost().unwrap();
        for (x, y) in gh.cells() {
            assert_eq!(c[y as usize][x as usize], GHOST + 2);
        }
        for (x, y) in g.cur.unwrap().cells() {
            assert_eq!(c[y as usize][x as usize], 3);
        }
        // locked blocks are never overdrawn by the guide
        g.board[H - 1] = [4; W];
        let c = compose(&g);
        assert!(c[H - 1].iter().all(|&v| v == 4));
    }

    #[test]
    fn a_blinking_row_is_white_every_other_step() {
        let mut g = playing(2);
        for y in H - 1..H {
            for x in 1..W {
                g.board[y][x] = 2;
            }
        }
        g.cur = Some(Piece { kind: 0, rot: 1, x: -2, y: H as i32 - 4 });
        let mut t = 0;
        while g.flash.is_none() {
            t += 1;
            g.tick(t);
            assert!(t < 5000);
        }
        let mut saw = (false, false);
        for _ in 0..600 {
            t += 1;
            g.tick(t);
            if let Some(f) = g.flash {
                let c = compose(&g);
                if f.lit() {
                    assert_eq!(c[H - 1], [FLASH; W]);
                    saw.0 = true;
                } else {
                    assert_ne!(c[H - 1], [FLASH; W]);
                    saw.1 = true;
                }
            }
        }
        assert_eq!(saw, (true, true));
    }

    /// `PREVIEW=/abs/dir cargo test dump_preview`: raw RGB565 frames of the screen, for pictures.
    #[test]
    fn dump_preview() {
        let Ok(dir) = std::env::var("PREVIEW") else { return };
        let save = |name: &str, px: &[u16]| {
            let bytes: Vec<u8> = px.iter().flat_map(|v| v.to_le_bytes()).collect();
            std::fs::write(format!("{dir}/{name}.raw"), bytes).unwrap();
        };
        let g = Game::new(3);
        save("ready", &full(&g, 12400));
        // a game in progress: pile some random blocks, a falling piece, and a few cleared rows
        let mut g = playing(11);
        let rows: [&str; 8] = ["IIIIZZ.TTT", "OO.JJJ.ZZT", "OOSSJLLLZZ", "TTTSSLJJJZ", "ZZ.OOLJ.SS", ".ZZOOL.SSL", "LLL.TTTSSL", "LJJJ.T.ZZL"];
        for (i, r) in rows.iter().enumerate() {
            for (x, ch) in r.chars().enumerate() {
                g.board[H - 8 + i][x] = match ch {
                    'I' => 1,
                    'O' => 2,
                    'T' => 3,
                    'S' => 4,
                    'Z' => 5,
                    'J' => 6,
                    'L' => 7,
                    _ => 0,
                };
            }
        }
        g.score = 4350;
        g.lines = 23;
        g.cur = Some(Piece { kind: 2, rot: 1, x: 4, y: 6 });
        save("playing", &full(&g, 12400));
        let mut g2 = g.clone();
        g2.state = State::Over;
        save("over", &full(&g2, 12400));
    }
}
