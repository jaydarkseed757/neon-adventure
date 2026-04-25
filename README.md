# Dark Adventure

A gothic text adventure set in Darkwood Manor. Written in Rust.

## Running the game

```bash
cargo run
```

Or build a release binary:

```bash
cargo build --release
./target/release/dark_adventure
```

## Building a universal macOS binary

```bash
./build.sh
```

Output: `dist/dark_adventure` — runs natively on both Apple Silicon and Intel Macs.

## Commands

| Input                 | Action                          |
|-----------------------|---------------------------------|
| LOOK / L              | Describe current room           |
| EXAMINE \<item\> / X  | Look at something closely       |
| NORTH / N             | Move north                      |
| SOUTH / S             | Move south                      |
| EAST / E              | Move east                       |
| WEST / W              | Move west                       |
| UP / U                | Move up                         |
| DOWN / D              | Move down                       |
| TAKE \<item\>         | Pick up an item                 |
| DROP \<item\>         | Drop an item                    |
| UNLOCK \<thing\>      | Unlock a door or mechanism      |
| INVENTORY / I         | List carried items              |
| SCORE                 | Show current score and rank     |
| QUIT / Q              | Exit the game                   |

## Project structure

```
dark-adventure/
├── Cargo.toml
├── rooms.toml          # all 28 rooms — edit to change the world
├── npcs.toml           # NPC encounter dialogue
├── build.sh            # universal macOS build script
└── src/
    ├── main.rs         # entry point and game loop
    ├── title.rs        # title screen
    ├── world.rs        # loads rooms.toml
    ├── npcs.rs         # loads npcs.toml
    ├── player.rs       # player state: position, inventory, score
    ├── parser.rs       # tokenizes input into verb + noun
    └── commands.rs     # all game actions and puzzle logic
```

## By

J. Collins
# neon-adventure
