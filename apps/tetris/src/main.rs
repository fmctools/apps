#![cfg_attr(any(target_os = "none", wp2_sim), no_main, cfg_attr(target_os = "none", no_std))]

#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod game;
#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod view;

#[cfg(not(any(target_os = "none", wp2_sim)))]
fn main() {}

#[cfg(any(target_os = "none", wp2_sim))]
mod device {
    use crate::game::{Game, State, Step, decode, due, encode};
    use crate::view::{frame, seen};
    use cmf_wp2_app::{Api, BUTTON, CROWN, PAUSE, RESUME, TOUCH_DOWN, TOUCH_UP};
    use cmf_wp2_ui::haptic::{DOUBLE, DONE, Motor, TICK};

    cmf_wp2_app::entry!(main);
    cmf_wp2_app::app_icon!();

    /// A finger that stays on the glass this long is a hold (the piece drops fast), a shorter one is a tap (it turns).
    const HOLD_MS: u32 = 220;
    /// Taps right after the game ended don't start a new one: they were meant for the game that just ended.
    const RETRY_MS: u32 = 700;
    /// Turning the crown clockwise moves the piece right. Flip this if your watch counts the other way.
    const CROWN_RIGHT: i16 = 1;

    // true on RESUME, false on BUTTON
    fn paused(api: &Api) -> bool {
        loop {
            match api.event(u32::MAX).map(|e| e.kind) {
                Some(RESUME) => return true,
                Some(BUTTON) => return false,
                _ => {}
            }
        }
    }

    fn seed(api: &Api) -> u32 {
        let t = api.time().map_or(0, |t| (t.minute as u32 * 60 + t.second as u32) * 1000 + t.millis as u32);
        api.now().wrapping_mul(2_654_435_761) ^ t
    }

    /// The finger on the glass: when it came down and what the game was doing then.
    #[derive(Clone, Copy)]
    struct Finger {
        at: u32,
        state: State,
        holding: bool,
    }

    fn main(api: &'static Api) -> i32 {
        let mut screen = api;
        let mut motor = Motor::new(api); // dropped on return: the motor goes off
        let mut blob = [0u8; 16];
        let n = api.load(&mut blob);
        let mut best = if n > 0 { decode(&blob[..n as usize]) } else { 0 };
        let mut g = Game::new(seed(api));
        let mut on_screen = Some(frame(&mut screen, &g, best, None));
        api.show();
        let mut finger: Option<Finger> = None;
        let mut over_at = 0u32;
        loop {
            motor.update();
            let now = api.now();
            // what is due next: the game, the end of a tap (a hold begins), the motor
            let mut wait = g.wake_in(now).unwrap_or(u32::MAX);
            if let Some(f) = finger.filter(|f| f.state == State::Playing && !f.holding) {
                let left = (f.at.wrapping_add(HOLD_MS).wrapping_sub(now) as i32).max(0) as u32;
                wait = wait.min(left);
            }
            if let Some(m) = motor.wait() {
                wait = wait.min(m);
            }
            let mut first = api.event(wait);
            let mut redraw_all = false;
            let mut quit = false;
            // handle this event and everything else already queued, then draw once
            for _ in 0..16 {
                let Some(e) = first.take().or_else(|| api.event(0)) else { break };
                let now = api.now();
                match e.kind {
                    BUTTON => quit = true,
                    PAUSE => {
                        motor.off();
                        g.set_soft(false, now);
                        if !paused(api) {
                            quit = true;
                        } else {
                            finger = None;
                            g.rebase(api.now());
                            redraw_all = true;
                        }
                    }
                    TOUCH_DOWN => {
                        finger = Some(Finger { at: now, state: g.state, holding: false });
                    }
                    TOUCH_UP => {
                        let Some(f) = finger.take() else { continue };
                        if f.holding {
                            g.set_soft(false, now);
                        } else if f.state != g.state {
                            // the game changed under the finger: this touch belonged to something else
                        } else {
                            match g.state {
                                State::Playing => {
                                    g.turn(now);
                                }
                                State::Ready => g.start(now),
                                State::Over => {
                                    if due(f.at, over_at.wrapping_add(RETRY_MS)) {
                                        g.start(now);
                                    }
                                }
                            }
                        }
                    }
                    CROWN => {
                        g.shift(if e.delta * CROWN_RIGHT > 0 { 1 } else { -1 }, now);
                    }
                    _ => {}
                }
                if quit {
                    break;
                }
            }
            if quit {
                api.keep_screen_on(false);
                return 0;
            }
            let now = api.now();
            // a finger that stayed down: the piece drops
            if let Some(f) = &mut finger {
                if f.state == State::Playing && g.state == State::Playing && !f.holding && due(now, f.at.wrapping_add(HOLD_MS)) {
                    f.holding = true;
                    g.set_soft(true, now);
                }
            }
            // what is due in the game; at most a few things per wake-up, the rest on the next one
            for _ in 0..4 {
                if g.wake_in(now) != Some(0) {
                    break;
                }
                match g.tick(now) {
                    Step::Locked => motor.play(TICK),
                    Step::Cleared(n) => motor.play(if n >= 4 { DONE } else { DOUBLE }),
                    Step::Crashed => {
                        motor.play(DONE);
                        over_at = now;
                        if g.score > best {
                            best = g.score;
                            api.save(&encode(best)); // on change: the app can be ended without notice
                        }
                    }
                    Step::Nothing => {}
                }
            }
            if g.state == State::Over && finger.is_some_and(|f| f.state == State::Playing) {
                finger = None; // the hold that ended the game ends with it
            }
            let before = on_screen;
            let now_seen = seen(&g, best);
            if redraw_all || before != Some(now_seen) {
                on_screen = Some(frame(&mut screen, &g, best, if redraw_all { None } else { before }));
                api.show();
            }
            api.keep_screen_on(g.state == State::Playing);
        }
    }
}
