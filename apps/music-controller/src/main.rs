#![cfg_attr(
    any(target_os = "none", wp2_sim),
    no_main,
    cfg_attr(target_os = "none", no_std)
)]

#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod artwork;
#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod model;
#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod ntype82;
#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod rpc;
#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod track;
#[cfg_attr(not(any(target_os = "none", wp2_sim)), allow(dead_code))]
mod view;

#[cfg(not(any(target_os = "none", wp2_sim)))]
fn main() {}

#[cfg(any(target_os = "none", wp2_sim))]
mod device {
    use crate::model::{wall_min, Got, Model, BLOB_MAX};
    use crate::rpc;
    use crate::view::{action, frame, Action};
    use cmf_wp2_app::{Api, BUTTON, CROWN, MSG_MAX, PAUSE, RESUME, TOUCH_DOWN};
    use cmf_wp2_ui::haptic::{Motor, TICK};
    use core::ptr::addr_of_mut;

    cmf_wp2_app::entry!(main);
    #[cfg(not(wp2_sim))]
    cmf_wp2_app::app_icon!();

    // Keep the model and cache buffer off the 8 KiB stack.
    static mut MODEL: Model = Model::new();
    static mut BLOB: [u8; BLOB_MAX] = [0; BLOB_MAX];

    /// Local wall-clock minutes, used for the cache timestamp rather than the monotonic millisecond clock.
    fn wall(api: &Api) -> i32 {
        api.time().map_or(0, |t| {
            wall_min(
                t.year as i32,
                t.month as u32,
                t.day as u32,
                t.hour as u32,
                t.minute as u32,
            )
        })
    }

    /// Persist metadata only when it changed and the NAND write succeeded.
    fn save_cache(api: &Api, model: &mut Model, blob: &mut [u8; BLOB_MAX]) {
        if !model.cache_dirty() {
            return;
        }
        let len = model.save(blob);
        if len > 0 && api.save(&blob[..len]) == 0 {
            model.cache_saved();
        }
    }

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

    fn main(api: &'static Api) -> i32 {
        let m = unsafe { &mut *addr_of_mut!(MODEL) };
        let blob = unsafe { &mut *addr_of_mut!(BLOB) };
        let n = api.load(blob);
        if n > 0 {
            m.load(&blob[..n as usize]);
        }
        m.ask_now(api.now());

        let mut screen = api;
        let mut motor = Motor::new(api);
        let mut drawn = None;
        let mut msg = [0u8; MSG_MAX];
        loop {
            motor.update();
            let now = api.now();
            let wall = wall(api);
            while let Some(data) = api.recv(&mut msg) {
                if m.reply(data, now, wall) == Got::Save {
                    save_cache(api, m, blob); // on change: the app can end without notice
                }
            }
            if let Some(request) = m.poll(now) {
                if api.send(request.bytes()) != 0 {
                    // No host or a full outbox is not a 30-second silence; recover on the next backoff.
                    m.send_failed(now);
                    drawn = None;
                }
            }
            if drawn != Some(m.rev) {
                frame(&mut screen, m);
                api.show();
                drawn = Some(m.rev);
            }

            let wait = m.wait(api.now()).min(motor.wait().unwrap_or(u32::MAX));
            let Some(event) = api.event(wait) else {
                continue;
            };
            match event.kind {
                BUTTON => return 0,
                PAUSE => {
                    motor.off();
                    if !paused(api) {
                        return 0;
                    }
                    m.ask_now(api.now());
                    drawn = None;
                }
                RESUME => {
                    m.ask_now(api.now());
                    drawn = None;
                }
                TOUCH_DOWN => {
                    let action = action(
                        event.x as i32,
                        event.y as i32,
                        m.max_volume(),
                        m.controls_enabled(),
                        m.volume_enabled(),
                    );
                    let queued = match action {
                        Action::Previous => m.command(rpc::PREVIOUS, 0),
                        Action::Toggle => m.command(rpc::TOGGLE, 0),
                        Action::Next => m.command(rpc::NEXT, 0),
                        Action::VolumeUp => m.command(rpc::VOLUME_UP, 0),
                        Action::VolumeDown => m.command(rpc::VOLUME_DOWN, 0),
                        Action::SetVolume(value) => m.command(rpc::SET_VOLUME, value),
                        Action::None => false,
                    };
                    if queued {
                        motor.play(TICK);
                        save_cache(api, m, blob);
                        drawn = None;
                    }
                }
                CROWN => {
                    // The crown adjusts the visible music volume, while the button remains the back action.
                    if event.delta != 0 {
                        let command = if event.delta > 0 {
                            rpc::VOLUME_UP
                        } else {
                            rpc::VOLUME_DOWN
                        };
                        if m.command(command, 0) {
                            motor.play(TICK);
                            save_cache(api, m, blob);
                            drawn = None;
                        }
                    }
                }
                _ => {} // MESSAGE: drained at the top
            }
        }
    }
}
