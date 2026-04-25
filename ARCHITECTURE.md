# Dark Adventure — Architecture

## Overview

Dark Adventure is a single-binary, single-threaded Rust text adventure set in Darkwood Manor. It uses a command-driven game loop with `rustyline` for line editing and command history. Static world data is loaded from TOML files at startup; all mutable state lives in memory and can be snapshotted for UNDO or serialized to JSON for SAVE/RESTORE.

---

## Module Map

```
src/
├── main.rs        — Entry point, game loop, UNDO/AGAIN dispatch
├── commands.rs    — All command handlers; atmospheric/sensory text
├── parser.rs      — Input normalization, alias resolution, fuzzy matching
├── player.rs      — Player state: inventory, position, score, events
├── world.rs       — Room graph (static structure, mutable items/exits)
├── mobs.rs        — Wandering entities with probabilistic AI
├── npcs.rs        — Static dialogue NPCs with item-triggered branches
├── ambient.rs     — Time-of-day system and atmospheric event pools
├── save.rs        — Snapshot types, JSON save/load
└── title.rs       — Splash screen

data/
├── rooms.toml     — ~25 room definitions
├── mobs.toml      — Mob definitions (cat, raven, …)
└── npcs.toml      — NPC definitions with dialogue trees

build.rs           — Embeds build date, target triple, rustc version
```

---

## Core Data Structures

### `Player` (`player.rs`)

```rust
struct Player {
    current_room: String,
    inventory: Vec<String>,
    worn: HashSet<String>,          // subset of inventory
    visited: HashSet<String>,       // rooms ever entered
    score: u32,
    turn: u32,
    scored_events: HashSet<String>, // prevents double-counting awards
}
```

Scoring is event-keyed: `player.first_time("discover_signet_ring")` returns `true` exactly once, so awards cannot be re-earned.

---

### `Room` / `World` (`world.rs`)

```rust
struct Room {
    id: String,
    name: String,
    description: String,
    exits: HashMap<String, String>,  // "north" → room_id
    items: Vec<String>,              // mutable
}

struct World {
    rooms: HashMap<String, Room>,
}
```

Room definitions (name, description) are effectively immutable after load. Exits and items are mutable — puzzles add exits, `take`/`drop` move items.

---

### `Mob` / `MobStore` (`mobs.rs`)

```rust
struct Mob {
    id: String,
    name: String,
    current_room: String,           // mutable
    wander_rooms: Vec<String>,      // allowed movement pool
    move_chance: u64,               // 1-in-N per turn
    idle_chance: u64,               // 1-in-N for idle message
    presence: String,               // line shown in room description
    // enter / leave / idle message pools
}
```

Uses an xorshift64 RNG seeded from `SystemTime`. Movement is probabilistic; messages are picked randomly from pools.

---

### `Npc` / `NpcStore` (`npcs.rs`)

```rust
struct Npc {
    room_id: String,
    name: String,
    lines: Vec<String>,             // first-visit greeting
    revisit_lines: Vec<String>,
    item_responses: Vec<ItemResponse>,  // item → response lines
}
```

Keyed by `room_id` in a `HashMap` for O(1) lookup on room entry.

---

### `Command` (`parser.rs`)

```rust
struct Command {
    verb: String,
    noun: Option<String>,  // underscored, filler-stripped
}
```

---

### `GameSnapshot` (`save.rs`)

```rust
pub struct GameSnapshot {
    pub player: PlayerSnapshot,
    pub world: WorldSnapshot,   // items & exits only (structure is static)
    pub mobs: MobSnapshot,
}
```

Used for both in-memory UNDO (one level) and persistent JSON (`darkwood.sav`).

---

## Systems

### Game Loop (`main.rs`)

```
Initialize
  └─ Load rooms.toml → World
  └─ Load mobs.toml  → MobStore
  └─ Load npcs.toml  → NpcStore
  └─ Player::new() at "foyer"
  └─ Show title → initial LOOK

Per-turn (rustyline readline)
  1. Snapshot state          ← enables UNDO
  2. Handle UNDO             ← restore previous snapshot, skip rest
  3. Handle AGAIN            ← replay last non-UNDO/AGAIN input string
  4. parse() → Command
  5. handle() → Action       ← all game logic here
  6. ambient.tick()          ← ~25% chance: print atmospheric line
  7. mobs.tick()             ← probabilistic movement & messages
  8. Win check: score ≥ 100 → Quit
```

---

### Command Dispatch (`commands.rs`)

`handle()` routes on `Command.verb`:

| Category   | Verbs                                         |
|------------|-----------------------------------------------|
| Meta       | quit, restart, help, undo, again, version     |
| Persistence| save, restore                                 |
| Movement   | north/south/east/west/up/down, go             |
| Inventory  | take, drop, inventory, wear, remove           |
| Examination| look, examine, read, search, unlock           |
| Sensory    | smell, listen, touch                          |
| Interaction| push, pull, turn, press, knock                |
| Info       | score, runtime                                |

Every handler increments `player.turn` and returns `Action::Continue`, `::Quit`, or `::Restart`.

---

### Parser & Fuzzy Matching (`parser.rs`)

