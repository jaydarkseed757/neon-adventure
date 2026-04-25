use macroquad::color::Color;
use macroquad::shapes::draw_rectangle;
use macroquad::text::{draw_text_ex, measure_text, Font, TextParams};
use macroquad::window::{screen_height, screen_width};

use crate::ui::{Segment, StyledLine, Style, TextColor};
use crate::input::InputState;
use crate::player::Player;
use crate::world::World;
use crate::ambient;

// ---------------------------------------------------------------------------
// Layout constants
// ---------------------------------------------------------------------------

pub const STATUS_H:  f32 = 32.0;
pub const INPUT_H:   f32 = 52.0;
pub const MARGIN:    f32 = 16.0;
pub const LINE_H:    f32 = 22.0;
pub const FONT_SIZE: u16 = 16;

pub const BG:         Color = Color { r: 0.05, g: 0.05, b: 0.08, a: 1.0 };
pub const STATUS_BG:  Color = Color { r: 0.07, g: 0.07, b: 0.11, a: 1.0 };
pub const INPUT_BG:   Color = Color { r: 0.07, g: 0.07, b: 0.11, a: 1.0 };
pub const SEP_COLOR:  Color = Color { r: 0.15, g: 0.45, b: 0.45, a: 0.50 };
pub const CURSOR_COL: Color = Color { r: 0.15, g: 0.90, b: 0.90, a: 0.85 };
pub const PROMPT_COL: Color = Color { r: 0.15, g: 0.90, b: 0.90, a: 1.0  };
pub const SCROLL_COL: Color = Color { r: 0.25, g: 0.55, b: 0.55, a: 0.50 };

fn output_top() -> f32 { STATUS_H }
fn output_h()   -> f32 { screen_height() - STATUS_H - INPUT_H }
fn output_bot() -> f32 { screen_height() - INPUT_H }
fn text_width()  -> f32 { screen_width() - MARGIN * 2.0 - 8.0 }

// ---------------------------------------------------------------------------
// Font set — stored as owned Font handles (Clone, not Copy)
// ---------------------------------------------------------------------------

pub struct Fonts {
    pub regular: Font,
    pub bold:    Font,
    pub italic:  Font,
}

impl Fonts {
    pub fn select(&self, style: &Style) -> &Font {
        if style.bold    { &self.bold   }
        else if style.italic { &self.italic }
        else              { &self.regular }
    }
}

fn tp(font: &Font, size: u16, color: Color) -> TextParams<'_> {
    TextParams { font: Some(font), font_size: size, color, ..Default::default() }
}

fn mw(text: &str, font: &Font, size: u16) -> f32 {
    measure_text(text, Some(font), size, 1.0).width
}

// ---------------------------------------------------------------------------
// A fully pre-wrapped render line
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct RenderedLine {
    pub segments: Vec<Segment>,
    pub y_offset: f32,
}

// ---------------------------------------------------------------------------
// Scroll state
// ---------------------------------------------------------------------------

pub struct ScrollState {
    pub offset_px:   f32,
    pub auto_scroll: bool,
    content_height:  f32,
}

impl ScrollState {
    pub fn new() -> Self {
        ScrollState { offset_px: 0.0, auto_scroll: true, content_height: 0.0 }
    }

    pub fn max_scroll(&self) -> f32 {
        (self.content_height - output_h()).max(0.0)
    }

    pub fn on_new_content(&mut self, rendered: &[RenderedLine]) {
        self.content_height = rendered
            .last()
            .map_or(0.0, |l| l.y_offset + LINE_H);
        if self.auto_scroll {
            self.offset_px = self.max_scroll();
        }
    }

    pub fn scroll_by(&mut self, delta_px: f32) {
        self.offset_px = (self.offset_px + delta_px).clamp(0.0, self.max_scroll());
        self.auto_scroll = self.offset_px >= self.max_scroll() - 1.0;
    }
}

// ---------------------------------------------------------------------------
// Word-wrap
// ---------------------------------------------------------------------------

pub fn wrap_new_lines(
    styled: &[StyledLine],
    from_idx: usize,
    rendered: &mut Vec<RenderedLine>,
    fonts: &Fonts,
) {
    let start_y = rendered
        .last()
        .map_or(0.0, |l| l.y_offset + LINE_H);

    let mut y = start_y;
    let max_w = text_width();

    for line in &styled[from_idx..] {
        if line.segments.is_empty() {
            rendered.push(RenderedLine { segments: vec![], y_offset: y });
            y += LINE_H;
            continue;
        }

        // Flatten into word tokens with their style
        let mut words: Vec<(String, Style)> = Vec::new();
        for seg in &line.segments {
            for word in seg.text.split_inclusive(' ') {
                words.push((word.to_string(), seg.style));
            }
        }

        let mut row_segs: Vec<Segment> = Vec::new();
        let mut row_w = 0.0f32;

        for (word, style) in words {
            let font = fonts.select(&style);
            let ww = mw(&word, font, FONT_SIZE);
            if row_w + ww > max_w && !row_segs.is_empty() {
                rendered.push(RenderedLine { segments: row_segs.clone(), y_offset: y });
                y += LINE_H;
                row_segs.clear();
                row_w = 0.0;
            }
            if let Some(last) = row_segs.last_mut() {
                if last.style == style {
                    last.text.push_str(&word);
                    row_w += ww;
                    continue;
                }
            }
            row_segs.push(Segment { text: word, style });
            row_w += ww;
        }
        if !row_segs.is_empty() {
            rendered.push(RenderedLine { segments: row_segs, y_offset: y });
            y += LINE_H;
        }
    }
}

