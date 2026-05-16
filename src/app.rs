use macroquad::input::mouse_wheel;
use macroquad::prelude::clear_background;
use macroquad::text::load_ttf_font_from_bytes;
use macroquad::window::screen_width;

use crate::world::World;
use crate::player::Player;
use crate::npcs::NpcStore;
use crate::mobs::MobStore;
use crate::ambient::Ambient;
use crate::save;
use crate::commands::{self, Action};
use crate::title::TitleScreen;
use crate::input::InputState;
use crate::gfx::{self, Fonts, RenderedLine, ScrollState};
use crate::ui;

pub enum AppState {
    Title,
    Playing,
    Won,
}

pub struct App {
    pub state:   AppState,
    pub title:   TitleScreen,
    pub fonts:   Fonts,
    pub input:   InputState,
    pub scroll:  ScrollState,

    // Game state
    pub world:     World,
    pub player:    Player,
    pub npc_store: NpcStore,
    pub mob_store: MobStore,
    pub ambient:   Ambient,

    // Persistent scroll buffer (all game output ever emitted this session)
    pub scroll_buf: Vec<crate::ui::StyledLine>,
    // Pre-wrapped render cache — rebuilt incrementally as scroll_buf grows
    pub rendered:     Vec<RenderedLine>,
    pub prev_buf_len: usize,
    pub last_width:   f32, // detect window resize to force full re-wrap

    // UNDO / AGAIN state
    pub undo_snapshot: Option<save::GameSnapshot>,
    pub last_input:    Option<String>,

    pub needs_redraw: bool,
}

impl App {
    pub async fn new() -> Self {
        // Load fonts (embedded in binary via include_bytes!)
        let reg_bytes  = include_bytes!("../assets/JetBrainsMono-Regular.ttf");
        let bold_bytes = include_bytes!("../assets/JetBrainsMono-Bold.ttf");
        let ital_bytes = include_bytes!("../assets/JetBrainsMono-Italic.ttf");

        let regular = load_ttf_font_from_bytes(reg_bytes).expect("failed to load regular font");
        let bold    = load_ttf_font_from_bytes(bold_bytes).expect("failed to load bold font");
        let italic  = load_ttf_font_from_bytes(ital_bytes).expect("failed to load italic font");

        let fonts = Fonts { regular, bold, italic };

        let (world, player, mob_store, ambient) = new_game();
        let npc_store = NpcStore::load().unwrap_or_else(|e| {
            eprintln!("Failed to load npcs.toml: {}", e);
            std::process::exit(1);
        });

        let mut title = TitleScreen::new();
        title.load().await;

        App {
            state:   AppState::Title,
            title,
            fonts,
            input:   InputState::new(),
            scroll:  ScrollState::new(),
            world,
            player,
            npc_store,
            mob_store,
            ambient,
            scroll_buf:   Vec::new(),
            rendered:     Vec::new(),
            prev_buf_len: 0,
            undo_snapshot: None,
            last_input:    None,
            last_width:    0.0,
            needs_redraw:  true,
        }
    }

    pub async fn frame(&mut self) {
        match self.state {
            AppState::Title => {
                if self.title.update_and_render(&self.fonts.regular, &self.fonts.bold) {
                    self.state = AppState::Playing;
                    // Emit the help hint and initial room look
                    ui::print_dim("Type HELP for a list of commands.");
                    ui::print_blank();
                    commands::look(&self.player, &self.world, &self.mob_store);
                    self.drain_to_buf();
                    self.needs_redraw = true;
                }
            }

            AppState::Playing => {
                let (submitted, input_dirty) = self.input.handle_frame();
                self.needs_redraw |= input_dirty;

                if let Some(ref cmd) = submitted {
                    let action = self.process_command(cmd);
                    self.needs_redraw = true;
                    match action {
                        Action::Quit => {
                            self.state = AppState::Won;
                        }
                        Action::Exit => {
                            std::process::exit(0);
                        }
                        Action::Restart => {
                            ui::print_blank();
                            ui::print_dim("--- Restarting Axiom Arcology... ---");
                            ui::print_blank();
                            let (world, player, mob_store, ambient) = new_game();
                            self.world     = world;
                            self.player    = player;
                            self.mob_store = mob_store;
                            self.ambient   = ambient;
                            self.undo_snapshot = None;
                            self.last_input    = None;
                            commands::look(&self.player, &self.world, &self.mob_store);
                            self.drain_to_buf();
                        }
                        Action::Continue => {}
                    }
                }

                self.render_frame();
            }

            AppState::Won => {
                let (_submitted, input_dirty) = self.input.handle_frame();
                self.needs_redraw |= input_dirty;
                self.render_frame();
            }
        }
    }

