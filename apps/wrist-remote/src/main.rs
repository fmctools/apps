#![cfg_attr(any(target_os = "none", wp2_sim), no_main, cfg_attr(target_os = "none", no_std))]

mod logic;

#[cfg(not(any(target_os = "none", wp2_sim)))]
fn main() {}

#[cfg(any(target_os = "none", wp2_sim))]
mod device {
    use crate::logic::*;
    use cmf_wp2_app::{
        Api, BUTTON, CROWN, MEDIA_NEXT, MEDIA_PAUSE, MEDIA_PLAY, MEDIA_PREV, MEDIA_VOLUME_DOWN,
        MEDIA_VOLUME_UP, MESSAGE, MSG_MAX, PAUSE, RESUME, TOUCH_DOWN,
    };
    use core::fmt::Write;

    cmf_wp2_app::entry!(main);
    cmf_wp2_app::app_icon!();

    const BLACK: u16 = 0x0000;
    const WHITE: u16 = 0xFFFF;
    const RED: u16 = 0xF9C6;
    const GREY: u16 = 0x8410;
    const ROW_BG: u16 = 0x2124;
    const HOME_TOP: i16 = 112;
    const HOME_ROW: i16 = 52;
    const HOME_ROWS: usize = 5;
    const ASK_MS: u32 = 2000;
    const RETRY_MS: u32 = 1200;
    const SCREEN_MS: u32 = 15000;
    const HAPTIC_MS: u32 = 20;

    #[derive(Clone, Copy, PartialEq)]
    enum Screen {
        Home,
        Remote,
    }

    struct State {
        screen: Screen,
        slot: u8,
        page: u8,
        top: u8,
        names: [Name; MAX_DEVICES],
        ndev: usize,
        buttons: [Button; MAX_BUTTONS],
        nbtn: usize,
        host_list: bool,
        pending: bool,
        pending_id: u8,
        pending_cmd: u8,
        pending_at: u32,
        last_ask: u32,
        next_id: u8,
        last_reply: u32,
        linked: bool,
        status: u8,
        screen_deadline: u32,
        motor_off: u32,
        need_draw: bool,
        paused: bool,
    }

    impl State {
        const fn new() -> Self {
            let mut names = [Name::from(b""); MAX_DEVICES];
            names[0] = Name::from(b"TV");
            names[1] = Name::from(b"PC");
            names[2] = Name::from(b"Media");
            names[3] = Name::from(b"Other");
            Self {
                screen: Screen::Home,
                slot: 0,
                page: 0,
                top: 0,
                names,
                ndev: 4,
                buttons: [Button::EMPTY; MAX_BUTTONS],
                nbtn: 0,
                host_list: false,
                pending: false,
                pending_id: 0,
                pending_cmd: 0,
                pending_at: 0,
                last_ask: 0,
                next_id: 1,
                last_reply: 0,
                linked: false,
                status: 0,
                screen_deadline: 0,
                motor_off: 0,
                need_draw: false,
                paused: false,
            }
        }
    }

    static mut STATE: State = State::new();

    fn request(api: &Api, st: &mut State, cmd: u8, payload: &[u8]) {
        if st.pending || st.paused {
            return;
        }
        let mut f = [0u8; MSG_MAX];
        let Some(n) = frame(&mut f, st.next_id, cmd, payload) else {
            return;
        };
        if api.send(&f[..n]) != 0 {
            st.linked = false;
            return;
        }
        st.pending = true;
        st.pending_id = st.next_id;
        st.pending_cmd = cmd;
        st.pending_at = api.now();
        st.last_ask = st.pending_at;
        st.next_id = st.next_id.wrapping_add(1);
    }

    fn on_reply(st: &mut State, r: Reply<'_>) {
        st.linked = true;
        st.status = r.status;
        st.pending = false;
        if r.status != OK {
            st.need_draw = true;
            return;
        }
        match st.pending_cmd {
            LIST => {
                let n = decode_list(r.payload, &mut st.names);
                if n > 0 {
                    st.ndev = n;
                    st.host_list = true;
                    if st.slot as usize >= n {
                        st.slot = 0;
                    }
                }
            }
            BTN => {
                st.nbtn = decode_buttons(r.payload, &mut st.buttons);
            }
            _ => {}
        }
        st.need_draw = true;
    }

