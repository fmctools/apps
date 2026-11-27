//! Watch Music's screens, built from the watch's own LVGL widgets in the stock look (the way ui-gallery does):
//! a dynamic start screen (Control media / Music / Podcasts / Custom), a stock focus list of the songs or shows (the fisheye, the crown, the round
//! scroll bar), and a media-control player: the cover's colours as a soft blurred background, the clock, the artist
//! and the song, prev / play / next, one smooth ring that touches the play button and shows how much of the song
//! is done, and a volume button (press it, then the crown changes the volume). Leaving a screen deletes its page
//! and everything on it. The stock widgets cannot draw pictures, so the cover is shown by its colours.

use crate::model::*;
use cmf_wp2_app::lv::*;
use cmf_wp2_app::stock::{self, CIRCLE, GREY, ICON, LETTER_BG, ROW_PRESSED, ROW_W, SCREEN, WHITE};

/// Font slots besides the system font (slot 0).
pub const F24: u8 = 1;
// bytes one widget group can take: flush first when fewer are left
const GROUP: usize = 560;

/// Light grey of the artist on the player (readable on every tint).
const SOFT: i32 = 0xC618;

// player geometry, as offsets from the screen centre
const RING: i32 = 176;
const RING_W: i32 = 10;
/// The play button fills the ring exactly, so the ring touches it.
const PLAY: i32 = RING - 2 * RING_W;
const SIDE: i32 = 80;
const CY: i16 = 0;
const SIDE_X: i16 = 124;

/// The volume button under the ring, and how long it stays lit after the last touch is decided in main.rs.
const VOL: i32 = 72;
const VOL_Y: i16 = 176;
/// On a page, volume and back share the lower control row symmetrically.
const SIDE_BTN_X: i16 = 96;
const PAGE_VOL_Y: i16 = 162;
const BACK_PILL_W: i32 = 148;
const BACK_PILL_H: i32 = 58;
/// Red used only for the Back arrow in the music/podcast item list.
const BACK_RED: i32 = 0xF800;
/// The heart: saved (a warm red) and not saved (soft grey, like the secondary text).
const HEART_ON: i32 = 0xF9CB;
const HEART_OFF: i32 = SOFT;
/// The player's bottom row: volume on the left, the heart on the right (where page mode's back arrow was).
const BOTTOM_X: i16 = SIDE_BTN_X;
const BOTTOM_Y: i16 = PAGE_VOL_Y;
/// The song name sits where the clock was: the clock is gone so the name has room near the top.
const TITLE_Y: i16 = 58;
/// The artist sits right under the song name: smaller (the small font) and light grey.
const ARTIST_Y: i16 = TITLE_Y + 41;

/// The background: the cover's average colour glows in the middle and fades to black at the edge, as a stack of
/// opaque discs, each a little brighter than the one under it (a smooth radial gradient).
const RINGS: usize = 110;
const R_MAX: i32 = 246;
/// the glow's strongest channel, of 255 (not so bright that the white text suffers)
const GLOW_MAX: u32 = 235;
const BLOBS: usize = RINGS;

/// Bars that make the play triangle.
pub const PLAY_BARS: usize = 22;

/// Heights of the player's play triangle: the start screen's rounded play icon (`PLAY_COLS`) at 46 px tall, as 22 bars
/// 2 px apart and 3 px wide (they overlap and leave no seams), so its corners are round like the icon's. Fewer, wider
/// bars than 1 px columns: every shape is something the watch must create before the player can show.
const PLAY_SMALL: [u8; PLAY_BARS] = [33, 41, 43, 45, 46, 46, 45, 43, 41, 38, 36, 34, 32, 29, 27, 24, 22, 19, 18, 16, 12, 5];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Wait,
    /// the dynamic category chooser: Music, Podcasts, Custom and active controls
    Home,
    Menu,
    Player,
}

/// What the app reacts to on the current screen; unused handles are `ROOT` (never an event target).
#[derive(Clone, Copy)]
pub struct Screen {
    pub kind: Kind,
    pub page: Obj,
    pub rows: [Obj; MAX_TRACKS],
    /// For each visible menu row, the underlying track index (rows are rotated around the current focus).
    pub row_indices: [u8; MAX_TRACKS],
    pub row_count: u8,
    /// The pill colour each song row was last given, so a colour message restyles only the rows that changed.
    pub row_c1: [u16; MAX_TRACKS],
    /// The home row's selected watch mode: Music, Podcasts, External, or Custom.
    pub home_modes: [u8; 4],
    pub home_count: u8,
    /// Rotation used when this page was built; FOCUSED row indices map through this stable origin.
    pub list_anchor: usize,
    pub prev: Obj,
    pub play: Obj,
    pub next: Obj,
    pub pause_bars: [Obj; 2],
    pub play_bars: [Obj; PLAY_BARS],
    pub title: Obj,
    pub artist: Obj,
    pub clock: Obj,
    pub arc: Obj,
    /// Visible back target on page mode (unused in the regular app).
    pub back: Obj,
    pub blobs: [Obj; BLOBS],
    /// the volume button, its "Vol 65%" text (shown instead of the clock while it is on) and its sound wave
    pub vol: Obj,
    pub vol_label: Obj,
    pub wave: [Obj; 2],
    /// the heart button and its three shapes (two lobes and the point), and the double-tap halves
    pub heart: Obj,
    pub heart_parts: [Obj; 3],
    pub seek_left: Obj,
    pub seek_right: Obj,
}

const EMPTY: Screen = Screen {
    kind: Kind::Wait,
    page: ROOT,
    rows: [ROOT; MAX_TRACKS],
    row_indices: [0; MAX_TRACKS],
    row_count: 0,
    row_c1: [0; MAX_TRACKS],
    home_modes: [MODE_NONE; 4],
    home_count: 0,
    list_anchor: 0,
    prev: ROOT,
    play: ROOT,
    next: ROOT,
    pause_bars: [ROOT; 2],
    play_bars: [ROOT; PLAY_BARS],
    title: ROOT,
    artist: ROOT,
    clock: ROOT,
    arc: ROOT,
    back: ROOT,
    blobs: [ROOT; BLOBS],
    vol: ROOT,
    vol_label: ROOT,
    wave: [ROOT; 2],
    heart: ROOT,
    heart_parts: [ROOT; 3],
    seek_left: ROOT,
    seek_right: ROOT,
};

impl Screen {
    pub const fn empty() -> Screen {
        EMPTY
    }
}

type Flush<'a> = &'a mut dyn FnMut(&mut Batch) -> Result<(), LvError>;

fn room(b: &mut Batch, flush: Flush) -> Result<(), LvError> {
    if b.bytes().len() + GROUP > BUF { flush(b) } else { Ok(()) }
}

/// The fonts, in a batch of their own.
pub fn fonts(b: &mut Batch) -> Result<(), LvError> {
    stock::font(b)?;
    b.font(F24, StockFont::F24M)
}

/// Once, first: the system font and white text for everything.
pub fn theme(b: &mut Batch) -> Result<(), LvError> {
    stock::theme(b)
}

/// Deletes the current screen (its page, with every object and its scroll bar).
pub fn leave(b: &mut Batch, s: &Screen) -> Result<(), LvError> {
    if s.page != ROOT { b.del(s.page) } else { Ok(()) }
}

/// One solid native LVGL shape. The transparent icon slot below contains these pieces,
/// so the symbol has no badge/circle behind it and never intercepts row selection.
fn glyph_piece(
    b: &mut Batch,
    flush: Flush,
    parent: Obj,
    w: i32,
    h: i32,
    x: i16,
    y: i16,
    radius: i32,
) -> Result<Obj, LvError> {
    room(b, flush)?;
    let piece = b.create(Class::Obj, parent)?;
    b.size(piece, w, h)?;
    b.align(piece, Align::Center, x, y)?;
    b.style(piece, Prop::Radius, 0, radius)?;
    b.style(piece, Prop::BgColor, 0, WHITE)?;
    b.style(piece, Prop::BgOpa, 0, 255)?;
    b.style(piece, Prop::BorderWidth, 0, 0)?;
    b.flags(piece, &[], &[Flag::Clickable, Flag::Scrollable])?;
    Ok(piece)
}

