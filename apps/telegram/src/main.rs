#![cfg_attr(any(target_os = "none", wp2_sim), no_main, cfg_attr(target_os = "none", no_std))]

#[cfg(not(any(target_os = "none", wp2_sim)))]
fn main() {}

#[cfg(any(target_os = "none", wp2_sim))]
mod device {
    use cmf_wp2_app::{Api, BUTTON, CROWN, RESUME, TOUCH_DOWN};

    cmf_wp2_app::entry!(main);
    cmf_wp2_app::app_icon!();

    const BLACK: u16 = 0x0000;
    const WHITE: u16 = 0xFFFF;
    const CARD_BG: u16 = 0x18E3;
    const BUBBLE_THEM: u16 = 0x2945;
    const TG_BLUE: u16 = 0x255D;
    const GREY_TEXT: u16 = 0x8C71;
    const PAD_BG: u16 = 0x2104;

    const KEYS: [u8; 33] = [
        b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H', b'I', b'J', b'K',
        b'L', b'M', b'N', b'O', b'P', b'Q', b'R', b'S', b'T', b'U', b'V',
        b'W', b'X', b'Y', b'Z', b'.', b',', b'?', b'!', b'_', b'<', b'>',
    ];

    #[derive(Clone, Copy, PartialEq)]
    enum Screen {
        ChatList,
        ChatView,
        Touchpad,
    }

    struct Chat {
        name: &'static str,
        last_msg: &'static str,
        unread: u8,
    }

    struct Message {
        is_me: bool,
        text: [u8; 48],
        len: usize,
    }

    impl Message {
        const fn from_static(is_me: bool, s: &str) -> Self {
            let mut text = [0u8; 48];
            let bytes = s.as_bytes();
            let mut i = 0;
            while i < bytes.len() && i < 48 {
                text[i] = bytes[i];
                i += 1;
            }
            Self { is_me, text, len: i }
        }

        fn as_str(&self) -> &str {
            core::str::from_utf8(&self.text[..self.len]).unwrap_or("")
        }
    }

    struct AppState {
        screen: Screen,
        selected_chat: usize,
        active_chat: usize,
        messages: [Message; 6],
        msg_count: usize,
        input_buf: [u8; 48],
        input_len: usize,
        cursor_idx: usize,
    }

    impl AppState {
        fn new() -> Self {
            let messages = [
                Message::from_static(false, "Privet! Kak chasy?"),
                Message::from_static(true, "Delau svoy TG na Rust!"),
                Message::from_static(false, "Krutaya klava-tachpad"),
                Message::from_static(false, ""),
                Message::from_static(false, ""),
                Message::from_static(false, ""),
            ];
            Self {
                screen: Screen::ChatList,
                selected_chat: 0,
                active_chat: 0,
                messages,
                msg_count: 3,
                input_buf: [0; 48],
                input_len: 0,
                cursor_idx: 0,
            }
        }

        fn input_str(&self) -> &str {
            core::str::from_utf8(&self.input_buf[..self.input_len]).unwrap_or("")
        }

        fn push_message(&mut self, is_me: bool) {
            if self.input_len == 0 {
                return;
            }
            let mut msg = Message {
                is_me,
                text: [0; 48],
                len: self.input_len,
            };
            msg.text[..self.input_len].copy_from_slice(&self.input_buf[..self.input_len]);

            if self.msg_count < self.messages.len() {
                self.messages[self.msg_count] = msg;
                self.msg_count += 1;
            } else {
                for i in 0..self.messages.len() - 1 {
                    self.messages[i] = Message {
                        is_me: self.messages[i + 1].is_me,
                        text: self.messages[i + 1].text,
                        len: self.messages[i + 1].len,
                    };
                }
                self.messages[self.messages.len() - 1] = msg;
            }
            self.input_len = 0;
        }
    }

    const CHATS: [Chat; 4] = [
        Chat { name: "Saved Messages", last_msg: "Notes & links", unread: 0 },
        Chat { name: "Alexey", last_msg: "Krutaya klava-tachpad", unread: 2 },
        Chat { name: "FMC Dev", last_msg: "New SDK released", unread: 5 },
        Chat { name: "Mom", last_msg: "Call me later", unread: 1 },
    ];

    fn draw_chat_list(api: &Api, state: &AppState) {
        api.rect(0, 0, api.width, api.height, BLACK);
        api.text(175, 24, "Telegram", TG_BLUE);

        let start_y: i16 = 65;
        let card_h: i16 = 82;

        for (i, chat) in CHATS.iter().enumerate() {
            let y = start_y + i as i16 * (card_h + 8);
            let bg = if i == state.selected_chat { 0x2965 } else { CARD_BG };
            api.rect(38, y, 390, card_h as u16, bg);

            api.text(54, y + 14, chat.name, WHITE);
            api.text(54, y + 46, chat.last_msg, GREY_TEXT);

            if chat.unread > 0 {
                api.rect(388, y + 26, 26, 26, TG_BLUE);
                let badge = [b'0' + chat.unread];
                if let Ok(s) = core::str::from_utf8(&badge) {
                    api.text(396, y + 31, s, WHITE);
                }
            }
        }
        api.show();
    }

