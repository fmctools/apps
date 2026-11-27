//! The music app's state and its message protocol. No `Api` in here, so `cargo test` runs it on the computer.
//!
//! Phone -> watch (first byte = kind, 1..=128 bytes):
//!   `C` count version                 a new list starts: `count` songs, list `version`
//!   `T` start n (tlen title alen artist)*   songs `start..start+n`, UTF-8
//!   `K` start n (c1 c2)*              the colour of songs `start..start+n` (two RGB565 LE; the pill uses `c1`)
//!   `Q` start n (4 x u16)*            the cover's four quarter colours (top-left, top-right, bottom-left, bottom-right;
//!                                     RGB565 LE) of songs `start..start+n`, for the player's background
//!   `N` index playing                 the song that plays now (`0xFF` = none) and whether it plays
//!   `S` playing                       play / pause changed
//!   `V` level muted                   volume 0..=100
//!   `G` pos dur                       seconds played and song length (u16 LE each); dur 0 = unknown
//!   `X` playing tlen title alen artist current external media metadata for Return to controls
//!   `H` flags                         home catalog: bit 0 Music, bit 1 Podcasts, bit 2 Custom, bit 3 active media,
//!                                     bit 4 open the active media's controls right now (cold launch while it plays)
//!   `R` flags                         refreshed home catalog (same flags, also asks the watch to re-evaluate its home)
//!   `W` mode index playing            restore the last watch-selected Music/Podcast row into its track-specific player
//!   `A`                               legacy "hello" acknowledgement
//!   `L` liked                         the song on the player is in the phone's saved songs (1) or not (0); sent after
//!                                     every song change and after `l`. A phone that never sends it: no heart.
//! Watch -> phone:
//!   `h` count version mode fresh player_resume launch_id:u32le
//!                                     hello; retries keep the same ID, a new launch or resume gets a new ID.
//!                                     `mode` = 0 music, 1 podcasts, 2 active media, 0xFF = the start screen.
//!                                     `fresh` marks a cold launch; `player_resume` marks a paused track-specific player.
//!                                     Choosing a category asks the phone for the matching list and controls
//!   `s` idx                           play this song (podcasts: the show, its last episode)
//!   `p` `n` `b` `m`                   play-pause / next / previous / mute (podcasts: next / previous episode)
//!   `v` level                         volume set to level 0..=100
//!   `k` lo hi                         seek to lo|hi<<8 thousandths of the song (double tap: -5 s / +5 s)
//!   `l`                               save / unsave the song on the player (the heart); the phone answers `L`

pub const MAX_TRACKS: usize = 50;
pub const TEXT_BYTES: usize = 40;
pub const NONE: u8 = 0xFF;
/// what the list on the watch is
pub const MODE_MUSIC: u8 = 0;
pub const MODE_PODCAST: u8 = 1;
/// the phone's currently active media session (not limited to a saved playlist)
pub const MODE_EXTERNAL: u8 = 2;
/// custom URL actions
pub const MODE_CUSTOM: u8 = 3;
/// the start screen (music or podcasts not chosen yet)
pub const MODE_NONE: u8 = 0xFF;
/// one crown click changes the volume by this many percent
pub const VOL_STEP: i16 = 5;
/// bytes of a song kept in the saved list
const SAVED_TEXT: usize = 24;
/// The phone sends "title, artist" in this order; `true` shows the first text as the artist and the second as the
/// song (the setting that matches the names as the phone app sends them today). Flip it if they look reversed.
/// Podcasts are never swapped: the phone sends the episode first and the show second.
pub const SWAP_NAMES: bool = false;
/// Turn the rows round that the phone could not decide (see `fix_order`).
pub const SWAP_UNSURE: bool = true;
pub const DEFAULT_C1: u16 = 0x2966;
pub const DEFAULT_C2: u16 = 0x4229;

#[derive(Clone, Copy)]
pub struct Track {
    pub tlen: u8,
    pub alen: u8,
    pub title: [u8; TEXT_BYTES],
    pub artist: [u8; TEXT_BYTES],
    pub c1: u16,
    pub c2: u16,
    /// the cover's quarters (top-left, top-right, bottom-left, bottom-right); `has_q` once the phone sent them
    pub q: [u16; 4],
    pub has_q: bool,
}

fn text(b: &[u8], len: u8) -> &str {
    core::str::from_utf8(&b[..len as usize]).unwrap_or("?")
}

impl Track {
    pub const EMPTY: Track =
        Track {
            tlen: 0,
            alen: 0,
            title: [0; TEXT_BYTES],
            artist: [0; TEXT_BYTES],
            c1: DEFAULT_C1,
            c2: DEFAULT_C2,
            q: [DEFAULT_C1; 4],
            has_q: false,
        };

    pub fn title(&self) -> &str {
        text(&self.title, self.tlen)
    }