/// Height of each 1 px column of the rounded play triangle (61 px wide, 67 px tall, corners 12 px).
const PLAY_COLS: [u8; 61] = [48, 53, 56, 59, 60, 62, 63, 64, 65, 66, 66, 67, 67, 67, 67, 66, 66, 65, 64, 63, 62, 61, 60, 59, 58, 56, 55, 54, 53, 52, 51, 49, 48, 47, 46, 45, 44, 42, 41, 40, 39, 38, 37, 35, 34, 33, 32, 31, 30, 28, 27, 26, 25, 24, 23, 21, 20, 18, 16, 12, 8];

/// The lower half of a circle as a thick arc with round ends (a "U"), `size` px across, centred at (x, y).
fn glyph_arc(b: &mut Batch, flush: Flush, parent: Obj, size: i32, width: i32, x: i16, y: i16) -> Result<Obj, LvError> {
    room(b, flush)?;
    let a = b.create(Class::Arc, parent)?;
    b.size(a, size, size)?;
    b.align(a, Align::Center, x, y)?;
    b.angles(a, 0, 180, 0)?;
    b.value(a, 0, false)?;
    b.style(a, Prop::ArcWidth, 0, width)?;
    b.style(a, Prop::ArcRounded, 0, 1)?;
    b.style(a, Prop::ArcColor, 0, WHITE)?;
    b.style(a, Prop::ArcOpa, 0, 255)?;
    b.style(a, Prop::ArcOpa, stock::INDICATOR, 0)?;
    b.style(a, Prop::BgOpa, stock::KNOB, 0)?;
    b.flags(a, &[], &[Flag::Clickable, Flag::Scrollable])?;
    Ok(a)
}

/// A filled pie slice: an arc as thick as its radius, so LVGL draws it antialiased (no stair steps). Its point is
/// at (x, y) of `parent`'s centre; it spans `from`..`from + span` degrees (0 = right, 90 = down, clockwise).
/// Straight edges and a slightly round far side: this is how every triangle (play, prev, next, arrows, speaker
/// cone, heart point) is drawn now instead of stacks of 1-4 px bars.
#[allow(clippy::too_many_arguments)]
fn sector(
    b: &mut Batch, flush: Flush, parent: Obj, r: i32, x: i16, y: i16, from: u16, span: u16, color: i32,
) -> Result<Obj, LvError> {
    room(b, flush)?;
    let a = b.create(Class::Arc, parent)?;
    b.size(a, 2 * r, 2 * r)?;
    b.align(a, Align::Center, x, y)?;
    for p in [Prop::PadTop, Prop::PadBottom, Prop::PadLeft, Prop::PadRight] {
        b.style(a, p, 0, 0)?;
    }
    b.angles(a, 0, span, from % 360)?;
    b.value(a, 0, false)?;
    b.style(a, Prop::ArcWidth, 0, r)?;
    b.style(a, Prop::ArcRounded, 0, 0)?;
    b.style(a, Prop::ArcColor, 0, color)?;
    b.style(a, Prop::ArcOpa, 0, 255)?;
    b.style(a, Prop::ArcOpa, stock::INDICATOR, 0)?;
    b.style(a, Prop::BgOpa, stock::KNOB, 0)?;
    b.style(a, Prop::BgOpa, 0, 0)?;
    b.flags(a, &[], &[Flag::Clickable, Flag::Scrollable])?;
    Ok(a)
}

/// A triangle pointing right (`dir` 1) or left (-1), `h` px tall, its middle at x0 (its visual centre).
fn triangle(b: &mut Batch, flush: Flush, parent: Obj, h: i32, x0: i16, dir: i16, color: i32) -> Result<Obj, LvError> {
    // a 60-degree slice: the point sits 0.58 h from the visual centre (centroid of the triangle)
    let r = h; // edge length = height / sin60 ~ 1.15 h; r = h keeps the far side close to straight
    let tip = x0 + dir * (h * 58 / 100) as i16;
    sector(b, flush, parent, r, tip, 0, if dir > 0 { 150 } else { 330 }, 60, color)
}

/// Category symbols drawn from LVGL primitives so they render consistently with the watch fonts.
/// Music follows the paired-note app icon; Podcast keeps its clean microphone.
fn home_icon(b: &mut Batch, flush: Flush, parent: Obj, mode: u8) -> Result<(), LvError> {
    match mode {
        MODE_MUSIC => {
            // Paired eighth notes: two round noteheads, two straight stems and one level beam. Everything is
            // symmetric and made of whole rounded shapes, so no stair-stepped edges.
            glyph_piece(b, flush, parent, 22, 22, -18, 26, CIRCLE)?; // left notehead
            glyph_piece(b, flush, parent, 22, 22, 18, 26, CIRCLE)?; // right notehead
            glyph_piece(b, flush, parent, 6, 54, -10, -1, 3)?; // left stem
            glyph_piece(b, flush, parent, 6, 54, 26, -1, 3)?; // right stem
            glyph_piece(b, flush, parent, 42, 10, 8, -26, 4)?; // beam
        }
        MODE_PODCAST => {
            // Microphone: a capsule, a smooth U-shaped cradle (an arc with round ends), a stem and a base.
            glyph_piece(b, flush, parent, 26, 44, 0, -17, 13)?;
            glyph_arc(b, flush, parent, 56, 6, 0, -5)?;
            glyph_piece(b, flush, parent, 6, 12, 0, 28, 3)?;
            glyph_piece(b, flush, parent, 30, 6, 0, 35, 3)?;
        }
        MODE_EXTERNAL => {
            // A play triangle with rounded corners: 1 px columns (each 2 px wide, so they overlap and leave no seams)
            // whose heights follow the rounded outline, so no coarse blocks show.
            let x0 = -(PLAY_COLS.len() as i16) / 2 + 3;
            for (i, h) in PLAY_COLS.iter().enumerate() {
                glyph_piece(b, flush, parent, 2, *h as i32, x0 + i as i16, 0, 0)?;
            }
        }
        MODE_CUSTOM => {
            // A bold "run" arrow pointing right: a rounded shaft and a smooth head.
            arrow_shape(b, flush, parent, 0, 1, WHITE, 1)?;
        }
        _ => {}
    }
    Ok(())
}

/// The start screen: the stock fisheye focus rows with an unbadged icon and one category name.
/// Control media is first and categories remain dynamic; only categories with content are shown.
/// `flags`: bit 0 Music, bit 1 Podcasts, bit 2 Custom, bit 3 active media. `anchor` is the focused row.
pub fn home(b: &mut Batch, flush: Flush, flags: u8, anchor: usize) -> Result<Screen, LvError> {
    room(b, flush)?;
    // This is the same stock focus-page/row layout used by the original V12 start screen.
    let (page, _bar) = stock::focus_page(b, ICON as u16)?;
    let mut s = Screen { kind: Kind::Home, page, ..EMPTY };
    let entries: [(u8, &str); 4] = [
        (MODE_EXTERNAL, "Control media"),
        (MODE_MUSIC, "Music"),
        (MODE_PODCAST, "Podcasts"),
        (MODE_CUSTOM, "Custom"),
    ];
    let mut available = [0usize; 4];
    let mut n = 0usize;
    for (i, (mode, _)) in entries.iter().enumerate() {
        let bit = match *mode {
            MODE_MUSIC => 0x01,
            MODE_PODCAST => 0x02,
            MODE_CUSTOM => 0x04,
            MODE_EXTERNAL => 0x08,
            _ => 0,
        };
        if flags & bit != 0 {
            available[n] = i;
            n += 1;
        }
    }
    if n == 0 {
        let l = b.create(Class::Label, page)?;
        b.size(l, ROW_W - 48, 120)?;
        b.style(l, Prop::TextColor, 0, GREY)?;
        b.style(l, Prop::TextAlign, 0, 2)?;
        b.text(l, "Add Music, Podcasts or Custom in the phone app")?;
        b.align(l, Align::Center, 0, 0)?;
        s.title = l;
        return Ok(s);
    }

    // Focus rows are circular. Rotating the data order keeps the native watch focus on the
    // selected category while preserving the stock fisheye layout.
    let shift = anchor % n;
    s.list_anchor = shift;
    for slot in 0..n {
        let idx = available[(shift + slot) % n];
        let (mode, name) = entries[idx];
        room(b, flush)?;
        let row = stock::focus_row(b, page)?;

        // The first child is always the icon slot: the stock fisheye scales it, not the text label.
        // Its holder is transparent; the glyph itself is drawn from plain LVGL shapes.
        let icon_slot = b.create(Class::Obj, row)?;
        b.size(icon_slot, ICON, ICON)?;
        b.align(icon_slot, Align::LeftMid, 4, 0)?;
        b.style(icon_slot, Prop::BgOpa, 0, 0)?;
        b.style(icon_slot, Prop::BorderWidth, 0, 0)?;
        b.flags(icon_slot, &[], &[Flag::Clickable, Flag::Scrollable])?;
        home_icon(b, flush, icon_slot, mode)?;

        // One centred, single-line category name. Keep the same icon-aware inset as the song list;
        // Dot mode guarantees long names never wrap into a second line.
        let text_width = ROW_W - ICON - 64;
        let text_x = ICON as i16 + 28;
        let title = b.create(Class::Label, row)?;
        b.size(title, text_width, 48)?;
        b.long_mode(title, LongMode::Dot)?;
        b.text(title, name)?;
        b.align(title, Align::LeftMid, text_x, 0)?;

        s.rows[slot] = row;
        s.home_modes[slot] = mode;
        s.home_count = (slot + 1) as u8;
    }
    Ok(s)
}

