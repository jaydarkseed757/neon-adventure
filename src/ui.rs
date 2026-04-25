use std::cell::RefCell;
use macroquad::color::Color;

// ---------------------------------------------------------------------------
// Styled text types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct StyledLine {
    pub segments: Vec<Segment>,
}

impl StyledLine {
    pub fn blank() -> Self {
        StyledLine { segments: vec![] }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        StyledLine {
            segments: vec![Segment { text: text.into(), style: Style::default() }],
        }
    }

    pub fn single(text: impl Into<String>, style: Style) -> Self {
        StyledLine {
            segments: vec![Segment { text: text.into(), style }],
        }
    }

    pub fn echo(text: impl Into<String>) -> Self {
        StyledLine::single(format!("> {}", text.into()), Style::echo())
    }
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub text: String,
    pub style: Style,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub color: TextColor,
    pub bold: bool,
    pub italic: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style { color: TextColor::Default, bold: false, italic: false }
    }
}

impl Style {
    pub fn echo() -> Self {
        Style { color: TextColor::Echo, bold: false, italic: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextColor {
    Default,
    Echo,       // echoed player input: dim cyan
    RoomHeader, // bold bright_yellow
    Items,      // green
    Exits,      // cyan
    Error,      // red
    Ambient,    // bright_black italic
    NpcName,    // magenta bold
    NpcDialogue,// magenta
    ScoreNotice,// bright_yellow
    WinMessage, // bright_green
    Dim,        // bright_black (misc dim text)
}

impl TextColor {
    pub fn to_mq_color(self) -> Color {
        match self {
            TextColor::Default    => Color::new(0.82, 0.85, 0.88, 1.0),
            TextColor::Echo       => Color::new(0.25, 0.70, 0.70, 1.0),
            TextColor::RoomHeader => Color::new(1.00, 0.90, 0.15, 1.0),
            TextColor::Items      => Color::new(0.20, 0.85, 0.35, 1.0),
            TextColor::Exits      => Color::new(0.15, 0.90, 0.90, 1.0),
            TextColor::Error      => Color::new(0.95, 0.25, 0.25, 1.0),
            TextColor::Ambient    => Color::new(0.45, 0.48, 0.52, 1.0),
            TextColor::NpcName    => Color::new(0.90, 0.25, 0.85, 1.0),
            TextColor::NpcDialogue=> Color::new(0.75, 0.20, 0.75, 1.0),
            TextColor::ScoreNotice=> Color::new(1.00, 0.90, 0.15, 1.0),
            TextColor::WinMessage => Color::new(0.15, 1.00, 0.40, 1.0),
            TextColor::Dim        => Color::new(0.42, 0.44, 0.48, 1.0),
        }
    }
}

// ---------------------------------------------------------------------------
// Thread-local output buffer
// ---------------------------------------------------------------------------

thread_local! {
    static OUTPUT: RefCell<Vec<StyledLine>> = RefCell::new(Vec::new());
}

fn push(line: StyledLine) {
    OUTPUT.with(|buf| buf.borrow_mut().push(line));
}

/// Drain all buffered lines since last call. Called by the app loop after
/// each command to move output into the persistent scroll buffer.
pub fn drain() -> Vec<StyledLine> {
    OUTPUT.with(|buf| buf.borrow_mut().drain(..).collect())
}

// ---------------------------------------------------------------------------
// Print helpers — replace all println!() call sites in the game
// ---------------------------------------------------------------------------

pub fn print_blank() {
    push(StyledLine::blank());
}

pub fn print_plain(text: &str) {
    push(StyledLine::plain(text));
}

pub fn print_room_header(text: &str) {
    push(StyledLine::single(text, Style { color: TextColor::RoomHeader, bold: true, italic: false }));
}

pub fn print_items(text: &str) {
    push(StyledLine::single(text, Style { color: TextColor::Items, bold: false, italic: false }));
}

pub fn print_exits(text: &str) {
    push(StyledLine::single(text, Style { color: TextColor::Exits, bold: false, italic: false }));
}

pub fn print_error(text: &str) {
    push(StyledLine::single(text, Style { color: TextColor::Error, bold: false, italic: false }));
}

pub fn print_ambient(text: &str) {
    push(StyledLine::single(text, Style { color: TextColor::Ambient, bold: false, italic: true }));
}

pub fn print_npc_name(text: &str) {
    push(StyledLine::single(text, Style { color: TextColor::NpcName, bold: true, italic: false }));
}

pub fn print_npc_dialogue(text: &str) {
    push(StyledLine::single(text, Style { color: TextColor::NpcDialogue, bold: false, italic: false }));
}

pub fn print_score_notice(text: &str) {
    push(StyledLine::single(text, Style { color: TextColor::ScoreNotice, bold: false, italic: false }));
}

pub fn print_win(text: &str) {
    push(StyledLine::single(text, Style { color: TextColor::WinMessage, bold: true, italic: false }));
}

pub fn print_dim(text: &str) {
    push(StyledLine::single(text, Style { color: TextColor::Dim, bold: false, italic: false }));
}

/// Score screen line: "[x]" or "[ ]" followed by description and optional points.
pub fn print_score_line(done: bool, description: &str, points: &str) {
    let check_color = if done { TextColor::ScoreNotice } else { TextColor::Dim };
    let check = if done { "[x] " } else { "[ ] " };
    push(StyledLine {
        segments: vec![
            Segment { text: check.to_string(),       style: Style { color: check_color, bold: false, italic: false } },
            Segment { text: description.to_string(), style: Style { color: TextColor::Default, bold: false, italic: false } },
            Segment { text: points.to_string(),      style: Style { color: TextColor::Dim, bold: false, italic: false } },
        ],
    });
}
