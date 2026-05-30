# Neon Descent — Architecture

## Overview

Neon Descent is a single-binary, single-threaded Rust text adventure set in Axiom Arcology, a decaying corporate arcology. It is a **windowed macroquad application** (GPU-rendered), not a terminal/TTY program — input is captured frame-by-frame from the OS keyboard event queue, and output is drawn to a scrollable canvas. Static world data (rooms, NPCs, mobs, map) is **embedded into the binary at compile time** via `include_str!`/`include_bytes!`. All mutable state lives in memory and can be snapshotted for UNDO or serialized to JSON for SAVE/RESTORE.

---

## Module Map

```
src/
├── main.rs        — Entry point, window config, macroquad frame loop
├── app.rs         — App state machine (Title/Playing/Won), frame dispatch,
│                    UNDO / AGAIN / TRANSCRIPT, tab-completion word lists
├── input.rs       — Frame-based keyboard handling, input buffer, history,
│                    cursor, tab completion
├── gfx.rs         — Rendering: status bar, output area, input bar; word-wrap
│                    and incremental render cache
├── ui.rs          — Styled text model (StyledLine/Segment/Style/TextColor),
│                    thread-local output buffer, print_* helpers
├── commands.rs    — All command handlers; puzzle logic; sensory/atmospheric text
├── parser.rs      — Input normalization, alias resolution, fuzzy matching
├── player.rs      — Player state: inventory, position, score, events
├── world.rs       — Room graph (static structure, mutable items/exits)
├── mobs.rs        — Wandering entities with probabilistic AI
├── npcs.rs        — Static dialogue NPCs with topic + item-triggered branches
├── net.rs         — Net node map + jacked-in command handler (cyberspace layer)
├── ambient.rs     — Time-of-day system and atmospheric event pools
├── save.rs        — Snapshot types, JSON save/load
└── title.rs       — Splash screen with scanline effect

(project root — all embedded at build time)
├── rooms.toml     — 37 room definitions
├── mobs.toml      — 6 mob definitions
├── npcs.toml      — 5 NPC definitions with dialogue trees
├── nodes.toml     — 7 net node definitions (the cyberspace layer)
└── map.txt        — ASCII room + net node map (shown by the MAP cheat)

build.rs           — Embeds build date, target triple, profile, rustc version
```

---

## Game Loop

`main.rs` runs `loop { app.frame().await; next_frame().await; }`. This is a **frame loop (~60fps)**, not a readline REPL. `App::frame()` dispatches on `AppState`:

- **`Title`** — draws the splash screen (throttled to ~30fps via an 18ms sleep in `main.rs`); transitions to `Playing` on Enter/Space. On transition it prints the one-time story briefing (`commands::print_intro`) then the opening room look.
- **`Playing`** — the main game: feed tab-completion lists, handle input, run a command on Enter, render.
- **`Won`** — reached when the net return completes; keeps rendering the final scroll.

A *turn* only advances when the player submits a command — most frames do nothing but redraw. Per submitted command (`App::process_command`):

```
1. Echo input into scroll_buf
2. Intercept TRANSCRIPT  → dump scroll_buf to text file, return
3. Intercept UNDO        → restore snapshot, return
4. Intercept AGAIN       → substitute last command
5. Snapshot state (enables UNDO)
6. Route by mode:
     • JACK / DISCONNECT verb → App::handle_jack (enter/leave the net)
     • jacked in + non-meta   → net::handle()
     • otherwise              → commands::handle()
        └─ at end: ambient.tick(), mobs.tick(), arm-console nudge (≥100)
7. drain ui buffer → scroll_buf
```

Both `commands::handle()` and `net::handle()` return an `Action`: `Continue`, `Quit` (win → `Won` state), `Restart` (new game), or `Exit` (terminate; the `quit` verb). The **win** (`Action::Quit`) now fires only from the net `RETURN` at the upload relay — reaching score 100 in the physical world merely arms the console and nudges the player to jack in.

---

## Rendering (`gfx.rs`, `ui.rs`)

Game logic never prints directly. It calls `ui::print_*` (e.g. `print_plain`, `print_room_header`, `print_items`, `print_error`) which push `StyledLine`s into a **thread-local buffer**. After each command `App` drains that buffer into the persistent `scroll_buf`.

`gfx.rs` renders three regions every frame: a status bar (room name, time, score, turn), a scrollable output area, and an input bar with a blinking cursor. Word-wrapping is cached incrementally — `wrap_new_lines()` only re-wraps newly appended lines unless the window was resized.

