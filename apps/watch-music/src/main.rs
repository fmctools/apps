//! Watch Music: the phone's Music, Podcasts, or Custom actions on the watch, in the stock look (ui-gallery widgets).
//! A dynamic start screen shows only categories with saved entries; songs and shows are a focus list, and a tap opens
//! the media-control player. Custom actions run on the phone, and Return to controls opens the active media session.
//! In the player the volume button turns the crown into a volume control for 30 seconds. The message formats are
//! listed at the top of model.rs.
#![cfg_attr(any(target_os = "none", wp2_sim), no_main, cfg_attr(target_os = "none", no_std))]

#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod model;
#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod screens;

#[cfg(not(any(target_os = "none", wp2_sim)))]
fn main() {}

#[cfg(any(target_os = "none", wp2_sim))]
mod device {
    use crate::model::*;
    use crate::screens::{self, Kind, Screen};
    use cmf_wp2_app::lv::{Batch, FOCUSED, Lv, LvError};
    use cmf_wp2_app::{Api, BUTTON, CROWN, LV, MESSAGE, MSG_MAX, PAUSE, RESUME};
    use cmf_wp2_ui::haptic::{Motor, Pattern, TICK};

    cmf_wp2_app::entry!(main);
    cmf_wp2_app::app_icon!();
    cmf_wp2_app::app_features!(cmf_wp2_app::PAGE);

    const CLICKED: i16 = 7;
    const LONG_PRESSED: i16 = 5;
    /// two taps on the same half of the player within this time = a double tap (seek)
    const DOUBLE_TAP_MS: u32 = 350;
    /// how far a double tap seeks
    const SEEK_S: i32 = 5;
    /// after a long press (back), the tap that LVGL still sends on release is ignored for this long
    const LONG_GUARD_MS: u32 = 900;
    /// how soon the hello is sent again while the phone has not answered; each try waits twice as long, up to the max
    /// (a steady 0.4 s flood made the phone answer every copy, and every answer rebuilt the screen)
    const HELLO_MS: u32 = 400;
    const HELLO_MAX_MS: u32 = 2000;
    /// the phone can open or close a screen by itself (a lone category, the media controls) at most this often
    const AUTO_GAP_MS: u32 = 4000;
    /// how long a restore may take before the watch asks the phone for it again
    const RESUME_MS: u32 = 2500;
    /// taps on the start screen's rows are ignored this long after it appears
    const TAP_GUARD_MS: u32 = 600;
    /// the player looks at the clock this often while the song is paused
    const IDLE_MS: u32 = 5000;
    /// the volume stays on the crown this long after the last touch or turn
    /// the soft tick of a list scroll
    const SOFT: Pattern = Pattern::new(&[(170, 14), (0, 0)]);
    /// at most one scroll tick this often, and none for this long after the screen or the phone changed something
    const TICK_GAP_MS: u32 = 220;
    const QUIET_MS: u32 = 900;
    const VOL_MS: u32 = 1_000;
    /// the tide is drawn as fast as the watch can, but not more often than this
    const TIDE_MS: u32 = 25;
    /// the volume goes to the phone at most this often while the crown turns (the last value is always sent)
    const VOL_GAP_MS: u32 = 120;
    /// A burst of native FOCUSED events is settled before its logical anchor is committed. Fast page swipes can
    /// enqueue several intermediate centre rows; only the final stable row is retained.
    const FOCUS_SETTLE_MS: u32 = 100;
    /// Focus events queued by the screen being replaced must not be applied to the newly-created list (LVGL handles
    /// are reused by the batch allocator).
    const FOCUS_REBUILD_GUARD_MS: u32 = 80;

    static mut MODEL: Model = Model::new();
    static mut STORE: [u8; 2560] = [0; 2560];

    fn after(now: u32, t: u32) -> bool {
        (now.wrapping_sub(t) as i32) >= 0
    }

