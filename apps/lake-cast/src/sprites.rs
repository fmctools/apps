//! Compact crisp multi-rect pixel art sprites for the 6 Lake Cast fish species.

use cmf_wp2_ui::{Rgb, Surface};

pub const fn c565(r: u8, g: u8, b: u8) -> u16 {
    (((r as u16) >> 3) << 11) | (((g as u16) >> 2) << 5) | ((b as u16) >> 3)
}

pub struct Sprite {
    pub w: i32,
    pub h: i32,
    pub rects: &'static [(i16, i16, i16, i16, u16)],
}

/// Draw a fish sprite scaled by integer factor (1..=3). If `tint` is `Some`, draws a crisp silhouette.
pub fn draw_fish(s: &mut impl Surface, sp: &Sprite, x: i32, y: i32, scale: i32, tint: Option<Rgb>) {
    let sc = scale.max(1);
    let fixed = tint.map(Rgb::rgb565);
    for &(rx, ry, rw, rh, c) in sp.rects {
        s.rect(
            x + (rx as i32) * sc,
            y + (ry as i32) * sc,
            (rw as i32) * sc,
            (rh as i32) * sc,
            fixed.unwrap_or(c),
        );
    }
}

// 1. SILVER ROACH (26x14): sleek silver-blue body with signature coral-red tail & fins.
const R_OUT: u16 = c565(22, 44, 68);
const R_BACK: u16 = c565(88, 152, 206);
const R_MID: u16 = c565(198, 224, 242);
const R_BELLY: u16 = c565(238, 246, 255);
const R_FIN: u16 = c565(238, 86, 62);
const EYE_W: u16 = c565(255, 255, 255);
const EYE_K: u16 = c565(18, 26, 36);

pub const SPRITE_ROACH: Sprite = Sprite {
    w: 26,
    h: 14,
    rects: &[
        // Dorsal fin
        (10, 0, 5, 2, R_FIN),
        // Upper outline & back
        (5, 2, 11, 1, R_OUT),
        (3, 3, 15, 2, R_BACK),
        (1, 5, 19, 3, R_MID),
        (2, 8, 17, 2, R_BELLY),
        (5, 10, 11, 1, R_OUT),
        // Ventral fin
        (11, 11, 4, 2, R_FIN),
        // Tail
        (20, 4, 3, 6, R_FIN),
        (23, 2, 3, 4, R_FIN),
        (23, 8, 3, 4, R_FIN),
        // Eye & gill
        (4, 5, 2, 2, EYE_W),
        (4, 5, 1, 1, EYE_K),
        (8, 5, 1, 4, R_OUT),
    ],
};

// 2. STRIPED PERCH (28x15): olive-gold body, bold dark vertical tiger stripes, orange fins.
const P_OUT: u16 = c565(26, 42, 28);
const P_BACK: u16 = c565(78, 134, 64);
const P_GOLD: u16 = c565(188, 198, 72);
const P_BELLY: u16 = c565(236, 228, 146);
const P_STRIPE: u16 = c565(34, 58, 40);
const P_FIN: u16 = c565(242, 112, 36);

pub const SPRITE_PERCH: Sprite = Sprite {
    w: 28,
    h: 15,
    rects: &[
        // Spiky dorsal fin
        (9, 0, 8, 2, P_BACK),
        (10, 1, 6, 1, P_FIN),
        // Body
        (5, 2, 13, 1, P_OUT),
        (3, 3, 17, 3, P_BACK),
        (1, 6, 20, 3, P_GOLD),
        (2, 9, 17, 2, P_BELLY),
        (5, 11, 12, 1, P_OUT),
        // Tiger stripes
        (9, 3, 2, 6, P_STRIPE),
        (13, 3, 2, 6, P_STRIPE),
        (17, 4, 2, 5, P_STRIPE),
        // Orange lower fins & tail
        (11, 12, 5, 2, P_FIN),
        (21, 5, 3, 5, P_FIN),
        (24, 3, 3, 4, P_FIN),
        (24, 8, 3, 4, P_FIN),
        // Eye & gill
        (4, 5, 2, 2, EYE_W),
        (4, 6, 1, 1, EYE_K),
        (7, 5, 1, 4, P_OUT),
    ],
};

// 3. RAINBOW TROUT (30x14): slender turquoise-silver predator with pink rainbow stripe & speckles.
const T_BACK: u16 = c565(46, 116, 126);
const T_SILVER: u16 = c565(186, 212, 224);
const T_PINK: u16 = c565(236, 114, 152);
const T_BELLY: u16 = c565(240, 244, 248);
const T_SPOT: u16 = c565(28, 48, 60);
const T_FIN: u16 = c565(132, 166, 186);

