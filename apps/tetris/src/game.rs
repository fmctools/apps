// The game itself: no Api, so `cargo test` runs it on the host. Time comes in as `now` (ms, wrapping), so the game
// never sleeps and never needs a fixed tick: `wake_in` says when something is next due, `tick` does it.

/// The well: 10 columns, 20 rows. Row 0 is the top.
pub const W: usize = 10;
pub const H: usize = 20;

/// Piece kinds, in the order of their colours: I O T S Z J L.
pub const KINDS: u8 = 7;
const I: u8 = 0;
const O: u8 = 1;

/// Milliseconds per row at levels 1..=20 (the last one holds from then on).
const GRAVITY: [u32; 20] = [800, 720, 630, 550, 470, 380, 300, 220, 130, 100, 83, 83, 83, 67, 67, 67, 50, 50, 50, 33];
/// Milliseconds per row while the finger is held on the screen.
pub const SOFT_MS: u32 = 45;
/// A piece that has landed can still be pushed around for this long; moving it gives the time back, 12 times at most.
pub const LOCK_MS: u32 = 500;
const SOFT_LOCK_MS: u32 = 120;
const MAX_RESETS: u8 = 12;
/// Cleared rows blink: this many steps of this many ms (odd steps are white), then they are gone.
pub const FLASH_STEPS: u8 = 6;
pub const FLASH_MS: u32 = 70;
/// Rows cleared at once: 1, 2, 3, 4. Times the level.
const LINE_POINTS: [u32; 5] = [0, 100, 300, 500, 800];
/// A score the HUD can show.
pub const SCORE_MAX: u32 = 999_999;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// Title screen, waiting for a tap.
    Ready,
    Playing,
    /// The stack reached the top.
    Over,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Nothing,
    /// A piece came to rest and joined the stack.
    Locked,
    /// A piece joined the stack and this many rows (1..=4) are going.
    Cleared(u8),
    Crashed,
}

/// A piece: its kind, how often it is turned clockwise (0..=3) and where the top left of its 4x4 box is. `y` can be
/// negative: the I piece spawns with its box one row above the well.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Piece {
    pub kind: u8,
    pub rot: u8,
    pub x: i32,
    pub y: i32,
}

/// The cells of each kind turned 0 times, in its box (the standard layout).
const BASE: [[(i32, i32); 4]; 7] = [
    [(0, 1), (1, 1), (2, 1), (3, 1)], // I
    [(1, 0), (2, 0), (1, 1), (2, 1)], // O
    [(1, 0), (0, 1), (1, 1), (2, 1)], // T
    [(1, 0), (2, 0), (0, 1), (1, 1)], // S
    [(0, 0), (1, 0), (1, 1), (2, 1)], // Z
    [(0, 0), (0, 1), (1, 1), (2, 1)], // J
    [(2, 0), (0, 1), (1, 1), (2, 1)], // L
];

/// The four cells of a kind turned `rot` times clockwise, relative to its box.
pub fn shape(kind: u8, rot: u8) -> [(i32, i32); 4] {
    let mut c = BASE[kind as usize % KINDS as usize];
    if kind == O {
        return c; // a square looks the same from every side
    }
    let n = if kind == I { 4 } else { 3 };
    for _ in 0..rot % 4 {
        for p in c.iter_mut() {
            *p = (n - 1 - p.1, p.0);
        }
    }
    c
}

impl Piece {
    /// Where its cells are in the well.
    pub fn cells(&self) -> [(i32, i32); 4] {
        shape(self.kind, self.rot).map(|(x, y)| (self.x + x, self.y + y))
    }
}

/// Where a turned piece may go if it does not fit where it is: sideways, then up.
const KICKS: [(i32, i32); 8] = [(0, 0), (-1, 0), (1, 0), (-2, 0), (2, 0), (0, -1), (-1, -1), (1, -1)];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Flash {
    /// Bit `y` is set for each full row.
    pub rows: u32,
    pub step: u8,
    at: u32,
}

impl Flash {
    /// Rows show white on odd steps.
    pub fn lit(&self) -> bool {
        self.step % 2 == 1
    }
}