    #[allow(dead_code)]
    struct App {
        api: &'static Api,
        lv: Lv,
        b: Batch,
        s: Screen,
        motor: Motor,
        m: &'static mut Model,
        /// when the phone's progress (`pos`) was received
        prog_at: u32,
        /// what the ring and the clock show now (so nothing is sent to the screen when they did not change)
        last_pct: i32,
        last_min: u16,
        /// true when this app instance is running as a page beside the watch face
        page: bool,
        /// the phone has answered the last hello (no more hellos until PAUSE or a new choice)
        heard: bool,
        /// the tide of the player background: asleep while the page is paused, next frame's time
        paused: bool,
        tide_next: u32,
        next_hello: u32,
        /// identifies one logical hello exchange; retries keep this ID, a new exchange gets a new ID
        hello_id: u32,
        /// when to ask again for a restore that has not arrived (0 = none pending)
        resume_at: u32,
        /// a tap on a start-screen row is ignored until then (a second tap on Back must not land on a row)
        tap_guard: u32,
        /// the phone has answered the last start-screen hello with its catalog
        got_catalog: bool,
        /// the volume is on the crown until this time (`None`: it is not)
        vol_until: Option<u32>,
        quiet_until: u32,
        last_tick: u32,
        /// checksum of the list last written to flash (0: none yet this run)
        saved_sum: u32,
        /// a crown turn has not been sent to the phone yet
        vol_dirty: bool,
        vol_sent_at: u32,
        /// the last crown turn: the phone's echo of our own change is ignored for a moment
        vol_local_at: u32,
        /// logical row currently centred in Home/Menu; page mode includes Back as index 0
        menu_anchor: usize,
        /// The latest valid native FOCUSED row, committed only after a short quiet period so fast swipes settle cleanly.
        pending_focus_row: i16,
        pending_focus_handle: u16,
        pending_focus_at: u32,
        /// Do not accept native FOCUSED events immediately after rebuilding a page; queued events can belong to its predecessor.
        focus_accept_at: u32,
        /// true only until the first phone response on a fresh app launch. This lets the phone restore
        /// a Watch Music song/podcast without confusing a deliberate return to Home with a relaunch.
        fresh_launch: bool,
        /// A phone-restored Music/Podcast row; open its track-specific player after the list arrives.
        resume_player: Option<u8>,
        /// RESUME started a new player-page handshake; the phone may restore a different live row.
        resume_request_pending: bool,
        /// true only until the first category catalog is received for a fresh app launch
        auto_pick_pending: bool,
        /// when the phone last moved the watch to another screen by itself (`None`: not yet)
        auto_at: Option<u32>,
        /// how many hellos went out since the last answer (the gap between them grows)
        hello_tries: u8,
        /// what the screen on show was built from: the category flags, the list, and whether Wait said "nothing"
        home_built: u8,
        menu_fp: u32,
        wait_empty: bool,
        /// the half of the player tapped last (its handle) and when, for the double tap
        last_tap_zone: u16,
        last_tap_at: u32,
        /// taps are ignored until this time after a long press went back
        long_guard: u32,
    }

    impl App {
        fn flush(&mut self) -> Result<(), LvError> {
            self.b.submit(&self.lv)
        }

        fn clock(&self, buf: &mut [u8; 5]) -> u16 {
            match self.api.time() {
                Some(t) => {
                    screens::clock_text(t.hour, t.minute, buf);
                    t.hour as u16 * 60 + t.minute as u16
                }
                None => {
                    *buf = *b"--:--";
                    u16::MAX
                }
            }
        }

        /// Replaces the screen by the one of `kind`, built from the model.
        fn go(&mut self, kind: Kind) {
            self.vol_flush(true);
            self.vol_until = None;
            // `Batch` reuses freed LVGL handles. A FOCUSED event from the old screen can therefore carry the same
            // handle as the new one. Clear the pending row and briefly reject queued focus events after each rebuild.
            self.pending_focus_row = -1;
            self.pending_focus_handle = 0;
            self.focus_accept_at = self.api.now().wrapping_add(FOCUS_REBUILD_GUARD_MS);
            if kind == Kind::Home {
                self.tap_guard = self.api.now().wrapping_add(TAP_GUARD_MS);
            }
            self.quiet_until = self.api.now().wrapping_add(QUIET_MS);
            let mut clock = [0u8; 5];
            let min = self.clock(&mut clock);
            let App { b, lv, s, m, page, menu_anchor, .. } = self;
            let mut flush = |b: &mut Batch| b.submit(lv);
            let text = core::str::from_utf8(&clock).unwrap_or("");
            let r = screens::leave(b, s).and_then(|_| match kind {
                Kind::Wait => screens::wait(b, &mut flush, m.mode, m.empty, *page),
                Kind::Home => screens::home(b, &mut flush, m.home_flags, *menu_anchor),
                Kind::Menu => {
                    // The list always starts in its natural order (Back, then the songs from the first). Starting it
                    // at the row last chosen put every row before it, and the Back arrow, at the end of the list.
                    *menu_anchor = 0;
                    screens::menu(b, &mut flush, m, *page, 0)
                }
                Kind::Player => screens::player(b, &mut flush, m, text, *page),
            });
            if let Ok(next) = r {
                *s = next;
                let _ = self.flush();
            }
            self.home_built = self.m.home_flags & 0x0F;
            self.menu_fp = self.m.fingerprint();
            self.wait_empty = self.m.empty;
            if kind == Kind::Player {
                self.last_min = min;
                self.refresh(true);
            }
        }

        fn since(&self) -> u32 {
            self.api.now().wrapping_sub(self.prog_at)
        }

