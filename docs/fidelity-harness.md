# Replay and trace harness

Development tooling for comparing the controller with the original game frame
by frame. Nothing here needs game data; the optional sound table
(`assets/local/sounds/player.json`) only adds sound names to the trace.

```sh
plumber-jump-voyage --replay run.input --trace rewrite.csv [--camera game.csv]
plumber-jump-voyage --replay run.input --trace lockstep.csv --lockstep game.csv
plumber-jump-voyage --replay run.input --shots runs/name   # rendered captures
plumber-jump-voyage --replay --states        # the original state names used
py tools/compare-traces.py game.csv rewrite.csv --out diff.md [--lockstep]
```

## Input script

```text
start x y z yaw_deg     # game units (cm); yaw 0 faces +Z, 90 faces +X
start spawn yaw_deg     # the game's default spawn; only the facing matters here
camera yaw_deg          # optional: camera yaw for the whole run
shot 30 90              # optional: frames to capture in a rendered replay
<frames> <lx> <ly> <buttons>
```

The stick is `lx ly` in [-1, 1] with +y up; buttons are Switch names joined by
`+` (`A B X Y L R ZL ZR PLUS MINUS LS RS UP DOWN LEFT RIGHT`) or `-`. They map
through the default layout of `input.rs`: A and B jump, X and Y throw, ZL and
ZR crouch, R homing throw, L recenters, MINUS returns to the checkpoint.

## What a replay does

The player is placed on a flat floor with nothing around it, idles for ten
frames, then takes one `Player::step` per scripted frame. The start position
only matters relative to itself on flat ground, so the run happens at the
origin and positions are written in game units relative to `start`. The camera
yaw is the script's `camera`, else the start yaw; with `--camera game.csv` it
follows that trace's `cam_yaw` column, so the world-space input matches the
real run. No window, clock or random source is involved, so a run is
byte-for-byte repeatable.

## Trace columns

One row per step, values after the step. The base columns are `frame`
through `events`; the extended columns follow the real-game probe's layout.
Empty cells mean the rewrite has no equivalent:

