//! The full-screen watch face: artwork, large bordered transport controls and volume.
//!
//! The current track title, artist, and a low-resolution copy of the matching notification artwork
//! can be displayed on the watch. These values stay in RAM; albums and notification text stay on
//! the phone.

use crate::model::{Model, Status};
use crate::ntype82::{self, FontSize};
use cmf_wp2_ui::color::Rgb;
use cmf_wp2_ui::{Surface, C, SIZE};

const BLACK: u16 = Rgb(0, 0, 0).rgb565();
const WHITE: u16 = Rgb(255, 255, 255).rgb565();
const GREY: u16 = Rgb(140, 140, 140).rgb565();
const DIM: u16 = Rgb(82, 82, 82).rgb565();
const ACCENT: u16 = Rgb(250, 35, 0).rgb565();
const RED: u16 = Rgb(210, 20, 20).rgb565();
const PANEL: u16 = Rgb(18, 18, 18).rgb565();

const ART_X: i32 = 169;
const ART_Y: i32 = 16;
const ART_SIZE: i32 = 128;
const ART_SOURCE_SIZE: i32 = 64;

const TITLE_Y: i32 = 149;
const ARTIST_Y: i32 = 175;
const PERMISSION_TITLE_Y: i32 = 155;
const FOOTER_Y: i32 = 414;

const PREV_X: i32 = 95;
const PLAY_X: i32 = 233;
const NEXT_X: i32 = 371;
const CONTROL_Y: i32 = 270;
const PREV_R: i32 = 44;
const PLAY_R: i32 = 64;
const NEXT_R: i32 = 44;

const VOLUME_X: i32 = 112;
const VOLUME_Y: i32 = 346;
const VOLUME_W: i32 = 242;
const VOLUME_H: i32 = 32;
const MINUS_X: i32 = 54;
const PLUS_X: i32 = 364;
const BUTTON_Y: i32 = 338;
const BUTTON_SIZE: i32 = 48;

/// Artwork supplied by the protocol layer. All coordinates are source pixels in the 64×64 image.
pub trait ArtworkSource {
    /// Whether a complete image may currently be displayed.
    fn visible(&self) -> bool;

    /// The committed image generation, or zero when there is no image.
    fn generation(&self) -> u64;

    /// The RGB565 colour at one source pixel.
    fn pixel(&self, x: u8, y: u8) -> Option<u16>;
}

impl ArtworkSource for Model {
    fn visible(&self) -> bool {
        self.artwork_visible()
    }

    fn generation(&self) -> u64 {
        self.artwork_generation()
    }

    fn pixel(&self, x: u8, y: u8) -> Option<u16> {
        self.artwork_pixel(x, y)
    }
}

/// What a tap in the transport row should do.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum Action {
    Previous,
    Toggle,
    Next,
    VolumeUp,
    VolumeDown,
    SetVolume(u8),
    None,
}

/// Maps a touch to a playback or volume action.
pub fn action(
    x: i32,
    y: i32,
    max_volume: u8,
    controls_enabled: bool,
    volume_enabled: bool,
) -> Action {
    if !controls_enabled {
        return Action::None;
    }

    if hit_circle(x, y, PLAY_X, CONTROL_Y, PLAY_R) {
        return Action::Toggle;
    }
    if hit_circle(x, y, PREV_X, CONTROL_Y, PREV_R) {
        return Action::Previous;
    }
    if hit_circle(x, y, NEXT_X, CONTROL_Y, NEXT_R) {
        return Action::Next;
    }

    if volume_enabled {
        if in_rect(x, y, MINUS_X, BUTTON_Y, BUTTON_SIZE, BUTTON_SIZE) {
            return Action::VolumeDown;
        }
        if in_rect(x, y, PLUS_X, BUTTON_Y, BUTTON_SIZE, BUTTON_SIZE) {
            return Action::VolumeUp;
        }
        if in_rect(x, y, VOLUME_X, VOLUME_Y, VOLUME_W, VOLUME_H) {
            let offset = (x - VOLUME_X).clamp(0, VOLUME_W - 1);
            let value = (offset * max_volume as i32 + (VOLUME_W - 1) / 2) / (VOLUME_W - 1);
            return Action::SetVolume(value.clamp(0, max_volume as i32) as u8);
        }
    }

    Action::None
}

