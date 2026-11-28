//! Pure Lake Cast game state: open shoreline beach + full lake canvas, 3 roaming fish,
//! dynamic cast target anywhere on water, and tuned vertical bar fishing duel.

use crate::sprites::{
    Sprite, SPRITE_CARP, SPRITE_KOI, SPRITE_PERCH, SPRITE_PIKE, SPRITE_ROACH, SPRITE_TROUT,
};
use cmf_wp2_ui::motion::isqrt;
use cmf_wp2_ui::Rgb;

pub const SIZE: i32 = 466;
pub const C: i32 = SIZE / 2;

/// Shoreline splits upper/mid open water (`0..SHORE_Y`) from sunny foreground beach (`SHORE_Y..SIZE`).
pub const SHORE_Y: i32 = 326;
pub const CAST_X_MIN: i32 = 106; // keeps bobber & ripples strictly clear of left vertical gauge
pub const CAST_X_MAX: i32 = SIZE - 42;
pub const CAST_Y_MIN: i32 = 82;
pub const CAST_Y_MAX: i32 = SHORE_Y - 14;
pub const CAST_R_MAX: i32 = 200;

/// Left vertical catch bar geometry (shown during `Reeling`).
pub const GAUGE_X: i32 = 34;
pub const GAUGE_Y: i32 = 112;
pub const GAUGE_W: i32 = 46;
pub const GAUGE_H: i32 = 198;
pub const TRACK_X: i32 = GAUGE_X + 5;
pub const TRACK_Y: i32 = GAUGE_Y + 7;
pub const TRACK_W: i32 = 24;
pub const TRACK_H: i32 = GAUGE_H - 14; // 184 px
pub const PROG_X: i32 = TRACK_X + TRACK_W + 4;
pub const PROG_W: i32 = 8;

pub const DERBY_MS: u32 = 90_000;
pub const CAST_FLIGHT_MS: u32 = 220;
pub const HOOK_WINDOW_MS: u32 = 1_300;
pub const RESULT_BANNER_MS: u32 = 1_100;
pub const CATCH_GUARD_MS: u32 = 360;
pub const CONFIRM_FLASH_MS: u32 = 160;
pub const MAX_FISH: usize = 6;

/// Top HUD tappable menu pill & mode/score pill geometry.
pub const HUD_Y: i32 = 34;
pub const HUD_H: i32 = 28;
pub const HUD_MENU_X: i32 = 118;
pub const HUD_MENU_W: i32 = 76;
pub const HUD_INFO_X: i32 = 200;
pub const HUD_INFO_W: i32 = 148;

/// Menu buttons geometry (centred symmetrically on 466px circle).
pub const MENU_BTN_X: i32 = 123;
pub const MENU_BTN_W: i32 = 220;
pub const MENU_BTN_H: i32 = 52;
pub const MENU_RELAX_Y: i32 = 142;
pub const MENU_DERBY_Y: i32 = 204;
pub const MENU_TROPHY_Y: i32 = 266;

/// Over / Derby summary & Caught continue buttons.
pub const OVER_BTN_X: i32 = 133;
pub const OVER_BTN_W: i32 = 200;
pub const OVER_BTN_H: i32 = 48;
pub const OVER_RETRY_Y: i32 = 286;
pub const OVER_MENU_Y: i32 = 344;

pub const CAUGHT_BTN_X: i32 = 148;
pub const CAUGHT_BTN_Y: i32 = 328;
pub const CAUGHT_BTN_W: i32 = 170;
pub const CAUGHT_BTN_H: i32 = 36;

/// Trophies prev/next arrow buttons.
pub const TROPHY_PREV_X: i32 = 48;
pub const TROPHY_NEXT_X: i32 = 364;
pub const TROPHY_NAV_W: i32 = 54;
pub const TROPHY_NAV_Y: i32 = 198;
pub const TROPHY_NAV_H: i32 = 56;

pub fn in_rect(x: i32, y: i32, rx: i32, ry: i32, rw: i32, rh: i32) -> bool {
    x >= rx && x < rx + rw && y >= ry && y < ry + rh
}

