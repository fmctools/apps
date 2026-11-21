//! The screens, from the watch's own LVGL parts and stock fonts (`cmf_wp2_app::stock`):
//! - in the menu app, three pages in the stock vertical pager: now (place, temperature, condition, high and low,
//!   wind and humidity), the next hours, the next days with a range bar each;
//! - on the page next to the watch face, the now screen alone;
//! - before the first answer, one line saying what is missing.
//!
//! LVGL here has no images, so a condition icon is two to four plain rounded objects: a disc for the sun, a pill
//! and a bump for a cloud, short pills for rain. Each screen is one root under `ROOT`; leaving it deletes the root
//! and everything in it.

use crate::model::{self, Icon, Line, Model, Status};
use cmf_wp2_app::lv::*;
use cmf_wp2_app::stock::{self, BLACK, CIRCLE, GREY, Pager, SCREEN, WHITE};
use core::fmt::Write;

/// Font slots: the system font is 0.
pub const F24: u8 = 1;
pub const F48: u8 = 2;

const SUN: i32 = 0xFE60; // #FFCC00
const MOON: i32 = 0xE73D; // #E5E5EA
const CLOUD: i32 = 0xD69A; // #D1D1D6
const STORM: i32 = 0x8C72; // #8E8E93
const WET: i32 = 0x669F; // #64D2FF
const TRACK: i32 = 0x39C7; // #3A3A3C

const MSG_W: i32 = 300;
/// The top of the now screen's content: the place; the rest hangs below it.
const NOW_Y: i32 = 92;
/// The first row of the hours and days pages, and their rows' height.
const ROWS_Y: i32 = 96;
const HOUR_H: i32 = 58;
const DAY_H: i32 = 54;
/// The range bar's track.
const BAR_W: i16 = 70;
const BAR_H: i16 = 8;

// bytes one row with its icon can take: flush first when fewer are left
const GROUP: usize = 760;

pub type Flush<'a> = dyn FnMut(&mut Batch) -> Result<(), LvError> + 'a;

/// Flushes unless a group of records still fits: called before each one, and none is longer than GROUP (the
/// tests build the views from every fill level).
fn room(b: &mut Batch, flush: &mut Flush) -> Result<(), LvError> {
    if b.bytes().len() + GROUP > BUF { flush(b) } else { Ok(()) }
}

/// The fonts, in a batch of their own.
pub fn fonts(b: &mut Batch) -> Result<(), LvError> {
    stock::font(b)?;
    b.font(F24, StockFont::F24M)?;
    b.font(F48, StockFont::F48M)
}

/// Black behind everything, the system font and white text: first, once.
pub fn theme(b: &mut Batch) -> Result<(), LvError> {
    stock::theme(b)?;
    b.style(ROOT, Prop::BgColor, 0, BLACK)
}

/// What to say instead of the weather while there is none to show.
pub fn message(m: &Model) -> &'static str {
    match m.status {
        Status::Ask => "Allow Weather in FMC Companion on your phone",
        Status::Denied => "Turn Weather on in FMC Companion",
        Status::Unavailable => "No weather right now",
        _ if m.silent() => "Open FMC Companion on your phone",
        _ => "Asking your phone...",
    }
}

/// A line under the weather from the cache, when the phone isn't giving a fresh one.
pub fn note(m: &Model, wall: i32) -> Option<Line> {
    let fixed = match m.status {
        Status::Ask => Some("Allow Weather in FMC Companion"),
        Status::Denied => Some("Weather is off in FMC Companion"),
        _ if m.silent() => Some("Phone not reachable"),
        _ => None,
    };
    match fixed {
        Some(t) => {
            let mut l = Line::new();
            let _ = l.write_str(t);
            Some(l)
        }
        None => model::updated(m.stale_min(wall)),
    }
}

/// The range bar's colour for a day, by the middle of its range: cold blue to hot red.
pub fn bar_color(min: i8, max: i8) -> i32 {
    match (min as i32 + max as i32).div_euclid(2) {
        ..0 => 0x351F,    // #30A0FF
        ..10 => 0x5EDC,   // #5AD8E6
        ..18 => 0x9F09,   // #9BE04A
        ..25 => SUN,
        ..32 => 0xFCA0,   // #FF9500
        _ => 0xFA27,      // #FF453A
    }
}

fn plain(b: &mut Batch, o: Obj) -> Result<(), LvError> {
    for p in [Prop::PadTop, Prop::PadBottom, Prop::PadLeft, Prop::PadRight, Prop::BorderWidth, Prop::BgOpa, Prop::Radius] {
        b.style(o, p, 0, 0)?;
    }
    Ok(())
}

