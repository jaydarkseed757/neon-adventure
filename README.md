# Neon Descent

A cyberpunk text adventure set in Axiom Arcology, a decaying corporate arcology. Written in Rust using macroquad.

You are a recovery runner sent into the three-years-dark tower to settle a debt the founders left open: Axiom was built on the substrate of an older system, the **Protocol**, and the assets it extracted were never returned. Recover the extracted legacy, deliver it to the lobby upload console, and **jack into the net** to make the final return. Type `OBJECTIVES` at any time to see your progress.

## Running the game

```bash
cargo run
```

Or build a release binary:

```bash
cargo build --release
./target/release/neon_descent
```

## Building a macOS .app bundle

```bash
./package_mac.sh
```

Output: `Neon Descent.app` — drag to Applications or share directly. All assets are embedded in the binary; no separate files required.

## Commands

| Input | Action |
|---|---|
| LOOK / L | Describe current room |
| EXAMINE \<item\> / X | Look at something closely |
| NORTH / SOUTH / EAST / WEST / UP / DOWN | Move in a direction |
| N / S / E / W / U / D | Move shorthand |
| READ \<item\> | Read a document or inscription |
| TAKE \<item\> | Pick up an item |
| DROP \<item\> | Drop an item |
| UNLOCK \<thing\> | Unlock a door, chest, or mechanism |
| WEAR \<item\> | Put something on |
| REMOVE \<item\> | Take something off |
| PUSH / PULL / TURN \<thing\> | Try to move something |
| PRESS \<thing\> | Press something |
| KNOCK \[thing\] | Knock on a door or surface |
| SMELL \[item\] | Smell the room or a specific item |
| LISTEN | Listen to the room |
| TOUCH / FEEL \[item\] | Touch something |
| SEARCH | Search the room more carefully |
| ASK \<name\> ABOUT \<topic\> | Talk to an NPC |
| TELL \<name\> ABOUT \<topic\> | Tell an NPC something |
| INVENTORY / I | List what you're carrying |
| SCORE | Show current score and rank |
| OBJECTIVES / GOALS | Show your objectives and progress |
| JACK IN / JACK OUT | Connect to (or leave) the net via your cyberdeck |
| GO \<route\> / READ / BYPASS / RETURN | Net commands, available while jacked in |
| WAIT / Z | Let time pass |
| AGAIN / G | Repeat the last command |
| UNDO | Undo the last action |
| SAVE | Save progress to disk |
| RESTORE | Restore a saved game |
| RESTART | Start over from the beginning |
| VERBOSE / BRIEF / SUPERBRIEF | Control room description verbosity |
| QUIT / Q | Exit the game |

## Project structure

```
neon-adventure/
├── Cargo.toml
├── package_mac.sh      # builds a macOS .app bundle
├── rooms.toml          # all rooms — edit to change the world
├── npcs.toml           # NPC dialogue
├── mobs.toml           # wandering mob definitions
├── nodes.toml          # net node map (cyberspace layer)
├── map.txt             # ASCII room + net node map
└── src/
    ├── main.rs         # entry point and game loop
    ├── app.rs          # game state and frame dispatch; net routing
    ├── title.rs        # title screen
    ├── gfx.rs          # rendering and text layout
    ├── input.rs        # keyboard input and history
    ├── ui.rs           # styled text buffer
    ├── world.rs        # loads rooms.toml
    ├── npcs.rs         # loads npcs.toml
    ├── mobs.rs         # loads mobs.toml, wandering mob logic
    ├── net.rs          # loads nodes.toml; net (jacked-in) command handler
    ├── ambient.rs      # atmospheric ambient messages
    ├── player.rs       # player state: position, inventory, score, net_node
    ├── parser.rs       # tokenizes input into verb + noun
    ├── commands.rs     # all game actions, puzzle logic, intro & objectives
    └── save.rs         # save / restore / undo snapshots
```

## By

J. Collins
