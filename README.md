# Plumber Jump Voyage

A Rust/Bevy rewrite of the Super Mario Odyssey moveset in an original obstacle
course. Collect 15 sun sparks and hit 3 cap targets on Sunbreak Terrace.

There are two ways to build it:

- **ROMless** (default): an original explorer character, no game data needed.
- **Game data**: the original Mario model, outfits, animations and sounds,
  extracted from your own copy of the game.

## ROMless build

Install [Rust](https://www.rust-lang.org/tools/install), then run from this folder:

```sh
cargo run --release
```

## Game-data build

This needs Windows, your own game files, and some free tools. Nothing extracted
ever leaves your computer or goes into the repository.

**You need:**

- Your Super Mario Odyssey base game `.nsp`, and optionally its update `.nsp`
- Your own `prod.keys` and `title.keys`
- [Rust](https://www.rust-lang.org/tools/install) and [Python 3](https://www.python.org/downloads/)
- [hactool](https://github.com/SciresM/hactool/releases) (unpacks the game)
- [Switch Toolbox](https://github.com/KillzXGaming/Switch-Toolbox/releases) (converts models)
- Optional, for sound: [vgmstream-cli](https://github.com/vgmstream/vgmstream/releases)
- About 25 GB of free disk space

**Steps** (run from this folder; replace the paths with yours):

1. Extract the player files from your game:

   ```sh
   python tools/extract.py --rom "C:/Games/odyssey.nsp" --update "C:/Games/odyssey-update.nsp" --keys "C:/Keys/prod.keys" --title-keys "C:/Keys/title.keys" --hactool "C:/Tools/hactool.exe"
   ```

   Leave out `--update` if you only have the base game.

2. Build the model converter against your Switch Toolbox (PowerShell):

   ```powershell
   ./tools/build-converter.ps1 -Toolbox "C:/Tools/Switch-Toolbox"
   ```

3. Convert the outfits and animations (a few minutes):

   ```sh
   python tools/convert-outfits.py
   ```

4. Optional, add the original sound effects:

   ```sh
   python tools/extract-sounds.py --vgmstream "C:/Tools/vgmstream/vgmstream-cli.exe"
   ```

5. Build and play:

   ```sh
   cargo run --release --features local-assets
   ```

The game-data build starts in Mario's classic outfit. Pick other outfits in
**Change outfit** in the pause menu; the original outfits are listed first,
then the ROMless explorers.

**Converted before 11 October 2026?** Repeat steps 2 to 4, with `--force` on
step 3 (`python tools/convert-outfits.py --force`). The current version exports
the dash run cycle, keeps glove and visor textures apart, uses the original
scale, and installs sounds with their original triggers and volumes.

More options, such as converting only some outfits or installing GLBs you
exported yourself, are in [the local outfit guide](docs/LOCAL-OUTFITS.md).

## Controls

Gamepad buttons use the Switch layout by position, as in the original:
B is the bottom face button and A the right one. On an Xbox pad, "B" is the
Xbox A button.

| Action | Keyboard / mouse | Gamepad |
|---|---|---|
| Move | W A S D (hold Left Ctrl to walk) | Left stick |
| Camera | Mouse or arrow keys | Right stick |
| Jump | Space | B or A |
| Crouch / ground pound | Left Shift | ZL or ZR |
| Cap throw (hold to keep it out) | Left click | Y or X |
| Homing throw (motion flick) | Right click | R |
| Upward / downward throw | E / Q | D-pad up / down |
| Spin throw | C | Left stick press, or circle the stick and throw |
| Recenter camera | R | L |
| Return to checkpoint | Backspace | - |
| Pause | Escape | + |

Every action can be rebound in **Rebind controls**.

**Moves**

- Jump again as you land for a double, then a triple jump.
- Crouch + jump: long jump while running, backflip while standing.
- Reverse direction at speed to skid, then jump for a side somersault.
- Circle the stick, then jump for a spin jump.
- Crouch in the air to ground pound. Jump on impact for a higher jump; throw
  during the pound to dive.
- Crouch + throw to roll. Tap throw in rhythm to boost. Keep crouch held or the
  roll stops; on flat ground it slows back into a crouch. Press jump and throw
  together to stand up at speed (roll cancel).
- Throw your cap, hold it, then dive into it (crouch + throw in the air) to
  bounce off it. You get another throw and dive after each bounce.
- Slide down walls and jump to kick off. Grab ledges and push toward the wall
  or jump to climb up.

## Fidelity

The controller runs at the original's 60 frames per second in its units
(1 unit = 1 cm) with the original tuning constants: speeds, acceleration and
brake frames, jump powers, gravities, chained-jump windows, long-jump, dive,
ground-pound, roll, wall and cap values. The game-data build plays the
original clips by name, blends the walk, run, dash and fast-dash cycles at the
original border speeds and rates, crossfades with the original blend table,
and plays sounds from the original per-animation sound triggers.

What is still inferred: the public decompilation covers only some player
states, so the logic that joins states (exact input windows, the order of
checks, collision response) is reconstructed from the constants' names and
observed behaviour. Effects are original procedural particles placed on the
original trigger frames, not the game's effect files. The outfit-to-footwear
sound choice is in game code, so it is inferred for a few outfits. The 8-bit
outfit is a rigid voxel figure in the original data, so it moves as one
piece. Captures, enemies and kingdoms are out of scope.

## About

This independent project contains only original code and procedural visuals.
No Nintendo assets, ROMs, keys, game code or extraction tools are included.
Super Mario Odyssey is © Nintendo. [OMM](https://github.com/PeachyPeachSM64/sm64ex-omm)
was consulted as a reference; its code and assets are not used.
