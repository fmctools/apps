//! Pure game state machine for Vault Rush (no `Api`, runs under `cargo test` on host).
//! Supports Classic (streak), Heist (selectable 10s..60s timed win with gold), and Blitz (60s arcade),
//! plus smooth fixed-point transitions for flying into the bank, safe zoom-in, and door-opening gold reveal.

use cmf_wp2_ui::ONE;
use cmf_wp2_ui::motion::ease_out;

/// Number of detents around the safe dial (10° per detent).
pub const DIAL_STEPS: i32 = 36;
/// Maximum lives in Classic mode.
pub const CLASSIC_LIVES: u8 = 3;
/// Maximum pins on a Classic safe.
pub const MAX_PINS: usize = 5;
/// Selectable time options (seconds) in Heist mode.
pub const HEIST_TIMES_S: [u8; 6] = [10, 20, 30, 40, 50, 60];
/// Minimum steps in the required direction before a directional pin arms.
pub const ARM_STEPS_CLASSIC: u16 = 3;
pub const ARM_STEPS_HEIST: u16 = 2;

/// Timers (ms).
pub const BLITZ_START_MS: i32 = 60_000;
pub const BLITZ_EXACT_BONUS_MS: i32 = 3_000;
pub const BLITZ_GOOD_BONUS_MS: i32 = 1_000;
pub const BLITZ_MISS_PENALTY_MS: i32 = 4_000;
pub const SIREN_THRESHOLD_MS: i32 = 5_000;

/// Animation durations (ms).
pub const ENTER_BANK_MS: u32 = 620;
pub const ZOOM_MS: u32 = 520;
pub const UNLOCK_MS: u32 = 320;
pub const OPEN_MS: u32 = 560;
pub const GOLD_CLASSIC_HOLD_MS: u32 = 1_350;
pub const OVER_SLAM_MS: u32 = 440;
pub const OVER_ANIM_MS: u32 = 1_400;
pub const FLASH_MS: u32 = 520;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Classic,
    Heist,
    Blitz,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Menu,
    HeistSelect,
    /// Camera flies into the bank entrance and fades to dark before the safe appears.
    EnterBank,
    /// Safe fades in and zooms in to the dial wheel.
    ZoomIn,
    Playing,
    /// Dial wheel spins/shrinks off and locking bolts retract.
    Unlocking,
    /// Steel door swings open to reveal gold.
    Opening,
    /// Classic celebration pause before advancing to the next safe.
    ClassicGold,
    /// Heist victory screen showing the open safe full of gold.
    HeistWin,
    /// Failure / busted screen (behind jail bars).
    Over,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Cw,
    Ccw,
}