#[derive(Clone)]
pub struct Game {
    pub state: State,
    /// 0 = empty, else the kind + 1 of the piece that left it there.
    pub board: [[u8; W]; H],
    /// The falling piece; `None` while rows blink.
    pub cur: Option<Piece>,
    pub next: u8,
    pub score: u32,
    pub lines: u32,
    pub flash: Option<Flash>,
    soft: bool,
    drop_at: u32,
    lock_at: Option<u32>,
    resets: u8,
    bag: [u8; 7],
    bag_at: usize,
    rng: u32,
}

pub fn due(now: u32, at: u32) -> bool {
    (now.wrapping_sub(at) as i32) >= 0
}

fn until(now: u32, at: u32) -> u32 {
    if due(now, at) { 0 } else { at.wrapping_sub(now) }
}

/// xorshift32: no random source on the watch, the app seeds it from the clock.
fn next(rng: &mut u32) -> u32 {
    let mut x = *rng;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *rng = x;
    x
}

impl Game {
    pub fn new(seed: u32) -> Game {
        Game {
            state: State::Ready,
            board: [[0; W]; H],
            cur: None,
            next: 0,
            score: 0,
            lines: 0,
            flash: None,
            soft: false,
            drop_at: 0,
            lock_at: None,
            resets: 0,
            bag: [0, 1, 2, 3, 4, 5, 6],
            bag_at: 7,
            rng: seed | 1,
        }
    }

    pub fn level(&self) -> u32 {
        (self.lines / 10).min(19)
    }

    /// Time between two falls of one row now.
    pub fn interval(&self) -> u32 {
        let g = GRAVITY[self.level() as usize];
        if self.soft { g.min(SOFT_MS) } else { g }
    }

    /// The next piece of the bag; every seven draws hold each kind once.
    fn draw(&mut self) -> u8 {
        if self.bag_at >= 7 {
            for i in (1..7).rev() {
                let j = (next(&mut self.rng) % (i as u32 + 1)) as usize;
                self.bag.swap(i, j);
            }
            self.bag_at = 0;
        }
        self.bag_at += 1;
        self.bag[self.bag_at - 1]
    }

    /// A new game, the first piece falling.
    pub fn start(&mut self, now: u32) {
        self.board = [[0; W]; H];
        self.score = 0;
        self.lines = 0;
        self.flash = None;
        self.soft = false;
        self.state = State::Playing;
        self.bag_at = 7;
        self.next = self.draw();
        self.spawn(now);
    }

    pub fn fits(&self, p: &Piece) -> bool {
        p.cells().iter().all(|&(x, y)| x >= 0 && x < W as i32 && y < H as i32 && (y < 0 || self.board[y as usize][x as usize] == 0))
    }

    /// The next piece comes in at the top. `false`: no room, the game is over.
    fn spawn(&mut self, now: u32) -> bool {
        let kind = self.next;
        self.next = self.draw();
        let p = Piece { kind, rot: 0, x: 3, y: if kind == I { -1 } else { 0 } };
        self.cur = Some(p);
        self.drop_at = now.wrapping_add(self.interval());
        self.lock_at = None;
        self.resets = 0;
        if self.fits(&p) {
            true
        } else {
            self.state = State::Over;
            false
        }
    }

    /// The piece can not fall further.
    pub fn resting(&self) -> bool {
        self.cur.is_some_and(|p| !self.fits(&Piece { y: p.y + 1, ..p }))
    }

    fn lock_delay(&self) -> u32 {
        if self.soft { SOFT_LOCK_MS } else { LOCK_MS }
    }

    /// After the piece moved or turned (`by_hand`) or fell: start, restart or cancel the lock timer.
    fn settle(&mut self, now: u32, by_hand: bool) {
        let delay = self.lock_delay();
        if !self.resting() {
            if self.lock_at.take().is_some() {
                self.drop_at = now.wrapping_add(self.interval()); // slid off the edge: it falls on
            }
        } else if self.lock_at.is_none() {
            self.lock_at = Some(now.wrapping_add(delay));
        } else if by_hand && self.resets < MAX_RESETS {
            self.resets += 1;
            self.lock_at = Some(now.wrapping_add(delay));
        }
    }

