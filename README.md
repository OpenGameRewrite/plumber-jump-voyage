# Plumber Jump Voyage

A Rust/Bevy movement playground inspired by Super Mario Odyssey. Explore
Sunbreak Terrace, an original obstacle course built for jumps, rolls, dives and
cap tricks, with keyboard/mouse and modern gamepad controls.

Hop between sky platforms, climb poles, practice wall jumps, swim in the tidal
pool and collect 15 sun sparks. Hit the three cap targets to finish the course.
Play with the original terrace explorer, or use your own local game data for
Mario's outfits and animations.

## Getting started

Install [Rust](https://www.rust-lang.org/tools/install), then launch from the
repository folder:

```sh
cargo run --release --locked -j 2
```

This default build needs no ROM, downloads no game assets and uses an original
procedural character and scenery. Choose **Let's explore** to begin. The pause
menu includes a wardrobe with a live preview, camera sensitivity, pitch
inversion, sound volume and saved bindings. Red map lines, white selection bars
and a gold wardrobe frame the original course and character.
Menus accept mouse clicks, arrow keys and gamepads.

## Controls

| Action | Keyboard / mouse | Gamepad |
|---|---|---|
| Move | W A S D | Left stick / D-pad |
| Camera | Mouse / J L | Right stick |
| Jump / swim up | Space | A / Cross |
| Crouch / ground pound / swim down | Left Ctrl | LT / L2 |
| Throw / roll / dive / swim dash | Left mouse button | X / Square |
| Walk slowly | Left Shift | RT / R2 |
| Roll boost / cap homing | E | RB / R1 |
| Upward throw | Q | Y / Triangle |
| Downward throw | F | B / Circle |
| Spin throw | C | LB / L1 |
| Spin | V / circle movement input | Left stick click |
| Recenter camera | R | Right stick click |
| Return to checkpoint | Backspace | Back / Select |
| Pause | Escape | Start |

Jump again shortly after landing while moving to chain double and triple jumps.
Crouch + jump gives a long jump while moving or a backflip at rest; turn sharply
and jump for a side somersault. Spin before jumping for a spin jump.

Crouch in midair to ground pound, then jump on impact for extra height. Hold
crouch and throw to roll on the ground or dive in the air. Boost a roll with E,
jump out of it, or release crouch and throw to cancel it.

Hold a cap throw to keep it out. Run into the cap to vault, or jump, throw and
dive onto it for an aerial bounce. Continue with another throw and dive to cross
a gap. Jump while wall sliding; approach a ledge to hang, then jump to climb or
crouch to drop. Use forward/back to climb a pole and jump to dismount.

**Rebind controls** captures a keyboard key, mouse button or gamepad button.
Duplicate bindings swap. Escape and gamepad B remain available to back out of
menus. Settings and the best course time save in your system's configuration
folder. Switching away from the window pauses play and releases the pointer.

## Using your own game data

You need your own Super Mario Odyssey base NSP, optionally its update, your own
`prod.keys` and `title.keys`, and separately installed
[hactool](https://github.com/SciresM/hactool) and
[Switch Toolbox](https://github.com/KillzXGaming/Switch-Toolbox).
Tools and keys are not included. The converter currently requires Windows and
the .NET Framework compiler; the ROMless game does not.

Extract the player resources locally:

```sh
python tools/extract.py --rom "base.nsp" --update "update.nsp" --keys "prod.keys" --title-keys "title.keys" --hactool "path/to/hactool.exe"
```

The extraction wrapper reads your local keys and common tickets, applies the
update and unpacks player model/animation archives. Data stays in ignored local
folders. See [the local outfit guide](docs/LOCAL-OUTFITS.md) for conversion and
launching the optional asset build. You can also install self-contained animated
GLB files you have already exported:

```sh
python tools/extract.py --exports "path/to/outfit-glbs" --scale 0.01
cargo run --release --locked -j 2 --features local-assets
```

The optional sound extractor adds local movement voices and effects, including
jumps, rolls, swimming and cap throws. Instructions are in the outfit guide.
Choose an installed outfit in the pause menu. The optional build also supports
the original explorer when a local pack is absent or fails to load.

## Work in progress

This is a functional movement approximation, not a verified reconstruction of
Odyssey's original controller. Speeds, jump timing, collisions, cap behavior and
animation transitions still need comparison with the original game. The course
uses a small kinematic collision scene; captures, enemies, kingdoms and 2D
transformations are outside this movement demo.

## About

This independent project distributes original code and procedural visuals.
No Nintendo assets, ROMs, keys, original game code or external extraction tools
are included. Super Mario Odyssey is © Nintendo. Extracted data stays on your
computer. [OMM](https://github.com/PeachyPeachSM64/sm64ex-omm) was consulted as a
reference; its code and assets are not used here.