- `state` is the name the real game's probe logs, `<actor nerve>/<sub-state
  nerve>` (`Wait/Wait`, `Run/Run3D`, `Jump/Jump`, `Wait/Land`). `--replay
  --states` lists every state with its name; a `?` marks a name that is this
  project's reading rather than one the probe logged or a symbol states
  (`src/game_names.rs` says where each comes from). Several states share a
  name where the game has one nerve for them: walking, running, skidding and
  turning are all `Run/Run3D`, and the jump family is `Jump/Jump`; the clip
  tells them apart. Landing is `Wait/Land` while the landing clip plays.
- Row conventions taken from the real-game traces: values are the state
  after the step; the clip frame is the frame the step's own animation advance
  left (a clip that began this step reads 0); `vy` on the ground is -3, the pull
  the original keeps reporting (the controller cancels it inside the step), and
  in the step that reaches the ground it is the fall speed that moved the
  player; standing is `Wait/Wait`. `state_step` counts the steps of the actor
  nerve (the part of `state` before the slash): 0 in the row where a state is
  chosen, 1 after its first step, and it keeps counting through the sub-states
  of one nerve (walking, braking and turning; landing and standing).
- `anim`, `anim_frame`, `anim_rate`: the controller's clip. The run cycle is the
  game's `Move` slot, a 60-frame phase (`anim_len` 60) whose clips are the
  `anim_layers`, with `blend` weights; its `anim_frame` is that phase, not a
  frame of one clip. Looping clips keep counting past their length.
- `interp` and `anim_layers` come from the renderer's crossfade rules (outgoing
  poses freeze and fade over the clip's interpolation frames).
- `events` lists sounds, reactions and effects that started: sounds from the
  animation triggers and movement events, reactions by name, effects by the
  names of the original's effect tables. As in the game, a sound or effect of
  Cappy's own keeper carries `@cap` and an effect of Mario's carries `@model`;
  `effects_live` is `owner:name` items.
- `counters`, `collision` and `triggers` use the game's keys where the rewrite
  has an equivalent (see below), then its own internal counters.
- `cap_state` and `cap_anim` use the game's cap nerve and clip names
  (`Throw`, `ThrowStay`, `Return`; `FlyingWaitR`, `StayR`).
- Not available headless: `bones`, camera position and pitch, clip lengths
  other than the run cycle's, hand/face/eye part clips, model offsets and the face.
- `effects_live` is approximate: bursts for their nominal lifetime plus the
  state's trail effects.
- `qx..qw` is the facing yaw only; the renderer's smoothed lean is not part of
  the simulation.

### Counters, collision and triggers

Each is `key=value` items (`|` between counters, `;` otherwise). The rewrite
writes the keys it has an equivalent for under the game's names:

| key | the game | the rewrite writes |
|---|---|---|
| `cjump=count/timer` | consecutive-jump count (counted when a jump lands; a continuing jump flies with the count and timer of its take-off) and the frames since | `(jump_count + 1) % 3` and `jump_timer + 1` while standing inside the 10-frame window, `jump_count` and `jump_timer + 1` while a second or third jump flies, else `0/0`. The rewrite also opens the window after a landing from a long jump or a fall; the game does not. |
| `clong=count/timer` | the long-jump count (counted at take-off) and its timer, which waits at zero in the air and runs a 15-frame window after landing | `long_jump_count/0` in the long jump, `long_jump_count/(long_jump_timer + 1)` standing inside the window, else `0/0`. |
| `quickturn` | quick-turn jump frames left; zero through every reverse and side somersault recorded | not written (the rewrite's own `quick_turn` window starts at a skid and is not the same counter) |
| `capjump` | a cap bounce is still possible | `cap_jump_possible` as 0 or 1 |
| `prejump` | frames left on the buffered jump press | `pre_jump` |
| `collision`: `ground`, `wall`, `gn`, `wn` | contact flags and the ground and wall normals | `grounded`, whether a wall was touched, the ramp's normal, the wall's normal |
| `triggers` | raw flag words of the player trigger object | none; the rewrite writes `fx=` (its movement event names), which has no counterpart |

Game keys with no equivalent are not written: `upperpunch`, `forcerun`,
`limitheight`, `wallhist`, `ceiling`, `gmat` (the rewrite has no ceiling flag
and no floor materials). The rewrite's own counters (`coyote`, `jump_count`,
`jump_timer`, `run_frames`, ...) follow the game's keys. The timer of
`cjump` and `clong` means nothing while their count is zero, and the
comparison ignores it then.

## Sample point

The game's probe takes one sample per logic frame after the player's movement
and the model's animation update. Inside that frame the game updates the
animation first (`PlayerAnimator::updateAnimFrame`) and then runs the state's
nerve, and the controller does the same: `Player::step` advances the clips,
then runs the state chosen last frame (`src/nerve.rs`). Animation-frame
triggers (sounds, effects) are scanned up to the frame the clip will hold after
the next advance, which is where the game logs them: a sound on clip frame t is
in the row whose pose still shows frame t - 1.

State changes follow the game's nerve machine (`al::NerveKeeper`). A nerve that
decides a change (a jump press in `Wait`, a landing seen in `Jump`) only names
the next one: that frame's row shows the new `state` with `state_step` 0, and
the old clip, position and velocity. The new state's first step, with its clip
at frame 0, its impulse and its sounds and effects, is frame N + 1
(`anim_frame == state_step - 1` from then on). The controller does the same:
`Player::change` records the state and what its first step does, and the next
`step` runs that first. A landing has one more frame: the ground contact is
flagged in row N, the state that handles it (`Wait/Land`, a run, the next jump)
is chosen in row N + 1, and its clip starts in row N + 2. Sub-states of one
nerve (`Squat/Brake` to `Squat/Wait`, `HipDrop/Fall` to `HipDrop/Land`) are
chosen and started the same way without restarting `state_step`; the run's
walk, skid and turn are one nerve whose physics continue through the change and
whose clip starts a frame later.

## Comparing

```sh
py tools/compare-traces.py game.csv rewrite.csv --out diff.md [--lockstep] [--json diff.json]
py tools/fidelity-report.py <runs dir> [--only idle jump ...]   # SUMMARY.md for every run
```

`compare-traces.py` compares column by column. `counters`, `collision` and
`triggers` are compared key by key and shown as `counters.cjump`; a column or
key that only one side ever writes is listed under "Not comparable" and does
not fail. The probe-only columns (`game_frame`, `tick`, `cam_real_*`,
`late_sample`, `other_events`, `anim_slots`, `floor_y`, `cap_step`,
`cap_hidden`) and the probe's own event kinds (`anim:`, `parts:`, `subanim:`,
`upper:`, `endsub:`, `effect_stop:`, `contact:`) are ignored. Sound mixes
(`sounds`) compare names only: the game logs the request's parameter where the
rewrite logs the mixed gain. The cap columns are compared on the frames where
both sides have the cap out; `cap_out` shows who has it out when.

Next to `game.csv` the probe writes `game.env.json` and `game.meta.json`. A run
whose environment was not clean is refused (exit status 3, a banner) unless
`--allow-contaminated` is given; then the frames it lists are left out of
every column, and the position columns are left out after the first of them.

`fidelity-report.py` replays every scenario that has a `game.csv` (with
`--camera game.csv`), writes `rewrite.csv`, `diff.md` and `diff.json` beside
it, and writes `SUMMARY.md` in the runs folder: per scenario the environment,
the first frame each group of columns diverges and the three worst columns
with their values.

## Lockstep

With `--lockstep game.csv`, before each step after the first the player is put
into the game's previous row: state (mapped back from the nerve name), position,
velocity, front, ground, clip and frame, state frame count and any counters
whose names match `Player::counters`. Internals the trace cannot show (throw,
roll, climb and fire controls, the cap, how long a jump may still extend) keep
the rewrite's own values, and gravity and air-speed limits are set to what
entering that state would have given. A row at the contact with the ground
(`ground` 1 in a state that flies) is injected with the landing still to be
decided. A row whose `sub_step` is 0 shows a state that was only chosen, and a
snapshot cannot hold the first step it has pending, so the replay injects the
row before it and chooses the state again with that frame's input. Compare with `--lockstep` to count the
frames that disagree rather than look for drift.

## Rendered captures

```sh
plumber-jump-voyage --replay run.input --shots runs/name [--camera game.csv]
```

Plays the script in the real game (the game-data build shows the original
Mario) on the same flat floor, one simulation step per update, and saves
`rewrite-<frame>.png` for every frame a `shot` line lists, showing the state
after that frame. The course HUD is hidden and the camera looks along the
scenario yaw with the game's default pitch and distance. Time stops during a
capture so the picture is exact; the camera's smoothing settles meanwhile.
Mario's outfit follows the saved settings (`PJV_SAVE` picks another file).
`PJV_REPLAY_WARMUP` sets the seconds of loading before the script starts.
Two runs give identical pictures. `--trace` can be given as well.

## Determinism

Nothing in the simulation reads a clock, a random number generator or a hash
map's iteration order. Random numbers exist only in presentation: the particle
spawner and the audio variants use fixed-seed generators, and blinking follows
elapsed time; none of them feeds back into the player. `--random game.csv`
therefore only reads the game's logged choices and reports how many frames
carry some; there is nothing in the rewrite to replace.