pub fn in_menu_back_pill(x: i32, y: i32) -> bool {
    in_rect(x, y, HUD_MENU_X - 4, HUD_Y - 4, HUD_MENU_W + 8, HUD_H + 8)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Relax,
    Derby,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Difficulty {
    Easy = 0,
    Medium = 1,
    Hard = 2,
}

pub const DIFFICULTIES: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DifficultyTuning {
    pub start_progress: i32,
    pub gain_per_s: i32,
    pub decay_per_s: i32,
    pub step_px: i32,
    pub accel_mul: i32,
    pub min_accel_dv: i32,
    pub overlap_pad: i32,
    pub jump_min: i32,
    pub jump_max: i32,
}

impl Difficulty {
    pub const fn idx(self) -> usize {
        self as usize
    }

    pub const fn name(self) -> &'static str {
        match self {
            Difficulty::Easy => "EASY",
            Difficulty::Medium => "MEDIUM",
            Difficulty::Hard => "HARD",
        }
    }

    pub const fn desc(self) -> &'static str {
        match self {
            Difficulty::Easy => "WIDE BAR - GENTLE",
            Difficulty::Medium => "BALANCED 50/50",
            Difficulty::Hard => "FAST FISH - NARROW",
        }
    }

    pub const fn short(self) -> &'static str {
        match self {
            Difficulty::Easy => "E",
            Difficulty::Medium => "M",
            Difficulty::Hard => "H",
        }
    }

    pub const fn accent(self) -> Rgb {
        match self {
            Difficulty::Easy => Rgb(84, 226, 114),
            Difficulty::Medium => Rgb(255, 212, 76),
            Difficulty::Hard => Rgb(255, 112, 86),
        }
    }

    pub const fn tuning(self) -> DifficultyTuning {
        match self {
            // Exact classic balance from before hardening.
            Difficulty::Easy => DifficultyTuning {
                start_progress: 320,
                gain_per_s: 220,
                decay_per_s: 115,
                step_px: 14,
                accel_mul: 4,
                min_accel_dv: 2,
                overlap_pad: 4,
                jump_min: 0,
                jump_max: 0,
            },
            // True 50/50 midpoint between Easy and Hard.
            Difficulty::Medium => DifficultyTuning {
                start_progress: 280,
                gain_per_s: 198,
                decay_per_s: 130,
                step_px: 13,
                accel_mul: 5,
                min_accel_dv: 3,
                overlap_pad: 3,
                jump_min: 28,
                jump_max: 92,
            },
            // Tight bar & darting fish balance.
            Difficulty::Hard => DifficultyTuning {
                start_progress: 240,
                gain_per_s: 175,
                decay_per_s: 145,
                step_px: 12,
                accel_mul: 6,
                min_accel_dv: 4,
                overlap_pad: 3,
                jump_min: 42,
                jump_max: 118,
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Species {
    Roach = 0,
    Perch = 1,
    Trout = 2,
    Carp = 3,
    Pike = 4,
    GoldenKoi = 5,
}

pub const SPECIES_ALL: [Species; 6] = [
    Species::Roach,
    Species::Perch,
    Species::Trout,
    Species::Carp,
    Species::Pike,
    Species::GoldenKoi,
];

impl Species {
    pub const fn idx(self) -> usize {
        self as usize
    }

    pub const fn name(self) -> &'static str {
        match self {
            Species::Roach => "SILVER ROACH",
            Species::Perch => "STRIPED PERCH",
            Species::Trout => "RAINBOW TROUT",
            Species::Carp => "AMBER CARP",
            Species::Pike => "NORTHERN PIKE",
            Species::GoldenKoi => "GOLDEN KOI",
        }
    }

    pub const fn rarity(self) -> &'static str {
        match self {
            Species::Roach | Species::Perch => "COMMON",
            Species::Trout | Species::Carp => "UNCOMMON",
            Species::Pike => "RARE",
            Species::GoldenKoi => "LEGENDARY",
        }
    }

    pub const fn sprite(self) -> &'static Sprite {
        match self {
            Species::Roach => &SPRITE_ROACH,
            Species::Perch => &SPRITE_PERCH,
            Species::Trout => &SPRITE_TROUT,
            Species::Carp => &SPRITE_CARP,
            Species::Pike => &SPRITE_PIKE,
            Species::GoldenKoi => &SPRITE_KOI,
        }
    }

    pub const fn accent(self) -> Rgb {
        match self {
            Species::Roach => Rgb(134, 214, 255),
            Species::Perch => Rgb(186, 224, 92),
            Species::Trout => Rgb(244, 138, 176),
            Species::Carp => Rgb(246, 182, 72),
            Species::Pike => Rgb(96, 224, 132),
            Species::GoldenKoi => Rgb(255, 208, 66),
        }
    }

    /// (min_hg, max_hg, base_pts)
    pub const fn stats(self) -> (u16, u16, u32) {
        match self {
            Species::Roach => (25, 165, 45),
            Species::Perch => (35, 220, 65),
            Species::Trout => (95, 440, 120),
            Species::Carp => (150, 680, 150),
            Species::Pike => (260, 1120, 235),
            Species::GoldenKoi => (340, 1450, 440),
        }
    }

    /// Difficulty-tuned bar duel parameters: `(bar_h_px, fish_speed_px_s, ai_min_ms, ai_max_ms)`.
    pub const fn fight(self, diff: Difficulty) -> (i32, i32, i32, i32) {
        let (eb, es, elo, ehi, hb, hs, hlo, hhi) = match self {
            Species::Roach => (58, 42, 750, 1250, 42, 66, 420, 820),
            Species::Perch => (54, 52, 650, 1100, 38, 82, 340, 700),
            Species::Trout => (48, 72, 480, 900, 34, 108, 240, 560),
            Species::Carp => (46, 60, 550, 1000, 32, 94, 280, 620),
            Species::Pike => (42, 92, 380, 760, 28, 134, 180, 450),
            Species::GoldenKoi => (38, 106, 340, 680, 25, 154, 150, 380),
        };
        match diff {
            Difficulty::Easy => (eb, es, elo, ehi),
            Difficulty::Medium => ((eb + hb) / 2, (es + hs) / 2, (elo + hlo) / 2, (ehi + hhi) / 2),
            Difficulty::Hard => (hb, hs, hlo, hhi),
        }
    }

    /// Underwater silhouette size & cruise speed (w, h, speed_px_per_s).
    pub const fn silhouette(self) -> (i32, i32, i32) {
        match self {
            Species::Roach => (24, 11, 25),
            Species::Perch => (26, 13, 23),
            Species::Trout => (30, 12, 32),
            Species::Carp => (29, 15, 19),
            Species::Pike => (36, 11, 29),
            Species::GoldenKoi => (32, 15, 24),
        }
    }

    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Species::Roach),
            1 => Some(Species::Perch),
            2 => Some(Species::Trout),
            3 => Some(Species::Carp),
            4 => Some(Species::Pike),
            5 => Some(Species::GoldenKoi),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Feedback {
    None,
    Tap,
    CastSplash,
    Nibble,
    Bite,
    BarTick,
    CatchHold,
    Caught,
    Escape,
    DerbyEnd,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BannerKind {
    Escaped,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PendingNav {
    OpenDifficulty(Mode),
    OpenTrophies,
    StartGame(Difficulty),
    RetryDerby,
    BackToMenu,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Menu,
    Difficulty,
    Trophies,
    Aim,
    Casting,
    Floating,
    Nibble,
    Bite,
    Reeling,
    Caught,
    Banner,
    DerbyOver,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FishAi {
    Cruise,
    Curious,
    Stalking,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LakeFish {
    pub species: Species,
    pub x_q8: i32,
    pub y_q8: i32,
    pub vx_q8: i32,
    pub vy_q8: i32,
    pub phase_ms: u16,
    pub weight_hg: u16,
    pub ai: FishAi,
    pub spook_ms: u16,
}

impl LakeFish {
    pub fn x(&self) -> i32 {
        self.x_q8 >> 8
    }

    pub fn y(&self) -> i32 {
        self.y_q8 >> 8
    }

    pub fn facing_right(&self) -> bool {
        self.vx_q8 >= 0
    }
}

/// Sleek rod extending from the sandy beach across the shoreline toward cast spot `(bx, by)`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RodPose {
    pub grip_x: i32,
    pub grip_y: i32,
    pub ctrl_x: i32,
    pub ctrl_y: i32,
    pub tip_x: i32,
    pub tip_y: i32,
}

/// Clamps any tap on the water cleanly inside the round bezel and right of the vertical bar zone.
pub fn clamp_cast(tx: i32, ty: i32) -> (i32, i32) {
    let mut y = ty.clamp(CAST_Y_MIN, CAST_Y_MAX);
    let mut x = tx.clamp(CAST_X_MIN, CAST_X_MAX);
    let dx = x - C;
    let dy = y - C;
    let d2 = dx * dx + dy * dy;
    let max_d2 = CAST_R_MAX * CAST_R_MAX;
    if d2 > max_d2 {
        let d = isqrt(d2).max(1);
        x = C + dx * CAST_R_MAX / d;
        y = (C + dy * CAST_R_MAX / d).clamp(CAST_Y_MIN, CAST_Y_MAX);
        let dy2 = y - C;
        let max_dx = isqrt((max_d2 - dy2 * dy2).max(0));
        x = x.clamp((C - max_dx).max(CAST_X_MIN), (C + max_dx).min(CAST_X_MAX));
    }
    (x, y)
}

/// Rod rooted on the sunny sand (`y ~ 420..446`), reaching straight over the shore line (`y ~ 282..308`).
pub fn rod_pose(bx: i32, by: i32, bend: i32) -> RodPose {
    let grip_x = (bx + if bx <= C { 10 } else { -10 }).clamp(124, 342);
    let dx_c = grip_x - C;
    let max_bottom = C + isqrt((C * C - dx_c * dx_c).max(0)) - 22;
    let grip_y = max_bottom.clamp(388, 444);

    let aim_x = grip_x + (bx - grip_x) * 42 / 100;
    let tip_x = aim_x.clamp(112, SIZE - 62);
    let base_tip_y = (302 - (SHORE_Y - by).max(0) / 8).clamp(272, 308);
    let flex = bend.clamp(0, 256);
    let tip_y = base_tip_y + flex * 12 / 256;

    let ctrl_x = (grip_x + tip_x) / 2 + if tip_x < grip_x { -5 } else { 5 };
    let ctrl_y = (grip_y + tip_y) / 2 - 6;

    RodPose { grip_x, grip_y, ctrl_x, ctrl_y, tip_x, tip_y }
}

/// Persistent catch records & stats across sessions (`43` bytes).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Records {
    pub relax_best: u32,
    pub derby_best: u32,
    pub total_caught: u32,
    pub biggest_species: Option<Species>,
    pub biggest_hg: u16,
    pub species_count: [u16; 6],
    pub species_best_hg: [u16; 6],
}

impl Records {
    pub fn species_unlocked(&self) -> usize {
        self.species_count.iter().filter(|&&c| c > 0).count()
    }

    pub fn best_overall_score(&self) -> u32 {
        self.relax_best.max(self.derby_best)
    }
}

const SAVE_MAGIC: [u8; 4] = *b"LC01";
pub const SAVE_LEN: usize = 43;

pub fn encode(r: Records) -> [u8; SAVE_LEN] {
    let mut b = [0u8; SAVE_LEN];
    b[0..4].copy_from_slice(&SAVE_MAGIC);
    b[4..8].copy_from_slice(&r.relax_best.to_le_bytes());
    b[8..12].copy_from_slice(&r.derby_best.to_le_bytes());
    b[12..16].copy_from_slice(&r.total_caught.to_le_bytes());
    b[16] = r.biggest_species.map_or(255, |s| s as u8);
    b[17..19].copy_from_slice(&r.biggest_hg.to_le_bytes());
    for i in 0..6 {
        let c = r.species_count[i].to_le_bytes();
        let w = r.species_best_hg[i].to_le_bytes();
        b[19 + i * 2..21 + i * 2].copy_from_slice(&c);
        b[31 + i * 2..33 + i * 2].copy_from_slice(&w);
    }
    b
}

pub fn decode(b: &[u8]) -> Records {
    if b.len() < SAVE_LEN || b[0..4] != SAVE_MAGIC {
        return Records::default();
    }
    let u32_at = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let u16_at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
    let mut r = Records {
        relax_best: u32_at(4),
        derby_best: u32_at(8),
        total_caught: u32_at(12),
        biggest_species: Species::from_u8(b[16]),
        biggest_hg: u16_at(17),
        species_count: [0; 6],
        species_best_hg: [0; 6],
    };
    for i in 0..6 {
        r.species_count[i] = u16_at(19 + i * 2);
        r.species_best_hg[i] = u16_at(31 + i * 2);
    }
    r
}

pub struct Game {
    pub state: State,
    pub mode: Mode,
    pub pending_mode: Mode,
    pub difficulty: Difficulty,
    pub menu_sel: usize,
    pub diff_sel: usize,
    pub trophy_idx: usize,
    pub over_sel: usize,
    pub score: u32,
    pub session_catches: u16,
    pub session_best_species: Option<Species>,
    pub session_best_hg: u16,
    pub derby_left_ms: u32,
    pub new_best_score: bool,

    pub fish: [LakeFish; MAX_FISH],
    pub lake_ms: u32,

    /// Current cast / bobber spot in the lake.
    pub bx: i32,
    pub by: i32,
    pub has_cast: bool,

    /// Cast / wait / bite timers.
    pub cast_ms: u32,
    pub wait_ms: u32,
    pub splash_ms: u32,
    pub nibbles_left: u8,
    pub target_fish: usize,
    pub bite_left_ms: u32,
    pub will_bite: bool,

    /// Vertical bar fight state (left-hand gauge).
    pub reel_species: Species,
    pub reel_weight_hg: u16,
    pub reel_progress: i32,
    pub bar_y: i32,
    pub bar_h: i32,
    pub track_fish_y_q8: i32,
    pub track_fish_vy_q8: i32,
    pub track_target_y: i32,
    pub track_ai_ms: u32,
    pub fish_in_bar: bool,
    pub reel_phase_ms: u32,

    /// Caught modal info.
    pub caught_species: Species,
    pub caught_hg: u16,
    pub caught_pts: u32,
    pub caught_record: bool,
    pub caught_guard_ms: u32,

    /// Banner state (Escaped).
    pub banner: BannerKind,
    pub banner_ms: u32,

    /// Brief pressed-button flash before switching screen/starting play.
    pub confirm_ms: u32,
    pub pending_nav: Option<PendingNav>,

    rng: u32,
}

impl Game {
    pub fn new(seed: u32) -> Self {
        let mut g = Game {
            state: State::Menu,
            mode: Mode::Relax,
            pending_mode: Mode::Relax,
            difficulty: Difficulty::Medium,
            menu_sel: 0,
            diff_sel: Difficulty::Medium.idx(),
            trophy_idx: 0,
            over_sel: 0,
            score: 0,
            session_catches: 0,
            session_best_species: None,
            session_best_hg: 0,
            derby_left_ms: DERBY_MS,
            new_best_score: false,
            fish: [LakeFish {
                species: Species::Roach,
                x_q8: C << 8,
                y_q8: 180 << 8,
                vx_q8: 20 << 8,
                vy_q8: 0,
                phase_ms: 0,
                weight_hg: 80,
                ai: FishAi::Cruise,
                spook_ms: 0,
            }; MAX_FISH],
            lake_ms: 0,
            bx: 186,
            by: 274,
            has_cast: false,
            cast_ms: 0,
            wait_ms: 0,
            splash_ms: 0,
            nibbles_left: 0,
            target_fish: 0,
            bite_left_ms: 0,
            will_bite: false,
            reel_species: Species::Roach,
            reel_weight_hg: 80,
            reel_progress: 240,
            bar_y: (TRACK_H - 38) / 2,
            bar_h: 38,
            track_fish_y_q8: (TRACK_H / 2) << 8,
            track_fish_vy_q8: 0,
            track_target_y: TRACK_H / 2,
            track_ai_ms: 0,
            fish_in_bar: true,
            reel_phase_ms: 0,
            caught_species: Species::Roach,
            caught_hg: 80,
            caught_pts: 45,
            caught_record: false,
            caught_guard_ms: 0,
            banner: BannerKind::Escaped,
            banner_ms: 0,
            confirm_ms: 0,
            pending_nav: None,
            rng: seed | 1,
        };
        for i in 0..MAX_FISH {
            g.respawn_fish(i, true);
        }
        g
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    fn rand_range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next_u32() % (hi - lo + 1) as u32) as i32
    }

    fn pick_species(&mut self, y: i32) -> Species {
        let roll = self.rand_range(0, 99);
        if y < 145 {
            // Far deep upper lake has larger rare fish & Koi!
            match roll {
                0..=34 => Species::Pike,
                35..=58 => Species::Carp,
                59..=78 => Species::Trout,
                79..=93 => Species::GoldenKoi,
                _ => Species::Perch,
            }
        } else if y < 235 {
            match roll {
                0..=26 => Species::Perch,
                27..=54 => Species::Trout,
                55..=76 => Species::Carp,
                77..=89 => Species::Pike,
                90..=96 => Species::GoldenKoi,
                _ => Species::Roach,
            }
        } else {
            // Shallow nearshore water.
            match roll {
                0..=38 => Species::Roach,
                39..=74 => Species::Perch,
                75..=90 => Species::Trout,
                91..=97 => Species::Carp,
                _ => Species::GoldenKoi,
            }
        }
    }

    fn roll_weight(&mut self, sp: Species) -> u16 {
        let (min_w, max_w, _) = sp.stats();
        let a = self.rand_range(min_w as i32, max_w as i32);
        let b = self.rand_range(min_w as i32, max_w as i32);
        ((a + b) / 2) as u16
    }

    fn respawn_fish(&mut self, idx: usize, initial: bool) {
        // Spread all 6 distinct fish cleanly across upper, mid, and nearshore lake lanes.
        let (y_min, y_max) = match idx % 6 {
            0 => (106, 122),
            1 => (134, 150),
            2 => (162, 182),
            3 => (194, 216),
            4 => (228, 254),
            _ => (266, 294),
        };
        let y = self.rand_range(y_min, y_max);
        let half_w = (isqrt((196 * 196 - (y - C) * (y - C)).max(0)) - 26).max(32);
        let left_to_right = if initial { (idx & 1) == 0 } else { (self.next_u32() & 1) == 0 };
        let x = if initial {
            match idx % 6 {
                0 => self.rand_range(C - half_w, C - 18),
                1 => self.rand_range(C + 18, C + half_w),
                2 => self.rand_range(C - half_w / 2, C + half_w / 2),
                3 => self.rand_range(C + 24, C + half_w),
                4 => self.rand_range(C - half_w, C - 10),
                _ => self.rand_range(C - half_w / 2, C + half_w),
            }
        } else if left_to_right {
            C - half_w
        } else {
            C + half_w
        };
        let sp = if initial {
            match idx % 6 {
                0 => Species::Trout,
                1 => Species::Perch,
                2 => Species::GoldenKoi,
                3 => Species::Roach,
                4 => Species::Carp,
                _ => Species::Pike,
            }
        } else {
            self.pick_species(y)
        };
        let (_, _, spd) = sp.silhouette();
        let dir = if left_to_right { 1 } else { -1 };
        let vy = self.rand_range(-3, 3);
        self.fish[idx] = LakeFish {
            species: sp,
            x_q8: x << 8,
            y_q8: y << 8,
            vx_q8: dir * (spd << 8),
            vy_q8: vy << 8,
            phase_ms: self.rand_range(0, 999) as u16,
            weight_hg: self.roll_weight(sp),
            ai: FishAi::Cruise,
            spook_ms: 0,
        };
    }

    fn clear_confirm(&mut self) {
        self.confirm_ms = 0;
        self.pending_nav = None;
    }

    fn queue_nav(&mut self, nav: PendingNav) {
        self.confirm_ms = CONFIRM_FLASH_MS;
        self.pending_nav = Some(nav);
    }

    pub fn open_difficulty(&mut self, mode: Mode) {
        self.clear_confirm();
        self.pending_mode = mode;
        self.diff_sel = self.difficulty.idx();
        self.state = State::Difficulty;
        self.has_cast = false;
    }

    pub fn start_with_difficulty(&mut self, mode: Mode, diff: Difficulty) {
        self.clear_confirm();
        self.difficulty = diff;
        self.diff_sel = diff.idx();
        self.start_mode(mode);
    }

    pub fn start_mode(&mut self, mode: Mode) {
        self.clear_confirm();
        self.mode = mode;
        self.state = State::Aim;
        self.score = 0;
        self.session_catches = 0;
        self.session_best_species = None;
        self.session_best_hg = 0;
        self.derby_left_ms = DERBY_MS;
        self.new_best_score = false;
        self.has_cast = false;
        self.bx = 186;
        self.by = 274;
        self.splash_ms = 0;
        for i in 0..MAX_FISH {
            self.respawn_fish(i, true);
        }
    }

    pub fn open_trophies(&mut self) {
        self.clear_confirm();
        self.state = State::Trophies;
        self.trophy_idx = 0;
    }

    pub fn back_to_menu(&mut self) {
        self.clear_confirm();
        self.state = State::Menu;
        self.has_cast = false;
    }

    pub fn animating(&self) -> bool {
        matches!(
            self.state,
            State::Aim
                | State::Casting
                | State::Floating
                | State::Nibble
                | State::Bite
                | State::Reeling
                | State::Banner
        )
    }

    pub fn needs_tick(&self) -> bool {
        self.animating()
            || self.confirm_ms > 0
            || (self.state == State::Caught && self.caught_guard_ms > 0)
    }

    /// Cast rod and bobber directly to tapped spot `(tx, ty)` anywhere on the water.
    pub fn cast_to(&mut self, tx: i32, ty: i32) -> Feedback {
        let (bx, by) = clamp_cast(tx, ty);
        self.bx = bx;
        self.by = by;
        self.has_cast = true;
        self.state = State::Casting;
        self.cast_ms = 0;
        self.splash_ms = 0;
        for f in &mut self.fish {
            f.ai = FishAi::Cruise;
        }
        Feedback::Tap
    }

    fn nearest_ready_fish(&self, bx: i32, by: i32) -> (usize, i32) {
        let mut best_i = 0usize;
        let mut best_d2 = i32::MAX;
        for (i, f) in self.fish.iter().enumerate() {
            if f.spook_ms > 0 {
                continue;
            }
            let dx = f.x() - bx;
            let dy = f.y() - by;
            let d2 = dx * dx + dy * dy;
            if d2 < best_d2 {
                best_d2 = d2;
                best_i = i;
            }
        }
        (best_i, best_d2)
    }

    pub fn begin_reeling(&mut self, fish_idx: usize) {
        let f = &self.fish[fish_idx];
        let sp = f.species;
        let w = f.weight_hg;
        let tuning = self.difficulty.tuning();
        let (bar_h, _, ai_min, ai_max) = sp.fight(self.difficulty);
        self.state = State::Reeling;
        self.reel_species = sp;
        self.reel_weight_hg = w;
        self.reel_progress = tuning.start_progress;
        self.bar_h = bar_h;
        let start_y = (TRACK_H - 12) / 2;
        self.bar_y = ((TRACK_H - bar_h) / 2).clamp(0, TRACK_H - bar_h);
        self.track_fish_y_q8 = start_y << 8;
        self.track_fish_vy_q8 = 0;
        if tuning.jump_max == 0 {
            self.track_target_y = self.rand_range(16, TRACK_H - 20);
            self.track_ai_ms = self.rand_range(ai_min, ai_max) as u32;
        } else {
            let up = (self.next_u32() & 1) == 0;
            let (lo_hi, hi_lo) = if self.difficulty == Difficulty::Medium {
                (66, TRACK_H - 68)
            } else {
                (54, TRACK_H - 56)
            };
            self.track_target_y = if up {
                self.rand_range(12, lo_hi)
            } else {
                self.rand_range(hi_lo, TRACK_H - 12)
            };
            let min_init = if self.difficulty == Difficulty::Medium { 260 } else { 200 };
            self.track_ai_ms = (ai_min as u32).max(min_init);
        }
        self.fish_in_bar = true;
        self.reel_phase_ms = 0;
        self.splash_ms = 0;
        self.respawn_fish(fish_idx, false);
    }

    pub fn try_hook(&mut self) -> Feedback {
        match self.state {
            State::Bite => {
                self.begin_reeling(self.target_fish);
                Feedback::CastSplash
            }
            State::Nibble => {
                self.state = State::Floating;
                self.wait_ms = 550;
                self.splash_ms = 260;
                Feedback::Tap
            }
            _ => Feedback::None,
        }
    }

    fn record_catch(&mut self, best: &mut Records) -> (Feedback, bool) {
        let sp = self.reel_species;
        let w = self.reel_weight_hg;
        let (min_w, max_w, base_pts) = sp.stats();
        let span = (max_w - min_w).max(1) as u32;
        let size_bonus = ((w - min_w) as u32 * base_pts / 2) / span;
        let pts = base_pts + size_bonus;

        self.score += pts;
        self.session_catches = self.session_catches.saturating_add(1);
        if w > self.session_best_hg {
            self.session_best_hg = w;
            self.session_best_species = Some(sp);
        }

        let mut broke_persist = false;
        best.total_caught = best.total_caught.saturating_add(1);
        let si = sp.idx();
        best.species_count[si] = best.species_count[si].saturating_add(1);
        let mut is_species_record = false;
        if w > best.species_best_hg[si] {
            best.species_best_hg[si] = w;
            is_species_record = true;
            broke_persist = true;
        }
        if w > best.biggest_hg {
            best.biggest_hg = w;
            best.biggest_species = Some(sp);
            broke_persist = true;
        }
        match self.mode {
            Mode::Relax => {
                if self.score > best.relax_best {
                    best.relax_best = self.score;
                    self.new_best_score = true;
                    broke_persist = true;
                }
            }
            Mode::Derby => {
                if self.score > best.derby_best {
                    best.derby_best = self.score;
                    self.new_best_score = true;
                    broke_persist = true;
                }
            }
        }

        self.caught_species = sp;
        self.caught_hg = w;
        self.caught_pts = pts;
        self.caught_record = is_species_record;
        self.caught_guard_ms = CATCH_GUARD_MS;
        self.state = State::Caught;
        (Feedback::Caught, broke_persist)
    }

    pub fn crown(&mut self, delta: i16) -> Feedback {
        if delta == 0 || self.confirm_ms > 0 {
            return Feedback::None;
        }
        let d = if delta > 0 { 1 } else { -1 };
        match self.state {
            State::Menu => {
                self.menu_sel = ((self.menu_sel as i32 + d).rem_euclid(3)) as usize;
                Feedback::Tap
            }
            State::Difficulty => {
                self.diff_sel = ((self.diff_sel as i32 + d).rem_euclid(3)) as usize;
                Feedback::Tap
            }
            State::Trophies => {
                self.trophy_idx = ((self.trophy_idx as i32 + d).rem_euclid(6)) as usize;
                Feedback::Tap
            }
            State::DerbyOver => {
                self.over_sel = ((self.over_sel as i32 + d).rem_euclid(2)) as usize;
                Feedback::Tap
            }
            State::Bite => self.try_hook(),
            State::Reeling => {
                let step_px = self.difficulty.tuning().step_px;
                let max_y = (TRACK_H - self.bar_h).max(0);
                let next_y = (self.bar_y + (delta as i32) * step_px).clamp(0, max_y);
                if next_y != self.bar_y {
                    self.bar_y = next_y;
                    self.update_fish_overlap();
                    return Feedback::BarTick;
                }
                Feedback::None
            }
            State::Caught if self.caught_guard_ms == 0 => {
                self.state = State::Aim;
                self.has_cast = false;
                Feedback::Tap
            }
            _ => Feedback::None,
        }
    }

    pub fn track_fish_y(&self) -> i32 {
        (self.track_fish_y_q8 >> 8).clamp(6, TRACK_H - 7)
    }

    fn update_fish_overlap(&mut self) -> bool {
        let fy = self.track_fish_y();
        let pad = self.difficulty.tuning().overlap_pad;
        let inside = fy >= self.bar_y - pad && fy <= self.bar_y + self.bar_h + pad;
        let entered = inside && !self.fish_in_bar;
        self.fish_in_bar = inside;
        entered
    }

    pub fn dismiss_caught(&mut self) -> Feedback {
        if self.state == State::Caught && self.caught_guard_ms == 0 {
            self.state = State::Aim;
            self.has_cast = false;
            Feedback::Tap
        } else {
            Feedback::None
        }
    }

    /// Strict button hit-testing across all UI screens: taps outside button bounds are ignored!
    pub fn tap_ui(&mut self, tx: i32, ty: i32) -> (Feedback, bool) {
        if self.confirm_ms > 0 {
            return (Feedback::None, false);
        }
        match self.state {
            State::Menu => {
                let opts = [
                    (MENU_RELAX_Y, PendingNav::OpenDifficulty(Mode::Relax)),
                    (MENU_DERBY_Y, PendingNav::OpenDifficulty(Mode::Derby)),
                    (MENU_TROPHY_Y, PendingNav::OpenTrophies),
                ];
                for (i, &(y, nav)) in opts.iter().enumerate() {
                    if in_rect(tx, ty, MENU_BTN_X, y, MENU_BTN_W, MENU_BTN_H) {
                        self.menu_sel = i;
                        self.queue_nav(nav);
                        return (Feedback::Tap, true);
                    }
                }
                (Feedback::None, false)
            }
            State::Difficulty => {
                if in_menu_back_pill(tx, ty) {
                    self.back_to_menu();
                    return (Feedback::Tap, true);
                }
                let ys = [MENU_RELAX_Y, MENU_DERBY_Y, MENU_TROPHY_Y];
                for (i, &y) in ys.iter().enumerate() {
                    if in_rect(tx, ty, MENU_BTN_X, y, MENU_BTN_W, MENU_BTN_H) {
                        self.diff_sel = i;
                        self.queue_nav(PendingNav::StartGame(DIFFICULTIES[i]));
                        return (Feedback::Tap, true);
                    }
                }
                (Feedback::None, false)
            }
            State::Trophies => {
                if in_menu_back_pill(tx, ty) {
                    self.back_to_menu();
                    (Feedback::Tap, true)
                } else if in_rect(tx, ty, TROPHY_PREV_X, TROPHY_NAV_Y, TROPHY_NAV_W, TROPHY_NAV_H) {
                    self.trophy_idx = (self.trophy_idx + 5) % 6;
                    (Feedback::Tap, false)
                } else if in_rect(tx, ty, TROPHY_NEXT_X, TROPHY_NAV_Y, TROPHY_NAV_W, TROPHY_NAV_H) {
                    self.trophy_idx = (self.trophy_idx + 1) % 6;
                    (Feedback::Tap, false)
                } else {
                    (Feedback::None, false)
                }
            }
            State::Caught => {
                if in_rect(tx, ty, CAUGHT_BTN_X - 4, CAUGHT_BTN_Y - 4, CAUGHT_BTN_W + 8, CAUGHT_BTN_H + 8) {
                    let fb = self.dismiss_caught();
                    (fb, fb != Feedback::None)
                } else {
                    (Feedback::None, false)
                }
            }
            State::DerbyOver => {
                let opts = [
                    (OVER_RETRY_Y, PendingNav::RetryDerby),
                    (OVER_MENU_Y, PendingNav::BackToMenu),
                ];
                for (i, &(y, nav)) in opts.iter().enumerate() {
                    if in_rect(tx, ty, OVER_BTN_X, y, OVER_BTN_W, OVER_BTN_H) {
                        self.over_sel = i;
                        self.queue_nav(nav);
                        return (Feedback::Tap, true);
                    }
                }
                (Feedback::None, false)
            }
            _ => (Feedback::None, false),
        }
    }

    fn update_fish_pool(&mut self, dt_ms: u32) {
        if self.state == State::Reeling {
            return;
        }
        let bob_active = matches!(self.state, State::Floating | State::Nibble | State::Bite);
        let bx = self.bx;
        let by = self.by;

        for i in 0..MAX_FISH {
            let mut f = self.fish[i];
            f.phase_ms = ((f.phase_ms as u32 + dt_ms) % 1000) as u16;
            if f.spook_ms > 0 {
                f.spook_ms = f.spook_ms.saturating_sub(dt_ms as u16);
            }

            if bob_active && i == self.target_fish && f.ai != FishAi::Cruise {
                let tx_q8 = bx << 8;
                let ty_q8 = by << 8;
                let dx = tx_q8 - f.x_q8;
                let dy = ty_q8 - f.y_q8;
                let dist_px = isqrt((dx >> 8) * (dx >> 8) + (dy >> 8) * (dy >> 8));
                if dist_px > 16 {
                    let spd = if f.ai == FishAi::Stalking { 46 } else { 36 };
                    f.vx_q8 = dx * spd / dist_px.max(1);
                    f.vy_q8 = dy * spd / dist_px.max(1);
                } else {
                    f.vx_q8 = if f.vx_q8 >= 0 { 6 << 8 } else { -6 << 8 };
                    f.vy_q8 = 0;
                }
            } else {
                let x = f.x();
                let y = f.y();
                let half_w = (isqrt((196 * 196 - (y - C) * (y - C)).max(0)) - 24).max(28);
                if x <= C - half_w && f.vx_q8 < 0 {
                    f.vx_q8 = -f.vx_q8;
                } else if x >= C + half_w && f.vx_q8 > 0 {
                    f.vx_q8 = -f.vx_q8;
                }
                if y <= 104 && f.vy_q8 < 0 {
                    f.vy_q8 = -f.vy_q8;
                } else if y >= SHORE_Y - 26 && f.vy_q8 > 0 {
                    f.vy_q8 = -f.vy_q8;
                }
            }

            f.x_q8 += (f.vx_q8 * dt_ms as i32) / 1000;
            f.y_q8 += (f.vy_q8 * dt_ms as i32) / 1000;
            self.fish[i] = f;
        }
    }

    pub fn tick(&mut self, raw_dt_ms: u32, best: &mut Records) -> (Feedback, bool) {
        let dt = raw_dt_ms.min(80);
        if dt == 0 {
            return (Feedback::None, false);
        }
        self.lake_ms = self.lake_ms.wrapping_add(dt);

        if self.state == State::Caught {
            self.caught_guard_ms = self.caught_guard_ms.saturating_sub(dt);
            return (Feedback::None, false);
        }

        if self.confirm_ms > 0 {
            self.confirm_ms = self.confirm_ms.saturating_sub(dt);
            if self.confirm_ms == 0 {
                if let Some(nav) = self.pending_nav.take() {
                    match nav {
                        PendingNav::OpenDifficulty(mode) => self.open_difficulty(mode),
                        PendingNav::OpenTrophies => self.open_trophies(),
                        PendingNav::StartGame(diff) => self.start_with_difficulty(self.pending_mode, diff),
                        PendingNav::RetryDerby => self.start_mode(Mode::Derby),
                        PendingNav::BackToMenu => self.back_to_menu(),
                    }
                }
            }
            return (Feedback::None, false);
        }

        if self.splash_ms > 0 {
            self.splash_ms = self.splash_ms.saturating_sub(dt);
        }

        self.update_fish_pool(dt);

        if self.mode == Mode::Derby
            && matches!(
                self.state,
                State::Aim
                    | State::Casting
                    | State::Floating
                    | State::Nibble
                    | State::Bite
                    | State::Reeling
                    | State::Banner
            )
        {
            if dt >= self.derby_left_ms {
                self.derby_left_ms = 0;
                self.state = State::DerbyOver;
                self.over_sel = 0;
                self.has_cast = false;
                return (Feedback::DerbyEnd, false);
            }
            self.derby_left_ms -= dt;
        }

        match self.state {
            State::Casting => {
                self.cast_ms += dt;
                if self.cast_ms >= CAST_FLIGHT_MS {
                    let (fi, d2) = self.nearest_ready_fish(self.bx, self.by);
                    self.target_fish = fi;
                    self.fish[fi].ai = FishAi::Curious;
                    let close_bonus = if d2 < 68 * 68 { 600 } else { 0 };
                    let base = self.rand_range(1200, 2250);
                    self.wait_ms = (base - close_bonus).max(550) as u32;
                    self.nibbles_left = 1;
                    self.splash_ms = 420;
                    self.state = State::Floating;
                    return (Feedback::CastSplash, false);
                }
            }
            State::Floating => {
                if dt >= self.wait_ms {
                    self.fish[self.target_fish].ai = FishAi::Stalking;
                    self.will_bite = true;
                    self.nibbles_left = 0;
                    self.wait_ms = self.rand_range(400, 620) as u32;
                    self.splash_ms = 340;
                    self.state = State::Nibble;
                    return (Feedback::Nibble, false);
                }
                self.wait_ms -= dt;
            }
            State::Nibble => {
                if dt >= self.wait_ms {
                    self.state = State::Bite;
                    self.bite_left_ms = HOOK_WINDOW_MS;
                    self.splash_ms = 520;
                    return (Feedback::Bite, false);
                }
                self.wait_ms -= dt;
            }
            State::Bite => {
                if dt >= self.bite_left_ms {
                    self.fish[self.target_fish].ai = FishAi::Cruise;
                    self.fish[self.target_fish].spook_ms = 2000;
                    self.banner = BannerKind::Escaped;
                    self.banner_ms = RESULT_BANNER_MS;
                    self.state = State::Banner;
                    return (Feedback::Escape, false);
                }
                self.bite_left_ms -= dt;
            }
            State::Reeling => {
                self.reel_phase_ms = self.reel_phase_ms.wrapping_add(dt);
                let tuning = self.difficulty.tuning();
                let (_, max_spd, ai_min, ai_max) = self.reel_species.fight(self.difficulty);

                if dt >= self.track_ai_ms {
                    self.track_ai_ms = self.rand_range(ai_min, ai_max) as u32;
                    if tuning.jump_max == 0 {
                        self.track_target_y = self.rand_range(10, TRACK_H - 12);
                    } else {
                        let cur = (self.track_fish_y_q8 >> 8).clamp(8, TRACK_H - 10);
                        let jump = self.rand_range(tuning.jump_min, tuning.jump_max);
                        let up = if cur > TRACK_H * 2 / 3 {
                            true
                        } else if cur < TRACK_H / 3 {
                            false
                        } else {
                            (self.next_u32() & 1) == 0
                        };
                        self.track_target_y = if up {
                            (cur - jump).max(10)
                        } else {
                            (cur + jump).min(TRACK_H - 12)
                        };
                    }
                } else {
                    self.track_ai_ms -= dt;
                }

                let diff = self.track_target_y - (self.track_fish_y_q8 >> 8);
                let desired_v = if diff.abs() <= 4 {
                    0
                } else {
                    (diff.signum() * max_spd).clamp(-max_spd, max_spd)
                };
                let accel = max_spd * tuning.accel_mul * dt as i32 / 1000;
                let cur_v = self.track_fish_vy_q8 >> 8;
                let next_v = if cur_v < desired_v {
                    (cur_v + accel.max(tuning.min_accel_dv)).min(desired_v)
                } else if cur_v > desired_v {
                    (cur_v - accel.max(tuning.min_accel_dv)).max(desired_v)
                } else {
                    cur_v
                };
                self.track_fish_vy_q8 = next_v << 8;
                self.track_fish_y_q8 = (self.track_fish_y_q8 + (self.track_fish_vy_q8 * dt as i32) / 1000)
                    .clamp(8 << 8, (TRACK_H - 9) << 8);

                let entered = self.update_fish_overlap();
                if self.fish_in_bar {
                    self.reel_progress =
                        (self.reel_progress + (tuning.gain_per_s * dt as i32) / 1000).min(1000);
                    if self.reel_progress >= 1000 {
                        return self.record_catch(best);
                    }
                    if entered {
                        return (Feedback::CatchHold, false);
                    }
                } else {
                    self.reel_progress =
                        (self.reel_progress - (tuning.decay_per_s * dt as i32) / 1000).max(0);
                    if self.reel_progress <= 0 {
                        self.banner = BannerKind::Escaped;
                        self.banner_ms = RESULT_BANNER_MS;
                        self.state = State::Banner;
                        return (Feedback::Escape, false);
                    }
                }
                return (Feedback::None, false);
            }
            State::Banner => {
                if dt >= self.banner_ms {
                    self.state = State::Aim;
                    self.has_cast = false;
                } else {
                    self.banner_ms -= dt;
                }
            }
            _ => {}
        }
        (Feedback::None, false)
    }

    pub fn bobber_pos(&self) -> (i32, i32) {
        match self.state {
            State::Casting => {
                let pose = rod_pose(self.bx, self.by, 12);
                let t = (self.cast_ms.min(CAST_FLIGHT_MS) as i32) * 256 / CAST_FLIGHT_MS as i32;
                let x = pose.tip_x + (self.bx - pose.tip_x) * t / 256;
                let arc = (t * (256 - t)) >> 10;
                let y = pose.tip_y + (self.by - pose.tip_y) * t / 256 - arc * 2;
                (x, y)
            }
            _ => (self.bx, self.by),
        }
    }
}

/// Format hectograms (e.g. `145` -> `"1.45 KG"`) without heap or floats.
pub fn format_weight(hg: u16, buf: &mut [u8; 16]) -> &str {
    let whole = hg / 100;
    let frac = hg % 100;
    let mut i = 0usize;
    if whole >= 10 {
        buf[i] = b'0' + (whole / 10) as u8;
        i += 1;
    }
    buf[i] = b'0' + (whole % 10) as u8;
    i += 1;
    buf[i] = b'.';
    i += 1;
    buf[i] = b'0' + (frac / 10) as u8;
    i += 1;
    buf[i] = b'0' + (frac % 10) as u8;
    i += 1;
    buf[i..i + 3].copy_from_slice(b" KG");
    i += 3;
    core::str::from_utf8(&buf[..i]).unwrap_or("0.00 KG")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cast_stays_inside_water_and_right_of_gauge() {
        for &(tx, ty) in &[(10, 20), (450, 400), (240, 180), (40, 220)] {
            let (bx, by) = clamp_cast(tx, ty);
            assert!(by >= CAST_Y_MIN && by <= CAST_Y_MAX);
            assert!(bx >= CAST_X_MIN && bx <= CAST_X_MAX);
            assert!((bx - C) * (bx - C) + (by - C) * (by - C) <= CAST_R_MAX * CAST_R_MAX);
        }
    }

    #[test]
    fn rod_starts_on_sand_and_leans_toward_cast() {
        let left = rod_pose(120, 180, 0);
        let right = rod_pose(360, 180, 0);
        assert!(left.grip_y > SHORE_Y && right.grip_y > SHORE_Y);
        assert!(left.tip_y < SHORE_Y && right.tip_y < SHORE_Y);
        assert!(left.tip_x < right.tip_x);
    }

    #[test]
    fn vertical_bar_follows_crown_and_catches_when_tracked() {
        let mut g = Game::new(42);
        let mut rec = Records::default();
        g.start_mode(Mode::Relax);
        g.cast_to(220, 180);
        while g.state != State::Bite {
            g.tick(40, &mut rec);
        }
        assert_eq!(g.try_hook(), Feedback::CastSplash);
        assert_eq!(g.state, State::Reeling);

        let initial_y = g.bar_y;
        g.crown(-1);
        assert!(g.bar_y < initial_y);
        g.crown(2);
        assert!(g.bar_y > initial_y);

        let mut steps = 0;
        while g.state == State::Reeling && steps < 500 {
            let fy = g.track_fish_y();
            let center = g.bar_y + g.bar_h / 2;
            if fy < center - 5 {
                g.crown(-1);
            } else if fy > center + 5 {
                g.crown(1);
            }
            g.tick(40, &mut rec);
            steps += 1;
        }
        assert_eq!(g.state, State::Caught);
        assert_eq!(rec.total_caught, 1);
    }

    #[test]
    fn idle_bar_loses_fish_when_fish_swims_away() {
        let mut g = Game::new(99);
        let mut rec = Records::default();
        g.start_mode(Mode::Relax);
        g.cast_to(220, 180);
        while g.state != State::Bite {
            g.tick(40, &mut rec);
        }
        g.try_hook();
        g.track_target_y = TRACK_H - 12;
        g.track_fish_y_q8 = (TRACK_H - 12) << 8;
        g.track_ai_ms = 5_000;
        for _ in 0..100 {
            if g.state != State::Reeling {
                break;
            }
            g.crown(-1);
            g.tick(40, &mut rec);
        }
        assert_eq!(g.state, State::Banner);
        assert_eq!(rec.total_caught, 0);
    }

    #[test]
    fn ui_buttons_ignore_taps_outside_button_bounds() {
        let mut g = Game::new(5);
        let mut rec = Records::default();
        g.menu_sel = 0;

        // Miss all Menu buttons -> stays on Menu, no feedback.
        assert_eq!(g.tap_ui(233, 50), (Feedback::None, false));
        assert_eq!(g.tap_ui(60, 233), (Feedback::None, false));
        assert_eq!(g.state, State::Menu);

        // Tap DERBY button while RELAX is focused -> DERBY lights up first, then opens Difficulty.
        assert_eq!(g.tap_ui(233, MENU_DERBY_Y + 10), (Feedback::Tap, true));
        assert_eq!(g.state, State::Menu);
        assert_eq!(g.menu_sel, 1);
        assert_eq!(g.confirm_ms, CONFIRM_FLASH_MS);
        assert!(g.needs_tick());
        assert_eq!(g.crown(-1), Feedback::None);
        assert_eq!(g.tap_ui(233, MENU_RELAX_Y + 10), (Feedback::None, false));
        g.tick(80, &mut rec);
        assert_eq!(g.state, State::Menu);
        g.tick(80, &mut rec);
        assert_eq!(g.state, State::Difficulty);
        assert_eq!(g.pending_mode, Mode::Derby);
        assert_eq!(g.confirm_ms, 0);

        // Crown focuses HARD, tapping off-button sand area MUST NOT launch game.
        g.crown(1);
        assert_eq!(g.diff_sel, Difficulty::Hard.idx());
        assert_eq!(g.tap_ui(233, 390), (Feedback::None, false));
        assert_eq!(g.state, State::Difficulty);

        // Reset focus back to MEDIUM, tap EASY -> EASY lights up visibly first, then starts EASY.
        g.open_difficulty(Mode::Relax);
        assert_eq!(g.diff_sel, Difficulty::Medium.idx());
        assert_eq!(g.tap_ui(233, MENU_RELAX_Y + 10), (Feedback::Tap, true));
        assert_eq!(g.state, State::Difficulty);
        assert_eq!(g.diff_sel, Difficulty::Easy.idx());
        assert_eq!(g.confirm_ms, CONFIRM_FLASH_MS);
        assert_eq!(g.crown(1), Feedback::None);
        assert_eq!(g.diff_sel, Difficulty::Easy.idx());
        g.tick(80, &mut rec);
        assert_eq!(g.state, State::Difficulty);
        g.tick(80, &mut rec);
        assert_eq!(g.state, State::Aim);
        assert_eq!(g.difficulty, Difficulty::Easy);

        // `< BACK` on Difficulty stays immediate.
        g.open_difficulty(Mode::Relax);
        assert_eq!(g.tap_ui(HUD_MENU_X + 10, HUD_Y + 10), (Feedback::Tap, true));
        assert_eq!(g.state, State::Menu);

        // Trophies: tapping blank center/edges does not turn pages.
        g.open_trophies();
        assert_eq!(g.tap_ui(233, 233), (Feedback::None, false));
        assert_eq!(g.trophy_idx, 0);
        assert_eq!(g.tap_ui(TROPHY_NEXT_X + 10, TROPHY_NAV_Y + 10), (Feedback::Tap, false));
        assert_eq!(g.trophy_idx, 1);

        // Caught: must tap CONTINUE button after guard window.
        g.state = State::Caught;
        g.caught_guard_ms = 0;
        assert_eq!(g.tap_ui(233, 150), (Feedback::None, false));
        assert_eq!(g.state, State::Caught);
        assert_eq!(g.tap_ui(CAUGHT_BTN_X + 20, CAUGHT_BTN_Y + 10), (Feedback::Tap, true));
        assert_eq!(g.state, State::Aim);

        // DerbyOver: stray tap ignored; tapping MENU highlights MENU first, then switches to Menu.
        g.state = State::DerbyOver;
        g.over_sel = 0;
        assert_eq!(g.tap_ui(233, 120), (Feedback::None, false));
        assert_eq!(g.state, State::DerbyOver);
        assert_eq!(g.tap_ui(OVER_BTN_X + 20, OVER_MENU_Y + 10), (Feedback::Tap, true));
        assert_eq!(g.state, State::DerbyOver);
        assert_eq!(g.over_sel, 1);
        g.tick(80, &mut rec);
        assert_eq!(g.state, State::DerbyOver);
        g.tick(80, &mut rec);
        assert_eq!(g.state, State::Menu);
    }

    #[test]
    fn difficulty_presets_scale_bar_speed_and_select_flow() {
        let mut g = Game::new(11);
        g.open_difficulty(Mode::Derby);
        assert_eq!(g.state, State::Difficulty);
        assert_eq!(g.pending_mode, Mode::Derby);
        assert_eq!(g.diff_sel, Difficulty::Medium.idx());

        g.crown(1);
        assert_eq!(g.diff_sel, Difficulty::Hard.idx());
        g.crown(1);
        assert_eq!(g.diff_sel, Difficulty::Easy.idx());

        g.start_with_difficulty(Mode::Derby, DIFFICULTIES[g.diff_sel]);
        assert_eq!(g.state, State::Aim);
        assert_eq!(g.mode, Mode::Derby);
        assert_eq!(g.difficulty, Difficulty::Easy);

        for sp in SPECIES_ALL {
            let (eb, es, ..) = sp.fight(Difficulty::Easy);
            let (mb, ms, ..) = sp.fight(Difficulty::Medium);
            let (hb, hs, ..) = sp.fight(Difficulty::Hard);
            assert!(eb > mb && mb > hb, "{:?}: bar_h should shrink Easy>Medium>Hard", sp);
            assert!(es < ms && ms < hs, "{:?}: speed should rise Easy<Medium<Hard", sp);
        }

        let te = Difficulty::Easy.tuning();
        let tm = Difficulty::Medium.tuning();
        let th = Difficulty::Hard.tuning();
        assert!(te.start_progress > tm.start_progress && tm.start_progress > th.start_progress);
        assert!(te.gain_per_s > tm.gain_per_s && tm.gain_per_s > th.gain_per_s);
        assert!(te.decay_per_s < tm.decay_per_s && tm.decay_per_s < th.decay_per_s);
    }

    #[test]
    fn records_roundtrip_and_weight_format() {
        let mut r = Records::default();
        r.relax_best = 540;
        r.derby_best = 320;
        r.total_caught = 9;
        r.biggest_species = Some(Species::Pike);
        r.biggest_hg = 875;
        r.species_count[Species::Pike.idx()] = 2;
        r.species_best_hg[Species::Pike.idx()] = 875;

        assert_eq!(decode(&encode(r)), r);
        let mut b = [0u8; 16];
        assert_eq!(format_weight(875, &mut b), "8.75 KG");
    }
}
