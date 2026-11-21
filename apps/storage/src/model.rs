use cmf_wp2_app::storage::{Filesystem, Partition};
use core::fmt::{self, Write};

pub struct Text {
    buf: [u8; 64],
    len: usize,
}
impl Text {
    pub fn new() -> Self {
        Self {
            buf: [0; 64],
            len: 0,
        }
    }
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("?")
    }
    pub fn bytes(n: u32) -> Self {
        let mut s = Self::new();
        if n >= 1048576 && n % 1048576 == 0 {
            let _ = write!(s, "{} MiB", n / 1048576);
        } else if n >= 1048576 {
            let _ = write!(
                s,
                "{}.{:02} MiB",
                n / 1048576,
                (u64::from(n % 1048576) * 100 / 1048576)
            );
        } else if n >= 1024 {
            if n % 1024 == 0 {
                let _ = write!(s, "{} KiB", n / 1024);
            } else {
                let _ = write!(s, "{}.{:02} KiB", n / 1024, (n % 1024) * 100 / 1024);
            }
        } else {
            let _ = write!(s, "{} B", n);
        }
        s
    }
}
impl Write for Text {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.len + s.len() > self.buf.len() {
            return Err(fmt::Error);
        }
        self.buf[self.len..self.len + s.len()].copy_from_slice(s.as_bytes());
        self.len += s.len();
        Ok(())
    }
}
pub fn total(parts: &[Partition], device: u8) -> Option<u32> {
    let mut total = 0u32;
    for (i, p) in parts.iter().enumerate().filter(|(_, p)| p.device == device) {
        let end = p.offset.checked_add(p.allocated)?;
        for q in &parts[..i] {
            if q.device == device && p.offset < q.offset.checked_add(q.allocated)? && q.offset < end
            {
                return None;
            }
        }
        total = total.checked_add(p.allocated)?;
    }
    Some(total)
}
pub fn other(fs: &Filesystem) -> Option<u32> {
    fs.total
        .checked_sub(fs.free)?
        .checked_sub(fs.apps)?
        .checked_sub(fs.app_data)?
        .checked_sub(fs.widgets)?
        .checked_sub(fs.loader)
}
pub fn percent(used: u32, total: u32) -> i16 {
    if total == 0 {
        0
    } else {
        (u64::from(used).min(total as u64) * 100 / total as u64) as i16
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i16,
    pub y: i16,
    pub w: i16,
    pub h: i16,
}
impl Rect {
    pub const fn new(x: i16, y: i16, w: i16, h: i16) -> Self {
        Self { x, y, w, h }
    }
    pub fn in_round_screen(self, margin: i32) -> bool {
        let r = 233 - margin;
        [self.x as i32, self.x as i32 + self.w as i32]
            .into_iter()
            .all(|x| {
                [self.y as i32, self.y as i32 + self.h as i32]
                    .into_iter()
                    .all(|y| (x - 233) * (x - 233) + (y - 233) * (y - 233) <= r * r)
            })
    }
}
pub const TITLE: Rect = Rect::new(113, 46, 240, 42);
pub const MAP: Rect = Rect::new(78, 142, 310, 174);
pub const BOTTOM: Rect = Rect::new(128, 376, 210, 52);
pub const CATEGORY_NAMES: [&str; 4] = ["Apps", "Saved data", "App loader", "Other + allocation"];
pub fn used_categories(f: &Filesystem) -> Option<[u32; 4]> {
    Some([
        f.apps,
        f.app_data.checked_add(f.widgets)?,
        f.loader,
        other(f)?,
    ])
}
/// Binary treemap. Preserve proportions; tiny categories are selected in the details list.
pub fn treemap(values: [u32; 4], bounds: Rect) -> [Rect; 4] {
    fn split(items: &[(usize, u32)], r: Rect, out: &mut [Rect; 4]) {
        if items.is_empty() {
            return;
        }
        if items.len() == 1 {
            out[items[0].0] = r;
            return;
        }
        let total: u64 = items.iter().map(|i| i.1 as u64).sum();
        let mut left = 0u64;
        let mut cut = 1;
        let mut best = u64::MAX;
        for i in 1..items.len() {
            left += items[i - 1].1 as u64;
            let distance = (2 * left).abs_diff(total);
            if distance < best {
                best = distance;
                cut = i;
            }
        }
        let part: u64 = items[..cut].iter().map(|i| i.1 as u64).sum();
        let mut a = r;
        let mut b = r;
        if r.w >= r.h {
            a.w = (r.w as u64 * part / total) as i16;
            b.x += a.w;
            b.w -= a.w;
        } else {
            a.h = (r.h as u64 * part / total) as i16;
            b.y += a.h;
            b.h -= a.h;
        }
        split(&items[..cut], a, out);
        split(&items[cut..], b, out);
    }
    let mut items = [(0usize, 0u32); 4];
    let mut n = 0;
    for (i, v) in values.into_iter().enumerate() {
        if v > 0 {
            items[n] = (i, v);
            n += 1;
        }
    }
    items[..n].sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut out = [Rect::default(); 4];
    split(&items[..n], bounds, &mut out);
    out
}
/// Exclusive allocation categories: firmware/boot, resources, writable files, reserved/settings/other.
pub fn allocation_groups(parts: &[Partition]) -> Option<[u32; 4]> {
    let mut out = [0u32; 4];
    for p in parts {
        let i = match p.file_id {
            1..=5 => 0,
            10..=39 => 1,
            40 => 2,
            _ => 3,
        };
        out[i] = out[i].checked_add(p.allocated)?;
    }
    for dev in 0..16 {
        total(parts, dev)?;
    }
    Some(out)
}
pub fn widths(values: &[u32], width: u32) -> [u32; 5] {
    let mut out = [0; 5];
    let total: u64 = values.iter().map(|&x| u64::from(x)).sum();
    if total == 0 {
        return out;
    }
    let mut sum = 0u64;
    let mut previous = 0;
    for (i, &v) in values.iter().take(5).enumerate() {
        sum += u64::from(v);
        let end = (sum * u64::from(width) / total) as u32;
        out[i] = end - previous;
        previous = end;
    }
    out
}
/// Reserve separators only for segments that are visible at the current pixel width.
pub fn separated_widths(values: &[u32], width: u32) -> [u32; 5] {
    // Pick visible segments once, then remove only segments that cannot retain a pixel after reserving gaps.
    // Removed segments never reappear, so tiny values cannot oscillate at a separator boundary.
    let initial = widths(values, width);
    let mut filtered = [0u32; 5];
    for i in 0..values.len().min(5) {
        if initial[i] > 0 {
            filtered[i] = values[i];
        }
    }
    for _ in 0..5 {
        let n = filtered.iter().filter(|&&v| v > 0).count();
        let out = widths(
            &filtered,
            width.saturating_sub(n.saturating_sub(1) as u32 * 2),
        );
        if out.iter().all(|&v| v == 0) && filtered.iter().any(|&v| v > 0) {
            let best = (0..5).max_by_key(|&i| filtered[i]).unwrap();
            let v = filtered[best];
            filtered = [0; 5];
            filtered[best] = v;
            continue;
        }
        let mut removed = false;
        for i in 0..5 {
            if filtered[i] > 0 && out[i] == 0 {
                filtered[i] = 0;
                removed = true;
            }
        }
        if !removed {
            return out;
        }
    }
    widths(&filtered, width)
}
#[cfg(test)]
mod separator_tests {
    use super::*;
    #[test]
    fn separators_fill_requested_bounds() {
        for v in [
            [20, 30, 10, 40, 900],
            [1, 0, 0, 0, 999999],
            [10, 10, 10, 10, 0],
            [0; 5],
            [1, 373, 0, 0, 0],
        ] {
            let w = separated_widths(&v, 374);
            let n = w.iter().filter(|&&x| x > 0).count();
            assert_eq!(
                w.iter().sum::<u32>() + n.saturating_sub(1) as u32 * 2,
                if n > 0 { 374 } else { 0 }
            );
        }
    }
}
pub fn purpose(p: &Partition) -> &'static str {
    match p.file_id {
        1 => "Bootloader",
        2 => "Partition table",
        3 => "Recovery",
        4 => "Firmware",
        5 => "System files",
        6..=8 => "Settings",
        12 | 13 => "Fonts / watch faces",
        10 | 11 => "UI resources",
        14 => "Exercise animations",
        20 => "Boot / sound assets",
        40 => "Writable files",
        248 => "Reserved",
        249 => "Settings / history",
        250 | 251 => "Logs",
        253 => "Crash records",
        254 => "OTA staging",
        _ => "Other allocation",
    }
}