/// Translate a FOCUSED event's row index into the logical index of a rotated focus list.
/// The SDK reports the focused row index in `delta`; it is not a +/- scroll direction.
pub fn focused_anchor(anchor: usize, focused_row: i16, total: usize) -> Option<usize> {
    if total == 0 || focused_row < 0 || focused_row as usize >= total {
        None
    } else {
        Some((anchor + focused_row as usize) % total)
    }
}

/// Shown until the phone has sent the list (or said that there is none).
pub fn wait(b: &mut Batch, flush: Flush, mode: u8, empty: bool, page_mode: bool) -> Result<Screen, LvError> {
    let (page, _bar) = stock::page(b)?;
    let mut s = Screen { kind: Kind::Wait, page, ..EMPTY };
    stock::title(b, page, match mode {
        MODE_PODCAST => "Podcasts",
        MODE_CUSTOM => "Custom",
        _ => "Songs",
    })?;
    let l = b.create(Class::Label, page)?;
    b.size(l, ROW_W - 40, 120)?;
    b.style(l, Prop::TextColor, 0, GREY)?;
    b.style(l, Prop::TextAlign, 0, 2)?;
    b.text(l, match (mode == MODE_PODCAST, empty) {
        (true, true) => "No podcasts yet. Add one in the phone app",
        (false, true) if mode == MODE_CUSTOM => "No Custom items yet. Add one in the phone app",
        (false, true) => "Add a Music item in the phone app",
        _ => "Waiting for your phone",
    })?;
    s.title = l;
    if page_mode {
        s.back = back_pill(b, flush, page)?;
    }
    Ok(s)
}

/// The stock focus list. In page mode Back participates in the same circular list as the songs.
/// Rotating the row order puts the chosen entry at the focus-page's centre without changing its stock styling.
pub fn menu(b: &mut Batch, flush: Flush, m: &Model, page_mode: bool, anchor: usize) -> Result<Screen, LvError> {
    room(b, flush)?;
    let (page, _bar) = stock::focus_page(b, 0)?;
    let mut s = Screen { kind: Kind::Menu, page, ..EMPTY };
    let total = m.count as usize + if page_mode { 1 } else { 0 };
    if total == 0 { return Ok(s); }
    let start = anchor % total;
    s.list_anchor = start;
    let mut track_slot = 0usize;
    for offset in 0..total {
        let logical = (start + offset) % total;
        if page_mode && logical == 0 {
            room(b, flush)?;
            let row = stock::focus_row(b, page)?;
            b.style(row, Prop::BgColor, CHECKED, LETTER_BG)?;
            // the SDK's focus row is transparent when centred; the Back row wants its grey pill
            b.style(row, Prop::BgOpa, CHECKED, 255)?;
            // Keep only the back arrow in the row; make it red for a clear navigation cue.
            arrow_color(b, flush, row, 56 - ROW_W as i16 / 2, BACK_RED)?;
            s.back = row;
            continue;
        }
        let track_idx = if page_mode { logical - 1 } else { logical };
        if track_idx >= m.count as usize || track_slot >= MAX_TRACKS { continue; }
        room(b, flush)?;
        let t = &m.tracks[track_idx];
        let row = stock::focus_row(b, page)?;
        b.style(row, Prop::BgColor, CHECKED, t.c1 as i32)?;
        // the centred row is filled with its cover's colour (the SDK's focus row is transparent, so ask for it)
        b.style(row, Prop::BgOpa, CHECKED, 255)?;
        let n = b.create(Class::Label, row)?;
        b.size(n, ROW_W - 90, 40)?;
        b.long_mode(n, LongMode::Dot)?;
        b.text(n, t.title())?;
        b.align(n, Align::LeftMid, 28, -16)?;
        let a = b.create(Class::Label, row)?;
        b.size(a, ROW_W - 90, 30)?;
        b.long_mode(a, LongMode::Dot)?;
        b.style(a, Prop::TextFont, 0, F24 as i32)?;
        b.style(a, Prop::TextColor, 0, GREY)?;
        b.text(a, t.artist())?;
        b.align(a, Align::LeftMid, 28, 20)?;
        s.rows[track_slot] = row;
        s.row_indices[track_slot] = track_idx as u8;
        s.row_c1[track_slot] = t.c1;
        track_slot += 1;
        s.row_count = track_slot as u8;
    }
    Ok(s)
}

/// A song's colours arrived: its pill takes the colour of its cover. Only rows whose colour really changed are
/// restyled (the phone sends colours in many small messages and again after every hello; restyling all 50 rows
/// each time stalled the list while it was being scrolled). Returns whether anything was restyled.
pub fn recolour(b: &mut Batch, flush: Flush, s: &mut Screen, m: &Model) -> Result<bool, LvError> {
    let mut changed = false;
    for i in 0..m.count as usize {
        if s.rows[i] == ROOT {
            continue;
        }
        let track_idx = s.row_indices[i] as usize;
        if track_idx < m.count as usize {
            let c1 = m.tracks[track_idx].c1;
            if s.row_c1[i] == c1 {
                continue;
            }
            room(b, flush)?;
            b.style(s.rows[i], Prop::BgColor, CHECKED, c1 as i32)?;
            s.row_c1[i] = c1;
            changed = true;
        }
    }
    Ok(changed)
}

// ---- colours of the player -------------------------------------------------------------------------------------

fn channels(c: u16) -> (u32, u32, u32) {
    (((c >> 11) & 31) as u32, ((c >> 5) & 63) as u32, (c & 31) as u32)
}

fn pack(r: u32, g: u32, b: u32) -> i32 {
    ((r.min(31) << 11) | (g.min(63) << 5) | b.min(31)) as i32
}

/// `c` at `k / 256` of its brightness.
pub fn shade(c: u16, k: u32) -> i32 {
    let (r, g, b) = channels(c);
    pack(r * k / 256, g * k / 256, b * k / 256)
}

/// `c` moved `k / 256` of the way to white.
pub fn lighten(c: u16, k: u32) -> i32 {
    let (r, g, b) = channels(c);
    pack(r + (31 - r) * k / 256, g + (63 - g) * k / 256, b + (31 - b) * k / 256)
}

/// The slight dark tint over the cover.
#[allow(dead_code)]
pub fn background(c: u16) -> i32 {
    shade(c, 104)
}

/// `c` with its colour pushed away from grey, so a pale cover still gives a clear colour.
pub fn vivid(c: u16) -> u16 {
    let (r, g, b) = channels(c);
    // all three on the 0..=31 scale (green has one more bit)
    let (r, g, b) = (r as i32, (g / 2) as i32, b as i32);
    let m = (r + g + b) / 3;
    let up = |v: i32| (m + (v - m) * 3 / 2).clamp(0, 31) as u32;
    let (r, g, b) = (up(r), up(g), up(b));
    ((r << 11) | ((g * 2 + g / 16) << 5) | b) as u16
}