    pub fn artist(&self) -> &str {
        text(&self.artist, self.alen)
    }
}

#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub struct Effect {
    pub list: bool,
    pub colours: bool,
    pub now: bool,
    pub volume: bool,
    pub progress: bool,
    pub saved: bool,
    /// The phone sent a refreshed home-category catalog.
    pub home: bool,
    /// The saved (heart) state of the song on the player changed.
    pub liked: bool,
}

pub struct Model {
    pub tracks: [Track; MAX_TRACKS],
    pub count: u8,
    pub version: u8,
    pub now: u8,
    pub playing: bool,
    pub level: u8,
    pub muted: bool,
    /// seconds played when `G` arrived, and the song length (0 = unknown)
    pub pos: u16,
    pub dur: u16,
    /// `MODE_MUSIC`, `MODE_PODCAST`, `MODE_CUSTOM`, `MODE_EXTERNAL` or `MODE_NONE` (the start screen)
    pub mode: u8,
    /// the phone said the list is empty (nothing to show yet)
    pub empty: bool,
    /// bit 0 Music, bit 1 Podcasts, bit 2 Custom, bit 3 an active external media session
    pub home_flags: u8,
    /// the song on the player is saved on the phone: [`LIKED_NO`], [`LIKED_YES`] or [`LIKED_UNKNOWN`] (the phone
    /// never said, so it has no saved songs: the heart stays hidden)
    pub liked: u8,
}

pub const LIKED_NO: u8 = 0;
pub const LIKED_YES: u8 = 1;
pub const LIKED_UNKNOWN: u8 = 0xFF;

fn clip_text(src: &[u8], max: usize) -> usize {
    // never cut a UTF-8 character in half
    let mut cut = src.len().min(max);
    while cut < src.len() && cut > 0 && (src[cut] & 0xC0) == 0x80 {
        cut -= 1;
    }
    cut
}

impl Model {
    pub const fn new() -> Model {
        Model {
            tracks: [Track::EMPTY; MAX_TRACKS],
            count: 0,
            version: 0,
            now: NONE,
            playing: false,
            level: 50,
            muted: false,
            pos: 0,
            dur: 0,
            mode: MODE_NONE,
            empty: false,
            home_flags: 0,
            liked: LIKED_UNKNOWN,
        }
    }

    /// Clears the large track table in place. Do not replace it with
    /// `self.tracks = [Track::EMPTY; MAX_TRACKS]`: on the watch that can
    /// materialize a multi-kilobyte temporary on the small app stack.
    fn clear_tracks(&mut self) {
        for track in self.tracks.iter_mut() {
            *track = Track::EMPTY;
        }
    }

    /// Resets a reused app instance without constructing a second full Model on the stack.
    pub fn reset(&mut self) {
        self.clear_tracks();
        self.count = 0;
        self.version = 0;
        self.now = NONE;
        self.playing = false;
        self.level = 50;
        self.muted = false;
        self.pos = 0;
        self.dur = 0;
        self.mode = MODE_NONE;
        self.empty = false;
        self.home_flags = 0;
        self.liked = LIKED_UNKNOWN;
    }

    /// The heart was tapped: flip it at once (the phone confirms with `L`). False if the phone has no saved songs.
    pub fn toggle_liked(&mut self) -> bool {
        match self.liked {
            LIKED_NO => self.liked = LIKED_YES,
            LIKED_YES => self.liked = LIKED_NO,
            _ => return false,
        }
        true
    }

    /// Where to seek for a double tap: `delta` seconds from `elapsed`, as thousandths of the song.
    /// `None` when the length is unknown (live streams, some players), so nothing is sent.
    pub fn seek_permille(&self, elapsed: u32, delta: i32) -> Option<(u16, u16)> {
        if self.dur == 0 {
            return None;
        }
        let dur = self.dur as i32;
        let target = (elapsed as i32 + delta).clamp(0, dur);
        Some(((target as i64 * 1000 / dur as i64) as u16, target as u16))
    }

    /// Switches to `mode` with an empty list: the phone sends the list of that mode next.
    pub fn pick(&mut self, mode: u8) {
        self.mode = mode;
        self.clear_tracks();
        self.count = 0;
        self.version = 0;
        self.now = NONE;
        self.playing = false;
        self.pos = 0;
        self.dur = 0;
        self.empty = false;
    }

    /// Whether the first text of a name pair is the artist (music) or the song (podcasts).
    fn swap(&self) -> bool {
        SWAP_NAMES && self.mode == MODE_MUSIC
    }

    pub fn title(&self, i: u8) -> &str {
        if i < self.count { self.tracks[i as usize].title() } else { "" }
    }

    pub fn artist(&self, i: u8) -> &str {
        if i < self.count { self.tracks[i as usize].artist() } else { "" }
    }