    /// One column left (-1) or right (+1).
    pub fn shift(&mut self, dx: i32, now: u32) -> bool {
        let Some(p) = self.cur.filter(|_| self.state == State::Playing && self.flash.is_none()) else { return false };
        let q = Piece { x: p.x + dx, ..p };
        if !self.fits(&q) {
            return false;
        }
        self.cur = Some(q);
        self.settle(now, true);
        true
    }

    /// A quarter turn clockwise, pushed aside or up if the wall or the stack is in the way.
    pub fn turn(&mut self, now: u32) -> bool {
        let Some(p) = self.cur.filter(|_| self.state == State::Playing && self.flash.is_none()) else { return false };
        if p.kind == O {
            return false;
        }
        for (dx, dy) in KICKS {
            let q = Piece { rot: (p.rot + 1) % 4, x: p.x + dx, y: p.y + dy, ..p };
            if self.fits(&q) {
                self.cur = Some(q);
                self.settle(now, true);
                return true;
            }
        }
        false
    }

    /// The finger is on (`true`) or off the screen: the piece falls faster while it is on.
    pub fn set_soft(&mut self, on: bool, now: u32) {
        if on == self.soft {
            return;
        }
        self.soft = on;
        if self.state != State::Playing || self.flash.is_some() {
            return;
        }
        if on {
            if until(now, self.drop_at) > SOFT_MS {
                self.drop_at = now.wrapping_add(SOFT_MS); // the first row comes soon, not after the long wait
            }
            if let Some(l) = self.lock_at {
                self.lock_at = Some(if until(now, l) > SOFT_LOCK_MS { now.wrapping_add(SOFT_LOCK_MS) } else { l });
            }
        } else {
            self.drop_at = now.wrapping_add(self.interval());
        }
    }

    /// Where the piece would land.
    pub fn ghost(&self) -> Option<Piece> {
        let mut p = self.cur?;
        while self.fits(&Piece { y: p.y + 1, ..p }) {
            p.y += 1;
        }
        Some(p)
    }

    /// Milliseconds until `tick` has something to do; `None`: nothing is due, wait for input.
    pub fn wake_in(&self, now: u32) -> Option<u32> {
        if let Some(f) = self.flash {
            return Some(until(now, f.at));
        }
        if self.state != State::Playing || self.cur.is_none() {
            return None;
        }
        Some(if self.resting() { until(now, self.lock_at.unwrap_or(now)) } else { until(now, self.drop_at) })
    }

    /// After a pause: nothing is overdue, and the finger is off.
    pub fn rebase(&mut self, now: u32) {
        self.soft = false;
        if let Some(f) = &mut self.flash {
            f.at = now.wrapping_add(FLASH_MS);
        }
        if self.lock_at.is_some() {
            self.lock_at = Some(now.wrapping_add(LOCK_MS));
        }
        self.drop_at = now.wrapping_add(self.interval());
    }

    /// Does what is due at `now`: one fall, the lock, or one blink.
    pub fn tick(&mut self, now: u32) -> Step {
        if let Some(mut f) = self.flash {
            if !due(now, f.at) {
                return Step::Nothing;
            }
            f.step += 1;
            f.at = now.wrapping_add(FLASH_MS);
            if f.step < FLASH_STEPS {
                self.flash = Some(f);
                return Step::Nothing;
            }
            self.flash = None;
            self.remove(f.rows);
            return if self.spawn(now) { Step::Nothing } else { Step::Crashed };
        }
        let Some(p) = self.cur.filter(|_| self.state == State::Playing) else { return Step::Nothing };
        if self.resting() {
            let at = *self.lock_at.get_or_insert(now.wrapping_add(self.lock_delay()));
            return if due(now, at) { self.lock(now) } else { Step::Nothing };
        }
        if due(now, self.drop_at) {
            self.cur = Some(Piece { y: p.y + 1, ..p });
            if self.soft {
                self.score = (self.score + 1).min(SCORE_MAX);
            }
            self.drop_at = now.wrapping_add(self.interval());
            self.settle(now, false);
        }
        Step::Nothing
    }

