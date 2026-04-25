use std::collections::HashMap;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ItemResponse {
    pub item: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TopicEntry {
    pub keys: Vec<String>,
    pub lines: Vec<String>,
    #[serde(default)]
    pub once: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Npc {
    pub room_id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub lines: Vec<String>,
    #[serde(default)]
    pub revisit_lines: Vec<String>,
    #[serde(default)]
    pub item_responses: Vec<ItemResponse>,
    #[serde(default)]
    pub topics: Vec<TopicEntry>,
    /// Printed when the player asks about a topic this NPC has no entry for.
    #[serde(default)]
    pub unknown_topic: Option<String>,
    /// Printed when the player asks again about a once=true topic they have already heard.
    #[serde(default)]
    pub repeat_topic: Option<String>,
}

impl Npc {
    /// Return the first TopicEntry whose keys match the player's topic input.
    ///
    /// Matching rules (first entry that passes any rule wins):
    ///   1. Any query word (length ≥ 3) is a substring of a normalized key.
    ///   2. Any word in the normalized key starts with a query word (prefix match).
    pub fn find_topic(&self, topic_input: &str) -> Option<&TopicEntry> {
        let q = topic_input.replace('_', " ").to_lowercase();
        let q_words: Vec<&str> = q.split_whitespace()
            .filter(|w| w.len() >= 3)
            .collect();

        if q_words.is_empty() {
            return None;
        }

        for entry in &self.topics {
            for key in &entry.keys {
                let k = key.replace('_', " ").to_lowercase();
                let k_words: Vec<&str> = k.split_whitespace().collect();

                for qw in &q_words {
                    // Rule 1: key contains the query word as a substring.
                    if k.contains(qw) {
                        return Some(entry);
                    }
                    // Rule 2: any word in the key starts with the query word.
                    if k_words.iter().any(|kw| kw.starts_with(qw)) {
                        return Some(entry);
                    }
                }
            }
        }
        None
    }

    /// Return true if this NPC's name or aliases match the given query string.
    pub fn name_matches(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        let name_lower = self.name.to_lowercase();

        // Exact match.
        if name_lower == q { return true; }
        // Name contains query as substring.
        if name_lower.contains(&q) { return true; }
        // Any word in the name starts with the query (min 3 chars).
        if q.len() >= 3 && name_lower.split_whitespace().any(|w| w.starts_with(&q)) {
            return true;
        }
        // Alias check.
        self.aliases.iter().any(|a| {
            let al = a.to_lowercase();
            al == q || al.contains(&q) || q.contains(&al)
        })
    }
}

#[derive(Debug, Deserialize)]
struct NpcFile {
    npcs: Vec<Npc>,
}

pub struct NpcStore {
    by_room: HashMap<String, Npc>,
}

impl NpcStore {
    pub fn load() -> Result<Self, String> {
        let raw = include_str!("../npcs.toml");
        let file: NpcFile = toml::from_str(raw)
            .map_err(|e| format!("TOML parse error: {}", e))?;
        let by_room = file.npcs.into_iter().map(|n| (n.room_id.clone(), n)).collect();
        Ok(NpcStore { by_room })
    }

    pub fn get(&self, room_id: &str) -> Option<&Npc> {
        self.by_room.get(room_id)
    }

    /// Find any NPC (across all rooms) whose name or aliases match the query.
    pub fn find_by_name(&self, query: &str) -> Option<&Npc> {
        self.by_room.values().find(|npc| npc.name_matches(query))
    }

    pub fn count(&self) -> usize {
        self.by_room.len()
    }
}