    /// The four quarter colours of song `i`'s cover; its pill colour in all four quarters until the phone sends them.
    pub fn quarters(&self, i: u8) -> [u16; 4] {
        if i >= self.count {
            return [DEFAULT_C1; 4];
        }
        let t = &self.tracks[i as usize];
        if t.has_q { t.q } else { [t.c1; 4] }
    }

    /// A cheap hash of what the list screen shows (mode, count, titles, artists). When the phone sends the same list
    /// again, the screen is not rebuilt.
    pub fn fingerprint(&self) -> u32 {
        fn put(h: u32, b: u8) -> u32 {
            (h ^ b as u32).wrapping_mul(16_777_619)
        }
        let mut h = put(put(2_166_136_261, self.mode), self.count);
        for t in &self.tracks[..self.count as usize] {
            for &c in &t.title[..t.tlen as usize] {
                h = put(h, c);
            }
            h = put(h, 0);
            for &c in &t.artist[..t.alen as usize] {
                h = put(h, c);
            }
            h = put(h, 1);
        }
        h
    }

    /// Applies one phone message.
    pub fn apply(&mut self, m: &[u8]) -> Effect {
        let mut fx = Effect::default();
        let Some(&kind) = m.first() else { return fx };
        match kind {
            b'L' if m.len() >= 2 => {
                let v = if m[1] != 0 { LIKED_YES } else { LIKED_NO };
                fx.liked = v != self.liked;
                self.liked = v;
            }
            b'H' | b'R' if m.len() >= 2 => {
                self.home_flags = m[1] & 0x1F;
                fx.home = true;
            }
            b'C' if m.len() >= 3 => {
                self.count = m[1].min(MAX_TRACKS as u8);
                self.version = m[2];
                self.empty = self.count == 0;
                for t in self.tracks.iter_mut() {
                    *t = Track::EMPTY;
                }
                if self.now != NONE && self.now >= self.count {
                    self.now = NONE;
                }
                fx.list = true;
            }
            b'T' if m.len() >= 3 => {
                let mut i = m[1] as usize;
                let mut n = m[2] as usize;
                let mut p = 3;
                let swap = self.swap();
                let mut touched = false;
                while n > 0 && p < m.len() {
                    let tl = m[p] as usize;
                    if p + 1 + tl >= m.len() {
                        break;
                    }
                    let al = m[p + 1 + tl] as usize;
                    if p + 2 + tl + al > m.len() {
                        break;
                    }
                    if i < MAX_TRACKS {
                        let first = &m[p + 1..p + 1 + tl];
                        let second = &m[p + 2 + tl..p + 2 + tl + al];
                        let (ts, as_) = if swap { (second, first) } else { (first, second) };
                        let (tc, ac) = (clip_text(ts, TEXT_BYTES), clip_text(as_, TEXT_BYTES));
                        let t = &mut self.tracks[i];
                        t.title[..tc].copy_from_slice(&ts[..tc]);
                        t.tlen = tc as u8;
                        t.artist[..ac].copy_from_slice(&as_[..ac]);
                        t.alen = ac as u8;
                        touched |= i == self.now as usize;
                    }
                    p += 2 + tl + al;
                    i += 1;
                    n -= 1;
                }
                fx.list = true;
                // the song that plays now got a new title (a podcast moved to another episode)
                fx.now |= touched;
                if i >= self.count as usize && self.count > 0 {
                    fx.saved = true;
                }
            }
            b'K' if m.len() >= 3 => {
                let mut i = m[1] as usize;
                let mut n = m[2] as usize;
                let mut p = 3;
                while n > 0 && p + 4 <= m.len() {
                    if i < MAX_TRACKS {
                        self.tracks[i].c1 = u16::from_le_bytes([m[p], m[p + 1]]);
                        self.tracks[i].c2 = u16::from_le_bytes([m[p + 2], m[p + 3]]);
                    }
                    p += 4;
                    i += 1;
                    n -= 1;
                }
                fx.colours = true;
            }
            b'Q' if m.len() >= 3 => {
                let mut i = m[1] as usize;
                let mut n = m[2] as usize;
                let mut p = 3;
                while n > 0 && p + 8 <= m.len() {
                    if i < MAX_TRACKS {
                        for k in 0..4 {
                            self.tracks[i].q[k] = u16::from_le_bytes([m[p + 2 * k], m[p + 2 * k + 1]]);
                        }
                        self.tracks[i].has_q = true;
                    }
                    p += 8;
                    i += 1;
                    n -= 1;
                }
                fx.colours = true;
            }
            b'N' if m.len() >= 3 => {
                let idx = if m[1] < self.count { m[1] } else { NONE };
                let playing = m[2] != 0;
                if idx != self.now || playing != self.playing {
                    fx.now = true;
                }
                self.now = idx;
                self.playing = playing;
            }
            b'S' if m.len() >= 2 => {
                let p = m[1] != 0;
                if p != self.playing {
                    self.playing = p;
                    fx.now = true;
                }
            }
            b'V' if m.len() >= 4 => {
                self.level = m[1].min(100);
                self.muted = m[2] != 0;
                fx.volume = m[3] != 0;
            }
            b'G' if m.len() >= 5 => {
                self.pos = u16::from_le_bytes([m[1], m[2]]);
                self.dur = u16::from_le_bytes([m[3], m[4]]);
                fx.progress = true;
            }
            // The selected system media session can be a video, browser tab, or any other player.
            // Unlike T, X is a single live item and must never be saved as part of the playlist.
            b'X' if m.len() >= 4 => {
                let playing = m[1] != 0;
                let tl = m[2] as usize;
                let title_start: usize = 3;
                let artist_len_at = title_start.saturating_add(tl);
                if artist_len_at < m.len() {
                    let al = m[artist_len_at] as usize;
                    let artist_start = artist_len_at + 1;
                    if artist_start + al <= m.len() {
                        let title = &m[title_start..title_start + tl];
                        let artist = &m[artist_start..artist_start + al];
                        let (tc, ac) = (clip_text(title, TEXT_BYTES), clip_text(artist, TEXT_BYTES));
                        // X status snapshots arrive repeatedly while the active session is open. Preserve its
                        // album palette for the same item; clear it only when the active title/artist changes.
                        let same_item = self.mode == MODE_EXTERNAL
                            && self.count == 1
                            && self.tracks[0].tlen as usize == tc
                            && self.tracks[0].alen as usize == ac
                            && self.tracks[0].title[..tc] == title[..tc]
                            && self.tracks[0].artist[..ac] == artist[..ac];
                        let t = &mut self.tracks[0];
                        t.title = [0; TEXT_BYTES];
                        t.artist = [0; TEXT_BYTES];
                        t.title[..tc].copy_from_slice(&title[..tc]);
                        t.artist[..ac].copy_from_slice(&artist[..ac]);
                        t.tlen = tc as u8;
                        t.alen = ac as u8;
                        if !same_item {
                            t.c1 = DEFAULT_C1;
                            t.c2 = DEFAULT_C2;
                            t.q = [DEFAULT_C1; 4];
                            t.has_q = false;
                        }
                        self.count = 1;
                        self.now = 0;
                        self.playing = playing;
                        self.mode = MODE_EXTERNAL;
                        fx.list = true;
                        fx.now = true;
                    }
                }
            }
            _ => {}
        }
        fx
    }