        /// The ring (when its percent changed) and the clock (when the minute changed), in one batch.
        fn refresh(&mut self, force: bool) {
            if self.s.kind != Kind::Player {
                return;
            }
            let mut dirty = false;
            let secs = self.m.elapsed(self.since());
            let pct = permille(secs, self.m.dur) / 10;
            if force || pct != self.last_pct {
                self.last_pct = pct;
                dirty |= screens::show_progress(&mut self.b, &self.s, pct).is_ok();
            }
            let mut clock = [0u8; 5];
            let min = self.clock(&mut clock);
            if min != self.last_min {
                self.last_min = min;
                let text = core::str::from_utf8(&clock).unwrap_or("");
                dirty |= screens::show_clock(&mut self.b, &self.s, text).is_ok();
            }
            if dirty {
                let _ = self.flush();
            }
        }

        /// The song, the artist, play / pause and the tint, after the model changed.
        fn redraw_now(&mut self) {
            let _ = screens::show_now(&mut self.b, &self.s, self.m);
            let _ = self.flush();
        }

        fn send(&self, data: &[u8]) {
            self.api.send(data);
        }

        /// Tells the phone what the watch shows (count and version of its list, and the mode). The fifth byte marks
        /// only the first, untouched hello after a cold launch; the sixth marks a paused track-specific player page.
        /// These let the phone restore the last watch-selected song/show instead of opening a generic media controller.
        /// Repeated until the phone answers.
        fn hello(&mut self) {
            let player_resume = self.s.kind == Kind::Player
                && (self.m.mode == MODE_MUSIC || self.m.mode == MODE_PODCAST)
                && self.m.now != NONE;
            let id = self.hello_id.to_le_bytes();
            self.send(&[
                b'h', self.m.count, self.m.version, self.m.mode,
                if self.fresh_launch { 1 } else { 0 },
                if player_resume { 1 } else { 0 },
                id[0], id[1], id[2], id[3],
            ]);
            self.heard = false;
            let gap = (HELLO_MS << self.hello_tries.min(3)).min(HELLO_MAX_MS);
            self.hello_tries = self.hello_tries.saturating_add(1);
            self.next_hello = self.api.now().wrapping_add(gap);
        }

        /// A new question to the phone (not a retry): the gaps start short again.
        fn hello_now(&mut self) {
            // A new question is a new exchange. All retries inside hello() retain this ID so the phone can
            // collapse the queued copies into one response instead of replaying a complete restoration.
            self.hello_id = self.hello_id.wrapping_add(1);
            if self.hello_id == 0 { self.hello_id = 1; }
            self.hello_tries = 0;
            self.hello();
        }

        /// The start screen's choice: the list of that mode, the saved one at once for music.
        fn pick(&mut self, mode: u8) {
            self.fresh_launch = false;
            self.resume_player = None;
            self.auto_pick_pending = false;
            self.menu_anchor = 0;
            self.m.pick(mode);
            if mode == MODE_MUSIC {
                let buf = unsafe { &mut *&raw mut STORE };
                let n = self.api.load(buf);
                if n > 0 {
                    self.m.load(&buf[..n as usize]);
                }
            }
            self.go(if self.m.count > 0 { Kind::Menu } else { Kind::Wait });
            self.hello_now();
        }

        /// Back to the start screen.
        fn home(&mut self) {
            // A deliberate Back should show the selector even if only one category exists; it must not trigger
            // the cold-launch resume path on the next hello.
            self.fresh_launch = false;
            self.resume_player = None;
            self.auto_pick_pending = false;
            self.menu_anchor = 0;
            self.m.pick(MODE_NONE);
            self.got_catalog = false;
            self.go(Kind::Home);
            self.hello_now();
        }

        /// The phone sent the set of categories with real content.
        /// The same set again changes nothing on screen, so nothing is rebuilt: a rebuild throws the list back to the
        /// top, blanks the icons for a moment and moves the Back arrow (that was the twitching). The phone moves the
        /// watch by itself only until the first choice, and not more often than `AUTO_GAP_MS`; the controls stay open
        /// while they are open, because the media session comes and goes with every pause (that was the flicker).
        fn home_catalog(&mut self, refreshed: bool) {
            self.got_catalog = true;
            let flags = self.m.home_flags;
            let shown = flags & 0x0F;
            let now = self.api.now();
            let calm = self.auto_at.map_or(true, |t| now.wrapping_sub(t) >= AUTO_GAP_MS);
            if self.s.kind == Kind::Home {
                // Cold launch while something plays: straight to its controls, no start screen in between.
                if flags & 0x10 != 0 && !refreshed && flags & 0x08 != 0 && calm {
                    self.auto_pick_pending = false;
                    self.auto_at = Some(now);
                    self.return_to_controls();
                    return;
                }
                if shown == self.home_built {
                    return;
                }
                self.menu_anchor = 0;
                if refreshed {
                    self.auto_pick_pending = true;
                }
                if self.auto_pick_pending && calm {
                    let mut only = MODE_NONE;
                    let mut count = 0usize;
                    for (mode, bit) in [(MODE_MUSIC, 0x01), (MODE_PODCAST, 0x02), (MODE_CUSTOM, 0x04)] {
                        if flags & bit != 0 { only = mode; count += 1; }
                    }
                    if count == 1 {
                        self.auto_at = Some(now);
                        self.pick(only);
                        return;
                    }
                    if count == 0 && flags & 0x08 != 0 {
                        self.auto_pick_pending = false;
                        self.auto_at = Some(now);
                        self.return_to_controls();
                        return;
                    }
                }
                self.go(Kind::Home);
                return;
            }
            let available = match self.m.mode {
                MODE_MUSIC => flags & 0x01 != 0,
                MODE_PODCAST => flags & 0x02 != 0,
                MODE_CUSTOM => flags & 0x04 != 0,
                // The controls stay while they are open. Back leaves them.
                MODE_EXTERNAL => true,
                _ => true,
            };
            if !available {
                self.m.pick(MODE_NONE);
                self.menu_anchor = 0;
                // Home stays: no automatic jump into another screen from here.
                self.auto_pick_pending = false;
                self.go(Kind::Home);
                self.hello_now();
            }
        }

