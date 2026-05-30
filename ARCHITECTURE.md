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
├── ambient.rs     — Time-of-day system and atmospheric event pools
├── save.rs        — Snapshot types, JSON save/load
└── title.rs       — Splash screen with scanline effect

(project root — all embedded at build time)
├── rooms.toml     — 37 room definitions
├── mobs.toml      — 6 mob definitions
├── npcs.toml      — 5 NPC definitions with dialogue trees
└── map.txt        — ASCII map for the in-game MAP command

build.rs           — Embeds build date, target triple, profile, rustc version
```

---

## Game Loop

`main.rs` runs `loop { app.frame().await; next_frame().await; }`. This is a **frame loop (~60fps)**, not a readline REPL. `App::frame()` dispatches on `AppState`:

- **`Title`** — draws the splash screen (throttled to ~30fps via an 18ms sleep in `main.rs`); transitions to `Playing` on Enter/Space.
- **`Playing`** — the main game: feed tab-completion lists, handle input, run a command on Enter, render.
- **`Won`** — reached when score ≥ 100; keeps rendering the final scroll.

A *turn* only advances when the player submits a command — most frames do nothing but redraw. Per submitted command (`App::process_command`):

```
1. Echo input into scroll_buf
2. Intercept TRANSCRIPT  → dump scroll_buf to text file, return
3. Intercept UNDO        → restore snapshot, return
4. Intercept AGAIN       → substitute last command
5. Snapshot state (enables UNDO)
6. commands::handle() → Action
   └─ at end of handle(): ambient.tick(), mobs.tick(), win check (≥100)
7. drain ui buffer → scroll_buf
```

`commands::handle()` returns an `Action`: `Continue`, `Quit` (win → `Won` state), `Restart` (new game), or `Exit` (terminate process; this is what the `quit` verb maps to).

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

`InputState::try_complete()` completes the word at the cursor; the first word is matched against verbs, later words against nouns. Repeated Tab cycles through all matches; any other keypress resets the cycle.

---

## Scoring & Puzzles

Win at **score ≥ 100** ("You are the Ghost in the Machine."). Awards are event-keyed (never doubled): exploration (first visit to notable rooms), discovery (picking up key items), deposits (returning items), and three unlock puzzles:

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

---

## Dependencies

| Crate        | Purpose                                          |
|--------------|--------------------------------------------------|
| `macroquad`  | Windowing, GPU rendering, input, fonts           |
| `image`      | PNG decode for the embedded title texture        |
| `serde`      | Derive macros for serialization                  |
| `serde_json` | JSON save file format                            |
| `toml`       | Parse the embedded room/mob/NPC data             |
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
| Mobs            | 6                           |
| NPCs            | 5                           |
| Max score       | 100                         |
| Command history | 5                           |
| Save format     | JSON (`neon_descent.sav`)   |
