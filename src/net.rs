use std::collections::HashMap;
use serde::Deserialize;

use crate::ui;
use crate::parser;
use crate::player::Player;
use crate::world::World;
use crate::net_daemons::DaemonStore;
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
    /// Black ICE: a failed/forced pass costs `attack` neural integrity.
    #[serde(default)]
    pub black: bool,
    #[serde(default)]
    pub attack: u8,
    /// Per-turn trace gain while sitting at this node.
    #[serde(default = "default_trace_rate")]
    pub trace_rate: u8,
    /// ICE family — which ICEbreaker program cracks it (e.g. "military").
    #[serde(default)]
    pub ice_type: Option<String>,
    /// Braindance memory: a scripted scene played by READ at this node.
    #[serde(default)]
    pub memory: Vec<String>,
}

fn default_trace_rate() -> u8 { 4 }

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
pub fn handle(input: &str, player: &mut Player, _world: &mut World, net: &NetStore, daemons: &mut DaemonStore) -> Action {
    player.turn += 1;
    let cmd = parser::parse(input);
    let node_id = match player.net_node.clone() {
        Some(n) => n,
        None => return Action::Continue,
    };

    match cmd.verb.as_str() {
        "look" => look(net, &node_id, player),

        "go" => match cmd.noun.as_deref() {
            Some(label) => net_move(label, player, net, daemons),
            None => ui::print_plain("Go where? Name a route."),
        },

        "read" => net_read(player, net),

        "bypass" | "crack" | "decrypt" => net_bypass(player, net),

        "run" | "execute" | "load" => match cmd.noun.as_deref() {
            Some(prog) => net_run(prog, player, net),
            None => ui::print_plain("Run what? Name a program you're carrying."),
        },

        "scan" => net_scan(player, net, daemons),

        "return" | "upload" | "send" => return net_return(player),

        // A bare route label typed as a verb, e.g. "archive" or "core".
        other if net.get(&node_id).map_or(false, |n| n.links.contains_key(other)) => {
            net_move(other, player, net, daemons)
        }

        _ => ui::print_plain(
            "That has no meaning in here. Try LOOK, GO <route>, READ, RUN <program>, SCAN, BYPASS, RETURN, or JACK OUT."
        ),
    }

    // Black ICE during this turn may have zeroed integrity.
    if player.integrity == 0 {
        flatline(player);
        daemons.reset_aggro();
        return Action::Continue;
    }

    // Daemons act, then trace accrues based on the (possibly new) current node.
    if let Some(cur) = player.net_node.clone() {
        // The hunter-killer (if active) homes onto you; other daemons wander.
        daemons.hunter_pursue(&cur);
        daemons.tick(&cur);

        let mut rate = net.get(&cur).map(|n| n.trace_rate).unwrap_or(4);
        if player.worn.contains("trace_buffer") {
            rate = rate.saturating_sub(2).max(1);
        }
        let spike = daemons.total_spike_at(&cur);
        if spike > 0 {
            ui::print_error(&format!("Hostile daemons flood your channel — trace +{} this turn.", spike));
        }
        let add = rate.saturating_add(spike);

        let before = player.trace;
        player.trace = before.saturating_add(add).min(100);
        if before < 50 && player.trace >= 50 {
            ui::print_ambient("A trace is tightening on your signal. Something is following it back toward you.");
        }
        if before < 80 && player.trace >= 80 {
            ui::print_error("TRACE CRITICAL — a hunter-killer is dispatched. Shake it or get out.");
            if player.first_time("hunter_active") {
                daemons.activate_hunter(&cur);
            }
        }
        if player.trace >= 100 {
            flatline(player);
            daemons.reset_aggro();
        }
    }

    Action::Continue
}

