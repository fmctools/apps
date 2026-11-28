//! Lake Cast device entrypoint, gentle haptics, widget sync, and compact 40ms frame pacing.

#![cfg_attr(any(target_os = "none", wp2_sim), no_main, cfg_attr(target_os = "none", no_std))]

#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod font;
#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod game;
#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod sprites;
#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod view;

#[cfg(not(any(target_os = "none", wp2_sim)))]
fn main() {}

#[cfg(any(target_os = "none", wp2_sim))]
mod device {
    use crate::game::{
        decode, encode, format_weight, in_menu_back_pill, Feedback, Game, Records, SAVE_LEN, State,
        CAST_Y_MIN, GAUGE_H, GAUGE_Y, HUD_H, HUD_Y, SHORE_Y, TRACK_Y,
    };
    use crate::view::frame;
    use cmf_wp2_app::{Api, BUTTON, CROWN, PAUSE, RESUME, TOUCH_DOWN};
    use cmf_wp2_ui::haptic::{Motor, Pattern, DONE, TICK};
    use cmf_wp2_ui::text::int;

    cmf_wp2_app::entry!(main);
    cmf_wp2_app::app_icon!();
    cmf_wp2_app::app_features!(cmf_wp2_app::WIDGET);

    const FRAME_MS: u32 = 40;
    const RETRY_MS: u32 = 450;

    const SPLASH_PULSE: Pattern = Pattern::new(&[(320, 26), (0, 0)]);
    const NIBBLE_PULSE: Pattern = Pattern::new(&[(240, 22), (0, 0)]);
    const BITE_PULSE: Pattern = Pattern::new(&[(860, 65), (0, 45), (680, 50), (0, 0)]);
    const BAR_TICK: Pattern = Pattern::new(&[(180, 12), (0, 0)]);
    const HOLD_PULSE: Pattern = Pattern::new(&[(340, 20), (0, 0)]);
    const CAUGHT_PULSE: Pattern = Pattern::new(&[(650, 55), (0, 40), (920, 110), (0, 0)]);

    fn play_feedback(motor: &mut Motor, fb: Feedback) {
        match fb {
            Feedback::None => {}
            Feedback::Tap => motor.play(TICK),
            Feedback::CastSplash => motor.play(SPLASH_PULSE),
            Feedback::Nibble => motor.play(NIBBLE_PULSE),
            Feedback::Bite => motor.play(BITE_PULSE),
            Feedback::BarTick => motor.play(BAR_TICK),
            Feedback::CatchHold => motor.play(HOLD_PULSE),
            Feedback::Caught => motor.play(CAUGHT_PULSE),
            Feedback::Escape | Feedback::DerbyEnd => motor.play(DONE),
        }
    }

    fn due(now: u32, at: u32) -> bool {
        (now.wrapping_sub(at) as i32) >= 0
    }

    fn seed(api: &Api) -> u32 {
        let t = api
            .time()
            .map_or(0, |t| (t.minute as u32 * 60 + t.second as u32) * 1000 + t.millis as u32);
        api.now().wrapping_mul(2_654_435_761) ^ t
    }

    fn paused(api: &Api) -> bool {
        loop {
            match api.event(u32::MAX).map(|e| e.kind) {
                Some(RESUME) => return true,
                Some(BUTTON) => return false,
                _ => {}
            }
        }
    }

    fn push_widget(api: &Api, best: Records) {
        let mut buf = [0u8; 39];
        let mut idx = 0usize;
        let push = |buf: &mut [u8], idx: &mut usize, bytes: &[u8]| {
            let n = bytes.len().min(buf.len() - *idx);
            buf[*idx..*idx + n].copy_from_slice(&bytes[..n]);
            *idx += n;
        };
        let mut db = [0u8; 11];
        push(&mut buf, &mut idx, b"Best:");
        push(&mut buf, &mut idx, int(best.best_overall_score() as i32, &mut db).as_bytes());
        push(&mut buf, &mut idx, b" Big:");
        if best.biggest_hg > 0 {
            let mut wb = [0u8; 16];
            push(&mut buf, &mut idx, format_weight(best.biggest_hg, &mut wb).as_bytes());
        } else {
            push(&mut buf, &mut idx, b"--");
        }
        push(&mut buf, &mut idx, b"\nCaught:");
        push(&mut buf, &mut idx, int(best.total_caught as i32, &mut db).as_bytes());
        push(&mut buf, &mut idx, b" (");
        push(&mut buf, &mut idx, int(best.species_unlocked() as i32, &mut db).as_bytes());
        push(&mut buf, &mut idx, b"/6)");
        let value = core::str::from_utf8(&buf[..idx]).unwrap_or("Best:0");
        api.widget("Lake Cast", value);
    }

    fn needs_screen_awake(state: State) -> bool {
        matches!(
            state,
            State::Casting | State::Floating | State::Nibble | State::Bite | State::Reeling
        )
    }