pub fn frame(s: &mut impl Surface, m: &Model) {
    frame_with_artwork(s, m, m);
}

fn frame_with_artwork(s: &mut impl Surface, m: &Model, artwork: &impl ArtworkSource) {
    s.rect(0, 0, SIZE, SIZE, BLACK);

    let active = m.active();
    draw_artwork(s, artwork, active);

    if !m.has_snapshot && matches!(m.status, Status::Ask | Status::Denied) {
        draw_permission(s, m.status);
        draw_footer(s, m);
        return;
    }

    draw_track_text(s, m);

    let controls_enabled = m.controls_enabled();
    draw_transport(s, m.playing(), controls_enabled);
    draw_volume(s, m.volume(), m.max_volume(), m.volume_enabled());
    draw_footer(s, m);
}

/// Draws the committed artwork, or a deliberately generic placeholder.
fn draw_artwork(s: &mut impl Surface, artwork: &impl ArtworkSource, active: bool) {
    let border = if active { ACCENT } else { DIM };
    let has_artwork = artwork.visible() && artwork.generation() != 0;

    if has_artwork {
        for source_y in 0..ART_SOURCE_SIZE {
            let mut source_x = 0;
            while source_x < ART_SOURCE_SIZE {
                let Some(colour) = artwork.pixel(source_x as u8, source_y as u8) else {
                    source_x += 1;
                    continue;
                };
                let start = source_x;
                while source_x < ART_SOURCE_SIZE
                    && artwork.pixel(source_x as u8, source_y as u8) == Some(colour)
                {
                    source_x += 1;
                }
                s.rect(
                    ART_X + start * 2,
                    ART_Y + source_y * 2,
                    (source_x - start) * 2,
                    2,
                    colour,
                );
            }
        }
    } else {
        draw_artwork_placeholder(s, active);
    }

    s.rect(ART_X - 2, ART_Y - 2, ART_SIZE + 4, 2, border);
    s.rect(ART_X - 2, ART_Y + ART_SIZE, ART_SIZE + 4, 2, border);
    s.rect(ART_X - 2, ART_Y, 2, ART_SIZE, border);
    s.rect(ART_X + ART_SIZE, ART_Y, 2, ART_SIZE, border);
}

fn draw_artwork_placeholder(s: &mut impl Surface, active: bool) {
    s.rect(ART_X, ART_Y, ART_SIZE, ART_SIZE, PANEL);
    let colour = if active { ACCENT } else { DIM };
    let fill = if active {
        Rgb(250, 35, 0)
    } else {
        Rgb(82, 82, 82)
    };

    // Two large notes centred in the cover well.
    s.rect(ART_X + 40, ART_Y + 28, 6, 58, colour);
    s.rect(ART_X + 96, ART_Y + 18, 6, 68, colour);
    s.rect(ART_X + 46, ART_Y + 28, 50, 6, colour);
    s.rect(ART_X + 102, ART_Y + 18, 20, 6, colour);
    cmf_wp2_ui::draw::disc(s, ART_X + 37, ART_Y + 86, 13, fill, Rgb(18, 18, 18));
    cmf_wp2_ui::draw::disc(s, ART_X + 93, ART_Y + 86, 13, fill, Rgb(18, 18, 18));
}

fn draw_track_text(s: &mut impl Surface, m: &Model) {
    if !m.active() {
        ntype82::centre(s, C, TITLE_Y + 7, "NO MEDIA", GREY, FontSize::Large);
        return;
    }

    let title = m.track_title();
    let title = if title.is_empty() {
        "UNKNOWN TRACK"
    } else {
        title
    };
    ntype82::centre_clipped(s, C, TITLE_Y, title, 244, WHITE, FontSize::Small);

    let artist = m.track_artist();
    if !artist.is_empty() {
        ntype82::centre_clipped(s, C, ARTIST_Y, artist, 244, GREY, FontSize::Small);
    }
}