/// Forced disconnect when integrity or trace maxes out: dumped to the physical
/// world with an integrity reboot, trace cleared, and a score penalty.
fn flatline(player: &mut Player) {
    const PENALTY: u32 = 15;
    let lost = player.score.min(PENALTY);

    ui::print_blank();
    ui::print_error("[ FLATLINE ]");
    ui::print_plain("The countermeasures close over you all at once. Your deck severs the link to save what's left of you, and the physical room comes back hard and too bright.");

    player.net_node  = None;
    player.trace     = 0;
    player.integrity = 20;
    player.score     = player.score.saturating_sub(PENALTY);
    player.turn      = player.turn.saturating_add(3);
    // Dropping below 100 re-arms the console nudge if the player climbs back.
    player.scored_events.remove("console_armed_nudge");
    // Let the hunter-killer re-arm on a future run.
    player.scored_events.remove("hunter_active");

    if lost > 0 {
        ui::print_dim(&format!("Neural integrity rebooted to 20%. Trace cleared. The dump cost you {} points.", lost));
    } else {
        ui::print_dim("Neural integrity rebooted to 20%. Trace cleared.");
    }
}

fn net_move(label: &str, player: &mut Player, net: &NetStore, daemons: &DaemonStore) {
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

    // A hostile daemon contests your exit — pushing past it costs extra trace
    // (friction, never a hard wall), except when retreating along the back route.
    if label != "back" && daemons.hostile_at(&cur) {
        player.trace = player.trace.saturating_add(6).min(100);
        ui::print_error("You force your signal past the daemon on the way out — trace +6.");
    }

    player.net_node = Some(target_id.clone());
    look(net, &target_id, player);
}

fn net_read(player: &mut Player, net: &NetStore) {
    let cur = match player.net_node.clone() { Some(c) => c, None => return };
    let node = match net.get(&cur) { Some(n) => n, None => return };

    // Braindance memory node: play the scripted scene.
    if !node.memory.is_empty() {
        let first = player.first_time(&format!("braindance_{}", cur));
        ui::print_blank();
        ui::print_dim(if first {
            "[ BRAINDANCE — neural playback engaged ]"
        } else {
            "[ BRAINDANCE — replay ]"
        });
        ui::print_blank();
        for line in &node.memory {
            ui::print_ambient(line);
        }
        ui::print_blank();
        ui::print_dim("[ playback ends ]");
        return;
    }

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
            let black = t.black;
            let attack = t.attack;
            if player.has_item("iron_ring") {
                player.first_time(&flag);
                ui::print_exits("You feed the master access pattern off the iron ring into the countermeasure. The ICE unwinds strand by strand and falls open. The route to the core is clear.");
            } else {
                ui::print_error("The ICE is keyed to the building's physical master ring. Without it, the countermeasure does not yield. An ICEbreaker program might force it instead.");
                if black {
                    bite(player, attack);
                }
            }
        }
    }
}

/// Apply black-ICE damage, halved if a neural dampener is installed.
fn bite(player: &mut Player, attack: u8) {
    if attack == 0 { return; }
    let dmg = if player.worn.contains("neural_dampener") {
        (attack / 2).max(1)
    } else {
        attack
    };
    player.integrity = player.integrity.saturating_sub(dmg);
    let note = if player.worn.contains("neural_dampener") { "  (dampened)" } else { "" };
    ui::print_error(&format!(
        "Black ICE lashes back down the link. Neural integrity −{}.{}  ({}% remaining)",
        dmg, note, player.integrity));
}