    fn handle_touch(
        api: &Api,
        game: &mut Game,
        motor: &mut Motor,
        tx: i32,
        ty: i32,
        now: u32,
        last_tick: &mut u32,
        end_at: &mut u32,
    ) {
        if matches!(
            game.state,
            State::Menu | State::Difficulty | State::Trophies | State::Caught | State::DerbyOver
        ) {
            if game.state == State::DerbyOver && !due(now, end_at.wrapping_add(RETRY_MS)) {
                return;
            }
            let prev = game.state;
            let (fb, reset_tick) = game.tap_ui(tx, ty);
            play_feedback(motor, fb);
            if reset_tick {
                *last_tick = now;
            }
            if prev == State::Caught && game.state == State::Aim {
                api.keep_screen_on(false);
            }
            return;
        }

        if in_menu_back_pill(tx, ty) {
            motor.play(TICK);
            api.keep_screen_on(false);
            game.back_to_menu();
            *last_tick = now;
            return;
        }

        match game.state {
            State::Aim | State::Floating | State::Banner => {
                if ty >= CAST_Y_MIN - 4 && ty <= SHORE_Y + 20 {
                    let fb = game.cast_to(tx, ty);
                    play_feedback(motor, fb);
                    api.keep_screen_on(true);
                    *last_tick = now;
                }
            }
            State::Nibble => {
                let dx = tx - game.bx;
                let dy = ty - game.by;
                if dx * dx + dy * dy > 44 * 44 && ty >= CAST_Y_MIN - 4 && ty <= SHORE_Y + 20 {
                    let fb = game.cast_to(tx, ty);
                    play_feedback(motor, fb);
                } else if ty >= HUD_Y + HUD_H + 8 {
                    let fb = game.try_hook();
                    play_feedback(motor, fb);
                }
            }
            State::Bite => {
                if ty >= HUD_Y + HUD_H + 8 {
                    let fb = game.try_hook();
                    play_feedback(motor, fb);
                }
            }
            State::Reeling => {
                if ty >= GAUGE_Y - 12 && ty <= GAUGE_Y + GAUGE_H + 12 {
                    let bar_screen_mid = TRACK_Y + game.bar_y + game.bar_h / 2;
                    let delta = if ty < bar_screen_mid { -1 } else { 1 };
                    let fb = game.crown(delta);
                    play_feedback(motor, fb);
                }
            }
            _ => {}
        }
    }

    fn main(api: &'static Api) -> i32 {
        let mut screen = api;
        let mut motor = Motor::new(api);
        let mut blob = [0u8; SAVE_LEN + 8];
        let n = api.load(&mut blob);
        let mut best = if n > 0 { decode(&blob[..n as usize]) } else { Records::default() };
        push_widget(api, best);

        let mut game = Game::new(seed(api));
        let (mut seen, _) = frame(&mut screen, &game, best, None);
        api.show();

        let mut last_tick = api.now();
        let mut next_frame = last_tick.wrapping_add(FRAME_MS);
        let mut end_at = 0u32;
        let mut dirty = false;

        loop {
            motor.update();

            let now = api.now();
            let animating = game.needs_tick();
            let frame_wait = if animating || dirty {
                (next_frame.wrapping_sub(now) as i32).max(0) as u32
            } else {
                u32::MAX
            };
            let wait = motor.wait().map_or(frame_wait, |m| m.min(frame_wait));
            let mut force_draw = false;

            if let Some(first) = api.event(wait) {
                let mut crown_delta = 0i32;
                let mut cur = Some(first);
                while let Some(e) = cur {
                    let ev_now = api.now();
                    match e.kind {
                        BUTTON => {
                            api.keep_screen_on(false);
                            return 0;
                        }
                        PAUSE => {
                            motor.off();
                            api.keep_screen_on(false);
                            if !paused(api) {
                                return 0;
                            }
                            let res_now = api.now();
                            last_tick = res_now;
                            next_frame = res_now.wrapping_add(FRAME_MS);
                            if needs_screen_awake(game.state) {
                                api.keep_screen_on(true);
                            }
                            let (s, _) = frame(&mut screen, &game, best, None);
                            seen = s;
                            api.show();
                            dirty = false;
                            crown_delta = 0;
                            break;
                        }
                        CROWN => {
                            crown_delta += e.delta as i32;
                            dirty = true;
                        }
                        TOUCH_DOWN => {
                            if crown_delta != 0 {
                                let fb = game.crown(crown_delta.clamp(-12, 12) as i16);
                                play_feedback(&mut motor, fb);
                                crown_delta = 0;
                            }
                            handle_touch(
                                api,
                                &mut game,
                                &mut motor,
                                e.x as i32,
                                e.y as i32,
                                ev_now,
                                &mut last_tick,
                                &mut end_at,
                            );
                            dirty = true;
                            force_draw = true;
                        }
                        _ => {}
                    }
                    cur = api.event(0);
                }
                if crown_delta != 0 {
                    let fb = game.crown(crown_delta.clamp(-12, 12) as i16);
                    play_feedback(&mut motor, fb);
                    dirty = true;
                }
            }

            let now = api.now();
            let animating_now = game.needs_tick();
            if animating_now && (force_draw || due(now, next_frame)) {
                let dt = now.wrapping_sub(last_tick);
                last_tick = now;
                if animating && dt > 0 {
                    let prev_state = game.state;
                    let (fb, broke) = game.tick(dt, &mut best);
                    if broke {
                        api.save(&encode(best));
                        push_widget(api, best);
                    }
                    if fb != Feedback::None {
                        play_feedback(&mut motor, fb);
                    }
                    if prev_state != game.state && matches!(game.state, State::Caught | State::DerbyOver | State::Aim) {
                        api.keep_screen_on(false);
                        if game.state == State::DerbyOver {
                            end_at = now;
                        }
                    }
                }
                dirty = true;
            }

            if (dirty || animating_now) && (force_draw || due(now, next_frame)) {
                let (next_seen, drew) = frame(&mut screen, &game, best, Some(seen));
                seen = next_seen;
                if drew {
                    api.show();
                }
                dirty = false;
                let stepped = next_frame.wrapping_add(FRAME_MS);
                next_frame = if due(now, stepped) {
                    now.wrapping_add(FRAME_MS)
                } else {
                    stepped
                };
            }
        }
    }
}