    /// The list as a blob for `Api::save` (at most 2560 bytes).
    pub fn save(&self, out: &mut [u8]) -> usize {
        out[0] = b'W';
        out[1] = b'M';
        out[2] = 4;
        out[3] = self.count;
        out[4] = self.version;
        let mut p = 5;
        for t in &self.tracks[..self.count as usize] {
            for (len, src) in [(t.tlen, &t.title), (t.alen, &t.artist)] {
                let n = clip_text(&src[..len as usize], SAVED_TEXT);
                out[p] = n as u8;
                out[p + 1..p + 1 + n].copy_from_slice(&src[..n]);
                p += 1 + n;
            }
        }
        p
    }

    /// Music names come as "Song - Artist" or "Artist - Song". The phone decides when a name comes back in other
    /// rows (an artist has several songs); the rows it could not decide arrive as (artist, song) and are turned
    /// round here: a row whose artist text occurs only once in the list is swapped.
    pub fn fix_order(&mut self) {
        if !SWAP_UNSURE || self.mode == MODE_PODCAST {
            return;
        }
        let n = self.count as usize;
        let same = |a: &str, b: &str| a.trim().eq_ignore_ascii_case(b.trim());
        let mut swap = [false; MAX_TRACKS];
        for (i, flag) in swap.iter_mut().enumerate().take(n) {
            let a = self.tracks[i].artist();
            if a.is_empty() {
                continue;
            }
            let mut seen = 0;
            for t in &self.tracks[..n] {
                seen += same(a, t.artist()) as u32 + same(a, t.title()) as u32;
            }
            *flag = seen < 2;
        }
        for (i, &sw) in swap.iter().enumerate().take(n) {
            if sw {
                let t = &mut self.tracks[i];
                core::mem::swap(&mut t.title, &mut t.artist);
                core::mem::swap(&mut t.tlen, &mut t.alen);
            }
        }
    }

