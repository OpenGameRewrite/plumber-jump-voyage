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
plumber-jump-voyage --replay run.input --trace rewrite.csv --bits   # also rewrite.csv.bits
```

## Input script

```text
start x y z yaw_deg     # game units (cm); yaw 0 faces +Z, 90 faces +X
start spawn yaw_deg     # the game's default spawn; only the facing matters here
camera yaw_deg          # optional: camera yaw for the whole run
preroll N               # optional: nonnegative idle frame count (default 10)
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
run. Where the run
has a view matrix the stick is laid on the world as the executable does
(`PlayerInput::calcMoveInput`, `src/movement/move_input.rs`, checked bit for bit with
`PJV_MOVE_INPUT_CASES=<local cases file> cargo test move_input`): the probe's `bits_cam_view` column
when the trace has it, else the matrix the probe pins a locked camera to (the script's `camera` line). The
wall slide, the run and the swim read that value (with the wall bend of `wall_along.rs`); the other states
still take the world direction above. `--bits` also writes `<trace>.bits`, one line per frame
(the ten idle frames as -10..-1, then the rows): `frame,vel,front,up,quat,position` (the position as start-relative centimetres) as the f32 bit patterns of the
probe's `bits_vel`, `bits_front`, `bits_up`, `bits_quat`, for an exact comparison with a full-precision
recording (the `bits_*` columns of `game.csv` and the idle frames of `game.preroll.csv`; `compare-traces.py`
leaves those columns out). The camera's position (`cam_px/py/pz`) goes to the player too: its sound
control hears the camera dip into the water (`CamWaterIn`, `CamWaterAmb`,
`CamWaterOut`). No window, clock or random source is involved, so a run is
byte-for-byte repeatable.

`preroll N` changes the number of idle simulation steps after placement in both
headless and rendered replays. It does not add scripted rows: the first input
row remains frame 0, and `--bits` numbers the idle steps `-N` through `-1`.
Zero skips those steps. The count must fit an unsigned 32-bit integer.
`game.preroll.csv` is observation data for diagnostics and environment judging;
the replay computes its own preroll and never injects those samples.

An explicit `preroll` directive also selects the interpolation contract for new
captures: the recorder carries its sampled clip and blended pose from preroll
into frame 0, including the first row's `interp` label. A corresponding game
trace converter must carry the `FIDP` animation history into `FIDE` rows and
emit `game.preroll.csv` with the same `-N..-1` numbering. It must not seed the
source clip with an empty string at frame 0. A pose displayed before teleport
is unobserved; use enough preroll for its interpolation to finish before the
compared interval, rather than reconstructing that pose from the recording.

Scripts without the directive retain the original ten steps and the legacy
first-row empty interpolation label, so existing captures keep their exact
replay output. An explicit count of ten uses the same simulation steps but
opts into the new first-row label convention when a blend is still active.
The directive is a coordinated capture/replay contract; recorder support must
be installed before using it to make new game recordings.

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
degrees a frame, so a return that begins facing away arcs. Two throws fly other ways: the spin
move's throw (a stick circle, then the throw button) circles the player on a widening spiral for 105
steps before it returns (`src/movement/cap_spiral.rs`), and the throw at the end of a ground pound's
landing falls to the floor and rolls along it at 30 a step, braking over 30 steps (`src/movement/cap_rolling.rs`;
its shape turns with its pose). A throw pressed while the cap is out waits in the input buffer and
starts on the frame the cap is caught. The replay's `collision` column then also carries `ceiling`, `gmat`
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
`alongVectorNormalH`, `turnVecToVecDegree`, `toQuat`, `slerpTo`, `makeQuatRotationRate`, `turnQuat`) in their single precision
order, checked bit for bit against the executable (`PJV_POSE_CASES=<local cases file> cargo
test pose_math`). A quarter turn about the vertical axis is a tie of `toQuat`'s scalar and
vertical-axis branches, so the sign of `qy`/`qw` follows the rounding noise of the pose
(the floor normals carry the game's own noise, and `slerpTo` does not renormalise). The
standing and running states therefore update the pose every step, also on a level floor. `rs::waitGround` starts with
`al::followRotateFrontAxisUp` whenever the collider found ground: the pose's front is carried
through the previous inverse and the current base matrix of the collision part under the feet and the
pose is rebuilt from the up and that front (`ActorPose::follow_part`, checked bit for bit with
`PJV_FOLLOW_CASES=<local cases file> cargo test pose_math`). The stage's parts are static, but a plaza
rotated by 45 degrees (`CapWorldHomeGround000`) makes the product of the two matrices differ from the
identity in the last bits, and that is what holds the settled pose of a placed player at the exact bit
pattern the probe records (the first step after the teleport has found no ground and does not follow;
every later one does, and the pose stays where it is).

Three more rules the full-precision recordings of the pond runs pinned down. The swim state's
in-water judge (`PlayerJudgeInWater::judge`) measures the root against the collided ground
position on a tilted floor in the stage's own coordinates, and `PlayerActorHakoniwa::movement` asks
the material code (the water prefix of the effect variants) after the step's collision, so its
puddle test reads the height check of the position the step ended on. A standard jump takes off with
the velocity of the plane under the body now (`PlayerActionAirMoveControl::setup`: the velocity
without its part along the collided ground's normal, then without the part along gravity), not
with the horizontal part the run kept for the plane it last steered on. The pound and the jump out
of a run on a slope settled the rest (`swim-hipdrop-in` passes strictly):

* A jump chosen out of the run or the standing state keeps the ground pose's up
  (`PlayerJointControlKeeper::calcGroundPoseUp`: the actor's up as the spine's stand-up matrix turns it;
  the exe stores that matrix as a copy of the forward one, not its inverse) read with the pose the
  run step *began* with, because the run's own pose update comes after the jump is chosen. The jump's
  first step turns the pose to it (`rs::slerpUp`, rate 1) and the air control relaxes the pose toward
  the world's up and the turn controller's front at `SlerpQuatRate` (`rs::slerpUpFront`, 0.15) every
  step, so the tilt keeps 0.39 of its size on the first row and 0.85 on every later one. The air
  control's turn controller is the faithful port (`turn_control.rs`, set up with the `JumpTurn*`
  constants).
* The pound's start turns the pose to the world's up at once (`al::turnQuatYDirRadian`), its landing
  runs `rs::waitGround` (the pull along the floor's normal, `GravityMove`, and the pose following the
  floor by `SlerpQuatRateWait`). The tilting pose tilts the collider's arrows with it, which is what
  sinks the body along the normal (a ratio of 0.85 per row) until the in-water judge fires. The
  landing in a puddle wets the body (`PlayerWetControl::recordHeavyLandPuddle`).
* The in-water judge's tilted floor test turns the pose's up onto the floor's normal with
  `al::makeQuatRotationRate` and measures the root's rise along gravity (`+ 5`), in the stage's own
  coordinates.
* The velocity the trace shows for a ground state is the one the step moved with: the normal the
  nerve saw, not the one the collision found since.

Still not bit exact: the first rows of a jump from a slope differ from the recording by a few ulps
(the pose the keeper stored before the jump is not observable), and the first row of the swim after
the run into the pond (`swim-dive`) differs by 1e-6 in the velocity.

The console's single precision library is not correctly rounded: the image links a musl
derivative (`sinf` and `cosf` through double precision kernels, `acosf`, `asinf`, `atanf` and
`atan2f` as single precision polynomials), and about one `acosf` in ten and one `atan2f` in
four differ from the correctly rounded value in the last bit. `src/movement/console_libm.rs`
ports those six functions and every routine that mirrors the game's own calls uses them
(`PJV_LIBM_CASES=<local cases file> cargo test console_libm`). On top of them three controls
are ported operation for operation and checked against the executable's own routines over
random sequences of frames, every field they write compared after every frame: the turn
controller (`PlayerActionTurnControl::update`, `turn_control.rs`, `PJV_TURN_CASES`), the
ground move control (`PlayerActionGroundMoveControl::updateNormalMove` with its helpers,
`ground_move.rs`, `PJV_MOVE_CASES`) and the stick's bend along a wall (`PlayerInput::
updateWallAlong`, `snapWallAlongInput`, `wall_along.rs`, `PJV_WALLALONG_CASES`); the wall
slide's numbers follow them (`PlayerStateNormalWallSlide::appear`, `followNormal` and the slide
step, `wall_slide.rs`, `PJV_WSLIDE_CASES`: the grab's velocity is a normalized direction times the
amount, and the direction the slide pushes by is the pose's front, turned round). The velocity
the ground control sets is kept whole, the pull along the normal included: the next step
rotates that vector onto the new floor (`alongVectorNormalH`, which strips the part along the
old normal), and that rounding is the noise of the game's speed (14.000001, 13.999999) which
decides, among others, the run's turn rate (`run-turn-right`). What stays out of reach is the
stick itself: the probe logs the world direction it got at six decimals and the camera's matrix
not at all, so a run that depends on the last bit of the stick drifts by a few 1e-6 a frame.

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

Landing (`PlayerActorHakoniwa::setNerveOnGround`) asks its judges in the original's order: the
buffered jump, `PlayerJudgeStartRun` (the stick), `PlayerJudgeStartSquat` (crouch), then the wait
state. The force-land judge (`PlayerJudgeForceLand`: a long fall, or action trigger 11, which
`exeJump` sets when the spin drop's `DownFall` nerve ends on the ground) vetoes the run and the
squat, and `PlayerStateWait::appear` then picks `LandStiffen`: it plays `LandDownFall` after the
spin drop and can be cancelled from its 24th step (`HipDropLandCancelFrame`), a hard fall's
clip not at all. A landing with the stick and crouch held therefore runs first and crouches a
step later. The roll's restart counters follow `PlayerStateRolling::appear` (counters set in
the choosing step; `control` first runs after the first nerve step), `control` (the jump nerve
empties charge and force every step) and `restartRolling` (the boost reaction is named by the
restart counter: `[1回目]` the first, `[2回目以降]` after that, which a roll that began on a landing
or a pound cancel starts at one). The probe prints the action trigger word in hexadecimal.

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
- Not available headless: `bones` (the head joint alone is evaluated, see "Head look-at"), camera position and pitch, clip lengths
  other than the run cycle's, hand/face/eye part clips, model offsets and the face.
- `effects_live` follows the original's effect system (`src/effect_life.rs`): an effect is
  listed while its emitter set has an emitter left alive after the frame's particle
  calculation. A one-time emitter is alive until `start + duration + life` frames of the set's
  age (plus the fade time of an emitter that fades its alpha or scale), an emitter that never
  stops emitting or whose particles never die is always alive, and a child emitter lives to its
  last emission plus its life. The calculation runs after the probe's sample, so an effect is
  first listed with age 0, or age 1 when Cappy's own update started it. The explicit stops
  (a clip change, the table's end frame) are the original's `tryDeleteEffect` calls. Limits: the
  randomised particle lives come from a global random generator the rewrite cannot reproduce
  (the longest life is assumed, only a child emitter depends on it), and a recorded
  teleport that skips game frames in one row is not modelled.
- `fx_sets` and `fx_kills` are probe-only (the emitter sets' state and the kill calls); the
  rewrite writes neither.
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

## Head look-at

The head follows a nearby sensor the way the original's `PlayerEyeSensorHitHolder`,
`PlayerJointControlKeeper` and `al::JointLookAtController` do (`src/movement/look_at.rs`,
`src/movement/look_joint.rs`; the joint callback is checked bit for bit against the executable,
`PJV_LOOKAT_CASES=<local cases file> cargo test look_joint`). The state lives in the player
(`Player::look`); the windowed game turns the model's `Head` joint after the animation
(`aim_head` in `src/local_assets.rs`), and the headless trace evaluates the head itself.

* **Targets.** A replay knows other actors only through the stage export: `--geometry` files
  may carry `look_sensors` (`collision_export.py`: the hit sensors of placed actors whose
  handlers answer neither `PlayerDisregard` nor `PlayerLookAtPosition`; today the Cap Kingdom's
  collected coins, `CoinCollectEmpty`, at the placement plus `DisplayTranslate`). Without them a
  replay has no look-at candidates, as on the flat floor. Hosts that answer `LookAtPosition`, NPCs
  that move and the fixed-direction modes of the bind and demo states are not ported.
* **Run state.** `PlayerStateRunHakoniwa2D3D::isEnableLookAt` hands `Run3D` to the base run
  state's test for its own nerve: the run, the brake and the run after a turn may look, the turn
  and the pivot (`TurnPoint`) may not. The mode follows the nerve chosen in the same step, a step
  before the clip starts (`spin-ground-restart`, `land-down-fall-zl`). The wall push is not ported.
* **Frame order.** The holder (`attackSensor`) runs in the sensor phase, before the player's own
  movement: the eye sensor is where the previous frame left it, for the overlap and for the
  distances compared against the other sensors' positions of this frame, and the keeper then reads
  the candidate that phase left (`rocket-ground-start`: a flower's `Equip` sensor trailing a
  running player takes the candidate back and forth with the static `Body`). Static sensors give
  the same rows as the older "current eye, keeper a frame later" reading. The keeper does not run
  in the player's very first movement: the first preroll row of every recording has `mode=0`
  (inferred from those rows; the flag behind it is unresolved).
* **Columns.** `look_keeper`, `look_sensors` (the holder's candidate and the keeper's target as
  `host/sensor/type`) and `look_target` (the stage position asked for this frame) follow the probe's
  format. `head_q` is the head joint's world rotation after the look-at. `head_pos` (the animated
  head's position, cm in the replay's frame) and `head_turn` (degrees the look-at has turned it) are
  the rewrite's own: the trace has no skeleton, so `src/head_pose.rs` poses the head joint chain
  from the clips of `assets/local/Mario.glb` with the blend the trace already shows. That is the
  animation alone: the original's joint controllers that run before the head's (ground and swim
  poses) lean the spine on slopes and in the water, which moves the head by up to 35 cm there.
* **Comparing.** `head_q` is compared (angle between quaternions, 1 degree) only where a look was
  asked for in the game (`requested=1`) or the rewrite's head is turned by more than half a
  degree, and only where the rewrite's `head_pos` is within 1 cm of the game's `bones` Head: that
  position does not depend on the look-at, so it tells where the unported joint controllers do not
  bend the chain. The report notes how many frames were left out. The keeper's acquisition frame
  depends on the same head position (the 400 cm gate), so a head that leans differently can take a
  target a frame early or late; lockstep takes the head position, keeper counter and target from
  the game's previous row.
* **Unplaced hosts.** With `--geometry`, `look_sensors.candidate` and `.target` are compared only on
  frames where the game's host is one the export places. A patrolling Donsuke or a pipe's bind
  sensor is picked by the holder when it is nearer than the static sensors; the replay cannot know
  where it is, so the report counts those frames as left out instead of failing Mario's look-at.
  Static placed coins come from `tools/export-coin-look-sensors.py`.

## Cap throw and catch

The throw is the player's `SpinCap` state (`SpinGround` or `SpinAir`). Its first
step takes the stick's world direction (the facing when the stick is idle) as the
direction the cap leaves along and turns the actor to it at once. In the air the
same step sets the air control up with the spin's limits (speed 7, lift 6, gravity
0.3, nothing extended) and the ordinary air update then pushes with the stick; the
ground fall judge never ends an air spin (it lasts 23 steps). A ground spin ends
when its clip does, after the cap is away; the run starts only then.

The cap is caught when its `Catch` sensor (radius 100) touches the player's `Body`
sensor (50, 75 above the feet) after the cap moved in the frame. The catch plays
its animation and reaction only in the states that ask for them (standing,
running, spinning, falling, the ground pound, the long jump, a plain jump or wall
jump, swimming); in any other (squat, roll, dive, flips, slide, ledge, pole) the
cap goes on the head at once and the frame is the last one that reports the cap.
A jump pressed in the catch window, in the air, pops the player up; without the
press an air catch changes nothing. The cap reads the throw button as the frame
before left it.

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

Damage/death is a scripted consequence when its entry has a proved static cause.
`PlayerActorHakoniwa::checkDamageFromCollision` reads collision before movement,
then `executePreMovementNerveChange` enters the damage state: the judge checks the
entry sample and its predecessor for `gcode=DamageFire` (fire), `Needle` (damage),
or `Poison` (damage/death), a ground contact and a static logged host. `LavaPink`
alone is not a fire code. The host's base position, in the trace's coordinate
frame, must match an exported `static=true` object carrying that code/material on
its triangles. Base positions match the probe's six-decimal f32 serialization;
no distance margin is used. The dashboard selects the export once and gives that
exact file to both replay and judge. Standalone comparisons supply `--geometry
<export.json>`; an export for a different recorded stage/scenario gives no exception.
The episode keeps that cause through its substates (including Dead); a later
damage episode must prove its entry again. Messages, actor contacts and moving
surfaces remain independently refused.

For a capture already burning at frame zero, `game.preroll.csv` supplies the
entry. Its first floor sample can use `floor_triangle_at_start` because preroll
does not log surface hosts: KCL triangle and attribute indices and all vertices
must match a static exported triangle, exactly at the export's three-decimal
serialization. The runner's older approximate `floor_triangle_match` is not used.
Wall/ceiling material names alone do not prove their floor code; those entries
still need probe wall/ceiling code or hit-triangle identity before this exception
can cover them. Fire volumes are also not covered by the floor-code exception.

`checkDeathArea` enters Abyss when the player position lies inside a DeathArea.
The judge checks the entry and previous absolute position against exported cube
areas, using `AreaShapeCube::isInVolume` / `AreaShape::calcLocalPos` bounds and
single precision operations, with no boundary margin. Unsupported shapes or
linked/conditional areas are unproved and retain the old rule. The game's
`isNearZero` scale guard is preserved. A SignBoardDanger sensor is still an actor
contact even though the sign also has exported collision.

New probe contacts append `actor=<positive observation token>` and
`origin=player_death_coin|unknown`. The token identifies one actor throughout
its pooled reuse without recording a guest address; reconstruction at that
address invalidates the old identity. The origin is set only on return from
`Coin::appearPlayerDead` or `appearPlayerDeadReplace`, and cleared by the shared
actor alive/dead paths before reuse or kill. Registry exhaustion emits token
zero and unknown origin. Class `Coin`, placement absence and late contact timing
alone cannot prove provenance; ordinary placed and other spawned coins remain unknown.

A player attack overlap with class `Coin`, a positive token and explicit
`player_death_coin` origin is accepted only while the trace is in a `Dead`
state/substate of an episode whose static cause was proved above. Contacts
before Dead, after leaving Dead, Cappy contacts and handled incoming messages
retain their previous rules. Old recordings without these fields keep their
verdict; the converter never fabricates origin. The environment self-check
(`py tools/compare-traces.py --self-check`) covers missing/invalid identities,
unknown or cleared origin, simultaneous same-name ordinary coins, class
mismatches, before/after Dead, unproved death,
Cappy and handled messages, as well as the accepted tagged death-coin case.

### Recording repeat determinism

`tools/repeat_trace.py` checks repeat CSVs with exact header, row-count and cell
equality outside boot-relative `game_frame`/`tick` and the separately reported
visual observations. `nn::vfx::Emitter::Initialize` has its own global RNG path
and resource/set seed modes (`SetEmitterSetRandomSeed`); the probe resets only
`sead::Random`. `effects_live` observes `isEffectEmitting`, so particle lifetime
can vary despite identical movement and effect emission events. It is reported
as **nondeterministic by design**, without a numeric tolerance.

`fx_kills` includes every actor, not only Mario. Observed other-owner repeat
differences include `stopcd` material updates; their exact source is unproved,
so attributing every difference to particle RNG or render scheduling would be
a guess. These graphics diagnostics are outside the player replay. Other-owner
entries and Mario/cap natural set lifetime observations
(`queue`, `invalid`, `fin`) are reported separately as nondeterministic by design.
Explicit Mario/cap effect kills, unknown records, `fx_sets`, effect start/stop
events, animation and every remaining column still match exactly. This policy
also separates `dropped=N`, overflow of the probe's all-owner kill buffer, and
explicitly reports incomplete diagnostics. On a row where either repetition
overflowed, `fx_kills` is unverified rather than equal/different; every other
cell still matches exactly. Complete rows retain explicit-call equality.
Current twice/late repeats contain that overflow. This policy only tests
recording repeats; game-vs-rewrite effect comparisons remain strict,
including the existing swim `effects_live` residual. The local recording runner
imports this helper; `PJV_FIDELITY_TOOLS` can select a worktree's tools directory.
Run `py tools/repeat_trace.py` and `py tools/compare-traces.py --self-check` to
exercise both policies without launching the emulator.

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

The trace's velocity includes the floor pull of a grounded state and, on the
bubble's landing row, the fall speed the ground already stopped; the injection
sets those back to zero so the next state adds its own pull. The bubble's inner
clocks (`RecoveryStart`, `RecoveryTravel`, `RecoveryFall`) are restored from the
rate-one `TractorBubble*` clip frame, since the exposed counter spans all three.

Known lockstep residuals (free replay passes both): `abyss-recovery-bubble` shows
only `interp` at the Land to Wait blend (the injection has no interpolation
history for the outgoing clip). `burn-lava-twice` still differs where state the
trace cannot show is not reconstructed: the first rows' layers and bones, the
fire run's looping effects (FireRunHipSmoke re-emits because live effects are not
restored from `effects_live`), and the second run's footstep and cycle phase.

Quicksand (`sand_sink.rs`) rebuilds its hidden state in lockstep from the rows: the depth
(`PlayerSandSinkAffect`, replayed over the sand episode from the floor the previous row
stood on and its `floor_y`; GUESS: the surface is flat under the body), the ground
control's turn state, the jump's air control (counted from the jump nerve's own steps) and
the slope slide's `mSlopeForceCounter` (the rows before it on flat ground). Residuals of the
three sand runs: the first rows' `anim_layers`/`bones` (the run start, as in the other
lockstep runs), the hill factor of the sand run on the slope (one row), the push judge's
counter (the wall push starts a row late), and the first row after `AreaAnimWait` is chosen
(`sub_step`).

## Skeleton and the partial animation

The `bones` column is the rewrite's own skeleton (`src/skeleton_pose.rs`): the clips of the
installed model blended by their weights, the joint controllers, and the same pose drives the
rendered model. A catch on the move (`CatchCapParts` on the ground, `CatchCapJumpParts` in the
air) plays as the game's `partial0` animation: it replaces the main blend on `Spine1` and every
joint below it (the model's partial group) and leaves the hips and legs alone, and when it ends
the main clip fades back in over five steps from the pose shown the row before. The standing
catch (`CatchCap`) is a whole-body sub animation instead.

The game samples a clip at the fractional frame of its animation and keeps the archive's
cubic curves, so the skeleton evaluates those too (`src/clip_curves.rs`, from
`assets/local/Mario.curves.bin`; without that file it falls back to the baked tracks, linear
between whole frames, which leaves toe and hand differences of 1 to 4 cm at fractional frames).
Rotations blend the way the game's blender does: the weighted first two rows of each layer's
rotation matrix are summed per joint and the matrix is rebuilt from the sums, and the poses of
a fade (`interp`) are then interpolated spherically, a frozen pose made of earlier fades
nesting them (`Layer::stage`, `PoseLayer::group`). The windowed game poses the same 27 joints
with that very chain: `trace::SkeletonFeed` steps the trace's animation state (crossfades, sub and
partial animations) once per simulation step, `Player::pose_skeleton` bends the result, and
`pose_joints` writes every one of those joints to the model before the transforms are propagated,
drawing between steps by advancing each layer's frame by its rate times the share of a step that has
passed. Bevy's weighted blend of the clips still poses the joints the trace does not follow (fingers
and face), and the whole body without the installed curves. `PJV_POSE_CHECK=1` makes the windowed game
log how far the drawn joints are from the traced ones, measured in the frame of `AllRoot` (about
0.0001 cm: the f32 rounding of the transform tree).

## Rendered captures

```sh
plumber-jump-voyage --replay run.input --shots runs/name [--camera game.csv]
```

Plays the script in the application (the game-data build shows the original
Mario), one simulation step per update, and saves
`rewrite-<frame>.png` for every frame a `shot` line lists, showing the state
after that frame. The window is 1600x900 (scale factor 1), the size of the
real game's screenshots. The course HUD is hidden. Time stops during a
capture so the picture is exact. Mario's outfit follows the saved settings
(`PJV_SAVE` picks another file; the default is the classic one).
`PJV_REPLAY_WARMUP` sets the seconds of loading before the script starts.
Two runs give identical pictures. `--trace` can be given as well. The
`stage` selects the collision export with `--geometry-dir`; `--geometry` selects
an export directly. Collision follows that export, while the rendered floor and
scenery remain placeholders. Exported look-at sensors are marked by yellow rings.
The real-game scripts' `outfit` and `hud` header lines are accepted and ignored.

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

## Death, Abyss, respawn and bubble recovery

`PlayerActorHakoniwa::checkDeathArea` (after the step's collision) either starts the bubble
recovery or sets the player dead and chooses the `Abyss` nerve; the nerve is chosen late, so
the trace shows the old state with step -1 for one row (`src/movement/abyss.rs`). A fall that
ends in a death area is dead for good unless a recovery is valid: a saved safety point alone
is not enough, the player needs an active recovery area, Cappy's rescue (assist) or the
assist option (`recovery_dead.rs`, `PJV_ASSIST_RECOVERY` in the windowed game). Safety points
are taken from static collision only (`safety.rs`: a +10/-100 cm ray, the 60 degree gate, the
unsafe floor and wall codes, and the 100 cm slide search); actors answer the original's
`AskSafetyPoint` message, so an export marks the objects that do with `ask_safety_point`
(default false, which also means a moving host never answers).

Normal-mode `RecoveryArea` entry is a separate pre-movement path. The actor
remembers an active recovery area while inside it; after leaving all recovery
volumes with a valid safety point, it starts the bubble immediately. This path
does not force damage or apply the death-area HP-one check. The safety-point
slide checks the projected dangerous position first when airborne: supported
safe ground vetoes moving the saved point farther inland.

`py tools/export-safety-hosts.py <geometry.json> --romfs <extracted RomFS>` adds
verified `AskSafetyPoint` replies using the scenario's placement classes.
`FixMapParts` accepts with no appear/kill switch links; `RippleFixMapParts`
accepts directly. Other classes stay unmarked until their handlers are ported.

The bubble (`RecoveryStart` 45 steps, `RecoveryTravel` 60 to 120 steps, `RecoveryFall`) is the
original's own math: the travel's length, arch, eased progress, spin and root offset, whose
routines (`al::rotateQuatRadian`, `al::rotateVectorDegree`, `al::scaleVectorDirection`,
`al::calcAngleOnPlaneDegree`) are checked bit for bit against the executable with
`PJV_RECOVERY_CASES=<local cases file> cargo test rotation_oracle` (the cases come from a
local generator that runs the executable's routines; nothing of them is in the repository).
The normal-mode RecoveryArea path is also compared against a complete recorded
Start/Recovery/Fall/landing sequence. A gap in the game's pad counter is not
an extra player update when its state and animation counters stay consecutive.
After the landing the damage and push invalidation last 120 steps.

Death is judged by `PlayerJudgeDeadWipeStart` (`death.rs`): the counter starts at -1 and
runs only while the player is dead, and the scene's Miss sequence closes the wipe
(22 + wait + 28 layout steps; 96 for damage, 30 for the Abyss, 90 with the cap message) before
the stage is rebuilt. The rebuilt player is a new actor at the stage's restart placement,
with the default health, keepers and an entrance camera. A replay learns where from the
local export: `tools/export-restart-points.py <geometry.json> --romfs <romfs>` adds
`restart_points` (the stage's `PlayerList` entry and the checkpoint flags' `PlayerRestartPos`
links) to a geometry export, and the recording's `restart_point_id` metadata, else its
`spawn_abs`, selects one. The scene loading gap is outside the player's updates: it is the jump
of the boot clock (`game_frame`) while the player is dead, never extra scripted frames. The toy
course has no loader, so its instant reset is gone and a dead or fallen player goes through the
same wipe and respawn at its checkpoint.

The entrance camera (`entrance_camera.rs`) keeps the head's look-at off in a new scene until
the player has moved more than 10 cm from the first position (or reset the camera); the
poser ends one update later and the player notices one frame after that, which the traces
confirm. Its time limit and right-stick ends come from stage camera data and are not modelled.

Limits: only 3D with world-down gravity exists (2D, arbitrary gravity and the original's
parallel-axis spin branch are not ported; the spin branch is a marked guess), a moving safety
point does not refresh the bubble's target, the original's `TractorBubbleWait` sound
sequence uses a command the sound table cannot play (the bubble is silent), the bubble is a
translucent sphere rather than the child actor's model, and the wipe is a black layer: the
original's layout animation and the fade-in after the scene loads are not drawn. Lockstep
replays of the target runs differ from free replay at the reload (one frame of a quarter-turn
quaternion sign) and after the long idle in `burn-lava-twice-late` (the injected state has no
area-animation sub-state); the free replays pass every comparable column.

### Quicksand fixture and smoke

The toy course has a quicksand patch (`Course::sands`, `world.rs`: the terrace under it is the
sand's bottom, two metres thick). Its collision answers `SandSink` like a stage triangle, the
body sinks into the solid at 200/240 cm a step, and a body deeper than a short step is held
inside the patch by its edge. `PJV_SCRIPT=sand` (walk on, stop, jump out while shallow) and
`PJV_SCRIPT=sand-die` (stay until the death) run it from `PJV_SPAWN=-0.5,0,1`; start the game
from the repository root with `PJV_PLAY=1 PJV_SAVE=<private save> PJV_SMOKE=8 PJV_SHOT=... PJV_SHOT_AT=...`.
Fidelity limits seen in the shots: the sand dust (`SandWait`/`SandWalk`) is drawn as red
speckle quads by the particle adapter, not the original shader, and the patch is a flat tinted box.

### Damage Life/Swim environment corrections

SandSink episodes use the same static-floor entry proof as damage: the entry
sample or its predecessor must report grounded `gcode=SandSink`, a static
collision host and its base matching exported static triangles with that
floor/material code. SandDesert material alone is insufficient. A new entry
after Slide needs a new proof. The episode includes SandSink/Dead and a direct
transition to Dead; only probe-tagged player-death coins during Dead receive
the existing coin exception. Missing geometry/replay, moving hosts, ordinary
coins and handled actor messages remain refused. SandGeyserRaise requires the
independent source proof described below. `environment_rule.self_check()` covers
these boundaries.

The legacy recorder's `trace_tools.env_report` starts horizontal displacement at
its teleport origin. Its `FIDP` placement/preroll samples are subsequent player
updates, numbered -N..-1, before the scripted sample 0. The environment judge
therefore rechecks a listed frame-zero displacement against `game.preroll.csv`'s
frame -1 when present. It keeps the existing 1 cm grounded threshold and refuses
an unexplained residual or missing immediate sample as before. No trace or
recorded environment file needs rewriting. A recorded Lake example is
px[-1]=-23.840332, px[0]=-26.169434, vx[0]=-2.328990: residual 0.000112 cm,
not the origin-based 23.840444 cm. Other listed reasons still receive their own
checks. `compare-traces.py --self-check` covers valid/missing/nonadjacent preroll
and an actual displacement.

`checkDamageFromCollision` (1.0.0 0x41fb00) raises ordinary damage for Needle;
`executePreMovementNerveChange` (0x420378) dispatches a swimming actor to
DamageSwim. The static-damage classifier now recognizes that family only for
exported static Needle entry, retaining the existing host/code and geometry
identity requirements. It does not infer damage from water or material alone,
and does not waive physical actor contacts. Self-checks cover the accepted
Needle and refused Ground/Fire/Poison, missing replay, and actor push cases.


### Settled candlestand fire messages

`tools/export-candlestands.py <geometry.json> --romfs <romfs>` adds local
`candlestands` causes and Body look targets, and marks their existing KCL parts
static. Back up the geometry in its local `old/` directory first. The exporter
supports upright, unit-scale, initially lit `LavaWorldCandlestand000` placements
without attachments, recovery or state/appearance links; visibility-only ViewGroup
links are permitted. Other placements remain unsupported. Fire is EnemyAttack10,
r40 at local y200; Body is MapObj12, r80 at y210. Wick is invalid while lit.

This is a settled recorder fixture, not an arbitrary candle state machine:
`Candlestand::appear` disables clipping until BurnUp180, Fire/Body are validated
at60, and the recorder waits120 consecutive player Wait updates before teleport.
Thus the initially lit stage-loaded source is active before placement preroll.
Extinguishing/relighting and linked candles are outside this fixture. The early
BurnUp push needs a Simple sensor (11/13/14); none of this candle's sensors has
that type, so Fire10/Body12 cannot send it even before step120.

Before each movement, the replay checks the preceding absolute player pose's
3D InitSensor spheres (Foot r40 y40, Body r50 y75, Head r40 y110) against Fire.
`HitSensorDirector` uses strict squared distance less than squared summed radii,
without a contact margin. PlayerAttack and PlayerEye do not receive this attack.
The message honours the damage keeper's prevent byte and enters the existing
DamageFire pre-movement dispatch; it supplies no fire floor/material code.
The burn recordings enter DamageFire on frame19, including the first jump step.

The environment judge requires the same stage/scenario export, a static matching
part, its settled initially lit source and the exact previous-pose overlap.
Only the exported Fire/Body sensors and known 3D player sensors qualify. The
`EnemyAttackFire` receive must have the right player/source types and answer;
its successful receive proves the entire DamageFire episode. The corresponding
receivePushMsg forward with answer false is harmless. Other messages, true push
answers, Cappy, Wick, unexported sources and unsupported sensor variants stay
refused. `compare-traces.py --self-check` exercises the acceptance boundaries.

A candle can be clipped during the fire hop. The replay uses its exported
placement-centred r300 InitClipping sphere, recorded visible camera position,
direction and FOV, and FrustumRadar's two vertical-plane tests. Invalidating the
sensor clears the eye holder's candidate; it returns on a new overlap after
unclipping. This reproduces candidate absence on frames41?86 in both burn
captures. This narrow fixture does not implement horizontal/far clipping,
camera roll, visibility-group state or other actors' clipping. Without a recorded
camera, those sensors remain active. No full actor-clipping parity is claimed.
The existing `candle-look` head controller discrepancy at its braking frame61
remains outside the fire-cause fix.

### Timed SandGeyser sources

`tools/export-sand-geysers.py <geometry.json> --romfs <romfs> --phase
<game.geysers.csv> --curves <SandGeyser.curves.bin>` adds placement-bound sources
and original Top translation coefficients to an existing raw-KCL stage export.
Back up the geometry in its local `old/` directory first. The separate curve
export uses `PJVCURV1`; it does not replace Mario's or Cappy's curves. Upright,
unit-scale placements with their raw Top KCL and original target/args are supported.
The exporter also adds stationary, unattached CoinRing Body look sensors from the
stage placement and original InitSensor, without making rings physical launchers.

The recorder supplies `game.geysers.csv` (one row per source per sample, including
preroll), `game.geyser_messages.csv` (receive/push observations with payload bits),
and the exporter writes `game.geyser_seed.json` beside them. Copy all three with
`game.csv`, `game.meta.json`, `game.env.json` and `game.preroll.csv` into a private
dashboard directory. Actor CSV positions are absolute centimetres; placement IDs
and stable actor tokens identify sources independently of their shared display name.
The supported initial seed is independently observed pending Wait0, local Top0,
Raise valid with radius450 at the placement. Missing seed disables source simulation;
an incompatible stage/scenario/preroll or unsupported seed is an error.

Each source advances its own Wait/UpStart/UpLoop/UpWait/UpEnd/UpEndWait sequence,
including the division-before-multiplication rise and step61 overshoot. The original
UpStart Top polynomial is composed after its parent translation; collision uses the
preceding animated sample. UpLoop resets the collision matrix history when validated.
The original UpEnd action ends at frame30 (executed steps0..30), independently
recorded on all four actors. Raise attacks use strict sphere broad phase followed
by the original inclusive cylinder/height checks. The source attacks after Mario,
before its own nerve update; Mario consumes that latch on the next pre-movement pass.
There is no player-position or launch-frame table in the source simulation.

The state disables collision, attracts horizontally toward the source origin and
raises at the payload speed until the original height test fails or the latch is
absent. It then enables collision and supplies world-up30. PlayerCollider first
sweeps from its retained pre-integration pose to the actor, correcting against Top,
then sweeps its own velocity. Thus the exit includes a geometric correction before
the impulse; no measured displacement is added. Grounded states follow the raw
moving part frame; Wait also applies its platform-motion joint impulse.

Acceptance requires all source phase/step/Top/radius/centre/validity rows and the
replay's `collision.geysers` diagnostics to agree for the entire capture, with raw
initial transform, nerve identity, generation and UpEnd query results checked.
Only the identified Raise sensor's proved overlaps and payload-bound receive/push
observations qualify. Top floor acceptance additionally checks its independently
predicted base; carry must explain the horizontal residual under the existing
grounded threshold. Unknown tokens, missing phase/replay data, changed payloads,
Cappy and unrelated hosts remain refused. Synthetic checks run through
`py tools/compare-traces.py --self-check`; cycle, boundary, latch and exit-sweep
checks also run in both Rust tests and `--self-check`.

Local recorder naming: main-relative nerve `1d9a478` is
`PlayerStateSandGeyser`'s SandGeyser sub-nerve, so `trace_tools.py` maps it to
`SandGeyser` (actor/sub-state `SandGeyser/SandGeyser`). This is an executable
identity correction, not an unknown-state comparison exception.

Limits: this fixture uses the recorded alive/unclipped source lifecycle and fixed
world-down gravity. It does not implement clipping/reinitialization or linked switch
behavior for other geyser placements. Moving KCL/source simulation is currently a
headless replay feature; the existing rendered-replay driver runs on its flat
course and does not reproduce this source. The bare scene does not render the geyser
model, its actor sand emitters/audio, or Mario's SandDesert heavy-land stain.
Player animation/effect/sound columns do not establish those presentation details.

The 1200-frame `sand-geyser` capture passes every comparable column in free replay,
including a full source cycle, the next rise and Mario's subsequent sand death.
Lockstep restores the state's pre-integration collider position from the preceding
sample and passes the entire launch/exit; its remaining 15 failing columns originate
in the run transition at110 and the hot-Wait transitions at303/545, including their
sub-state counter, interpolation/effect consequences and the re-executed transition's
moving-ground history. Those hidden-state reconstruction limits do not affect free
replay. No comparator tolerance or contamination exclusion was added for the capture.


### Fixed GrabCeil bars

`tools/export-grab-ceil-bars.py <geometry.json> --romfs <romfs> --phase
<game.grabceil_bars.csv>` exports upright, unit-scale, unconnected PoleGrabCeil
placements with their raw KCL and Snap sensor. Back up geometry into its local
`old/` directory before exporting. It writes `game.grabceil_seed.json` beside the
independent actor CSV; retain it and `game.grabceil_messages.csv` with every
private dashboard copy. Mario's existing assets need the four original clips
GrabCeilStart, GrabCeilWait, GrabCeilSwing and GrabCeilJump. Append clips with
`convert-outfits.py --add-clip` and export the complete original curves with
`convert-curves.exe`; rebuilding all outfits can discard previously added clips.

The bar follows Mario's projection along its 380 cm sensor segment, independently
of Mario's movement state. Its Wait/Grab/GrabEnd lifecycle receives start/end
messages, and GrabEnd returns to Wait only after step30. Snap attack uses the
original strict sphere overlap, body projection and distance/direction arithmetic.
The receive-side ObjSnapForce is retained until updateCollider is eligible to
consume it. In particular, the entry-frame message survives attachment and can
replace collision displacement after Jump step10 or after the state ends on
landing; it does not replace Mario's reported velocity. Clearing this latch every
frame incorrectly removes the later displacement in all three captures.

GrabCeil's ordered collision finders fit the bar, then its two angular springs
advance before input/force evaluation. Start/Wait/Swing follow a full static snap
pose; Swing samples its clip from spring phase through the existing animator's
pending frame change. Release uses the model's AllRoot offset and the original
135-degree jump pose, and the air controller preserves the upside-down hemisphere
while recovering its pose. Upright ledge/pole snap arithmetic remains unchanged.
The exported upright, unlinked AirOnly SnapMoveArea in this fixture restricts
input to its front plane; thus side stick in the lateral capture supplies no drive.
Airborne Land continues its velocity and gravity without restarting its landing
reaction when it touches the floor again.

The environment rule requires matching stage/scenario, fixed raw-KCL placement,
independent initial source observations, every phase/step/centre bit pattern and
replay source diagnostic, stable actor identity and exact message payloads. It
accepts only the identified Snap12's proved player-sensor overlaps and receive
answers and the matching fixed Collision part. Missing/changed source data,
KeyMove or connected bars, Snap13, Cappy and unrelated contacts remain refused.
`compare-traces.py --self-check`, Rust tests and `--self-check` exercise source,
spring, latch and evidence rejection boundaries. No tolerance or trace exclusion
was added for these recordings.

The three bounded GrabCeil recordings pass every comparable free-replay column.
The swing capture ends during Jump; it verifies no landing. These checks cover
world-down, fixed upright bars and the recorded initial conditions. Moving parts,
NoSnap areas, moon tuning, arbitrary gravity and off-plane SnapMoveArea position
centering are unsupported. The nonzero lateral GrabPose arm-IK branch is not
implemented: these captures have zero lateral spring, whose requested grab up
aligns with the actor pose; the existing GrabDynamics block remains disabled for
GrabCeil. Converted animation joints and AllRoot offsets are compared, but this
is not evidence for the unrecorded lateral joint branch. No windowed bar smoke
or full stage presentation fidelity is claimed; the rendered-replay driver still
uses its plain course.


## Skate / ice recordings

FloorCode22 (`Skate`) selects GroundMove's two signed-component passes from
actual ground contact and its velocity/normal test. Ice material10 alone does
not select it; Slide23 and the unresolved Slip24 are separate. The ordinary
turn controller uses the Ice setup and forced reversals. Neutral inertia keeps
Run active; its animation and IK consume DashJudgeSpeed + RunSkateAnimSpeedOffset
(19.5), while movement retains its actual speed and the turn tilt is zero.
Squat preserves backward velocity and uses its Skate brake/side parameters;
Rolling selects its Skate slope-control parameters. The toy's unused slick flag
has been removed: its ice platform now reports the actual Skate floor code.

The seven `ice-*` captures cover straight acceleration, cap14, release, turns,
reversal, small steering, jump/landing, squat braking and rolling/stand-up.
Code22 slope drive and wall braking are ported but unverified by these recordings.
Squat side steering on slopes and SpinCap's special ice entry/reaction remain
outside this recording coverage; the latter entry/reaction is not yet ported.
2D hold rules and Slip24 remain unresolved here. ForceRun and the water-surface run are
ported from the RocketFlower recordings (next section).

The exported infinite TemperatureArea(-3) selects WaitVeryCold through the
existing area handling; its original36-frame loop shows the end frame for one
row before wrapping. No run-name condition or environment-rule exception is used.
For lockstep, recorded floor contact/normal is retained but the contacted code is
read from the geometry export: the probe's boolean gcode classifier reports
Ground even on Skate. Raw input components are retained in `in_stick` while
movement still clamps the stick length to1.

`tools/export-figure-walking-look.py <geometry.json> --romfs <romfs>` adds the
idle FigureWalkingNpc's original NokonokoNpc Body sensor and InitClipping sphere.
Back up the geometry in its local `old/` directory first. The class loads that
archive in its init; do not look for an archive named FigureWalkingNpc. The replay
uses the visible camera and the original16:9 side-plane formula for exported
static clipping spheres; near/far/group clipping and camera roll are unverified.
This fixture only covers the idle NPC before its minigame starts.

Windowed replay smoke captures are local under orch-bin/ice-smoke-stage-verified, frames90,
150 and250 of ice-weave. Mario animates, slides and stops visibly. This is a
controller/presentation smoke, not a reproduction of IceWalker stage visuals:
the replay renders a plain floor and placeholder dust; the original ice shading,
chill-snow atmosphere and stage models are not installed by this port.

Rendered `--shots` now retains the loaded collision export (including its areas)
and the same per-frame input view as the headless replay. Previously it silently
ran on the default flat floor, so a stage-relative recorded camera could lose
Mario. Stage meshes still provide collision rather than stage visual assets.

## RocketFlower equipment, forced run and water-surface run

Four recordings (`rocket-ground-start`, `rocket-water-run`, `rocket-water-jump`,
`rocket-equipment-expiry`) start at `IceWaterDashExStage` scenario 1 with Cappy thrown at a
RocketFlower (pickup at row 45), the equipment worn from row 98 (Mario's forced run to the
water, a jump and the expiry after 240 equipment frames).

* **Source.** `src/movement/rocket_flower.rs` owns the flower and the equipment watcher as an
  independently ticked source, seeded from `game.rocket_seed.json` (the flower's and watcher's
  actor state at preroll start; no Mario row is read). Frame order, evidenced by the side files
  (`game.rocket_flowers.csv`, `game.rocket_watcher.csv`): flower and watcher movement (they read
  the head and cap of the frame before), Cappy, the sensor phase (cap pickup of a `Wait` flower
  against the placed `Body`; the `Equip` overlap with Mario's `Body`/`Head`, which requests the
  equipment), then Mario's movement. The first `Follow` step of a pickup asks for two scene-stop
  ticks that advance only the watcher's clock (the effect age too), as the logic-counter gap
  shows. The watcher drops the oldest of three flowers (or ends) when its clock reaches
  `step % 240 == 239`. Only the upright, unlinked, unit-scale composition is supported;
  the watcher constructs its orientation with `makeQuatFrontUp(headSide,-headUp)` and
  `lineUp` rotates the three basis vectors by that quaternion before spacing the flowers.
  The Equip sensor sees that pose on the next source update. Quaternion round-trip and
  arithmetic order are checked against the executable, including non-unit input.
  `tools/export-rocket-flowers.py` installs the placement and seed into the stage export (back
  the export up first) and `tools/rocket_flower.py` is the environment rule's evidence.
* **Environment rule.** `tools/environment_rule.py` accepts the flower/equipment contacts only
  when the export names the exact flower, the placement matches the original `InitSensor`
  records and the rewrite reproduces its pickup, phase, equip request, refresh and expiry frames
  (`rocket-*`: "accepted exported RocketFlower equipment"). Evidence also checks raw/effective
  nerves, generation, sensor float bits, clipping, duplicate samples and actual pickup/equip
  overlaps. Physical flower contacts must overlap the independently simulated source as well
  as the observed sensor. Unknown source identities and messages retain the existing refusal.
* **ForceRun.** `src/movement/force_run.rs` is `PlayerCounterForceRun`: while the watcher holds
  equipment, `movement` sets the counter to 2 and the speed to 38 before the state choice (the
  reaction `強制走り開始` starts when it was not positive) and the late update takes it down by
  one, so 2 is a refresh window and the watcher decides the duration. GroundMove then runs with
  the forced stick (unit direction, or the turn controller's facing at neutral), the Force turn
  parameters, no reversing brake and the speed floored at 38; squatting is refused; jumps use
  `JumpPowerForceRun` 18, `JumpGravityForceRun` 1 and the `JumpDashFast` clip.
* **Water-surface run.** `src/movement/surface_run.rs` ports the start judge (counter > 0, found
  surface at distance >= 0 or within 0.001, falling) and the sustain judge (enabled by a positive
  counter, then counter > 0 or horizontal speed > 20) from the exported `WaterArea`, and the
  surface response of the Run wrapper (`SwimRunSurface*`: upward velocity toward the surface by
  the approach rate, clamped to 5, plus the finder's displacement, which is zero for the area
  finder). The fall judge is suppressed while the start judge holds, or while Run's enabled
  sustain result holds; a cached Run result cannot keep unrelated states afloat.
* **Presentation of the first run row.** The `CatchCap` partial animation outlives the sub
  animation a forced run ends (`Recorder::upper_outlives_sub`): the main clip's own blend is
  reported again (`WaitVeryCold` into `Move`), which hiding it behind any sub animation did not.
* **Lockstep.** Reconstructing a chosen Mario nerve does not advance the independent source
  clocks or deliver its sensor messages again. Only lockstep restores the preceding observed
  player head pose; the source phases and sensor centres remain simulated. The water finder
  is recomputed from the injected position and exported area. Forced ground/air controllers
  retain their hidden steering history; reconstructing `JumpDashFast` retains gravity 1 and
  the forced horizontal-speed limit. An ongoing catch retains the clip selected at arrival,
  even when equipment changes Mario's state.
* **Results.** All four recordings pass the free replay with 0 failing columns and no excluded
  frames. Lockstep failing-column counts are 9 ground-start and 12 each water-run, water-jump
  and equipment-expiry. The physical water entry, forced jump and expiry tail match; remaining
  failures concern the standing throw's unrecorded recoil at rows 46-55, the cold-area Wait
  choice at row 55, and the resulting blend, head rotation and water footstep reconstruction.
  These are residuals, not a lockstep PASS claim.
* **Coverage and presentation limits.** The flower's radius 300 is from its original
  `InitClipping.byml`. Applying the existing 16:9 side-plane formula to it is inferred and
  reproduces this fixture's clip flags; near/far planes and arbitrary camera roll remain
  unverified. Guess: the first-movement idle keeper gate is inferred from initial samples,
  without a resolved original flag. Other flowers, carried/bound Mario, capture cancellation,
  and eventual slowing into swimming or ice damage remain unverified. Rendered replay seeds
  the same independent source and shows the original Mario with installed assets. It requires
  the original head joint for equipment following; the ROMless explorer has no such joint.
  The stage/water and equipped flower models are not drawn, the placeholder floor occludes
  Mario below its height, and existing run/water effects render oversized overlapping geometry.
  These captures establish a native application smoke, not visual or audio parity with SMO.

The `pole-top-end` and `pole-top-end-turn` captures exercise TopEnd from both
TopWait and TopTurn. Their observed `PoleHandStandEnd` transition retains the
outgoing pose at half weight for one frame (a two-frame interpolation).
The typed contacts include CapHanger and ReactionObject physical overlaps;
the existing environment rule checks their handlers and consequence markers.
