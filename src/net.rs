use std::collections::HashMap;
use serde::Deserialize;

use crate::ui;
use crate::parser;
use crate::player::Player;
use crate::world::World;
use crate::commands::Action;

// ---------------------------------------------------------------------------
// Net node data — the cyberspace layer reached via JACK IN.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct NetNode {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub links: HashMap<String, String>,
    /// Sealed until `unlock_flag` is present in the player's scored_events.
    #[serde(default)]
    pub ice: bool,
    #[serde(default)]
    pub unlock_flag: Option<String>,
    /// Text revealed by READ at this node.
    #[serde(default)]
    pub data: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NodeFile {
    nodes: Vec<NetNode>,
}

pub struct NetStore {
    pub nodes: HashMap<String, NetNode>,
}

impl NetStore {
    pub fn load() -> Result<Self, String> {
        let raw = include_str!("../nodes.toml");
        let file: NodeFile = toml::from_str(raw)
            .map_err(|e| format!("nodes.toml parse error: {}", e))?;
        let nodes = file.nodes.into_iter().map(|n| (n.id.clone(), n)).collect();
        Ok(NetStore { nodes })
    }

    pub fn get(&self, id: &str) -> Option<&NetNode> {
        self.nodes.get(id)
    }
}

/// The node the player arrives at when first jacking in.
pub const ENTRY_NODE: &str = "entry_relay";

/// Physical rooms too far from the arcology spine to carry a usable signal.
pub const NO_SIGNAL_ROOMS: &[&str] = &[
    "front_porch", "gravel_path", "garden_gate", "dead_garden", "kitchen_garden",
    "woods_edge", "deep_woods", "greenhouse", "stable",
];

// ---------------------------------------------------------------------------
// Presentation
// ---------------------------------------------------------------------------

pub fn jack_in_flavor() {
    ui::print_blank();
    ui::print_dim("[ NEURAL LINK ESTABLISHED ]");
}

/// Describe a node: header, description, and available routes.
pub fn look(net: &NetStore, node_id: &str, _player: &Player) {
    let node = match net.get(node_id) {
        Some(n) => n,
        None => { ui::print_error("[ net node missing ]"); return; }
    };
    ui::print_blank();
    ui::print_room_header(&format!("NET // {}", node.name.to_uppercase()));
    ui::print_plain(&node.description);
    let mut routes: Vec<String> = node.links.keys().cloned().collect();
    routes.sort();
    if !routes.is_empty() {
        ui::print_exits(&format!("Routes: {}", routes.join(", ")));
    }
}

// ---------------------------------------------------------------------------
// Command handling while jacked in
// ---------------------------------------------------------------------------

/// Dispatch a command issued while jacked into the net. JACK / DISCONNECT
/// transitions are handled upstream in app.rs; this never sees them.
pub fn handle(input: &str, player: &mut Player, _world: &mut World, net: &NetStore) -> Action {
    player.turn += 1;
    let cmd = parser::parse(input);
    let node_id = match player.net_node.clone() {
        Some(n) => n,
        None => return Action::Continue,
    };

    match cmd.verb.as_str() {
        "look" => look(net, &node_id, player),

        "go" => match cmd.noun.as_deref() {
            Some(label) => net_move(label, player, net),
            None => ui::print_plain("Go where? Name a route."),
        },

        "read" => net_read(player, net),

        "bypass" | "crack" | "decrypt" => net_bypass(player, net),

        "return" | "upload" | "send" => return net_return(player),

        // A bare route label typed as a verb, e.g. "archive" or "core".
        other if net.get(&node_id).map_or(false, |n| n.links.contains_key(other)) => {
            net_move(other, player, net)
        }

        _ => ui::print_plain(
            "That has no meaning in here. Try LOOK, GO <route>, READ, BYPASS, RETURN, or JACK OUT."
        ),
    }

    Action::Continue
}

fn net_move(label: &str, player: &mut Player, net: &NetStore) {
    let cur = match player.net_node.clone() { Some(c) => c, None => return };
    let node = match net.get(&cur) { Some(n) => n, None => return };

    let target_id = match node.links.get(label) {
        Some(t) => t.clone(),
        None => {
            ui::print_plain(&format!("There is no {} route from here.", label));
            return;
        }
    };

    if let Some(target) = net.get(&target_id) {
        if target.ice {
            let unlocked = target.unlock_flag.as_ref()
                .map_or(false, |f| player.scored_events.contains(f));
            if !unlocked {
                ui::print_error("ICE seals that route. The countermeasure holds — you will need to BYPASS it first.");
                return;
            }
        }
    }

    player.net_node = Some(target_id.clone());
    look(net, &target_id, player);
}

fn net_read(player: &mut Player, net: &NetStore) {
    let cur = match player.net_node.clone() { Some(c) => c, None => return };
    let node = match net.get(&cur) { Some(n) => n, None => return };

    match &node.data {
        Some(text) => {
            ui::print_dim(text);
            // Reading the archive is what makes the debt legible — advances OBJECTIVES.
            if cur == "archive_node" && player.first_time("net_debt_understood") {
                ui::print_blank();
                ui::print_score_notice("[ Objective updated: you understand what Axiom owes. ]");
            }
        }
        None => ui::print_plain("There is nothing here to read."),
    }
}

fn net_bypass(player: &mut Player, net: &NetStore) {
    let cur = match player.net_node.clone() { Some(c) => c, None => return };
    let node = match net.get(&cur) { Some(n) => n, None => return };

    // Find a linked ICE node that is still sealed.
    let target = node.links.values()
        .filter_map(|id| net.get(id))
        .find(|n| n.ice && n.unlock_flag.as_ref()
            .map_or(false, |f| !player.scored_events.contains(f)));

    match target {
        None => ui::print_plain("There is no ICE here to bypass."),
        Some(t) => {
            let flag = match &t.unlock_flag { Some(f) => f.clone(), None => return };
            if player.has_item("iron_ring") {
                player.first_time(&flag);
                ui::print_exits("You feed the master access pattern off the iron ring into the countermeasure. The ICE unwinds strand by strand and falls open. The route to the core is clear.");
            } else {
                ui::print_error("The ICE is keyed to the building's physical master ring. Without it, the countermeasure does not yield.");
            }
        }
    }
}

fn net_return(player: &mut Player) -> Action {
    let at_relay = player.net_node.as_deref() == Some("upload_relay");
    if !at_relay {
        ui::print_plain("There is no return queue at this node. The relay is elsewhere in the net.");
        return Action::Continue;
    }

    if player.score < 100 {
        ui::print_error("The relay holds the channel open but refuses to send. The return is incomplete.");
        ui::print_dim("The legacy must be fully delivered to the lobby console before the relay will authorise the send. Check OBJECTIVES.");
        return Action::Continue;
    }

    // The debt closes.
    ui::print_blank();
    ui::print_dim("[ AUTHORISING RETURN... ]");
    ui::print_blank();
    ui::print_win("****  THE ACCOUNTING BALANCES  ****");
    ui::print_blank();
    ui::print_plain("The amber channel turns white and empties. Three years of held breath release at once.");
    ui::print_plain("The extracted legacy crosses into the Protocol's keeping, and the obligation — carried, deferred, suppressed, and at last honoured — discharges in a single clean transaction.");
    ui::print_plain("The substrate goes quiet. For the first time since the arcology was poured over it, the account is closed.");
    ui::print_blank();
    ui::print_win("You are the Ghost in the Machine.");
    ui::print_blank();

    Action::Quit
}