    fn draw_chat_view(api: &Api, state: &AppState) {
        api.rect(0, 0, api.width, api.height, BLACK);
        api.text(48, 28, "Back", TG_BLUE);
        api.text(165, 28, CHATS[state.active_chat].name, WHITE);

        let start_idx = state.msg_count.saturating_sub(4);
        let mut y: i16 = 72;

        for i in start_idx..state.msg_count {
            let msg = &state.messages[i];
            let bubble_x: i16 = if msg.is_me { 115 } else { 42 };
            let bubble_col = if msg.is_me { TG_BLUE } else { BUBBLE_THEM };

            api.rect(bubble_x, y, 308, 62, bubble_col);
            api.text(bubble_x + 14, y + 22, msg.as_str(), WHITE);
            y += 72;
        }

        api.rect(123, 385, 220, 52, TG_BLUE);
        api.text(192, 403, "Reply", WHITE);
        api.show();
    }

    fn draw_touchpad(api: &Api, state: &AppState) {
        api.rect(0, 0, api.width, api.height, BLACK);

        api.rect(48, 36, 370, 48, CARD_BG);
        let text_to_show = if state.input_len > 0 {
            state.input_str()
        } else {
            "Tap pad to move/type"
        };
        let text_col = if state.input_len > 0 { WHITE } else { GREY_TEXT };
        api.text(62, 52, text_to_show, text_col);

        let cols: i16 = 11;
        let cell_w: i16 = 34;
        let cell_h: i16 = 38;
        let grid_x: i16 = 46;
        let grid_y: i16 = 100;

        for (idx, &ch) in KEYS.iter().enumerate() {
            let col = (idx as i16) % cols;
            let row = (idx as i16) / cols;
            let x = grid_x + col * cell_w;
            let y = grid_y + row * cell_h;

            if idx == state.cursor_idx {
                api.rect(x - 2, y - 2, cell_w as u16, cell_h as u16, TG_BLUE);
            }

            let buf = [ch];
            if let Ok(s) = core::str::from_utf8(&buf) {
                api.text(x + 9, y + 10, s, WHITE);
            }
        }

        api.rect(53, 230, 360, 195, PAD_BG);
        api.text(135, 318, "TOUCHPAD AREA", GREY_TEXT);
        api.text(115, 345, "Tap to move / Type", GREY_TEXT);
        api.show();
    }

    fn draw(api: &Api, state: &AppState) {
        match state.screen {
            Screen::ChatList => draw_chat_list(api, state),
            Screen::ChatView => draw_chat_view(api, state),
            Screen::Touchpad => draw_touchpad(api, state),
        }
    }

    fn main(api: &'static Api) -> i32 {
        let mut state = AppState::new();
        draw(api, &state);

        loop {
            let Some(event) = api.event(u32::MAX) else { continue };
            match event.kind {
                BUTTON => match state.screen {
                    Screen::Touchpad => state.screen = Screen::ChatView,
                    Screen::ChatView => state.screen = Screen::ChatList,
                    Screen::ChatList => return 0,
                },
                CROWN if event.delta != 0 => {
                    let dir = event.delta.signum();
                    match state.screen {
                        Screen::ChatList => {
                            let next = (state.selected_chat as i32 + dir as i32)
                                .clamp(0, (CHATS.len() - 1) as i32);
                            state.selected_chat = next as usize;
                        }
                        Screen::Touchpad => {
                            let next = (state.cursor_idx as i32 + dir as i32)
                                .rem_euclid(KEYS.len() as i32);
                            state.cursor_idx = next as usize;
                        }
                        _ => {}
                    }
                }
                TOUCH_DOWN => match state.screen {
                    Screen::ChatList => {
                        if event.y >= 65 && event.y < 65 + 4 * 90 {
                            let idx = ((event.y - 65) / 90) as usize;
                            if idx < CHATS.len() {
                                state.active_chat = idx;
                                state.screen = Screen::ChatView;
                            }
                        }
                    }
                    Screen::ChatView => {
                        if event.y < 70 && event.x < 160 {
                            state.screen = Screen::ChatList;
                        } else if event.y >= 380 {
                            state.input_len = 0;
                            state.screen = Screen::Touchpad;
                        }
                    }
                    Screen::Touchpad => {
                        if event.y >= 230 && event.y <= 425 && event.x >= 53 && event.x <= 413 {
                            let rel_x = (event.x - 53).clamp(0, 359);
                            let rel_y = (event.y - 230).clamp(0, 194);

                            let col = (rel_x * 11 / 360) as usize;
                            let row = (rel_y * 3 / 195) as usize;
                            let target_idx = (row * 11 + col).min(KEYS.len() - 1);

      if target_idx == state.cursor_idx {
let ch = KEYS[state.cursor_idx];
match ch {
b'_' => {
if state.input_len < state.input_buf.len() {
state.input_buf[state.input_len] = b' ';
state.input_len += 1;
}
}
b'<' => {
if state.input_len > 0 {
state.input_len -= 1;
}
}
b'>' => {
state.push_message(true);
state.screen = Screen::ChatView;
}
_ => {
if state.input_len < state.input_buf.len() {
state.input_buf[state.input_len] = ch;
state.input_len += 1;
}
}
}
} else {
state.cursor_idx = target_idx;
}
}
}
},
RESUME => {}
_ => continue,
}
draw(api, &state);
}
}
}
