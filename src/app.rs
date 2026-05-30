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
use crate::net::{self, NetStore};
use crate::parser;
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
    pub net:       NetStore,

    // Persistent scroll buffer (all game output ever emitted this session)
    pub scroll_buf: Vec<crate::ui::StyledLine>,
    // Pre-wrapped render cache — rebuilt incrementally as scroll_buf grows
    pub rendered:     Vec<RenderedLine>,
    pub prev_buf_len: usize,
    pub last_width:   f32, // detect window resize to force full re-wrap

    // UNDO / AGAIN state
    pub undo_snapshot: Option<save::GameSnapshot>,
    pub last_input:    Option<String>,
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
        let net = NetStore::load().unwrap_or_else(|e| {
            eprintln!("Failed to load nodes.toml: {}", e);
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
            net,
            scroll_buf:   Vec::new(),
            rendered:     Vec::new(),
            prev_buf_len: 0,
            undo_snapshot: None,
            last_input:    None,
            last_width:    0.0,
        }
    }

    pub async fn frame(&mut self) {
        match self.state {
            AppState::Title => {
                if self.title.update_and_render(&self.fonts.regular, &self.fonts.bold) {
                    self.state = AppState::Playing;
                    // Opening briefing, then the initial room look
                    commands::print_intro();
                    commands::look(&self.player, &self.world, &self.mob_store);
                    self.drain_to_buf();
                }
            }

            AppState::Playing => {
                self.update_completions();
                let (submitted, _) = self.input.handle_frame();

                if let Some(ref cmd) = submitted {
                    let action = self.process_command(cmd);
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
                self.input.handle_frame();
                self.render_frame();
            }
        }
    }

    fn render_frame(&mut self) {
        let (_dx, dy) = mouse_wheel();
        if dy != 0.0 {
            self.scroll.scroll_by(-dy * gfx::LINE_H * 3.0);
        }
        self.rebuild_render_cache();
        let net_label = self.player.net_node.as_ref()
            .and_then(|id| self.net.get(id))
            .map(|n| n.name.as_str());
        clear_background(gfx::BG);
        gfx::render_status_bar(&self.player, &self.world, &self.fonts, net_label);
        gfx::render_output_area(&self.rendered, &self.scroll, &self.fonts);
        gfx::render_input_bar(&self.input, &self.fonts, net_label.is_some());
    }

    /// Rebuild the verb + noun lists for tab completion and push them to InputState.
    fn update_completions(&mut self) {
        // While jacked in, complete net verbs and the current node's route labels.
        if let Some(node_id) = self.player.net_node.clone() {
            let verbs: Vec<String> = ["look", "go", "read", "bypass", "return", "jack",
                "disconnect", "help", "objectives", "save", "restore", "quit"]
                .into_iter().map(String::from).collect();
            let mut nouns: Vec<String> = vec!["out".to_string()];
            if let Some(node) = self.net.get(&node_id) {
                nouns.extend(node.links.keys().cloned());
            }
            nouns.sort();
            nouns.dedup();
            self.input.set_completions(verbs, nouns);
            return;
        }

        // Static verb list — canonical user-facing commands, alphabetically sorted
        let verbs: Vec<String> = vec![
            "again", "ask", "brief", "drop", "examine", "help", "inventory", "jack",
            "knock", "listen", "look", "north", "south", "east", "west", "up", "down",
            "objectives",
            "press", "pull", "push", "quit", "read", "remove", "restart", "restore",
            "save", "score", "search", "smell", "superbrief", "take", "tell", "touch",
            "transcript", "turn", "undo", "unlock", "verbose", "wait", "wear",
        ].into_iter().map(String::from).collect();

        // Dynamic noun list — items in room, inventory, exits, NPC in room
        let mut nouns: Vec<String> = Vec::new();

        if let Some(room) = self.world.rooms.get(&self.player.current_room) {
            for item in &room.items {
                nouns.push(item.clone());
            }
            for dir in room.exits.keys() {
                nouns.push(dir.clone());
            }
        }
        for item in &self.player.inventory {
            nouns.push(item.clone());
        }
        if let Some(npc) = self.npc_store.get(&self.player.current_room) {
            let name = npc.name.to_lowercase().replace(' ', "_");
            nouns.push(name);
            for alias in &npc.aliases {
                nouns.push(alias.to_lowercase());
            }
        }
        nouns.sort();
        nouns.dedup();

        self.input.set_completions(verbs, nouns);
    }

    fn process_command(&mut self, input: &str) -> Action {
        // Echo the player's command into the output
        self.scroll_buf.push(ui::StyledLine::echo(input));

        // TRANSCRIPT — dump the full session log to a plain-text file
        if input == "transcript" {
            let path = "neon_descent_transcript.txt";
            let text: String = self.scroll_buf
                .iter()
                .map(|line| line.segments.iter().map(|s| s.text.as_str()).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
            match std::fs::write(path, text) {
                Ok(()) => ui::print_dim(&format!("Transcript saved to {}.", path)),
                Err(e) => ui::print_error(&format!("Transcript failed: {}", e)),
            }
            self.drain_to_buf();
            return Action::Continue;
        }

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

        let parsed = parser::parse(&effective);
        let action = if parsed.verb == "jack" || parsed.verb == "disconnect" {
            // JACK / DISCONNECT transitions need NetStore access — handle here.
            let want_out = parsed.verb == "disconnect" || parsed.noun.as_deref() == Some("out");
            self.handle_jack(want_out)
        } else if self.player.net_node.is_some() && !is_net_meta(&parsed.verb) {
            // Jacked in: route to the net handler (except global meta verbs).
            net::handle(&effective, &mut self.player, &mut self.world, &self.net)
        } else {
            commands::handle(
                &effective,
                &mut self.player,
                &mut self.world,
                &self.npc_store,
                &mut self.mob_store,
                &mut self.ambient,
            )
        };

        if !is_again {
            self.last_input = Some(effective);
        }

        self.drain_to_buf();
        action
    }

    /// Enter or leave the net. Returns the resulting Action (always Continue).
    fn handle_jack(&mut self, want_out: bool) -> Action {
        if want_out {
            if self.player.net_node.is_none() {
                ui::print_plain("You aren't jacked in.");
            } else {
                self.player.net_node = None;
                ui::print_dim("[ LINK SEVERED ]");
                ui::print_plain("You withdraw from the net. The physical room resolves around you.");
                commands::look(&self.player, &self.world, &self.mob_store);
            }
            return Action::Continue;
        }

        if self.player.net_node.is_some() {
            ui::print_plain("You are already jacked in.");
            return Action::Continue;
        }
        if !self.player.has_item("cyberdeck") {
            ui::print_plain("You have no cyberdeck to jack in with.");
            return Action::Continue;
        }
        if net::NO_SIGNAL_ROOMS.contains(&self.player.current_room.as_str()) {
            ui::print_plain("The signal flattens out here, too far from the arcology's spine. There is no net to reach.");
            return Action::Continue;
        }

        self.player.net_node = Some(net::ENTRY_NODE.to_string());
        net::jack_in_flavor();
        net::look(&self.net, net::ENTRY_NODE, &self.player);
        Action::Continue
    }

    fn drain_to_buf(&mut self) {
        self.scroll_buf.extend(ui::drain());
    }

    fn rebuild_render_cache(&mut self) {
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
        }
    }
}

/// Verbs that keep their normal (non-net) behaviour even while jacked in.
fn is_net_meta(verb: &str) -> bool {
    matches!(verb,
        "help" | "save" | "restore" | "quit" | "restart" |
        "objectives" | "score" | "version" | "runtime")
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