        /// Opens the controls for whatever media session Android currently considers active.
        /// The phone returns the live title, artist, state and progress; this is not a playlist pick.
        fn return_to_controls(&mut self) {
            self.fresh_launch = false;
            self.resume_player = None;
            self.m.pick(MODE_EXTERNAL);
            self.m.count = 1;
            self.m.now = 0;
            self.m.playing = false;
            self.m.pos = 0;
            self.m.dur = 0;
            self.go(Kind::Player);
            // Mode 2 uses the existing hello/retry handshake, so a dropped request is retried.
            self.hello_now();
        }

        // ---- the volume on the crown ----

        /// Sends the last crown turn to the phone (now, or when the gap since the last send has passed).
        fn vol_flush(&mut self, force: bool) {
            if !self.vol_dirty {
                return;
            }
            let now = self.api.now();
            if force || now.wrapping_sub(self.vol_sent_at) >= VOL_GAP_MS {
                self.send(&[b'v', self.m.level]);
                self.vol_sent_at = now;
                self.vol_dirty = false;
            }
        }

        /// The button, the "Vol 65%" text and the sound wave follow the state.
        fn vol_show(&mut self) {
            let on = self.vol_until.is_some();
            let _ = screens::show_volume(&mut self.b, &self.s, on, self.m.level, self.m.muted);
            let _ = self.flush();
        }

        /// A press on the volume button: the crown is the volume for 30 s, or it is not any more.
        fn vol_toggle(&mut self) {
            if self.vol_until.is_some() {
                self.vol_flush(true);
                self.vol_until = None;
            } else {
                self.vol_until = Some(self.api.now().wrapping_add(VOL_MS));
            }
            self.vol_show();
        }

        /// One crown click while the volume is on.
        fn crown(&mut self, delta: i16) {
            if delta != 0 { self.fresh_launch = false; }
            if self.s.kind != Kind::Player || self.vol_until.is_none() || delta == 0 {
                return;
            }
            let now = self.api.now();
            self.vol_until = Some(now.wrapping_add(VOL_MS));
            self.vol_local_at = now;
            let level = step_volume(self.m.level, delta);
            if level != self.m.level {
                self.m.level = level;
                self.vol_dirty = true;
                self.vol_flush(false);
            }
            self.vol_show();
        }

        /// Every turn of the loop: a held-back volume goes out, and the 30 s end.
        fn vol_tick(&mut self) {
            self.vol_flush(false);
            if let Some(t) = self.vol_until {
                if after(self.api.now(), t) {
                    self.vol_flush(true);
                    self.vol_until = None;
                    self.vol_show();
                }
            }
        }

        /// How long the loop may sleep for the volume (the end of the 30 s, a held-back send).
        fn vol_wait(&self) -> Option<u32> {
            let now = self.api.now();
            let mut w = self.vol_until.map(|t| (t.wrapping_sub(now) as i32).max(0) as u32);
            if self.vol_dirty {
                let d = VOL_GAP_MS.saturating_sub(now.wrapping_sub(self.vol_sent_at));
                w = Some(w.map_or(d, |x| x.min(d)));
            }
            w
        }

        fn save(&mut self) {
            let buf = unsafe { &mut *&raw mut STORE };
            let n = self.m.save(buf);
            // The phone sends the same list after every hello. Writing it again is a flash write that stalls the
            // loop (and scrolling with it) for nothing, so an identical list is not saved twice.
            let sum = buf[..n].iter().fold(0x811c_9dc5u32, |h, &x| (h ^ x as u32).wrapping_mul(0x0100_0193)) | 1;
            if sum == self.saved_sum {
                return;
            }
            self.saved_sum = sum;
            self.api.save(&buf[..n]);
        }

