# CLAUDE.md

Guidance for working in this repository.

## Project

**Neon Descent** — a cyberpunk text adventure set in Axiom Arcology, a decaying corporate arcology. Single-binary Rust game built on **macroquad** (GPU-rendered windowed app, not a terminal/TTY program). Single-threaded async game loop.

Binary name: `neon_descent`. Author: J. Collins.

## Build & Run

```bash
cargo run                          # debug run
cargo build --release              # release binary at target/release/neon_descent
./package_mac.sh                   # produce "Neon Descent.app" bundle (strips quarantine)
```

All assets (fonts, title PNG) and game data (rooms, npcs, mobs, map) are **embedded into the binary at compile time** via `include_bytes!` / `include_str!`. The shipped binary is fully self-contained — no external files needed at runtime. The only file written at runtime is the save (`neon_descent.sav`) and transcript (`neon_descent_transcript.txt`), both relative to the working directory.

`build.rs` injects build metadata (`BUILD_DATE`, `BUILD_TARGET`, `BUILD_PROFILE`, `BUILD_RUSTC`) as env vars, surfaced by the in-game `VERSION` command.

## Architecture

Frame-driven loop, not a read-eval-print loop. `main.rs` runs `loop { app.frame().await; next_frame().await; }`. `App::frame()` ([src/app.rs](src/app.rs)) dispatches on `AppState` (`Title` / `Playing` / `Won`) and is called ~60×/sec.

- **[src/main.rs](src/main.rs)** — entry point, window config, the macroquad loop. Title screen is throttled to ~30fps here via an 18ms sleep.
- **[src/app.rs](src/app.rs)** — central `App` struct holds all state. `frame()` drives input → command → render each tick. `process_command()` intercepts `undo`, `again`, and `transcript` directly (they need `App`-level state); everything else goes to `commands::handle()`. Also builds tab-completion word lists in `update_completions()`.
- **[src/input.rs](src/input.rs)** — event-driven keyboard handling via macroquad. Owns the input buffer, cursor, history, and **tab completion** (cycles verb/noun matches; `set_completions()` is fed each frame by app.rs).
- **[src/gfx.rs](src/gfx.rs)** — rendering: status bar, scrolling output area, input bar. Word-wrap and an incremental render cache (`wrap_new_lines` only processes newly-added lines).
- **[src/ui.rs](src/ui.rs)** — styled text model (`StyledLine` / `Segment` / `Style` / `TextColor`). Game logic emits output via `ui::print_*` into a thread-local buffer; `App` drains it into `scroll_buf` after each command.
- **[src/parser.rs](src/parser.rs)** — tokenizes input into verb + noun, resolves aliases, fuzzy-matches (Levenshtein) against known nouns. Joins multi-word nouns with underscores.
- **[src/commands.rs](src/commands.rs)** — all command handlers and puzzle logic. Large match on the canonical verb. Also `print_help`, `print_runtime`, `print_score`.
- **[src/world.rs](src/world.rs)** — `World { rooms: HashMap<String, Room> }`. `Room` has `exits: HashMap<dir, room_id>` and `items: Vec<String>`.
- **[src/player.rs](src/player.rs)** — `Player`: `current_room`, `inventory`, `worn`, `visited`, `score`, `turn`, `scored_events`, `dialogue_seen`, `verbose_mode`.
- **[src/npcs.rs](src/npcs.rs)** — static dialogue NPCs, keyed by room. Topic + item-response branches.
- **[src/mobs.rs](src/mobs.rs)** — wandering entities with a small xorshift RNG and probabilistic move/idle AI (`tick()`).
- **[src/ambient.rs](src/ambient.rs)** — time-of-day labels and atmospheric event pools (own copy of the xorshift RNG).
- **[src/save.rs](src/save.rs)** — snapshot types for UNDO + JSON SAVE/RESTORE.
- **[src/title.rs](src/title.rs)** — splash screen with scanline effect.

## Data files (project root, embedded at build time)

- `rooms.toml` — room graph (the world)
- `npcs.toml` — NPC dialogue trees
- `mobs.toml` — wandering mob definitions
- `map.txt` — ASCII map for the in-game MAP command

Editing any of these requires a rebuild to take effect (they are compiled in).

## Conventions

- **Items and room IDs are snake_case strings** (e.g. `iron_ring`). Display strips underscores: `.replace('_', " ")`.
- **All player-facing output goes through `ui::print_*`**, never `println!`. This keeps it in the scrollable buffer and styled.
- **Two xorshift RNG copies exist** (mobs.rs, ambient.rs) — identical algorithm, different seeds. Both `range()` and `one_in()` are guarded against `n == 0`.
- The game is **single-threaded**; only `thread_local!` is used (the ui output buffer). No real concurrency.

## Gotchas

- This is a **windowed macroquad app**, not a terminal program — it cannot be tested by piping stdin. Running it opens a GPU window.
- **Dirty-flag / skip-rendering optimizations don't work** with macroquad's double buffering: skipping draw calls on a frame presents a blank back buffer and flickers. The main game must redraw every frame. CPU savings come from throttling the title screen, not from skipping frames.
- `ARCHITECTURE.md` and `build.sh` are **stale** (left over from a prior "Dark Adventure" / rustyline incarnation with a `dark_adventure` binary and a `data/` dir). Don't trust them; trust this file and the code. `build.sh` in particular references the wrong binary name and will not produce a working universal binary as written.

## Git

Work happens on `main`, pushed to `origin` (GitHub: jaydarkseed757/neon-adventure). Commit and push only when asked.