    pub fn load(&mut self, b: &[u8]) {
        if b.len() < 5 || &b[..3] != b"WM\x04" || b[3] as usize > MAX_TRACKS {
            return;
        }
        let count = b[3] as usize;
        // Validate the entire blob first so a truncated save never partially replaces a good list.
        // This first pass deliberately uses only scalars; a local `[Track; 50]` is several KB on the watch stack.
        let mut check = 5usize;
        for _ in 0..count {
            for _ in 0..2 {
                let Some(&n) = b.get(check) else { return };
                let n = n as usize;
                if n > TEXT_BYTES || check + 1 + n > b.len() {
                    return;
                }
                check += 1 + n;
            }
        }

        self.clear_tracks();
        let mut p: usize = 5;
        for i in 0..count {
            let track = &mut self.tracks[i];
            let title_len = b[p] as usize;
            p += 1;
            track.title[..title_len].copy_from_slice(&b[p..p + title_len]);
            track.tlen = title_len as u8;
            p += title_len;

            let artist_len = b[p] as usize;
            p += 1;
            track.artist[..artist_len].copy_from_slice(&b[p..p + artist_len]);
            track.alen = artist_len as u8;
            p += artist_len;
        }
        self.count = count as u8;
        self.version = b[4];
        self.empty = false;
    }
}

impl Model {
    /// Seconds played now: the phone's number plus the `since_ms` that passed since it came.
    pub fn elapsed(&self, since_ms: u32) -> u32 {
        let e = self.pos as u32 + if self.playing { since_ms / 1000 } else { 0 };
        if self.dur > 0 { e.min(self.dur as u32) } else { e }
    }
}

/// Position of `secs` in a song of `dur` seconds, 0..=1000 (0 when the length is unknown).
pub fn permille(secs: u32, dur: u16) -> i32 {
    if dur == 0 { 0 } else { (secs.min(dur as u32) * 1000 / dur as u32) as i32 }
}

/// The volume after one crown click (`delta` is +1 or -1): `VOL_STEP` percent, kept in 0..=100.
pub fn step_volume(level: u8, delta: i16) -> u8 {
    (level as i16 + delta.signum() * VOL_STEP).clamp(0, 100) as u8
}