> **Note:** rendering must happen every frame. Skipping draw calls on "idle" frames does not work with macroquad's double buffering — the swapped-in back buffer shows blank and flickers. CPU is saved by throttling the title screen, not by skipping frames.

---

## Core Data Structures

### `Player` (`player.rs`)

```rust
struct Player {
    current_room: String,
    inventory: Vec<String>,
    worn: HashSet<String>,           // subset of inventory
    visited: HashSet<String>,        // rooms ever entered
    score: u32,
    turn: u32,
    scored_events: HashSet<String>,  // prevents double-counting awards
    dialogue_seen: HashSet<String>,  // NPC topics already shown
    verbose_mode: VerboseMode,       // Verbose / Brief / SuperBrief
}
```

Scoring is event-keyed: `player.first_time("discover_iron_ring")` returns `true` exactly once, and `player.award(event, pts)` only grants points the first time, so awards cannot be re-earned.

### `Room` / `World` (`world.rs`)

```rust
struct Room {
    id: String,
    name: String,
    description: String,
    exits: HashMap<String, String>,  // "north" → room_id
    items: Vec<String>,              // mutable
}
struct World { rooms: HashMap<String, Room> }
```

Names/descriptions are immutable after load. Exits and items are mutable — puzzles add exits, `take`/`drop` move items.

### `Mob` / `MobStore` (`mobs.rs`)

```rust
struct Mob {
    id, name: String,
    current_room: String,            // mutable
    wander_rooms: Vec<String>,
    move_chance: u64,                // 1-in-N per turn
    idle_chance: u64,                // 1-in-N for idle message
    presence: String,
    // enter / leave / idle message pools
}
```

Uses an xorshift64 RNG seeded from `SystemTime`. `range()` and `one_in()` are guarded against `n == 0`.

### `Npc` / `NpcStore` (`npcs.rs`)

NPCs are keyed by `room_id` for O(1) lookup on entry. Each has first-visit `lines`, `revisit_lines`, `item_responses`, and `topics` (for ASK/TELL). NPCs are cyberpunk-themed: *The Analyst, The Protocol, The Director's Partner, The Maintenance Tech, The Young Coder*.

### `Command` (`parser.rs`)

```rust
struct Command { verb: String, noun: Option<String> }  // noun: underscored, filler-stripped
```

### `GameSnapshot` (`save.rs`)

Captures only mutable state (player, room items/exits, mob locations). Room name/description are re-loaded from the embedded TOML. Used for both one-level in-memory UNDO and the JSON save file `neon_descent.sav`.

---

## Parser & Fuzzy Matching (`parser.rs`)

**Pipeline:** lowercase + tokenize → resolve verb alias (`n`→`north`, `x`/`l`→`look`, `get`→`take`, …) → strip filler words (`the`, `a`, `an`, `at`, `from`, `on`, `in`, `with`, …) → join remaining tokens with underscores → `noun`.

**`fuzzy_match()`** resolves item/direction strings in four escalating passes: exact (spaces ↔ underscores) → substring → word-level prefix (3+ chars) → Levenshtein `edit_distance()` (threshold scales with length).

---

## Tab Completion (`input.rs` ⇄ `app.rs`)

Each frame in `Playing`, `App::update_completions()` pushes two lists into `InputState` via `set_completions()`:
- **verbs** — static canonical command list.
- **nouns** — dynamic: items in the current room, player inventory, exit directions, and any NPC present (name + aliases).

`InputState::try_complete()` completes the word at the cursor; the first word is matched against verbs, later words against nouns. Repeated Tab cycles through all matches; any other keypress resets the cycle. While jacked in, the completion lists switch to net verbs and the current node's route labels.

---

## The Net (`net.rs`, `nodes.toml`)

A second, navigable layer reached with `JACK IN` (the player starts with a `cyberdeck`). `net.rs`
mirrors `world.rs`: a `NetNode { id, name, description, links, ice, unlock_flag, data }` graph loaded
from the embedded `nodes.toml` (7 nodes mapping to story beats — archive, perimeter log, executive
purge queue, vault ICE spine, the Protocol core, and the upload relay).

- **Entry/exit** is owned by `App::handle_jack` (it needs the `NetStore` plus physical world/mobs):
  `JACK IN` requires the cyberdeck and a room with signal (`net::NO_SIGNAL_ROOMS` denies far-exterior
  rooms); `JACK OUT`/`DISCONNECT` clears `player.net_node` and re-shows the physical room.
- **While jacked in**, `App::process_command` routes to `net::handle()` instead of `commands::handle()`,
  except for global meta verbs (`is_net_meta`: help/save/restore/quit/restart/objectives/score/
  version/runtime). Net verbs: `LOOK`, `GO <route>` (or a bare route label), `READ`, `BYPASS`, `RETURN`.