pub const NAND_MAIN: u32 = 128 * 1048576;
pub const NAND_LOGICAL: u32 = 123 * 1048576;
pub const NOR_MAIN: u32 = 4 * 1048576;
pub const FTL_MANAGEMENT: u32 = 5 * 1048576;
pub const LOGICAL_TAIL: u32 = 11010048;
const VERIFIED_LAYOUT: &[(u8, u8, u32, u32)] = &[
    (0, 1, 0x00000000, 4096),
    (0, 2, 0x00001000, 4096),
    (0, 1, 0x00002000, 4096),
    (0, 2, 0x00003000, 4096),
    (0, 3, 0x00004000, 131072),
    (0, 4, 0x00024000, 3452928),
    (0, 248, 0x0036f000, 409600),
    (0, 5, 0x003d3000, 65536),
    (0, 6, 0x003e3000, 4096),
    (0, 7, 0x003e4000, 8192),
    (0, 8, 0x003e6000, 8192),
    (0, 251, 0x003e8000, 16384),
    (2, 249, 0x00480000, 2097152),
    (0, 253, 0x003ec000, 81920),
    (2, 10, 0x01280000, 10485760),
    (2, 12, 0x00680000, 12582912),
    (2, 11, 0x01c80000, 8388608),
    (2, 14, 0x02480000, 67108864),
    (2, 20, 0x00380000, 1048576),
    (2, 254, 0x00000000, 3670016),
    (2, 250, 0x06480000, 2097152),
    (2, 40, 0x06680000, 10485760),
];
pub fn reference_layout(parts: &[Partition]) -> bool {
    parts.len() == VERIFIED_LAYOUT.len()
        && VERIFIED_LAYOUT
            .iter()
            .all(|&(device, id, offset, allocated)| {
                parts
                    .iter()
                    .filter(|p| {
                        p.device == device
                            && p.file_id == id
                            && p.offset == offset
                            && p.allocated == allocated
                    })
                    .count()
                    == 1
            })
}
pub fn valid_filesystem(f: &Filesystem) -> bool {
    f.total > 0
        && f.allocation_unit.is_power_of_two()
        && f.total % f.allocation_unit == 0
        && f.free % f.allocation_unit == 0
        && other(f).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verified_reference_requires_exact_device_layout() {
        let mut parts: [Partition; 22] = [Partition::default(); 22];
        for (p, &(device, file_id, offset, allocated)) in parts.iter_mut().zip(VERIFIED_LAYOUT) {
            *p = Partition {
                device,
                file_id,
                offset,
                allocated,
                ..Default::default()
            };
        }
        assert!(reference_layout(&parts));
        assert_eq!(total(&parts, 2), Some(1125 * 1048576 / 10));
        assert_eq!(total(&parts, 0), Some(NOR_MAIN));
        assert_eq!(NAND_LOGICAL - total(&parts, 2).unwrap(), LOGICAL_TAIL);
        assert_eq!(NAND_MAIN - NAND_LOGICAL, FTL_MANAGEMENT);
        parts[0].allocated += 1;
        assert!(!reference_layout(&parts));
    }
    #[test]
    fn invalid_live_counters_are_not_presented_as_usage() {
        assert!(!valid_filesystem(&Filesystem::default()));
        assert!(!valid_filesystem(&Filesystem {
            total: 1024,
            free: 2048,
            allocation_unit: 1024,
            ..Default::default()
        }));
        assert!(valid_filesystem(&Filesystem {
            total: 10240,
            free: 8192,
            apps: 1024,
            allocation_unit: 1024,
            ..Default::default()
        }));
    }
    #[test]
    fn units() {
        assert_eq!(Text::bytes(1048576).as_str(), "1 MiB");
        assert_eq!(Text::bytes(0).as_str(), "0 B");
    }
    #[test]
    fn never_double_counts_overlaps() {
        let a = Partition {
            device: 2,
            offset: 0,
            allocated: 100,
            ..Default::default()
        };
        assert_eq!(total(&[a, a], 2), None);
        assert_eq!(total(&[a, Partition { offset: 100, ..a }], 2), Some(200));
    }
    #[test]
    fn inconsistent_live_snapshot_is_unknown() {
        assert_eq!(
            other(&Filesystem {
                total: 10,
                free: 11,
                ..Default::default()
            }),
            None
        );
    }
    #[test]
    fn bar_is_bounded() {
        assert_eq!(percent(u32::MAX, u32::MAX), 100);
        assert_eq!(percent(3, 0), 0);
    }
    #[test]
    fn segments_cover_exact_width() {
        assert_eq!(widths(&[1, 1, 1], 370).iter().sum::<u32>(), 370);
        assert_eq!(widths(&[0, 0], 370), [0; 5]);
    }
    #[test]
    fn fixed_controls_fit_the_round_safe_area() {
        for r in [
            TITLE,
            MAP,
            BOTTOM,
            Rect::new(123, 376, 104, 52),
            Rect::new(239, 376, 104, 52),
            Rect::new(63, 332, 340, 34),
            Rect::new(68, 136, 330, 86),
            Rect::new(68, 234, 330, 86),
        ] {
            assert!(r.in_round_screen(8), "{r:?}");
        }
    }
    #[test]
    fn treemap_covers_area_without_overlaps() {
        for values in [
            [17982, 0, 49944, 6802],
            [0, 0, 0, 1],
            [1, 1, 1, 1],
            [u32::MAX, 1, 3, 0],
        ] {
            let rects = treemap(values, MAP);
            assert_eq!(
                rects
                    .iter()
                    .map(|r| i32::from(r.w) * i32::from(r.h))
                    .sum::<i32>(),
                i32::from(MAP.w) * i32::from(MAP.h)
            );
            for (i, a) in rects.iter().enumerate().filter(|(_, r)| r.w > 0 && r.h > 0) {
                assert!(
                    a.x >= MAP.x
                        && a.y >= MAP.y
                        && a.x + a.w <= MAP.x + MAP.w
                        && a.y + a.h <= MAP.y + MAP.h
                );
                for b in &rects[..i] {
                    assert!(
                        b.w == 0
                            || b.h == 0
                            || a.x >= b.x + b.w
                            || b.x >= a.x + a.w
                            || a.y >= b.y + b.h
                            || b.y >= a.y + a.h
                    );
                }
            }
        }
        assert_eq!(treemap([0; 4], MAP), [Rect::default(); 4]);
    }
}

