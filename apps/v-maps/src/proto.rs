//! The Vela ↔ watch protocol and the navigation model it fills. Pure: no `Api`, so `make test` covers it.
//!
//! Every message starts with the magic `n1`. Phone → watch messages then carry a kind byte and a `seq` byte that
//! counts every phone message (wrapping); a gap makes the watch ask for a snapshot again. All numbers are
//! little-endian.
//!
//! | Kind | Body | Meaning |
//! |---|---|---|
//! | `S` | flags u8, maneuver u8, dist u16 m, after u8, after dist u16 m, remaining u16 (10 m), remaining u16 s, limit u8, x i32 m, y i32 m, course u16, progress u16 | the drive, about once a second |
//! | `T` | UTF-8, ≤ 100 B | the road being entered (or the destination on arrival) |
//! | `B` | id u8, count u16, scale u8 (m per unit) | a new route line begins |
//! | `R` | id u8, start u16, n u8, n × (x i16, y i16) | route points `start..start+n`, n ≤ 30 |
//! | `E` | id u8, count u16 | the route line is complete |
//! | `N` | id u8, lines u8, scale u8, centre x i32, centre y i32 | nearby-road context begins |
//! | `G` | id u8, line u8, start u8, n u8, n × (x i16, y i16) | nearby-road points, n ≤ 28 |
//! | `O` | id u8, lines u8, points u16 | nearby-road context is complete |
//!
//! Nearby-road points are local metre offsets from the phone fix at the time of collection. The `N` centre is
//! that fix in the route's coordinate frame. This keeps the BLE channel free of latitude/longitude.
//!
//! Watch → phone: `n1Q`, "send me everything" (on start, after a gap, after `RESUME`, and while nothing arrives);
//! `n1H` and `n1W`, "take me Home" and "take me to Work".
//!
//! Positions are metres east (`x`) and north (`y`) of an origin the phone picks per drive, never latitude and
//! longitude: the channel can be read by anyone in Bluetooth range. `course` is the heading in 1/65536 turns
//! clockwise from north. `progress` is the index of the first route point still ahead of the car.

pub const ASK: [u8; 3] = *b"n1Q";
/// Watch → phone: "navigate to Home" and "to Work" (the places saved in Vela). A drive that is on is ended first.
pub const HOME: [u8; 3] = *b"n1H";
pub const WORK: [u8; 3] = *b"n1W";
pub const ROUTE_MAX: usize = 512;
pub const TEXT_MAX: usize = 100;
pub const CHUNK_MAX: usize = 30;
pub const ROADS_POINTS_MAX: usize = 480;
pub const ROADS_LINES_MAX: usize = 40;
pub const ROADS_CHUNK_MAX: usize = 28;

pub const F_ACTIVE: u8 = 1 << 0;
pub const F_ARRIVED: u8 = 1 << 1;
pub const F_IMPERIAL: u8 = 1 << 3;
pub const F_FIX: u8 = 1 << 4;
/// This state ends the snapshot the watch asked for with `n1Q`: everything before it in the run is current.
pub const F_SNAPSHOT: u8 = 1 << 6;

/// Maneuver codes, the same numbers as Vela's `DeviceManeuver`.
pub mod man {
    pub const CONTINUE: u8 = 1;
    pub const SLIGHT_LEFT: u8 = 2;
    pub const LEFT: u8 = 3;
    pub const SHARP_LEFT: u8 = 4;
    pub const SLIGHT_RIGHT: u8 = 5;
    pub const RIGHT: u8 = 6;
    pub const SHARP_RIGHT: u8 = 7;
    pub const KEEP_LEFT: u8 = 8;
    pub const KEEP_RIGHT: u8 = 9;
    pub const UTURN: u8 = 10;
    pub const ROUNDABOUT: u8 = 11;
    pub const OFF_ROUTE: u8 = 12;
    pub const ARRIVE: u8 = 13;
}

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct State {
    pub flags: u8,
    pub maneuver: u8,
    pub dist_m: u16,
    pub after: u8,
    pub after_m: u16,
    pub remaining_10m: u16,
    pub remaining_s: u16,
    pub limit: u8,
    pub x: i32,
    pub y: i32,
    pub course: u16,
    pub progress: u16,
}