    fn poll(api: &Api, st: &mut State) {
        let mut msg = [0u8; MSG_MAX];
        while let Some(m) = api.recv(&mut msg) {
            if let Some(r) = parse(m) {
                if st.pending && r.id == st.pending_id {
                    let now = api.now();
                    on_reply(st, r);
                    st.last_reply = now;
                }
            }
        }
        let now = api.now();
        if st.motor_off != 0 && (now.wrapping_sub(st.motor_off) as i32) >= 0 {
            api.vibrate(0, 31);
            st.motor_off = 0;
        }
        if st.pending && (now.wrapping_sub(st.pending_at) as i32) > RETRY_MS as i32 {
            st.pending = false;
        }
        if !st.pending && !st.paused && (now.wrapping_sub(st.last_ask) as i32) >= ASK_MS as i32 {
            if st.host_list {
                request(api, st, PING, &[]);
            } else {
                request(api, st, LIST, &[]);
            }
        }
        if st.linked && (now.wrapping_sub(st.last_reply) as i32) > 6000 {
            st.linked = false;
            st.need_draw = true;
        }
    }

    fn buzz(api: &Api, st: &mut State) {
        if api.vibrate(250, 31) == 0 {
            st.motor_off = api.now().wrapping_add(HAPTIC_MS);
        }
    }

    fn save_slot(api: &Api, st: &State) {
        // Saved on change: the loader can end the app without any event.
        let _ = api.save(&[b'W', 1, st.slot]);
    }

    fn open_device(api: &Api, st: &mut State, slot: usize) {
        st.slot = slot as u8;
        st.screen = Screen::Remote;
        st.page = 0;
        st.nbtn = 0;
        st.need_draw = true;
        buzz(api, st);
        request(api, st, BTN, &[st.slot, st.page]);
        save_slot(api, st);
    }

    fn phone_media(cmd: &[u8]) -> bool {
        if cmd.len() < 7 || &cmd[..6] != b"phone:" {
            return false;
        }
        let key = match &cmd[6..] {
            b"play" => MEDIA_PLAY,
            b"pause" => MEDIA_PAUSE,
            b"next" => MEDIA_NEXT,
            b"prev" => MEDIA_PREV,
            b"volup" => MEDIA_VOLUME_UP,
            b"voldn" => MEDIA_VOLUME_DOWN,
            _ => return true,
        };
        let _ = cmf_wp2_app::api().map(|a| a.media_key(key));
        true
    }

    fn tap(api: &Api, st: &mut State, x: i16, y: i16) {
        match st.screen {
            Screen::Home => {
                if let Some(i) = row_hit(y, HOME_TOP, HOME_ROW, st.ndev.min(HOME_ROWS)) {
                    let slot = st.top as usize + i;
                    if slot < st.ndev {
                        open_device(api, st, slot);
                    }
                }
            }
            Screen::Remote => {
                if x < 70 && y < 90 {
                    st.screen = Screen::Home;
                    st.need_draw = true;
                    return;
                }
                if x > 380 && y < 90 {
                    st.page = 1 - st.page;
                    st.nbtn = 0;
                    st.need_draw = true;
                    request(api, st, BTN, &[st.slot, st.page]);
                    return;
                }
                if let Some(i) = grid_hit(x, y) {
                    if i >= st.nbtn {
                        return;
                    }
                    let cmd = st.buttons[i].command();
                    if !phone_media(cmd) {
                        request(api, st, KEY, &[st.slot, st.page, i as u8]);
                    }
                    buzz(api, st);
                }
            }
        }
    }

