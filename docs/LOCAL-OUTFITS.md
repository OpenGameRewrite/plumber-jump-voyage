# Local outfits, animations and sounds

The default game uses an original explorer and needs no game data. This optional
workflow lets you play with outfits, movement animations and sound effects from
your own Super Mario Odyssey files. Nothing extracted is uploaded or included
in this repository.

## What you need

- Your own base NSP, optionally the update, and your local `prod.keys` and
  `title.keys`. Common tickets in your NSP can supply matching title keys.
- [Python 3](https://www.python.org/downloads/). The scripts use its standard
  library and need no Python packages.
- Separately installed [hactool](https://github.com/SciresM/hactool/releases),
  [Switch Toolbox](https://github.com/KillzXGaming/Switch-Toolbox/releases) and,
  for sound, [vgmstream-cli](https://github.com/vgmstream/vgmstream/releases).
- Windows with the .NET Framework compiler for the Toolbox adapter. The default
  game can also build on other Bevy-supported desktop platforms.

Allow roughly 20 GB of free space while extracting, in addition to your NSPs.
Each tool deletes its own working files when it finishes (about 2.5 GB stays
under `local/`); pass `--keep-intermediates` to keep them for debugging, and
run `python tools/extract.py --clean` to remove leftovers from older runs. Run the commands below from the repository folder. Substitute your
own paths; keys may sit beside a ROM symlink and need not sit beside its target.
Do not download ROMs or keys from third parties.

## Extract the player resources

```sh
python tools/extract.py --rom "path/to/base.nsp" --update "path/to/update.nsp" --keys "path/to/prod.keys" --title-keys "path/to/title.keys" --hactool "path/to/hactool.exe"
```

Omit `--update` if you only have the base game. Add `--probe` to check your
inputs before a full extraction. The wrapper invokes your installed tool,
applies the update and unpacks player resources under `local/`, then keeps only
the parts of the RomFS the other tools read. It never prints key contents. You can also supply an already accessible RomFS folder to the
conversion and sound commands with `--romfs "path/to/romfs"`; models still need
to be unpacked with `extract.py --romfs "path/to/romfs"` first.

## Convert outfits and animations

Build the small adapter against your installed Toolbox in PowerShell:

```powershell
./tools/build-converter.ps1 -Toolbox "C:/Tools/Switch-Toolbox"
```

Then convert the wardrobe:

```sh
python tools/convert-outfits.py
```

This assembles bodies, heads, hands and accessories, exports textures and the
movement clips, and installs matching throwable caps. The clips are stored once,
in `Mario.glb`; every other outfit plays them by bone name (its manifest entry
names that file in `animation_file`). Existing conversions are reused; add
`--force` to regenerate them. To select a smaller
wardrobe, list the archive names and choose one or more:

```sh
python tools/convert-outfits.py --list
python tools/convert-outfits.py --outfit Mario --outfit MarioAloha --outfit MarioTuxedo
```

The adapter and its copied dependencies stay under `local/converter/`. Converted
GLBs and the outfit manifest go into `assets/local/`, which Git ignores.

## Add movement sounds

```sh
python tools/extract-sounds.py --vgmstream "C:/Tools/vgmstream/vgmstream-cli.exe"
```

The script reads the game's own sound tables (which sounds each animation plays
on which frame, landing and cap events, volumes, pitch and speed curves),
decodes those sounds, renders the sequence-driven ones from their instrument
bank and writes `assets/local/sounds/player.json`. Adjust **Sound volume** in the
pause menu. Surface materials, 3D positioning and the game's mixer are not
reproduced.

## Add the original particle effects

```sh
python tools/extract-effects.py
```

The script reads the game's particle archive and effect tables from the RomFS
under `local/romfs` (pass `--romfs` for another folder) and installs what Mario
and Cappy use into `assets/local/effects/`: the emitters, the tables that say
when each effect starts, a few small meshes and the particle textures. It needs
the Pillow package (`pip install pillow`). If you extracted your files before
this step existed, run `tools/extract.py` again so the effect files stay in
`local/romfs`.

With these installed the game-data build draws the original dust, rings,
sparks, wind lines and cap trails (smoke puffs with crisp edges, glowing
sparks) instead of the built-in stand-ins, and the replay trace
(`--replay ... --trace`) logs the original effect names. Without them the
stand-in effects stay. Floor materials other than puddles, the smoke's lighting
and a few effects that need the game's flow-noise fields (swim bubbles, the fire
run's seat smoke, the cap's spiral stripes) are only approximated.

## Play

```sh
cargo run --release --locked -j 2 --features local-assets
```

Open **Change outfit**, select an outfit and confirm to wear it. The gold
wardrobe displays a live preview. The three original explorer palettes remain
available alongside your local outfits. Missing or unreadable packs fall back
to the explorer.

Hand, eye, mouth, eyebrow and head animation sets follow each action, including
pupil movement and eyelid visibility. To update an older local pack, rebuild the
adapter and run the outfit conversion again with `--force`.

The conversion does not reproduce every material shader, cloth simulation or
accessory behavior, so some specialty costumes may need further visual tuning.

## Already exported GLBs

Self-contained GLB 2.0 files can be installed without the Toolbox conversion:

```sh
python tools/extract.py --exports "path/to/glbs" --scale 0.01 --yaw 3.141592653589793
```

Adjust scale and yaw to match your exporter. The manifest maps named clips to
movement states automatically when recognizable names are present. You can
edit its `animations` entries to map your own clip names, and its optional
`cap_file` to a GLB under `assets/local/caps/`. All files must remain local.

## Troubleshooting

If extraction fails, use `--probe` to distinguish a key/package problem from
conversion. Ensure the base and update belong to the same title. Conversion
logs live beside each export in `local/outfits/` (add `--keep-intermediates` to
also keep the converter's DAE/glTF files); sound events are listed in
`local/audio/events.json`. A missing optional pack does not prevent ROMless play.

Keep ROMs, keys, extracted files, logs containing sensitive data and external
tools out of commits and release archives. This repository's original source
license does not grant rights to any game data you supply.
