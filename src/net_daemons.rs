use serde::Deserialize;
use crate::ui;

// ---------------------------------------------------------------------------
// Minimal xorshift64 RNG — same algorithm as mobs.rs / ambient.rs, own seed
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| (d.as_nanos() as u64).wrapping_add(0x1ce_d00d_cafe_f00d))
            .unwrap_or(0xbeef_cafe_1234_5678);
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
// Daemon data
// ---------------------------------------------------------------------------

/// The dormant-node sentinel value for daemons that aren't currently placed.
const DORMANT: &str = "";
/// The daemon that activates when trace crosses the critical threshold.
const HUNTER_ID: &str = "hunter_killer";

#[derive(Debug, Deserialize, Clone)]
pub struct NetDaemon {
    pub id: String,
    /// Display name used in messages, e.g. "a watchdog process".
    pub name: String,
    /// Node the daemon currently occupies ("" = dormant / not placed).
    pub current_node: String,
    /// Pool of nodes the daemon may wander between.
    pub wander_nodes: Vec<String>,
    /// 1-in-N chance to move each net turn.
    pub move_chance: u64,
    /// Whether it pressures the player (spikes trace) while co-located.
    #[serde(default)]
    pub hostile: bool,
    /// Extra trace per turn while it shares the player's node.
    #[serde(default)]
    pub trace_spike: u8,
    #[serde(default)]
    pub enter_messages: Vec<String>,
    #[serde(default)]
    pub leave_messages: Vec<String>,
    #[serde(default)]
    pub idle_messages: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DaemonFile {
    daemons: Vec<NetDaemon>,
}

// ---------------------------------------------------------------------------
// DaemonStore
// ---------------------------------------------------------------------------

pub struct DaemonStore {
    pub daemons: Vec<NetDaemon>,
    rng: Rng,
}

impl DaemonStore {
    pub fn load() -> Result<Self, String> {
        let raw = include_str!("../daemons.toml");
        let file: DaemonFile = toml::from_str(raw)
            .map_err(|e| format!("daemons.toml parse error: {}", e))?;
        Ok(DaemonStore { daemons: file.daemons, rng: Rng::new() })
    }

    /// Tick daemon movement once per net turn. Mirrors `MobStore::tick`, keyed
    /// on net nodes. Dormant daemons (current_node == "") do not move here.
    pub fn tick(&mut self, player_node: &str) {
        let n = self.daemons.len();
        for i in 0..n {
            let current = self.daemons[i].current_node.clone();
            // Dormant daemons don't move; the hunter-killer is pursuit-driven only.
            if current == DORMANT || self.daemons[i].id == HUNTER_ID {
                continue;
            }
            let was_with_player = current == player_node;

            if self.rng.one_in(self.daemons[i].move_chance) {
                let candidates: Vec<String> = self.daemons[i]
                    .wander_nodes
                    .iter()
                    .filter(|nd| nd.as_str() != current)
                    .cloned()
                    .collect();

                if !candidates.is_empty() {
                    let dest = {
                        let idx = self.rng.range(candidates.len());
                        candidates[idx].clone()
                    };
                    let now_with_player = dest == player_node;

                    if was_with_player && !now_with_player {
                        self.emit(i, MsgKind::Leave);
                    } else if !was_with_player && now_with_player {
                        self.emit(i, MsgKind::Enter);
                    }
                    self.daemons[i].current_node = dest;
                }
            } else if was_with_player {
                // Stays with the player — occasional idle pressure line.
                if self.rng.one_in(2) {
                    self.emit(i, MsgKind::Idle);
                }
            }
        }
    }

    fn emit(&mut self, i: usize, kind: MsgKind) {
        let pool = match kind {
            MsgKind::Enter => &self.daemons[i].enter_messages,
            MsgKind::Leave => &self.daemons[i].leave_messages,
            MsgKind::Idle  => &self.daemons[i].idle_messages,
        };
        if pool.is_empty() { return; }
        let idx = self.rng.range(pool.len());
        let msg = pool[idx].clone();
        ui::print_ambient(&msg);
    }

    /// Total trace spike from all hostile daemons sharing the given node.
    pub fn total_spike_at(&self, node: &str) -> u8 {
        self.daemons.iter()
            .filter(|d| d.hostile && d.current_node == node)
            .map(|d| d.trace_spike)
            .fold(0u8, |a, d| a.saturating_add(d))
    }

    /// True if any hostile daemon currently shares the node.
    pub fn hostile_at(&self, node: &str) -> bool {
        self.daemons.iter().any(|d| d.hostile && d.current_node == node)
    }

    /// Names of hostile daemons at the given node (for SCAN).
    pub fn names_at(&self, node: &str) -> Vec<String> {
        self.daemons.iter()
            .filter(|d| d.hostile && d.current_node == node)
            .map(|d| d.name.clone())
            .collect()
    }

    /// Drop the hunter-killer onto the player's node (called at trace ≥ 80).
    pub fn activate_hunter(&mut self, node: &str) {
        if let Some(h) = self.daemons.iter_mut().find(|d| d.id == HUNTER_ID) {
            if h.current_node == DORMANT {
                h.current_node = node.to_string();
                let msg = h.enter_messages.first().cloned();
                if let Some(m) = msg { ui::print_error(&m); }
            }
        }
    }

    /// If the hunter-killer is active, move it onto the player's node (pursuit).
    pub fn hunter_pursue(&mut self, player_node: &str) {
        if let Some(h) = self.daemons.iter_mut().find(|d| d.id == HUNTER_ID) {
            if h.current_node != DORMANT {
                h.current_node = player_node.to_string();
            }
        }
    }

    /// Send the hunter-killer dormant again (on jack out / flatline).
    pub fn reset_aggro(&mut self) {
        if let Some(h) = self.daemons.iter_mut().find(|d| d.id == HUNTER_ID) {
            h.current_node = DORMANT.to_string();
        }
    }
}

enum MsgKind { Enter, Leave, Idle }
