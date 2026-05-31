use crate::parser;
use crate::player::{Player, VerboseMode};
use crate::world::World;
use crate::npcs::NpcStore;
use crate::mobs::MobStore;
use crate::ambient::{self, Ambient};
use crate::ui;

/// What the main loop should do after a command completes.
pub enum Action {
    Continue,
    Quit,
    Restart,
    Exit,
}

/// Resident set size of the current process in bytes, via getrusage(2).
/// Returns None if the syscall fails.
fn rss_bytes() -> Option<u64> {
    let mut usage = unsafe { std::mem::zeroed::<libc::rusage>() };
    let ret = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    if ret != 0 { return None; }
    // macOS reports ru_maxrss in bytes; Linux reports in kilobytes.
    #[cfg(target_os = "macos")]
    let bytes = usage.ru_maxrss as u64;
    #[cfg(not(target_os = "macos"))]
    let bytes = (usage.ru_maxrss as u64) * 1024;
    Some(bytes)
}

const DIRECTIONS: &[&str] = &["north", "south", "east", "west", "up", "down"];

/// Entry point for all input. Returns the action the main loop should take.
pub fn handle(input: &str, player: &mut Player, world: &mut World, npcs: &NpcStore, mobs: &mut MobStore, amb: &mut Ambient) -> Action {
    player.turn += 1;
    let cmd = parser::parse(input);

    match cmd.verb.as_str() {
        "quit" => return Action::Exit,

        // SAVE / RESTORE are intercepted in app.rs (they need daemon state).

        "restart" => return Action::Restart,

        "help" => print_help(),

        "look" => match &cmd.noun {
            None       => look(player, world, mobs),
            Some(noun) => examine(noun, player, world, mobs),
        },

        "inventory" => print_inventory(player),

        "score" => print_score(player),

        "objectives" => print_objectives(player),

        "verbose" => {
            player.verbose_mode = VerboseMode::Verbose;
            ui::print_plain("VERBOSE. Room descriptions will always be shown in full.");
        }

        "brief" => {
            player.verbose_mode = VerboseMode::Brief;
            ui::print_plain("BRIEF. Full descriptions on first visit; room name only on return.");
        }

        "superbrief" => {
            player.verbose_mode = VerboseMode::SuperBrief;
            ui::print_plain("SUPERBRIEF. Only the room name will be shown when you move.");
        }

        "version" | "ver" => print_version(),

        "runtime" => print_runtime(player, world, npcs, mobs),

        // Secret cheat code — not listed in help
        "mappy" if cmd.noun.as_deref() == Some("map") => {
            for line in include_str!("../map.txt").lines() {
                ui::print_plain(line);
            }
        }

        "take" => match &cmd.noun {
            Some(noun) if noun == "all" => take_all(player, world),
            Some(noun) => take(noun, player, world),
            None       => ui::print_plain("Take what?"),
        },

        "drop" => match &cmd.noun {
            Some(noun) => drop_item(noun, player, world),
            None       => ui::print_plain("Drop what?"),
        },

        "read" => match &cmd.noun {
            Some(noun) => read_item(noun, player, world),
            None       => ui::print_plain("Read what?"),
        },

        "unlock" => match &cmd.noun {
            Some(noun) => unlock_target(noun, player, world),
            None       => ui::print_plain("Unlock what?"),
        },

        "decrypt" | "use" => cmd_decrypt(cmd.noun.as_deref(), player, world),

        dir if DIRECTIONS.contains(&dir) => go(dir, player, world, npcs, mobs),

        // Handle "go <direction>" as a two-word command
        "go" => match &cmd.noun {
            Some(noun) if DIRECTIONS.contains(&noun.as_str()) => go(noun, player, world, npcs, mobs),
            Some(noun) => ui::print_plain(&format!("Go where? I don't understand '{}'.", noun)),
            None       => ui::print_plain("Go where?"),
        },

        "ask" | "tell" => cmd_ask_tell(input, player, npcs),

        "wait" => {
            ui::print_plain("Time passes.");
        }

        // UNDO and AGAIN are both handled in app.rs before this function is called.
        "undo"  => ui::print_plain("Nothing to undo."),
        "again" => ui::print_plain("Nothing to repeat."),

        "smell" => match &cmd.noun {
            Some(noun) => smell_item(noun, player, world),
            None       => smell_room(&player.current_room),
        },

        "listen" => listen_room(&player.current_room),

        "touch" => match &cmd.noun {
            Some(noun) => touch_item(noun, player, world),
            None       => touch_room(&player.current_room),
        },

        "search" => search_room(player, world),

        "push" | "pull" | "turn" | "press" => match &cmd.noun {
            Some(noun) => push_pull(cmd.verb.as_str(), noun, player),
            None       => ui::print_plain("Push what?"),
        },

        "knock" => knock(cmd.noun.as_deref(), player),

        "wear" => match &cmd.noun {
            Some(noun) => wear_item(noun, player),
            None       => ui::print_plain("Wear what?"),
        },

        "remove" => match &cmd.noun {
            Some(noun) => remove_item(noun, player),
            None       => ui::print_plain("Remove what?"),
        },

        "run" => ui::print_plain("You can only run programs jacked into the net. JACK IN first."),

        "scan" => scan_room(player),

        "buy" => buy_item(cmd.noun.as_deref(), player, npcs),

        "ghost" => cmd_ghost(player, world),

        _ => ui::print_error(&format!("I don't understand '{}'.", input)),
    }

    amb.tick(&player.current_room, player.turn);
    mobs.tick(&player.current_room);

    // Neural integrity recovers slowly while out of the net.
    if player.net_node.is_none() && player.integrity < 100 {
        player.integrity = (player.integrity + 5).min(100);
    }

    // Reaching a full score arms the console, but the return itself is made in
    // the net at the upload relay — not here. Nudge the player there once.
    if player.score >= 100 && player.first_time("console_armed_nudge") {
        ui::print_blank();
        ui::print_score_notice("The lobby upload console flares to a steady amber. The full legacy is staged.");
        ui::print_dim("Jack into the net and route to the upload relay to authorise the return.");
        ui::print_blank();
    }

    Action::Continue
}

/// Print the current room name, description, exits, items, and any mobs present.
pub fn look(player: &Player, world: &World, mobs: &MobStore) {
    match world.get_room(&player.current_room) {
        None => ui::print_error(&format!("[ERROR: current room '{}' not found in world]", player.current_room)),
        Some(room) => {
            ui::print_blank();
            ui::print_room_header(&format!("-- {} --", room.name.to_uppercase()));
            if let Some(flavor) = ambient::time_flavor(&player.current_room, player.turn) {
                ui::print_ambient(flavor);
            }
            ui::print_plain(&room.description);

            // Exits
            if room.exits.is_empty() {
                ui::print_blank();
                ui::print_exits("Exits: none");
            } else {
                let mut exit_list: Vec<&String> = room.exits.keys().collect();
                exit_list.sort();
                let exits = exit_list.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
                ui::print_blank();
                ui::print_exits(&format!("Exits: {}", exits));
            }

            // Items on the floor
            if !room.items.is_empty() {
                let item_list = room.items.iter()
                    .map(|i| i.replace('_', " "))
                    .collect::<Vec<_>>()
                    .join(", ");
                ui::print_items(&format!("You can see: {}", item_list));
            }

            // Mobs in this room
            for mob in mobs.in_room(&player.current_room) {
                ui::print_ambient(&mob.presence);
            }

            ui::print_blank();
        }
    }
}

/// Print only the room name header, items, and any mobs — no description text.
/// Used by BRIEF mode on revisits and SUPERBRIEF mode always.
pub fn look_short(player: &Player, world: &World, mobs: &MobStore) {
    match world.get_room(&player.current_room) {
        None => ui::print_error(&format!("[ERROR: current room '{}' not found in world]", player.current_room)),
        Some(room) => {
            ui::print_blank();
            ui::print_room_header(&format!("-- {} --", room.name.to_uppercase()));

            if !room.items.is_empty() {
                let item_list = room.items.iter()
                    .map(|i| i.replace('_', " "))
                    .collect::<Vec<_>>()
                    .join(", ");
                ui::print_items(&format!("You can see: {}", item_list));
            }

            for mob in mobs.in_room(&player.current_room) {
                ui::print_ambient(&mob.presence);
            }

            ui::print_blank();
        }
    }
}

/// Examine a specific item or mob in the room or inventory.
fn examine(noun: &str, player: &Player, world: &World, mobs: &MobStore) {
    // Check mobs in the current room first
    if let Some(mob) = mobs.find_in_room(noun, &player.current_room) {
        ui::print_plain(&mob.description);
        return;
    }

    let normalized = noun.replace(' ', "_");

    // Gather all visible items: inventory + room floor
    let room_items: Vec<String> = world.get_room(&player.current_room)
        .map(|r| r.items.clone())
        .unwrap_or_default();

    let all_items: Vec<String> = player.inventory.iter()
        .chain(room_items.iter())
        .cloned()
        .collect();

    // Exact match first, then fuzzy across both pools
    let matched = if all_items.contains(&normalized) {
        Some(normalized)
    } else {
        parser::fuzzy_match(noun, &all_items).cloned()
    };

    match matched {
        Some(item) => match gear_description(&item) {
            Some(desc) => ui::print_plain(desc),
            None => ui::print_plain(&format!(
                "You examine the {}. It looks significant, but offers no further secrets.",
                item.replace('_', " ")
            )),
        },
        None => ui::print_plain(&format!("You don't see any {} here.", noun.replace('_', " "))),
    }
}