    fn lock(&mut self, now: u32) -> Step {
        let Some(p) = self.cur.take() else { return Step::Nothing };
        let mut out = false;
        for (x, y) in p.cells() {
            if y < 0 {
                out = true; // locked above the well
            } else {
                self.board[y as usize][x as usize] = p.kind + 1;
            }
        }
        if out {
            self.state = State::Over;
            return Step::Crashed;
        }
        let rows = (0..H).filter(|&y| self.board[y].iter().all(|&c| c != 0)).fold(0u32, |m, y| m | 1 << y);
        let n = rows.count_ones() as usize;
        if n == 0 {
            return if self.spawn(now) { Step::Locked } else { Step::Crashed };
        }
        self.score = (self.score + LINE_POINTS[n] * (self.level() + 1)).min(SCORE_MAX);
        self.lines += n as u32;
        self.flash = Some(Flash { rows, step: 0, at: now.wrapping_add(FLASH_MS) });
        Step::Cleared(n as u8)
    }

    /// Takes the rows in `rows` out; what is above falls down.
    fn remove(&mut self, rows: u32) {
        let mut to = H;
        for from in (0..H).rev() {
            if rows >> from & 1 == 0 {
                to -= 1;
                self.board[to] = self.board[from];
            }
        }
        for row in &mut self.board[..to] {
            *row = [0; W];
        }
    }
}

pub const MAGIC: &[u8; 2] = b"T1";

pub fn encode(best: u32) -> [u8; 6] {
    let b = best.to_le_bytes();
    [MAGIC[0], MAGIC[1], b[0], b[1], b[2], b[3]]
}

