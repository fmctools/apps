//! Native LVGL list, stock fonts, curved stock scrollbar. Cards are reserved for actions.
use crate::{
    inventory::{Category, Inventory},
    model::{self, Rect, Text},
};
use cmf_wp2_app::{
    Event,
    lv::*,
    stock,
    storage::{Filesystem, Partition},
};
use core::fmt::Write;
const SURFACE: i32 = 0x2966; // Stock WP2 Settings card reference: (39,42,47) at full brightness
const SECONDARY: i32 = 0x8c51; // sampled WP3 #8E8B8E
// Semantic chart colors; surfaces and text retain the native watch theme.
const CHART_COLORS: [i32; 5] = [0xda67, 0xe4c6, 0xf649, 0xbe18, 0x2966];
pub const BACK: u16 = 250;
pub const REFRESH: u16 = 251;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Page {
    Home,
    Apps,
    Faces(u8),
    Other(u8),
    System,
    Nand,
    Nor,
    App(u8),
    File(u8),
    Partition(u8),
    Accounting,
}
pub struct Screen {
    pub root: Obj,
    bar: Obj,
    pub page: Page,
    pub complete: bool,
    buttons: [(Obj, u16); 32],
    n: usize,
}
impl Screen {
    pub fn action(&self, e: &Event) -> u16 {
        if e.kind != cmf_wp2_app::LV || e.y != 7 {
            return 0;
        }
        self.buttons[..self.n]
            .iter()
            .find(|(o, _)| o.0 == e.x as u16)
            .map_or(0, |(_, a)| *a)
    }
    pub fn visible(&self, b: &mut Batch, on: bool) -> Result<(), LvError> {
        for o in [self.root, self.bar] {
            if on {
                b.flags(o, &[], &[Flag::Hidden])?;
            } else {
                b.flags(o, &[Flag::Hidden], &[])?;
            }
        }
        if on {
            b.knob(self.root)?;
        }
        Ok(())
    }
    fn bind(&mut self, b: &mut Batch, o: Obj, action: u16) -> Result<(), LvError> {
        b.events(o, &[Ev::Clicked])?;
        self.buttons[self.n] = (o, action);
        self.n += 1;
        Ok(())
    }
}
fn label(
    b: &mut Batch,
    p: Obj,
    text: &str,
    r: Rect,
    font: i32,
    color: i32,
    align: i32,
    wrap: bool,
) -> Result<Obj, LvError> {
    let o = b.create(Class::Label, p)?;
    b.size(o, r.w as i32, r.h as i32)?;
    b.align(o, Align::TopLeft, r.x, r.y)?;
    b.text(o, text)?;
    b.style(o, Prop::TextFont, 0, font)?;
    b.style(o, Prop::TextColor, 0, color)?;
    b.style(o, Prop::TextAlign, 0, align)?;
    b.long_mode(o, if wrap { LongMode::Wrap } else { LongMode::Dot })?;
    b.flags(o, &[], &[Flag::Clickable, Flag::Scrollable])?;
    Ok(o)
}
fn block(b: &mut Batch, p: Obj, h: i32) -> Result<Obj, LvError> {
    let o = b.create(Class::Obj, p)?;
    b.size(o, 418, h)?;
    for prop in [
        Prop::PadTop,
        Prop::PadBottom,
        Prop::PadLeft,
        Prop::PadRight,
        Prop::BorderWidth,
        Prop::BgOpa,
    ] {
        b.style(o, prop, 0, 0)?;
    }
    b.flags(o, &[], &[Flag::Scrollable, Flag::Clickable])?;
    Ok(o)
}
fn heading(b: &mut Batch, p: Obj, text: &str, accent: bool) -> Result<(), LvError> {
    let o = b.create(Class::Label, p)?;
    b.size(o, 418, 42)?;
    b.text(o, text)?;
    b.style(o, Prop::TextAlign, 0, 2)?;
    b.style(o, Prop::TextFont, 0, 0)?;
    b.style(
        o,
        Prop::TextColor,
        0,
        if accent { stock::ACCENT } else { stock::WHITE },
    )?;
    b.long_mode(o, LongMode::Dot)?;
    Ok(())
}
fn note(b: &mut Batch, lv: &Lv, p: Obj, text: &str) -> Result<(), LvError> {
    let lines = ((text.len() + 25) / 26).max(1) as i32;
    let o = block(b, p, lines * 32 + 20)?;
    label(
        b,
        o,
        text,
        Rect::new(40, 6, 338, (lines * 32 + 8) as i16),
        1,
        SECONDARY,
        0,
        true,
    )?;
    b.submit(lv)
}
// Category pictograms follow the filled stock Settings vocabulary. Storage/folder symbols
// are app-specific LVGL geometry; they are not claimed to be extracted stock artwork.
#[derive(Clone, Copy)]
enum Icon {
    Apps,
    Face,
    Folder,
    Chip,
    Drive,
    Usage,
    App(char),
    Installed(u8),
    Document,
}
fn shape(
    b: &mut Batch,
    p: Obj,
    x: i16,
    y: i16,
    w: i16,
    h: i16,
    color: i32,
    radius: i32,
) -> Result<Obj, LvError> {
    let o = b.create(Class::Obj, p)?;
    b.size(o, w as i32, h as i32)?;
    b.align(o, Align::TopLeft, x, y)?;
    for prop in [
        Prop::PadTop,
        Prop::PadBottom,
        Prop::PadLeft,
        Prop::PadRight,
        Prop::BorderWidth,
    ] {
        b.style(o, prop, 0, 0)?;
    }
    b.style(o, Prop::BgColor, 0, color)?;
    b.style(o, Prop::BgOpa, 0, 255)?;
    b.style(o, Prop::Radius, 0, radius)?;
    b.flags(o, &[], &[Flag::Clickable, Flag::Scrollable])?;
    Ok(o)
}
/// Square internal ends, two-pixel separators, and rounded outside ends only.
fn chart(b: &mut Batch, p: Obj, x: i16, y: i16, width: u16, values: &[u32]) -> Result<(), LvError> {
    let widths = model::separated_widths(values, u32::from(width));
    let first = widths.iter().position(|&w| w > 0).unwrap_or(0);
    let last = widths.iter().rposition(|&w| w > 0).unwrap_or(0);
    let mut cursor = x;
    for i in 0..5 {
        let w = widths[i] as i16;
        if w == 0 {
            continue;
        }
        let radius = if i == first || i == last {
            6.min(w as i32 / 2)
        } else {
            0
        };
        shape(b, p, cursor, y, w, 24, CHART_COLORS[i], radius)?;
        if first != last && radius > 0 {
            let r = radius as i16;
            if i == first {
                shape(b, p, cursor + w - r, y, r, 24, CHART_COLORS[i], 0)?;
            }
            if i == last {
                shape(b, p, cursor, y, r, 24, CHART_COLORS[i], 0)?;
            }
        }
        cursor += w + 2;
    }
    Ok(())
}
fn icon(b: &mut Batch, lv: &Lv, p: Obj, kind: Icon, x: i16, y: i16) -> Result<(), LvError> {
    b.submit(lv)?;
    match kind {
        Icon::Installed(slot) => {
            let o = shape(b, p, 16, 16, 96, 96, SURFACE, 0)?;
            b.style(o, Prop::BgOpa, 0, 0)?;
            b.installed_app_icon(o, slot, IconSize::Native)?;
        }
        Icon::Apps => {
            for row in 0..3 {
                for col in 0..3 {
                    shape(
                        b,
                        p,
                        x + 4 + col * 14,
                        y + 4 + row * 14,
                        8,
                        8,
                        stock::WHITE,
                        4,
                    )?;
                }
                b.submit(lv)?;
            }
        }
        Icon::Face => {
            shape(b, p, x + 13, y - 3, 18, 50, stock::WHITE, 4)?;
            shape(b, p, x + 4, y + 5, 36, 36, stock::WHITE, 18)?;
            shape(b, p, x + 9, y + 10, 26, 26, SURFACE, 13)?;
            shape(b, p, x + 20, y + 14, 3, 12, stock::WHITE, 1)?;
            shape(b, p, x + 20, y + 24, 10, 3, stock::ACCENT, 1)?;
        }
        Icon::Folder => {
            shape(b, p, x + 1, y + 6, 20, 12, stock::WHITE, 4)?;
            shape(b, p, x + 1, y + 13, 42, 28, stock::WHITE, 5)?;
        }
        Icon::Document => {
            shape(b, p, x + 7, y + 2, 30, 40, stock::WHITE, 4)?;
            shape(b, p, x + 13, y + 13, 18, 3, SURFACE, 1)?;
            shape(b, p, x + 13, y + 21, 18, 3, SURFACE, 1)?;
            shape(b, p, x + 13, y + 29, 12, 3, SURFACE, 1)?;
        }
        Icon::Chip => {
            // Fit the reference's visible silhouette to the full44px slot, rather than its padded viewBox.
            for offset in [14, 25] {
                shape(b, p, x + offset, y, 5, 44, stock::WHITE, 0)?;
                shape(b, p, x, y + offset, 44, 5, stock::WHITE, 0)?;
            }
            b.submit(lv)?;
            shape(b, p, x + 5, y + 5, 34, 34, stock::WHITE, 5)?;
            shape(b, p, x + 10, y + 10, 24, 24, SURFACE, 0)?;
            shape(b, p, x + 14, y + 14, 16, 16, stock::WHITE, 0)?;
        }
        Icon::Drive => {
            for i in 0..3 {
                shape(
                    b,
                    p,
                    x,
                    y + i * 16,
                    46,
                    12,
                    if i == 0 { stock::WHITE } else { SECONDARY },
                    4,
                )?;
            }
            shape(b, p, x + 35, y + 3, 5, 5, stock::ACCENT, 3)?;
        }
        Icon::Usage => {
            shape(b, p, x, y, 44, 44, stock::WHITE, 22)?;
            shape(b, p, x + 20, y + 10, 4, 4, SURFACE, 2)?;
            shape(b, p, x + 20, y + 20, 4, 14, SURFACE, 2)?;
        }
        Icon::App(letter) => {
            let circle = shape(b, p, x - 2, y - 2, 48, 48, stock::LETTER_BG, 24)?;
            let mut initial = Text::new();
            let _ = write!(initial, "{}", letter.to_ascii_uppercase());
            let (dx, ty) = model::button_label_position(initial.as_str(), 48);
            label(
                b,
                circle,
                initial.as_str(),
                Rect::new(dx, ty, 48, 38),
                0,
                stock::WHITE,
                2,
                false,
            )?;
        }
    }
    b.submit(lv)
}
fn navigation(
    b: &mut Batch,
    lv: &Lv,
    s: &mut Screen,
    name: &str,
    value: &str,
    action: u16,
    kind: Icon,
) -> Result<(), LvError> {
    let app_row = matches!(s.page, Page::Apps);
    let height = 128;
    let text_x = if app_row { 132 } else { 96 };
    let o = block(b, s.root, height)?;
    b.style(o, Prop::BgColor, 0, SURFACE)?;
    b.style(o, Prop::BgOpa, 0, 255)?;
    b.style(o, Prop::Radius, 0, height / 2)?;
    b.flags(o, &[Flag::Clickable], &[])?;
    b.style(o, Prop::BgColor, stock::PRESSED, stock::ROW_PRESSED)?;
    s.bind(b, o, action)?;
    icon(b, lv, o, kind, 34, 42)?;
    label(
        b,
        o,
        name,
        Rect::new(
            text_x,
            if app_row {
                22
            } else if value.is_empty() {
                48
            } else {
                25
            },
            390 - text_x,
            46,
        ),
        3,
        stock::WHITE,
        0,
        false,
    )?;
    if !value.is_empty() {
        label(
            b,
            o,
            value,
            Rect::new(text_x, if app_row { 72 } else { 75 }, 390 - text_x, 38),
            0,
            SECONDARY,
            0,
            false,
        )?;
    }
    b.submit(lv)
}
fn action_button(
    b: &mut Batch,
    lv: &Lv,
    s: &mut Screen,
    name: &str,
    action: u16,
) -> Result<(), LvError> {
    let outer = block(b, s.root, 92)?;
    let w = 280;
    let button = shape(
        b,
        outer,
        ((418 - w) / 2) as i16,
        8,
        w as i16,
        72,
        if action == REFRESH {
            stock::ACCENT
        } else {
            stock::ROW_BG
        },
        36,
    )?;
    b.flags(button, &[Flag::Clickable], &[])?;
    b.style(button, Prop::BgColor, stock::PRESSED, stock::ROW_PRESSED)?;
    s.bind(b, button, action)?;
    let (dx, ty) = model::button_label_position(name, 72);
    label(
        b,
        button,
        name,
        Rect::new(12 + dx, ty, (w - 24) as i16, 42),
        0,
        stock::WHITE,
        2,
        false,
    )?;
    b.submit(lv)
}
fn row(
    b: &mut Batch,
    lv: &Lv,
    s: &mut Screen,
    name: &str,
    value: &str,
    action: u16,
    _filled: bool,
) -> Result<(), LvError> {
    if action == BACK || action == REFRESH || action == 200 || action == 201 {
        return action_button(b, lv, s, name, action);
    }
    if (128..=157).contains(&action) {
        let o = block(b, s.root, 104)?;
        b.flags(o, &[Flag::Clickable], &[])?;
        b.style(o, Prop::BgColor, stock::PRESSED, stock::ROW_PRESSED)?;
        b.style(o, Prop::BgOpa, stock::PRESSED, 255)?;
        b.style(o, Prop::Radius, 0, 52)?;
        s.bind(b, o, action)?;
        label(
            b,
            o,
            name,
            Rect::new(40, 10, 338, 42),
            0,
            stock::WHITE,
            0,
            false,
        )?;
        label(
            b,
            o,
            value,
            Rect::new(40, 59, 338, 32),
            1,
            SECONDARY,
            0,
            false,
        )?;
        return b.submit(lv);
    }
    if action != 0 {
        let kind = match action {
            1 => Icon::Apps,
            2 => Icon::Face,
            3 => Icon::Folder,
            4..=6 => Icon::Chip,
            8 => Icon::Usage,
            16..=31 => {
                if name == "Storage" {
                    Icon::Drive
                } else {
                    Icon::App(name.chars().next().unwrap_or('A'))
                }
            }
            32..=127 => {
                if matches!(s.page, Page::Faces(_)) {
                    Icon::Face
                } else {
                    Icon::Document
                }
            }
            _ => Icon::Drive,
        };
        return navigation(b, lv, s, name, value, action, kind);
    }
    // Read-only facts use compact text/value spacing, not button surfaces.
    let o = block(b, s.root, 94)?;
    label(
        b,
        o,
        name,
        Rect::new(40, 10, 338, 40),
        0,
        stock::WHITE,
        0,
        false,
    )?;
    label(
        b,
        o,
        value,
        Rect::new(40, 55, 338, 32),
        1,
        SECONDARY,
        0,
        false,
    )?;
    b.submit(lv)
}
fn value(b: &mut Batch, lv: &Lv, s: &mut Screen, name: &str, n: u32) -> Result<(), LvError> {
    row(b, lv, s, name, Text::bytes(n).as_str(), 0, false)
}
fn count_label(inv: &Inventory, cat: Category) -> Text {
    let mut t = Text::new();
    if inv.error < 0 && !inv.ready {
        let _ = write!(t, "Scan unavailable");
    } else if !inv.ready {
        let _ = write!(t, "Reading files...");
    } else {
        let _ = write!(
            t,
            "{}{}",
            Text::bytes(inv.sum(cat)).as_str(),
            if inv.partial { " found" } else { "" }
        );
    }
    t
}
fn start(b: &mut Batch, lv: &Lv, page: Page, title: &str) -> Result<Screen, LvError> {
    stock::theme(b)?;
    let (root, bar) = stock::page(b)?;
    // Keep the final action inside the circular safe area without centering it.
    b.style(root, Prop::PadBottom, 0, 48)?;
    b.style(root, Prop::PadLeft, 0, 24)?;
    b.style(root, Prop::PadRight, 0, 24)?;
    b.flex(
        root,
        Flow::Column,
        FlexAlign::Start,
        FlexAlign::Start,
        FlexAlign::Start,
    )?;
    b.style(root, Prop::PadRow, 0, 10)?;
    heading(b, root, title, page == Page::Home)?;
    b.submit(lv)?;
    Ok(Screen {
        root,
        bar,
        page,
        complete: false,
        buttons: [(ROOT, 0); 32],
        n: 0,
    })
}
pub fn render(
    b: &mut Batch,
    lv: &Lv,
    page: Page,
    parts: &[Partition],
    fs: Option<Filesystem>,
    inv: &Inventory,
    reading: bool,
    stale: bool,
) -> Result<Screen, LvError> {
    let title = match page {
        Page::Home => "Storage",
        Page::Apps => "Apps",
        Page::Faces(_) => "Watch faces",
        Page::Other(_) => "Other files",
        Page::System => "System storage",
        Page::Nand => "NAND",
        Page::Nor => "System flash",
        Page::App(slot) => {
            if inv.apps[slot as usize].valid {
                inv.apps[slot as usize].name()
            } else {
                "App details"
            }
        }
        Page::File(_) => "File details",
        Page::Partition(_) => "Reservation",
        Page::Accounting => "Usage details",
    };
    let mut s = start(b, lv, page, title)?;
    s.complete = inv.ready;
    match page {
        Page::Home => {
            if let Some(f) = fs {
                let complete = crate::inventory::breakdown(&f, inv);
                let values = if let Some(v) = complete {
                    [v[0], v[1], v[2], v[3], f.free]
                } else {
                    [f.total - f.free, 0, 0, 0, f.free]
                };
                let names = if complete.is_some() {
                    ["Apps", "Loader", "Faces", "Other", "Free"]
                } else {
                    ["Used", "Loader", "Faces", "Other", "Free"]
                };
                let visible = values[..4].iter().filter(|&&v| v > 0).count();
                let legend_width: i16 = values
                    .iter()
                    .enumerate()
                    .filter(|(i, v)| *i < 4 && **v > 0)
                    .map(|(i, _)| model::legend_width(names[i]) + 14)
                    .sum::<i16>()
                    + visible.saturating_sub(1) as i16 * 18;
                let rows = if legend_width > 374 { 2 } else { 1 };
                // Keep the legend clear of the bar, with breathing room before the cards.
                let summary = block(b, s.root, 156 + rows * 32)?;
                let free = Text::bytes(f.free);
                let (number, unit) = free
                    .as_str()
                    .split_once(' ')
                    .unwrap_or((free.as_str(), "B"));
                let mut free_unit = Text::new();
                let _ = write!(free_unit, "{} free", unit);
                let nw = model::number_width(number);
                let uw = match unit {
                    "MiB" => 94,
                    "KiB" => 89,
                    _ => 67,
                };
                let left = (418 - nw - 12 - uw) / 2;
                label(
                    b,
                    summary,
                    number,
                    Rect::new(left, 0, nw + 4, 64),
                    2,
                    stock::WHITE,
                    0,
                    false,
                )?;
                label(
                    b,
                    summary,
                    free_unit.as_str(),
                    Rect::new(left + nw + 12, 29, uw + 4, 36),
                    1,
                    SECONDARY,
                    0,
                    false,
                )?;
                let mut caption = Text::new();
                let _ = write!(
                    caption,
                    "{} used / {}",
                    Text::bytes(f.total - f.free).as_str(),
                    Text::bytes(f.total).as_str()
                );
                label(
                    b,
                    summary,
                    caption.as_str(),
                    Rect::new(40, 64, 338, 36),
                    1,
                    SECONDARY,
                    2,
                    false,
                )?;
                b.submit(lv)?;
                chart(b, summary, 22, 106, 374, &values)?;
                b.flags(summary, &[Flag::Clickable], &[])?;
                s.bind(b, summary, 8)?;
                b.submit(lv)?;
                let mut indices = [0usize; 5];
                let mut n = 0;
                for i in 0..5 {
                    if i < 4 && values[i] > 0 {
                        indices[n] = i;
                        n += 1;
                    }
                }
                let first = if rows > 1 { (n + 1) / 2 } else { n };
                for row in 0..rows {
                    let range = if row == 0 { 0..first } else { first..n };
                    let mut width = 0;
                    for (position, k) in range.clone().enumerate() {
                        width += model::legend_width(names[indices[k]]) + 14;
                        if position > 0 {
                            width += 18;
                        }
                    }
                    let mut x = 22 + (374 - width) / 2;
                    for k in range {
                        let i = indices[k];
                        shape(
                            b,
                            summary,
                            x,
                            150 + row as i16 * 32,
                            8,
                            8,
                            CHART_COLORS[i],
                            4,
                        )?;
                        let w = model::legend_width(names[i]);
                        label(
                            b,
                            summary,
                            names[i],
                            Rect::new(x + 14, 144 + row as i16 * 32, w + 4, 32),
                            1,
                            SECONDARY,
                            0,
                            false,
                        )?;
                        x += w + 32;
                    }
                    b.submit(lv)?;
                }
            } else {
                note(
                    b,
                    lv,
                    s.root,
                    if inv.error == -95 {
                        "Update App Loader to use Storage."
                    } else {
                        "Usage unavailable. Refresh to try again."
                    },
                )?;
            }
            if stale {
                note(
                    b,
                    lv,
                    s.root,
                    "Refresh did not finish. Showing the previous reading.",
                )?;
            }
            row(
                b,
                lv,
                &mut s,
                "Apps",
                count_label(inv, Category::App(0)).as_str(),
                1,
                false,
            )?;
            row(
                b,
                lv,
                &mut s,
                "Watch faces",
                count_label(inv, Category::Face).as_str(),
                2,
                false,
            )?;
            row(
                b,
                lv,
                &mut s,
                "Other files",
                count_label(inv, Category::Other).as_str(),
                3,
                false,
            )?;
            row(b, lv, &mut s, "System storage", "Firmware & data", 4, true)?;
            row(
                b,
                lv,
                &mut s,
                "Usage details",
                "Files & allocation",
                8,
                false,
            )?;
            if inv.partial {
                note(
                    b,
                    lv,
                    s.root,
                    "File list is incomplete. Some folders could not be read or exceed scan limits.",
                )?;
            }
            row(
                b,
                lv,
                &mut s,
                if reading { "Cancel scan" } else { "Refresh" },
                "",
                REFRESH,
                true,
            )?;
        }
        Page::Apps => {
            let mut found = false;
            for i in 0..16 {
                let total = inv.app_total(i);
                let a = inv.apps[i as usize];
                if a.valid || total > 0 {
                    found = true;
                    let mut name = Text::new();
                    if a.valid {
                        let _ = write!(name, "{}", a.name());
                    } else {
                        let _ = write!(name, "App slot {}", i + 1);
                    }
                    navigation(
                        b,
                        lv,
                        &mut s,
                        name.as_str(),
                        Text::bytes(total).as_str(),
                        16 + i as u16,
                        if a.icon {
                            Icon::Installed(i)
                        } else {
                            Icon::App(name.as_str().chars().next().unwrap_or('A'))
                        },
                    )?;
                }
            }
            if !found {
                note(
                    b,
                    lv,
                    s.root,
                    if inv.ready {
                        "No app files found."
                    } else if inv.error < 0 {
                        "File scan unavailable. Refresh to try again."
                    } else {
                        "Reading app files..."
                    },
                )?;
            }
        }
        Page::Faces(offset) | Page::Other(offset) => {
            let cat = if matches!(page, Page::Faces(_)) {
                Category::Face
            } else {
                Category::Other
            };
            let mut found = false;
            let mut ordinal = 0usize;
            let mut more = false;
            for (i, f) in inv.files[..inv.count].iter().enumerate() {
                if f.category == cat {
                    let at = ordinal;
                    ordinal += 1;
                    if at < (offset as usize) * 12 {
                        continue;
                    }
                    if at >= (offset as usize + 1) * 12 {
                        more = true;
                        continue;
                    }
                    found = true;
                    row(
                        b,
                        lv,
                        &mut s,
                        f.name(),
                        Text::bytes(f.size).as_str(),
                        32 + i as u16,
                        false,
                    )?;
                }
            }
            if offset > 0 {
                row(b, lv, &mut s, "Previous files", "", 201, true)?;
            }
            if more {
                row(b, lv, &mut s, "More files", "", 200, true)?;
            }
            if !found {
                let empty = block(b, s.root, 126)?;
                icon(
                    b,
                    lv,
                    empty,
                    if matches!(page, Page::Faces(_)) {
                        Icon::Face
                    } else {
                        Icon::Folder
                    },
                    187,
                    44,
                )?;
                note(
                    b,
                    lv,
                    s.root,
                    if !inv.ready && inv.error < 0 {
                        "File scan unavailable. Refresh to try again."
                    } else if !inv.ready {
                        "Reading files..."
                    } else if inv.partial {
                        "No matching files found in the partial scan."
                    } else {
                        "No files found."
                    },
                )?;
            }
            if inv.partial {
                note(b, lv, s.root, "Partial file list. Refresh to try again.")?;
            }
            if matches!(page, Page::Faces(_)) {
                note(
                    b,
                    lv,
                    s.root,
                    "Lists .wfc files by filename. Built-in faces are part of system resources.",
                )?;
            }
        }
        Page::App(slot) => {
            let a = inv.apps[slot as usize];
            if a.valid && a.name().len() > 16 {
                note(b, lv, s.root, a.name())?;
            }
            value(b, lv, &mut s, "Files total", inv.app_total(slot))?;
            for (i, f) in inv.files[..inv.count].iter().enumerate() {
                if f.category == Category::App(slot) {
                    let name = if f.name().as_bytes()[2..].eq_ignore_ascii_case(b"pkg") {
                        "App package"
                    } else if f.name().as_bytes()[2..].eq_ignore_ascii_case(b"dat") {
                        "Saved data"
                    } else {
                        "Widget record"
                    };
                    row(
                        b,
                        lv,
                        &mut s,
                        name,
                        Text::bytes(f.size).as_str(),
                        32 + i as u16,
                        false,
                    )?;
                }
            }
            if a.valid {
                let mut t = Text::new();
                let _ = write!(t, "Version {}", a.version);
                note(b, lv, s.root, t.as_str())?;
            }
        }
        Page::File(i) => {
            let f = &inv.files[i as usize];
            note(b, lv, s.root, f.name())?;
            value(b, lv, &mut s, "File size", f.size)?;
            let mut exact = Text::new();
            let _ = write!(exact, "{} bytes", f.size);
            note(b, lv, s.root, exact.as_str())?;
            note(b, lv, s.root, f.path())?;
            note(
                b,
                lv,
                s.root,
                "Logical file bytes. Space on disk may be larger.",
            )?;
        }
        Page::System => {
            row(b, lv, &mut s, "NAND", "Resources & files", 5, true)?;
            row(b, lv, &mut s, "System flash", "Firmware & boot", 6, true)?;
            if let Some(f) = fs {
                value(b, lv, &mut s, "App loader file", f.loader)?;
            }
            note(
                b,
                lv,
                s.root,
                "Reservations are capacities, not measured used space.",
            )?;
        }
        Page::Nand | Page::Nor => {
            let dev = if page == Page::Nand { 2 } else { 0 };
            if model::reference_layout(parts) {
                value(
                    b,
                    lv,
                    &mut s,
                    "Chip capacity",
                    if dev == 2 {
                        model::NAND_MAIN
                    } else {
                        model::NOR_MAIN
                    },
                )?;
                if dev == 2 {
                    value(b, lv, &mut s, "Logical NAND", model::NAND_LOGICAL)?;
                    value(b, lv, &mut s, "Flash management", model::FTL_MANAGEMENT)?;
                    note(b, lv, s.root, "Management space is not file storage.")?;
                }
            } else {
                note(
                    b,
                    lv,
                    s.root,
                    "Hardware capacity is not verified for this layout.",
                )?;
            }
            for (i, p) in parts.iter().enumerate().filter(|(_, p)| p.device == dev) {
                row(
                    b,
                    lv,
                    &mut s,
                    model::purpose(p),
                    Text::bytes(p.allocated).as_str(),
                    128 + i as u16,
                    false,
                )?;
            }
            if dev == 2 && model::reference_layout(parts) {
                value(b, lv, &mut s, "Unpartitioned tail", model::LOGICAL_TAIL)?;
                note(
                    b,
                    lv,
                    s.root,
                    "Tail capacity is not available as file storage.",
                )?;
            }
        }
        Page::Partition(i) => {
            let p = &parts[i as usize];
            note(b, lv, s.root, model::purpose(p))?;
            value(b, lv, &mut s, "Reserved capacity", p.allocated)?;
            row(b, lv, &mut s, "Partition name", p.name(), 0, false)?;
            let mut t = Text::new();
            let _ = write!(t, "0x{:08X}", p.offset);
            row(b, lv, &mut s, "Device offset", t.as_str(), 0, false)?;
            note(
                b,
                lv,
                s.root,
                "Reserved capacity does not report how many bytes are in use.",
            )?;
        }
        Page::Accounting => {
            if let Some(f) = fs {
                let used = f.total - f.free;
                let summary = block(b, s.root, 142)?;
                label(
                    b,
                    summary,
                    Text::bytes(used).as_str(),
                    Rect::new(40, 0, 338, 64),
                    2,
                    stock::WHITE,
                    2,
                    false,
                )?;
                label(
                    b,
                    summary,
                    "Breakdown of used space",
                    Rect::new(40, 70, 338, 34),
                    1,
                    SECONDARY,
                    2,
                    false,
                )?;
                b.submit(lv)?;
                if let Some(values) = crate::inventory::breakdown(&f, inv) {
                    let colors = CHART_COLORS;
                    chart(b, summary, 40, 116, 338, &values)?;
                    b.submit(lv)?;
                    for (i, name) in ["Apps", "App loader", "Watch faces", "Other usage"]
                        .into_iter()
                        .enumerate()
                    {
                        if values[i] == 0 {
                            continue;
                        }
                        let item = block(b, s.root, 86)?;
                        shape(b, item, 40, 28, 10, 10, colors[i], 3)?;
                        label(
                            b,
                            item,
                            name,
                            Rect::new(68, 6, 310, 40),
                            0,
                            stock::WHITE,
                            0,
                            false,
                        )?;
                        label(
                            b,
                            item,
                            Text::bytes(values[i]).as_str(),
                            Rect::new(68, 50, 310, 32),
                            1,
                            SECONDARY,
                            0,
                            false,
                        )?;
                        b.submit(lv)?;
                    }
                    note(
                        b,
                        lv,
                        s.root,
                        "Other usage includes remaining files and unassigned allocation.",
                    )?;
                } else {
                    note(
                        b,
                        lv,
                        s.root,
                        "A complete, consistent scan is needed for the breakdown.",
                    )?;
                }
                value(b, lv, &mut s, "Free space", f.free)?;
                value(b, lv, &mut s, "File-data capacity", f.total)?;
                note(
                    b,
                    lv,
                    s.root,
                    "Firmware and built-in resources are separate.",
                )?;
            } else {
                note(b, lv, s.root, "No usage reading is available.")?;
            }
        }
    }
    if page != Page::Home {
        row(b, lv, &mut s, "Back", "", BACK, true)?;
    }
    Ok(s)
}