// ---------------------------------------------------------------------------
// Render panels
// ---------------------------------------------------------------------------

pub fn render_status_bar(player: &Player, world: &World, fonts: &Fonts) {
    let w = screen_width();
    draw_rectangle(0.0, 0.0, w, STATUS_H, STATUS_BG);
    draw_rectangle(0.0, STATUS_H - 1.0, w, 1.0, SEP_COLOR);

    let room_name = world.rooms.get(&player.current_room)
        .map(|r| r.name.as_str())
        .unwrap_or("???");
    let label = format!("  [ {} ]", room_name.to_uppercase());
    let time   = format!("  {}", ambient::time_label(player.turn));
    let score  = format!("Score: {}  ", player.score);
    let turn   = format!("Turn: {}  ", player.turn);

    let y = STATUS_H - 8.0;

    draw_text_ex(&label, MARGIN, y, tp(&fonts.bold, 14, TextColor::RoomHeader.to_mq_color()));

    let right = w - MARGIN;
    let tw  = mw(&time,  &fonts.regular, 14);
    let turw = mw(&turn, &fonts.regular, 14);
    let sw  = mw(&score, &fonts.regular, 14);

    draw_text_ex(&time,  right - tw,               y, tp(&fonts.regular, 14, TextColor::Ambient.to_mq_color()));
    draw_text_ex(&turn,  right - tw - turw,         y, tp(&fonts.regular, 14, TextColor::Default.to_mq_color()));
    draw_text_ex(&score, right - tw - turw - sw,    y, tp(&fonts.regular, 14, TextColor::Default.to_mq_color()));
}

pub fn render_output_area(rendered: &[RenderedLine], scroll: &ScrollState, fonts: &Fonts) {
    let w   = screen_width();
    let top = output_top();
    let bot = output_bot();
    let h   = output_h();

    draw_rectangle(0.0, top, w, h, BG);

    for line in rendered {
        let screen_y = top + line.y_offset - scroll.offset_px + FONT_SIZE as f32;
        if screen_y + LINE_H < top { continue; }
        if screen_y - FONT_SIZE as f32 > bot { break; }

        let mut x = MARGIN;
        for seg in &line.segments {
            let font = fonts.select(&seg.style);
            draw_text_ex(&seg.text, x, screen_y, tp(font, FONT_SIZE, seg.style.color.to_mq_color()));
            x += mw(&seg.text, font, FONT_SIZE);
        }
    }

    // Scroll thumb
    let max = scroll.max_scroll();
    if max > 0.0 {
        let thumb_h = ((h / scroll.content_height) * h).clamp(20.0, h);
        let thumb_y = top + (scroll.offset_px / max) * (h - thumb_h);
        draw_rectangle(w - 6.0, thumb_y, 6.0, thumb_h, SCROLL_COL);
    }

    draw_rectangle(0.0, bot, w, 1.0, SEP_COLOR);
}

pub fn render_input_bar(input: &InputState, fonts: &Fonts) {
    let w   = screen_width();
    let top = output_bot() + 1.0;
    let h   = INPUT_H;

    draw_rectangle(0.0, top, w, h, INPUT_BG);

    let y = top + h / 2.0 + FONT_SIZE as f32 / 2.0 - 2.0;

    let prompt = "> ";
    draw_text_ex(prompt, MARGIN, y, tp(&fonts.bold, FONT_SIZE, PROMPT_COL));
    let pw = mw(prompt, &fonts.bold, FONT_SIZE);

    let text = &input.buffer;
    draw_text_ex(text, MARGIN + pw, y, tp(&fonts.regular, FONT_SIZE, TextColor::Default.to_mq_color()));

    if input.cursor_visible {
        let before = &text[..input.cursor_pos];
        let cx = mw(before, &fonts.regular, FONT_SIZE);
        draw_rectangle(
            MARGIN + pw + cx,
            y - FONT_SIZE as f32 + 2.0,
            2.0,
            FONT_SIZE as f32 + 2.0,
            CURSOR_COL,
        );
    }
}