fn draw_transport(s: &mut impl Surface, playing: bool, enabled: bool) {
    let secondary = if enabled { WHITE } else { DIM };
    let accent = if enabled { ACCENT } else { DIM };

    disc_button(s, PREV_X, CONTROL_Y, PREV_R, secondary);
    disc_button(s, PLAY_X, CONTROL_Y, PLAY_R, accent);
    disc_button(s, NEXT_X, CONTROL_Y, NEXT_R, secondary);

    // Previous: a vertical bar at the left and a triangle whose point faces left.
    s.rect(PREV_X - 17, CONTROL_Y - 15, 7, 30, secondary);
    triangle_left(s, PREV_X + 14, CONTROL_Y, 20, 20, secondary);

    // Next: a triangle whose point faces right and a vertical bar at the right.
    triangle_right(s, NEXT_X - 14, CONTROL_Y, 20, 20, secondary);
    s.rect(NEXT_X + 10, CONTROL_Y - 15, 7, 30, secondary);

    if playing {
        s.rect(PLAY_X - 17, CONTROL_Y - 18, 9, 36, accent);
        s.rect(PLAY_X + 8, CONTROL_Y - 18, 9, 36, accent);
    } else {
        triangle_right(s, PLAY_X - 22, CONTROL_Y, 23, 24, accent);
    }
}

fn disc_button(s: &mut impl Surface, cx: i32, cy: i32, radius: i32, colour: u16) {
    cmf_wp2_ui::draw::disc(s, cx, cy, radius, Rgb::from565(colour), Rgb(0, 0, 0));
    cmf_wp2_ui::draw::disc(s, cx, cy, radius - 5, Rgb(0, 0, 0), Rgb(0, 0, 0));
}

/// Draws a solid left-pointing triangle with its point at `x - half_width`.
fn triangle_left(
    s: &mut impl Surface,
    x: i32,
    y: i32,
    half_width: i32,
    half_height: i32,
    colour: u16,
) {
    for row in 0..=half_height {
        let width = half_width - row;
        if width > 0 {
            s.rect(x - width, y - row, width, 1, colour);
            s.rect(x - width, y + row, width, 1, colour);
        }
    }
}

/// Draws a solid right-pointing triangle with its point at `x + half_width - 1`.
fn triangle_right(
    s: &mut impl Surface,
    x: i32,
    y: i32,
    half_width: i32,
    half_height: i32,
    colour: u16,
) {
    for row in 0..=half_height {
        let width = half_width - row;
        if width > 0 {
            s.rect(x, y - row, width, 1, colour);
            s.rect(x, y + row, width, 1, colour);
        }
    }
}

fn draw_volume(s: &mut impl Surface, value: u8, max: u8, enabled: bool) {
    let border = if enabled { WHITE } else { DIM };
    rounded_button(s, MINUS_X, BUTTON_Y, BUTTON_SIZE, BUTTON_SIZE, border);
    rounded_button(s, PLUS_X, BUTTON_Y, BUTTON_SIZE, BUTTON_SIZE, border);

    s.rect(MINUS_X + 11, BUTTON_Y + 22, 26, 4, border);
    s.rect(PLUS_X + 11, BUTTON_Y + 22, 26, 4, border);
    s.rect(PLUS_X + 22, BUTTON_Y + 11, 4, 26, border);

    cmf_wp2_ui::draw::rrect(
        s,
        VOLUME_X,
        VOLUME_Y,
        VOLUME_W,
        VOLUME_H,
        16,
        Rgb::from565(GREY),
        Rgb(0, 0, 0),
    );
    cmf_wp2_ui::draw::rrect(
        s,
        VOLUME_X + 4,
        VOLUME_Y + 4,
        VOLUME_W - 8,
        VOLUME_H - 8,
        12,
        Rgb(0, 0, 0),
        Rgb(0, 0, 0),
    );

    if max > 0 {
        let width =
            ((value.min(max) as i32 * (VOLUME_W - 10)) / max as i32).clamp(0, VOLUME_W - 10);
        cmf_wp2_ui::draw::rrect(
            s,
            VOLUME_X + 5,
            VOLUME_Y + 5,
            width,
            VOLUME_H - 10,
            10,
            Rgb::from565(ACCENT),
            Rgb(0, 0, 0),
        );
    }

    let mut percent = crate::model::Buf::<8>::new();
    crate::model::percent(&mut percent, value, max);
    ntype82::centre(
        s,
        C,
        VOLUME_Y + 4,
        percent.as_str(),
        if enabled { WHITE } else { DIM },
        FontSize::Small,
    );
}