    fn render_frame(&mut self) {
        let (_dx, dy) = mouse_wheel();
        if dy != 0.0 {
            self.scroll.scroll_by(-dy * gfx::LINE_H * 3.0);
            self.needs_redraw = true;
        }
        self.needs_redraw |= self.rebuild_render_cache();

        if self.needs_redraw {
            clear_background(gfx::BG);
            gfx::render_status_bar(&self.player, &self.world, &self.fonts);
            gfx::render_output_area(&self.rendered, &self.scroll, &self.fonts);
            gfx::render_input_bar(&self.input, &self.fonts);
            self.needs_redraw = false;
        }
    }

    fn process_command(&mut self, input: &str) -> Action {
        // Echo the player's command into the output
        self.scroll_buf.push(ui::StyledLine::echo(input));

        // UNDO — handled here, not in commands::handle
        if input == "undo" {
            match self.undo_snapshot.take() {
                None => ui::print_plain("Nothing to undo."),
                Some(snap) => {
                    save::restore_snapshot(snap, &mut self.player, &mut self.world, &mut self.mob_store);
                    ui::print_plain("[Undone.]");
                    commands::look(&self.player, &self.world, &self.mob_store);
                }
            }
            self.drain_to_buf();
            return Action::Continue;
        }

        // AGAIN — replay last command
        let is_again = input == "again" || input == "g";
        let effective = if is_again {
            match self.last_input.clone() {
                None => {
                    ui::print_plain("Nothing to repeat.");
                    self.drain_to_buf();
                    return Action::Continue;
                }
                Some(prev) => prev,
            }
        } else {
            input.to_string()
        };

        // Take undo snapshot before mutating state
        self.undo_snapshot = Some(save::take_snapshot(&self.player, &self.world, &self.mob_store));

        let action = commands::handle(
            &effective,
            &mut self.player,
            &mut self.world,
            &self.npc_store,
            &mut self.mob_store,
            &mut self.ambient,
        );

        if !is_again {
            self.last_input = Some(effective);
        }

        self.drain_to_buf();
        action
    }

    fn drain_to_buf(&mut self) {
        self.scroll_buf.extend(ui::drain());
    }

    /// Returns true if the render cache changed (new content or window resize).
    fn rebuild_render_cache(&mut self) -> bool {
        let w = screen_width();
        let resized = (w - self.last_width).abs() > 1.0;
        if resized {
            self.rendered.clear();
            self.prev_buf_len = 0;
            self.last_width = w;
        }
        if self.scroll_buf.len() > self.prev_buf_len {
            gfx::wrap_new_lines(
                &self.scroll_buf,
                self.prev_buf_len,
                &mut self.rendered,
                &self.fonts,
            );
            self.prev_buf_len = self.scroll_buf.len();
            self.scroll.on_new_content(&self.rendered);
            return true;
        }
        resized
    }
}

pub fn new_game() -> (World, Player, MobStore, Ambient) {
    let world = World::load().unwrap_or_else(|e| {
        eprintln!("Failed to load rooms.toml: {}", e);
        std::process::exit(1);
    });
    let mob_store = MobStore::load().unwrap_or_else(|e| {
        eprintln!("Failed to load mobs.toml: {}", e);
        std::process::exit(1);
    });
    let player  = Player::new("foyer");
    let ambient = Ambient::new();
    (world, player, mob_store, ambient)
}