        /// One phone message.
        /// A scroll tick for a new centre row: soft, sparse, and never while nothing is being scrolled.
        /// `focus_obj` must be the current focus-page handle. The SDK reports the index in the rendered child order;
        /// the current screen's list_anchor converts that order back to the stable logical row.
        fn focus_tick(&mut self, focus_obj: u16, focused_row: i16) {
            if !matches!(self.s.kind, Kind::Home | Kind::Menu)
                || focus_obj != self.s.page.0
                || !after(self.api.now(), self.focus_accept_at)
            {
                return;
            }
            let total = match self.s.kind {
                Kind::Home => self.s.home_count as usize,
                Kind::Menu => self.m.count as usize + if self.page { 1 } else { 0 },
                _ => 0,
            };
            // Do not modulo an invalid/stale index into a valid row. That was able to turn an old list's last row
            // into the new list's first/last choice after a fast swipe or a page rebuild.
            let Some(anchor) = screens::focused_anchor(self.s.list_anchor, focused_row, total) else {
                return;
            };
            // A changed centre row is a real user navigation action; do not let a delayed cold-launch
            // restore replace a menu the user has already started browsing.
            if anchor != self.menu_anchor {
                self.fresh_launch = false;
            }
            let now = self.api.now();
            self.pending_focus_row = focused_row;
            self.pending_focus_handle = focus_obj;
            self.pending_focus_at = now;
            if !after(now, self.quiet_until) || now.wrapping_sub(self.last_tick) < TICK_GAP_MS {
                return;
            }
            self.last_tick = now;
            self.motor.play(SOFT);
        }

        /// Commit the last valid native focus event after the on-watch scroll has settled.
        fn commit_focus(&mut self, force: bool) {
            if self.pending_focus_row < 0 {
                return;
            }
            let now = self.api.now();
            if !force && now.wrapping_sub(self.pending_focus_at) < FOCUS_SETTLE_MS {
                return;
            }
            let total = match self.s.kind {
                Kind::Home => self.s.home_count as usize,
                Kind::Menu => self.m.count as usize + if self.page { 1 } else { 0 },
                _ => 0,
            };
            if self.pending_focus_handle == self.s.page.0 {
                if let Some(anchor) = screens::focused_anchor(self.s.list_anchor, self.pending_focus_row, total) {
                    self.menu_anchor = anchor;
                }
            }
            self.pending_focus_row = -1;
            self.pending_focus_handle = 0;
        }