fn rounded_button(s: &mut impl Surface, x: i32, y: i32, w: i32, h: i32, colour: u16) {
    cmf_wp2_ui::draw::rrect(s, x, y, w, h, 12, Rgb::from565(colour), Rgb(0, 0, 0));
    cmf_wp2_ui::draw::rrect(s, x + 4, y + 4, w - 8, h - 8, 8, Rgb(0, 0, 0), Rgb(0, 0, 0));
}

fn draw_permission(s: &mut impl Surface, status: Status) {
    let (title, line1, line2, colour) = match status {
        Status::Ask => ("ALLOW", "ALLOW ON PHONE", "MUSIC COMPANION", ACCENT),
        _ => ("ACCESS DENIED", "OPEN MUSIC", "TURN IT ON", RED),
    };
    ntype82::centre(s, C, PERMISSION_TITLE_Y, title, colour, FontSize::Large);

    cmf_wp2_ui::draw::rrect(
        s,
        102,
        207,
        262,
        116,
        16,
        Rgb::from565(colour),
        Rgb(0, 0, 0),
    );
    cmf_wp2_ui::draw::rrect(s, 106, 211, 254, 108, 12, Rgb(0, 0, 0), Rgb(0, 0, 0));
    ntype82::centre(s, C, 228, line1, WHITE, FontSize::Small);
    ntype82::centre(s, C, 260, line2, GREY, FontSize::Small);
}

fn draw_footer(s: &mut impl Surface, m: &Model) {
    let (footer, colour) = match m.status {
        Status::Ask => ("SETUP", ACCENT),
        Status::Denied => ("DENIED", RED),
        Status::Unavailable => ("NO PHONE", ACCENT),
        Status::Silent if m.has_snapshot => ("CACHED", ACCENT),
        Status::Silent => ("NO PHONE", ACCENT),
        Status::Waiting => ("WAITING", DIM),
        Status::Ok if !m.active() => ("NO MEDIA / PHONE OK", DIM),
        Status::Ok => (
            if m.playing() {
                "PLAYING / PHONE OK"
            } else {
                "PAUSED / PHONE OK"
            },
            DIM,
        ),
    };
    ntype82::centre_clipped(s, C, FOOTER_Y, footer, 276, colour, FontSize::Small);
}

fn hit_circle(x: i32, y: i32, cx: i32, cy: i32, radius: i32) -> bool {
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy <= radius * radius
}