    fn draw(api: &Api, st: &State) {
        api.rect(0, 0, api.width, api.height, BLACK);
        let mut line = Line::new();
        let _ = write!(line, "WRIST REMOTE");
        api.text(centred_x(line.n / 2), 40, line.as_str(), WHITE);

        line.clear();
        let _ = write!(line, "{}", if st.linked { "LINK OK" } else { "NO LINK" });
        api.text(centred_x(line.n / 2), 70, line.as_str(), if st.linked { RED } else { GREY });

        match st.screen {
            Screen::Home => {
                let shown = st.ndev.min(HOME_ROWS);
                for row in 0..shown {
                    let y = HOME_TOP + row as i16 * HOME_ROW;
                    let slot = st.top as usize + row;
                    if slot == st.slot as usize {
                        api.rect(24, y - 6, 418, 44, ROW_BG);
                    }
                    let name = st.names[slot].as_str();
                    api.text(centred_x(name.chars().count()), y, name, if slot == st.slot as usize { RED } else { WHITE });
                }
                line.clear();
                let _ = write!(line, "tap open / crown move");
                api.text(centred_x(line.n / 2), 392, line.as_str(), GREY);
            }
            Screen::Remote => {
                api.text(30, 40, "<", WHITE);
                api.text(410, 40, ">", WHITE);
                let name = st.names[st.slot as usize].as_str();
                line.clear();
                let _ = write!(line, "{}  {}/2", name, st.page + 1);
                api.text(centred_x(line.n / 2), 100, line.as_str(), RED);
                for i in 0..st.nbtn.min(MAX_BUTTONS) {
                    let (x, y) = match i {
                        0 => (BTN_X0, BTN_Y0),
                        1 => (BTN_X1, BTN_Y0),
                        2 => (BTN_X0, BTN_Y1),
                        _ => (BTN_X1, BTN_Y1),
                    };
                    api.rect(x, y, BTN_W, BTN_H, ROW_BG);
                    let label = st.buttons[i].label();
                    api.text(x + (BTN_W as i16 - 8 * label.chars().count() as i16) / 2, y + 24, label, WHITE);
                }
                line.clear();
                let _ = write!(line, "crown volume / < back");
                api.text(centred_x(line.n / 2), 392, line.as_str(), GREY);
            }
        }

        line.clear();
        let _ = write!(line, "{}", if st.status == OK { "READY" } else { "CMD ERR" });
        api.text(centred_x(line.n / 2), 414, line.as_str(), if st.status == OK { GREY } else { RED });
        api.show();
    }

    fn paused(api: &Api, st: &mut State) -> bool {
        st.paused = true;
        api.vibrate(0, 31);
        api.keep_screen_on(false);
        loop {
            match api.event(u32::MAX).map(|e| e.kind) {
                Some(RESUME) => {
                    st.paused = false;
                    st.pending = false;
                    st.need_draw = true;
                    return true;
                }
                Some(BUTTON) => return false,
                _ => {}
            }
        }
    }

    fn main(api: &'static Api) -> i32 {
        let st = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
        let mut blob = [0u8; 4];
        let n = api.load(&mut blob);
        if n >= 3 && blob[0] == b'W' && blob[1] == 1 && blob[2] < MAX_DEVICES as u8 {
            st.slot = blob[2];
            if st.slot as usize >= st.ndev {
                st.slot = 0;
            }
        }
        st.screen_deadline = api.now().wrapping_add(SCREEN_MS);
        api.keep_screen_on(false);
        draw(api, st);
        loop {
            let now = api.now();
            api.keep_screen_on(!st.paused && (now.wrapping_sub(st.screen_deadline) as i32) <= 0);
            poll(api, st);
            if st.need_draw {
                draw(api, st);
                st.need_draw = false;
            }
            let Some(e) = api.event(250) else {
                continue;
            };
            match e.kind {
                BUTTON => {
                    api.vibrate(0, 31);
                    api.keep_screen_on(false);
                    return 0;
                }
                PAUSE => {
                    if !paused(api, st) {
                        return 0;
                    }
                    continue;
                }
                RESUME => {
                    st.need_draw = true;
                }
                MESSAGE => {}
                CROWN if e.delta != 0 => {
                    st.screen_deadline = api.now().wrapping_add(SCREEN_MS);
                    match st.screen {
                        Screen::Home => {
                            let next = (st.slot as i16 + e.delta.signum()).rem_euclid(st.ndev as i16);
                            st.slot = next as u8;
                            if st.slot < st.top {
                                st.top = st.slot;
                            } else if st.slot as usize >= st.top as usize + HOME_ROWS {
                                st.top = st.slot - HOME_ROWS as u8 + 1;
                            }
                            st.need_draw = true;
                            save_slot(api, st);
                        }
                        Screen::Remote => {
                            let d = e.delta.signum() as i8;
                            request(api, st, VOL, &[st.slot, d as u8]);
                        }
                    }
                }
                TOUCH_DOWN => {
                    st.screen_deadline = api.now().wrapping_add(SCREEN_MS);
                    tap(api, st, e.x, e.y);
                }
                _ => {}
            }
            if st.need_draw {
                draw(api, st);
                st.need_draw = false;
            }
        }
    }
}