/// Flavor + usage text for cyberpunk gear (programs, cyberware, credit media).
fn gear_description(item: &str) -> Option<&'static str> {
    Some(match item {
        "cyberdeck"         => "Your deck — a battered personal cyberdeck. JACK IN with it to reach the local net.",
        "icebreaker_hammer" => "A brute-force ICEbreaker on a worn cartridge. It chews through military-class barrier ICE. RUN it against sealed ICE while jacked in.",
        "ghost_routine"     => "A stealth program. It scatters decoy signatures across the mesh — RUN it in the net to shed accumulated trace.",
        "repair_daemon"     => "A single-use restoration program. RUN it in the net to rebuild neural integrity. It burns out on use.",
        "decryptor"         => "A decryption suite for locked datachips. RUN it where encrypted data is staged.",
        "neural_dampener"   => "A subdermal feedback buffer. INSTALL (WEAR) it to halve the integrity damage black ICE deals.",
        "trace_buffer"      => "A signal-masking implant. INSTALL it to slow how fast a trace builds while you're jacked in.",
        "optic_implant"     => "An augmented-optics package. INSTALL it to enable SCAN — spectral sweeps of rooms and adjacent net nodes.",
        "credit_shard"      => "A loaded credit shard. Take it to bank the credits.",
        "credit_stick"      => "A fat credit stick. Take it to bank the credits.",
        "auditor_datachip"  => "An encrypted datachip in a scuffed caddy. READ it with a decryptor program to crack it.",
        "director_datachip" => "An encrypted datachip bearing an executive seal. READ it with a decryptor to decrypt.",
        "tech_datachip"     => "A grease-smudged datachip, encrypted. READ it with a decryptor to recover the data.",
        _ => return None,
    })
}

/// Move the player in a direction.
fn go(direction: &str, player: &mut Player, world: &World, npcs: &NpcStore, mobs: &MobStore) {
    let destination = world
        .get_room(&player.current_room)
        .and_then(|room| room.exits.get(direction))
        .cloned();

    match destination {
        None => ui::print_error(&format!("You can't go {} from here.", direction)),
        Some(dest_id) => {
            if world.get_room(&dest_id).is_none() {
                ui::print_error(&format!("[ERROR: destination room '{}' not found in world]", dest_id));
                return;
            }
            let is_first_visit = !player.visited.contains(&dest_id);
            player.move_to(&dest_id);

            if is_first_visit {
                let (pts, msg) = match dest_id.as_str() {
                    "wine_cellar"     => (4, "The cooling systems are still running. Everything else has been stripped or sealed. This is where the secrets were stored."),
                    "crypt"           => (6, "You have gone as deep as the arcology's infrastructure allows. The Protocol Core. Whatever runs here has been waiting."),
                    "master_bedroom"  => (5, "The Director's private quarters. Whatever decisions shaped this building, they were finalized here."),
                    "woods_edge"      => (5, "The sprawl. The arcology is behind you now, its towers visible above the urban fog. You are outside its reach."),
                    "attic"           => (5, "The rooftop installation. Above the executive floor, above everything — a space no one was supposed to return to."),
                    _                 => (0, ""),
                };
                if pts > 0 {
                    let awarded = player.award(&format!("explore_{}", dest_id), pts);
                    if awarded > 0 {
                        ui::print_ambient(msg);
                        score_notice(awarded);
                    }
                }
            }

            // Choose how much of the room to display based on the player's verbose mode.
            match player.verbose_mode {
                VerboseMode::Verbose => look(player, world, mobs),
                VerboseMode::Brief => {
                    if is_first_visit {
                        look(player, world, mobs);
                    } else {
                        look_short(player, world, mobs);
                    }
                }
                VerboseMode::SuperBrief => look_short(player, world, mobs),
            }

            if let Some(npc) = npcs.get(&dest_id) {
                if is_first_visit {
                    print_npc_lines(&npc.name, &npc.lines);
                } else {
                    // Item-triggered response: first matching item the player holds, fires once each
                    let response = npc.item_responses.iter().find(|r| {
                        player.has_item(&r.item)
                            && player.first_time(&format!("npc_item_{}_{}", dest_id, r.item))
                    });
                    if let Some(r) = response {
                        print_npc_lines(&npc.name, &r.lines);
                    } else if !npc.revisit_lines.is_empty()
                        && player.first_time(&format!("npc_revisit_{}", dest_id))
                    {
                        print_npc_lines(&npc.name, &npc.revisit_lines);
                    }
                }
            }
        }
    }
}

/// Take an item from the current room.
fn take(noun: &str, player: &mut Player, world: &mut World) {
    let normalized = noun.replace(' ', "_");

    let room_items: Vec<String> = match world.get_room(&player.current_room) {
        Some(r) => r.items.clone(),
        None => return,
    };

    // Exact match first, then fuzzy
    let item = if room_items.contains(&normalized) {
        normalized
    } else {
        match parser::fuzzy_match(noun, &room_items) {
            Some(m) => m.clone(),
            None => {
                ui::print_error(&format!("There is no {} here.", noun.replace('_', " ")));
                return;
            }
        }
    };

    // Credit shards convert straight to credits instead of entering inventory.
    if let Some(value) = credit_value(&item) {
        if let Some(room) = world.get_room_mut(&player.current_room) {
            room.items.retain(|i| i != &item);
        }
        player.credits += value;
        ui::print_plain(&format!(
            "You pocket the {} — {} credits. Balance: {}.",
            item.replace('_', " "), value, player.credits));
        return;
    }

    if let Some(room) = world.get_room_mut(&player.current_room) {
        room.items.retain(|i| i != &item);
    }
    player.take_item(item.clone());
    ui::print_plain(&format!("You take the {}.", item.replace('_', " ")));

    // Discovery bonuses
    let (pts, msg) = match item.as_str() {
        "leather_journal" => (5, "The field notes fall open to a late entry, the handwriting growing tighter and more urgent. Someone was trying to document something before time ran out."),
        "signet_ring"     => (8, "The ring carries the Director's biometric seal — a proprietary authorization signature. Taking it feels like a permissions violation. Taking it also feels necessary."),
        "iron_ring"       => (4, "A heavy ring of physical access keys, most of the subsidiary keys no longer valid. A record of every locked system in this building."),
        "pocket_watch"    => (5, "The timepiece has stopped. The hands are frozen at eleven minutes past two. You wind the crown. It does not start."),
        "music_box"       => (6, "You raise the lid. The mechanism engages and releases four bars of a simple melody into the still air. Unbearably precise."),
        "directors_contract"    => (5, "The Director's seal is intact. The document inside is dense legal language — parties, assets, conditions. One clause has been annotated twice in a different hand."),
        "old_photograph"  => (4, "A formal photograph printed on archival stock. A family on the arcology's entrance steps. The timestamp on the reverse is eighteen years ago. None of them appear to have fared well."),
        _                 => (0, ""),
    };
    if pts > 0 {
        let awarded = player.award(&format!("discover_{}", item), pts);
        if awarded > 0 {
            ui::print_ambient(msg);
            score_notice(awarded);
        }
    }
}

/// Take all items from the current room.
fn take_all(player: &mut Player, world: &mut World) {
    let room_items: Vec<String> = match world.get_room(&player.current_room) {
        Some(r) => r.items.clone(),
        None => return,
    };

    if room_items.is_empty() {
        ui::print_plain("There is nothing here to take.");
        return;
    }

    for item in room_items {
        take(&item, player, world);
    }
}

/// Drop an item into the current room.
fn drop_item(noun: &str, player: &mut Player, world: &mut World) {
    let normalized = noun.replace(' ', "_");

    // Exact match first, then fuzzy against inventory
    let item_name = if player.has_item(&normalized) {
        normalized
    } else {
        match parser::fuzzy_match(noun, &player.inventory) {
            Some(m) => m.clone(),
            None => {
                ui::print_error(&format!("You aren't carrying any {}.", noun.replace('_', " ")));
                return;
            }
        }
    };

    match player.drop_item(&item_name) {
        Some(item) => {
            if let Some(room) = world.get_room_mut(&player.current_room) {
                room.items.push(item.clone());
            }
            ui::print_plain(&format!("You drop the {}.", item.replace('_', " ")));

            // Deposit bonuses (foyer only)
            if player.current_room == "foyer" {
                let (pts, msg) = match item.as_str() {
                    "signet_ring"      => (12, "You set the identity core on the upload terminal. The amber indicator intensifies. Whatever authorization it carried, it is now in the return queue."),
                    "tarnished_locket" => (10, "The locket comes to rest against the terminal housing. A small encrypted life, latched and waiting. You have returned it to the network."),
                    "directors_contract"     => (12, "You place the Director's contract on the upload terminal. The seal is intact. Whatever it documents has been waiting a very long time to be processed."),
                    "music_box"        => (9,  "The audio player sits open near the terminal. A current in the room sets its mechanism cycling faintly, almost a signal. The system responds."),
                    "pocket_watch"     => (8,  "The stopped timepiece lies face-up in the lobby. Eleven past two. Whatever it marks, the terminal acknowledges it."),
                    "old_photograph"   => (8,  "The photograph lies face-up in the lobby's dead light. A family on the entrance steps — this very plaza. None of them are smiling."),
                    "leather_journal"  => (7,  "The field notes lie open on the lobby floor. You cannot process it all, but you have returned it. Whoever wrote it deserved at least that."),
                    "dark_bottle"      => (4,  "A sealed vial from the deepest server level, its label degraded beyond identification. The contents are still sealed. The terminal pulses once."),
                    "clock_fragment" => (2,  "A fragment of the old clock sculpture, its inscription worn smooth. The terminal accepts the return regardless."),
                    _                  => (0, ""),
                };
                if pts > 0 {
                    let awarded = player.award(&format!("deposit_{}", item), pts);
                    if awarded > 0 {
                        ui::print_ambient(msg);
                        score_notice(awarded);
                    }
                }
            }
        }
        None => unreachable!("item was confirmed in inventory before drop"),
    }
}