fn in_rect(x: i32, y: i32, left: i32, top: i32, width: i32, height: i32) -> bool {
    (left..left + width).contains(&x) && (top..top + height).contains(&y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Model;
    use cmf_wp2_ui::Fb;

    struct FakeArtwork {
        visible: bool,
        generation: u64,
    }

    impl ArtworkSource for FakeArtwork {
        fn visible(&self) -> bool {
            self.visible
        }

        fn generation(&self) -> u64 {
            self.generation
        }

        fn pixel(&self, x: u8, y: u8) -> Option<u16> {
            if !self.visible {
                return None;
            }
            Some(if x == 0 && y == 0 { ACCENT } else { WHITE })
        }
    }

    fn no_artwork() -> FakeArtwork {
        FakeArtwork {
            visible: false,
            generation: 0,
        }
    }

    #[test]
    fn every_state_draws_inside_the_circle() {
        let mut px = vec![0u16; (SIZE * SIZE) as usize];
        let mut fb = Fb::new(&mut px);
        let mut m = Model::new();
        for status in [
            Status::Waiting,
            Status::Ok,
            Status::Ask,
            Status::Denied,
            Status::Unavailable,
            Status::Silent,
        ] {
            m.status = status;
            frame_with_artwork(&mut fb, &m, &no_artwork());
        }
        for y in 0..SIZE {
            for x in 0..SIZE {
                let (dx, dy) = (x - C, y - C);
                if dx * dx + dy * dy > (C - 2) * (C - 2) {
                    assert_eq!(fb.at(x, y), 0, "{x},{y}");
                }
            }
        }
    }

    #[test]
    fn controls_have_large_distinct_touch_targets() {
        assert!(PLAY_R * 2 >= 120);
        assert!(PREV_R * 2 >= 88);
        assert!(NEXT_R * 2 >= 88);
        assert!(BUTTON_SIZE * 2 >= 88);

        assert_eq!(action(PREV_X, CONTROL_Y, 15, true, true), Action::Previous);
        assert_eq!(action(PLAY_X, CONTROL_Y, 15, true, true), Action::Toggle);
        assert_eq!(action(NEXT_X, CONTROL_Y, 15, true, true), Action::Next);
        assert_eq!(
            action(MINUS_X + 1, BUTTON_Y + 1, 15, true, true),
            Action::VolumeDown
        );
        assert_eq!(
            action(PLUS_X + 1, BUTTON_Y + 1, 15, true, true),
            Action::VolumeUp
        );
        assert_eq!(
            action(VOLUME_X, VOLUME_Y + 16, 15, true, true),
            Action::SetVolume(0)
        );
        assert_eq!(
            action(VOLUME_X + VOLUME_W - 1, VOLUME_Y + 16, 15, true, true),
            Action::SetVolume(15)
        );
    }

    #[test]
    fn control_regions_do_not_overlap() {
        assert!(PREV_X + PREV_R < PLAY_X - PLAY_R);
        assert!(PLAY_X + PLAY_R < NEXT_X - NEXT_R);
        assert!(CONTROL_Y + PLAY_R < BUTTON_Y);
        assert!(MINUS_X + BUTTON_SIZE < VOLUME_X);
        assert!(VOLUME_X + VOLUME_W < PLUS_X);
    }

    #[test]
    fn disabled_controls_do_not_queue_commands() {
        assert_eq!(action(PLAY_X, CONTROL_Y, 15, false, false), Action::None);
        assert_eq!(
            action(VOLUME_X, VOLUME_Y + 16, 15, true, false),
            Action::None
        );
        assert_eq!(
            action(VOLUME_X, VOLUME_Y + 16, 0, true, false),
            Action::None
        );
    }

    #[test]
    fn artwork_is_scaled_to_the_cover_tile() {
        let mut px = vec![0u16; (SIZE * SIZE) as usize];
        let mut fb = Fb::new(&mut px);
        let mut m = Model::new();
        m.metadata.flags = 3;
        m.status = Status::Ok;
        let artwork = FakeArtwork {
            visible: true,
            generation: 42,
        };
        frame_with_artwork(&mut fb, &m, &artwork);
        assert_eq!(fb.at(ART_X, ART_Y), ACCENT);
        assert_eq!(fb.at(ART_X + 1, ART_Y + 1), ACCENT);
        assert_eq!(fb.at(ART_X + ART_SIZE - 1, ART_Y + ART_SIZE - 1), WHITE);
    }

    #[test]
    fn uploaded_font_fits_the_visible_labels() {
        for text in [
            "PLAYING",
            "PAUSED",
            "NO MEDIA",
            "ACCESS DENIED",
            "ALLOW ON PHONE",
            "MUSIC COMPANION",
            "PHONE OK",
            "NO PHONE",
            "100%",
        ] {
            assert!(ntype82::width(FontSize::Small, text) <= 250, "{text}");
        }
        assert!(ntype82::width(FontSize::Large, "ACCESS DENIED") <= 250);
    }
}