pub const SPRITE_TROUT: Sprite = Sprite {
    w: 30,
    h: 14,
    rects: &[
        (12, 0, 5, 2, T_FIN),
        (5, 2, 16, 3, T_BACK),
        (2, 5, 21, 2, T_SILVER),
        (3, 7, 19, 2, T_PINK),
        (4, 9, 17, 2, T_BELLY),
        // Speckles
        (9, 3, 1, 1, T_SPOT),
        (13, 4, 1, 1, T_SPOT),
        (16, 3, 1, 1, T_SPOT),
        (19, 4, 1, 1, T_SPOT),
        // Lower fin & forked tail
        (13, 11, 4, 2, T_PINK),
        (23, 5, 3, 4, T_BACK),
        (26, 3, 4, 3, T_FIN),
        (26, 8, 4, 3, T_FIN),
        // Eye & mouth
        (4, 5, 2, 2, EYE_W),
        (4, 5, 1, 1, EYE_K),
        (1, 7, 3, 1, T_SPOT),
    ],
};

// 4. AMBER CARP (29x17): broad golden-bronze carp with deep scales & warm belly.
const C_DARK: u16 = c565(106, 56, 20);
const C_BRONZE: u16 = c565(204, 126, 44);
const C_GOLD: u16 = c565(242, 182, 68);
const C_CREAM: u16 = c565(250, 224, 138);
const C_FIN: u16 = c565(218, 92, 34);

pub const SPRITE_CARP: Sprite = Sprite {
    w: 29,
    h: 17,
    rects: &[
        // Long carp dorsal crest
        (9, 0, 10, 2, C_FIN),
        (6, 2, 13, 1, C_DARK),
        (3, 3, 17, 3, C_BRONZE),
        (1, 6, 20, 4, C_GOLD),
        (2, 10, 18, 3, C_CREAM),
        (5, 13, 12, 1, C_DARK),
        // Scale glints
        (10, 5, 2, 2, C_GOLD),
        (14, 7, 2, 2, C_CREAM),
        (11, 8, 2, 2, C_BRONZE),
        // Belly fins & fan tail
        (12, 14, 5, 2, C_FIN),
        (21, 6, 4, 5, C_FIN),
        (25, 4, 4, 4, C_FIN),
        (25, 9, 4, 4, C_FIN),
        // Eye & whisker
        (4, 6, 2, 2, EYE_W),
        (4, 7, 1, 1, EYE_K),
        (0, 9, 2, 2, C_GOLD),
    ],
};

// 5. NORTHERN PIKE (32x13): long sleek ambush predator with emerald hide & pale yellow spots.
const K_DARK: u16 = c565(24, 66, 44);
const K_GREEN: u16 = c565(56, 134, 74);
const K_SPOT: u16 = c565(218, 232, 118);
const K_BELLY: u16 = c565(222, 234, 198);
const K_FIN: u16 = c565(204, 98, 42);

pub const SPRITE_PIKE: Sprite = Sprite {
    w: 32,
    h: 13,
    rects: &[
        // Rear dorsal fin (signature pike silhouette!)
        (19, 0, 5, 2, K_FIN),
        // Long snout & body
        (1, 4, 24, 2, K_DARK),
        (0, 6, 26, 3, K_GREEN),
        (3, 9, 21, 2, K_BELLY),
        // Predator spots
        (10, 5, 2, 1, K_SPOT),
        (14, 7, 2, 1, K_SPOT),
        (18, 5, 2, 1, K_SPOT),
        (22, 7, 2, 1, K_SPOT),
        // Tail & lower fins
        (20, 11, 4, 2, K_FIN),
        (26, 5, 3, 4, K_DARK),
        (29, 3, 3, 3, K_FIN),
        (29, 8, 3, 3, K_FIN),
        // Fierce eye & jaw
        (5, 4, 2, 2, K_SPOT),
        (5, 5, 1, 1, EYE_K),
        (0, 7, 5, 1, K_DARK),
    ],
};

// 6. GOLDEN KOI (31x16): rare radiant orange-gold & pearl-white ornamental trophy fish.
const G_CRIMSON: u16 = c565(234, 58, 36);
const G_GOLD: u16 = c565(255, 196, 46);
const G_PEARL: u16 = c565(252, 248, 238);
const G_OUT: u16 = c565(146, 54, 16);

pub const SPRITE_KOI: Sprite = Sprite {
    w: 31,
    h: 16,
    rects: &[
        // Flowing crest & body
        (10, 0, 9, 2, G_GOLD),
        (5, 2, 15, 1, G_OUT),
        (3, 3, 18, 3, G_GOLD),
        (1, 6, 21, 4, G_PEARL),
        (3, 10, 18, 2, G_GOLD),
        (6, 12, 12, 1, G_OUT),
        // Koi crimson patches
        (3, 4, 5, 4, G_CRIMSON),
        (12, 3, 6, 5, G_CRIMSON),
        (17, 7, 4, 3, G_CRIMSON),
        // Flowing veil fins & tail
        (10, 13, 6, 3, G_GOLD),
        (22, 5, 4, 6, G_CRIMSON),
        (26, 2, 5, 5, G_GOLD),
        (26, 8, 5, 5, G_PEARL),
        // Eye
        (4, 6, 2, 2, EYE_W),
        (4, 6, 1, 1, EYE_K),
    ],
};