impl Dir {
    pub fn flip(self) -> Self {
        match self {
            Dir::Cw => Dir::Ccw,
            Dir::Ccw => Dir::Cw,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Zone {
    Cold,
    Warm,
    Hot,
    Exact,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flash {
    None,
    PinLock,
    ExactBonus,
    GoodBonus,
    Miss,
    WrongDir,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Feedback {
    None,
    ColdTick,
    WarmTick,
    HotTick,
    ExactTick,
    PinLock,
    SafeOpen,
    Miss,
    Warn,
    GameOver,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Records {
    pub classic: u32,
    pub blitz: u32,
    /// Fastest Heist clear time in ms for each `HEIST_TIMES_S` bucket (`0` = not cleared yet).
    pub heist_best_ms: [u32; 6],
}

#[derive(Clone)]
pub struct Game {
    pub state: State,
    pub mode: Mode,
    /// Menu focus (`0` = Classic, `1` = Heist, `2` = Blitz).
    pub menu_sel: usize,
    /// Selected Heist duration index (`0..6` -> `10s..60s`).
    pub heist_idx: usize,
    /// End-screen focus (`0` = Retry/Again, `1` = Menu).
    pub over_sel: usize,
    /// Current dial position `0..DIAL_STEPS`.
    pub dial: i32,
    /// Secret pin angle `0..DIAL_STEPS`.
    pub target: i32,
    /// Safes/locks cracked in this run.
    pub score: u32,
    /// Current pin index (`0..pin_count`).
    pub pin_idx: usize,
    /// Total pins in the current safe.
    pub pin_count: usize,
    /// Required turn direction for directional pins.
    pub expected_dir: Dir,
    /// Steps moved in `expected_dir` since the current pin started.
    pub dir_progress: u16,
    /// Remaining lives in Classic.
    pub lives: u8,
    /// Remaining countdown in Blitz / Heist (ms).
    pub time_ms: i32,
    /// Elapsed time in Heist (ms).
    pub elapsed_ms: u32,
    /// Consecutive exact hits in Blitz.
    pub combo: u32,
    /// Whether this run set a new record.
    pub new_best: bool,
    /// Transition progress (ms) for animations.
    pub anim_ms: u32,
    /// Transient LED/status flash while playing.
    pub flash: Flash,
    pub flash_left_ms: i32,
    rng: u32,
}

fn next(rng: &mut u32) -> u32 {
    let mut x = *rng;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *rng = x;
    x
}

pub fn ring_dist(a: i32, b: i32) -> i32 {
    let d = (a - b).rem_euclid(DIAL_STEPS);
    d.min(DIAL_STEPS - d)
}

pub fn zone_for_dist(d: i32) -> Zone {
    if d == 0 {
        Zone::Exact
    } else if d <= 2 {
        Zone::Hot
    } else if d <= 5 {
        Zone::Warm
    } else {
        Zone::Cold
    }
}

pub fn pins_for_safe(score: u32) -> usize {
    if score < 2 {
        3
    } else if score < 5 {
        4
    } else {
        MAX_PINS
    }
}

pub fn heist_pins_for_idx(idx: usize) -> usize {
    if idx <= 2 { 3 } else { 4 }
}

pub fn heist_penalty_for_idx(idx: usize) -> i32 {
    match idx {
        0 => 2_000,
        1 => 3_000,
        _ => 4_000,
    }
}

impl Game {
    pub fn new(seed: u32) -> Self {
        Game {
            state: State::Menu,
            mode: Mode::Classic,
            menu_sel: 0,
            heist_idx: 3, // default 40s
            over_sel: 0,
            dial: 0,
            target: 12,
            score: 0,
            pin_idx: 0,
            pin_count: 3,
            expected_dir: Dir::Cw,
            dir_progress: 0,
            lives: CLASSIC_LIVES,
            time_ms: BLITZ_START_MS,
            elapsed_ms: 0,
            combo: 0,
            new_best: false,
            anim_ms: 0,
            flash: Flash::None,
            flash_left_ms: 0,
            rng: seed | 1,
        }
    }

    pub fn zone(&self) -> Zone {
        zone_for_dist(ring_dist(self.dial, self.target))
    }

    pub fn arm_threshold(&self) -> u16 {
        match self.mode {
            Mode::Classic => ARM_STEPS_CLASSIC,
            Mode::Heist => ARM_STEPS_HEIST,
            Mode::Blitz => 0,
        }
    }

    pub fn armed(&self) -> bool {
        self.mode == Mode::Blitz || self.dir_progress >= self.arm_threshold()
    }

    /// Returns `0` when off, or `1`/`2` alternating police siren phases during the last 5 seconds.
    pub fn siren_phase(&self) -> u8 {
        if self.state == State::Playing
            && matches!(self.mode, Mode::Heist | Mode::Blitz)
            && self.time_ms > 0
            && self.time_ms <= SIREN_THRESHOLD_MS
        {
            (((self.time_ms / 250) & 1) + 1) as u8
        } else {
            0
        }
    }

    pub fn animating(&self) -> bool {
        matches!(
            self.state,
            State::EnterBank | State::ZoomIn | State::Unlocking | State::Opening | State::ClassicGold | State::HeistWin
        ) || (self.state == State::Over && self.anim_ms < OVER_ANIM_MS)
            || (self.state == State::Playing && (self.mode != Mode::Classic || self.flash != Flash::None))
    }

    pub fn anim_t(&self) -> i32 {
        let dur = match self.state {
            State::EnterBank => ENTER_BANK_MS,
            State::ZoomIn => ZOOM_MS,
            State::Unlocking => UNLOCK_MS,
            State::Opening => OPEN_MS,
            State::ClassicGold => GOLD_CLASSIC_HOLD_MS,
            _ => return ONE,
        };
        let linear = ((self.anim_ms.min(dur) as i32) * ONE / dur as i32).clamp(0, ONE);
        ease_out(linear)
    }

    pub fn open_heist_select(&mut self) {
        self.mode = Mode::Heist;
        self.menu_sel = 1;
        self.state = State::HeistSelect;
        self.flash = Flash::None;
        self.flash_left_ms = 0;
    }

    pub fn back_to_menu(&mut self) {
        self.state = State::Menu;
        self.menu_sel = match self.mode {
            Mode::Classic => 0,
            Mode::Heist => 1,
            Mode::Blitz => 2,
        };
        self.flash = Flash::None;
        self.flash_left_ms = 0;
        self.anim_ms = 0;
    }

    pub fn start_heist(&mut self, idx: usize) {
        self.heist_idx = idx.min(HEIST_TIMES_S.len() - 1);
        self.start(Mode::Heist);
    }

    pub fn start(&mut self, mode: Mode) {
        self.mode = mode;
        self.state = State::EnterBank;
        self.anim_ms = 0;
        self.over_sel = 0;
        self.score = 0;
        self.pin_idx = 0;
        self.lives = CLASSIC_LIVES;
        self.elapsed_ms = 0;
        self.combo = 0;
        self.new_best = false;
        self.flash = Flash::None;
        self.flash_left_ms = 0;
        self.expected_dir = Dir::Cw;
        self.dir_progress = 0;
        self.time_ms = match mode {
            Mode::Heist => HEIST_TIMES_S[self.heist_idx] as i32 * 1000,
            Mode::Blitz => BLITZ_START_MS,
            Mode::Classic => 0,
        };
        self.pin_count = match mode {
            Mode::Classic => pins_for_safe(0),
            Mode::Heist => heist_pins_for_idx(self.heist_idx),
            Mode::Blitz => 1,
        };
        self.target = self.pick_target(6);
    }

    fn setup_next_classic_safe(&mut self) {
        self.pin_idx = 0;
        self.pin_count = pins_for_safe(self.score);
        self.expected_dir = Dir::Cw;
        self.dir_progress = 0;
        self.target = self.pick_target(6);
        self.flash = Flash::None;
        self.flash_left_ms = 0;
        self.state = State::Playing;
    }

    fn pick_target(&mut self, min_away: i32) -> i32 {
        for _ in 0..16 {
            let candidate = (next(&mut self.rng) % DIAL_STEPS as u32) as i32;
            if ring_dist(candidate, self.dial) >= min_away {
                return candidate;
            }
        }
        (self.dial + DIAL_STEPS / 2) % DIAL_STEPS
    }

    fn set_flash(&mut self, f: Flash) {
        self.flash = f;
        self.flash_left_ms = FLASH_MS as i32;
    }

    fn enter_over(&mut self) {
        self.state = State::Over;
        self.over_sel = 0;
        self.anim_ms = 0;
        self.flash = Flash::None;
        self.flash_left_ms = 0;
    }

    pub fn crown(&mut self, delta: i16) -> Feedback {
        if delta == 0 {
            return Feedback::None;
        }
        match self.state {
            State::Menu => {
                let n = 3i32;
                self.menu_sel = (self.menu_sel as i32 + delta.signum() as i32).rem_euclid(n) as usize;
                self.mode = match self.menu_sel {
                    0 => Mode::Classic,
                    1 => Mode::Heist,
                    _ => Mode::Blitz,
                };
                Feedback::ColdTick
            }
            State::HeistSelect => {
                let n = HEIST_TIMES_S.len() as i32;
                self.heist_idx = (self.heist_idx as i32 + delta.signum() as i32).rem_euclid(n) as usize;
                Feedback::ColdTick
            }
            State::Over | State::HeistWin => {
                self.over_sel = 1 - self.over_sel;
                Feedback::ColdTick
            }
            State::Playing => {
                self.dial = (self.dial + delta as i32).rem_euclid(DIAL_STEPS);
                if self.mode != Mode::Blitz {
                    let turned = if delta > 0 { Dir::Cw } else { Dir::Ccw };
                    let mag = delta.unsigned_abs();
                    if turned == self.expected_dir {
                        self.dir_progress = self.dir_progress.saturating_add(mag).min(self.arm_threshold() + 10);
                    } else if self.dir_progress < self.arm_threshold() {
                        self.dir_progress = 0;
                    }
                }
                match self.zone() {
                    Zone::Exact => Feedback::ExactTick,
                    Zone::Hot => Feedback::HotTick,
                    Zone::Warm => Feedback::WarmTick,
                    Zone::Cold => Feedback::ColdTick,
                }
            }
            _ => Feedback::None,
        }
    }

    pub fn try_lock(&mut self, best: &mut Records) -> (Feedback, bool) {
        if self.state != State::Playing {
            return (Feedback::None, false);
        }
        let d = ring_dist(self.dial, self.target);
        match self.mode {
            Mode::Classic => {
                if !self.armed() {
                    self.set_flash(Flash::WrongDir);
                    return (Feedback::Warn, false);
                }
                if d == 0 {
                    self.pin_idx += 1;
                    if self.pin_idx >= self.pin_count {
                        self.score += 1;
                        let broke = self.record_classic(best);
                        self.state = State::Unlocking;
                        self.anim_ms = 0;
                        self.flash = Flash::None;
                        (Feedback::SafeOpen, broke)
                    } else {
                        self.expected_dir = self.expected_dir.flip();
                        self.dir_progress = 0;
                        self.target = self.pick_target(6);
                        self.set_flash(Flash::PinLock);
                        (Feedback::PinLock, false)
                    }
                } else {
                    self.lives = self.lives.saturating_sub(1);
                    if self.lives == 0 {
                        let broke = self.record_classic(best);
                        self.enter_over();
                        (Feedback::GameOver, broke)
                    } else {
                        self.set_flash(Flash::Miss);
                        (Feedback::Miss, false)
                    }
                }
            }
            Mode::Heist => {
                if !self.armed() {
                    self.set_flash(Flash::WrongDir);
                    return (Feedback::Warn, false);
                }
                if d == 0 {
                    self.pin_idx += 1;
                    if self.pin_idx >= self.pin_count {
                        self.score = 1;
                        let broke = self.record_heist(best);
                        self.state = State::Unlocking;
                        self.anim_ms = 0;
                        self.flash = Flash::None;
                        (Feedback::SafeOpen, broke)
                    } else {
                        self.expected_dir = self.expected_dir.flip();
                        self.dir_progress = 0;
                        self.target = self.pick_target(6);
                        self.set_flash(Flash::PinLock);
                        (Feedback::PinLock, false)
                    }
                } else {
                    let pen = heist_penalty_for_idx(self.heist_idx);
                    self.time_ms = (self.time_ms - pen).max(0);
                    if self.time_ms == 0 {
                        self.enter_over();
                        (Feedback::GameOver, false)
                    } else {
                        self.set_flash(Flash::Miss);
                        (Feedback::Miss, false)
                    }
                }
            }
            Mode::Blitz => {
                if d <= 1 {
                    self.score += 1;
                    let broke = self.record_blitz(best);
                    if d == 0 {
                        self.combo += 1;
                        self.time_ms = (self.time_ms + BLITZ_EXACT_BONUS_MS).min(99_900);
                        self.set_flash(Flash::ExactBonus);
                    } else {
                        self.combo = 0;
                        self.time_ms = (self.time_ms + BLITZ_GOOD_BONUS_MS).min(99_900);
                        self.set_flash(Flash::GoodBonus);
                    }
                    self.target = self.pick_target(5);
                    (Feedback::SafeOpen, broke)
                } else {
                    self.combo = 0;
                    self.time_ms = (self.time_ms - BLITZ_MISS_PENALTY_MS).max(0);
                    if self.time_ms == 0 {
                        let broke = self.record_blitz(best);
                        self.enter_over();
                        (Feedback::GameOver, broke)
                    } else {
                        self.set_flash(Flash::Miss);
                        (Feedback::Miss, false)
                    }
                }
            }
        }
    }

    pub fn skip_gold_hold(&mut self) {
        if self.state == State::ClassicGold {
            self.setup_next_classic_safe();
        }
    }

    pub fn tick(&mut self, dt_ms: u32, best: &mut Records) -> (Feedback, bool) {
        if self.flash != Flash::None {
            self.flash_left_ms -= dt_ms as i32;
            if self.flash_left_ms <= 0 {
                self.flash = Flash::None;
                self.flash_left_ms = 0;
            }
        }

        match self.state {
            State::EnterBank => {
                self.anim_ms = self.anim_ms.saturating_add(dt_ms);
                if self.anim_ms >= ENTER_BANK_MS {
                    self.anim_ms = 0;
                    self.state = State::ZoomIn;
                }
                (Feedback::None, false)
            }
            State::ZoomIn => {
                self.anim_ms = self.anim_ms.saturating_add(dt_ms);
                if self.anim_ms >= ZOOM_MS {
                    self.anim_ms = 0;
                    self.state = State::Playing;
                }
                (Feedback::None, false)
            }
            State::Unlocking => {
                self.anim_ms = self.anim_ms.saturating_add(dt_ms);
                if self.anim_ms >= UNLOCK_MS {
                    self.anim_ms = 0;
                    self.state = State::Opening;
                }
                (Feedback::None, false)
            }
            State::Opening => {
                self.anim_ms = self.anim_ms.saturating_add(dt_ms);
                if self.anim_ms >= OPEN_MS {
                    self.anim_ms = 0;
                    match self.mode {
                        Mode::Classic => self.state = State::ClassicGold,
                        Mode::Heist => {
                            self.over_sel = 0;
                            self.state = State::HeistWin;
                        }
                        Mode::Blitz => self.state = State::Playing,
                    }
                }
                (Feedback::None, false)
            }
            State::ClassicGold => {
                self.anim_ms = self.anim_ms.saturating_add(dt_ms);
                if self.anim_ms >= GOLD_CLASSIC_HOLD_MS {
                    self.setup_next_classic_safe();
                }
                (Feedback::None, false)
            }
            State::HeistWin => {
                self.anim_ms = self.anim_ms.wrapping_add(dt_ms);
                (Feedback::None, false)
            }
            State::Over => {
                if self.anim_ms < OVER_ANIM_MS {
                    self.anim_ms = self.anim_ms.saturating_add(dt_ms).min(OVER_ANIM_MS);
                }
                (Feedback::None, false)
            }
            State::Playing => match self.mode {
                Mode::Classic => (Feedback::None, false),
                Mode::Heist => {
                    self.elapsed_ms = self.elapsed_ms.saturating_add(dt_ms);
                    self.time_ms = (self.time_ms - dt_ms as i32).max(0);
                    if self.time_ms == 0 {
                        self.enter_over();
                        return (Feedback::GameOver, false);
                    }
                    (Feedback::None, false)
                }
                Mode::Blitz => {
                    self.time_ms = (self.time_ms - dt_ms as i32).max(0);
                    if self.time_ms == 0 {
                        let broke = self.record_blitz(best);
                        self.enter_over();
                        return (Feedback::GameOver, broke);
                    }
                    (Feedback::None, false)
                }
            },
            State::Menu | State::HeistSelect => (Feedback::None, false),
        }
    }

    fn record_classic(&mut self, best: &mut Records) -> bool {
        if self.score > best.classic {
            best.classic = self.score;
            self.new_best = true;
            true
        } else {
            false
        }
    }

    fn record_blitz(&mut self, best: &mut Records) -> bool {
        if self.score > best.blitz {
            best.blitz = self.score;
            self.new_best = true;
            true
        } else {
            false
        }
    }

    fn record_heist(&mut self, best: &mut Records) -> bool {
        let t = self.elapsed_ms.max(100);
        let slot = &mut best.heist_best_ms[self.heist_idx];
        if *slot == 0 || t < *slot {
            *slot = t;
            self.new_best = true;
            true
        } else {
            false
        }
    }
}

/// Encodes saved records: `"VC3"` + classic (`u32`) + blitz (`u32`) + 6 × heist_best_ms (`u32`).
pub fn encode(r: Records) -> [u8; 35] {
    let mut out = [0u8; 35];
    out[0..3].copy_from_slice(b"VC3");
    out[3..7].copy_from_slice(&r.classic.to_le_bytes());
    out[7..11].copy_from_slice(&r.blitz.to_le_bytes());
    for (i, &ms) in r.heist_best_ms.iter().enumerate() {
        let off = 11 + i * 4;
        out[off..off + 4].copy_from_slice(&ms.to_le_bytes());
    }
    out
}

/// Decodes `VC3` (migrating `VC2`/`VC1`); corrupted blobs return zeros.
pub fn decode(blob: &[u8]) -> Records {
    if blob.len() == 35 && &blob[0..3] == b"VC3" {
        let mut r = Records {
            classic: u32::from_le_bytes([blob[3], blob[4], blob[5], blob[6]]),
            blitz: u32::from_le_bytes([blob[7], blob[8], blob[9], blob[10]]),
            heist_best_ms: [0; 6],
        };
        for (i, slot) in r.heist_best_ms.iter_mut().enumerate() {
            let off = 11 + i * 4;
            *slot = u32::from_le_bytes([blob[off], blob[off + 1], blob[off + 2], blob[off + 3]]);
        }
        return r;
    }
    if blob.len() == 15 && &blob[0..3] == b"VC2" {
        let mut r = Records {
            classic: u32::from_le_bytes([blob[3], blob[4], blob[5], blob[6]]),
            blitz: u32::from_le_bytes([blob[7], blob[8], blob[9], blob[10]]),
            heist_best_ms: [0; 6],
        };
        r.heist_best_ms[3] = u32::from_le_bytes([blob[11], blob[12], blob[13], blob[14]]);
        return r;
    }
    if blob.len() == 11 && &blob[0..3] == b"VC1" {
        return Records {
            classic: u32::from_le_bytes([blob[3], blob[4], blob[5], blob[6]]),
            blitz: u32::from_le_bytes([blob[7], blob[8], blob[9], blob[10]]),
            heist_best_ms: [0; 6],
        };
    }
    Records::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_and_heist_time_selector_cycle_cleanly() {
        let mut g = Game::new(1);
        g.crown(1);
        assert_eq!((g.menu_sel, g.mode), (1, Mode::Heist));
        g.open_heist_select();
        assert_eq!(g.state, State::HeistSelect);
        assert_eq!(HEIST_TIMES_S[g.heist_idx], 40);

        g.crown(1);
        assert_eq!(HEIST_TIMES_S[g.heist_idx], 50);
        g.crown(1);
        assert_eq!(HEIST_TIMES_S[g.heist_idx], 60);
        g.crown(1);
        assert_eq!(HEIST_TIMES_S[g.heist_idx], 10);
        g.crown(-1);
        assert_eq!(HEIST_TIMES_S[g.heist_idx], 60);
    }

    #[test]
    fn enter_bank_zoom_in_and_siren_phases() {
        let mut best = Records::default();
        let mut g = Game::new(55);
        g.start_heist(0); // 10s
        assert_eq!(g.state, State::EnterBank);
        assert_eq!(g.siren_phase(), 0);
        g.tick(ENTER_BANK_MS, &mut best);
        assert_eq!(g.state, State::ZoomIn);
        g.tick(ZOOM_MS, &mut best);
        assert_eq!(g.state, State::Playing);
        assert_eq!(g.siren_phase(), 0);

        // Advance into the final 5 seconds -> police siren alternates 1 / 2.
        g.tick(6_000, &mut best);
        assert_eq!(g.time_ms, 4_000);
        let p1 = g.siren_phase();
        assert!(p1 == 1 || p1 == 2);
        g.tick(250, &mut best);
        let p2 = g.siren_phase();
        assert!(p2 == 1 || p2 == 2);
        assert_ne!(p1, p2);
    }

    #[test]
    fn heist_selected_duration_sets_timer_pins_and_bucket_record() {
        let mut best = Records::default();
        let mut g = Game::new(55);
        g.start_heist(0);
        g.tick(ENTER_BANK_MS, &mut best);
        g.tick(ZOOM_MS, &mut best);
        assert_eq!(g.time_ms, 10_000);
        assert_eq!(g.pin_count, 3);

        g.tick(4_300, &mut best);
        for pin in 0..g.pin_count {
            let dir = if g.expected_dir == Dir::Cw { 1 } else { -1 };
            for _ in 0..ARM_STEPS_HEIST {
                g.crown(dir);
            }
            while g.dial != g.target {
                g.crown(dir);
            }
            let (fb, broke) = g.try_lock(&mut best);
            if pin + 1 == 3 {
                assert_eq!(fb, Feedback::SafeOpen);
                assert!(broke);
                assert_eq!(best.heist_best_ms[0], 4_300);
                assert_eq!(best.heist_best_ms[3], 0);
            } else {
                assert_eq!(fb, Feedback::PinLock);
            }
        }
        g.tick(UNLOCK_MS, &mut best);
        g.tick(OPEN_MS, &mut best);
        assert_eq!(g.state, State::HeistWin);
    }

    #[test]
    fn heist_timeout_ends_in_over() {
        let mut best = Records::default();
        let mut g = Game::new(10);
        g.start_heist(1); // 20s
        g.tick(ENTER_BANK_MS, &mut best);
        g.tick(ZOOM_MS, &mut best);
        let (fb, _) = g.tick(20_000, &mut best);
        assert_eq!(fb, Feedback::GameOver);
        assert_eq!(g.state, State::Over);
        assert_eq!(g.anim_ms, 0);
        assert!(g.animating());
        assert_eq!(best.heist_best_ms[1], 0);

        g.tick(OVER_SLAM_MS, &mut best);
        assert_eq!(g.anim_ms, OVER_SLAM_MS);
        assert!(g.animating());
        g.tick(OVER_ANIM_MS, &mut best);
        assert_eq!(g.anim_ms, OVER_ANIM_MS);
        assert!(!g.animating());
    }

    #[test]
    fn storage_roundtrip_and_migrations() {
        let r = Records { classic: 5, blitz: 14, heist_best_ms: [4200, 0, 11200, 18400, 0, 29000] };
        assert_eq!(decode(&encode(r)), r);
        let mut vc2 = [0u8; 15];
        vc2[0..3].copy_from_slice(b"VC2");
        vc2[3..7].copy_from_slice(&3u32.to_le_bytes());
        vc2[7..11].copy_from_slice(&9u32.to_le_bytes());
        vc2[11..15].copy_from_slice(&17_500u32.to_le_bytes());
        assert_eq!(decode(&vc2).heist_best_ms[3], 17_500);
        assert_eq!(decode(b"bad"), Records::default());
    }
}