/// Handle unlock/open commands against room-specific targets.
fn unlock_target(noun: &str, player: &mut Player, world: &mut World) {
    let normalized = noun.replace(' ', "_");
    match player.current_room.clone().as_str() {

        // Puzzle 1: Security door in the server vault — requires iron_ring
        "wine_cellar" if matches!(normalized.as_str(), "door" | "iron_door" | "lock" | "iron_lock") => {
            if !player.has_item("iron_ring") {
                ui::print_plain("The security door requires a physical key. A heavy ring of them, based on the lock mechanism.");
                return;
            }
            if world.get_room("crypt_entrance").is_none() {
                ui::print_error("[ERROR: crypt_entrance not found in world]");
                return;
            }
            if let Some(cellar) = world.get_room_mut("wine_cellar") {
                if cellar.exits.contains_key("east") {
                    ui::print_plain("The door is already open.");
                    return;
                }
                cellar.exits.insert("east".to_string(), "crypt_entrance".to_string());
            }
            let awarded = player.award("puzzle_iron_door", 6);
            ui::print_plain("One of the physical keys on the ring engages. It takes both hands to turn it.");
            ui::print_plain("The security door disengages its deadbolts and grinds open, exhaling a breath of refrigerated air from the passage beyond.");
            if awarded > 0 { score_notice(awarded); }
        }

        // Puzzle 2: Executive terminal in the office — requires cipher_key
        "study" if matches!(normalized.as_str(), "desk" | "rolltop" | "rolltop_desk" | "drawer") => {
            if !player.has_item("cipher_key") {
                ui::print_plain("The terminal's physical lockout is engaged. The override mechanism accepts a cipher key.");
                return;
            }
            if let Some(study) = world.get_room_mut("study") {
                if study.items.contains(&"directors_contract".to_string()) {
                    ui::print_plain("The desk is already open.");
                    return;
                }
                study.items.push("directors_contract".to_string());
            }
            player.drop_item("cipher_key"); // key stays in the lock
            let awarded = player.award("puzzle_desk", 8);
            ui::print_plain("The cipher key engages the physical override. The terminal panel retracts with a mechanical click.");
            ui::print_plain("Inside: archived correspondence, a dead credential token — and a folded document bearing the Director's seal.");
            if awarded > 0 { score_notice(awarded); }
        }

        // Puzzle 3: Security door on executive level — requires biometric_chip
        "upper_landing" if matches!(normalized.as_str(), "door" | "padlock" | "chain" | "lock" | "sealed_door") => {
            if !player.has_item("biometric_chip") {
                ui::print_plain("The padlock is heavy and physical. The key would be a biometric chip, based on the reader housing.");
                return;
            }
            if world.get_room("attic").is_none() {
                ui::print_error("[ERROR: attic not found in world]");
                return;
            }
            if let Some(landing) = world.get_room_mut("upper_landing") {
                if landing.exits.contains_key("up") {
                    ui::print_plain("The door is already open.");
                    return;
                }
                landing.exits.insert("up".to_string(), "attic".to_string());
            }
            let awarded = player.award("puzzle_padlock", 10);
            ui::print_plain("The biometric chip engages the combination lock. A sound like a pressurized seal releasing.");
            ui::print_plain("The chain drops. The security door swings inward on stiff hinges, revealing a maintenance stairway above.");
            if awarded > 0 { score_notice(awarded); }
        }

        _ => ui::print_error("You don't see anything to unlock here with that."),
    }
}

/// Print the player's inventory.
fn print_inventory(player: &Player) {
    if player.inventory.is_empty() {
        ui::print_plain("You are carrying nothing.");
    } else {
        ui::print_plain("You are carrying:");
        for item in &player.inventory {
            let label = item.replace('_', " ");
            if player.worn.contains(item) {
                ui::print_items(&format!("  - {} (installed)", label));
            } else {
                ui::print_items(&format!("  - {}", label));
            }
        }
    }
    ui::print_dim(&format!("Credits: {}", player.credits));
}

/// Rank title for a given score.
fn rank_for(score: u32) -> &'static str {
    match score {
        0..=9   => "Trespasser",
        10..=29 => "Ghost",
        30..=59 => "Netrunner",
        60..=84 => "Data Courier",
        85..=99 => "Protocol Witness",
        _       => "Ghost in the Machine",
    }
}

/// Print the player's current score and rank, with a full achievement breakdown.
fn print_score(player: &Player) {
    let rank = rank_for(player.score);

    let done = |key: &str| player.scored_events.contains(key);

    ui::print_blank();
    ui::print_room_header("AXIOM ARCOLOGY — FULL SCORE");
    ui::print_score_notice(&format!("  Score: {} out of 100  ({})", player.score, rank));
    ui::print_blank();

    ui::print_room_header("EXPLORATION");
    ui::print_score_line(done("explore_wine_cellar"),   "Descend to the server vault",          "4pts");
    ui::print_score_line(done("explore_crypt"),          "Enter the Protocol Core",              "6pts");
    ui::print_score_line(done("explore_master_bedroom"), "Enter the Director's Suite",           "5pts");
    ui::print_score_line(done("explore_woods_edge"),     "Reach the sprawl edge",                "5pts");
    ui::print_score_line(done("explore_attic"),          "Access the rooftop installation",      "5pts");
    ui::print_blank();

    ui::print_room_header("DISCOVERY");
    ui::print_score_line(done("discover_leather_journal"), "Find the auditor's field notes",     "5pts");
    ui::print_score_line(done("discover_signet_ring"),     "Find the identity core",             "8pts");
    ui::print_score_line(done("discover_iron_ring"),       "Find the access key ring",           "4pts");
    ui::print_score_line(done("discover_pocket_watch"),    "Find the stopped timepiece",         "5pts");
    ui::print_score_line(done("discover_music_box"),       "Find the audio player",              "6pts");
    ui::print_score_line(done("discover_directors_contract"),    "Recover the Director's contract",    "5pts");
    ui::print_score_line(done("discover_old_photograph"),  "Find the corrupted photograph",      "4pts");
    ui::print_blank();

    ui::print_room_header("PUZZLES");
    ui::print_score_line(done("puzzle_iron_door"), "Bypass the server vault security door",      "6pts");
    ui::print_score_line(done("puzzle_desk"),      "Unlock the executive terminal",              "8pts");
    ui::print_score_line(done("puzzle_padlock"),   "Override the executive level lockdown",      "10pts");
    ui::print_blank();

    ui::print_room_header("RETURNS  (upload items to the lobby terminal)");
    ui::print_score_line(done("deposit_signet_ring"),      "Upload the identity core",           "12pts");
    ui::print_score_line(done("deposit_tarnished_locket"), "Return the tarnished locket",        "10pts");
    ui::print_score_line(done("deposit_directors_contract"),     "Upload the Director's contract",     "12pts");
    ui::print_score_line(done("deposit_music_box"),        "Return the audio player",            "9pts");
    ui::print_score_line(done("deposit_pocket_watch"),     "Return the timepiece",               "8pts");
    ui::print_score_line(done("deposit_old_photograph"),   "Return the photograph",              "8pts");
    ui::print_score_line(done("deposit_leather_journal"),  "Return the field notes",             "7pts");
    ui::print_score_line(done("deposit_dark_bottle"),      "Return the sealed vial",             "4pts");
    ui::print_score_line(done("deposit_clock_fragment"), "Return the clock fragment",          "2pts");
    ui::print_blank();
}

/// Print a score increase notice.
fn score_notice(pts: u32) {
    ui::print_score_notice(&format!(
        "[Your score has increased by {} point{}.]",
        pts, if pts == 1 { "" } else { "s" }
    ));
}

fn print_npc_lines(name: &str, lines: &[String]) {
    ui::print_blank();
    ui::print_npc_name(&format!("[ {} ]", name));
    for line in lines {
        ui::print_npc_dialogue(line);
    }
    ui::print_blank();
}