        fn phone(&mut self, msg: &[u8]) {
            if msg.is_empty() { return; }
            // Capture this before clearing it: W is allowed once for a cold launch, but a late W must not
            // pull the user back into Player after they deliberately pressed Back to the list or Home.
            let cold_launch = self.fresh_launch;
            if msg.first() != Some(&b'W') { self.resume_request_pending = false; }
            self.fresh_launch = false;
            // `W mode index playing`: restore the exact Music/Podcast selection that was last opened
            // from this watch. The phone sends the matching list, title, progress and cover colours next.
            if msg.first() == Some(&b'W') && msg.len() >= 4 {
                let mode = msg[1];
                let index = msg[2];
                if (mode == MODE_MUSIC || mode == MODE_PODCAST) && (index as usize) < MAX_TRACKS {
                    let same_pending = self.resume_player == Some(index)
                        && self.m.mode == mode && self.m.now == index;
                    let same_player = self.s.kind == Kind::Player
                        && self.m.mode == mode && self.m.now == index;
                    let page_resume = self.resume_request_pending
                        && self.s.kind == Kind::Player && self.m.mode == mode;

                    // A duplicate restore must never rebuild the same player. A new page-resume exchange may
                    // legitimately select a different row if the podcast/music session advanced while hidden.
                    if same_pending || same_player {
                        self.resume_request_pending = false;
                        let playing = msg[3] != 0;
                        if self.m.playing != playing {
                            self.m.playing = playing;
                            self.prog_at = self.api.now();
                            if same_player { self.redraw_now(); }
                        }
                        return;
                    }
                    if !cold_launch && !page_resume {
                        return; // stale W from a previous launch/restore after the user navigated away
                    }

                    self.resume_request_pending = false;
                    self.resume_player = Some(index);
                    self.auto_pick_pending = false;
                    self.menu_anchor = 0;
                    self.m.pick(mode);
                    self.m.now = index;
                    self.m.playing = msg[3] != 0;
                    // if the rest of it never arrives, ask again instead of waiting for ever
                    self.resume_at = self.api.now().wrapping_add(RESUME_MS);
                }
                return;
            }
            // A live-media update that was already on its way when the user pressed Back must not pull the
            // model back into Control media (it only counts while that mode is the one being shown).
            if msg.first() == Some(&b'X') && self.m.mode != MODE_EXTERNAL {
                return;
            }
            let refreshed = msg.first() == Some(&b'R');
            let level = self.m.level;
            self.quiet_until = self.api.now().wrapping_add(QUIET_MS);
            let fx = self.m.apply(msg);
            if fx.home {
                self.home_catalog(refreshed);
                return;
            }
            // the phone's echo of a volume we just set by the crown: the crown is ahead of it
            let local = self.api.now().wrapping_sub(self.vol_local_at) < 1500;
            if msg.first() == Some(&b'V') && self.vol_until.is_some() && (local || self.vol_dirty) {
                self.m.level = level;
            }
            // only the music list is kept on the watch; the podcasts come from the phone every time
            if fx.saved && self.m.mode == MODE_MUSIC {
                self.m.fix_order();
                self.save();
            }
            // The restored player opens as soon as its own row has arrived, not after the whole list.
            if let Some(index) = self.resume_player {
                if index < self.m.count && self.m.tracks[index as usize].tlen > 0 && !fx.saved {
                    self.resume_player = None;
                    self.resume_at = 0;
                    self.m.now = index;
                    self.go(Kind::Player);
                }
            }
            if fx.list && self.m.count > 0 && fx.saved {
                self.resume_at = 0;
                if let Some(index) = self.resume_player.take() {
                    if index < self.m.count {
                        // Open the track-specific player, not the generic active-session controller. K/Q cover
                        // colours and G progress continue arriving afterwards and update its matching background.
                        self.m.now = index;
                        self.go(Kind::Player);
                    } else if matches!(self.s.kind, Kind::Home | Kind::Wait | Kind::Menu) {
                        self.go(Kind::Menu);
                    }
                } else if self.s.kind == Kind::Wait
                    || (self.s.kind == Kind::Menu && self.m.fingerprint() != self.menu_fp)
                {
                    // The phone sends the whole list again after every hello; the same list is not rebuilt.
                    self.go(Kind::Menu);
                }
            }
            if fx.list && self.m.count == 0 && self.m.empty && self.s.kind == Kind::Wait && !self.wait_empty {
                self.go(Kind::Wait); // the same screen again, now saying that there is nothing to show
            }
            if msg.first() == Some(&b'V') && self.vol_until.is_some() && self.s.kind == Kind::Player {
                self.vol_show();
            }
            if fx.colours {
                match self.s.kind {
                    Kind::Menu => {
                        let App { b, lv, s, m, .. } = self;
                        let mut flush = |b: &mut Batch| b.submit(lv);
                        // restyle only the rows whose colour changed, and send nothing when none did
                        if let Ok(true) = screens::recolour(b, &mut flush, s, m) {
                            let _ = self.flush();
                        }
                    }
                    Kind::Player => {
                        let _ = screens::show_colours(&mut self.b, &self.s, self.m);
                        let _ = self.flush();
                    }
                    Kind::Wait | Kind::Home => {}
                }
            }
            if self.s.kind == Kind::Player && fx.now {
                self.redraw_now();
            }
            if self.s.kind == Kind::Player && fx.liked {
                let _ = screens::show_liked(&mut self.b, &self.s, self.m.liked);
                let _ = self.flush();
            }
            if fx.progress {
                self.prog_at = self.api.now();
                self.refresh(true);
            }
            if fx.now && self.s.kind == Kind::Player {
                self.refresh(true);
            }
        }

        /// Long press anywhere on the player: back, the same as the crown button.
        fn long_press(&mut self, h: u16) {
            if self.s.kind != Kind::Player || h == 0 {
                return;
            }
            self.fresh_launch = false;
            self.long_guard = self.api.now().wrapping_add(LONG_GUARD_MS);
            self.last_tap_zone = 0;
            self.motor.play(TICK);
            if self.m.mode == MODE_EXTERNAL { self.home() } else { self.go(Kind::Menu) }
        }

        /// A tap on the empty left / right half of the player: a second one soon after seeks -5 s / +5 s.
        fn zone_tap(&mut self, h: u16) {
            let now = self.api.now();
            if self.last_tap_zone == h && now.wrapping_sub(self.last_tap_at) <= DOUBLE_TAP_MS {
                self.last_tap_zone = 0;
                let delta = if h == self.s.seek_right.0 { SEEK_S } else { -SEEK_S };
                let elapsed = self.m.elapsed(self.since());
                if let Some((permille, target)) = self.m.seek_permille(elapsed, delta) {
                    self.motor.play(TICK);
                    self.send(&[b'k', (permille & 0xFF) as u8, (permille >> 8) as u8]);
                    self.m.pos = target;
                    self.prog_at = now;
                    self.refresh(true);
                }
            } else {
                self.last_tap_zone = h;
                self.last_tap_at = now;
            }
        }