/// "Vol 65%".
pub fn volume_text(level: u8, buf: &mut [u8; 8]) -> &str {
    let v = level.min(100);
    buf[..4].copy_from_slice(b"Vol ");
    let mut p = 4;
    if v >= 100 {
        buf[4..7].copy_from_slice(b"100");
        p += 3;
    } else if v >= 10 {
        buf[p] = b'0' + v / 10;
        buf[p + 1] = b'0' + v % 10;
        p += 2;
    } else {
        buf[p] = b'0' + v;
        p += 1;
    }
    buf[p] = b'%';
    core::str::from_utf8(&buf[..p + 1]).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(buf: &mut [u8], p: &mut usize, title: &str, artist: &str) {
        for s in [title, artist] {
            buf[*p] = s.len() as u8;
            buf[*p + 1..*p + 1 + s.len()].copy_from_slice(s.as_bytes());
            *p += 1 + s.len();
        }
    }

    fn list(m: &mut Model, songs: &[(&str, &str)]) {
        m.apply(&[b'C', songs.len() as u8, 7]);
        let mut msg = [0u8; 128];
        msg[0] = b'T';
        msg[2] = songs.len() as u8;
        let mut p = 3;
        for (t, a) in songs {
            entry(&mut msg, &mut p, t, a);
        }
        m.apply(&msg[..p]);
    }

    #[test]
    fn list_with_artists_arrives_and_saves() {
        let mut m = Model::new();
        list(&mut m, &[("One", "Ann"), ("Два", "Борис"), ("Three", "")]);
        assert_eq!((m.count, m.version), (3, 7));
        // the phone sends (first, second); SWAP_NAMES decides which one is the song
        let (song, artist) = if SWAP_NAMES { ("Борис", "Два") } else { ("Два", "Борис") };
        assert_eq!((m.title(1), m.artist(1)), (song, artist));
        let mut blob = [0u8; 2560];
        let n = m.save(&mut blob);
        let mut again = Model::new();
        again.load(&blob[..n]);
        assert_eq!((again.count, again.version, again.title(1), again.artist(1)), (3, 7, song, artist));
    }

    #[test]
    fn reset_clears_model_in_place_and_restores_defaults() {
        let mut m = Model::new();
        list(&mut m, &[("A song", "An artist"), ("Another", "Artist")]);
        m.apply(&[b'N', 1, 1]);
        m.apply(&[b'V', 82, 1, 1]);
        m.apply(&[b'G', 10, 0, 40, 0]);
        m.apply(&[b'H', 0x0F]);

        m.reset();

        assert_eq!(m.count, 0);
        assert_eq!(m.version, 0);
        assert_eq!(m.now, NONE);
        assert!(!m.playing);
        assert_eq!(m.level, 50);
        assert!(!m.muted);
        assert_eq!((m.pos, m.dur), (0, 0));
        assert_eq!(m.mode, MODE_NONE);
        assert!(!m.empty);
        assert_eq!(m.home_flags, 0);
        assert!(m.tracks.iter().all(|t| t.tlen == 0 && t.alen == 0));
    }

    #[test]
    fn fifty_songs_fit_the_saved_blob() {
        let mut m = Model::new();
        m.count = 50;
        for t in m.tracks.iter_mut() {
            t.tlen = 40;
            t.alen = 40;
        }
        let mut blob = [0u8; 2560];
        assert!(m.save(&mut blob) <= 2560);
    }

    #[test]
    fn colours_arrive_for_a_range() {
        let mut m = Model::new();
        list(&mut m, &[("A", ""), ("B", ""), ("C", "")]);
        assert_eq!(m.tracks[1].c1, DEFAULT_C1);
        let fx = m.apply(&[b'K', 1, 2, 0x34, 0x12, 0x78, 0x56, 0xBC, 0x9A, 0xF0, 0xDE]);
        assert!(fx.colours);
        assert_eq!((m.tracks[1].c1, m.tracks[1].c2), (0x1234, 0x5678));
        assert_eq!((m.tracks[2].c1, m.tracks[2].c2), (0x9ABC, 0xDEF0));
        assert_eq!(m.tracks[0].c1, DEFAULT_C1);
        m.apply(&[b'K', 0, 5, 1, 2]); // cut short: nothing breaks
    }

    #[test]
    fn quarters_arrive_and_default_to_the_pill_colour() {
        let mut m = Model::new();
        list(&mut m, &[("A", ""), ("B", ""), ("C", "")]);
        assert_eq!(m.quarters(1), [DEFAULT_C1; 4]);
        let fx = m.apply(&[b'Q', 1, 2, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0, 8, 0]);
        assert!(fx.colours);
        assert_eq!(m.quarters(1), [1, 2, 3, 4]);
        assert_eq!(m.quarters(2), [5, 6, 7, 8]);
        assert_eq!(m.quarters(0), [DEFAULT_C1; 4]);
        assert_eq!(m.quarters(9), [DEFAULT_C1; 4]);
        m.apply(&[b'Q', 0, 3, 1, 2, 3]); // cut short: nothing breaks
        assert_eq!(m.quarters(0), [DEFAULT_C1; 4]);
        m.apply(&[b'C', 3, 1]); // a new list forgets them
        assert_eq!(m.quarters(1), [DEFAULT_C1; 4]);
    }

    #[test]
    fn saved_flag_only_when_complete() {
        let mut m = Model::new();
        m.apply(&[b'C', 2, 1]);
        assert!(!m.apply(&[b'T', 0, 1, 1, b'a', 0]).saved);
        assert!(m.apply(&[b'T', 1, 1, 1, b'b', 0]).saved);
    }

    #[test]
    fn repeated_external_status_keeps_album_quarter_colours() {
        let mut m = Model::new();
        let title = b"Video title";
        let artist = b"Video app";
        let mut x = vec![b'X', 1, title.len() as u8];
        x.extend_from_slice(title);
        x.push(artist.len() as u8);
        x.extend_from_slice(artist);
        m.apply(&x);
        let q = [0xF800u16, 0x07E0, 0x001F, 0xFFFF];
        let mut qm = vec![b'Q', 0, 1];
        for c in q { qm.extend_from_slice(&c.to_le_bytes()); }
        m.apply(&qm);
        assert_eq!(m.quarters(0), q);

        // Same title/artist with a play-state change is only a status update, not a new cover.
        x[1] = 0;
        m.apply(&x);
        assert_eq!(m.quarters(0), q);
    }

    #[test]
    fn external_media_message_replaces_the_player_metadata_without_a_playlist() {
        let mut m = Model::new();
        m.pick(MODE_EXTERNAL);
        let title = b"A video playing";
        let artist = b"Browser";
        let mut msg = vec![b'X', 1, title.len() as u8];
        msg.extend_from_slice(title);
        msg.push(artist.len() as u8);
        msg.extend_from_slice(artist);
        let fx = m.apply(&msg);
        assert!(fx.list && fx.now);
        assert_eq!(m.mode, MODE_EXTERNAL);
        assert_eq!(m.count, 1);
        assert_eq!(m.now, 0);
        assert_eq!((m.title(0), m.artist(0)), ("A video playing", "Browser"));
        assert!(m.playing);

        let paused = [b'X', 0, 8, b'N', b'o', b't', b'h', b'i', b'n', b'g', b' ', 0, 0];
        let fx = m.apply(&paused);
        assert!(fx.now);
        assert_eq!(m.title(0), "Nothing ");
        assert!(!m.playing);
    }

    #[test]
    fn home_catalog_messages_preserve_mode_and_decode_category_bits() {
        let mut m = Model::new();
        let fx = m.apply(&[b'H', 0x0D]);
        assert!(fx.home);
        assert_eq!(m.home_flags, 0x0D);
        assert_eq!(m.mode, MODE_NONE);
        m.pick(MODE_CUSTOM);
        let fx = m.apply(&[b'R', 0x06]);
        assert!(fx.home);
        assert_eq!(m.home_flags, 0x06);
        assert_eq!(m.mode, MODE_CUSTOM);
    }

    #[test]
    fn home_catalog_keeps_the_open_controls_bit() {
        let mut m = Model::new();
        m.apply(&[b'H', 0x18]);
        assert_eq!(m.home_flags, 0x18);
        m.apply(&[b'H', 0xFF]);
        assert_eq!(m.home_flags, 0x1F);
    }

    #[test]
    fn restore_row_arrives_before_the_rest_of_the_list() {
        let mut m = Model::new();
        m.apply(&[b'W']); // too short: ignored by the model, handled by main
        m.pick(MODE_MUSIC);
        m.apply(&[b'C', 30, 1]);
        let mut msg = vec![b'T', 17, 1, 1, b'a', 1, b'b'];
        m.apply(&msg);
        assert!(m.tracks[17].tlen > 0);
        assert_eq!(m.tracks[3].tlen, 0);
        msg.clear();
    }

    #[test]
    fn bad_input_is_ignored() {
        let mut m = Model::new();
        m.load(b"junk");
        m.load(&[b'W', b'M', 4, 2, 0, 200]);
        assert_eq!(m.count, 0);
        assert_eq!(m.apply(&[]), Effect::default());
        assert_eq!(m.apply(&[b'Z', 1, 2, 3]), Effect::default());
        assert_eq!(m.apply(&[b'T', 0]), Effect::default());
        m.apply(&[b'C', 2, 1]);
        m.apply(&[b'T', 0, 1, 200, b'a']); // title longer than the message
        assert_eq!(m.title(0), "");
        assert_eq!(m.apply(&[b'G', 1, 0]), Effect::default());
    }

    #[test]
    fn now_playing_volume_progress_and_play_state() {
        let mut m = Model::new();
        list(&mut m, &[("A", ""), ("B", ""), ("C", "")]);
        assert!(m.apply(&[b'N', 1, 1]).now);
        assert_eq!((m.now, m.playing), (1, true));
        assert!(m.apply(&[b'S', 0]).now && !m.playing);
        assert!(!m.apply(&[b'N', 1, 0]).now);
        m.apply(&[b'N', 9, 1]);
        assert_eq!(m.now, NONE);
        let fx = m.apply(&[b'V', 130, 1, 1]);
        assert!(fx.volume);
        assert_eq!((m.level, m.muted), (100, true));
        assert!(m.apply(&[b'G', 0x2C, 0x01, 0xB4, 0x00]).progress);
        assert_eq!((m.pos, m.dur), (300, 180));
    }

    #[test]
    fn long_titles_are_cut_on_a_character() {
        let mut m = Model::new();
        m.apply(&[b'C', 1, 1]);
        let title = "ы".repeat(30); // 60 bytes
        let mut msg = vec![b'T', 0, 1, title.len() as u8];
        msg.extend_from_slice(title.as_bytes());
        msg.push(0);
        m.apply(&msg);
        let long = if SWAP_NAMES { m.artist(0) } else { m.title(0) };
        assert_eq!(long.chars().count(), 20);
    }

    #[test]
    fn podcasts_are_not_swapped_and_a_new_episode_reaches_the_player() {
        let mut m = Model::new();
        m.pick(MODE_PODCAST);
        assert_eq!((m.mode, m.count), (MODE_PODCAST, 0));
        list(&mut m, &[("Episode 12", "The Show"), ("Other episode", "Other Show")]);
        // the phone sends the episode first and the show second: the episode is the song line
        assert_eq!((m.title(0), m.artist(0)), ("Episode 12", "The Show"));
        m.apply(&[b'N', 0, 1]);
        assert_eq!(m.now, 0);
        // the phone moved to another episode of show 0: only that row is sent again
        let mut msg = [0u8; 64];
        msg[..3].copy_from_slice(&[b'T', 0, 1]);
        let mut p = 3;
        entry(&mut msg, &mut p, "Episode 13", "The Show");
        let fx = m.apply(&msg[..p]);
        assert!(fx.now && fx.list);
        assert_eq!(m.title(0), "Episode 13");
        // a row that does not play now does not ask for a redraw of the player
        msg[1] = 1;
        assert!(!m.apply(&msg[..p]).now);
        // music is swapped again after `pick`
        m.pick(MODE_MUSIC);
        list(&mut m, &[("First", "Second")]);
        assert_eq!(m.title(0), if SWAP_NAMES { "Second" } else { "First" });
    }

    #[test]
    fn empty_list_and_pick() {
        let mut m = Model::new();
        assert_eq!(m.mode, MODE_NONE);
        m.pick(MODE_PODCAST);
        assert!(!m.empty);
        m.apply(&[b'C', 0, 3]);
        assert!(m.empty && m.count == 0);
        m.apply(&[b'C', 2, 3]);
        assert!(!m.empty);
        m.apply(&[b'N', 1, 1]);
        m.pick(MODE_MUSIC);
        assert_eq!((m.count, m.now, m.playing), (0, NONE, false));
    }

    #[test]
    fn volume_steps_and_text() {
        assert_eq!(step_volume(50, 1), 55);
        assert_eq!(step_volume(50, -1), 45);
        assert_eq!(step_volume(98, 1), 100);
        assert_eq!(step_volume(2, -1), 0);
        assert_eq!(step_volume(0, -1), 0);
        assert_eq!(step_volume(40, 0), 40);
        let mut b = [0u8; 8];
        assert_eq!(volume_text(0, &mut b), "Vol 0%");
        assert_eq!(volume_text(5, &mut b), "Vol 5%");
        assert_eq!(volume_text(65, &mut b), "Vol 65%");
        assert_eq!(volume_text(100, &mut b), "Vol 100%");
        assert_eq!(volume_text(250, &mut b), "Vol 100%");
    }

    #[test]
    fn unsure_rows_are_turned_round() {
        let mut m = Model::new();
        m.pick(MODE_MUSIC);
        // phone output: a decided row (artist Ann has two songs), and two undecided ones sent as (artist, song)
        list(&mut m, &[("Song A", "Ann"), ("Song B", "Ann"), ("Bob", "Lonely Song"), ("Cy", "Other")]);
        m.fix_order();
        assert_eq!((m.title(0), m.artist(0)), ("Song A", "Ann"));
        assert_eq!((m.title(1), m.artist(1)), ("Song B", "Ann"));
        assert_eq!((m.title(2), m.artist(2)), ("Lonely Song", "Bob"));
        assert_eq!((m.title(3), m.artist(3)), ("Other", "Cy"));
        m.pick(MODE_PODCAST);
        list(&mut m, &[("Episode", "Show")]);
        m.fix_order();
        assert_eq!(m.title(0), "Episode");
    }

    #[test]
    fn elapsed_and_progress() {
        let mut m = Model::new();
        m.apply(&[b'C', 1, 1]);
        m.apply(&[b'N', 0, 1]);
        m.apply(&[b'G', 10, 0, 100, 0]);
        assert_eq!(m.elapsed(2500), 12);
        assert_eq!(m.elapsed(500_000), 100); // never past the end
        m.playing = false;
        assert_eq!(m.elapsed(5000), 10);
        assert_eq!(permille(50, 100), 500);
        assert_eq!(permille(5, 0), 0);
    }

    #[test]
    fn fingerprint_changes_only_with_what_the_list_shows() {
        let mut m = Model::new();
        m.apply(&[b'C', 2, 1]);
        m.apply(&[b'T', 0, 2, 1, b'a', 1, b'x', 1, b'b', 1, b'y']);
        let f = m.fingerprint();
        m.apply(&[b'C', 2, 1]);
        m.apply(&[b'T', 0, 2, 1, b'a', 1, b'x', 1, b'b', 1, b'y']);
        assert_eq!(m.fingerprint(), f);
        m.apply(&[b'K', 0, 1, 1, 0, 2, 0]);
        assert_eq!(m.fingerprint(), f);
        m.apply(&[b'T', 1, 1, 1, b'c', 1, b'y']);
        assert_ne!(m.fingerprint(), f);
    }

    #[test]
    fn heart_follows_the_phone_and_flips_on_tap() {
        let mut m = Model::new();
        assert_eq!(m.liked, LIKED_UNKNOWN);
        assert!(!m.toggle_liked(), "no heart until the phone says it keeps saved songs");
        assert!(m.apply(&[b'L', 1]).liked);
        assert_eq!(m.liked, LIKED_YES);
        assert!(!m.apply(&[b'L', 1]).liked, "same state again: nothing to redraw");
        assert!(m.toggle_liked());
        assert_eq!(m.liked, LIKED_NO);
        m.reset();
        assert_eq!(m.liked, LIKED_UNKNOWN);
    }

    #[test]
    fn double_tap_seek_is_clamped_and_needs_a_length() {
        let mut m = Model::new();
        assert_eq!(m.seek_permille(10, 5), None);
        m.dur = 200;
        assert_eq!(m.seek_permille(100, 5), Some((525, 105)));
        assert_eq!(m.seek_permille(3, -5), Some((0, 0)));
        assert_eq!(m.seek_permille(198, 5), Some((1000, 200)));
    }
}
