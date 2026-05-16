use serde::Deserialize;
use crate::ui;

// ---------------------------------------------------------------------------
// Minimal xorshift64 RNG — same algorithm as ambient.rs, different seed
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| (d.as_nanos() as u64).wrapping_add(0xdead_c0de_1337_cafe))
            .unwrap_or(0xf00d_babe_dead_beef);
        Rng(if seed == 0 { 1 } else { seed })
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn range(&mut self, n: usize) -> usize {
        if n == 0 { return 0; }
        (self.next() as usize) % n
    }

    fn one_in(&mut self, n: u64) -> bool {
        if n == 0 { return false; }
        self.next() % n == 0
    }
}

// ---------------------------------------------------------------------------
// Mob data
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct Mob {
    #[allow(dead_code)]
    pub id: String,
    /// Display name used in messages, e.g. "the cat".
    pub name: String,
    /// Room the mob starts in and is currently occupying.
    pub current_room: String,
    /// Pool of rooms the mob is allowed to wander between.
    pub wander_rooms: Vec<String>,
    /// 1-in-N chance to move each turn.
    pub move_chance: u64,
    /// 1-in-N chance to emit an idle message when sharing a room with the player.
    pub idle_chance: u64,
    /// Short line shown in room description when the mob is present.
    pub presence: String,
    /// Text shown when the player examines the mob.
    pub description: String,
    /// Printed when the mob moves into the player's current room.
    pub enter_messages: Vec<String>,
    /// Printed when the mob moves out of the player's current room.
    pub leave_messages: Vec<String>,
    /// Occasionally printed when the mob is in the same room as the player.
    #[serde(default)]
    pub idle_messages: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MobFile {
    mobs: Vec<Mob>,
}

// ---------------------------------------------------------------------------
// MobStore
// ---------------------------------------------------------------------------

pub struct MobStore {
    pub mobs: Vec<Mob>,
    rng: Rng,
}

impl MobStore {
    pub fn load() -> Result<Self, String> {
        let raw = include_str!("../mobs.toml");
        let file: MobFile = toml::from_str(raw)
            .map_err(|e| format!("TOML parse error: {}", e))?;
        Ok(MobStore { mobs: file.mobs, rng: Rng::new() })
    }

    /// Returns all mobs currently in the given room.
    pub fn in_room(&self, room_id: &str) -> Vec<&Mob> {
        self.mobs.iter().filter(|m| m.current_room == room_id).collect()
    }

    /// Find a mob in the given room whose id or name fuzzy-matches `noun`.
    pub fn find_in_room<'a>(&'a self, noun: &str, room_id: &str) -> Option<&'a Mob> {
        let candidates: Vec<&Mob> = self.in_room(room_id);
        let lower = noun.to_lowercase();
        // Exact id or name match first
        candidates.iter().copied().find(|m| m.id == lower || m.name == lower)
            // Then substring match on id or name
            .or_else(|| candidates.iter().copied().find(|m| m.id.contains(&*lower) || m.name.contains(&*lower)))
    }

    /// Tick mob movement. Called once per player turn.
    ///
    /// For each mob:
    /// - Roll 1-in-`move_chance`. On success, pick a random room from `wander_rooms`
    ///   (preferring a different room) and move there.
    /// - Print an enter message if the mob arrives in the player's room.
    /// - Print a leave message if the mob departs the player's room.
    /// - If the mob stays and shares the player's room, occasionally print an idle line.
    pub fn tick(&mut self, player_room: &str) {
        let n = self.mobs.len();
        for i in 0..n {
            let was_with_player = self.mobs[i].current_room == player_room;

            if self.rng.one_in(self.mobs[i].move_chance) {
                // Candidates: any wander_room that isn't the mob's current room.
                let current = self.mobs[i].current_room.clone();
                let candidates: Vec<String> = self.mobs[i]
                    .wander_rooms
                    .iter()
                    .filter(|r| r.as_str() != current)
                    .cloned()
                    .collect();

                if !candidates.is_empty() {
                    let dest = {
                        let idx = self.rng.range(candidates.len());
                        candidates[idx].clone()
                    };
                    let now_with_player = dest == player_room;

                    if was_with_player && !now_with_player {
                        // Mob leaves the player's room
                        let n_msgs = self.mobs[i].leave_messages.len();
                        if n_msgs > 0 {
                            let msg_idx = self.rng.range(n_msgs);
                            let msg = self.mobs[i].leave_messages[msg_idx].clone();
                            ui::print_ambient(&msg);
                        }
                    } else if !was_with_player && now_with_player {
                        // Mob enters the player's room
                        let n_msgs = self.mobs[i].enter_messages.len();
                        if n_msgs > 0 {
                            let msg_idx = self.rng.range(n_msgs);
                            let msg = self.mobs[i].enter_messages[msg_idx].clone();
                            ui::print_ambient(&msg);
                        }
                    }

                    self.mobs[i].current_room = dest;
                }
            } else if was_with_player {
                // Mob stays — maybe print an idle line
                let n_msgs = self.mobs[i].idle_messages.len();
                let idle_chance = self.mobs[i].idle_chance;
                if n_msgs > 0 && self.rng.one_in(idle_chance) {
                    let msg_idx = self.rng.range(n_msgs);
                    let msg = self.mobs[i].idle_messages[msg_idx].clone();
                    ui::print_ambient(&msg);
                }
            }
        }
    }

    pub fn count(&self) -> usize {
        self.mobs.len()
    }
}