        fn click(&mut self, h: u16) {
            if !after(self.api.now(), self.long_guard) {
                return; // the release of a long press
            }
            self.fresh_launch = false;
            // A page cannot rely on the watch's normal back action, so each inner screen owns a visible back target.
            if h != 0 && h == self.s.back.0 {
                self.motor.play(TICK);
                match self.s.kind {
                    Kind::Player if self.m.mode == MODE_EXTERNAL => self.home(),
                    Kind::Player => self.go(Kind::Menu),
                    Kind::Menu | Kind::Wait => self.home(),
                    Kind::Home => {}
                }
                return;
            }
            if self.s.kind == Kind::Home {
                if !after(self.api.now(), self.tap_guard) {
                    return;
                }
                if let Some(slot) = self.s.rows.iter().take(self.s.home_count as usize).position(|r| r.0 == h) {
                    self.motor.play(TICK);
                    match self.s.home_modes[slot] {
                        MODE_MUSIC => self.pick(MODE_MUSIC),
                        MODE_PODCAST => self.pick(MODE_PODCAST),
                        MODE_CUSTOM => self.pick(MODE_CUSTOM),
                        MODE_EXTERNAL => self.return_to_controls(),
                        _ => {}
                    }
                }
                return;
            }
            if self.s.kind == Kind::Menu {
                if let Some(slot) = self.s.rows.iter().take(self.s.row_count as usize).position(|r| r.0 == h) {
                    self.motor.play(TICK);
                    let index = self.s.row_indices[slot];
                    self.send(&[b's', index]);
                    if self.m.mode == MODE_CUSTOM {
                        // Custom entries execute their phone-side URL action; there is no media player screen.
                        return;
                    }
                    // The song/show that already plays: open its controls without restarting it.
                    if self.m.now != index {
                        self.m.now = index;
                        self.m.playing = true;
                        self.m.pos = 0;
                        self.m.dur = 0;
                        self.prog_at = self.api.now();
                    }
                    self.go(Kind::Player);
                }
                return;
            }
            if self.s.kind != Kind::Player || h == 0 {
                return;
            }
            if h == self.s.seek_left.0 || h == self.s.seek_right.0 {
                self.zone_tap(h);
                return;
            }
            self.last_tap_zone = 0;
            if h == self.s.heart.0 {
                if self.m.toggle_liked() {
                    self.motor.play(TICK);
                    self.send(b"l");
                    let _ = screens::show_liked(&mut self.b, &self.s, self.m.liked);
                    let _ = self.flush();
                }
            } else if h == self.s.vol.0 {
                self.motor.play(TICK);
                self.vol_toggle();
            } else if h == self.s.play.0 {
                self.motor.play(TICK);
                self.m.pos = self.m.elapsed(self.since()).min(u16::MAX as u32) as u16;
                self.prog_at = self.api.now();
                self.m.playing = !self.m.playing;
                self.send(b"p");
                self.redraw_now();
            } else if h == self.s.next.0 || h == self.s.prev.0 {
                self.motor.play(TICK);
                let fwd = h == self.s.next.0;
                self.send(if fwd { b"n" } else { b"b" });
                if self.m.mode == MODE_PODCAST || self.m.mode == MODE_EXTERNAL {
                    // The phone plays the next/previous podcast episode or active system media item
                    // and sends fresh metadata when Android reports the new item.
                    return;
                }
                let c = self.m.count;
                if c > 0 && self.m.now != NONE {
                    self.m.now = if fwd { (self.m.now + 1) % c } else { (self.m.now + c - 1) % c };
                    self.m.playing = true;
                    self.m.pos = 0;
                    self.m.dur = 0;
                    self.prog_at = self.api.now();
                    self.redraw_now();
                    self.refresh(true);
                }
            }
        }
    }

