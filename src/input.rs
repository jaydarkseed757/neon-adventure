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

    // Tab completion
    completion_verbs: Vec<String>,
    completion_nouns: Vec<String>,
    tab_matches:      Vec<String>, // current match list
    tab_cycle_idx:    usize,       // position within tab_matches
    tab_word_start:   usize,       // where the completed word begins (byte idx)
    tab_is_verb:      bool,        // completing first word (verb) vs noun
    in_tab_cycle:     bool,        // true while consecutive Tabs are cycling
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

            completion_verbs: Vec::new(),
            completion_nouns: Vec::new(),
            tab_matches:      Vec::new(),
            tab_cycle_idx:    0,
            tab_word_start:   0,
            tab_is_verb:      false,
            in_tab_cycle:     false,
        }
    }

    /// Set the word lists used by tab completion. Call once per frame before handle_frame().
    pub fn set_completions(&mut self, verbs: Vec<String>, nouns: Vec<String>) {
        self.completion_verbs = verbs;
        self.completion_nouns = nouns;
    }

    /// Process input for one frame.
    /// Returns `(submitted, dirty)` where `submitted` is a command string if Enter was
    /// pressed, and `dirty` is true if any visual state changed this frame.
    pub fn handle_frame(&mut self) -> (Option<String>, bool) {
        let now = get_time();
        let mut dirty = false;

        // Cursor blink
        if now - self.blink_timer > 0.5 {
            self.cursor_visible = !self.cursor_visible;
            self.blink_timer = now;
            dirty = true;
        }

        // Tab completion — handled before printable chars so Tab isn't also
        // consumed as a control character by get_char_pressed.
        if is_key_pressed(KeyCode::Tab) {
            self.try_complete();
            dirty = true;
        }

        // Printable characters
        while let Some(ch) = get_char_pressed() {
            if !ch.is_control() {
                self.buffer.insert(self.cursor_pos, ch);
                self.cursor_pos += ch.len_utf8();
                self.history_pos = None;
                self.reset_blink();
                dirty = true;
            }
        }

        // Backspace — single press
        if is_key_pressed(KeyCode::Backspace) {
            self.do_backspace();
            self.backspace_next = now + REPEAT_INITIAL;
            self.reset_blink();
            dirty = true;
        } else if is_key_down(KeyCode::Backspace) && now >= self.backspace_next {
            self.do_backspace();
            self.backspace_next = now + REPEAT_RATE;
            self.reset_blink();
            dirty = true;
        }
        if is_key_pressed(KeyCode::Delete) {
            self.do_delete();
            self.reset_blink();
            dirty = true;
        }

        // Cursor movement
        if is_key_pressed(KeyCode::Left) && self.cursor_pos > 0 {
            let ch = self.buffer[..self.cursor_pos].chars().last().unwrap();
            self.cursor_pos -= ch.len_utf8();
            self.reset_blink();
            dirty = true;
        }
        if is_key_pressed(KeyCode::Right) && self.cursor_pos < self.buffer.len() {
            let ch = self.buffer[self.cursor_pos..].chars().next().unwrap();
            self.cursor_pos += ch.len_utf8();
            self.reset_blink();
            dirty = true;
        }
        if is_key_pressed(KeyCode::Home) {
            self.cursor_pos = 0;
            self.reset_blink();
            dirty = true;
        }
        if is_key_pressed(KeyCode::End) {
            self.cursor_pos = self.buffer.len();
            self.reset_blink();
            dirty = true;
        }

        // History navigation
        if is_key_pressed(KeyCode::Up) {
            self.history_up();
            self.reset_blink();
            dirty = true;
        }
        if is_key_pressed(KeyCode::Down) {
            self.history_down();
            self.reset_blink();
            dirty = true;
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
            return (Some(submitted), true);
        }

        (None, dirty)
    }

    /// Complete the word at/before the cursor.
    /// Consecutive Tab presses cycle through all matches.
    fn try_complete(&mut self) {
        if !self.in_tab_cycle {
            // --- Start a new completion cycle ---
            let before = self.buffer[..self.cursor_pos].to_lowercase();
            let word_start = before.rfind(' ').map_or(0, |i| i + 1);
            let partial = &before[word_start..];
            let is_verb = !before.contains(' ');

            let candidates = if is_verb {
                &self.completion_verbs
            } else {
                &self.completion_nouns
            };

            let mut matches: Vec<String> = candidates
                .iter()
                .filter(|c| c.starts_with(partial))
                .cloned()
                .collect();

            if matches.is_empty() { return; }
            matches.sort();

            self.tab_word_start = word_start;
            self.tab_is_verb    = is_verb;
            self.tab_matches    = matches;
            self.tab_cycle_idx  = 0;
            self.in_tab_cycle   = true;
        } else {
            // --- Advance within existing cycle ---
            self.tab_cycle_idx = (self.tab_cycle_idx + 1) % self.tab_matches.len();
        }

        let completion = self.tab_matches[self.tab_cycle_idx].clone();

        // Preserve text that was after the cursor
        let after = self.buffer[self.cursor_pos..].to_string();

        // Replace the partial word with the completion
        self.buffer.truncate(self.tab_word_start);
        self.buffer.push_str(&completion);

        // Add a trailing space after verbs so the user can type the noun immediately
        if self.tab_is_verb {
            self.buffer.push(' ');
        }

        // cursor lands just after the completion (before any preserved tail)
        self.cursor_pos = self.buffer.len();
        self.buffer.push_str(&after);
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

    /// Reset cursor blink and break any active Tab cycle.
    fn reset_blink(&mut self) {
        self.cursor_visible = true;
        self.blink_timer = get_time();
        self.in_tab_cycle = false;
    }
}