impl State {
    pub fn active(&self) -> bool {
        self.flags & F_ACTIVE != 0
    }
    pub fn arrived(&self) -> bool {
        self.flags & F_ARRIVED != 0
    }
    pub fn imperial(&self) -> bool {
        self.flags & F_IMPERIAL != 0
    }
    pub fn fix(&self) -> bool {
        self.flags & F_FIX != 0
    }
}

/// What a message did to the model.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Effect {
    /// Not ours or malformed: ignored.
    Ignored,
    /// Applied; the screen is out of date.
    Changed,
    /// Messages were lost: ask the phone for a snapshot (`ASK`).
    Resync,
}

pub struct Model {
    pub state: State,
    /// A state arrived since the last `reset` (the screen shows "waiting" until then).
    pub has_state: bool,
    /// A state with `F_SNAPSHOT` arrived after the last gap: the watch has everything.
    pub synced: bool,
    text: [u8; TEXT_MAX],
    text_len: usize,
    pub route: [[i32; 2]; ROUTE_MAX],
    /// Points of a complete route line; 0 while none (or while a new one arrives).
    pub route_len: usize,
    route_id: Option<u8>,
    route_count: usize,
    route_got: usize,
    route_scale: i32,
    /// Nearby roads as local offsets from `roads_center`, with one start and length per polyline.
    pub roads: [[i16; 2]; ROADS_POINTS_MAX],
    pub roads_len: usize,
    pub roads_starts: [u16; ROADS_LINES_MAX],
    pub roads_sizes: [u8; ROADS_LINES_MAX],
    pub roads_count: usize,
    pub roads_center: [i32; 2],
    // a set being received: the streets already shown stay until the whole new set has arrived
    stage: [[i16; 2]; ROADS_POINTS_MAX],
    stage_len: usize,
    stage_starts: [u16; ROADS_LINES_MAX],
    stage_sizes: [u8; ROADS_LINES_MAX],
    stage_center: [i32; 2],
    /// One bit per street segment (by the index of its first point): the segment lies along the route, which
    /// is drawn over it, so it is not drawn. Worked out once per street set, not per frame.
    hidden: [u32; ROADS_POINTS_MAX.div_ceil(32)],
    roads_id: Option<u8>,
    roads_expected: usize,
    roads_got: usize,
    roads_points_expected: usize,
    last_seq: Option<u8>,
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}
fn i16_at(b: &[u8], i: usize) -> i16 {
    i16::from_le_bytes([b[i], b[i + 1]])
}
fn i32_at(b: &[u8], i: usize) -> i32 {
    i32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

impl Model {
    pub const fn new() -> Model {
        Model {
            state: State {
                flags: 0,
                maneuver: 0,
                dist_m: 0,
                after: 0,
                after_m: 0,
                remaining_10m: 0,
                remaining_s: 0,
                limit: 0,
                x: 0,
                y: 0,
                course: 0,
                progress: 0,
            },
            has_state: false,
            synced: false,
            text: [0; TEXT_MAX],
            text_len: 0,
            route: [[0; 2]; ROUTE_MAX],
            route_len: 0,
            route_id: None,
            route_count: 0,
            route_got: 0,
            route_scale: 1,
            roads: [[0; 2]; ROADS_POINTS_MAX],
            roads_len: 0,
            roads_starts: [0; ROADS_LINES_MAX],
            roads_sizes: [0; ROADS_LINES_MAX],
            roads_count: 0,
            roads_center: [0; 2],
            stage: [[0; 2]; ROADS_POINTS_MAX],
            stage_len: 0,
            stage_starts: [0; ROADS_LINES_MAX],
            stage_sizes: [0; ROADS_LINES_MAX],
            stage_center: [0; 2],
            hidden: [0; ROADS_POINTS_MAX.div_ceil(32)],
            roads_id: None,
            roads_expected: 0,
            roads_got: 0,
            roads_points_expected: 0,
            last_seq: None,
        }
    }

    /// Forget the sequence (after a pause or a resync request): the next message starts a new run.
    pub fn reset_seq(&mut self) {
        self.last_seq = None;
    }

    /// The road line, cut on a character boundary.
    pub fn text(&self) -> &str {
        cmf_wp2_ui::text::fit(&self.text[..self.text_len], TEXT_MAX)
    }

    pub fn apply(&mut self, m: &[u8]) -> Effect {
        let [b'n', b'1', kind, seq, body @ ..] = m else {
            return Effect::Ignored;
        };
        let gap = self.last_seq.is_some_and(|l| *seq != l.wrapping_add(1));
        self.last_seq = Some(*seq);
        if gap {
            self.synced = false;
        }
        let applied = match kind {
            b'S' => self.on_state(body),
            b'T' => self.on_text(body),
            b'B' => self.on_begin(body),
            b'R' => self.on_points(body),
            b'E' => self.on_end(body),
            b'N' => self.on_roads_begin(body),
            b'G' => self.on_roads_points(body),
            b'O' => self.on_roads_end(body),
            _ => Some(false),
        };
        match applied {
            None => {
                self.synced = false;
                Effect::Resync
            }
            Some(_) if gap => Effect::Resync,
            Some(true) => Effect::Changed,
            Some(false) => Effect::Ignored,
        }
    }

    // Each handler: Some(true) applied, Some(false) malformed (ignored), None the route broke (resync).

    fn on_state(&mut self, b: &[u8]) -> Option<bool> {
        if b.len() < 24 {
            return Some(false);
        }
        self.state = State {
            flags: b[0],
            maneuver: b[1],
            dist_m: u16_at(b, 2),
            after: b[4],
            after_m: u16_at(b, 5),
            remaining_10m: u16_at(b, 7),
            remaining_s: u16_at(b, 9),
            limit: b[11],
            x: i32_at(b, 12),
            y: i32_at(b, 16),
            course: u16_at(b, 20),
            progress: u16_at(b, 22),
        };
        self.has_state = true;
        if self.state.flags & F_SNAPSHOT != 0 {
            self.synced = true;
        }
        if !self.state.active() {
            // no drive: drop the line so a later drive never shows the previous one
            self.route_len = 0;
            self.route_id = None;
            self.roads_len = 0;
            self.roads_count = 0;
            self.roads_id = None;
        }
        Some(true)
    }

    fn on_text(&mut self, b: &[u8]) -> Option<bool> {
        let t = cmf_wp2_ui::text::fit(b, TEXT_MAX);
        self.text[..t.len()].copy_from_slice(t.as_bytes());
        self.text_len = t.len();
        Some(true)
    }

    fn on_begin(&mut self, b: &[u8]) -> Option<bool> {
        if b.len() < 4 {
            return Some(false);
        }
        let count = u16_at(b, 1) as usize;
        if count > ROUTE_MAX || b[3] == 0 {
            return Some(false);
        }
        (self.route_id, self.route_count, self.route_got, self.route_scale) = (Some(b[0]), count, 0, b[3] as i32);
        self.route_len = 0;
        Some(true)
    }

    fn on_points(&mut self, b: &[u8]) -> Option<bool> {
        if b.len() < 4 {
            return Some(false);
        }
        let (id, start, n) = (b[0], u16_at(b, 1) as usize, b[3] as usize);
        if self.route_id != Some(id) {
            self.route_len = 0; // a chunk of a line whose begin we missed: the line shown is old
            return None;
        }
        if b.len() < 4 + 4 * n || n > CHUNK_MAX || start != self.route_got || start + n > self.route_count {
            self.route_id = None;
            return None;
        }
        for i in 0..n {
            let at = 4 + 4 * i;
            self.route[start + i] =
                [i16_at(b, at) as i32 * self.route_scale, i16_at(b, at + 2) as i32 * self.route_scale];
        }
        self.route_got += n;
        Some(false) // nothing to show until the end
    }

    fn on_end(&mut self, b: &[u8]) -> Option<bool> {
        if b.len() < 3 {
            return Some(false);
        }
        if self.route_id != Some(b[0]) || u16_at(b, 1) as usize != self.route_got || self.route_got != self.route_count
        {
            self.route_id = None;
            return None;
        }
        self.route_len = self.route_got;
        self.hide_under_route();
        Some(true)
    }

    fn on_roads_begin(&mut self, b: &[u8]) -> Option<bool> {
        if b.len() < 11 {
            return Some(false);
        }
        let lines = b[1] as usize;
        let scale = b[2] as i32;
        if lines > ROADS_LINES_MAX || scale != 1 {
            return Some(false);
        }
        self.roads_id = Some(b[0]);
        self.roads_expected = lines;
        self.roads_got = 0;
        self.roads_points_expected = 0;
        self.stage_len = 0;
        self.stage_center = [i32_at(b, 3), i32_at(b, 7)];
        self.stage_starts = [0; ROADS_LINES_MAX];
        self.stage_sizes = [0; ROADS_LINES_MAX];
        Some(true)
    }

    fn on_roads_points(&mut self, b: &[u8]) -> Option<bool> {
        if b.len() < 4 {
            return Some(false);
        }
        let (id, line, start, n) = (b[0], b[1] as usize, b[2] as usize, b[3] as usize);
        if self.roads_id != Some(id) || line >= self.roads_expected {
            self.roads_id = None;
            return None;
        }
        if n == 0 || n > ROADS_CHUNK_MAX || b.len() < 4 + n * 4
            || start != self.stage_sizes[line] as usize
            || self.stage_len + n > ROADS_POINTS_MAX
            || start + n > u8::MAX as usize
        {
            self.roads_id = None;
            return None;
        }
        if start == 0 {
            self.stage_starts[line] = self.stage_len as u16;
            self.roads_got += 1;
        }
        for i in 0..n {
            let at = 4 + i * 4;
            self.stage[self.stage_len + i] = [i16_at(b, at), i16_at(b, at + 2)];
        }
        self.stage_len += n;
        self.stage_sizes[line] = (start + n) as u8;
        self.roads_points_expected += n;
        Some(false)
    }

    fn on_roads_end(&mut self, b: &[u8]) -> Option<bool> {
        if b.len() < 4 {
            return Some(false);
        }
        let (id, lines, points) = (b[0], b[1] as usize, u16_at(b, 2) as usize);
        if self.roads_id != Some(id) || lines != self.roads_expected || self.roads_got != lines
            || points != self.stage_len || points != self.roads_points_expected
            || self.stage_sizes[..lines].iter().any(|n| *n < 2)
        {
            self.roads_id = None;
            return None;
        }
        // the new set is whole: swap it in at once, the old streets stay on screen until this moment
        self.roads_len = self.stage_len;
        self.roads_center = self.stage_center;
        self.roads_starts = self.stage_starts;
        self.roads_sizes = self.stage_sizes;
        let n = self.stage_len;
        self.roads[..n].copy_from_slice(&self.stage[..n]);
        self.roads_count = lines;
        self.roads_id = None;
        self.hide_under_route();
        Some(true)
    }
    /// Whether the street segment starting at point `i` lies along the route (see `hidden`).
    pub fn under_route(&self, i: usize) -> bool {
        self.route_len >= 2 && self.hidden.get(i / 32).is_some_and(|w| w >> (i % 32) & 1 != 0)
    }

    /// Marks every street segment that lies within `UNDER_M` of the route, all of it: both ends and the middle.
    /// Each street line only meets the route segments whose box overlaps its own, so this stays cheap.
    fn hide_under_route(&mut self) {
        self.hidden = [0; ROADS_POINTS_MAX.div_ceil(32)];
        if self.route_len < 2 || self.roads_count == 0 {
            return;
        }
        let route = &self.route[..self.route_len];
        let at = |i: usize| [self.roads_center[0] + self.roads[i][0] as i32, self.roads_center[1] + self.roads[i][1] as i32];
        for l in 0..self.roads_count.min(ROADS_LINES_MAX) {
            let start = self.roads_starts[l] as usize;
            let end = (start + self.roads_sizes[l] as usize).min(self.roads_len);
            if end < start + 2 {
                continue;
            }
            let (mut lo, mut hi) = (at(start), at(start));
            for i in start..end {
                let p = at(i);
                (lo, hi) = ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]);
            }
            let (lo, hi) = ([lo[0] - UNDER_M, lo[1] - UNDER_M], [hi[0] + UNDER_M, hi[1] + UNDER_M]);
            let meets = |w: &[[i32; 2]]| {
                w[0][0].max(w[1][0]) >= lo[0] && w[0][0].min(w[1][0]) <= hi[0]
                    && w[0][1].max(w[1][1]) >= lo[1] && w[0][1].min(w[1][1]) <= hi[1]
            };
            let near = |p: [i32; 2]| route.windows(2).any(|w| meets(w) && seg_dist2(p, w[0], w[1]) <= (UNDER_M * UNDER_M) as i64);
            for i in start..end - 1 {
                let (a, b) = (at(i), at(i + 1));
                let mid = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
                if near(a) && near(b) && near(mid) {
                    self.hidden[i / 32] |= 1 << (i % 32);
                }
            }
        }
    }
}