/// Nominal integer glyph advances from the extracted WP2 font48M (hash in design/VISUAL-REVISION.md).
/// Used for the unit's position, not claimed to be a full kerning engine.
pub fn number_width(s: &str) -> i16 {
    const DIGITS: [i16; 10] = [31, 19, 29, 30, 31, 29, 30, 27, 30, 30];
    s.bytes()
        .map(|b| match b {
            b'0'..=b'9' => DIGITS[(b - b'0') as usize],
            b'.' => 14,
            _ => 24,
        })
        .sum()
}

/// Visible-ink centering for the actual WP2 font32M. Native baseline measured at26px.
/// Nominal ASCII advances/bearings come from the extracted font, not string length.
pub fn button_label_position(text: &str, height: i16) -> (i16, i16) {
    const GLYPHS: [(i16, i16, i16, i16, i16); 95] = [
        (8, 0, 0, 0, 0),
        (9, 2, 0, 6, 24),
        (15, 3, 14, 10, 10),
        (20, 0, 0, 20, 24),
        (20, 1, -3, 19, 31),
        (31, 3, 0, 26, 24),
        (20, 1, 0, 20, 24),
        (10, 3, 14, 4, 10),
        (11, 3, -4, 8, 30),
        (11, 1, -4, 8, 30),
        (16, 2, 10, 13, 14),
        (21, 2, 2, 17, 16),
        (9, 1, -6, 7, 10),
        (14, 2, 8, 11, 4),
        (9, 2, 0, 6, 5),
        (11, 0, -4, 12, 29),
        (20, 1, 0, 18, 24),
        (13, 1, 0, 10, 24),
        (19, 2, 0, 16, 24),
        (20, 1, 0, 18, 24),
        (21, 1, 0, 19, 24),
        (19, 2, 0, 17, 24),
        (20, 1, 0, 18, 24),
        (18, 1, 0, 16, 24),
        (20, 1, 0, 18, 24),
        (20, 1, 0, 18, 24),
        (9, 2, 0, 6, 17),
        (10, 1, -6, 7, 23),
        (21, 2, 0, 16, 19),
        (21, 3, 4, 15, 11),
        (21, 3, 0, 16, 19),
        (16, 1, 0, 15, 24),
        (31, 1, -7, 29, 30),
        (22, 0, 0, 22, 24),
        (21, 2, 0, 18, 24),
        (23, 1, 0, 21, 24),
        (23, 2, 0, 20, 24),
        (19, 2, 0, 16, 24),
        (18, 2, 0, 16, 24),
        (23, 1, 0, 22, 24),
        (23, 2, 0, 20, 24),
        (8, 2, 0, 5, 24),
        (18, 1, 0, 15, 24),
        (22, 2, 0, 20, 24),
        (18, 2, 0, 15, 24),
        (29, 2, 0, 25, 24),
        (24, 2, 0, 20, 24),
        (24, 1, 0, 22, 24),
        (20, 2, 0, 17, 24),
        (24, 1, -2, 22, 26),
        (20, 2, 0, 18, 24),
        (20, 1, 0, 19, 24),
        (20, 1, 0, 19, 24),
        (23, 2, 0, 20, 24),
        (22, 0, 0, 22, 24),
        (32, 0, 0, 32, 24),
        (22, 0, 0, 22, 24),
        (22, 0, 0, 22, 24),
        (20, 1, 0, 18, 24),
        (11, 3, -4, 7, 30),
        (11, 0, -4, 12, 29),
        (11, 1, -4, 8, 30),
        (15, 1, 13, 14, 10),
        (14, 0, -3, 15, 3),
        (10, 2, 20, 7, 5),
        (18, 1, 0, 15, 18),
        (19, 2, 0, 17, 24),
        (18, 1, 0, 17, 18),
        (19, 1, 0, 17, 24),
        (18, 1, 0, 17, 18),
        (12, 0, 0, 12, 25),
        (19, 1, -7, 17, 25),
        (19, 2, 0, 15, 24),
        (8, 1, 0, 6, 25),
        (8, -1, -7, 8, 32),
        (17, 2, 0, 16, 24),
        (8, 2, 0, 4, 24),
        (28, 2, 0, 25, 18),
        (19, 2, 0, 15, 18),
        (19, 1, 0, 17, 18),
        (19, 2, -7, 17, 25),
        (19, 1, -7, 17, 25),
        (12, 2, 0, 10, 18),
        (17, 1, 0, 15, 18),
        (10, 0, 0, 11, 22),
        (19, 2, 0, 15, 18),
        (18, 0, 0, 18, 18),
        (26, 0, 0, 26, 18),
        (17, 0, 0, 17, 18),
        (18, 0, -7, 18, 25),
        (17, 1, 0, 15, 18),
        (14, 2, -4, 11, 30),
        (11, 3, -8, 5, 39),
        (14, 1, -4, 11, 30),
        (21, 2, 6, 17, 7),
    ];
    let (mut pen, mut left, mut right, mut top, mut bottom) =
        (0i16, i16::MAX, i16::MIN, i16::MAX, i16::MIN);
    for c in text.bytes() {
        if !(32..127).contains(&c) {
            continue;
        }
        let (a, x, y, w, h) = GLYPHS[(c - 32) as usize];
        if w > 0 && h > 0 {
            left = left.min(pen + x);
            right = right.max(pen + x + w);
            top = top.min(-y - h);
            bottom = bottom.max(-y);
        }
        pen += a;
    }
    if left == i16::MAX {
        return (0, (height - 32) / 2);
    }
    (
        (pen - left - right) / 2,
        (height - (bottom - top)) / 2 - (26 + top),
    )
}

