//! Headless tests for game logic and embedded data. The game itself only runs
//! in a GPU window, but command handling and the data files don't touch it.

use crate::ambient::Ambient;
use crate::commands;
use crate::mobs::MobStore;
use crate::net::{self, NetStore};
use crate::net_daemons::DaemonStore;
use crate::npcs::NpcStore;
use crate::player::Player;
use crate::ui;
use crate::world::World;

struct Game {
    world: World,
    player: Player,
    npcs: NpcStore,
    mobs: MobStore,
    amb: Ambient,
    net: NetStore,
    daemons: DaemonStore,
}

fn game() -> Game {
    Game {
        world: World::load().unwrap(),
        player: Player::new("foyer"),
        npcs: NpcStore::load().unwrap(),
        mobs: MobStore::load().unwrap(),
        amb: Ambient::new(),
        net: NetStore::load().unwrap(),
        daemons: DaemonStore::load().unwrap(),
    }
}

impl Game {
    fn cmd(&mut self, input: &str) {
        commands::handle(input, &mut self.player, &mut self.world, &self.npcs, &mut self.mobs, &mut self.amb);
        ui::drain();
    }
    fn net_cmd(&mut self, input: &str) {
        net::handle(input, &mut self.player, &mut self.world, &self.net, &mut self.daemons);
        ui::drain();
    }
}

// ---------------------------------------------------------------------------
// Logic regressions
// ---------------------------------------------------------------------------

#[test]
fn meta_commands_take_no_time() {
    let mut g = game();
    for verb in ["score", "help", "objectives", "inventory", "version", "runtime", "brief"] {
        g.cmd(verb);
    }
    assert_eq!(g.player.turn, 0, "informational commands must not advance the turn");
    g.cmd("wait");
    assert_eq!(g.player.turn, 1);
}

#[test]
fn contested_exit_cannot_skip_the_hunter_threshold() {
    let mut g = game();
    g.player.net_node = Some(net::ENTRY_NODE.to_string());
    g.player.trace = 76;
    // A hostile daemon at the entry node makes leaving cost +6 trace (76 → 82),
    // which happens before normal accrual — the crossing of 80 must still count.
    g.daemons.daemons.iter_mut().find(|d| d.id == "watchdog").unwrap()
        .current_node = net::ENTRY_NODE.to_string();
    g.net_cmd("go archive");
    assert!(g.player.trace >= 80, "trace was {}", g.player.trace);
    assert!(g.player.scored_events.contains("hunter_active"), "hunter-killer never dispatched");
    let hunter = g.daemons.daemons.iter().find(|d| d.id == "hunter_killer").unwrap();
    assert_eq!(hunter.current_node, "archive_node");
}

#[test]
fn score_is_capped_but_events_still_register() {
    let mut p = Player::new("foyer");
    p.score = crate::player::MAX_SCORE - 3;
    assert_eq!(p.award("a", 8), Some(3), "should only gain up to the cap");
    assert_eq!(p.score, crate::player::MAX_SCORE);
    assert_eq!(p.award("b", 5), Some(0), "first-time event at the cap still registers");
    assert_eq!(p.award("b", 5), None, "repeat event is not re-awarded");
}

#[test]
fn ghost_routine_shakes_the_hunter_killer() {
    let mut g = game();
    g.player.net_node = Some(net::ENTRY_NODE.to_string());
    g.player.take_item("ghost_routine".to_string());
    g.player.trace = 85;
    g.player.first_time("hunter_active");
    g.daemons.activate_hunter(net::ENTRY_NODE);
    ui::drain();
    assert!(g.daemons.hunter_active());

    g.net_cmd("run ghost_routine");
    assert!(!g.daemons.hunter_active(), "hunter-killer still pursuing");
    assert!(!g.player.scored_events.contains("hunter_active"), "hunter must be able to re-arm");
    assert!(g.player.trace < 80, "trace was {}", g.player.trace);
}

#[test]
fn dropping_installed_cyberware_uninstalls_it() {
    let mut p = Player::new("foyer");
    p.take_item("neural_dampener".to_string());
    p.worn.insert("neural_dampener".to_string());
    p.drop_item("neural_dampener");
    assert!(!p.worn.contains("neural_dampener"));
}

// ---------------------------------------------------------------------------
// Data integrity — every id a data file points at must exist
// ---------------------------------------------------------------------------

#[test]
fn room_exits_lead_to_real_rooms() {
    let w = World::load().unwrap();
    for room in w.rooms.values() {
        for (dir, dest) in &room.exits {
            assert!(w.rooms.contains_key(dest), "{} --{}--> missing room '{}'", room.id, dir, dest);
        }
    }
    for id in net::NO_SIGNAL_ROOMS {
        assert!(w.rooms.contains_key(*id), "NO_SIGNAL_ROOMS names missing room '{}'", id);
    }
}

#[test]
fn net_links_lead_to_real_nodes() {
    let net = NetStore::load().unwrap();
    assert!(net.get(net::ENTRY_NODE).is_some(), "entry node missing");
    for node in net.nodes.values() {
        for (label, dest) in &node.links {
            assert!(net.get(dest).is_some(), "{} --{}--> missing node '{}'", node.id, label, dest);
        }
    }
}

#[test]
fn daemons_patrol_real_nodes() {
    let net = NetStore::load().unwrap();
    for d in DaemonStore::load().unwrap().daemons {
        assert!(d.current_node.is_empty() || net.get(&d.current_node).is_some(),
            "daemon {} starts on missing node '{}'", d.id, d.current_node);
        for n in &d.wander_nodes {
            assert!(net.get(n).is_some(), "daemon {} wanders to missing node '{}'", d.id, n);
        }
    }
}

#[test]
fn mobs_wander_real_rooms() {
    let w = World::load().unwrap();
    for m in MobStore::load().unwrap().mobs {
        assert!(w.rooms.contains_key(&m.current_room), "mob {} starts in missing room", m.id);
        for r in &m.wander_rooms {
            assert!(w.rooms.contains_key(r), "mob {} wanders to missing room '{}'", m.id, r);
        }
    }
}
