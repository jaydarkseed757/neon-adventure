use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};
use crate::player::Player;
use crate::world::World;
use crate::mobs::MobStore;

const SAVE_FILE: &str = "neon_descent.sav";

// ---------------------------------------------------------------------------
// Snapshot types — plain data, no game logic
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct PlayerSnapshot {
    pub current_room: String,
    pub inventory: Vec<String>,
    pub worn: HashSet<String>,
    pub visited: HashSet<String>,
    pub score: u32,
    pub turn: u32,
    pub scored_events: HashSet<String>,
    /// Absent in older save files — defaults to empty.
    #[serde(default)]
    pub dialogue_seen: HashSet<String>,
}

#[derive(Serialize, Deserialize)]
pub struct RoomSnapshot {
    pub items: Vec<String>,
    pub exits: HashMap<String, String>,
}

#[derive(Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub rooms: HashMap<String, RoomSnapshot>,
}

#[derive(Serialize, Deserialize)]
pub struct MobSnapshot {
    /// (mob_id, current_room) pairs
    pub mob_rooms: Vec<(String, String)>,
}

#[derive(Serialize, Deserialize)]
pub struct GameSnapshot {
    pub player: PlayerSnapshot,
    pub world: WorldSnapshot,
    pub mobs: MobSnapshot,
}

// ---------------------------------------------------------------------------
// Capture / restore helpers
// ---------------------------------------------------------------------------

pub fn take_snapshot(player: &Player, world: &World, mobs: &MobStore) -> GameSnapshot {
    GameSnapshot {
        player: PlayerSnapshot {
            current_room:  player.current_room.clone(),
            inventory:     player.inventory.clone(),
            worn:          player.worn.clone(),
            visited:       player.visited.clone(),
            score:         player.score,
            turn:          player.turn,
            scored_events: player.scored_events.clone(),
            dialogue_seen: player.dialogue_seen.clone(),
        },
        world: WorldSnapshot {
            rooms: world.rooms.iter().map(|(id, r)| {
                (id.clone(), RoomSnapshot {
                    items: r.items.clone(),
                    exits: r.exits.clone(),
                })
            }).collect(),
        },
        mobs: MobSnapshot {
            mob_rooms: mobs.mobs.iter()
                .map(|m| (m.id.clone(), m.current_room.clone()))
                .collect(),
        },
    }
}

pub fn restore_snapshot(snap: GameSnapshot, player: &mut Player, world: &mut World, mobs: &mut MobStore) {
    player.current_room  = snap.player.current_room;
    player.inventory     = snap.player.inventory;
    player.worn          = snap.player.worn;
    player.visited       = snap.player.visited;
    player.score         = snap.player.score;
    player.turn          = snap.player.turn;
    player.scored_events = snap.player.scored_events;
    player.dialogue_seen = snap.player.dialogue_seen;

    for (id, rs) in snap.world.rooms {
        if let Some(room) = world.rooms.get_mut(&id) {
            room.items = rs.items;
            room.exits = rs.exits;
        }
    }

    for (mob_id, room) in snap.mobs.mob_rooms {
        if let Some(mob) = mobs.mobs.iter_mut().find(|m| m.id == mob_id) {
            mob.current_room = room;
        }
    }
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

pub fn save_to_file(snap: &GameSnapshot) -> Result<(), String> {
    let json = serde_json::to_string_pretty(snap)
        .map_err(|e| format!("serialisation error: {}", e))?;
    std::fs::write(SAVE_FILE, json)
        .map_err(|e| format!("could not write save file: {}", e))?;
    Ok(())
}

pub fn load_from_file() -> Result<GameSnapshot, String> {
    let data = std::fs::read_to_string(SAVE_FILE)
        .map_err(|e| format!("could not read save file: {}", e))?;
    serde_json::from_str(&data)
        .map_err(|e| format!("save file is corrupt or from an older version: {}", e))
}
