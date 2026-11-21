//! weather: now, the next hours and the next days, from FMC Companion (`location` for the place, `weather`).
//!
//! - The menu app is three pages in the stock pager (`screen::pages_view`): now, hours, days. The crown or a
//!   swipe pages; the hours and days scroll first. BUTTON exits.
//! - The page next to the watch face (`app_features!(PAGE)`) is the now screen alone (`screen::face_view`).
//! - The card left of the face (`WIDGET`): the place, the temperature with the condition, high and low; written
//!   whenever it changes while the app runs.
//!
//! The weather comes from the phone at start and every 15 minutes while shown, and is kept in the app's storage so
//! it opens at once without the phone (`model.rs`).
#![cfg_attr(target_os = "none", no_std, no_main)]

#[cfg_attr(not(target_os = "none"), allow(dead_code))]
mod model;
#[cfg_attr(not(target_os = "none"), allow(dead_code))]
mod screen;

#[cfg(not(target_os = "none"))]
fn main() {}

#[cfg(target_os = "none")]
mod device {
    use crate::model::{BLOB_MAX, Got, Line, Model, wall_min};
    use crate::screen::{self, View};
    use cmf_wp2_app::lv::{Batch, Lv, ROOT};
    use cmf_wp2_app::{Api, BUTTON, MESSAGE, MSG_MAX, PAUSE, RESUME};

    cmf_wp2_app::entry!(main);
    cmf_wp2_app::app_icon!();
    cmf_wp2_app::app_features!(cmf_wp2_app::WIDGET | cmf_wp2_app::PAGE);

    /// How often to look at the clock: a new hour moves the hours, the "Updated" line ages.
    const TICK_MS: u32 = 60_000;
    /// On the page next to the face, coming back asks the phone only when the copy is older than this.
    const FRESH_MIN: i32 = 30;

    static mut STORE: [u8; BLOB_MAX] = [0; BLOB_MAX];
    static mut MODEL: Model = Model::new();

    fn wall(api: &Api) -> i32 {
        api.time().map_or(0, |t| wall_min(t.year as i32, t.month as u32, t.day as u32, t.hour as u32, t.minute as u32))
    }

    struct App {
        api: &'static Api,
        lv: Lv,
        b: Batch,
        model: &'static mut Model,
        view: View,
        /// The pager's page, kept over a redraw.
        page: usize,
        on_page: bool,
        /// What the screen shows depends on, besides the data.
        shown: (i32, bool, i32),
        card: (Line, Line),
    }

    impl App {
        /// The hour, whether the phone is silent, and the age once it is worth a line.
        fn key(&self, w: i32) -> (i32, bool, i32) {
            let stale = self.model.stale_min(w);
            (w.div_euclid(60), self.model.silent(), if self.model.now.is_some() && stale > 45 { stale / 10 } else { 0 })
        }

        fn draw(&mut self) {
            let (lv, w) = (self.lv, wall(self.api));
            let mut flush = |b: &mut Batch| b.submit(&lv);
            let r = screen::leave(&mut self.b, self.view.root)
                .and_then(|_| {
                    if self.on_page {
                        screen::face_view(&mut self.b, &mut flush, self.model, w)
                    } else {
                        screen::pages_view(&mut self.b, &mut flush, self.model, w, self.page)
                    }
                })
                .and_then(|v| self.b.submit(&lv).map(|_| v));
            match r {
                Ok(v) => self.view = v,
                Err(_) => self.b.clear(), // or the records left in it fail every later draw
            }
            self.shown = self.key(w);
        }

        fn save(&mut self) {
            let blob = unsafe { &mut *core::ptr::addr_of_mut!(STORE) };
            let n = self.model.save(blob);
            if n > 0 {
                self.api.save(&blob[..n]); // on change: the app can end without notice
            }
        }

        /// The card, when its text changes.
        fn card(&mut self) {
            let (t, v) = self.model.card(wall(self.api));
            if t.as_str() != self.card.0.as_str() || v.as_str() != self.card.1.as_str() {
                self.api.widget(t.as_str(), v.as_str());
                self.card = (t, v);
            }
        }

        fn poll(&mut self) {
            let now = self.api.now();
            while let Some(req) = self.model.poll(now) {
                if self.api.send(&req) == -11 {
                    break; // outbox full: the retry timer sends it again
                } // -107 with no phone: the retry timer covers it too
            }
        }

        fn message(&mut self, m: &[u8]) {
            let got = self.model.reply(m, self.api.now(), wall(self.api));
            if got == Got::Saved {
                self.save();
            }
            if got != Got::Nothing {
                self.draw();
                self.card();
            }
        }

        /// The clock moved, or the phone went silent.
        fn tick(&mut self) {
            if self.key(wall(self.api)) != self.shown {
                self.draw();
                self.card();
            }
        }
    }

    fn main(api: &'static Api) -> i32 {
        let Some(lv) = api.lv() else { return -95 }; // loader older than ABI 1.2
        let Some(mut b) = Batch::new() else { return -1 };
        if screen::fonts(&mut b).and_then(|_| b.submit(&lv)).is_err() {
            return 1;
        }
        if screen::theme(&mut b).and_then(|_| b.submit(&lv)).is_err() {
            return 2;
        }
        let model = unsafe { &mut *core::ptr::addr_of_mut!(MODEL) };
        let blob = unsafe { &mut *core::ptr::addr_of_mut!(STORE) };
        let n = api.load(blob);
        if n > 0 {
            model.load(&blob[..n as usize]);
        }
        model.ask_now(api.now());
        let view = View { root: ROOT, pager: None, bars: [ROOT; 3] };
        let mut app =
            App { api, lv, b, model, view, page: 0, on_page: api.on_page(), shown: (0, false, 0), card: (Line::new(), Line::new()) };
        app.draw();
        app.card();
        let mut paused = false;
        let mut msg = [0u8; MSG_MAX];
        loop {
            if !paused {
                app.poll();
            }
            let wait = if paused { u32::MAX } else { app.model.wait(api.now()).min(TICK_MS) };
            let e = api.event(wait);
            if !paused {
                app.tick();
            }
            let Some(e) = e else { continue };
            match e.kind {
                BUTTON => break,
                PAUSE => paused = true,
                RESUME => {
                    paused = false;
                    if app.model.now.is_none() || app.model.stale_min(wall(api)) > FRESH_MIN {
                        app.model.ask_now(api.now());
                    }
                    app.tick();
                }
                MESSAGE => {
                    while let Some(m) = api.recv(&mut msg) {
                        app.message(m);
                    }
                }
                _ if !paused => {
                    // the pager settled on a page: move its dots
                    if let Some(p) = app.view.pager.as_mut() {
                        if let Ok(Some(i)) = p.event(&mut app.b, &e) {
                            app.page = i;
                            let _ = app.b.submit(&lv);
                        }
                    }
                }
                _ => {}
            }
        }
        0
    }
}