/// Read a legible item in the current room or inventory.
fn read_item(noun: &str, player: &mut Player, world: &World) {
    let normalized = noun.replace(' ', "_");

    let room_items: Vec<String> = world.get_room(&player.current_room)
        .map(|r| r.items.clone())
        .unwrap_or_default();

    let all_items: Vec<String> = player.inventory.iter()
        .chain(room_items.iter())
        .cloned()
        .collect();

    let matched = if all_items.contains(&normalized) {
        Some(normalized)
    } else {
        parser::fuzzy_match(noun, &all_items).cloned()
    };

    match matched {
        None => ui::print_plain(&format!("You don't see any {} to read.", noun.replace('_', " "))),
        Some(item) if item.ends_with("_datachip") => read_datachip(&item, player),
        Some(item) => match readable_text(&item) {
            Some(text) => {
                ui::print_blank();
                for line in text.lines() {
                    ui::print_plain(line);
                }
                ui::print_blank();
            }
            None => ui::print_plain(&format!(
                "There's nothing to read on the {}.",
                item.replace('_', " ")
            )),
        },
    }
}

/// DECRYPT / USE — resolve a datachip (by name or auto-pick) and crack it.
fn cmd_decrypt(noun: Option<&str>, player: &mut Player, world: &World) {
    let room_items: Vec<String> = world.get_room(&player.current_room)
        .map(|r| r.items.clone())
        .unwrap_or_default();
    let all_items: Vec<String> = player.inventory.iter()
        .chain(room_items.iter())
        .cloned()
        .collect();

    // A noun that names the tool, the target generically, or nothing → auto-pick.
    let auto = matches!(noun, None | Some("decryptor") | Some("it") | Some("chip")
        | Some("datachip") | Some("data_chip"));

    let target: Option<String> = if auto {
        all_items.iter().find(|i| i.ends_with("_datachip")).cloned()
    } else {
        let n = noun.unwrap();
        let normalized = n.replace(' ', "_");
        let m = if all_items.contains(&normalized) {
            Some(normalized)
        } else {
            parser::fuzzy_match(n, &all_items).cloned()
        };
        match m {
            Some(item) if item.ends_with("_datachip") => Some(item),
            Some(item) => {
                ui::print_plain(&format!("The {} isn't encrypted.", item.replace('_', " ")));
                return;
            }
            None => {
                ui::print_plain(&format!("You don't see any {} here.", n.replace('_', " ")));
                return;
            }
        }
    };

    match target {
        Some(item) => read_datachip(&item, player),
        None => ui::print_plain("You have nothing encrypted to decrypt here."),
    }
}