/// A full-screen root, hidden until the screen is built.
fn root(b: &mut Batch) -> Result<Obj, LvError> {
    let r = b.create(Class::Obj, ROOT)?;
    b.size(r, SCREEN, SCREEN)?;
    plain(b, r)?;
    b.flags(r, &[Flag::Hidden], &[Flag::Scrollable])?;
    Ok(r)
}

/// A label `w` wide whose centre is `cx` right of the parent's and whose top is at `y`; `align` 1 left, 2 centre,
/// 3 right.
fn label(b: &mut Batch, parent: Obj, text: &str, w: i32, cx: i32, y: i32, align: i32) -> Result<Obj, LvError> {
    let l = b.create(Class::Label, parent)?;
    b.size(l, w, CONTENT)?;
    b.style(l, Prop::TextAlign, 0, align)?;
    b.text(l, text)?;
    b.align(l, Align::TopMid, cx as i16, y as i16)?;
    Ok(l)
}

fn small(b: &mut Batch, l: Obj, color: i32) -> Result<(), LvError> {
    b.style(l, Prop::TextFont, 0, F24 as i32)?;
    b.style(l, Prop::TextColor, 0, color)
}

/// A filled rounded shape, centre `cx` right of the parent's, top at `y`.
fn shape(b: &mut Batch, parent: Obj, cx: i32, y: i32, w: i32, h: i32, color: i32) -> Result<Obj, LvError> {
    let o = b.create(Class::Obj, parent)?;
    b.size(o, w, h)?;
    b.style(o, Prop::Radius, 0, CIRCLE)?;
    b.style(o, Prop::BorderWidth, 0, 0)?;
    b.style(o, Prop::BgColor, 0, color)?;
    b.style(o, Prop::BgOpa, 0, 255)?;
    b.flags(o, &[], &[Flag::Clickable, Flag::Scrollable])?;
    b.align(o, Align::TopMid, cx as i16, y as i16)?;
    Ok(o)
}

/// A condition icon in a box 48 wide and 40 high at scale `s`/40, centred at `cx`, top at `y`: at most four
/// objects. It sits on black: the moon's bite is a black disc.
pub fn icon(b: &mut Batch, parent: Obj, kind: Icon, night: bool, cx: i32, y: i32, s: i32) -> Result<(), LvError> {
    let u = |n: i32| n * s / 40;
    let disc = |b: &mut Batch, dx: i32, dy: i32, d: i32, color: i32| shape(b, parent, cx + u(dx), y + u(dy), u(d), u(d), color).map(|_| ());
    // a cloud whose top is `dy` down the box
    let cloud = |b: &mut Batch, dy: i32, color: i32| -> Result<(), LvError> {
        shape(b, parent, cx - u(5), y + u(dy), u(22), u(22), color)?;
        shape(b, parent, cx, y + u(dy + 10), u(40), u(18), color).map(|_| ())
    };
    // what falls from a cloud at the top of the box
    let fall = |b: &mut Batch, w: i32, h: i32, color: i32| -> Result<(), LvError> {
        shape(b, parent, cx - u(8), y + u(31), u(w), u(h), color)?;
        shape(b, parent, cx + u(8), y + u(31), u(w), u(h), color).map(|_| ())
    };
    match kind {
        Icon::Clear if night => {
            disc(b, 0, 7, 26, MOON)?;
            disc(b, 8, 3, 22, BLACK)
        }
        Icon::Clear => disc(b, 0, 6, 28, SUN),
        Icon::Partly => {
            disc(b, 10, 2, 20, if night { MOON } else { SUN })?;
            cloud(b, 8, CLOUD)
        }
        Icon::Cloud => cloud(b, 6, CLOUD),
        Icon::Fog => {
            for dy in [8, 18, 28] {
                shape(b, parent, cx, y + u(dy), u(40), u(5), STORM)?;
            }
            Ok(())
        }
        Icon::Drizzle => {
            cloud(b, 2, CLOUD)?;
            fall(b, 4, 5, WET)
        }
        Icon::Rain => {
            cloud(b, 2, CLOUD)?;
            fall(b, 4, 9, WET)
        }
        Icon::Snow => {
            cloud(b, 2, CLOUD)?;
            fall(b, 6, 6, WHITE)
        }
        Icon::Storm => {
            cloud(b, 2, STORM)?;
            shape(b, parent, cx, y + u(30), u(6), u(10), SUN).map(|_| ())
        }
    }
}

