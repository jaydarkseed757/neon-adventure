use std::collections::VecDeque;
use macroquad::input::{get_char_pressed, is_key_down, is_key_pressed, KeyCode};
use macroquad::time::get_time;

const HISTORY_SIZE: usize = 5;
const REPEAT_INITIAL: f64 = 0.40; // seconds before key repeat starts
const REPEAT_RATE:    f64 = 0.05; // seconds between repeat firings

pub struct InputState {
    pub buffer: String,
    history:      VecDeque<String>,
    history_pos:  Option<usize>, // None = editing live input
    pub cursor_pos:   usize,     // byte index into buffer
    blink_timer:  f64,
    pub cursor_visible: bool,
    backspace_next: f64, // time of next backspace repeat fire
}

impl InputState {
    pub fn new() -> Self {
        InputState {
            buffer: String::new(),
            history: VecDeque::new(),
            history_pos: None,
            cursor_pos: 0,
            blink_timer: 0.0,
            cursor_visible: true,
            backspace_next: f64::MAX,
        }
    }

    /// Process input for one frame. Returns a submitted command string if Enter was pressed.
    pub fn handle_frame(&mut self) -> Option<String> {
        let now = get_time();

        // Cursor blink
        if now - self.blink_timer > 0.5 {
            self.cursor_visible = !self.cursor_visible;
            self.blink_timer = now;
        }

        // Printable characters
        while let Some(ch) = get_char_pressed() {
            if !ch.is_control() {
                self.buffer.insert(self.cursor_pos, ch);
                self.cursor_pos += ch.len_utf8();
                self.history_pos = None;
                self.reset_blink();
            }
        }

        // Backspace — single press
        if is_key_pressed(KeyCode::Backspace) {
            self.do_backspace();
            self.backspace_next = now + REPEAT_INITIAL;
            self.reset_blink();
        } else if is_key_down(KeyCode::Backspace) && now >= self.backspace_next {
            self.do_backspace();
            self.backspace_next = now + REPEAT_RATE;
            self.reset_blink();
        }
        if is_key_pressed(KeyCode::Delete) {
            self.do_delete();
            self.reset_blink();
        }

        // Cursor movement
        if is_key_pressed(KeyCode::Left) && self.cursor_pos > 0 {
            let ch = self.buffer[..self.cursor_pos].chars().last().unwrap();
            self.cursor_pos -= ch.len_utf8();
            self.reset_blink();
        }
        if is_key_pressed(KeyCode::Right) && self.cursor_pos < self.buffer.len() {
            let ch = self.buffer[self.cursor_pos..].chars().next().unwrap();
            self.cursor_pos += ch.len_utf8();
            self.reset_blink();
        }
        if is_key_pressed(KeyCode::Home) {
            self.cursor_pos = 0;
            self.reset_blink();
        }
        if is_key_pressed(KeyCode::End) {
            self.cursor_pos = self.buffer.len();
            self.reset_blink();
        }

        // History navigation
        if is_key_pressed(KeyCode::Up) {
            self.history_up();
            self.reset_blink();
        }
        if is_key_pressed(KeyCode::Down) {
            self.history_down();
            self.reset_blink();
        }

        // Submit on Enter
        if is_key_pressed(KeyCode::Enter) && !self.buffer.trim().is_empty() {
            let submitted = self.buffer.trim().to_lowercase();
            // Store original capitalisation in history
            let raw = self.buffer.trim().to_string();
            if self.history.front().map_or(true, |h| h != &raw) {
                self.history.push_front(raw);
                if self.history.len() > HISTORY_SIZE {
                    self.history.pop_back();
                }
            }
            self.buffer.clear();
            self.cursor_pos = 0;
            self.history_pos = None;
            self.backspace_next = f64::MAX;
            self.reset_blink();
            return Some(submitted);
        }

        None
    }

    fn do_backspace(&mut self) {
        if self.cursor_pos > 0 {
            let ch = self.buffer[..self.cursor_pos].chars().last().unwrap();
            self.cursor_pos -= ch.len_utf8();
            self.buffer.remove(self.cursor_pos);
            self.history_pos = None;
        }
    }

    fn do_delete(&mut self) {
        if self.cursor_pos < self.buffer.len() {
            self.buffer.remove(self.cursor_pos);
            self.history_pos = None;
        }
    }

    fn history_up(&mut self) {
        if self.history.is_empty() { return; }
        let next = match self.history_pos {
            None    => 0,
            Some(n) => (n + 1).min(self.history.len() - 1),
        };
        self.history_pos = Some(next);
        self.buffer = self.history[next].clone();
        self.cursor_pos = self.buffer.len();
    }

    fn history_down(&mut self) {
        match self.history_pos {
            None => {}
            Some(0) => {
                self.history_pos = None;
                self.buffer.clear();
                self.cursor_pos = 0;
            }
            Some(n) => {
                let prev = n - 1;
                self.history_pos = Some(prev);
                self.buffer = self.history[prev].clone();
                self.cursor_pos = self.buffer.len();
            }
        }
    }

    fn reset_blink(&mut self) {
        self.cursor_visible = true;
        self.blink_timer = get_time();
    }
}
