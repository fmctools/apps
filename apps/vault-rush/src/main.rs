#![cfg_attr(any(target_os = "none", wp2_sim), no_main, cfg_attr(target_os = "none", no_std))]

#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod font;
#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod game;
#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod view;

#[cfg(not(any(target_os = "none", wp2_sim)))]
fn main() {}

#[cfg(any(target_os = "none", wp2_sim))]
mod device {
    use crate::game::{decode, encode, Feedback, Game, HEIST_TIMES_S, Mode, Records, State};
    use crate::view::{
        frame, heist_tile_rect, in_rect, HEIST_BACK_H, HEIST_BACK_W, HEIST_BACK_X, HEIST_BACK_Y,
        MENU_BLITZ_Y, MENU_BTN_H, MENU_BTN_W, MENU_BTN_X, MENU_CLASSIC_Y, MENU_HEIST_Y, OVER_BTN_H,
        OVER_BTN_W, OVER_BTN_X, OVER_MENU_Y, OVER_RETRY_Y,
    };
    use cmf_wp2_app::{Api, BUTTON, CROWN, PAUSE, RESUME, TOUCH_DOWN};
    use cmf_wp2_ui::haptic::{Motor, Pattern, ALERT, DONE, TICK};
    use cmf_wp2_ui::text::int;

    cmf_wp2_app::entry!(main);
    cmf_wp2_app::app_icon!();
    cmf_wp2_app::app_features!(cmf_wp2_app::WIDGET);

    const FRAME_MS: u32 = 33;
    const RETRY_MS: u32 = 450;

    // Tactile dial feedback patterns (all end with duty 0, under 1500 ms).
    const COLD_TICK: Pattern = Pattern::new(&[(180, 14), (0, 0)]);
    const WARM_TICK: Pattern = Pattern::new(&[(350, 22), (0, 0)]);
    const HOT_TICK: Pattern = Pattern::new(&[(460, 22), (0, 45), (460, 22), (0, 0)]);
    const EXACT_TICK: Pattern = Pattern::new(&[(750, 35), (0, 35), (540, 25), (0, 0)]);
    const PIN_LOCK: Pattern = Pattern::new(&[(820, 45), (0, 50), (620, 40), (0, 0)]);
    const SAFE_OPEN: Pattern = Pattern::new(&[(700, 65), (0, 45), (920, 120), (0, 0)]);
    const WARN_TICK: Pattern = Pattern::new(&[(300, 40), (0, 50), (300, 40), (0, 0)]);

