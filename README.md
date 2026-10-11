# Plumber Jump Voyage

A Rust/Bevy rewrite of the Super Mario Odyssey moveset in an original obstacle
course. Collect 15 sun sparks and hit 3 cap targets on Sunbreak Terrace.

Two builds:

- **ROMless** (default): an original explorer character, no game data needed.
- **Game data**: the original Mario, outfits, animations and sounds, extracted
  from your own copy of the game. Nothing extracted leaves your computer.

## ROMless build

Install [Rust](https://github.com/rust-lang/rustup) and run:

```sh
cargo run --release
```

## Game-data build (Windows)

**You need**

| What | Where | Tested |
|---|---|---|
| Your game `.nsp` (base, optionally its update) and your `prod.keys` / `title.keys` | your own console | |
| Rust | [rust-lang/rustup](https://github.com/rust-lang/rustup) | |
| Python 3 | [python/cpython](https://github.com/python/cpython) ([installer](https://www.python.org/downloads/)) | 3.13 |
| hactool, unpacks the game | [SciresM/hactool](https://github.com/SciresM/hactool/releases) | 1.4.0 |
| Switch Toolbox, reads models | [KillzXGaming/Switch-Toolbox](https://github.com/KillzXGaming/Switch-Toolbox/releases) | "Final" release |
| vgmstream (optional, for sound) | [vgmstream/vgmstream](https://github.com/vgmstream/vgmstream/releases) | r2117 |

About 20 GB of free space is needed while extracting; the tools delete their
leftovers afterwards, leaving about 2.5 GB.

**Steps** (from this folder; use your own paths)

1. Extract the player files (leave out `--update` if you have none):

   ```sh
   python tools/extract.py --rom "C:/Games/odyssey.nsp" --update "C:/Games/odyssey-update.nsp" --keys "C:/Keys/prod.keys" --title-keys "C:/Keys/title.keys" --hactool "C:/Tools/hactool.exe"
   ```

2. Build the model converter (PowerShell, uses your Switch Toolbox folder):

   ```powershell
   ./tools/build-converter.ps1 -Toolbox "C:/Tools/Switch-Toolbox"
   ```

3. Convert the outfits and animations:

   ```sh
   python tools/convert-outfits.py
   ```

4. Optional, install the original sounds:

   ```sh
   python tools/extract-sounds.py --vgmstream "C:/Tools/vgmstream/vgmstream-cli.exe"
   ```

5. Play:

   ```sh
   cargo run --release --features local-assets
   ```

You start in Mario's classic outfit; **Change outfit** in the pause menu lists
the others. After updating this project, run step 3 again with `--force`.
Leftovers from older extractions can be removed with
`python tools/extract.py --clean`. More options are in
[the local outfit guide](docs/LOCAL-OUTFITS.md).

## Controls

Gamepad buttons follow the Switch layout by position: B is the bottom face
button (A on an Xbox pad). Everything can be rebound in **Rebind controls**.

| Action | Keyboard / mouse | Gamepad |
|---|---|---|
| Move (walk: hold Left Ctrl) | W A S D | Left stick |
| Camera | Mouse or arrow keys | Right stick |
| Jump | Space | B or A |
| Crouch / ground pound | Left Shift | ZL or ZR |
| Cap throw (hold to keep it out) | Left click | Y or X |
| Homing throw | Right click | R |
| Up / down throw | E / Q | D-pad up / down |
| Spin throw | C | Left stick press, or circle the stick and throw |
| Recenter camera | R | L |
| Return to checkpoint | Backspace | - |
| Pause | Escape | + |

**Moves**

- Land and jump again for a double, then a triple jump.
- Crouch + jump: long jump while running, backflip while standing.
- Reverse at speed, then jump: side somersault. Circle the stick, then jump: spin jump.
- Crouch in the air to ground pound; jump on landing to go higher, or throw
  during the pound to dive.
- Crouch + throw to roll; throw in rhythm to boost; release crouch to stand.
  Jump + throw together cancels the roll at speed.
- Throw the cap and jump or dive into it to bounce off it.
- Wall slide and wall jump; grab ledges and poles, then climb or jump off.

## Fidelity

The controller runs at the original 60 frames per second in its units
(1 unit = 1 cm) with the original tuning constants. The game-data build plays
the original clips by name, including the hand, face and eye animation sets,
blends the run cycle at the original speeds, crossfades with the original
blend table, and plays sounds from the original per-animation triggers.

Inferred rather than taken from the game: how states join (input windows,
order of checks, collision response), since the public decompilation covers
only some player states; the visual effects, which are original particles on
the original trigger frames; and footwear sounds for a few outfits. Captures,
enemies and kingdoms are out of scope.

## About

This independent project contains only original code and procedural visuals.
No Nintendo assets, ROMs, keys, game code or extraction tools are included.
Super Mario Odyssey is © Nintendo. [OMM](https://github.com/PeachyPeachSM64/sm64ex-omm)
was consulted as a reference; its code and assets are not used.