/// Each overview band covers one third of this same filesystem, not a separate chip.
pub fn band_used(used: u32, total: u32, index: u8) -> u32 {
    if total == 0 || index >= 3 {
        return 0;
    }
    ((u64::from(used.min(total)) * 3).saturating_sub(u64::from(index) * u64::from(total)))
        .min(u64::from(total)) as u32
}
#[cfg(test)]
mod visual_tests {
    use super::*;
    #[test]
    fn storage_bands_preserve_usage() {
        for total in [1, 3, 1000, 10223616, u32::MAX] {
            for used in [0, total / 100, total / 3, total / 2, total - 1, total] {
                let sum: u64 = (0..3).map(|i| u64::from(band_used(used, total, i))).sum();
                assert_eq!(sum, u64::from(used) * 3);
            }
        }
        assert_eq!(band_used(200, 100, 0), 100);
        assert_eq!(band_used(1, 0, 0), 0);
        assert_eq!(band_used(100, 100, 3), 0);
    }
    #[test]
    fn action_text_uses_visible_ink() {
        assert_eq!(button_label_position("Back", 72), (-1, 22));
        assert_eq!(button_label_position("Refresh", 72), (0, 22));
    }
}

/// Nominal advances for the compact24M legend labels.
pub fn legend_width(s: &str) -> i16 {
    match s {
        "Apps" => 57,
        "Faces" => 66,
        "Loader" => 77,
        "Other" => 63,
        "Free" => 51,
        "Used" => 57,
        _ => 100,
    }
}