    fn play_feedback(motor: &mut Motor, fb: Feedback) {
        match fb {
            Feedback::None => {}
            Feedback::ColdTick => motor.play(COLD_TICK),
            Feedback::WarmTick => motor.play(WARM_TICK),
            Feedback::HotTick => motor.play(HOT_TICK),
            Feedback::ExactTick => motor.play(EXACT_TICK),
            Feedback::PinLock => motor.play(PIN_LOCK),
            Feedback::SafeOpen => motor.play(SAFE_OPEN),
            Feedback::Miss => motor.play(ALERT),
            Feedback::Warn => motor.play(WARN_TICK),
            Feedback::GameOver => motor.play(DONE),
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
        push(&mut buf, &mut idx, b"C:");
        push(&mut buf, &mut idx, int(best.classic as i32, &mut db).as_bytes());
        push(&mut buf, &mut idx, b" B:");
        push(&mut buf, &mut idx, int(best.blitz as i32, &mut db).as_bytes());
        push(&mut buf, &mut idx, b"\nHeist ");
        if let Some((i, &ms)) = best
            .heist_best_ms
            .iter()
            .enumerate()
            .filter(|&(_, &v)| v > 0)
            .min_by_key(|&(_, &v)| v)
        {
            let tenths = (ms + 99) / 100;
            push(&mut buf, &mut idx, int(HEIST_TIMES_S[i] as i32, &mut db).as_bytes());
            push(&mut buf, &mut idx, b"s:");
            push(&mut buf, &mut idx, int((tenths / 10) as i32, &mut db).as_bytes());
            push(&mut buf, &mut idx, b".");
            push(&mut buf, &mut idx, &[b'0' + (tenths % 10) as u8, b's']);
        } else {
            push(&mut buf, &mut idx, b"--");
        }
        let value = core::str::from_utf8(&buf[..idx]).unwrap_or("C:0 B:0");
        api.widget("Vault Rush", value);
    }

    fn needs_screen_awake(state: State) -> bool {
        matches!(
            state,
            State::EnterBank
                | State::ZoomIn
                | State::Playing
                | State::Unlocking
                | State::Opening
                | State::ClassicGold
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_touch(
        api: &Api,
        game: &mut Game,
        best: &mut Records,
        motor: &mut Motor,
        tx: i32,
        ty: i32,
        now: u32,
        last_tick: &mut u32,
        end_at: &mut u32,
    ) {
        let mut start_mode = |game: &mut Game, mode: Mode, last_tick: &mut u32| {
            motor.play(TICK);
            if mode == Mode::Heist {
                game.start_heist(game.heist_idx);
            } else {
                game.start(mode);
            }
            api.keep_screen_on(true);
            *last_tick = now;
        };

        match game.state {
            State::Menu => {
                if in_rect(tx, ty, MENU_BTN_X, MENU_CLASSIC_Y, MENU_BTN_W, MENU_BTN_H) {
                    start_mode(game, Mode::Classic, last_tick);
                } else if in_rect(tx, ty, MENU_BTN_X, MENU_HEIST_Y, MENU_BTN_W, MENU_BTN_H) {
                    motor.play(TICK);
                    game.open_heist_select();
                } else if in_rect(tx, ty, MENU_BTN_X, MENU_BLITZ_Y, MENU_BTN_W, MENU_BTN_H) {
                    start_mode(game, Mode::Blitz, last_tick);
                }
            }
            State::HeistSelect => {
                if in_rect(tx, ty, HEIST_BACK_X, HEIST_BACK_Y, HEIST_BACK_W, HEIST_BACK_H) {
                    motor.play(TICK);
                    game.back_to_menu();
                } else {
                    for i in 0..HEIST_TIMES_S.len() {
                        let [rx, ry, rw, rh] = heist_tile_rect(i);
                        if in_rect(tx, ty, rx, ry, rw, rh) {
                            motor.play(TICK);
                            game.start_heist(i);
                            api.keep_screen_on(true);
                            *last_tick = now;
                            break;
                        }
                    }
                }
            }
            State::Playing => {
                let (fb, broke) = game.try_lock(best);
                play_feedback(motor, fb);
                if broke {
                    api.save(&encode(*best));
                    push_widget(api, *best);
                }
                if game.state == State::Over {
                    *end_at = now;
                    api.keep_screen_on(false);
                }
            }
            State::ClassicGold => {
                motor.play(TICK);
                game.skip_gold_hold();
            }
            State::Over | State::HeistWin if !due(now, end_at.wrapping_add(RETRY_MS)) => {}
            State::Over | State::HeistWin => {
                if in_rect(tx, ty, OVER_BTN_X, OVER_RETRY_Y, OVER_BTN_W, OVER_BTN_H) {
                    start_mode(game, game.mode, last_tick);
                } else if in_rect(tx, ty, OVER_BTN_X, OVER_MENU_Y, OVER_BTN_W, OVER_BTN_H) {
                    motor.play(TICK);
                    game.back_to_menu();
                } else if game.over_sel == 0 {
                    start_mode(game, game.mode, last_tick);
                } else {
                    motor.play(TICK);
                    game.back_to_menu();
                }
            }
            State::EnterBank | State::ZoomIn | State::Unlocking | State::Opening => {}
        }
    }

    fn main(api: &'static Api) -> i32 {
        let mut screen = api;
        let mut motor = Motor::new(api);
        let mut blob = [0u8; 40];
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
            let animating = game.animating();
            let frame_wait = if animating || dirty {
                (next_frame.wrapping_sub(now) as i32).max(0) as u32
            } else {
                u32::MAX
            };
            let wait = motor.wait().map_or(frame_wait, |m| m.min(frame_wait));
            let mut force_draw = false;

            if let Some(first) = api.event(wait) {
                let mut crown_fb: Option<Feedback> = None;
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
                            crown_fb = None;
                            break;
                        }
                        CROWN => {
                            crown_fb = Some(game.crown(e.delta));
                            dirty = true;
                        }
                        TOUCH_DOWN => {
                            if let Some(fb) = crown_fb.take() {
                                play_feedback(&mut motor, fb);
                            }
                            handle_touch(
                                api,
                                &mut game,
                                &mut best,
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
                if let Some(fb) = crown_fb {
                    play_feedback(&mut motor, fb);
                }
            }

            let now = api.now();
            let animating_now = game.animating();
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
                    if prev_state != game.state && matches!(game.state, State::Over | State::HeistWin) {
                        end_at = now;
                        api.keep_screen_on(false);
                    }
                }
                dirty = true;
            }

            if (dirty || game.animating()) && (force_draw || due(now, next_frame)) {
                let (next_seen, drew) = frame(&mut screen, &game, best, Some(seen));
                seen = next_seen;
                if drew {
                    api.show();
                }
                dirty = false;
                let stepped = next_frame.wrapping_add(FRAME_MS);
                next_frame = if due(now, stepped) { now.wrapping_add(FRAME_MS) } else { stepped };
            }
        }
    }
}