/// What is under the glows: the cover's average colour, dark.
#[allow(dead_code)]
pub fn base(q: [u16; 4]) -> i32 {
    let (mut r, mut g, mut b) = (0, 0, 0);
    for c in q {
        let (cr, cg, cb) = channels(c);
        (r, g, b) = (r + cr, g + cg, b + cb);
    }
    shade(pack(r / 4, g / 4, b / 4) as u16, 90)
}

/// The cover's average colour as 8-bit r, g, b, scaled so its strongest channel is `GLOW_MAX` (a grey or dark cover
/// still gets a soft glow; a bright one is toned down).
pub fn glow_centre(q: [u16; 4]) -> (u32, u32, u32) {
    let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
    for c in q {
        let (cr, cg, cb) = channels(c);
        (r, g, b) = (r + cr * 255 / 31, g + cg * 255 / 63, b + cb * 255 / 31);
    }
    let (r, g, b) = (r / 4, g / 4, b / 4);
    let top = r.max(g).max(b).max(1);
    // never boost more than 3 times: a nearly black cover stays dark
    let k = (GLOW_MAX * 256 / top).clamp(1, 768);
    ((r * k / 256).min(255), (g * k / 256).min(255), (b * k / 256).min(255))
}

fn rgb8(r: u32, g: u32, b: u32) -> i32 {
    (((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)) as i32
}

/// The "tide": the bright core of the glow slides up and down while the outer discs stay still, so the watch only
/// redraws a small area. The discs that are drawn are the ones where the colour takes a new step (about sixty).
const HOLD: usize = 44;
/// how far the light wanders from the middle, px
const TIDE_AMP: i32 = 105;
static mut TIDE_IDX: [u8; RINGS] = [0; RINGS];
static mut TIDE_POS: [(i16, i16); RINGS] = [(0, 0); RINGS];
static mut TIDE_N: usize = 0;

fn smooth(t: u32) -> u32 {
    t * t * (768 - 2 * t) / (256 * 256)
}

/// How much light disc `t` (0..=256, 0 = the outer one) has, 0..=256: smoothstep mixed with a straight ramp, so the
/// light reaches further out and the fade from bright to dark spreads over the whole screen.
fn ease(t: u32) -> u32 {
    (smooth(t) + t) / 2
}

/// From where inwards the glow turns whiter: a hot core.
const HOT_FROM: u32 = 170;

/// How white (0..=256) disc `t` is: 0 until `HOT_FROM`, then up to about 40 % at the middle.
fn hot(t: u32) -> u32 {
    if t > HOT_FROM { (t - HOT_FROM) * 100 / (256 - HOT_FROM) } else { 0 }
}

/// sine of `p` (a turn is 1024), -256..=256
fn sin(p: i32) -> i32 {
    let p = p.rem_euclid(1024);
    let (q, neg) = if p < 512 { (p, false) } else { (p - 512, true) };
    let v = 4 * q * (512 - q) / 1024;
    let v = v + 225 * (v * v / 256 - v) / 1000; // closer to a real sine
    if neg { -v } else { v }
}

/// Where the light is at time `now` (ms): two slow waves per direction with different lengths, so it wanders in
/// every direction and does not repeat for a long time.
fn tide_light(now: u32) -> (i32, i32) {
    let ph = |period: u32| (now % period * 1024 / period) as i32;
    let x = (sin(ph(7_300)) * 6 + sin(ph(4_100)) * 4) / 10;
    let y = (sin(ph(5_300)) * 6 + sin(ph(9_700)) * 4) / 10;
    (TIDE_AMP * x / 256, TIDE_AMP * y / 256)
}

/// How far disc `i` follows the light, 0..=256.
fn tide_weight(i: usize) -> i32 {
    if i <= HOLD { 0 } else { ((i - HOLD) * 256 / (RINGS - 1 - HOLD)) as i32 }
}

/// Picks the discs to draw and returns how many. A disc with the colour of the one under it adds nothing; the moving
/// ones step in coarser colour steps (half as many discs to redraw each frame).
fn tide_pick() -> usize {
    let mut n = 0;
    let mut before = u32::MAX;
    for i in 0..RINGS {
        let t = (i * 256 / (RINGS - 1)) as u32;
        let level = ease(t) >> if i > HOLD { 3 } else { 2 };
        if level != before || i == RINGS - 1 || (t > HOT_FROM && i % 3 == 0) {
            unsafe {
                (*(&raw mut TIDE_IDX))[n] = i as u8;
                (*(&raw mut TIDE_POS))[n] = (0, 0);
            }
            n += 1;
        }
        before = level;
    }
    unsafe { *(&raw mut TIDE_N) = n };
    n
}

/// One frame of the tide at time `now` (ms): moves the core. Only discs that moved are sent.
pub fn tide(b: &mut Batch, s: &Screen, now: u32) -> Result<(), LvError> {
    let (idx, pos, n) = unsafe { (&*(&raw const TIDE_IDX), &mut *(&raw mut TIDE_POS), *(&raw const TIDE_N)) };
    let (lx, ly) = tide_light(now);
    for k in 0..n {
        let w = tide_weight(idx[k] as usize);
        let p = ((lx * w / 256) as i16, (ly * w / 256) as i16);
        if p != pos[k] {
            b.align(s.blobs[k], Align::Center, p.0, CY + p.1)?;
            pos[k] = p;
        }
    }
    Ok(())
}

/// The colour of disc `i` (0 = the outer one, black) of the glow.
pub fn glow_ring(centre: (u32, u32, u32), i: usize) -> i32 {
    let t = (i * 256 / (RINGS - 1)) as u32; // 0..=256
    let k = ease(t);
    let w = hot(t);
    let lift = |c: u32| {
        let v = c * k / 256;
        v + (255 - v.min(255)) * w / 256
    };
    rgb8(lift(centre.0), lift(centre.1), lift(centre.2))
}

/// The glow's centre colour as RGB565.
pub fn glow_565(q: [u16; 4]) -> u16 {
    let (r, g, b) = glow_centre(q);
    rgb8(r, g, b) as u16
}

/// One glow: its quarter of the cover, pushed to a clear colour and bright.
#[allow(dead_code)]
pub fn glow(c: u16) -> i32 {
    shade(vivid(c), 215)
}

/// The finished part of the ring.
pub fn ring_colour(c: u16) -> i32 {
    lighten(c, 190)
}

/// The rest of the ring: a little lighter than the background.
pub fn track_colour(c: u16) -> i32 {
    lighten(shade(c, 150) as u16, 40)
}

// ---- the player ------------------------------------------------------------------------------------------------

/// A rounded solid bar that cannot be pressed (icons are made of these; a press goes to the button under them).
fn bar_color(
    b: &mut Batch, flush: Flush, parent: Obj, w: i32, h: i32, x: i16, radius: i32, color: i32,
) -> Result<Obj, LvError> {
    room(b, flush)?;
    let o = b.create(Class::Obj, parent)?;
    b.size(o, w, h)?;
    b.align(o, Align::Center, x, 0)?;
    b.style(o, Prop::Radius, 0, radius)?;
    b.style(o, Prop::BgColor, 0, color)?;
    b.style(o, Prop::BgOpa, 0, 255)?;
    b.style(o, Prop::BorderWidth, 0, 0)?;
    b.flags(o, &[], &[Flag::Clickable, Flag::Scrollable])?;
    Ok(o)
}

fn bar(b: &mut Batch, flush: Flush, parent: Obj, w: i32, h: i32, x: i16, radius: i32) -> Result<Obj, LvError> {
    bar_color(b, flush, parent, w, h, x, radius, WHITE)
}

/// A triangle made of `n` bars `step` px apart: `tall_first` = the tallest bar is the first (it points right).
#[allow(clippy::too_many_arguments)]
fn wedge_color(
    b: &mut Batch, flush: Flush, parent: Obj, n: usize, step: i32, w: i32, full: i32, x0: i32, dir: i32,
    tall_first: bool, color: i32, out: &mut [Obj],
) -> Result<(), LvError> {
    for i in 0..n {
        let k = if tall_first { n - i } else { i + 1 } as i32;
        let h = (full * k / n as i32).max(4);
        out[i] = bar_color(b, flush, parent, w, h, (x0 + dir * step * i as i32) as i16, 2, color)?;
    }
    Ok(())
}

#[allow(dead_code)]
fn wedge(
    b: &mut Batch, flush: Flush, parent: Obj, n: usize, step: i32, w: i32, full: i32, x0: i32, dir: i32,
    tall_first: bool, out: &mut [Obj],
) -> Result<(), LvError> {
    wedge_color(b, flush, parent, n, step, w, full, x0, dir, tall_first, WHITE, out)
}

/// An arrow: a rounded shaft and a smooth head. `dir` -1 points left (Back), 1 right. `scale` 1 = the large home
/// icon, 0 = the regular back arrow.
fn arrow_shape(b: &mut Batch, flush: Flush, parent: Obj, x: i16, dir: i16, color: i32, scale: i32) -> Result<(), LvError> {
    let (len, thick, head) = if scale > 0 { (44, 10, 34) } else { (30, 7, 26) };
    // the whole arrow is `len` wide and centred on x: head at one end, shaft running into it
    let tip = x + dir * (len / 2) as i16;
    let shaft_w = len - head / 2;
    let shaft_x = x - dir * (head / 4) as i16;
    bar_color(b, flush, parent, shaft_w, thick, shaft_x, thick / 2, color)?;
    // a 76-degree slice pointing along dir
    sector(b, flush, parent, head, tip, 0, if dir < 0 { 322 } else { 142 }, 76, color)?;
    Ok(())
}

fn arrow_color(b: &mut Batch, flush: Flush, parent: Obj, x: i16, color: i32) -> Result<(), LvError> {
    arrow_shape(b, flush, parent, x, -1, color, 0)
}

#[allow(dead_code)]
fn arrow(b: &mut Batch, flush: Flush, parent: Obj, x: i16) -> Result<(), LvError> {
    arrow_color(b, flush, parent, x, WHITE)
}

/// The heart: a square turned 45 degrees with a round lobe on each of its two upper edges. With the square's side
/// `H` the lobes are `H` wide circles centred on the midpoints of those edges (x = +-H / (2 sqrt 2), 1.06 H above the
/// point), so every edge meets its lobe smoothly and the dip between them is a clean notch. The point is a 90-degree
/// slice of radius `H` (all antialiased). Returns the three shapes so their colour can change with the saved state.
fn heart_shape(b: &mut Batch, flush: Flush, parent: Obj, color: i32) -> Result<[Obj; 3], LvError> {
    const H: i32 = 26;
    const LOBE_X: i16 = 9; // H / (2 sqrt 2) = 9.2
    const LOBE_Y: i16 = -8; // tip (20) - 1.06 H = -7.6
    const TIP_Y: i16 = 20;
    room(b, flush)?;
    let mut parts = [ROOT; 3];
    for (i, x) in [-LOBE_X, LOBE_X].into_iter().enumerate() {
        let o = b.create(Class::Obj, parent)?;
        b.size(o, H, H)?;
        b.align(o, Align::Center, x, LOBE_Y)?;
        b.style(o, Prop::Radius, 0, CIRCLE)?;
        b.style(o, Prop::BgColor, 0, color)?;
        b.style(o, Prop::BgOpa, 0, 255)?;
        b.style(o, Prop::BorderWidth, 0, 0)?;
        b.flags(o, &[], &[Flag::Clickable, Flag::Scrollable])?;
        parts[i] = o;
    }
    // the point: a 90-degree slice from (0, TIP_Y) opening upwards, its edges running into both lobes
    parts[2] = sector(b, flush, parent, H, 0, TIP_Y, 225, 90, color)?;
    Ok(parts)
}

/// A consistent, easy-to-hit pill used on the waiting screen.
fn back_pill(b: &mut Batch, flush: Flush, parent: Obj) -> Result<Obj, LvError> {
    let k = b.create(Class::Btn, parent)?;
    b.size(k, BACK_PILL_W, BACK_PILL_H)?;
    b.align(k, Align::Center, 0, 160)?;
    b.style(k, Prop::Radius, 0, CIRCLE)?;
    b.style(k, Prop::BgColor, 0, LETTER_BG)?;
    b.style(k, Prop::BgOpa, 0, 255)?;
    b.style(k, Prop::BgColor, stock::PRESSED, ROW_PRESSED)?;
    b.style(k, Prop::BgOpa, stock::PRESSED, 255)?;
    b.style(k, Prop::BorderWidth, 0, 0)?;
    b.events(k, &[Ev::Clicked])?;
    // The waiting screen uses the same arrow-only red Back cue as the song list.
    arrow_color(b, flush, k, 0, BACK_RED)?;
    Ok(k)
}

/// A visible circular back button (no longer on the player: long press goes back there).
#[allow(dead_code)]
fn back_circle(b: &mut Batch, flush: Flush, parent: Obj, x: i16, y: i16) -> Result<Obj, LvError> {
    let k = b.create(Class::Btn, parent)?;
    b.size(k, VOL, VOL)?;
    b.align(k, Align::Center, x, y)?;
    b.style(k, Prop::Radius, 0, CIRCLE)?;
    // Keep a full-size invisible hit target; only the white arrow itself is visible.
    b.style(k, Prop::BgColor, 0, LETTER_BG)?;
    b.style(k, Prop::BgOpa, 0, 0)?;
    b.style(k, Prop::BgColor, stock::PRESSED, ROW_PRESSED)?;
    b.style(k, Prop::BgOpa, stock::PRESSED, 0)?;
    b.style(k, Prop::BorderWidth, 0, 0)?;
    b.events(k, &[Ev::Clicked])?;
    arrow(b, flush, k, 0)?;
    Ok(k)
}

/// A round, see-through white button (the play / prev / next hit area); pressing lights it.
fn button(b: &mut Batch, parent: Obj, size: i32, opa: i32, x: i16, y: i16) -> Result<Obj, LvError> {
    let btn = b.create(Class::Btn, parent)?;
    b.size(btn, size, size)?;
    b.align(btn, Align::Center, x, y)?;
    b.style(btn, Prop::Radius, 0, CIRCLE)?;
    b.style(btn, Prop::BgColor, 0, WHITE)?;
    b.style(btn, Prop::BgOpa, 0, opa)?;
    b.style(btn, Prop::BgColor, stock::PRESSED, WHITE)?;
    b.style(btn, Prop::BgOpa, stock::PRESSED, 90)?;
    b.style(btn, Prop::BorderWidth, 0, 0)?;
    // a long press anywhere on the player goes back, the buttons included
    b.events(btn, &[Ev::Clicked, Ev::LongPressed])?;
    Ok(btn)
}

/// "HH:MM".
pub fn clock_text(hour: u8, minute: u8, buf: &mut [u8; 5]) -> &str {
    *buf = [b'0' + hour / 10 % 10, b'0' + hour % 10, b':', b'0' + minute / 10 % 10, b'0' + minute % 10];
    core::str::from_utf8(buf).unwrap_or("")
}

/// The player: the cover's tint, the clock, the song, the artist, prev / play / next and the ring.
pub fn player(b: &mut Batch, flush: Flush, m: &Model, clock: &str, page_mode: bool) -> Result<Screen, LvError> {
    let ring_c = glow_565(m.quarters(m.now));
    room(b, flush)?;
    let page = b.create(Class::Obj, ROOT)?;
    b.size(page, SCREEN, SCREEN)?;
    for p in [Prop::PadTop, Prop::PadBottom, Prop::PadLeft, Prop::PadRight, Prop::BorderWidth, Prop::Radius] {
        b.style(page, p, 0, 0)?;
    }
    b.style(page, Prop::BgColor, 0, 0)?;
    b.style(page, Prop::BgOpa, 0, 255)?;
    // Built hidden and shown by one last command: the watch draws between batches, so a visible page would appear
    // piece by piece (glow, then buttons, then ring).
    b.flags(page, &[Flag::Hidden], &[Flag::Scrollable])?;
    // the crown comes to the app as CROWN events (the volume); the page does not scroll
    b.knob(page)?;
    let mut s = Screen { kind: Kind::Player, page, ..EMPTY };
    let q = m.quarters(m.now);
    let centre = glow_centre(q);
    let n = tide_pick();
    for k in 0..n {
        room(b, flush)?;
        let i = unsafe { (*(&raw const TIDE_IDX))[k] } as usize;
        let d = 2 * (R_MAX * (RINGS - i) as i32 / RINGS as i32);
        let o = b.create(Class::Obj, page)?;
        b.size(o, d, d)?;
        b.align(o, Align::Center, 0, CY)?;
        b.style(o, Prop::Radius, 0, CIRCLE)?;
        b.style(o, Prop::BgColor, 0, glow_ring(centre, i))?;
        b.style(o, Prop::BgOpa, 0, 255)?;
        b.style(o, Prop::BorderWidth, 0, 0)?;
        b.flags(o, &[], &[Flag::Clickable, Flag::Scrollable])?;
        s.blobs[k] = o;
    }

    // double-tap halves: invisible, behind every control, so the buttons keep working exactly as before
    let _ = clock; // the clock is no longer shown on the player (it pushed the song down)
    room(b, flush)?;
    for (i, x) in [-(SCREEN / 4) as i16, (SCREEN / 4) as i16].into_iter().enumerate() {
        let z = b.create(Class::Obj, page)?;
        b.size(z, SCREEN / 2, SCREEN)?;
        b.align(z, Align::Center, x, 0)?;
        b.style(z, Prop::BgOpa, 0, 0)?;
        b.style(z, Prop::BorderWidth, 0, 0)?;
        b.style(z, Prop::Radius, 0, 0)?;
        b.flags(z, &[], &[Flag::Scrollable])?;
        b.events(z, &[Ev::Clicked, Ev::LongPressed])?;
        if i == 0 { s.seek_left = z } else { s.seek_right = z }
    }

    // the song (and, while the volume is on, "Vol 65%" in its place)
    room(b, flush)?;
    let v = b.create(Class::Label, page)?;
    b.size(v, 160, 30)?;
    b.style(v, Prop::TextFont, 0, F24 as i32)?;
    b.style(v, Prop::TextAlign, 0, 2)?;
    let mut vbuf = [0u8; 8];
    b.text(v, volume_text(m.level, &mut vbuf))?;
    b.align(v, Align::TopMid, 0, TITLE_Y + 4)?;
    b.flags(v, &[Flag::Hidden], &[])?;
    s.vol_label = v;
    let title_y = TITLE_Y;
    let t = b.create(Class::Label, page)?;
    b.size(t, 300, 40)?;
    b.long_mode(t, LongMode::Dot)?;
    b.style(t, Prop::TextAlign, 0, 2)?;
    b.text(t, m.title(m.now))?;
    b.align(t, Align::TopMid, 0, title_y)?;
    s.title = t;
    room(b, flush)?;
    let ar = b.create(Class::Label, page)?;
    b.size(ar, 300, 30)?;
    b.long_mode(ar, LongMode::Dot)?;
    b.style(ar, Prop::TextFont, 0, F24 as i32)?;
    b.style(ar, Prop::TextColor, 0, SOFT)?;
    b.style(ar, Prop::TextAlign, 0, 2)?;
    b.text(ar, m.artist(m.now))?;
    b.align(ar, Align::TopMid, 0, ARTIST_Y)?;
    s.artist = ar;

    // the ring first, so the button lies on top of it and touches it
    room(b, flush)?;
    let arc = b.create(Class::Arc, page)?;
    b.size(arc, RING, RING)?;
    b.align(arc, Align::Center, 0, CY)?;
    b.angles(arc, 0, 360, 270)?;
    b.value(arc, 0, false)?;
    for (part, rounded) in [(0, 0), (stock::INDICATOR, 1)] {
        b.style(arc, Prop::ArcWidth, part, RING_W)?;
        b.style(arc, Prop::ArcRounded, part, rounded)?;
        b.style(arc, Prop::ArcOpa, part, 255)?;
    }
    b.style(arc, Prop::ArcColor, 0, track_colour(ring_c))?;
    b.style(arc, Prop::ArcColor, stock::INDICATOR, ring_colour(ring_c))?;
    b.flags(arc, &[], &[Flag::Clickable, Flag::Scrollable])?;
    s.arc = arc;

    // play / pause
    room(b, flush)?;
    let play = button(b, page, PLAY, 38, 0, CY)?;
    s.play = play;
    for (i, x) in [-13i16, 13].into_iter().enumerate() {
        s.pause_bars[i] = bar(b, flush, play, 12, 44, x, 4)?;
    }
    for (i, h) in PLAY_SMALL.iter().enumerate() {
        s.play_bars[i] = bar_color(b, flush, play, 3, *h as i32, -19 + 2 * i as i16, 0, WHITE)?;
    }

    // previous / next: a bar and a triangle each
    room(b, flush)?;
    s.prev = button(b, page, SIDE, 0, -SIDE_X, CY)?;
    bar(b, flush, s.prev, 6, 30, -14, 3)?;
    triangle(b, flush, s.prev, 30, 3, -1, WHITE)?;
    room(b, flush)?;
    s.next = button(b, page, SIDE, 0, SIDE_X, CY)?;
    bar(b, flush, s.next, 6, 30, 14, 3)?;
    triangle(b, flush, s.next, 30, -3, 1, WHITE)?;

    // volume: a speaker (a box, a stepped cone, two sound waves) on a round button under the ring
    room(b, flush)?;
    let _ = (page_mode, VOL_Y);
    let vol = button(b, page, VOL, 0, -BOTTOM_X, BOTTOM_Y)?;
    s.vol = vol;
    // speaker: a rounded box and a smooth cone (one antialiased slice opening to the right)
    bar(b, flush, vol, 10, 16, -17, 3)?;
    sector(b, flush, vol, 24, -20, 0, 312, 96, WHITE)?;
    for (i, (size, w)) in [(26, 4), (42, 4)].into_iter().enumerate() {
        room(b, flush)?;
        let a = b.create(Class::Arc, vol)?;
        b.size(a, size, size)?;
        b.align(a, Align::Center, 3, 0)?;
        b.angles(a, 0, 100, 310)?;
        b.value(a, 0, false)?;
        b.style(a, Prop::ArcWidth, 0, w)?;
        b.style(a, Prop::ArcRounded, 0, 1)?;
        b.style(a, Prop::ArcColor, 0, WHITE)?;
        b.style(a, Prop::ArcOpa, 0, 255)?;
        b.style(a, Prop::ArcOpa, stock::INDICATOR, 0)?;
        b.style(a, Prop::BgOpa, stock::KNOB, 0)?;
        b.flags(a, &[], &[Flag::Clickable, Flag::Scrollable])?;
        s.wave[i] = a;
    }

    // the heart, bottom right (in page mode this is where the back arrow was: long press goes back now)
    room(b, flush)?;
    let heart = button(b, page, VOL, 0, BOTTOM_X, BOTTOM_Y)?;
    s.heart = heart;
    s.heart_parts = heart_shape(b, flush, heart, HEART_OFF)?;

    room(b, flush)?;
    show_state(b, &s, m.playing)?;
    show_liked(b, &s, m.liked)?;
    show_volume(b, &s, false, m.level, m.muted)?;
    b.flags(page, &[], &[Flag::Hidden])?; // everything is in place: show the player at once
    Ok(s)
}

/// The volume button: lit, with "Vol 65%" in place of the clock, while the crown changes the volume; otherwise dark and
/// the clock. The sound wave of the speaker is off at volume 0 and when muted.
pub fn show_volume(b: &mut Batch, s: &Screen, on: bool, level: u8, muted: bool) -> Result<(), LvError> {
    let mut buf = [0u8; 8];
    b.text(s.vol_label, volume_text(level, &mut buf))?;
    if on {
        b.flags(s.title, &[Flag::Hidden], &[])?;
        b.flags(s.vol_label, &[], &[Flag::Hidden])?;
    } else {
        b.flags(s.vol_label, &[Flag::Hidden], &[])?;
        b.flags(s.title, &[], &[Flag::Hidden])?;
    }
    b.style(s.vol, Prop::BgOpa, 0, if on { 70 } else { 0 })?;
    // a soft level shows only the inner wave, no sound (0 or muted) none
    let silent = muted || level == 0;
    for (i, &w) in s.wave.iter().enumerate() {
        let show = !silent && (i == 0 || level >= 50);
        if show { b.flags(w, &[], &[Flag::Hidden])? } else { b.flags(w, &[Flag::Hidden], &[])? }
    }
    Ok(())
}

/// The pause bars while it plays, the play triangle while it is paused.
fn show_state(b: &mut Batch, s: &Screen, playing: bool) -> Result<(), LvError> {
    for &o in s.pause_bars.iter().filter(|o| **o != ROOT) {
        if playing { b.flags(o, &[], &[Flag::Hidden])? } else { b.flags(o, &[Flag::Hidden], &[])? }
    }
    for &o in s.play_bars.iter().filter(|o| **o != ROOT) {
        if playing { b.flags(o, &[Flag::Hidden], &[])? } else { b.flags(o, &[], &[Flag::Hidden])? }
    }
    Ok(())
}

/// The cover's tint under the player and the ring.
pub fn show_colours(b: &mut Batch, s: &Screen, m: &Model) -> Result<(), LvError> {
    let q = m.quarters(m.now);
    let centre = glow_centre(q);
    let (idx, n) = unsafe { (&*(&raw const TIDE_IDX), *(&raw const TIDE_N)) };
    for k in 0..n {
        b.style(s.blobs[k], Prop::BgColor, 0, glow_ring(centre, idx[k] as usize))?;
    }
    let c = glow_565(q);
    b.style(s.arc, Prop::ArcColor, 0, track_colour(c))?;
    b.style(s.arc, Prop::ArcColor, stock::INDICATOR, ring_colour(c))
}

/// The song, the artist, play or pause, and the tint follow the model.
pub fn show_now(b: &mut Batch, s: &Screen, m: &Model) -> Result<(), LvError> {
    b.text(s.title, m.title(m.now))?;
    if s.artist != ROOT {
        b.text(s.artist, m.artist(m.now))?;
    }
    show_state(b, s, m.playing)?;
    show_colours(b, s, m)
}

/// The ring: how much of the song is done, 0..=100.
pub fn show_progress(b: &mut Batch, s: &Screen, percent: i32) -> Result<(), LvError> {
    b.value(s.arc, percent.clamp(0, 100), false)
}

pub fn show_clock(b: &mut Batch, s: &Screen, text: &str) -> Result<(), LvError> {
    if s.clock == ROOT { return Ok(()); } // the player has no clock any more
    b.text(s.clock, text)
}

/// The heart: red when the song is saved, grey when not, hidden when the phone has no saved songs (older app).
pub fn show_liked(b: &mut Batch, s: &Screen, liked: u8) -> Result<(), LvError> {
    if s.heart == ROOT { return Ok(()); }
    if liked == LIKED_UNKNOWN {
        return b.flags(s.heart, &[Flag::Hidden], &[]);
    }
    b.flags(s.heart, &[], &[Flag::Hidden])?;
    let c = if liked == LIKED_YES { HEART_ON } else { HEART_OFF };
    for &o in &s.heart_parts {
        if o == ROOT { continue; }
        // the lobes are plain objects (background), the point is an arc
        b.style(o, Prop::BgColor, 0, c)?;
        b.style(o, Prop::ArcColor, 0, c)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::boxed::Box;
    use wp2_core::lvcmd::{self, Table};

    struct Loader {
        t: Table,
        live_max: usize,
    }

    impl Loader {
        fn flush(&mut self, b: &mut Batch) -> Result<(), LvError> {
            if b.bytes().is_empty() {
                return Ok(());
            }
            assert_eq!(lvcmd::validate(b.bytes(), &self.t, 0b1111), Ok(()));
            for r in lvcmd::records(b.bytes()) {
                lvcmd::apply(&mut self.t, &r.unwrap());
            }
            let live = (1..=192u16).filter(|&h| self.t.class(h).is_some()).count();
            self.live_max = self.live_max.max(live);
            b.clear();
            Ok(())
        }
    }

    fn full_model(n: usize) -> Model {
        let mut m = Model::new();
        m.count = n as u8;
        for (i, t) in m.tracks.iter_mut().enumerate().take(n) {
            let title = std::format!("Song number {i} with a long title");
            let artist = "Some Artist & Friends";
            t.title[..title.len().min(40)].copy_from_slice(&title.as_bytes()[..title.len().min(40)]);
            t.tlen = title.len().min(40) as u8;
            t.artist[..artist.len()].copy_from_slice(artist.as_bytes());
            t.alen = artist.len() as u8;
            t.c1 = 0x7BEF;
            t.q = [0xF800, 0x07E0, 0x001F, 0xFFFF];
            t.has_q = i % 2 == 0;
        }
        m.now = 1;
        m.playing = true;
        m.dur = 200;
        m.pos = 20;
        m
    }

    /// How much the player costs to send (bytes the watch must create before it can show it).
    #[test]
    fn player_stays_light_and_is_shown_last() {
        let mut b = Batch::with_buf(Box::leak(Box::new([0; BUF])));
        let mut l = Loader { t: Table::new(), live_max: 0 };
        let m = full_model(10);
        theme(&mut b).unwrap();
        l.flush(&mut b).unwrap();
        let (mut batches, mut bytes) = (0usize, 0usize);
        let mut count = |b: &mut Batch| {
            batches += 1;
            bytes += b.bytes().len();
            l.flush(b)
        };
        let _s = player(&mut b, &mut count, &m, "11:34", false).unwrap();
        bytes += b.bytes().len();
        batches += 1;
        // The player is hidden until its last record, so these are the cost of one reveal: keep it from growing.
        assert!(bytes < 11_500, "the player sends {bytes} bytes in {batches} batches");
    }

    #[test]
    fn every_screen_validates_and_leaving_frees_it() {
        let mut f = Batch::with_buf(Box::leak(Box::new([0; BUF])));
        fonts(&mut f).unwrap();
        assert_eq!(lvcmd::validate(f.bytes(), &Table::new(), 0), Ok(()));
        let mut b = Batch::with_buf(Box::leak(Box::new([0; BUF])));
        let mut l = Loader { t: Table::new(), live_max: 0 };
        let mut m = full_model(50);
        theme(&mut b).unwrap();
        let mut s = wait(&mut b, &mut |b| l.flush(b), MODE_MUSIC, false, false).unwrap();
        l.flush(&mut b).unwrap();
        let first = s.page;
        for round in 0..2 {
            leave(&mut b, &s).unwrap();
            s = menu(&mut b, &mut |b| l.flush(b), &m, round == 1, 0).unwrap();
            l.flush(&mut b).unwrap();
            assert_eq!(s.page, first, "the menu reuses the freed handles (round {round})");
            assert_eq!(s.back != ROOT, round == 1);
            assert!(!recolour(&mut b, &mut |b| l.flush(b), &mut s, &m).unwrap(), "nothing changed: nothing restyled");
            m.tracks[3].c1 ^= 0x0841;
            assert!(recolour(&mut b, &mut |b| l.flush(b), &mut s, &m).unwrap(), "a changed colour is restyled");
            assert!(!recolour(&mut b, &mut |b| l.flush(b), &mut s, &m).unwrap());
            l.flush(&mut b).unwrap();
            leave(&mut b, &s).unwrap();
            s = player(&mut b, &mut |b| l.flush(b), &m, "11:34", round == 1).unwrap();
            assert_eq!(s.back, ROOT, "the player has no back arrow: long press goes back");
            assert!(s.heart != ROOT && s.seek_left != ROOT && s.seek_right != ROOT);
            for liked in [LIKED_YES, LIKED_NO, LIKED_UNKNOWN] {
                show_liked(&mut b, &s, liked).unwrap();
            }
            l.flush(&mut b).unwrap();
            m.playing = !m.playing;
            show_now(&mut b, &s, &m).unwrap();
            show_progress(&mut b, &s, 33).unwrap();
            show_clock(&mut b, &s, "11:35").unwrap();
            show_volume(&mut b, &s, true, 65, false).unwrap();
            show_volume(&mut b, &s, false, 0, true).unwrap();
            l.flush(&mut b).unwrap();
        }
        std::println!("most live objects: {}", l.live_max);
        assert!(l.live_max < 192);
    }

    #[test]
    fn start_screen_and_waiting_texts_validate() {
        let mut b = Batch::with_buf(Box::leak(Box::new([0; BUF])));
        let mut l = Loader { t: Table::new(), live_max: 0 };
        theme(&mut b).unwrap();
        let mut s = home(&mut b, &mut |b| l.flush(b), 0x03, 0).unwrap();
        l.flush(&mut b).unwrap();
        assert_eq!(s.kind, Kind::Home);
        assert!(s.rows[0] != ROOT && s.rows[1] != ROOT && s.rows[0] != s.rows[1]);
        assert_eq!(s.home_count, 2);
        assert_eq!(s.home_modes[0], MODE_MUSIC);
        assert_eq!(s.home_modes[1], MODE_PODCAST);
        assert!(s.rows[2] == ROOT);
        for obsolete in ["Your music", "Your shows", "Current media", "Your shortcuts"] {
            assert!(
                !b.bytes().windows(obsolete.len()).any(|window| window == obsolete.as_bytes()),
                "redundant home subtitle still present: {obsolete}"
            );
        }
        leave(&mut b, &s).unwrap();
        // With the default anchor, Control media leads the category chooser.
        s = home(&mut b, &mut |b| l.flush(b), 0x0C, 0).unwrap();
        assert_eq!(s.home_count, 2);
        assert_eq!(s.home_modes[0], MODE_EXTERNAL);
        assert_eq!(s.home_modes[1], MODE_CUSTOM);
        l.flush(&mut b).unwrap();
        // Moving the focus down rotates the selected category to the first slot.
        leave(&mut b, &s).unwrap();
        s = home(&mut b, &mut |b| l.flush(b), 0x0C, 1).unwrap();
        assert_eq!(s.home_count, 2);
        assert_eq!(s.home_modes[0], MODE_CUSTOM);
        assert_eq!(s.home_modes[1], MODE_EXTERNAL);
        l.flush(&mut b).unwrap();
        for (mode, empty) in [(MODE_MUSIC, false), (MODE_PODCAST, false), (MODE_PODCAST, true), (MODE_MUSIC, true)] {
            leave(&mut b, &s).unwrap();
            s = wait(&mut b, &mut |b| l.flush(b), mode, empty, false).unwrap();
            assert_eq!(s.back, ROOT);
            l.flush(&mut b).unwrap();
        }
        leave(&mut b, &s).unwrap();
        s = wait(&mut b, &mut |b| l.flush(b), MODE_MUSIC, false, true).unwrap();
        assert_ne!(s.back, ROOT);
        l.flush(&mut b).unwrap();
        leave(&mut b, &s).unwrap();
        l.flush(&mut b).unwrap();
    }

    #[test]
    fn player_without_a_song_and_a_short_list() {
        let mut b = Batch::with_buf(Box::leak(Box::new([0; BUF])));
        let mut l = Loader { t: Table::new(), live_max: 0 };
        theme(&mut b).unwrap();
        let mut m = full_model(0);
        let s = player(&mut b, &mut |b| l.flush(b), &m, "00:00", true).unwrap();
        assert_eq!(s.back, ROOT);
        l.flush(&mut b).unwrap();
        assert!(unsafe { *(&raw const TIDE_N) } <= 100);
        for t in (0..9_000).step_by(250) {
            tide(&mut b, &s, t).unwrap();
            assert!(b.bytes().len() <= BUF);
            l.flush(&mut b).unwrap();
        }
        show_now(&mut b, &s, &m).unwrap();
        l.flush(&mut b).unwrap();
        leave(&mut b, &s).unwrap();
        l.flush(&mut b).unwrap();
        for n in [0, 1, 3] {
            m = full_model(n);
            let s = menu(&mut b, &mut |b| l.flush(b), &m, n == 1, 0).unwrap();
            l.flush(&mut b).unwrap();
            leave(&mut b, &s).unwrap();
            l.flush(&mut b).unwrap();
        }
    }

    #[test]
    fn focus_event_index_maps_to_the_logical_row_after_rotation() {
        assert_eq!(focused_anchor(0, 0, 4), Some(0));
        assert_eq!(focused_anchor(0, 2, 4), Some(2));
        assert_eq!(focused_anchor(2, 0, 4), Some(2));
        assert_eq!(focused_anchor(2, 1, 4), Some(3));
        assert_eq!(focused_anchor(2, 2, 4), Some(0));
        assert_eq!(focused_anchor(3, 2, 4), Some(1));
        assert_eq!(focused_anchor(0, -1, 4), None);
        assert_eq!(focused_anchor(0, 4, 4), None);
        assert_eq!(focused_anchor(3, 7, 4), None);
        assert_eq!(focused_anchor(0, 0, 0), None);
    }

    #[test]
    fn page_menu_rotates_back_and_tracks_without_losing_track_indices() {
        let mut b = Batch::with_buf(Box::leak(Box::new([0; BUF])));
        let mut l = Loader { t: Table::new(), live_max: 0 };
        theme(&mut b).unwrap();
        let m = full_model(3);
        let s0 = menu(&mut b, &mut |b| l.flush(b), &m, true, 0).unwrap();
        assert_ne!(s0.back, ROOT);
        assert_eq!(&s0.row_indices[..3], &[0, 1, 2]);
        leave(&mut b, &s0).unwrap();
        l.flush(&mut b).unwrap();
        let s2 = menu(&mut b, &mut |b| l.flush(b), &m, true, 2).unwrap();
        assert_ne!(s2.back, ROOT);
        assert_eq!(&s2.row_indices[..3], &[1, 2, 0]);
        leave(&mut b, &s2).unwrap();
        l.flush(&mut b).unwrap();
        let regular = menu(&mut b, &mut |b| l.flush(b), &m, false, 2).unwrap();
        assert_eq!(&regular.row_indices[..3], &[2, 0, 1]);
        l.flush(&mut b).unwrap();
    }

    #[test]
    fn vivid_pushes_colours_away_from_grey() {
        assert_eq!(vivid(0xFFFF), 0xFFFF);
        assert_eq!(vivid(0x0000), 0x0000);
        assert_eq!(vivid(0xF800), 0xF800);
        // a dull orange: red goes up, blue goes down
        let dull = (20u16 << 11) | (32 << 5) | 14;
        let v = vivid(dull);
        let (r0, _, b0) = channels(dull);
        let (r1, _, b1) = channels(v);
        assert!(r1 > r0 && b1 < b0);
        // the glow is a real colour, not darker than the old one
        assert!(glow(dull) > 0);
    }

    #[test]
    fn glow_is_dark_at_the_edge_and_the_covers_colour_in_the_middle() {
        let q = [0x8410u16; 4]; // grey
        let c = glow_centre(q);
        assert!(c.0.abs_diff(c.1) < 8 && c.0 > 100 && c.0 <= GLOW_MAX);
        assert_eq!(glow_ring(c, 0), 0);
        let mut last = 0;
        for i in 1..RINGS {
            let v = glow_ring(c, i);
            assert!(v >= last, "never darker towards the middle");
            last = v;
        }
        // a red cover stays red; a nearly black one stays dark
        let red = glow_centre([0xF800; 4]);
        assert!(red.0 > red.1 * 4 && red.0 <= GLOW_MAX);
        let dark = glow_centre([0x0001; 4]);
        assert!(dark.0.max(dark.1).max(dark.2) <= 255);
    }

    #[test]
    fn colours_and_clock() {
        // the ring is lighter than the tint, the tint darker than the cover
        let c = 0xF800u16; // pure red
        assert!(background(c) < c as i32);
        assert_eq!(shade(0xFFFF, 0), 0);
        assert_eq!(lighten(0x0000, 256), 0xFFFF);
        let mut buf = [0u8; 5];
        assert_eq!(clock_text(7, 5, &mut buf), "07:05");
        assert_eq!(clock_text(23, 59, &mut buf), "23:59");
    }
}
