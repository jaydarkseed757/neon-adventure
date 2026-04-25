use std::collections::HashMap;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub description: String,
    pub exits: HashMap<String, String>,
    pub items: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RoomFile {
    rooms: Vec<Room>,
}

pub struct World {
    pub rooms: HashMap<String, Room>,
}

impl World {
    pub fn load() -> Result<Self, String> {
        let raw = include_str!("../rooms.toml");

        let file: RoomFile = toml::from_str(&raw)
            .map_err(|e| format!("TOML parse error: {}", e))?;

        let rooms: HashMap<String, Room> = file.rooms
            .into_iter()
            .map(|r| (r.id.clone(), r))
            .collect();

        Ok(World { rooms })
    }

    pub fn get_room(&self, id: &str) -> Option<&Room> {
        self.rooms.get(id)
    }

    pub fn get_room_mut(&mut self, id: &str) -> Option<&mut Room> {
        self.rooms.get_mut(id)
    }
}