/// RUN a program from inventory while jacked in.
fn net_run(prog_noun: &str, player: &mut Player, net: &NetStore) {
    let normalized = prog_noun.replace(' ', "_");
    // Resolve against inventory (exact, then fuzzy).
    let prog = if player.has_item(&normalized) {
        normalized
    } else {
        match parser::fuzzy_match(prog_noun, &player.inventory) {
            Some(m) => m.clone(),
            None => {
                ui::print_error(&format!("You aren't carrying any program called '{}'.", prog_noun.replace('_', " ")));
                return;
            }
        }
    };

    match prog.as_str() {
        "icebreaker_hammer" => run_icebreaker(&prog, "military", player, net),

        "ghost_routine" => {
            let before = player.trace;
            player.trace = player.trace.saturating_sub(40);
            ui::print_exits(&format!(
                "Ghost routine deploys — it scatters decoy signatures across the mesh. Trace falls {}% → {}%.",
                before, player.trace));
        }

        "repair_daemon" => {
            if player.integrity >= 100 {
                ui::print_plain("Your neural integrity is already nominal. The repair daemon idles.");
                return;
            }
            let before = player.integrity;
            player.integrity = (player.integrity + 40).min(100);
            // Consume it.
            player.drop_item(&prog);
            ui::print_exits(&format!(
                "The repair daemon knits your fried connection back together. Integrity {}% → {}%. The program burns out in the process.",
                before, player.integrity));
        }

        "decryptor" => {
            ui::print_plain("The decryptor spins up, finds nothing encrypted at this node, and idles.");
        }

        _ => ui::print_plain(&format!("The {} isn't a program you can run in here.", prog.replace('_', " "))),
    }
}

/// Use an ICEbreaker against a linked sealed ICE node whose type matches.
fn run_icebreaker(prog: &str, breaks: &str, player: &mut Player, net: &NetStore) {
    let cur = match player.net_node.clone() { Some(c) => c, None => return };
    let node = match net.get(&cur) { Some(n) => n, None => return };

    let target = node.links.values()
        .filter_map(|id| net.get(id))
        .find(|n| n.ice && n.unlock_flag.as_ref()
            .map_or(false, |f| !player.scored_events.contains(f)));

    match target {
        None => ui::print_plain("There is no sealed ICE adjacent to run that against."),
        Some(t) => {
            let matches = t.ice_type.as_deref() == Some(breaks);
            let flag = match &t.unlock_flag { Some(f) => f.clone(), None => return };
            let black = t.black;
            let attack = t.attack;
            if matches {
                player.first_time(&flag);
                ui::print_exits(&format!(
                    "You load the {} and drive it into the countermeasure. It chews through the ICE layer by layer until the barrier collapses. The route opens.",
                    prog.replace('_', " ")));
            } else {
                ui::print_error(&format!(
                    "The {} grinds against this ICE and finds no purchase — wrong countermeasure class for this barrier.",
                    prog.replace('_', " ")));
                if black {
                    bite(player, attack);
                }
            }
        }
    }
}

/// SCAN: optic-implant sweep. In the net, reveal adjacent ICE classes + daemons.
fn net_scan(player: &mut Player, net: &NetStore, daemons: &DaemonStore) {
    if !player.worn.contains("optic_implant") {
        ui::print_plain("You have no optic implant online. There is nothing to scan with.");
        return;
    }
    let cur = match player.net_node.clone() { Some(c) => c, None => return };
    let node = match net.get(&cur) { Some(n) => n, None => return };

    // Hostile daemons sharing this node.
    let here = daemons.names_at(&cur);
    if here.is_empty() {
        ui::print_dim("Optic sweep — no hostile processes share this node.");
    } else {
        ui::print_error(&format!("Optic sweep — hostile here: {}.", here.join(", ")));
    }

    ui::print_exits("Adjacent nodes:");
    let mut any_ice = false;
    let mut labels: Vec<(&String, &String)> = node.links.iter().collect();
    labels.sort_by(|a, b| a.0.cmp(b.0));
    for (label, target_id) in labels {
        if let Some(t) = net.get(target_id) {
            let sealed = t.ice && t.unlock_flag.as_ref()
                .map_or(false, |f| !player.scored_events.contains(f));
            if sealed {
                any_ice = true;
                let class = t.ice_type.as_deref().unwrap_or("unknown");
                let black = if t.black { ", BLACK ICE — bites on a failed pass" } else { "" };
                ui::print_dim(&format!("  {} → {}: sealed, {} class{}", label, t.name, class, black));
            } else {
                ui::print_dim(&format!("  {} → {}: open", label, t.name));
            }
        }
    }
    if !any_ice {
        ui::print_dim("  No active ICE on any adjacent route.");
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
