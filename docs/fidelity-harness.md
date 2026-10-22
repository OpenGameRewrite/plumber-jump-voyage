# Replay and trace harness

Development tooling for comparing the controller with the original game frame
by frame. Nothing here needs game data; the optional sound table
(`assets/local/sounds/player.json`) only adds sound names to the trace.

```sh
plumber-jump-voyage --replay run.input --trace rewrite.csv [--camera game.csv]
plumber-jump-voyage --replay run.input --trace lockstep.csv --lockstep game.csv
plumber-jump-voyage --replay run.input --trace rewrite.csv --geometry-dir <exports> [--camera game.csv]
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

The player is placed on a flat floor with nothing around it (or in the real
stage's collision, see below), idles for ten
frames, then takes one `Player::step` per scripted frame. The start position
only matters relative to itself on flat ground, so the run happens at the
origin and positions are written in game units relative to `start`. A start with
no floor within 40 cm below it puts the player in the air: the player has found
no ground since the teleport, as the probe's player has not (the fall state is
chosen in the fifth step). The camera
yaw is the script's `camera`, else the start yaw; with `--camera game.csv` it
follows that trace's `cam_yaw` column, and the stick's world direction is turned
on top of it to the one the trace logged (`move_input`: the game turns the stick
by the line from its camera to the player, which swings while the player moves
although the locked yaw does not), so the world-space input matches the real
run. The camera's position (`cam_px/py/pz`) goes to the player too: its sound
control hears the camera dip into the water (`CamWaterIn`, `CamWaterAmb`,
`CamWaterOut`). No window, clock or random source is involved, so a run is
byte-for-byte repeatable.

## Replaying in the real stage's collision

`--geometry <stage.json>` loads one local export of a stage's static collision
(the triangles of the stage's KCL files with their collision attributes, in game
units, written by the local export tool; nothing of it is in this repository).
`--geometry-dir <dir>` picks the export for the stage (and scenario) the
script's `stage` line names, else the ones recorded in the `game.meta.json`
beside the `--camera` or `--lockstep` trace. A stage's scenarios place different
objects, so `<stage>.s<scenario>.json` is preferred and `<stage>.json` serves
only a scenario it records itself (its `scenario` field); a stage without a
matching export replays on the flat floor. The script's `start` is the origin of the
traces, so the triangles are loaded relative to it (the floor under the start is
at height 0) and the replay's positions stay comparable to the game's. The export
covers a box around the start (25 m by default); a run that leaves it falls off
the loaded area, so re-export with a larger `--half` for long runs.

In geometry the player collides with the triangles the way the game's player
collider does (`src/collider.rs`, written from the 1.0.0 executable): normal or
crouched shapes, three ground arrows, the sweep in steps with a push-out per
contact, the 70 degree rule for floor, wall and ceiling. Cappy collides with a
sphere of its own (`PlayerColliderHackCap`) and turns along a wall it meets at
an angle. The thrown cap keeps a pose like any actor (`src/movement/cap_pose.rs`):
it faces back at the thrower while it flies and hovers, the cap jump turns it to the
front of its model's `AllRoot` joint (`src/cap_joint.rs` reads that joint's clips from
the installed `assets/local/caps/MarioCap.glb`), and the return leans it toward the
player's head and flies along its front, which turns toward the head by at most 40
degrees a frame, so a return that begins facing away arcs. The replay's `collision` column then also carries `ceiling`, `gmat`
(the floor's material), `gcode` and `gcodeidx` (its floor code), taken from the
triangle under the feet. Replays without geometry are unchanged.
`py tools/fidelity-report.py <runs> --geometry <exports>` replays every
scenario in its recorded stage.

The collider and the mesh hold their numbers as the game does, in single
precision at the stage's own coordinates (`TriangleMesh::snap_to_game_precision`),
which matters wherever a body rests against a surface it is pressed into every frame.

A newer export also carries what the game itself tests against: every placed object's
base matrix (with its inverse) and the raw prisms of its KCL files (`models`, with the
triangle rows pointing at their prism). The game does not test a body against world
triangles. It carries the body's spheres and arrows into the part's own frame with the
inverse matrix, runs `KCHitSphereForPlayer` and `KCHitArrow` on the raw prism, and carries
the results back (`src/prism.rs`, checked bit for bit against the executable's functions;
`PJV_PRISM_CASES=<local cases file> cargo test prism` repeats that check). Which side of a
surface a body is on is decided at the 1e-4 cm level, so every operation is the
executable's single precision operation in its order. Older exports (world triangles only)
still load and are tested in world space. After `rs::resetCollisionExpandCheck` (the wall
slide and the ledge catch call it when they appear) the collider grows its shapes from a
bounding radius of 20 to full size before the frame's own move, and that move starts at
its second pose (`PlayerCollider::reset_expand_check` in `src/collider.rs`). A change of
collision form does the same from the old form's radius (`setCollisionShapeKeeper` keeps the
old keeper's radius as the one the collider last had), so the first move after the squat
begins also skips its starting pose.

The actor's pose is the quaternion its keeper was last given, plus the rotation matrix made
from it (`ActorPoseKeeperTQGMSV::updatePoseQuat`). The game reads the pose back through that
matrix (`al::calcFrontDir`, `calcUpDir`, `calcQuat` = `toQuat` of the matrix), the probe's
`qx..qw` columns and the collider's from/to quaternions included. `src/movement/pose_math.rs`
ports the routines behind it (`slerpUpFront`, `waitGround`'s pose, `makeQuatUpFront`,
`alongVectorNormalH`, `turnVecToVecDegree`, `toQuat`, `slerpTo`) in their single precision
order, checked bit for bit against the executable (`PJV_POSE_CASES=<local cases file> cargo
test pose_math`). A quarter turn about the vertical axis is a tie of `toQuat`'s scalar and
vertical-axis branches, so the sign of `qy`/`qw` follows the rounding noise of the pose
(the floor normals carry the game's own noise, and `slerpTo` does not renormalise). The
standing and running states therefore update the pose every step, also on a level floor.

Ported on this collision: falling off a ledge (`PlayerJudgeSpeedCheckFall`), the
long-fall landing (`PlayerJudgeLongFall`, `Wait/LandStiffen`), the wall slide
and the wall jump (`PlayerJudgeWallKeep`, `PlayerStateNormalWallSlide`,
`PlayerStateNormalWallJump`, with the wall history), the wall hit scaling of
a jump and a fall (`scaleVelocityInertiaWallHit`), the air control's turning
of the facing and the ledge hang (`PlayerJudgeWallCatch`,
`PlayerStateWallCatch` with its snap, `src/movement/ledge.rs`): the catch, the
shimmy along a ledge, the climbs, the climb jump and letting go. While the body
hangs the collider uses the hanging form (`changeCollisionWallGrab`: two
spheres hanging out from the wall that ignore floors, and the push along the
wall's direction cut), and carries itself from where it stood the step before
to the pose the snap puts the actor in.

The run on a slope (`src/movement/ground.rs`, `src/movement/plane.rs`) works
in the plane of the floor under the feet as `PlayerActionGroundMoveControl`
does: the velocity is turned from the floor of the step before onto this one
(`updateNormalAndSnap`), the stick is mapped into the plane
(`PlayerInput::calcMoveInput` with the ground normal), the turn control carries
the facing and its last stick into the plane, the hill factor
(`updateHillAffect`) adds speed downhill, and the pose follows the floor's
normal (`rs::slerpUpFront`, `rs::waitGround`, `rs::slerpUp`). The collider
hangs its shapes from the whole tilted pose and averages a push with the
previous round's (`calcResultVec`'s direction flags, `calcGroundArrowAverage`'s
per-arrow slots). The move input is bent along a wall for a while after a
contact (`updateWallAlong`, `snapWallAlongInput`, `src/movement/wall_along.rs`).
The squat keeps the mini collision form through its stand-up. Shapes' world
positions are held as the game holds them, in single precision at the
stage's magnitude.

The pole climb (`PlayerJudgePoleClimb`, `PlayerStatePoleClimb`,
`src/movement/pole.rs`, the finders of `rs::findPoleClimb*` in
`src/movement/pole_find.rs`) reads a pole off its collision triangles: the
codes `Pole`, `Pole10`, `Pole20` and `Pole30Plus`, the touched face, its width
and its axis. The catch, the climb up and down, the turn round the pole, the
top (start, wait, turn, end), the jump off and the top jump run on the same
snap as the ledge hang (`src/movement/snap.rs`) with the pole collider form
(`changeCollisionPoleClimb`). The collider sweeps turn the body from the facing
it had the step before to the one it has now, as the game's pose interpolator
does, and drop the wall hits with a face right behind them
(`rs::calcExistCollisionBorder`). The toy course's poles are hexagonal prisms of
the Cap Kingdom pole's size. Not ported: the swing of motion controls, the
joints of the arms and legs, the slanted codes' angle offset, moving poles.

The forced slope slide (`PlayerStateSlope`, started by a floor with the `Slide` code and
ended on flat ground) runs `PlayerActionSlopeSlideControl` as the roll does. The nerve
machine starts the state in the step that chose it (`appear` and its collision, in the
squat form, come in that step, the first `exeSlide` in the next), a buffered jump press is
refused while the slide forces (`PlayerJudgePreInputJump::judge`) and the pressed buffer
runs down all the same. The vector routines it uses are checked against the executable bit
for bit, and so are the ground averages of the collider (the sum multiplied by the
reciprocal of the count, `al::normalize`) and the stage positions the collider keeps
(the game's single-precision ones; metres cannot hold them near 2048 cm).

Not ported yet: the squeeze of `Press` collision, moving collision parts, the
game's collision in each part's own single-precision frame (the export is in
world space and rounded, so a contact that is decided by 1e-4 cm, such as the
wall-slide landing on two floors, can go the other way), the ledge hang on a moving platform or a tilted top,
the ground nerves' ledge judge (`PlayerJudgeWallCatchInputDir`) and Cappy's
head-on wall stop, floor and lock-on reactions.

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
  player; standing is `Wait/Wait`, which relaxes into `Wait/WaitRelaxStart`
  (clip `WaitRelaxStart`), `WaitRelax`, `WaitSnoozeStart` (clip `SnoozeStart`),
  `WaitSnooze` (`Snooze`), `WaitSleepStart` (`SleepStart`) and `WaitSleep`
  (`Sleep`) when left alone (`src/movement/wait.rs`). `state_step` counts the steps of the actor
  nerve (the part of `state` before the slash): 0 in the row where a state is
  chosen, 1 after its first step, and it keeps counting through the sub-states
  of one nerve (walking, braking and turning; landing and standing).
- `anim`, `anim_frame`, `anim_rate`: the controller's clip. The run cycle is the
  game's `Move` slot, a 60-frame phase (`anim_len` 60) whose clips are the
  `anim_layers`, with `blend` weights (Walk, Run, Dash, DashFast and RunStart, from the speed
  and the stick's tilt as the original's run animation control chooses them).
  Its `anim_frame` is the animator's counter, not a frame of one clip: the
  pose's phase of the previous frame plus the previous rate, so it is not wrapped
  at 60 and leads the layers' own frames. Each layer plays the shared phase at
  its own clip length (`frame = phase * length / 60`); the run start plays
  once and holds its last frame. A one-shot clip's counter runs one past the
  last frame it holds (`Brake` reads 6 while the layer holds 5).
- `interp` comes from the renderer's crossfade rules (the outgoing pose fades
  over the clip's interpolation frames) and names the outgoing slot (`Move`,
  `Brake`); `anim_layers` lists only the clips of the playing slot with their
  own weights, as the game does.
- `events` lists sounds, reactions and effects that started: sounds from the
  animation triggers and movement events, reactions by name, effects by the
  names of the original's effect tables. As in the game, a sound or effect of
  Cappy's own keeper carries `@cap` and an effect of Mario's carries `@model`;
  `effects_live` is `owner:name` items.
- `counters`, `collision` and `triggers` use the game's keys where the rewrite
  has an equivalent (see below), then its own internal counters.
- `cap_state` and `cap_anim` use the game's cap nerve and clip names
  (`SpinAttack` while the cap waits on the player for the throw frame, then
  `Throw`, `ThrowBrake`, `ThrowStay`, `TrampleReturn` (the player stood on the
  hovering cap, the cap jump), `Return` and `Catch`; `FlyingWaitR`, `StayR`,
  `Trample`, `CatchCap`, `CatchCapJumpParts`). Reactions raised on the cap are logged with `@cap`.
- `sub_step` counts the game's sub-nerve: it starts over whenever the state
  label changes or the actor nerve restarts, and at the start of a replay it
  equals `state_step`.
- Not available headless: `bones`, camera position and pitch, clip lengths
  other than the run cycle's, hand/face/eye part clips, model offsets and the face.
- `effects_live` is approximate: bursts for their nominal lifetime plus the
  state's trail effects. The effects a hit reaction starts (`Land`,
  `HipDropLand`, listed as `player:...`) stay live until the same reaction
  starts them again, as in every recording of the game.
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
| `collision`: `ground`, `wall`, `gn`, `wn` | contact flags and the ground and wall normals | `grounded`, whether a wall was touched, the ramp's normal, the wall's normal; in geometry also `ceiling`, `gmat`, `gcode`, `gcodeidx` |
| `triggers` | raw flag words of the player trigger object | none; the rewrite writes `fx=` (its movement event names), which has no counterpart |

Game keys with no equivalent are not written: `upperpunch`, `forcerun`,
`limitheight`, `wallhist`, and without geometry `ceiling` and `gmat` (a flat
floor has no codes). The rewrite's own counters (`ground_lost`, `jump_count`,
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
`cap_hidden`, `contacts`, `hit_hosts`) and the probe's own event kinds (`anim:`, `parts:`, `subanim:`,
`upper:`, `endsub:`, `effect_stop:`, `contact:`) are ignored. Sound mixes
(`sounds`) compare names only: the game logs the request's parameter where the
rewrite logs the mixed gain. The cap columns are compared on the frames where
both sides have the cap out; `cap_out` shows who has it out when.

Next to `game.csv` the probe writes `game.env.json` and `game.meta.json`, which
list every frame in which the run touched something besides flat, empty ground.
That list is not a verdict: most entries are harmless. `tools/environment_rule.py`
judges each listed frame and the report's "Environment" section says which contacts
were accepted and why. Any other contamination refuses the run (exit status 3, a
banner naming the frames and reasons) unless `--allow-contaminated` is given; then
the refused frames are left out of every column, and the position columns are left
out after the first of them. A refusal needs the replay too (it asks whether the
rewrite replayed the stage's triangles), so `fidelity-report.py` replays every run
before it decides. Frames whose only reason is Cappy's `Trample`/`TrampleReturn`
nerve (the player standing on his own thrown cap, the cap jump) are not
contamination.

### Environment rule

A reason is contamination only if it can change Mario or makes the world differ from
what the rewrite replays.

* **Eye overlaps are accepted.** Mario owns an eye sensor of radius 800 cm. The engine
  calls his attack path once for every actor sensor inside it, but that path only
  records look-at targets and sends queries. Nearly every `contact:` an older log
  lists is such an overlap. The probe now logs both sensors of every pair (`contacts`
  column: own and other sensor name and type, the host and its class, the message
  class Mario received and his answer), so an eye overlap is told apart from a
  physical one and never listed.
* **Harmless physical overlaps are accepted on frames without a consequence marker**:
  the cap ships and hangers, the cap door and cap flowers (their handlers have no
  reaction to a player-sent message), and reaction objects (which react to a
  trample, hip drop or kick, so the next frame is checked too). Markers are an
  `attack`, `receive` or `collision` trigger, a state the scenario does not script
  (bind, hack, damage, demo, shine get, press, sand sink, abyss), and a displacement
  that differs from the velocity by more than 0.05 cm in the air or 1 cm on flat
  ground outside a wall, a pole, a hang, a state change or a jump frame.
* **Refused**: NPCs, items, coins, enemies, bushes, dig points and any host or class
  not listed above when one of Mario's non-eye sensors overlaps it; a message Mario
  handles (bind, hack, hit, damage, blow, push, jump requests, enemy fire and so on)
  or an unresolved one he answered; every contact of Cappy.
* **Map geometry is accepted** (`wall contact`, `ceiling contact`, `floor normal`,
  `floor material changed`, a displacement explained by a slope, wall, pole or
  transition) only when the rewrite replayed the same stage's triangles (`--geometry`;
  its `collision` column then carries `gcode`) and the surfaces' hosts are static
  collision parts (`hit_hosts` column: the host of the floor, wall and ceiling
  contact, its `isHostMoved` flags and its position). Without the geometry replay
  these runs stay refused. A log from an older probe has no hosts; its entries are
  accepted by the replay alone and the report says the hosts were not verified.
* **`floor height changed`** is accepted on the frame a fall lands (the previous floor
  sample is older than the previous frame: the intended drop), refused when the floor
  under a stationary player changes height on a flat floor (a moving floor), and
  otherwise judged like map geometry.

Old probe logs (no `contacts` / `hit_hosts` columns) carry only the host's display
name (cut at 48 bytes) in `hit-sensor contact <name>` reasons. Those are judged as
physical overlaps by name, so a run that touches an NPC or an item stays refused
until it is recorded again with the current probe.

`fidelity-report.py` replays every scenario that has a `game.csv` (with
`--camera game.csv`), writes `rewrite.csv`, `diff.md` and `diff.json` beside
it, and writes `SUMMARY.md` in the runs folder: per scenario the environment,
the first frame each group of columns diverges and the three worst columns
with their values.

## Lockstep

With `--lockstep game.csv`, before each step after the first the player is put
into the game's previous row: state (mapped back from the nerve name), position,
velocity, front, ground, clip and frame, state frame count and any counters
whose names match `Player::counters`. The thrown cap (nerve, steps, place,
speed), the spin throw's progress and the model's sub animation are restored
from the cap columns and the logged `subanim`/`endsub` events. Internals the
trace cannot show (roll, climb and fire controls, how long a jump may still
extend) keep the rewrite's own values, and gravity and air-speed limits are set to what
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
after that frame. The window is 1600x900 (scale factor 1), the size of the
real game's screenshots. The course HUD is hidden. Time stops during a
capture so the picture is exact. Mario's outfit follows the saved settings
(`PJV_SAVE` picks another file; the default is the classic one).
`PJV_REPLAY_WARMUP` sets the seconds of loading before the script starts.
Two runs give identical pictures. `--trace` can be given as well. The
real-game scripts' `stage`, `outfit` and `hud` header lines are accepted and
ignored.

With `--camera game.csv` the camera of each picture is the real game's for
that frame: `cam_px/py/pz` (cm, relative to the start, so metres from the
rewrite's origin), the view direction from `cam_real_yaw/pitch` (the probe's
own reading of the game's camera) and `cam_fov` (vertical, 40 degrees in the
probe's chase camera). The probe builds image f's camera from Mario's position
one frame earlier, so a moving Mario sits `speed_h` ahead of the camera
target; the rewrite shows its own Mario in front of that camera, which makes
any difference in position visible too. Both worlds are right-handed with +Y
up, so only the unit changes. Without a trace camera the orbit camera is
used. In an idle frame Mario's head, hands and feet land within a pixel or two
of the game's.

The scene of a rendered replay is deliberately plain: one bare ground plane
the colour of the reference plateau's soil, the usual gradient sky and
distance haze, and no course props. The game-data build lights it the way the
real game lights the Cascade Kingdom (`src/look.rs`, `src/tone.rs`): the sun
stands where the stage's graphics preset puts it (70 degrees up, a little
toward +Z, so it shines on Mario's back from beyond him), Mario's ambient light
is the game's light rig for the player, and a per-channel S curve with a
shoulder (the game's `alHdrCompose` tone map type 8) takes the place of Bevy's
tonemapper. Shadows have hard edges, the procedural drop-shadow disc is not
drawn under an original model, and smoke is lit by its normal map with its
own shadows. The ROMless build keeps its earlier sun and sky. The ground's
colour and glow are matched to the game's picture by eye and by sampling;
its texture, grass hills, waterfalls and sky are not reproduced.

```sh
py tools/visual-compare.py <run dir>
```

For every `game-<f>.png` with a `rewrite-<f>.png` beside it (in a run
directory next to `game.csv` and `rewrite.csv`) writes `side-<f>.png` (game
left, rewrite right, each labelled with the frame, state, clip and clip frame
of its own trace), `crop-<f>.png` (both cut around Mario and enlarged; Mario's
position is projected from the game trace through the game's camera, so both
crops show the same part of the screen) and `visual.md` (a table of the frames
with both traces' states, clips and live effects). Standard library and
Pillow only.

## Determinism

Nothing in the simulation reads a clock, a random number generator or a hash
map's iteration order. Random numbers exist only in presentation: the particle
spawner and the audio variants use fixed-seed generators, and blinking follows
elapsed time; none of them feeds back into the player. `--random game.csv`
therefore only reads the game's logged choices and reports how many frames
carry some; there is nothing in the rewrite to replace.
