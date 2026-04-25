use macroquad::color::Color;
use macroquad::input::{is_key_pressed, KeyCode};
use macroquad::shapes::draw_rectangle;
use macroquad::text::Font;
use macroquad::texture::{draw_texture_ex, DrawTextureParams, Texture2D};
use macroquad::time::get_time;
use macroquad::window::{screen_height, screen_width};

pub struct TitleScreen {
    texture:        Option<Texture2D>,
    scanline_offset: f32,
    enter_time:     f64,
}

impl TitleScreen {
    pub fn new() -> Self {
        TitleScreen {
            texture:         None,
            scanline_offset: 0.0,
            enter_time:      0.0,
        }
    }

    /// Decode the embedded title image and upload it to the GPU.
    pub async fn load(&mut self) {
        let png_bytes = include_bytes!("../assets/title.png");
        let img = image::load_from_memory(png_bytes)
            .expect("failed to decode embedded title.png")
            .to_rgba8();
        let tex = Texture2D::from_rgba8(img.width() as u16, img.height() as u16, &img);
        tex.set_filter(macroquad::texture::FilterMode::Nearest);
        self.texture = Some(tex);
    }

    /// Returns true once the user has pressed Enter/Space and the brief pause is done.
    pub fn update_and_render(&mut self, _font_reg: &Font, _font_bold: &Font) -> bool {
        let now = get_time();
        let w   = screen_width();
        let h   = screen_height();

        // --- Draw the title image scaled to fill the window ---
        if let Some(ref tex) = self.texture {
            let img_w = tex.width();
            let img_h = tex.height();

            // Fit the image to the window preserving aspect ratio, centred
            let scale = (w / img_w).min(h / img_h);
            let dst_w = img_w * scale;
            let dst_h = img_h * scale;
            let dst_x = (w - dst_w) / 2.0;
            let dst_y = (h - dst_h) / 2.0;

            // Letterbox bars (pure black) in case image doesn't fill the window
            draw_rectangle(0.0, 0.0, w, dst_y, macroquad::color::BLACK);
            draw_rectangle(0.0, dst_y + dst_h, w, h - (dst_y + dst_h), macroquad::color::BLACK);
            draw_rectangle(0.0, 0.0, dst_x, h, macroquad::color::BLACK);
            draw_rectangle(dst_x + dst_w, 0.0, w - (dst_x + dst_w), h, macroquad::color::BLACK);

            draw_texture_ex(tex, dst_x, dst_y, Color::new(1.0, 1.0, 1.0, 1.0),
                DrawTextureParams {
                    dest_size: Some(macroquad::math::Vec2::new(dst_w, dst_h)),
                    ..Default::default()
                });
        } else {
            // Fallback while texture loads
            draw_rectangle(0.0, 0.0, w, h, macroquad::color::BLACK);
        }

        // --- Scanlines over the top ---
        self.scanline_offset = (self.scanline_offset + 0.5) % 4.0;
        let mut sy = self.scanline_offset;
        while sy < h {
            draw_rectangle(0.0, sy, w, 1.0, Color::new(0.0, 0.0, 0.0, 0.18));
            sy += 4.0;
        }

        // --- Detect Enter or Space ---
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
            if self.enter_time == 0.0 {
                self.enter_time = now;
            }
        }

        self.enter_time > 0.0 && now - self.enter_time > 0.15
    }
}