- **Gating** is flag-based and reuses `scored_events`: an `ice` node is sealed until its `unlock_flag`
  is set. `BYPASS` at the vault spine sets `net_ice_vault_cracked` if the player holds the `iron_ring`,
  opening the route to the Protocol core. Reading the archive sets `net_debt_understood` (advances OBJECTIVES).
- **State**: `player.net_node: Option<String>` (the current node, or `None` in the physical world) is
  the only new persisted field — added to `PlayerSnapshot` with `#[serde(default)]`. All other net
  progress lives in `scored_events`, which is already saved.
- **Visual cue**: while jacked in the status bar shows `[ NET // <node> ]` (cyan) and the input prompt
  becomes `NET>` (a `net_label`/`in_net` flag threaded into `gfx::render_status_bar` / `render_input_bar`).

## Story spine

The fiction (an ancient **Protocol** owed a return of extracted assets) is surfaced two ways:
the one-time **briefing** (`commands::print_intro`) shown on entering `Playing`, and the
**`OBJECTIVES`** command (`commands::print_objectives`), a 3-phase checklist computed live from
existing state — debt understood → legacy recovered (the deposit-set assets) → return made. The
climax ties both new features together: gather to score 100 (arms the console), then `JACK IN` and
`RETURN` at the upload relay to win.

---

## Scoring & Puzzles

The gather loop scores to **100** (which arms the upload console; the actual win is the net `RETURN`).
Awards are event-keyed (never doubled): exploration (first visit to notable rooms), discovery (picking
up key items), deposits (returning items to the lobby console), and three unlock puzzles:

| Puzzle  | Room            | Key item        | Points | Effect                          |
|---------|-----------------|-----------------|--------|---------------------------------|
| Iron door | `wine_cellar` | `iron_ring`     | 6      | Opens a sealed exit             |
| Desk    | `study`         | `cipher_key`    | 8      | Reveals `masters_will`          |
| Padlock | `upper_landing` | `biometric_chip`| 10     | Opens `up` exit to `attic`      |

---

## Atmospheric System (`ambient.rs`)

**Time of day** — 120-turn cycle, 8 periods of 15 turns: Dawn → Morning → Midday → Afternoon → Late Afternoon → Dusk → Evening → Night. `time_period(turn) = (turn + 60) % 120 / 15`. Surfaced in the status bar and LOOK flavor for rooms with exterior exposure.

**Ambient events** — `ambient.tick()` fires roughly 1-in-4 turns, drawing from room-specific or category message pools. Uses its own copy of the xorshift RNG (distinct seed from mobs.rs).

---

## Save / Restore (`save.rs`)

| Mechanism  | Storage              | Depth  | How                              |
|------------|----------------------|--------|----------------------------------|
| UNDO       | In-memory            | 1 step | Snapshot before every command    |
| SAVE       | `neon_descent.sav`   | Full   | `serde_json` pretty JSON         |
| RESTORE    | `neon_descent.sav`   | Full   | Deserialize + `look()` refresh   |
| TRANSCRIPT | `neon_descent_transcript.txt` | — | Plain-text dump of `scroll_buf` |

Save/transcript paths are relative to the working directory (≈ `~` when launched from Finder).
The snapshot captures `player.net_node`, so saving/restoring while jacked in preserves the net
location; all other net progress rides along in `scored_events`.

---

## Dependencies

| Crate        | Purpose                                          |
|--------------|--------------------------------------------------|
| `macroquad`  | Windowing, GPU rendering, input, fonts           |
| `image`      | PNG decode for the embedded title texture        |
| `serde`      | Derive macros for serialization                  |
| `serde_json` | JSON save file format                            |
| `toml`       | Parse the embedded room/mob/NPC/net-node data    |
| `libc`       | RSS memory reporting (`runtime` command)         |

---

## Build

```bash
cargo run                 # debug
cargo build --release     # target/release/neon_descent
./package_mac.sh          # "Neon Descent.app" bundle (strips quarantine)
```

`build.rs` injects `BUILD_DATE`, `BUILD_TARGET`, `BUILD_PROFILE`, `BUILD_RUSTC` env vars, shown by the in-game `VERSION` command.

---

## Scale

| Metric          | Value                       |
|-----------------|-----------------------------|
| Rooms           | 37                          |
| Net nodes       | 7                           |
| Mobs            | 6                           |
| NPCs            | 5                           |
| Max score       | 100 (arms the net return)   |
| Command history | 5                           |
| Save format     | JSON (`neon_descent.sav`)   |
