use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq)]
pub enum VerboseMode {
    /// Always show full room description (VERBOSE).
    Verbose,
    /// Full description on first visit, name only on revisits (BRIEF — default).
    Brief,
    /// Room name only, always (SUPERBRIEF).
    SuperBrief,
}

pub struct Player {
    pub current_room: String,
    pub inventory: Vec<String>,
    pub worn: HashSet<String>,
    pub visited: HashSet<String>,
    pub score: u32,
    pub turn: u32,
    pub scored_events: HashSet<String>,
    /// Tracks which once=true NPC topics have already been delivered.
    pub dialogue_seen: HashSet<String>,
    /// Controls how much is shown when entering a room.
    pub verbose_mode: VerboseMode,
}

impl Player {
    pub fn new(starting_room: &str) -> Self {
        let mut visited = HashSet::new();
        visited.insert(starting_room.to_string());

        Player {
            current_room: starting_room.to_string(),
            inventory: vec!["cyberdeck".to_string()],
            worn: HashSet::new(),
            visited,
            score: 0,
            turn: 0,
            scored_events: HashSet::new(),
            dialogue_seen: HashSet::new(),
            verbose_mode: VerboseMode::Brief,
        }
    }

    pub fn move_to(&mut self, room_id: &str) {
        self.current_room = room_id.to_string();
        self.visited.insert(room_id.to_string());
    }

    pub fn has_item(&self, item: &str) -> bool {
        self.inventory.iter().any(|i| i == item)
    }

    pub fn take_item(&mut self, item: String) {
        self.inventory.push(item);
    }

    pub fn drop_item(&mut self, item: &str) -> Option<String> {
        if let Some(pos) = self.inventory.iter().position(|i| i == item) {
            Some(self.inventory.remove(pos))
        } else {
            None
        }
    }

    /// Mark a one-time event. Returns true the first time, false on repeats.
    pub fn first_time(&mut self, event: &str) -> bool {
        self.scored_events.insert(event.to_string())
    }

    /// Award points for a one-time event. Returns points awarded (0 if already scored).
    pub fn award(&mut self, event: &str, points: u32) -> u32 {
        if self.first_time(event) {
            self.score += points;
            points
        } else {
            0
        }
    }
}