**Input pipeline:**
1. Lowercase, tokenize
2. Resolve verb alias: `n` → `north`, `x` / `l` → `look`, `get` → `take`, etc.
3. Strip filler words from noun tokens: `the`, `a`, `an`, `at`, `from`, `on`, `in`, `with`
4. Join remaining tokens with underscores → `noun`

**Fuzzy matching** (`fuzzy_match()`) — used to resolve item/direction strings:
1. Exact match (case-insensitive, spaces ↔ underscores)
2. Substring match
3. Word-level prefix match (3+ chars)
4. Levenshtein edit distance (threshold: 1 for ≤4 chars, 2 for 5–7, 3 for 8+)

---

### Scoring & Achievement System

Max score: **100 points**, never doubled (event-keyed).

| Category      | Points | Trigger                            |
|---------------|--------|------------------------------------|
| Exploration   | 25     | First visit to notable rooms       |
| Discovery     | 32     | Picking up key items               |
| Puzzles       | 24     | Solving the three unlock puzzles   |
| Deposits      | 19     | Returning items to the foyer       |

**Puzzles:**

| Puzzle       | Location      | Key Item       | Effect              |
|--------------|---------------|----------------|---------------------|
| Iron Door    | wine_cellar   | iron_ring      | Opens crypt exit    |
| Rolltop Desk | study         | brass_key      | Reveals masters_will|
| Padlock Door | upper_landing | portrait_key   | Opens attic exit    |

---

### Mob AI (`mobs.rs`)

Each mob per turn:
1. Roll 1-in-`move_chance` for movement
2. Pick random destination from `wander_rooms` (excluding current)
3. If entering the player's room → random `enter_message`
4. If leaving the player's room → random `leave_message`
5. If staying in the player's room → roll 1-in-`idle_chance` for random `idle_message`

---

### NPC Dialogue (`npcs.rs`, triggered from `commands.rs`)

On room entry:
1. **First visit** → print `npc.lines`
2. **Revisit, player holds trigger item** → print `item_response.lines` (once per item)
3. **Revisit, no trigger item** → print `revisit_lines` (once)

Keyed by `"npc_item_{room}_{item}"` and `"npc_revisit_{room}"` in `player.scored_events`.

---

### Atmospheric System (`ambient.rs`)

**Time of day** — 120-turn cycle, 8 periods:

```
Dawn → Morning → Midday → Afternoon → Late Afternoon → Dusk → Evening → Night
```

Affects flavor text shown with LOOK in outdoor and windowed rooms. Underground/sealed rooms have no time flavor.

**Ambient events** — ~25% chance per turn:
- Room-specific message pools for named locations
- Category fallback pools: outdoor, underground, upper floors

**Sensory commands** — hardcoded per location/item: `smell`, `listen`, `touch`.

---

### Save / Restore (`save.rs`)

| Mechanism | Storage       | Depth  | How                              |
|-----------|---------------|--------|----------------------------------|
| UNDO      | In-memory     | 1 step | Snapshot before every command    |
| SAVE      | `darkwood.sav`| Full   | `serde_json::to_string_pretty()` |
| RESTORE   | `darkwood.sav`| Full   | Deserialize + `look()` refresh   |

Only mutable state is saved: player fields, room items/exits, mob locations. Room definitions (name, description) are re-loaded from TOML on startup.

---

## Module Interaction Diagram

```
main()
 ├─ World::load()  ──►  rooms.toml
 ├─ MobStore::load() ─► mobs.toml
 ├─ NpcStore::load() ─► npcs.toml
 └─ game loop
      │
      ├─ parser::parse()
      │    └─ parser::fuzzy_match() / edit_distance()
      │
      ├─ commands::handle()
      │    ├─ world  (read exits, get/set items)
      │    ├─ player (move, award, inventory)
      │    ├─ mobs   (query presence)
      │    ├─ npcs   (query on room entry)
      │    └─ save   (snapshot / file I/O)
      │
      ├─ ambient::tick()
      │    └─ ambient::ambient_pool() → colored output
      │
      └─ mobs::tick()
           └─ probabilistic movement + messages
```

---

## Dependencies

| Crate        | Purpose                                       |
|--------------|-----------------------------------------------|
| `serde`      | Derive macros for serialization               |
| `serde_json` | JSON save file format                         |
| `toml`       | Static game data (rooms, mobs, NPCs)          |
| `colored`    | ANSI terminal color output                    |
| `rustyline`  | Readline-style input with 5-entry history     |
| `libc`       | RSS memory reporting (`runtime` command)      |

---

## Error Handling

| Scenario              | Behavior                                      |
|-----------------------|-----------------------------------------------|
| TOML parse failure    | `eprintln` + `exit(1)` at startup             |
| Invalid direction     | "You can't go that way."                      |
| Item not found        | Fuzzy match fails → "You don't see that here."|
| Missing save file     | Error printed; game continues                 |
| Corrupt save file     | JSON error printed; game continues            |
| Ctrl-C / Ctrl-D       | Clean exit via rustyline `Interrupted`/`Eof`  |

---

## Scale

| Metric          | Value             |
|-----------------|-------------------|
| Rooms           | ~25               |
| Items           | ~35               |
| Mobs            | 2–3               |
| NPCs            | 4–5               |
| Max score       | 100               |
| Command history | 5 (rustyline)     |
| Save format     | JSON (`darkwood.sav`) |
