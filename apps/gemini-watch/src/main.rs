#![no_std]
#![no_main]

use cmf_wp2_app::{
    entry,
    host_channel::{self, HostMessage},
    input::{Button, CrownDirection, TouchEvent},
    ui::{Color, Display, Rect, TextAlignment},
    App, AppContext,
};
use heapless::{String, Vec};

const MAX_RESPONSE_LEN: usize = 1024;

static PRESET_PROMPTS: &[&str] = &[
    "Summarize my day in 3 points",
    "Give me a quick motivation quote",
    "Explain quantum computing in 20 words",
    "Give me an interesting science fact",
    "Quick healthy desk stretch exercises",
    "What to cook with eggs & spinach?",
];

#[derive(PartialEq, Clone, Copy)]
enum AppState {
    SelectPrompt,
    WaitingResponse,
    DisplayResponse,
}

pub struct GeminiApp {
    state: AppState,
    selected_idx: usize,
    scroll_offset: i16,
    response_buffer: String<MAX_RESPONSE_LEN>,
}

impl App for GeminiApp {
    fn new(_ctx: &mut AppContext) -> Self {
        Self {
            state: AppState::SelectPrompt,
            selected_idx: 0,
            scroll_offset: 0,
            response_buffer: String::new(),
        }
    }

    fn update(&mut self, _ctx: &mut AppContext) {
        while let Some(msg) = host_channel::receive() {
            match msg {
                HostMessage::Payload(data) => {
                    if let Ok(text) = core::str::from_utf8(&data) {
                        if text == "[DONE]" {
                            self.state = AppState::DisplayResponse;
                            self.scroll_offset = 0;
                        } else {
                            let _ = self.response_buffer.push_str(text);
                        }
                    }
                }
                HostMessage::Error => {
                    self.response_buffer.clear();
                    let _ = self.response_buffer.push_str("Bridge error. Retry.");
                    self.state = AppState::DisplayResponse;
                }
            }
        }
    }

    fn on_touch(&mut self, _ctx: &mut AppContext, event: TouchEvent) {
        match self.state {
            AppState::SelectPrompt => {
                if event.is_tap() {
                    self.send_prompt(PRESET_PROMPTS[self.selected_idx]);
                }
            }
            AppState::DisplayResponse => {
                if event.is_tap() {
                    self.state = AppState::SelectPrompt;
                    self.response_buffer.clear();
                }
            }
            _ => {}
        }
    }

    fn on_crown(&mut self, _ctx: &mut AppContext, direction: CrownDirection) {
        match self.state {
            AppState::SelectPrompt => match direction {
                CrownDirection::Clockwise => {
                    if self.selected_idx + 1 < PRESET_PROMPTS.len() {
                        self.selected_idx += 1;
                    }
                }
                CrownDirection::CounterClockwise => {
                    if self.selected_idx > 0 {
                        self.selected_idx -= 1;
                    }
                }
            },
            AppState::DisplayResponse => match direction {
                CrownDirection::Clockwise => {
                    self.scroll_offset += 24;
                }
                CrownDirection::CounterClockwise => {
                    if self.scroll_offset >= 24 {
                        self.scroll_offset -= 24;
                    }
                }
            },
            _ => {}
        }
    }

    fn render(&self, display: &mut Display) {
        display.clear(Color::BLACK);

        match self.state {
            AppState::SelectPrompt => {
                display.draw_text(
                    "Gemini AI",
                    Rect::new(20, 20, 200, 30),
                    Color::ORANGE,
                    TextAlignment::Center,
                );

                let start_y = 60;
                for (i, prompt) in PRESET_PROMPTS.iter().enumerate() {
                    let y = start_y + (i as i16 * 40);
                    let is_active = i == self.selected_idx;
                    let text_color = if is_active { Color::WHITE } else { Color::GRAY };
                    let bg_color = if is_active { Color::DARK_GRAY } else { Color::BLACK };

                    display.fill_rect(Rect::new(10, y, 220, 35), bg_color);
                    display.draw_text_truncated(
                        prompt,
                        Rect::new(15, y + 8, 210, 30),
                        text_color,
                    );
                }
            }
            AppState::WaitingResponse => {
                display.draw_text(
                    "Querying Gemini...",
                    Rect::new(20, 100, 200, 30),
                    Color::CYAN,
                    TextAlignment::Center,
                );
                display.draw_text(
                    "Waiting for BLE bridge",
                    Rect::new(20, 140, 200, 30),
                    Color::GRAY,
                    TextAlignment::Center,
                );
            }
            AppState::DisplayResponse => {
                display.draw_text_multiline(
                    self.response_buffer.as_str(),
                    Rect::new(15, 30 - self.scroll_offset, 210, 800),
                    Color::WHITE,
                );
            }
        }
    }
}

impl GeminiApp {
    fn send_prompt(&mut self, prompt: &str) {
        self.response_buffer.clear();
        self.state = AppState::WaitingResponse;
        let _ = host_channel::send_event(prompt.as_bytes());
    }
}

entry!(GeminiApp);