    fn main(api: &'static Api) -> i32 {
        let Some(lv) = api.lv() else { return -95 }; // loader older than ABI 1.2
        let Some(b) = Batch::new() else { return -1 };
        let initial_hello_id = api.now().max(1);
        let mut app = App {
            api,
            lv,
            b,
            s: Screen::empty(),
            motor: Motor::new(api),
            m: unsafe { &mut *&raw mut MODEL },
            prog_at: api.now(),
            last_pct: -1,
            last_min: u16::MAX,
            page: api.on_page(),
            heard: false,
            paused: false,
            tide_next: api.now(),
            next_hello: api.now(),
            hello_id: initial_hello_id,
            resume_at: 0,
            tap_guard: 0,
            got_catalog: false,
            vol_until: None,
            quiet_until: 0,
            last_tick: 0,
            saved_sum: 0,
            vol_dirty: false,
            vol_sent_at: api.now(),
            vol_local_at: api.now(),
            menu_anchor: 0,
            pending_focus_row: -1,
            pending_focus_handle: 0,
            pending_focus_at: api.now(),
            focus_accept_at: api.now(),
            fresh_launch: true,
            resume_player: None,
            resume_request_pending: false,
            auto_pick_pending: true,
            auto_at: None,
            hello_tries: 0,
            home_built: 0xFF,
            menu_fp: 0,
            wait_empty: false,
            last_tap_zone: 0,
            last_tap_at: 0,
            long_guard: 0,
        };
        // A new app instance starts from Home even if the loader reuses the process and static allocation.
        // Reset field-by-field: constructing a temporary Model here would put the full 50-track table
        // (several KB) on the watch's small app stack and can reset the watch before the UI opens.
        app.m.reset();
        if screens::fonts(&mut app.b).and_then(|_| app.flush()).is_err() {
            return 1;
        }
        if screens::theme(&mut app.b).is_err() {
            return 2;
        }
        api.own_back(true);
        // The phone will return a dynamic category catalog; a single available category opens automatically.
        app.go(Kind::Home);
        let mut msg = [0u8; MSG_MAX];
        loop {
            let now = api.now();
            if !app.heard && after(now, app.next_hello) {
                app.hello();
            }
            if app.resume_at != 0 && after(now, app.resume_at) {
                if app.resume_player.is_some() {
                    app.resume_at = now.wrapping_add(RESUME_MS);
                    app.hello();
                } else {
                    app.resume_at = 0;
                }
            }
            app.motor.update();
            // wake for the motor, the next second of the song, the clock, the volume, or the next hello
            let mut wait = if app.heard { u32::MAX } else { app.next_hello.wrapping_sub(now).min(HELLO_MAX_MS) };
            if app.resume_at != 0 {
                wait = wait.min(app.resume_at.wrapping_sub(now).min(RESUME_MS));
            }
            if app.s.kind == Kind::Player {
                wait = wait.min(if app.m.playing { 1000 - app.since() % 1000 } else { IDLE_MS });
            }
            if let Some(w) = app.motor.wait() {
                wait = wait.min(w);
            }
            if app.s.kind == Kind::Player && !app.paused {
                wait = wait.min(app.tide_next.wrapping_sub(now).min(TIDE_MS));
            }
            if let Some(w) = app.vol_wait() {
                wait = wait.min(w);
            }
            if app.pending_focus_row >= 0 {
                let elapsed = now.wrapping_sub(app.pending_focus_at);
                let remaining = FOCUS_SETTLE_MS.saturating_sub(elapsed);
                wait = wait.min(remaining.max(1));
            }
            let ev = api.event(wait.max(1));
            app.motor.update();
            app.refresh(false);
            app.vol_tick();
            if app.s.kind == Kind::Player && !app.paused && after(api.now(), app.tide_next) {
                let t = api.now();
                if screens::tide(&mut app.b, &app.s, t).is_ok() && !app.b.bytes().is_empty() {
                    let _ = app.flush();
                }
                app.tide_next = api.now().wrapping_add(TIDE_MS);
            }
            let Some(e) = ev else {
                app.commit_focus(false);
                continue;
            };
            match e.kind {
                BUTTON => {
                    app.fresh_launch = false;
                    match app.s.kind {
                    Kind::Player if app.m.mode == MODE_EXTERNAL => app.home(),
                    Kind::Player => app.go(Kind::Menu),
                    Kind::Menu | Kind::Wait => app.home(),
                    Kind::Home => break,
                    }
                }
                PAUSE => {
                    app.paused = true;
                    app.motor.off();
                    // The loader rejects host messages while paused. Do not keep asking the phone (and
                    // accumulating stale replies); RESUME starts one fresh, identified exchange instead.
                    app.heard = true;
                    app.hello_tries = 0;
                    app.resume_at = 0;
                    app.resume_request_pending = false;
                }
                RESUME => {
                    app.paused = false;
                    app.tide_next = api.now();
                    app.hello_id = app.hello_id.wrapping_add(1);
                    if app.hello_id == 0 { app.hello_id = 1; }
                    app.resume_request_pending = app.s.kind == Kind::Player;
                    app.heard = false;
                    app.hello_tries = 0;
                    app.next_hello = api.now();
                }
                CROWN => app.crown(e.delta),
                LV => {
                    let h = e.x as u16;
                    match e.y {
                        CLICKED => app.click(h),
                        LONG_PRESSED => app.long_press(h),
                        FOCUSED => app.focus_tick(e.x as u16, e.delta),
                        _ => {}
                    }
                }
                MESSAGE => {
                    app.heard = true;
                    app.hello_tries = 0;
                    // Keep fresh_launch set until phone() examines the first payload. Android sends W first
                    // for a saved player restore; phone() captures the cold-launch permission exactly once.
                    while let Some(m) = api.recv(&mut msg) {
                        app.phone(m);
                    }
                    // any message counts as an answer, except while the start screen still waits for its catalog:
                    // the phone has to hear the hello that tells it the watch left Control media
                    if app.s.kind == Kind::Home && !app.got_catalog {
                        app.heard = false;
                    }
                }
                _ => {}
            }
            app.commit_focus(false);
        }
        app.motor.off();
        0
    }
}
