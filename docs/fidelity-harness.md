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

- `state` uses the original nerve names (`trace.rs` lists every state and marks
  the sub-state names that are guesses).
- Row conventions taken from the first real-game trace: values are the state
  after the step, except `anim_frame`, which shows the frame the step started
  from (a clip that began this step reads 0); `vy` on the ground is -3, the pull
  the original keeps reporting (the controller cancels it inside the step);
  standing is `Wait/Wait`.
- `anim`, `anim_frame`, `anim_rate`: the controller's clip. The `Move` cycle
  is reported as its dominant clip with `blend` weights; its `anim_frame` is
  the 60-frame cycle phase shared by all four clips, not a frame of one clip.
  Looping clips keep counting past their length.
- `interp` and `anim_layers` come from the renderer's crossfade rules (outgoing
  poses freeze and fade over the clip's interpolation frames).
- `events` lists sounds, reactions and effects that started: sounds from the
  animation triggers and movement events, reactions by name, effects by this
  project's own effect names (its particles are procedural).
- Not available headless: `bones`, camera position and pitch, clip lengths,
  hand/face/eye part clips, model offsets and the face.
- `effects_live` is approximate: bursts for their nominal lifetime plus the
  state's trail effects.
- `qx..qw` is the facing yaw only; the renderer's smoothed lean is not part of
  the simulation.

## Lockstep

With `--lockstep game.csv`, before each step after the first the player is put
into the game's previous row: state (mapped back from the nerve name), position,
velocity, front, ground, clip and frame, state frame count and any counters
whose names match `Player::counters`. Internals the trace cannot show (throw,
roll, climb and fire controls, the cap, how long a jump may still extend) keep
the rewrite's own values, and gravity and air-speed limits are set to what
entering that state would have given. Compare with `--lockstep` to count the
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