/// How close to the route a street must be to be hidden under it (the route line is 16 px wide: 8 m at the
/// closest zoom).
const UNDER_M: i32 = 5;

/// Squared distance from `p` to the segment `a`..`b`.
fn seg_dist2(p: [i32; 2], a: [i32; 2], b: [i32; 2]) -> i64 {
    let (px, py, ax, ay) = (p[0] as i64, p[1] as i64, a[0] as i64, a[1] as i64);
    let (dx, dy) = (b[0] as i64 - ax, b[1] as i64 - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0 { 0 } else { (((px - ax) * dx + (py - ay) * dy) * 1024 / len2).clamp(0, 1024) };
    let (cx, cy) = (ax + dx * t / 1024, ay + dy * t / 1024);
    (px - cx) * (px - cx) + (py - cy) * (py - cy)
}

impl Default for Model {
    fn default() -> Self {
        Model::new()
    }
}

#[cfg(test)]
pub mod enc {
    //! The phone side, for tests: what Vela's encoder must produce.
    use super::*;
    use std::vec::Vec;

    pub fn msg(kind: u8, seq: u8, body: &[u8]) -> Vec<u8> {
        let mut v = vec![b'n', b'1', kind, seq];
        v.extend_from_slice(body);
        assert!(v.len() <= 128);
        v
    }

    pub fn state(seq: u8, s: &State) -> Vec<u8> {
        let mut b = vec![s.flags, s.maneuver];
        b.extend(s.dist_m.to_le_bytes());
        b.push(s.after);
        b.extend(s.after_m.to_le_bytes());
        b.extend(s.remaining_10m.to_le_bytes());
        b.extend(s.remaining_s.to_le_bytes());
        b.push(s.limit);
        b.extend(s.x.to_le_bytes());
        b.extend(s.y.to_le_bytes());
        b.extend(s.course.to_le_bytes());
        b.extend(s.progress.to_le_bytes());
        msg(b'S', seq, &b)
    }

    /// Begin, chunks and end for a line of points in metres; returns the messages and the next seq.
    pub fn route(mut seq: u8, id: u8, pts: &[[i32; 2]]) -> (Vec<Vec<u8>>, u8) {
        let max = pts.iter().flat_map(|p| p.iter()).map(|v| v.abs()).max().unwrap_or(0);
        let scale = (max / i16::MAX as i32 + 1) as u8;
        let mut out = Vec::new();
        let n = (pts.len() as u16).to_le_bytes();
        out.push(msg(b'B', seq, &[id, n[0], n[1], scale]));
        seq = seq.wrapping_add(1);
        for (c, chunk) in pts.chunks(CHUNK_MAX).enumerate() {
            let start = ((c * CHUNK_MAX) as u16).to_le_bytes();
            let mut b = vec![id, start[0], start[1], chunk.len() as u8];
            for p in chunk {
                b.extend(((p[0] / scale as i32) as i16).to_le_bytes());
                b.extend(((p[1] / scale as i32) as i16).to_le_bytes());
            }
            out.push(msg(b'R', seq, &b));
            seq = seq.wrapping_add(1);
        }
        out.push(msg(b'E', seq, &[id, n[0], n[1]]));
        (out, seq.wrapping_add(1))
    }


    /// Nearby road context, local offsets from the supplied centre. Each polyline is independent.
    pub fn roads(mut seq: u8, id: u8, centre: [i32; 2], lines: &[Vec<[i16; 2]>]) -> (Vec<Vec<u8>>, u8) {
        let safe: Vec<&Vec<[i16; 2]>> = lines.iter().filter(|l| l.len() >= 2 && l.len() <= 255).take(ROADS_LINES_MAX).collect();
        let mut out = Vec::new();
        let mut begin = vec![id, safe.len() as u8, 1];
        begin.extend(centre[0].to_le_bytes());
        begin.extend(centre[1].to_le_bytes());
        out.push(msg(b'N', seq, &begin));
        seq = seq.wrapping_add(1);
        let mut total = 0usize;
        for (line_idx, line) in safe.iter().enumerate() {
            for (chunk_idx, chunk) in line.chunks(ROADS_CHUNK_MAX).enumerate() {
                let start = chunk_idx * ROADS_CHUNK_MAX;
                let mut b = vec![id, line_idx as u8, start as u8, chunk.len() as u8];
                for p in chunk {
                    b.extend(p[0].to_le_bytes());
                    b.extend(p[1].to_le_bytes());
                }
                out.push(msg(b'G', seq, &b));
                seq = seq.wrapping_add(1);
                total += chunk.len();
            }
        }
        let mut end = vec![id, safe.len() as u8];
        end.extend((total as u16).to_le_bytes());
        out.push(msg(b'O', seq, &end));
        (out, seq.wrapping_add(1))
    }
}

#[cfg(test)]
mod tests {
    use super::enc::*;
    use super::*;
    use std::boxed::Box;

    fn active() -> State {
        State {
            flags: F_ACTIVE | F_FIX,
            maneuver: man::LEFT,
            dist_m: 350,
            x: -12,
            y: 40000,
            course: 16384,
            ..State::default()
        }
    }

    #[test]
    fn state_round_trips() {
        let mut m = Box::new(Model::new());
        let s = State {
            limit: 50,
            remaining_10m: 820,
            remaining_s: 840,
            after: man::RIGHT,
            after_m: 90,
            progress: 7,
            ..active()
        };
        assert_eq!(m.apply(&state(0, &s)), Effect::Changed);
        assert_eq!(m.state, s);
        assert!(m.has_state);
    }

    #[test]
    fn route_is_shown_only_when_complete() {
        let mut m = Box::new(Model::new());
        let pts: std::vec::Vec<[i32; 2]> = (0..100).map(|i| [i * 37 - 2000, i * i * 9]).collect();
        let (msgs, _) = route(0, 3, &pts);
        assert_eq!(msgs.len(), 1 + 4 + 1);
        for (i, msg) in msgs.iter().enumerate() {
            m.apply(msg);
            assert_eq!(m.route_len, if i + 1 == msgs.len() { 100 } else { 0 });
        }
        // max |v| is 89100 m, so scale 3: points come back within 3 m
        for (a, b) in m.route[..100].iter().zip(&pts) {
            assert!((a[0] - b[0]).abs() < 3 && (a[1] - b[1]).abs() < 3, "{a:?} {b:?}");
        }
    }

    #[test]
    fn a_lost_chunk_asks_again_and_shows_no_line() {
        let mut m = Box::new(Model::new());
        let pts: std::vec::Vec<[i32; 2]> = (0..90).map(|i| [i, -i]).collect();
        let (msgs, _) = route(10, 1, &pts);
        assert_eq!(m.apply(&msgs[0]), Effect::Changed);
        assert_eq!(m.apply(&msgs[1]), Effect::Ignored);
        assert_eq!(m.apply(&msgs[3]), Effect::Resync); // chunk 2 lost
        assert_eq!(m.apply(&msgs[4]), Effect::Resync);
        assert_eq!(m.route_len, 0);
    }

    #[test]
    fn seq_gap_and_wrap() {
        let mut m = Box::new(Model::new());
        assert_eq!(m.apply(&state(255, &active())), Effect::Changed);
        assert_eq!(m.apply(&state(0, &active())), Effect::Changed);
        assert_eq!(m.apply(&state(2, &active())), Effect::Resync);
        m.reset_seq();
        assert_eq!(m.apply(&state(9, &active())), Effect::Changed);
    }

    #[test]
    fn synced_only_by_a_snapshot_end() {
        let mut m = Box::new(Model::new());
        m.apply(&state(0, &active()));
        assert!(!m.synced);
        m.apply(&state(1, &State { flags: active().flags | F_SNAPSHOT, ..active() }));
        assert!(m.synced);
        m.apply(&state(5, &active())); // gap
        assert!(!m.synced);
    }

    #[test]
    fn ending_the_drive_drops_the_line() {
        let mut m = Box::new(Model::new());
        let (msgs, seq) = route(0, 1, &[[0, 0], [10, 10]]);
        msgs.iter().for_each(|x| {
            m.apply(x);
        });
        assert_eq!(m.route_len, 2);
        m.apply(&state(seq, &State::default()));
        assert_eq!(m.route_len, 0);
    }

    #[test]
    fn streets_along_the_route_are_hidden_and_crossings_are_not() {
        let mut m = Box::new(Model::new());
        let (r, seq) = route(0, 1, &[[0, 0], [0, 200], [0, 400]]);
        for msg in r { m.apply(&msg); }
        // a street 2 m beside the route (along it), one 30 m away (parallel), and one crossing it
        let lines = vec![
            vec![[2, 50], [2, 150], [2, 250]],
            vec![[30, 50], [30, 150]],
            vec![[-100, 100], [0, 100], [100, 100]],
        ];
        let (s, _) = roads(seq, 1, [0, 0], &lines);
        for msg in s { m.apply(&msg); }
        // points: line 0 = 0..3, line 1 = 3..5, line 2 = 5..8
        assert!(m.under_route(0) && m.under_route(1), "along the route");
        assert!(!m.under_route(3), "30 m away");
        assert!(!m.under_route(5) && !m.under_route(6), "a crossing street");
    }

    #[test]
    fn the_old_streets_stay_until_the_new_set_is_whole() {
        let mut m = Box::new(Model::new());
        let (a, seq) = roads(0, 1, [10, 20], &[vec![[0, 0], [50, 50]]]);
        for msg in a { m.apply(&msg); }
        let (b, _) = roads(seq, 2, [900, 900], &[vec![[1, 1], [2, 2], [3, 3]], vec![[5, 5], [6, 6]]]);
        let last = b.len() - 1;
        for msg in &b[..last] {
            m.apply(msg);
            assert_eq!((m.roads_count, m.roads_len, m.roads_center), (1, 2, [10, 20]), "old set still shown");
        }
        m.apply(&b[last]);
        assert_eq!((m.roads_count, m.roads_len, m.roads_center), (2, 5, [900, 900]));
        assert_eq!(m.roads[4], [6, 6]);
    }

    #[test]
    fn nearby_roads_round_trip_as_separate_lines() {
        let mut m = Box::new(Model::new());
        let lines = vec![
            vec![[0, 0], [120, 80], [240, 160]],
            vec![[-150, 300], [-100, 350]],
        ];
        let (msgs, _) = roads(0, 7, [1200, -3400], &lines);
        for msg in msgs { m.apply(&msg); }
        assert_eq!(m.roads_count, 2);
        assert_eq!(m.roads_len, 5);
        assert_eq!(m.roads_center, [1200, -3400]);
        assert_eq!(m.roads_sizes[0], 3);
        assert_eq!(m.roads_sizes[1], 2);
        assert_eq!(m.roads[0], [0, 0]);
        assert_eq!(m.roads[4], [-100, 350]);
    }

    #[test]
    fn empty_road_snapshot_clears_context() {
        let mut m = Box::new(Model::new());
        let lines = vec![vec![[0, 0], [100, 100]]];
        let (msgs, seq) = roads(0, 1, [0, 0], &lines);
        for msg in msgs { m.apply(&msg); }
        assert_eq!(m.roads_count, 1);
        let (clear, _) = roads(seq, 2, [0, 0], &[]);
        for msg in clear { m.apply(&msg); }
        assert_eq!(m.roads_count, 0);
        assert_eq!(m.roads_len, 0);
    }

    #[test]
    fn junk_is_ignored() {
        let mut m = Box::new(Model::new());
        for junk in [
            &b""[..],
            b"n",
            b"x1S\x00",
            b"n1S\x00\x01",
            b"n1Z\x00abc",
            b"n1B\x00\x01\xff\xff\x01",
            b"n1B\x00\x01\x02\x00\x00",
        ] {
            m.reset_seq();
            assert_eq!(m.apply(junk), Effect::Ignored, "{junk:?}");
        }
        assert!(!m.has_state);
    }

    #[test]
    fn text_is_cut_on_a_char_boundary() {
        let mut m = Box::new(Model::new());
        let mut long = std::vec::Vec::from(&b"Stra\xc3\x9fe "[..]);
        long.resize(99, b'a');
        long.extend_from_slice("ß".as_bytes()); // 2 bytes, crosses 100
        m.apply(&msg(b'T', 0, &long));
        assert_eq!(m.text().len(), 99);
        m.apply(&msg(b'T', 1, b"Main St\xff"));
        assert_eq!(m.text(), "Main St");
    }
}