/// The now screen's content, into a full-screen `parent`.
fn now_content(b: &mut Batch, flush: &mut Flush, parent: Obj, m: &Model, wall: i32) -> Result<(), LvError> {
    let Some(n) = m.now else { return Ok(()) };
    room(b, flush)?;
    if !m.place().is_empty() {
        let l = label(b, parent, m.place(), MSG_W, 0, NOW_Y, 2)?;
        b.long_mode(l, LongMode::Dot)?;
        small(b, l, GREY)?;
    }
    // the temperature and, since the fonts have no degree sign, a ring after it: a row as wide as the two, moved
    // right by half the ring so the number stays in the middle
    room(b, flush)?;
    let row = b.create(Class::Obj, parent)?;
    b.size(row, CONTENT, CONTENT)?;
    plain(b, row)?;
    b.flags(row, &[], &[Flag::Clickable, Flag::Scrollable])?;
    b.flex(row, Flow::Row, FlexAlign::Start, FlexAlign::Start, FlexAlign::Start)?;
    b.style(row, Prop::PadColumn, 0, 4)?;
    b.align(row, Align::TopMid, 9, (NOW_Y + 36) as i16)?;
    let t = b.create(Class::Label, row)?;
    b.text(t, model::temp_text(model::deg(n.temp)).as_str())?;
    b.style(t, Prop::TextFont, 0, F48 as i32)?;
    let ring = b.create(Class::Obj, row)?;
    b.size(ring, 14, 14)?;
    b.style(ring, Prop::Radius, 0, CIRCLE)?;
    b.style(ring, Prop::BgOpa, 0, 0)?;
    b.style(ring, Prop::BorderWidth, 0, 3)?;
    b.style(ring, Prop::BorderColor, 0, WHITE)?;
    b.style(ring, Prop::TranslateY, 0, 8)?;
    b.flags(ring, &[], &[Flag::Clickable, Flag::Scrollable])?;
    // night for the icon: what the current hour says, when there are hours
    let night = m.hour_at(0, wall).is_some_and(|(_, is_now, h)| is_now && h.night);
    room(b, flush)?;
    icon(b, parent, model::icon(n.code), night, 0, NOW_Y + 98, 52)?;
    room(b, flush)?;
    label(b, parent, model::condition(n.code), MSG_W, 0, NOW_Y + 158, 2)?;
    let mut y = NOW_Y + 204;
    if let Some(d) = m.today(wall) {
        let mut l = Line::new();
        let _ = write!(l, "H:{} L:{}", model::temp_text(d.max as i16).as_str(), model::temp_text(d.min as i16).as_str());
        let h = label(b, parent, l.as_str(), MSG_W, 0, y, 2)?;
        small(b, h, WHITE)?;
        y += 36;
    }
    let mut l = Line::new();
    let _ = write!(l, "{} m/s   {}%", n.wind, n.humidity);
    let w = label(b, parent, l.as_str(), MSG_W, 0, y, 2)?;
    small(b, w, GREY)?;
    if let Some(t) = note(m, wall) {
        let l = label(b, parent, t.as_str(), 280, 0, y + 36, 2)?;
        small(b, l, GREY)?;
        b.long_mode(l, LongMode::Wrap)?;
    }
    Ok(())
}

/// A grey line in the middle of a page that has nothing else.
fn empty(b: &mut Batch, parent: Obj, text: &str) -> Result<(), LvError> {
    let l = label(b, parent, text, MSG_W, 0, 180, 2)?;
    b.style(l, Prop::TextColor, 0, GREY)?;
    b.long_mode(l, LongMode::Wrap)
}

/// What the hours page says without hours.
pub fn no_hours_text(m: &Model) -> &'static str {
    if m.no_hours { "Update FMC Companion for the hourly forecast" } else { "No hours to show yet" }
}

fn hours_content(b: &mut Batch, flush: &mut Flush, page: Obj, m: &Model, wall: i32) -> Result<(), LvError> {
    let n = m.hours_len(wall).min(model::ASK_HOURS as usize);
    if n == 0 {
        room(b, flush)?;
        return empty(b, page, no_hours_text(m));
    }
    for k in 0..n {
        let Some((hod, is_now, h)) = m.hour_at(k, wall) else { break };
        room(b, flush)?;
        let y = ROWS_Y + k as i32 * HOUR_H;
        let mut l = Line::new();
        let _ = if is_now { l.write_str("Now") } else { write!(l, "{hod:02}") };
        let t = label(b, page, l.as_str(), 80, -110, y + 8, 1)?;
        if !is_now {
            b.style(t, Prop::TextColor, 0, GREY)?;
        }
        icon(b, page, model::icon(h.code), h.night, -22, y + 6, 40)?;
        if h.precip >= 30 {
            let mut l = Line::new();
            let _ = write!(l, "{}%", h.precip);
            let p = label(b, page, l.as_str(), 64, 42, y + 14, 2)?;
            small(b, p, WET)?;
        }
        label(b, page, model::temp_text(h.temp as i16).as_str(), 80, 110, y + 8, 3)?;
    }
    b.style(page, Prop::PadBottom, 0, 130) // the last row can scroll up clear of the round edge
}