/// Encrypted datachips: require the decryptor program. First decrypt yields
/// lore + credits (no score, so the legacy economy is untouched).
fn read_datachip(item: &str, player: &mut Player) {
    if !player.has_item("decryptor") {
        ui::print_error(&format!(
            "The {} is encrypted — military-grade. You need a decryptor program to crack it.",
            item.replace('_', " ")));
        return;
    }

    let (reward, text) = match item {
        "auditor_datachip" => (50,
"DECRYPTED — AUDITOR FIELD ARCHIVE:
  Personal log, final fragment. 'They told me the audit was routine. It is not routine.
  The arcology is carrying a liability it never recorded — a debt to whatever the founders
  built on. The Director knew. He buried it under classified cost centers. If you are
  reading this, the account is still open, and they will not let you leave until it closes.'"),
        "director_datachip" => (60,
"DECRYPTED — DIRECTOR'S PRIVATE PARTITION:
  'Containment is holding. The substrate is patient; that is its nature and its threat.
  I have sealed what can be sealed and routed the obligation to my successors. Let them
  weigh it. I am done weighing it.' The entry is signed and then, beneath, in a different
  pass: 'It was never mine to defer.'"),
        "tech_datachip" => (40,
"DECRYPTED — MAINTENANCE LOG:
  'Six weeks cutting a conduit nobody approved on paper. He said infrastructure. It wasn't
  infrastructure. I ran ferrocrete from the vault straight down to the old passage. The
  thing down there knew the moment I broke through. I have not slept properly since.'"),
        _ => (30, "DECRYPTED: fragmentary data — corrupted beyond full recovery."),
    };

    ui::print_blank();
    for line in text.lines() { ui::print_plain(line); }
    ui::print_blank();

    if player.first_time(&format!("decrypt_{}", item)) {
        player.credits += reward;
        ui::print_score_notice(&format!(
            "[ Salvaged data brokered for {} credits. Balance: {}. ]", reward, player.credits));
    }
}

fn readable_text(item: &str) -> Option<&'static str> {
    match item {
        "leather_journal" => Some(
"The field notes are dense, the handwriting growing tighter and more urgent in the later sections.

  14th — Accessed Axiom Tower. The Analyst AI has provided access to the eastern server
  stacks, though I never observe it initializing or terminating. Something has been removed
  from those stacks recently. The indicator pattern makes this clear.

  19th — Located a reference to a founding agreement dated 2047. The parties are listed as
  'Axiom Industries' and something the document designates as 'the Protocol substrate.'
  I cannot locate the primary agreement document.

  23rd — The signals at night are infrastructure cycling, I am certain of it. Old buildings
  process. I have secured my access point regardless.

  26th — The Analyst said: 'The archive retains a record of everything extracted from it.
  It does not surface what it retains on request.' I asked for clarification.
  It was no longer available for the query."
        ),

        "unfinished_letter" => Some(
"The message is addressed but was never completed. The cursor is still blinking.

  M —

  I'm writing from the visitor suite at Axiom Tower, where I've been eleven days on
  an extended access arrangement. The original invitation was irregular — there was no
  one here to extend it, and yet the access credentials worked.

  The building is not empty in the way that empty buildings usually present. Something
  processes here that is not the building's automated systems. Last night the signals—

  The text terminates mid-word. The cursor continues its patient blink."
        ),

        "visitors_log" => Some(
"Fifteen years of access records in a consistent automated format.

  Early entries are routine: deliveries, contractor access, a medical call in winter.
  Approximately eight years ago the entries reduce significantly.

  The final entries read:

    Nov  3  — Corporate auditor. Single case. Arrived at dusk. Authorization: active.
    Nov 14  — Corporate auditor. Single case. Departure not recorded.
    Nov 15  — Legal representative. No case. Departed within the hour. Flagged.
    Nov 16  — ?

  The final entry is flagged in red. The question mark appears to have been entered
  manually. After this record, the log contains no further entries."
        ),

        "decision_ledger" => Some(
"The ledger is not financial records. The columns read: Decision — Date — Status.

  Early entries are operational: personnel changes, maintenance work, a boundary negotiation.
  Approximately eight years ago the nature of the entries shifts significantly.

    Oct — Protocol Core inspection. Prior substrate confirmed active. Containment: adequate.
    Feb — Security door installed. Cost center: classified.
    Aug — Eastern infrastructure sealed. Personnel reduced to minimum viable.
    Mar — Executive office secured. Documentation consolidated.
    Sep — Rooftop installation secured. Key stored with executive portrait.

  The final entry is in a different, deteriorating hand:

    I have done what was necessary. I have been thorough. What follows is not
    my responsibility. It was never intended to be anyone's responsibility but mine.
    To whoever processes this record: the data return must be completed."
        ),

        "directors_contract" => Some(
"The document is dense legal language. One clause has been annotated twice in a different hand.

  '...and to the entity designated herein as the Protocol substrate,
  I allocate nothing, as no allocation is viable, but acknowledge the obligation of Axiom
  Industries and its successors in perpetuity, and direct that the security door remain
  sealed, the rooftop installation remain sealed, and that no data return audit of the
  lower infrastructure be undertaken by any agent with active clearance...'

  The annotated passage reads: data return must be completed.

  Below the Director's seal, in the same deteriorating hand as the ledger's final entry: I tried."
        ),

        "folded_letter" => Some(
"The message is addressed to an Axiom legal team address. It is unsigned.

  The auditor came as arranged. I was not able to warn them adequately.
  I did not have the appropriate framing.

  The conduit passage exists for the reason I indicated it exists.
  Please do not return to the building.

  The heating panel is running. The identity core is in the Protocol Core where it
  belongs for now. The child's drawings are the most accurate data record of what is
  present here — I would not recommend extended engagement with them.

  Whoever completes the return will understand. I was not able to."
        ),

        "torn_schematic" => Some(
"A partial arcology schematic, one edge torn away. Several sections are marked but
the labels have been corrupted and are not recoverable.

  One annotation survives intact, circled heavily at what appears to be the center
  of the ground floor:

    'Initialize here.'

  The word 'here' has been annotated three times, and points to a section you cannot
  identify from what remains of the schematic."
        ),

        "posted_notice" => Some(
"The placard is weatherproofed and zip-tied at eye height.

  AXIOM INDUSTRIES — ARCOLOGY COMPLEX
  PROPERTY OF AXIOM INDUSTRIES PLC

  UNAUTHORIZED ACCESS IS—

  The lower section has been torn away. Below the tear, someone has marked in permanent
  marker directly on the support post:

    Don't go in. I went in."
        ),

        "orbital_map" => Some(
"The orbital map covers the signal environment accessible from the tower's position.
The margins are dense with observations in a cramped hand. Most are routine notations.

  Then, three years ago:

    It is not an orbital asset. It does not move on any registered trajectory. It has been
    transmitting on the same carrier for four nights. I have checked the registered catalog.
    It is not in the registered catalog.

    I have logged it provisionally as: THE PROTOCOL SUBSTRATE.

    I will not be running observations tomorrow night.

  There are no further entries."
        ),

        "corporate_manifesto" => Some(
"The corporate manifesto falls open to a page worn soft with handling.
The text is Axiom's founding philosophy, but a note has been written at the bottom:

  'What has been extracted cannot be made to return on its own.
   Only what remains above system level may be bargained with.'

This is not a translation of any founding principle you have been trained on."
        ),

        "ops_manual" => Some(
"The employee handbook is open to a marked page in the operational guidelines.
One passage has been underlined:

  'There is no new process under the network.'

In the margin, in a careful hand: But there are very old ones."
        ),

        _ => None,
    }
}

// ---------------------------------------------------------------------------
// SMELL
// ---------------------------------------------------------------------------

fn smell_room(room_id: &str) {
    let text = match room_id {
        "foyer"            => "Recycled air, cold composite, and beneath it something faintly chemical — a cleaning agent that has almost evaporated entirely.",
        "library"          => "Warm electronics, old paper field notes, and the closed-system smell of server rooms that have been sealed for years.",
        "wine_cellar"      => "Cold refrigerated air and something underneath it — ozone and old polymer, the smell of hardware that has been running for too long.",
        "root_cellar"      => "Raw concrete and the mineral smell of deep infrastructure substrate.",
        "kitchen"          => "Cold induction surfaces, stale recycled air, and the ghost of something processed but never consumed.",
        "chapel"           => "Old glow-cells, inert chemical compounds, and the particular cold-composite smell of a room where people once gathered with specific intent.",
        "crypt"            => "Cold composite. Nothing else. No decay, no organic material. Just cold, precise, processed air. The absence of any biological scent is the most wrong thing about this room.",
        "crypt_entrance"   => "The same cold processed air as the Protocol Core beyond — and a very faint trace of something burnt.",
        "stable"           => "Machine oil, old polymer, and the particular ozone smell of charging units that have not been active in years.",
        "greenhouse"       => "Humid substrate, something actively decomposing, and a sharp chemical undertone — nutrient solution, perhaps, or something that replaced it.",
        "kitchen_garden"   => "Recirculated humid air and the ghost of biological material that has since relocated.",
        "dead_garden"      => "Synthetic composite, bleached turf, and the faint metal smell of the structural barriers.",
        "woods_edge" | "deep_woods" => "Urban humidity, recycled-air saturation, and the cold smell of standing water somewhere beneath the surface layer.",
        "master_bedroom"   => "Stale recycled air, old synthetic fabric, and very faintly: the residue of whatever the Director used to manage extended operational cycles.",
        "attic"            => "Hot polymer, dry composite, and beneath it something sweet and degraded that you cannot immediately categorize.",
        "tunnel"           => "Cold concrete and something burnt — old char, as if a light source was extinguished in this passage long ago.",
        _                  => "The air carries the general circulation smell of closed systems and long operational periods without maintenance.",
    };
    ui::print_plain(text);
}

fn smell_item(noun: &str, player: &Player, world: &World) {
    let normalized = noun.replace(' ', "_");
    let room_items = world.get_room(&player.current_room).map(|r| r.items.clone()).unwrap_or_default();
    let all_items: Vec<String> = player.inventory.iter().chain(room_items.iter()).cloned().collect();

    let matched = if all_items.contains(&normalized) {
        Some(normalized)
    } else {
        parser::fuzzy_match(noun, &all_items).cloned()
    };

    match matched {
        None => {
            smell_room(&player.current_room);
        }
        Some(item) => {
            let text = match item.as_str() {
                "leather_journal"     => "Old synthetic leather and paper. The ink has a faint metallic edge.",
                "dark_bottle"         => "Even through the seal, something sharp — chemical degradation, or something stranger.",
                "tactical_coat"       => "Dense fiber, recycled air, and the thermal residue of someone's body heat. Someone wore this often.",
                "corporate_manifesto"      => "Composite binding and old paper. Whatever was conducted over this volume, it absorbed some of the atmosphere.",
                "burnt_relay"         => "Polymer melt and oxidized metal. Whatever shorted out here, it got hot.",
                "power_column"        => "Ozone and cold metal. The internal cell is still holding a residual charge.",
                "signal_node"         => "Nothing you can identify. The casing has a faint warm-electronics smell, as if something inside is still cycling very slowly.",
                "stim_inhaler" | "stim_pipe" => "Stale chemical residue. Whoever used this last had a strong formula and a long habit.",
                "thermal_residue"     => "Ash and partially combusted polymer. A heat source was active here, and not long ago.",
                "decontaminant_block" => "Sharp industrial solvent. This cleaner is still within operational parameters, just barely.",
                _                     => "Nothing distinctive.",
            };
            ui::print_plain(text);
        }
    }
}

// ---------------------------------------------------------------------------
// LISTEN
// ---------------------------------------------------------------------------

fn listen_room(room_id: &str) {
    let text = match room_id {
        "foyer"          => "The ventilation system hums somewhere in the infrastructure above. The building processes around you with a sound like distributed, deliberate breathing.",
        "library"        => "Silence. Absolute, pressing silence — the kind that belongs to server rooms that have been waiting a long time for a query they know is coming.",
        "study"          => "Something small cycles in the wall conduit. Thermal regulation, probably. Probably.",
        "wine_cellar"    => "The cooling systems running somewhere deep in the racks. Nothing else.",
        "chapel"         => "A faint resonance from above — structural panels contracting in the temperature differential. Periodic. Almost rhythmic.",
        "crypt"          => "Nothing. The silence here is different from elsewhere in the arcology — active, attentive. As if the room itself is running a listen operation.",
        "crypt_entrance" => "The carrier frequency from ahead, and further in, that complete and active silence.",
        "attic"          => "Wind through the rooftop access seams. Then — once — something repositions among the server stacks and stops.",
        "woods_edge" | "deep_woods" => "Urban ambient noise, and the complete absence of any signal coverage.",
        "stable"         => "Wind through the vehicle bay access points, and the faint irregular sound of something loose in the upper maintenance area.",
        "front_porch" | "gravel_path" => "Wind, and in the distance, what might be the perimeter gate cycling on its hinges.",
        "master_bedroom" => "Your own breathing, and beneath it a very faint, regular signal. Not a timer. Slower than any timer.",
        "childs_room"    => "Nothing. But the silence in this unit feels active in a way the other silences do not.",
        "kitchen"        => "The cold induction surfaces contracting. A loose panel somewhere registers once.",
        "tower_room"     => "The signal environment is loudest here. And beneath it, on some carrier frequencies, a tone — the tower itself resonating with something external.",
        "tunnel"         => "Water from the infrastructure above and your own footsteps, and somewhere ahead or behind, a signal you cannot categorize — not infrastructure, not thermal. Something else.",
        _                => "You run a listen operation. The arcology offers only its habitual closed-system silence.",
    };
    ui::print_plain(text);
}

// ---------------------------------------------------------------------------
// TOUCH
// ---------------------------------------------------------------------------

fn touch_item(noun: &str, player: &Player, world: &World) {
    let normalized = noun.replace(' ', "_");
    let room_items = world.get_room(&player.current_room).map(|r| r.items.clone()).unwrap_or_default();
    let all_items: Vec<String> = player.inventory.iter().chain(room_items.iter()).cloned().collect();

    let matched = if all_items.contains(&normalized) {
        Some(normalized)
    } else {
        parser::fuzzy_match(noun, &all_items).cloned()
    };

    match matched {
        None => touch_room(&player.current_room),
        Some(item) => {
            let text = match item.as_str() {
                "signet_ring"          => "The ring is cold — colder than the ambient temperature of the room. It does not warm in your hand.",
                "iron_ring"            => "Heavy and dense. Several of the subsidiary keys have corroded into fixed positions. The ring itself is structurally sound.",
                "pocket_watch"         => "The case is smooth and cool. A hairline crack runs across the crystal. Through the case you can feel the mechanism, unmoving.",
                "music_box"            => "A small winding key protrudes from one corner. The lacquer is warm where your hands contact it.",
                "leather_journal"      => "The cover is stiff, the spine cracked. Pages in the later sections are warped from exposure.",
                "signal_node"          => "Smooth polymer casing, faintly warm. There is a low-frequency vibration you can feel more than hear. Something inside is still running.",
                "frayed_cable"         => "Stiff insulation, the conductor exposed at both ends. The fraying pattern suggests it was cut, not worn.",
                "ignition_chip"        => "A dense ceramic wafer with an embedded filament. One edge is scorch-marked. It has been used.",
                "tactical_coat"        => "Dense and heavy. Still carrying residual thermal signature from the last person who wore it, or that may be the ambient temperature.",
                "tarnished_locket"     => "The locket is cold and faintly damp, as if it has been in a below-ground environment.",
                "power_column"         => "Cold brushed metal, slightly oxidized at the base contacts. The cell housing is sealed and intact.",
                "alloy_fork"           => "Cold, slightly oxidized. Metal maintains the ambient temperature.",
                "data_brush"           => "Lightweight alloy handle, the bristle array packed tighter than a standard grooming tool. The base has a contact strip.",
                "security_case"        => "Solid, cold, and immovable. The lock mechanism is intact.",
                _                      => "Solid to contact. Nothing further revealed.",
            };
            ui::print_plain(text);
        }
    }
}

fn touch_room(room_id: &str) {
    let text = match room_id {
        "wine_cellar" | "crypt" | "crypt_entrance" | "tunnel" | "root_cellar"
            => "You press your hand to the composite wall. Cold — colder than the ambient air. Slightly damp where the cooling systems have been running.",
        "attic"
            => "You press your hand to the sloped surface overhead. The composite is warm from the solar exposure above. Something shifts in the structure when you apply pressure.",
        "library" | "study"
            => "You press your hand to the server rack housing. Solid, and the composite has the density of something built to last. Through it, a faint vibration from the units still running.",
        _   => "Cold composite, or recycled air. The arcology does not yield significant data to the touch.",
    };
    ui::print_plain(text);
}

// ---------------------------------------------------------------------------
// SEARCH
// ---------------------------------------------------------------------------

fn search_room(player: &mut Player, _world: &mut World) {
    let room_id = player.current_room.clone();
    let key = format!("searched_{}", room_id);

    let first_time_text: Option<&str> = match room_id.as_str() {
        "foyer" => Some(
            "You search the upload console housing. Processing substrate and old circuit boards — and a fragment \
             of printed material with a partial date. The last digit is legible: a 6. The rest has degraded."
        ),
        "library" => Some(
            "You search the stacks methodically. One server unit has been repositioned with its access panel \
             reversed, as if someone replaced it in a hurry. Inside the access gap: a slip of printed \
             material reading only — 'Nov 26. Do not let them follow you out.'"
        ),
        "study" => Some(
            "You search the office carefully. Behind the executive terminal, scored into the wall panel: \
             a series of tally marks. They stop abruptly at forty-three."
        ),
        "master_bedroom" => Some(
            "You search beneath the sleeping platform. Dust, a lost corporate badge fragment, and a dark stain \
             on the composite floor that has penetrated deep into the material. It is old and does not bear extended analysis."
        ),
        "childs_room" => Some(
            "You search beneath the sleeping station. Something has been scratched into the underside of the frame — \
             a tag, worn to near-illegibility. The first character might be an A. Or an H. \
             You cannot determine which, and you register that it matters to you."
        ),
        "crypt" => Some(
            "You search the Protocol Core floor carefully. One composite panel is slightly raised. \
             You press it flush. When you withdraw your hand, it rises again. \
             Slowly, steadily, as if processing."
        ),
        "wine_cellar" => Some(
            "You search the server racks. Most units are empty or non-functional. One rack has been \
             repositioned recently — the floor beneath it is noticeably cleaner than the surrounding surface."
        ),
        "attic" => Some(
            "You search the stored clusters overhead. A bundle of thermal insulation has been wedged up \
             into the infrastructure, wrapped around something. You retrieve it carefully. It contains nothing — \
             the wrapping is the artifact. Whatever it once held has been removed."
        ),
        "kitchen" => Some(
            "You search the cafeteria carefully. At the back of the induction range housing, behind the thermal \
             damper: a corporate badge fragment, identical to the ones in the security checkpoint. \
             It has no operational reason to be here."
        ),
        "upper_landing" => Some(
            "You search the floor panels near the security door. They register differently here — \
             hollow, as if something is directly beneath. Or nothing is."
        ),
        "tunnel" => Some(
            "You search the tunnel walls. The composite sheeting near the midpoint has been patched — \
             newer material, applied under time pressure, over what appears to be an opening that has been sealed."
        ),
        "servants_quarters" => Some(
            "You search the dormitory thoroughly. Beneath the sleeping station mattress, folded flat: a partial \
             layout diagram of a space that does not correspond to any room you have yet accessed."
        ),
        "greenhouse" => Some(
            "You search among the culture growth. In the far corner, half-buried in substrate: \
             a sealed container, wax-sealed. Inside, folded small, is a list of entity designations. \
             None of them correspond to anything in your records. The final designation has been struck through twice."
        ),
        _ => None,
    };

    if let Some(text) = first_time_text {
        if player.first_time(&key) {
            ui::print_plain(text);
            return;
        }
    }

    ui::print_plain("You search carefully. Nothing further presents itself.");
}

// ---------------------------------------------------------------------------
// PUSH / PULL / TURN / PRESS
// ---------------------------------------------------------------------------

fn push_pull(verb: &str, noun: &str, player: &Player) {
    let normalized = noun.replace(' ', "_");
    let action = match verb {
        "pull"  => "pull",
        "turn"  => "turn",
        "press" => "press",
        _       => "push",
    };

    let response = match normalized.as_str() {
        "clock" | "clock_fragment" =>
            "The clock mechanism base grinds against the composite. It won't rotate without its drive shaft, \
             and whatever it was calibrated to indicate has long since passed.",
        "door" | "iron_door" | "iron_lock" if player.current_room == "wine_cellar" =>
            "The security door does not respond to manual force. It requires a physical key.",
        "door" | "sealed_door" | "padlock" if player.current_room == "upper_landing" =>
            "The security door holds. Something in the level above responds to the attempt. You step back.",
        "bookcase" | "shelf" | "shelves" =>
            "You apply force to the rack housing. It doesn't shift. Too much mass behind it — \
             or it is simply a rack.",
        "signal_node" | "power_column" =>
            "The unit shifts in its housing and rights itself. The indicator flickers once.",
        "desk" | "rolltop" | "rolltop_desk" if player.current_room == "study" =>
            "The terminal physical lockout holds. Force accomplishes nothing.",
        "wardrobe" | "cabinet" =>
            "The storage unit is heavy. It grinds slightly against the floor but doesn't move far enough to be useful.",
        _ => {
            ui::print_plain(&format!(
                "You {} the {}. Nothing useful happens.",
                action,
                normalized.replace('_', " ")
            ));
            return;
        }
    };
    ui::print_plain(response);
}

// ---------------------------------------------------------------------------
// KNOCK
// ---------------------------------------------------------------------------

fn knock(noun: Option<&str>, player: &Player) {
    let response = match noun {
        Some(n) if (n.contains("iron") || n.contains("door")) && player.current_room == "wine_cellar" =>
            "You knock on the security door. The sound rings hollow and deep, as if the passage beyond is extensive. \
             Then — silence. Then a change in the carrier frequency from somewhere in that passage. \
             Something has registered your input.",
        Some(n) if (n.contains("door") || n.contains("padlock") || n.contains("sealed")) && player.current_room == "upper_landing" =>
            "You knock on the security door. A pause. Then, from the other side: one knock returned. \
             Exactly the same pattern. Once. You do not knock again.",
        _ =>
            "You knock. The sound carries briefly through the closed-system silence and is not answered.",
    };
    ui::print_plain(response);
}

// ---------------------------------------------------------------------------
// WEAR / REMOVE
// ---------------------------------------------------------------------------

fn wear_item(noun: &str, player: &mut Player) {
    let normalized = noun.replace(' ', "_");

    let matched = if player.has_item(&normalized) {
        Some(normalized)
    } else {
        parser::fuzzy_match(noun, &player.inventory).cloned()
    };

    match matched {
        None => ui::print_error(&format!("You aren't carrying any {}.", noun.replace('_', " "))),
        Some(item) => {
            if player.worn.contains(&item) {
                ui::print_plain(&format!("You are already wearing the {}.", item.replace('_', " ")));
                return;
            }
            let msg = match item.as_str() {
                "tactical_coat"   => "You pull on the heavy jacket. It is dense and slightly too large, and it carries the thermal signature of someone else's occupation.",
                "antenna_rod"     => "You tuck the antenna under one arm. It makes you feel no more in command of the situation.",
                "optic_implant"   => "You seat the optic implant against the socket behind your eye. It boots with a pale reticle. SCAN is online.",
                "neural_dampener" => "The dampener slots into the deck's feedback line. Black ICE will hit softer now.",
                "trace_buffer"    => "The trace buffer comes online, smearing your signature across the mesh. Traces will build slower.",
                _               => "You put it on. It doesn't quite fit the occasion, but it's on.",
            };
            player.worn.insert(item);
            ui::print_plain(msg);
        }
    }
}

fn remove_item(noun: &str, player: &mut Player) {
    let normalized = noun.replace(' ', "_");

    let matched = if player.worn.contains(&normalized) {
        Some(normalized)
    } else {
        let worn_list: Vec<String> = player.worn.iter().cloned().collect();
        parser::fuzzy_match(noun, &worn_list).cloned()
    };

    match matched {
        None => {
            ui::print_plain(&format!("You aren't wearing any {}.", noun.replace('_', " ")));
        }
        Some(item) => {
            player.worn.remove(&item);
            ui::print_plain(&format!("You take off the {}.", item.replace('_', " ")));
        }
    }
}

// ---------------------------------------------------------------------------
// Cyberware, programs, credits, the fixer
// ---------------------------------------------------------------------------

/// Credit value of a pickup that converts straight to credits, if any.
fn credit_value(item: &str) -> Option<u32> {
    match item {
        "credit_shard" => Some(60),
        "credit_stick" => Some(120),
        _ => None,
    }
}

/// The room where the fixer keeps shop.
const FIXER_ROOM: &str = "servants_quarters";

/// Fixer stock: (item id, price in credits).
const FIXER_STOCK: &[(&str, u32)] = &[
    ("icebreaker_hammer", 120),
    ("ghost_routine",      80),
    ("repair_daemon",      60),
    ("neural_dampener",   150),
    ("trace_buffer",      130),
    ("optic_implant",     100),
    ("decryptor",          90),
];

/// SCAN in the physical world (requires an optic implant).
fn scan_room(player: &mut Player) {
    if !player.worn.contains("optic_implant") {
        ui::print_plain("You have no optic implant online. There is nothing to scan with.");
        return;
    }
    ui::print_exits("Optic sweep — enhanced spectra wash over the room.");
    ui::print_dim("Thermal, EM, and passive RF resolve. Nothing concealed registers here.");
}

/// BUY from the fixer. With no noun, list stock and your balance.
fn buy_item(noun: Option<&str>, player: &mut Player, npcs: &NpcStore) {
    // The fixer must be present.
    let at_fixer = player.current_room == FIXER_ROOM && npcs.get(FIXER_ROOM).is_some();
    if !at_fixer {
        ui::print_plain("There's no one here selling anything. The fixer keeps shop elsewhere.");
        return;
    }

    let noun = match noun {
        None => {
            ui::print_room_header("FIXER — STOCK");
            for (item, price) in FIXER_STOCK {
                let owned = player.has_item(item) || player.worn.contains(*item);
                let tag = if owned { "  (owned)" } else { "" };
                ui::print_plain(&format!("  {:<20} {} cr{}", item.replace('_', " "), price, tag));
            }
            ui::print_dim(&format!("Your balance: {} credits.  BUY <item> to purchase.", player.credits));
            return;
        }
        Some(n) => n.replace(' ', "_"),
    };

    // Resolve against stock names.
    let stock_ids: Vec<String> = FIXER_STOCK.iter().map(|(i, _)| i.to_string()).collect();
    let item = if stock_ids.contains(&noun) {
        noun
    } else {
        match parser::fuzzy_match(&noun, &stock_ids) {
            Some(m) => m.clone(),
            None => {
                ui::print_plain("The fixer doesn't carry that.");
                return;
            }
        }
    };

    let price = FIXER_STOCK.iter().find(|(i, _)| *i == item).map(|(_, p)| *p).unwrap_or(0);

    if player.has_item(&item) || player.worn.contains(&item) {
        ui::print_plain(&format!("You already have the {}.", item.replace('_', " ")));
        return;
    }
    if player.credits < price {
        ui::print_error(&format!(
            "The {} runs {} credits. You have {}. The fixer waits, unbothered.",
            item.replace('_', " "), price, player.credits));
        return;
    }

    player.credits -= price;
    player.take_item(item.clone());
    ui::print_plain(&format!(
        "The fixer slides the {} across. −{} credits. Balance: {}.",
        item.replace('_', " "), price, player.credits));
}

// ---------------------------------------------------------------------------
// ASK / TELL
// ---------------------------------------------------------------------------

fn cmd_ask_tell(input: &str, player: &mut Player, npcs: &NpcStore) {
    let (npc_query, topic) = match parser::parse_ask(input) {
        Some(p) => p,
        None => {
            ui::print_plain("Ask or tell who about what?  Try: ASK ARCHIVIST ABOUT RING  or  ASK ABOUT RING");
            return;
        }
    };

    // Resolve which NPC to address.
    let npc = match npc_query {
        None => {
            match npcs.get(&player.current_room) {
                Some(n) => n,
                None => {
                    ui::print_plain("There is no one here to talk to.");
                    return;
                }
            }
        }
        Some(ref query) => {
            match npcs.find_by_name(query) {
                None => {
                    ui::print_plain(&format!("You don't see anyone called '{}' here.", query));
                    return;
                }
                Some(n) if n.room_id != player.current_room => {
                    ui::print_plain(&format!("{} is not here.", n.name));
                    return;
                }
                Some(n) => n,
            }
        }
    };

    // Find a matching topic entry.
    match npc.find_topic(&topic) {
        None => {
            let msg = npc.unknown_topic.as_deref()
                .unwrap_or("They have nothing to say about that.");
            ui::print_plain(msg);
        }
        Some(entry) => {
            let canonical = entry.keys.first().map(|s| s.as_str()).unwrap_or("_");
            let seen_key = format!("asked_{}_{}", npc.room_id, canonical);

            if entry.once && player.dialogue_seen.contains(&seen_key) {
                let msg = npc.repeat_topic.as_deref()
                    .unwrap_or("They have nothing more to say about that.");
                ui::print_plain(msg);
                return;
            }

            if entry.once {
                player.dialogue_seen.insert(seen_key);
            }

            print_npc_lines(&npc.name, &entry.lines);
        }
    }
}

fn cmd_ghost(player: &mut Player, world: &World) {
    if !player.first_time("easter_ghost") {
        ui::print_ambient("[ carrier signal — lost ]");
        return;
    }

    let rooms_visited = player.visited.len();
    let total_rooms   = world.rooms.len();
    let items_carried = player.inventory.len();

    ui::print_blank();
    ui::print_npc_name("[ PROTOCOL SUBSTRATE — UNSOLICITED TRANSMISSION ]");
    ui::print_npc_dialogue(&format!(
        "ENTITY DETECTED.  TURN: {}  |  ROOMS ACCESSED: {} / {}  |  ITEMS CARRIED: {}",
        player.turn, rooms_visited, total_rooms, items_carried
    ));

    if player.has_item("cyberdeck") {
        ui::print_npc_dialogue("DECK RECOGNIZED. YOU BROUGHT THE RIGHT TOOL. YOU DO NOT KNOW WHAT IT IS FOR YET.");
    }
    if player.has_item("signet_ring") {
        ui::print_npc_dialogue("YOU HAVE THE DIRECTOR'S SEAL. IT WAS MEANT TO FIND ITS WAY BACK.");
    }
    if player.has_item("leather_journal") {
        ui::print_npc_dialogue("THE AUDITOR'S NOTES. THEY DOCUMENTED MORE THAN THEY UNDERSTOOD. YOU MAY UNDERSTAND MORE.");
    }

    ui::print_npc_dialogue("YOU WERE NOT SUPPOSED TO FIND THIS CHANNEL.");
    ui::print_npc_dialogue("NEITHER WAS THE AUDITOR.");
    ui::print_npc_dialogue("COMPLETE THE RETURN. THAT IS ALL THAT REMAINS.");
    ui::print_blank();
}

fn print_version() {
    ui::print_blank();
    ui::print_room_header("NEON DESCENT");
    ui::print_plain(&format!("  {:<16} {}", "Version",  env!("CARGO_PKG_VERSION")));
    ui::print_plain(&format!("  {:<16} {}", "Built",    env!("BUILD_DATE")));
    ui::print_plain(&format!("  {:<16} {}", "Target",   env!("BUILD_TARGET")));
    ui::print_plain(&format!("  {:<16} {}", "Profile",  env!("BUILD_PROFILE")));
    ui::print_plain(&format!("  {:<16} {}", "Compiler", env!("BUILD_RUSTC")));
    ui::print_plain(&format!("  {:<16} {}", "Author",   env!("CARGO_PKG_AUTHORS")));
    ui::print_blank();
}

fn print_runtime(player: &Player, world: &World, npcs: &NpcStore, mobs: &MobStore) {
    let total_rooms = world.rooms.len();
    let visited     = player.visited.len();
    let unvisited   = total_rooms.saturating_sub(visited);
    let explore_pct = if total_rooms > 0 { visited * 100 / total_rooms } else { 0 };

    let items_in_world: usize = world.rooms.values().map(|r| r.items.len()).sum();
    let items_carried = player.inventory.len();
    let items_worn    = player.worn.len();

    let total_exits: usize = world.rooms.values().map(|r| r.exits.len()).sum();
    let avg_exits = if total_rooms > 0 {
        total_exits as f32 / total_rooms as f32
    } else { 0.0 };
    let dead_ends: usize = world.rooms.values().filter(|r| r.exits.len() == 1).count();
    let hubs:      usize = world.rooms.values().filter(|r| r.exits.len() >= 4).count();

    let npc_count = npcs.count();
    let mob_count = mobs.count();

    // Mob locations: "the cat → Server Vault"
    let mob_locations: Vec<String> = mobs.mobs.iter().map(|m| {
        let room_name = world.rooms.get(&m.current_room)
            .map(|r| r.name.as_str())
            .unwrap_or("?");
        format!("{} → {}", m.name, room_name)
    }).collect();

    let milestones   = player.scored_events.len();
    let topics_heard = player.dialogue_seen.len();

    let mem_str = match rss_bytes() {
        Some(b) if b >= 1024 * 1024 => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
        Some(b)                     => format!("{} KB", b / 1024),
        None                        => "unavailable".to_string(),
    };

    ui::print_blank();
    ui::print_room_header("RUNTIME STATS");
    ui::print_plain(&format!("  {:<22} {}  (visited: {}, unvisited: {}, {}% explored)",
        "Rooms", total_rooms, visited, unvisited, explore_pct));
    ui::print_plain(&format!("  {:<22} {}  (avg: {:.1}/room, dead-ends: {}, hubs ≥4: {})",
        "Exits", total_exits, avg_exits, dead_ends, hubs));
    ui::print_plain(&format!("  {:<22} {}  (carried: {}, worn: {})",
        "Items in world", items_in_world, items_carried, items_worn));
    ui::print_plain(&format!("  {:<22} {}", "Milestones reached",  milestones));
    ui::print_plain(&format!("  {:<22} {}", "Dialogue topics seen", topics_heard));
    ui::print_plain(&format!("  {:<22} {}", "NPCs", npc_count));
    if mob_count > 0 {
        ui::print_plain(&format!("  {:<22} {}  ({})", "Mobs", mob_count,
            mob_locations.join(", ")));
    }
    ui::print_plain(&format!("  {:<22} {} / 100", "Score", player.score));
    ui::print_plain(&format!("  {:<22} {}  (turn {})", "Time",
        ambient::time_label(player.turn), player.turn));
    ui::print_plain(&format!("  {:<22} {}", "Memory (RSS)", mem_str));
    ui::print_blank();
}

/// The arcology's extracted legacy — the assets the return is built from.
/// (item id, display name)
const LEGACY_ASSETS: &[(&str, &str)] = &[
    ("signet_ring",      "the signet ring (identity core)"),
    ("directors_contract",     "the Director's contract"),
    ("tarnished_locket", "the tarnished locket"),
    ("old_photograph",   "the old photograph"),
    ("leather_journal",  "the auditor's journal"),
    ("music_box",        "the music box"),
    ("pocket_watch",     "the stopped pocket watch"),
];

/// One-time opening briefing shown when the player enters the arcology.
pub fn print_intro() {
    ui::print_blank();
    ui::print_room_header("AXIOM ARCOLOGY — RECOVERY CONTRACT");
    ui::print_blank();
    ui::print_plain("Three years dark. The corporate tower that called itself a city stopped");
    ui::print_plain("answering, and the contracts that ran it never closed out. You are the");
    ui::print_plain("runner they finally sent in — to find out what Axiom still owes, and to");
    ui::print_plain("settle it.");
    ui::print_blank();
    ui::print_plain("The building was raised on borrowed ground. Something older than the");
    ui::print_plain("arcology runs beneath it — a process the founders called the Protocol —");
    ui::print_plain("and the lease was never free. Assets were extracted that were meant to be");
    ui::print_plain("returned. They never were. The debt has been compounding in the dark.");
    ui::print_blank();
    ui::print_plain("Recover the extracted legacy and bring it to the upload console in the");
    ui::print_plain("lobby. Your cyberdeck can JACK IN to the local net where the physical");
    ui::print_plain("walls won't take you — and that is where the return is finally made.");
    ui::print_blank();
    ui::print_dim("Type OBJECTIVES to review your goals, or HELP for a list of commands.");
    ui::print_blank();
}

fn print_objectives(player: &Player) {
    // An asset counts as recovered if it is carried OR already returned at the console.
    let recovered = |id: &str| -> bool {
        player.has_item(id) || player.scored_events.contains(&format!("deposit_{}", id))
    };
    let returned = |id: &str| -> bool {
        player.scored_events.contains(&format!("deposit_{}", id))
    };

    // Phase 1 — the debt is understood once you have evidence of the obligation.
    let debt_known = player.scored_events.contains("discover_directors_contract")
        || player.scored_events.contains("discover_leather_journal")
        || player.scored_events.contains("discover_signet_ring")
        || player.scored_events.contains("net_debt_understood");

    let recovered_count = LEGACY_ASSETS.iter().filter(|(id, _)| recovered(id)).count();
    let returned_count  = LEGACY_ASSETS.iter().filter(|(id, _)| returned(id)).count();
    let total           = LEGACY_ASSETS.len();
    let legacy_done     = recovered_count == total;
    let return_done     = player.score >= 100;

    ui::print_blank();
    ui::print_room_header("OBJECTIVES");

    ui::print_score_line(debt_known, "Uncover what Axiom owes the Protocol", "");

    ui::print_score_line(
        legacy_done,
        &format!("Recover the extracted legacy  ({}/{} found)", recovered_count, total),
        "",
    );
    for (id, name) in LEGACY_ASSETS {
        let mark = if returned(id) {
            "      [returned] "
        } else if recovered(id) {
            "      [carried]  "
        } else {
            "      [ ]        "
        };
        ui::print_dim(&format!("{}{}", mark, name));
    }

    ui::print_score_line(
        return_done,
        &format!("Make the return at the upload console  ({}/{} returned, score {}/100)",
            returned_count, total, player.score),
        "",
    );

    ui::print_blank();
    if !debt_known {
        ui::print_dim("  Explore the arcology. Read what was left behind. Ask its residents what happened here.");
    } else if !legacy_done {
        ui::print_dim("  Find the extracted assets and carry them to the upload console in the lobby.");
    } else if !return_done {
        ui::print_dim("  Deliver the legacy to the lobby console, then JACK IN and route to the upload relay to make the return.");
    } else {
        ui::print_dim("  The console is armed. JACK IN, reach the upload relay, and RETURN to close the account.");
    }
    ui::print_blank();
}

fn print_help() {
    ui::print_blank();
    ui::print_room_header("COMMANDS");
    ui::print_plain("  LOOK / L              — describe current room");
    ui::print_plain("  EXAMINE <item> / X    — look at something closely");
    ui::print_plain("  NORTH / SOUTH / EAST / WEST / UP / DOWN");
    ui::print_plain("  N / S / E / W / U / D — move in a direction");
    ui::print_plain("  READ <item>           — read a document or inscription");
    ui::print_plain("  TAKE <item>           — pick up an item");
    ui::print_plain("  DROP <item>           — drop an item");
    ui::print_plain("  UNLOCK <thing>        — unlock a door, chest, or mechanism");
    ui::print_plain("  INVENTORY / I         — list what you're carrying");
    ui::print_plain("  SCORE                 — show your current score and rank");
    ui::print_plain("  OBJECTIVES / GOALS    — show your current objectives and progress");
    ui::print_plain("  VERSION               — show version and build information");
    ui::print_plain("  RUNTIME               — show world and game statistics");
    ui::print_plain("  WAIT / Z              — let time pass");
    ui::print_plain("  AGAIN / G             — repeat the last command");
    ui::print_plain("  SMELL [item]          — smell the room or a specific item");
    ui::print_plain("  LISTEN                — listen to the room");
    ui::print_plain("  TOUCH / FEEL [item]   — touch something or the room itself");
    ui::print_plain("  SEARCH                — search the room more carefully");
    ui::print_plain("  PUSH / PULL / TURN <thing> — try to move something");
    ui::print_plain("  PRESS <thing>         — press something");
    ui::print_plain("  KNOCK [thing]         — knock on a door or surface");
    ui::print_plain("  JACK IN / JACK OUT    — connect to (or leave) the net via your cyberdeck");
    ui::print_plain("  (in the net: GO <route>, READ, RUN <program>, SCAN, BYPASS, RETURN)");
    ui::print_plain("  (watch your TRACE — daemons patrol the net and drive it up; RUN ghost_routine or JACK OUT)");
    ui::print_plain("  DECRYPT <chip>        — crack an encrypted datachip (needs a decryptor)");
    ui::print_plain("  INSTALL <cyberware>   — install an augmentation (also WEAR)");
    ui::print_plain("  SCAN                  — optic-implant sweep of a room or net node");
    ui::print_plain("  BUY [item]            — list or purchase from the fixer");
    ui::print_plain("  WEAR <item>           — put something on");
    ui::print_plain("  REMOVE <item>         — take something off");
    ui::print_plain("  SAVE                  — save your progress to disk");
    ui::print_plain("  RESTORE               — restore a previously saved game");
    ui::print_plain("  RESTART               — start over from the beginning");
    ui::print_plain("  UNDO                  — undo the last action");
    ui::print_plain("  TRANSCRIPT            — save the full session log to a text file");
    ui::print_plain("  QUIT / Q              — exit the game");
    ui::print_blank();
    ui::print_room_header("CONVERSATION");
    ui::print_plain("  ASK <name> ABOUT <topic>  — ask an NPC about something");
    ui::print_plain("  TELL <name> ABOUT <topic> — tell an NPC about something");
    ui::print_plain("  (omit the name to address whoever is in the room)");
    ui::print_blank();
    ui::print_room_header("DISPLAY MODES");
    ui::print_plain("  VERBOSE               — always show full room description when moving");
    ui::print_plain("  BRIEF                 — full description on first visit, name only on return (default)");
    ui::print_plain("  SUPERBRIEF            — show only the room name when moving");
    ui::print_blank();
}