/// Anything that isn't a blob of ours is no best score yet.
pub fn decode(blob: &[u8]) -> u32 {
    match blob {
        [b'T', b'1', a, b, c, d] => u32::from_le_bytes([*a, *b, *c, *d]).min(SCORE_MAX),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing(seed: u32) -> Game {
        let mut g = Game::new(seed);
        g.start(0);
        g
    }

    /// Runs the game's clock until `ms` have passed, in 1 ms steps like a worst case of tiny wake-ups.
    fn run(g: &mut Game, from: u32, ms: u32) -> (u32, Vec<Step>) {
        let mut seen = Vec::new();
        for t in from..from + ms {
            let s = g.tick(t);
            if s != Step::Nothing {
                seen.push(s);
            }
        }
        (from + ms, seen)
    }

    #[test]
    fn every_piece_has_four_cells_in_its_box_and_turns_back_after_four() {
        for kind in 0..KINDS {
            for rot in 0..4 {
                let c = shape(kind, rot);
                for (i, a) in c.iter().enumerate() {
                    assert!(a.0 >= 0 && a.0 < 4 && a.1 >= 0 && a.1 < 4, "kind {kind} rot {rot} {a:?}");
                    assert!(!c[i + 1..].contains(a), "kind {kind} rot {rot} repeats {a:?}");
                }
            }
            assert_eq!(shape(kind, 4), shape(kind, 0));
        }
    }

    #[test]
    fn turning_keeps_the_pieces_shape() {
        // T: three in a row and one on top, whatever the turn: one cell touches the three others
        for rot in 0..4 {
            let c = shape(2, rot);
            let touching = |a: (i32, i32)| c.iter().filter(|b| (a.0 - b.0).abs() + (a.1 - b.1).abs() == 1).count();
            assert_eq!(c.iter().map(|&a| touching(a)).max(), Some(3), "rot {rot}");
        }
        // I: a line of four, flat or standing
        let flat = shape(0, 0);
        assert!(flat.iter().all(|c| c.1 == 1));
        let tall = shape(0, 1);
        assert!(tall.iter().all(|c| c.0 == 2));
    }

    #[test]
    fn the_bag_holds_each_kind_once_per_seven() {
        let mut g = Game::new(99);
        for _ in 0..20 {
            let mut seen = [0; 7];
            for _ in 0..7 {
                seen[g.draw() as usize] += 1;
            }
            assert_eq!(seen, [1; 7]);
        }
    }

    #[test]
    fn every_kind_spawns_inside_the_well_and_fits() {
        for kind in 0..KINDS {
            let g = playing(5);
            let p = Piece { kind, rot: 0, x: 3, y: if kind == I { -1 } else { 0 } };
            assert!(g.fits(&p), "kind {kind}");
            let top = p.cells().iter().map(|c| c.1).min().unwrap();
            assert_eq!(top, 0, "kind {kind} shows from the first row");
        }
    }

    #[test]
    fn a_piece_falls_one_row_per_interval() {
        let mut g = playing(1);
        let y0 = g.cur.unwrap().y;
        run(&mut g, 0, 799);
        assert_eq!(g.cur.unwrap().y, y0);
        run(&mut g, 799, 2);
        assert_eq!(g.cur.unwrap().y, y0 + 1);
        run(&mut g, 801, 800);
        assert_eq!(g.cur.unwrap().y, y0 + 2);
        assert_eq!(g.wake_in(1601).map(|w| w <= 800), Some(true));
    }

    #[test]
    fn it_lands_waits_half_a_second_and_locks() {
        let mut g = playing(3);
        // let it fall to the floor
        let mut t = 0;
        while !g.resting() {
            t += 1;
            g.tick(t);
            assert!(t < 30_000);
        }
        let landed = t;
        let (_, steps) = run(&mut g, landed + 1, LOCK_MS - 2);
        assert!(steps.is_empty(), "{steps:?}");
        let (_, steps) = run(&mut g, landed + LOCK_MS - 1, 10);
        assert_eq!(steps, vec![Step::Locked]);
        assert!(g.board.iter().flatten().filter(|&&c| c != 0).count() == 4);
        assert!(g.cur.is_some(), "the next piece is already in");
    }

    #[test]
    fn walls_stop_the_piece_and_moves_into_them_say_so() {
        let mut g = playing(7);
        let mut n = 0;
        while g.shift(-1, 0) {
            n += 1;
            assert!(n < 12);
        }
        assert!(g.cur.unwrap().cells().iter().any(|c| c.0 == 0));
        assert!(!g.shift(-1, 0));
        let mut m = 0;
        while g.shift(1, 0) {
            m += 1;
            assert!(m < 20);
        }
        assert!(g.cur.unwrap().cells().iter().any(|c| c.0 == W as i32 - 1));
        assert!(m >= 6);
    }

    #[test]
    fn a_turn_next_to_the_wall_kicks_the_piece_back_in() {
        // a standing I against the right wall turned flat would stick out: it is moved left
        let mut g = playing(7);
        g.cur = Some(Piece { kind: 0, rot: 1, x: 7, y: 5 }); // cells at x = 9
        assert!(g.fits(&g.cur.unwrap()));
        assert!(g.turn(0));
        let p = g.cur.unwrap();
        assert!(g.fits(&p));
        assert_eq!(p.rot, 2);
        assert!(p.cells().iter().all(|c| c.0 <= 9));
        // the square does not turn at all
        g.cur = Some(Piece { kind: 1, rot: 0, x: 3, y: 5 });
        assert!(!g.turn(0));
    }

    #[test]
    fn turning_is_refused_when_it_can_not_fit() {
        let mut g = playing(7);
        for y in 0..H {
            for x in 0..W {
                g.board[y][x] = 1;
            }
        }
        // a one-cell-wide shaft, a flat I at the bottom of it
        for y in 10..H {
            g.board[y][4] = 0;
        }
        g.cur = Some(Piece { kind: 0, rot: 1, x: 2, y: 10 }); // standing in column 4
        assert!(g.fits(&g.cur.unwrap()));
        let before = g.cur;
        assert!(!g.turn(0));
        assert_eq!(g.cur, before);
    }

    #[test]
    fn full_rows_blink_then_go_and_the_rest_falls() {
        let mut g = playing(2);
        // two full rows with a gap at column 0, and a single block on top of them
        for y in H - 2..H {
            for x in 1..W {
                g.board[y][x] = 3;
            }
        }
        g.board[H - 3][5] = 6;
        // a vertical I to fill column 0: x = -2 puts its cells in column 0
        g.cur = Some(Piece { kind: 0, rot: 1, x: -2, y: H as i32 - 4 });
        assert!(g.fits(&g.cur.unwrap()));
        let level = g.level();
        let mut t = 0;
        let mut got = None;
        while got.is_none() && t < 5000 {
            t += 1;
            let s = g.tick(t);
            if matches!(s, Step::Cleared(_)) {
                got = Some(s);
            }
        }
        assert_eq!(got, Some(Step::Cleared(2)));
        assert_eq!(g.score, 300 * (level + 1));
        assert_eq!(g.lines, 2);
        let f = g.flash.unwrap();
        assert_eq!(f.rows, 0b11 << (H - 2));
        assert!(g.cur.is_none());
        assert_eq!(g.wake_in(t), Some(FLASH_MS));
        // the blink runs, the rows go
        let mut lit = 0;
        for _ in 0..FLASH_STEPS as u32 * FLASH_MS + 5 {
            t += 1;
            if g.flash.is_some_and(|f| f.lit()) {
                lit += 1;
            }
            g.tick(t);
        }
        assert!(lit > 100, "the rows blinked white");
        assert!(g.flash.is_none());
        assert!(g.cur.is_some(), "the next piece is in");
        // what was on top fell two rows: the single block and the top of the I piece (column 0) are at the bottom
        assert_eq!(g.board[H - 1][5], 6);
        assert_eq!(g.board[H - 1][0], 1);
        assert_eq!(g.board[H - 2][0], 1);
        assert_eq!(g.board[H - 1].iter().filter(|&&c| c != 0).count(), 2);
        assert_eq!(g.board[H - 2].iter().filter(|&&c| c != 0).count(), 1);
        assert!(g.board[..H - 2].iter().flatten().all(|&c| c == 0));
    }

    #[test]
    fn a_tetris_scores_800_and_the_level_follows_the_lines() {
        let mut g = playing(2);
        for y in H - 4..H {
            for x in 1..W {
                g.board[y][x] = 2;
            }
        }
        g.cur = Some(Piece { kind: 0, rot: 1, x: -2, y: H as i32 - 4 });
        let mut t = 0;
        loop {
            t += 1;
            if let Step::Cleared(n) = g.tick(t) {
                assert_eq!(n, 4);
                break;
            }
            assert!(t < 5000);
        }
        assert_eq!(g.score, 800);
        g.lines = 9;
        assert_eq!(g.level(), 0);
        g.lines = 10;
        assert_eq!(g.level(), 1);
        g.lines = 10_000;
        assert_eq!(g.level(), 19);
        assert!(g.interval() == 33);
    }

    #[test]
    fn holding_makes_it_fall_fast_and_scores_a_point_per_row() {
        let mut g = playing(4);
        let y0 = g.cur.unwrap().y;
        g.set_soft(true, 10);
        // first row after SOFT_MS, then one per SOFT_MS
        run(&mut g, 10, SOFT_MS - 2);
        assert_eq!(g.cur.unwrap().y, y0);
        run(&mut g, 10 + SOFT_MS - 2, 3);
        assert_eq!(g.cur.unwrap().y, y0 + 1);
        run(&mut g, 10 + SOFT_MS + 1, 4 * SOFT_MS);
        assert_eq!(g.cur.unwrap().y, y0 + 5);
        assert_eq!(g.score, 5);
        // letting go: back to the level's speed
        let now = 10 + 5 * SOFT_MS + 1;
        g.set_soft(false, now);
        let y = g.cur.unwrap().y;
        run(&mut g, now, 700);
        assert_eq!(g.cur.unwrap().y, y);
        run(&mut g, now + 700, 150);
        assert_eq!(g.cur.unwrap().y, y + 1);
    }

    #[test]
    fn a_piece_on_the_floor_can_be_slid_but_not_forever() {
        let mut g = playing(3);
        let mut t = 0;
        while !g.resting() {
            t += 1;
            g.tick(t);
        }
        // slide every 400 ms: the timer restarts, up to the cap, then the piece locks anyway
        let landed = t;
        let mut left = true;
        let mut locked_at = None;
        for _ in 0..40 {
            t += 400;
            for k in 0..400 {
                if g.tick(t - 400 + k) != Step::Nothing {
                    locked_at = Some(t - 400 + k);
                    break;
                }
            }
            if locked_at.is_some() {
                break;
            }
            if !g.shift(if left { -1 } else { 1 }, t) {
                left = !left;
                g.shift(if left { -1 } else { 1 }, t);
            }
            if !g.resting() {
                break; // slid off the stack: fine for this test
            }
        }
        let n = (locked_at.expect("it locks") - landed) / 400;
        assert!(n >= 2 && n <= 2 + MAX_RESETS as u32 + 2, "{n}");
    }

    #[test]
    fn a_full_stack_ends_the_game() {
        let mut g = playing(8);
        for y in 0..H {
            for x in 0..W {
                g.board[y][x] = if x == 0 { 0 } else { 1 };
            }
        }
        // the next spawn has no room
        g.board[0][0] = 1;
        let mut t = 0;
        let mut over = false;
        while t < 3000 {
            t += 1;
            if g.tick(t) == Step::Crashed {
                over = true;
                break;
            }
        }
        assert!(over);
        assert_eq!(g.state, State::Over);
        assert_eq!(g.wake_in(t), None);
        assert!(!g.shift(1, t) && !g.turn(t));
    }

    #[test]
    fn the_ghost_is_where_the_piece_lands() {
        let mut g = playing(6);
        let gh = g.ghost().unwrap();
        assert!(g.fits(&gh));
        assert!(!g.fits(&Piece { y: gh.y + 1, ..gh }));
        assert_eq!((gh.kind, gh.rot, gh.x), (g.cur.unwrap().kind, g.cur.unwrap().rot, g.cur.unwrap().x));
        g.board[H - 1] = [1; W];
        let gh2 = g.ghost().unwrap();
        assert_eq!(gh2.y, gh.y - 1);
    }

    #[test]
    fn pausing_leaves_nothing_overdue() {
        let mut g = playing(6);
        let y = g.cur.unwrap().y;
        g.rebase(1_000_000);
        assert_eq!(g.tick(1_000_000), Step::Nothing);
        assert_eq!(g.cur.unwrap().y, y);
        assert!(g.wake_in(1_000_000).unwrap() > 700);
    }

    #[test]
    fn the_clock_may_wrap() {
        let mut g = Game::new(3);
        g.start(u32::MAX - 100);
        let y = g.cur.unwrap().y;
        let mut t = u32::MAX - 100;
        for _ in 0..900 {
            t = t.wrapping_add(1);
            g.tick(t);
        }
        assert_eq!(g.cur.unwrap().y, y + 1);
    }

    #[test]
    fn best_score_blob_round_trips_and_rejects_junk() {
        assert_eq!(decode(&encode(12345)), 12345);
        assert_eq!(decode(&[]), 0);
        assert_eq!(decode(b"SH1\x01\x00\x00\x00"), 0);
        assert_eq!(decode(&encode(u32::MAX)), SCORE_MAX);
    }

    /// Whatever is pressed, the stack never holds more than the well and every piece stays on the board.
    #[test]
    fn random_play_keeps_the_rules() {
        for seed in 1..40u32 {
            let mut g = playing(seed);
            let mut r = seed * 7919;
            let mut t = 0u32;
            for _ in 0..30_000 {
                t += 7;
                match next(&mut r) % 9 {
                    0 => {
                        g.shift(-1, t);
                    }
                    1 => {
                        g.shift(1, t);
                    }
                    2 => {
                        g.turn(t);
                    }
                    3 => g.set_soft(next(&mut r) % 2 == 0, t),
                    _ => {}
                }
                g.tick(t);
                if let Some(p) = g.cur {
                    if g.state == State::Playing {
                        assert!(g.fits(&p), "seed {seed} t {t}");
                    }
                }
                if g.state == State::Over {
                    g.start(t);
                }
            }
        }
    }
}