fn days_content(b: &mut Batch, flush: &mut Flush, page: Obj, m: &Model, wall: i32) -> Result<(), LvError> {
    let n = m.days_len(wall);
    if n == 0 {
        room(b, flush)?;
        return empty(b, page, "No forecast to show yet");
    }
    let (lo, hi) = m.range(wall);
    for k in 0..n {
        let Some((abs, today, d)) = m.day_at(k, wall) else { break };
        room(b, flush)?;
        let y = ROWS_Y + k as i32 * DAY_H;
        label(b, page, if today { "Today" } else { model::weekday(abs) }, 96, -112, y + 6, 1)?;
        icon(b, page, model::icon(d.code), false, -34, y + 4, 40)?;
        let lo_l = label(b, page, model::temp_text(d.min as i16).as_str(), 56, 6, y + 12, 3)?;
        small(b, lo_l, GREY)?;
        room(b, flush)?;
        let track = shape(b, page, 73, y + 22, BAR_W as i32, BAR_H as i32, TRACK)?;
        b.style(track, Prop::PadLeft, 0, 0)?;
        let (x, len) = model::bar(d.min, d.max, lo, hi, BAR_W, BAR_H);
        let seg = b.create(Class::Obj, track)?;
        b.size(seg, len as i32, BAR_H as i32)?;
        b.style(seg, Prop::Radius, 0, CIRCLE)?;
        b.style(seg, Prop::BorderWidth, 0, 0)?;
        b.style(seg, Prop::BgColor, 0, bar_color(d.min, d.max))?;
        b.style(seg, Prop::BgOpa, 0, 255)?;
        b.flags(seg, &[], &[Flag::Clickable, Flag::Scrollable])?;
        b.align(seg, Align::LeftMid, x, 0)?;
        let hi_l = label(b, page, model::temp_text(d.max as i16).as_str(), 56, 138, y + 12, 3)?;
        small(b, hi_l, WHITE)?;
    }
    b.style(page, Prop::PadBottom, 0, 130)
}

/// A screen: its root, and the pager when it has pages.
#[derive(Clone, Copy)]
pub struct View {
    pub root: Obj,
    pub pager: Option<Pager<3>>,
    /// The stock round scroll bar of each page that scrolls (hours, days); `ROOT` for the rest.
    pub bars: [Obj; 3],
}


/// One line in the middle of the screen.
pub fn message_view(b: &mut Batch, flush: &mut Flush, text: &str) -> Result<View, LvError> {
    room(b, flush)?;
    let r = root(b)?;
    let l = label(b, r, text, MSG_W, 0, 170, 2)?;
    b.style(l, Prop::TextColor, 0, GREY)?;
    b.long_mode(l, LongMode::Wrap)?;
    b.flags(r, &[], &[Flag::Hidden])?;
    Ok(View { root: r, pager: None, bars: [ROOT; 3] })
}

/// The menu app: now, hours, days, opened on `page`.
pub fn pages_view(b: &mut Batch, flush: &mut Flush, m: &Model, wall: i32, page: usize) -> Result<View, LvError> {
    if m.now.is_none() {
        return message_view(b, flush, message(m));
    }
    room(b, flush)?;
    let mut p = stock::pager::<3>(b, 3)?;
    b.flags(p.root, &[Flag::Hidden], &[])?;
    now_content(b, flush, p.pages[0], m, wall)?;
    hours_content(b, flush, p.pages[1], m, wall)?;
    days_content(b, flush, p.pages[2], m, wall)?;
    room(b, flush)?;
    let mut bars = [ROOT; 3];
    for i in 1..3 {
        bars[i] = b.scroll_bar(p.pages[i])?;
    }
    // a scroll bar takes the crown for its list, which then never pages on at its end: the pager keeps it, scrolls
    // the page first and pages at its end, and the bar shows while the page scrolls
    b.knob(p.obj)?;
    let page = page.min(2);
    if page > 0 {
        p.jump(b, page)?;
    }
    b.flags(p.root, &[], &[Flag::Hidden])?;
    Ok(View { root: p.root, pager: Some(p), bars })
}

/// The page next to the watch face: the now screen alone.
pub fn face_view(b: &mut Batch, flush: &mut Flush, m: &Model, wall: i32) -> Result<View, LvError> {
    if m.now.is_none() {
        return message_view(b, flush, message(m));
    }
    room(b, flush)?;
    let r = root(b)?;
    now_content(b, flush, r, m, wall)?;
    b.flags(r, &[], &[Flag::Hidden])?;
    Ok(View { root: r, pager: None, bars: [ROOT; 3] })
}

/// Deletes a screen.
pub fn leave(b: &mut Batch, root: Obj) -> Result<(), LvError> {
    if root != ROOT { b.del(root) } else { Ok(()) }
}

#[cfg(test)]
#[path = "screen_tests.rs"]
mod tests;
