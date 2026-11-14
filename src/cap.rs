//! Cappy in flight: the thrown cap's nerves, step by step.
//!
//! Written from the original's `HackCap` (its throw start, `exeThrow`,
//! `exeThrowBrake`, `exeThrowStay` and `exeReturn`, with their parameter
//! block `HackCapThrowParam`) as read from the 1.0.0 executable and confirmed
//! against recorded runs (`capthrow`, `spin-throw`). The flight over open
//! ground is reproduced, and in a stage's real collision (`geometry.rs`) the
//! cap collides with the walls and slides along one it meets at an angle.
//! The spin-move throw (`exeThrowSpiral`, `cap_spiral.rs`) and the throw down at the end of a
//! ground pound (`exeThrowRolling` and its brake, `cap_rolling.rs`) have their own nerves;
//! the re-throw after a catch is the player's (`PlayerJudgePreInputCapThrow`).
//! Cappy's other wall reactions (a head-on hit that stops it, the rebound of the spiral and the
//! rolling throw, the lock-on, the attack on what it hits), its floor reactions, the stick
//! steering of a throw in progress and the throws of the motion controls (`DoubleUp`,
//! `ThrowAppend`, `Rethrow`) have no counterpart yet.
//! The ordinary SignBoardNormal reflection and four-execution zero-velocity Rebound
//! are ported (0x401914, 0x406c0c). `redirect()` remains an invented flick
//! shortcut; this round deliberately leaves it unchanged.
//!
//! The cap is a separate actor that updates before the player in the same
//! frame. It runs its current nerve once per frame and then its collider moves
//! it by its velocity, so a nerve that sets the velocity moves the cap in that
//! very frame. A nerve change (`al::setNerve`) takes effect on the next frame;
//! the nerve name a trace row shows is the one the frame left behind.
use crate::collider::{Location, PlayerCollider};
use crate::geometry::TriangleMesh;
use crate::movement::cap_pose::{CapPose, direction_of, horizontal_length, length};
use crate::movement::{Throw, UNIT};
use crate::movement::{cap_ground, cap_return, cap_rolling, cap_spiral};
use crate::water::{SurfaceFinder, WaterAreas};
use bevy::prelude::*;

/// `HackCapThrowParam`, as stored in `MarioCapCommonInfo/ActorParam`:
/// distances in centimetres, speeds in centimetres per frame.
mod param {
    /// Height of the cap above the feet when it is thrown.
    pub const THROW_HEIGHT: f32 = 70.;
    /// Frames the cap waits at the end of the throw (`mEndpointStopTime`) and
    /// the longest wait while the button is held (`mMaxEndpointStopTime`).
    pub const STAY_FRAMES: u32 = 20;
    pub const STAY_FRAMES_MAX: u32 = 120;
    /// Flight speed (`mMaxVel`).
    pub const MAX_SPEED: f32 = 32.;
    /// Flight speed of the continuous throw (`mContinuousThrowSpeed`).
    pub const CONTINUOUS_SPEED: f32 = 64.;
    /// How hard the return accelerates and its top speed.
    pub const RETURN_STRENGTH: f32 = 18.;
    pub const RETURN_SPEED_MAX: f32 = 80.;
    /// The same for a cap under the water (`mWaterMaxRetSpeed`; the value is read off the
    /// underwater recording, `swim-lake-spincap`, where the return flies 18 then 20 a step).
    pub const WATER_RETURN_SPEED_MAX: f32 = 20.;
    /// Throw distance (`mMaxDist`) and braking time (`mBreakTime`).
    pub const RANGE: f32 = 500.;
    pub const BRAKE_FRAMES: u32 = 10;
    /// Most the return turns toward the player in one frame, in degrees (`ターン限界角度`, read
    /// by `exeReturn` at 0x407240 from `HackCapThrowParam + 0x48`). The value is the game's own:
    /// 40.0 in `ActorParam.byml` of `MarioCapCommonInfo.szs` (and in its `ActorParamMoon` and
    /// `ActorParamMoonBackUp` siblings), the archive the other throw parameters here come from.
    pub const TURN_LIMIT_DEGREES: f32 = 40.;
}
/// Where the cap appears, in front of the thrown-from point
/// (`HackCap::exeThrowStart` sets the position that far along the throw).
const START_DISTANCE: f32 = 150.;
/// The cap's `Attack` sensor (`PlayerFunction::initCapModelActor` 0x4464a4): radius 100, raised 20.
const ATTACK_SENSOR_RADIUS: f32 = 100.;
const ATTACK_SENSOR_HEIGHT: f32 = 20.;
/// `followTarget`: the interpolation runs 4 steps with weight 0.25 each; the other counter stops at 10.
const LOCK_ON_INTERPOLATION_STEPS: u32 = 4;
const LOCK_ON_WEIGHT: f32 = 0.25;
const LOCK_ON_COUNTER_LIMIT: u32 = 10;
/// `tryReturn` (0x3fd438): an ordinary target (`CapTargetInfo::_78` false) turns the cap's collision
/// off for three steps; the other kind (0x3fd40c) for 1800, not ported.
const RETURN_COLLISION_DELAY: i32 = 3;
/// Height of the player's head above the feet (`rs::getPlayerHeadPos`
/// while standing): the point the cap returns to.
const HEAD_HEIGHT: f32 = 110.;
/// The cap is caught when its `Catch` hit sensor (a sphere of 100 around the cap's centre,
/// added by `PlayerFunction::initCapModelActor`) touches a Player sensor: Body (50 at75)
/// or Head (40 at110). PlayerActorHakoniwa::attackSensor checks isSensorPlayer,
/// so both InitSensor entries participate. Contact is read after the cap has moved in the frame
/// and before the player's movement (`HackCap::receiveMsg` takes `PlayerCapCatch` and flags the
/// cap, the player's movement then puts it on), so a cap that flew into range this frame is
/// caught in it.
const CATCH_SENSOR_RADIUS: f32 = 100.;
const BODY_SENSOR_RADIUS: f32 = 50.;
const HEAD_SENSOR_RADIUS: f32 = 40.;
/// Height of the player's `Body` sensor above the feet (`PlayerActorHakoniwa/InitSensor`).
const CATCH_CENTER_HEIGHT: f32 = 75.;
/// The return only slows down this near the head: speed is limited to a
/// fraction of the distance there.
const RETURN_SLOW_DISTANCE: f32 = 200.;
const RETURN_SLOW_RATE: f32 = 0.8;
/// `tryCollideWallReaction` slides the cap along a wall it meets at an angle
/// whose cosine to the cap's heading is under this (30 degrees from head-on).
const SLIDE_LIMIT: f32 = 0.866_025_4;
/// Under this cosine to the wall the cap grazes it and raises no reaction
/// (sine of 5 degrees); a wall within 5 degrees of the last one counts as that
/// same wall (cosine of 5 degrees).
const GRAZE_LIMIT: f32 = 0.087_155_74;
const SAME_WALL: f32 = 0.996_194_7;
/// The radius of Cappy's collision sphere (`PlayerColliderHackCap`): a cap on the floor is that high.
const COLLIDER_RADIUS: f32 = 60.;
/// Frames the cap stays put after it was trampled (`exeTrample` compares its
/// step with 15).
const TRAMPLE_FRAMES: u32 = 15;
/// What `HackCap::receiveMsg` takes of a trample on its `Trample` sensor: the
/// vertical and horizontal distance between the player's foot sensor and its
/// own sensor must not exceed these.
const TRAMPLE_MAX_VERTICAL: f32 = 70.;
const TRAMPLE_MAX_HORIZONTAL: f32 = 200.;
/// The cap's `Trample` hit sensor (`PlayerFunction::initCapModelActor` 0x446678): a sphere of
/// 300 around local (0,20,0), turned with the cap's pose. The foot sensor's trample message is
/// tested against exactly this.
const TRAMPLE_SENSOR_RADIUS: f32 = 300.;
const TRAMPLE_SENSOR_Y: f32 = 20.;
/// The player's foot sensor (`PlayerActorHakoniwa/InitSensor`: `Foot`).
pub const FOOT_SENSOR_RADIUS: f32 = 40.;
pub const FOOT_SENSOR_Y: f32 = 40.;
/// `rs::isEnableSendTrampleMsg`: the foot must be within 70 degrees of straight
/// above the cap's sensor (cosine of 70 degrees).
const TRAMPLE_ANGLE_COS: f32 = 0.342_020_15;
/// `updateWaterArea` looks for the surface this far around the cap, and the cap counts as sunk
/// when the surface is at least `SUBMERGED_DEPTH` above it.
const WATER_FIND_DISTANCE: f32 = 100.;
const SUBMERGED_DEPTH: f32 = 30.;
/// The cap skims or rests on a surface that is closer than this, and a cap that has not sunk
/// floats up to this far above it, a quarter of the remaining way each step.
const SKIM_HEIGHT: f32 = 50.;
const FLOAT_RATE: f32 = 0.25;
/// What the throw nerves keep of the velocity across the front each step
/// (`al::scaleVelocityExceptDirection(cap, front, 0.94)`).
const ACROSS_DAMPING: f32 = 0.94;
/// `al::scaleVelocityExceptDirection`: the part of `velocity` along `dir` stays, the rest is scaled.
fn scale_except_direction(dir: Vec3, velocity: Vec3, scale: f32) -> Vec3 {
    let along = dir.x * velocity.x + dir.y * velocity.y + dir.z * velocity.z;
    let kept = Vec3::new(dir.x * along, dir.y * along, dir.z * along);
    let rest = velocity - kept;
    Vec3::new(
        scale * rest.x + kept.x,
        scale * rest.y + kept.y,
        scale * rest.z + kept.z,
    )
}
/// A replay-relative position in the frame the water areas are placed in.
fn stage_frame(mesh: Option<&TriangleMesh>, relative_cm: Vec3) -> Vec3 {
    mesh.map_or(relative_cm, |mesh| mesh.to_game_vec(relative_cm))
}
/// Speed is considered gone below this (`al::isNearZero`).
const NEAR_ZERO: f32 = 0.001;
/// Frames over which the return gives up the velocity it had (ease-in).
const RETURN_EASE_FRAMES: f32 = 10.;
/// The catch animation lasts this many frames; the cap hides when the nerve
/// has run longer than that. The cap plays the clip of the way the player
/// takes it (`PlayerActorHakoniwa::attackSensor`, the cap's catch message):
/// `CatchCap` standing, `CatchCapJumpParts` in the air, `CatchCapParts` on the
/// ground in any other state. The lengths are those of the cap's own clips
/// (`MarioCapAnimation`: `CatchCap` 5, `CatchCapParts` 2, `CatchCapJumpParts` 3, and
/// `JumpCapCatch` 2, which `HackCap::startCatch` plays for the air catch's pop).
pub fn catch_frames(clip: &str) -> u32 {
    match clip {
        "CatchCapJumpParts" => 3,
        "CatchCapParts" | "JumpCapCatch" => 2,
        _ => 5,
    }
}

/// One of the player's `Type: Player` hit sensors in the form the actor is in
/// (`PlayerActorHakoniwa/InitSensor.byml`): centre in centimetres, radius.
#[derive(Clone, Copy, Debug)]
pub struct PlayerSensor {
    pub center: Vec3,
    pub radius: f32,
}
/// What `HackCap::receiveMsg` asks of the player's side for `PlayerCapTouchJump`.
#[derive(Clone, Copy, Debug)]
pub struct TouchJumpGate {
    /// `PlayerInput::isHoldSpinCap`.
    pub hold: bool,
    /// `PlayerCapActionHistory::mIsCapJumpPossible`.
    pub jump_possible: bool,
    /// `PlayerWallActionHistory::mIsJumpStored`.
    pub wall_jump_stored: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapPhase {
    Out,
    Stay,
    Return,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapNerve {
    Throw,
    ThrowBrake,
    /// The throw down at the end of a ground pound (`exeThrowRolling`): the cap rolls away along
    /// the floor, then brakes (`exeThrowRollingBrake`).
    ThrowRolling,
    ThrowRollingBrake,
    /// The spin-move throw (`exeThrowSpiral`): the cap circles the player on a widening spiral.
    ThrowSpiral,
    ThrowStay,
    /// The player stood on the hovering cap by touch jump (`HackCap::exeTrample` under the
    /// nerve the recordings name `TrampleReturn`): it stays put for a short while, then goes
    /// back.
    TrampleReturn,
    /// The player's foot sensor trampled the cap (`rs::sendMsgPlayerCapTrample`, `HackCap::
    /// receiveMsg` 0x407e30): the same `exeTrample` under its own nerve, `Trample`. It ends
    /// like the other one here; the original lets it chain back into `ThrowStay` for a second
    /// player, which these single-player scenarios never exercise (GUESS: not modelled).
    Trample,
    Rebound,
    /// `exeLockOn`: the cap attached to a target (`CapHanger`), see `LockOn`.
    LockOn,
    Return,
}
impl CapNerve {
    /// The nerve a recorded cap state names, when it is one of the flight's.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "Throw" => Self::Throw,
            "ThrowBrake" => Self::ThrowBrake,
            "ThrowRolling" => Self::ThrowRolling,
            "ThrowRollingBrake" => Self::ThrowRollingBrake,
            "ThrowSpiral" => Self::ThrowSpiral,
            "ThrowStay" => Self::ThrowStay,
            "TrampleReturn" => Self::TrampleReturn,
            "Trample" => Self::Trample,
            "Rebound" => Self::Rebound,
            "LockOn" => Self::LockOn,
            "Return" => Self::Return,
            _ => return None,
        })
    }
    /// The nerve's name in the game's traces.
    pub fn name(self) -> &'static str {
        match self {
            Self::Throw => "Throw",
            Self::ThrowBrake => "ThrowBrake",
            Self::ThrowRolling => "ThrowRolling",
            Self::ThrowRollingBrake => "ThrowRollingBrake",
            Self::ThrowSpiral => "ThrowSpiral",
            Self::ThrowStay => "ThrowStay",
            Self::TrampleReturn => "TrampleReturn",
            Self::Trample => "Trample",
            Self::Rebound => "Rebound",
            Self::LockOn => "LockOn",
            Self::Return => "Return",
        }
    }
}

/// A cap as a recording shows it at one moment: the nerve it is in, the steps
/// it has run there, where it is, how fast it moves and the speed it was thrown at.
#[derive(Clone, Copy, Debug)]
pub struct Moment {
    pub nerve: CapNerve,
    pub step: u32,
    /// Centimetres, and centimetres per frame.
    pub spot: Vec3,
    pub velocity: Vec3,
    pub speed: f32,
    pub return_speed: f32,
    /// Where the cap was the row before its lock-on began (the saved matrix `followTarget`
    /// blends from).
    pub anchor: Vec3,
    /// The return run began after a lock-on (its collision is off for the first steps).
    pub after_lock_on: bool,
}

/// What the player hands the cap when it throws (`HackCap::startThrow`).
#[derive(Clone, Copy, Debug)]
pub struct Launch {
    /// The way the throw goes, horizontal and unit length.
    pub front: Vec3,
    pub kind: Throw,
    /// Thrown from the left hand (the clip names carry the side).
    pub left: bool,
    /// Thrown in the air (the down throw only rolls along the ground).
    pub air: bool,
    /// The third throw in a row, which flies faster.
    pub continuous: bool,
    /// Which way the spin-move throw circles (`HackCap::_268`, set by `startThrow` from the throw
    /// type's vector): -1 for the clockwise spin (`DoubleRight`), +1 for the counterclockwise one
    /// (`DoubleLeft`), 0 for every other throw.
    pub spiral_side: f32,
    /// The cap's up axis (`HackCap::startThrow` takes it from the thrower): the world's up, or the
    /// ground's normal for a throw from a slope under the water; the cap starts that high over the
    /// thrower's feet along it, and `front` lies across it.
    pub up: Vec3,
}

/// Moments of the flight the player and the presentation react to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CapEvent {
    /// The first step of the throw nerve: the throw's hit reaction and voice.
    Thrown,
    /// A step of the stay nerve: it starts a level sound on every step.
    Staying,
    /// The stay nerve chose the return (the reaction named after it).
    LeftStay,
    /// The first step of the stay nerve while the button is held: the
    /// reaction `Stay状態開始`.
    StayStarted,
    /// The first step of the trample nerve: the reaction `通常Trample`.
    Trampled,
    /// The cap reached the head.
    Caught,
    /// The cap met a wall at an angle and turned along it: the collision
    /// reaction, its effect and the sound `HitCol`, which is louder the
    /// squarer the hit (the cosine of the angle).
    WallHit(f32),
    /// The throw nerves' terrain helper (`0x401d0c`) turned the cap onto the surface its collider
    /// touched, by this many degrees: the same collision reaction and sound.
    TerrainTurn(f32),
    /// Accepted CapReflectCollide: index of the exported SignBoardNormal.
    Reflected(usize),
    /// An accepted `CapStartLockOn`: index of the exported CapHanger.
    LockedOn(usize),
    /// A rejected `CapKeepLockOn` made the cap return (`tryReturn`).
    LockReturn,
    /// The return ended away from the player (`exeReturn`'s tail, see `cap_return`): the cap
    /// hid itself (`HackCap::hide(false)`, 0x407584) and the player takes it back as in a catch.
    Hidden,
    /// The cap began to skim the water's surface (the reaction `水面移動`, its effect and sound).
    WaterFly,
    /// The cap began to rest on the water's surface (the reaction `水面待機`).
    WaterStay,
    /// Area membership changed with a surface on which to place the splash.
    WaterIn,
    WaterOut,
}

/// What the cap needs to know of the player and the stage each frame.
#[derive(Clone, Copy, Debug)]
pub struct CapWorld<'a> {
    /// Player position, metres.
    pub player: Vec3,
    /// The throw button is held (`PlayerInput::isHoldSpinCap`).
    pub hold: bool,
    /// The stage's triangles, when the replay has them.
    pub mesh: Option<&'a TriangleMesh>,
    /// The water areas of the stage (`HackCap::updateWaterArea` asks them), when there are any
    /// to ask.
    pub water: Option<&'a WaterAreas>,
    /// The exported actors the cap can lock onto (hangers, switches).
    pub targets: &'a [LockTarget],
}

/// What the cap needs of an exported lock-on target (centimetres, start-relative).
#[derive(Clone, Copy, Debug)]
pub struct LockTarget {
    /// The target matrix's translation (`CapTargetInfo`), as the actor poses it now.
    pub target: Vec3,
    /// The Body sensor.
    pub body: Vec3,
    pub body_radius: f32,
    /// Accepts `CapStartLockOn` (Wait) / `CapKeepLockOn` (Keep).
    pub waiting: bool,
    pub keeps: bool,
    /// The rotation of the target matrix when the target turns the cap (the switch's joint); the
    /// hanger's leaves the cap's pose as it was.
    pub rotation: Option<Quat>,
    /// The length of the target's start action when a wait action follows it (the hat goes on to
    /// `CapPointHookWait`); `None` for a start action that ends in nothing (the switch's).
    pub start_action_frames: Option<u32>,
}

/// The cap's attachment to a target (`prepareLockOn`, `followTarget`, `exeLockOn`).
#[derive(Clone, Copy, Debug)]
struct LockOn {
    index: usize,
    /// The saved matrix: where the cap was when the target accepted it.
    from: Vec3,
    target: Vec3,
    /// Counter `+0x584`: the target interpolation, approaching 4 by one a step.
    interpolation: u32,
    /// Counter `+0x580`: approaching 10 by one a step.
    counter: u32,
    /// The target's start action began (delayed while the cap still interpolates).
    started: bool,
    /// Steps since the start action began.
    age: u32,
    /// The target accepted the start in this very frame, so it keeps the cap in this frame's
    /// first execution before its own update shows the Keep nerve.
    fresh: bool,
}

/// What the cap keeps of the water (`HackCap::updateWaterArea`).
#[derive(Clone, Copy, Debug, Default)]
struct CapWater {
    /// The surface finder's last answer (`+0x170`).
    finder: SurfaceFinder,
    /// The cap is under the surface, at least `SUBMERGED_DEPTH` deep (`+0x29a`).
    submerged: bool,
    /// The splash bookkeeping (`+0x29c`, `+0x29d`): the surface was found when the cap last asked,
    /// and the water area it was last in.
    splash_found: bool,
    splash_area: bool,
    /// The skimming effect runs (`+0x30d`) and the resting effect (`+0x30e`).
    fly_effect: bool,
    stay_effect: bool,
    /// An effect started in this update (and not necessarily kept), and the ones that were
    /// started and deleted within it.
    fly_started: bool,
    stay_started: bool,
    fly_flash: bool,
    stay_flash: bool,
}

/// What `HackCap::changeThrowParamInWater` adds to the height of a cap that has not sunk: a
/// quarter of the way to `SKIM_HEIGHT` above the surface the finder found.
fn lift_toward_surface(finder: &SurfaceFinder) -> f32 {
    (finder.distance + SKIM_HEIGHT).max(0.) * FLOAT_RATE
}

/// The lift a throw nerve's step added to the height of a cap that began it at `spot` (centimetres,
/// replay-relative) and did not sink: zero out of the water areas, with no surface in reach, or
/// once sunk. A recording shows the lifted position, so the velocity the nerve held is the row's
/// displacement less this lift (it is added to the position, never to the velocity).
pub fn throw_water_lift(mesh: Option<&TriangleMesh>, water: &WaterAreas, spot: Vec3) -> f32 {
    let at = stage_frame(mesh, spot);
    let finder = water.update_finder(at, Vec3::Y, WATER_FIND_DISTANCE);
    if !finder.found || water.is_water(at) && Cap::deep(&finder) {
        return 0.;
    }
    lift_toward_surface(&finder)
}

/// What the rolling throw keeps: the up axis the model rolls about (`+0x24c`), how long and how
/// hard it flies, and the grounding effect (`+0x30c`: running, and called in this step).
#[derive(Clone, Copy, Debug)]
struct Rolling {
    up: Vec3,
    plan: cap_rolling::Plan,
    effect: bool,
    called: bool,
}

#[derive(Clone)]
pub struct Cap {
    /// Metres.
    pub pos: Vec3,
    /// Seconds since the throw, for presentation.
    pub age: f32,
    pub returning: bool,
    pub kind: Throw,
    air: bool,
    pub phase: CapPhase,
    /// The nerve the next frame runs and the frames it has run so far.
    pub nerve: CapNerve,
    step: u32,
    /// Frames since the cap appeared.
    pub frames: u32,
    /// The clip the cap plays (`FlyingWaitR`, `StayR`, ...), as the trace names it,
    /// how many frames it has played and which start of a clip it is (a later
    /// start of the same name restarts its sounds and effects).
    action: &'static str,
    action_frame: u32,
    action_serial: u32,
    left: bool,
    /// Centimetres, and centimetres per frame.
    spot: Vec3,
    velocity: Vec3,
    /// Direction of the throw, and the speed the braking started from.
    front: Vec3,
    /// The cap's up axis (`+0x24c`) and the steps the terrain helper has worked (`+0x28c`).
    up: Vec3,
    ground_counter: i32,
    /// The actor's pose (`HackCap`'s quaternion): facing back at the thrower while it
    /// flies and stays, then turned by the trample and the return.
    pose: CapPose,
    speed: f32,
    /// Frames the throw flies at full speed before it brakes, and the speed
    /// the brake takes off each frame (`_27c` and `_280`).
    cruise_frames: u32,
    brake_step: f32,
    /// The speed the return has built up (`_288`).
    return_speed: f32,
    /// Which way the spin-move throw circles (`_268.x`), and the direction its spiral turns
    /// from (`_300`, the throw direction the nerve's first step keeps).
    spiral_side: f32,
    curve_front: Vec3,
    /// The rolling throw's state (`_26c != 0`), when the cap is thrown down.
    rolling: Option<Rolling>,
    /// The continuous throw flies on the `FlyingWaitCapCatch` clip (`startThrow`'s first flag).
    capcatch: bool,
    /// Where the floor is when the replay has no stage collision (the player's feet at the
    /// throw), and whether the cap rests on it (the rolling nerves read that contact).
    floor_y: f32,
    floor_contact: bool,
    /// The player trampled the cap in this frame's sensor phase: the nerve the cap took is
    /// entered when it next updates (`al::setNerve`).
    trampled: Option<CapNerve>,
    /// `Stay状態開始` was raised (once per throw, while the button is held).
    stay_reaction_done: bool,
    /// Cappy's collider (`PlayerColliderHackCap`), against a triangle mesh.
    collider: PlayerCollider,
    /// The normal of the wall the last reaction turned along (`+0x514`) and
    /// whether the collider found a wall in the previous frame.
    wall_memory: Vec3,
    wall_last_frame: bool,
    /// A throw put back mid-flight (lockstep) has no collider history: the move of the row before
    /// it, from this spot with this velocity, is replayed when a source needs the contact.
    contact_probe: Option<(Vec3, Vec3)>,
    lock_on: Option<LockOn>,
    /// A cap put back into a lock-on (or the return after it) keeps the collider of the row before:
    /// the collision is off meanwhile, so the history is the throw's.
    carry_collider: bool,
    /// `tryReturn` turned the collision off for `+0x290` steps (`al::offCollide`); the return's
    /// executions count it down and turn it back on at zero (0x407030).
    collision_off_steps: i32,
    /// `exeReturn`'s wall counter (`+0x284`), cleared by its first execution.
    wall_hits: i32,
    /// The return ended (`hide`): the cap does not run any more.
    hidden: bool,
    /// The throw type is the rolling one (`+0x268` near zero and `+0x26c` not, `HackCap::
    /// isThrowTypeRolling` 0x400250): `startThrow` writes the pair and nothing clears it, not
    /// even the rebound, which does clear `rolling` here.
    rolling_throw: bool,
    water: CapWater,
}
impl Cap {
    /// Place a cap as `exeThrowStart` does on its one step: in front of the
    /// thrown-from point, flying at `speed`. The cap moves by that speed in
    /// the same frame.
    /// `serial` numbers the cap's first clip; later clips count on from it.
    pub fn start(launch: Launch, player: Vec3, serial: u32) -> Self {
        Self::start_in(launch, player, serial, None, None)
    }
    /// [`Self::start`] in a stage's collision: the cap's first move, in the frame it appears, is
    /// the collider's (a cap thrown down lands on the floor in it).
    pub fn start_in(
        launch: Launch,
        player: Vec3,
        serial: u32,
        mesh: Option<&TriangleMesh>,
        water: Option<&WaterAreas>,
    ) -> Self {
        let up = launch.up;
        // (A throw with an up axis of its own lies across it, tilted with it.)
        let front = if up == Vec3::Y {
            launch.front.with_y(0.).normalize_or_zero()
        } else {
            launch.front.normalize_or_zero()
        };
        let base = player / UNIT + up * param::THROW_HEIGHT;
        let spiral = launch.spiral_side != 0.;
        let rolled = launch.kind == Throw::Down;
        let spot = if spiral {
            cap_spiral::start_position(base, front, launch.spiral_side)
        } else {
            base + front * START_DISTANCE
        };
        let speed = if launch.continuous {
            param::CONTINUOUS_SPEED
        } else {
            param::MAX_SPEED
        };
        let brake_frames = param::BRAKE_FRAMES as f32;
        let mut cap = Self {
            pos: spot * UNIT,
            age: 0.,
            returning: false,
            kind: launch.kind,
            air: launch.air,
            phase: CapPhase::Out,
            nerve: if spiral {
                CapNerve::ThrowSpiral
            } else if rolled {
                CapNerve::ThrowRolling
            } else {
                CapNerve::Throw
            },
            step: 0,
            frames: 0,
            action: capcatch(
                thrown(launch.kind, launch.air, launch.left),
                launch.continuous,
            ),
            action_frame: 0,
            action_serial: serial,
            left: launch.left,
            spot,
            // (`exeThrowStart` sends the spiral off with no velocity: its nerve sets it.)
            velocity: if spiral { Vec3::ZERO } else { front * speed },
            front,
            up,
            ground_counter: 0,
            pose: CapPose::thrown_along(front),
            speed,
            cruise_frames: cruise_frames(speed),
            brake_step: -speed / brake_frames,
            return_speed: 0.,
            spiral_side: launch.spiral_side,
            curve_front: front,
            rolling: None,
            capcatch: launch.continuous,
            floor_y: player.y / UNIT,
            floor_contact: false,
            trampled: None,
            stay_reaction_done: false,
            collider: PlayerCollider::for_cap(),
            wall_memory: Vec3::ZERO,
            wall_last_frame: false,
            contact_probe: None,
            lock_on: None,
            carry_collider: false,
            collision_off_steps: 0,
            wall_hits: 0,
            hidden: false,
            rolling_throw: rolled,
            water: CapWater::default(),
        };
        if rolled {
            cap.begin_rolling(front);
        }
        // `rs::resetCollisionExpandCheck` came with the start's `resetPosition`.
        cap.collider.reset_expand_check(0.);
        cap.start_water(mesh, water);
        cap.advance(mesh);
        cap
    }
    /// `exeThrowStart` for a throw down: the cap sets off straight down at the rolling speed,
    /// and its up axis is the throw's side.
    fn begin_rolling(&mut self, front: Vec3) {
        let (direction, up) = cap_rolling::start_axes(front);
        let plan = cap_rolling::plan_down();
        self.front = direction;
        self.velocity = direction * cap_rolling::SPEED;
        self.pose = CapPose::rolling(direction, up);
        self.speed = cap_rolling::SPEED;
        self.rolling = Some(Rolling {
            up,
            plan,
            effect: false,
            called: false,
        });
    }
    /// Send the cap on another leg toward `direction` (the flick throw, which the
    /// original runs as `ThrowAppend`; here a fresh short throw).
    pub fn redirect(&mut self, direction: Vec3) {
        let Some(direction) = direction.try_normalize() else {
            return;
        };
        self.front = direction;
        self.velocity = direction * self.speed;
        self.nerve = CapNerve::Throw;
        self.step = 0;
        self.cruise_frames = 6;
        self.phase = CapPhase::Out;
        self.returning = false;
        self.play(flying(self.kind, self.air, self.left));
    }
    /// `HackCap::receiveMsg` for a trample on the `Trample` sensor
    /// (`rs::sendMsgPlayerCapTrample`): the player's foot sensor is at `foot`
    /// (centimetres). The cap takes it while it hovers (`ThrowStay`) with the
    /// button held, when the foot is within 70 degrees of straight above its
    /// sensor, the sensors touch, and the distance between them is at most 70
    /// vertically and 200 horizontally; the cap then stands still and returns
    /// (the nerve is entered when it next updates). `jump_possible` is
    /// `PlayerCapActionHistory`'s flag: one cap jump per time on the ground.
    pub fn take_trample(&mut self, foot: Vec3, hold: bool, jump_possible: bool) -> bool {
        if self.nerve != CapNerve::ThrowStay || self.trampled.is_some() || !hold || !jump_possible {
            return false;
        }
        let sensor = self.spot + self.pose.world_axis(Vec3::Y * TRAMPLE_SENSOR_Y);
        let between = foot - sensor;
        let reach = (TRAMPLE_SENSOR_RADIUS + FOOT_SENSOR_RADIUS).powi(2);
        let above = between.try_normalize().map_or(0., |dir| dir.y);
        if between.length_squared() >= reach
            || above <= TRAMPLE_ANGLE_COS
            || between.y.abs() > TRAMPLE_MAX_VERTICAL
            || between.with_y(0.).length() > TRAMPLE_MAX_HORIZONTAL
        {
            return false;
        }
        self.trampled = Some(CapNerve::Trample);
        true
    }
    /// `HackCap::receiveMsg` for `PlayerCapTouchJump` on the `Catch` sensor (0x407eb8..0x407f38,
    /// gates 0x4082dc..0x408414): any non-foot `Player` sensor of the player that touches the
    /// cap's `Catch` sphere (the sender, `PlayerActorHakoniwa::attackSensor` 0x428028, tests every
    /// such sensor against every cap sensor; only the `Catch` one answers this message).
    /// The cap takes it while it hovers (`ThrowStay`) with the throw button held
    /// (`isEnableCapTouchJumpInput`, ordinary play), when no wall jump is stored in
    /// `PlayerWallActionHistory` and the history still allows a cap jump. It then goes into the
    /// nerve the recordings name `TrampleReturn`: the ordinary hover state answers false to
    /// `isEnableKeepStayTouchJump` and `isEnableTouchJumpTransWarp` (0x40be18, 0x40be8c).
    ///
    /// Not ported: the stay-wall-hit window (`+0x530`/`+0x534`, set by `stayWallHit` 0x4098b4
    /// when the hovering cap meets a wall) lets the message through only while the player is off
    /// the ground (0x4083d8..0x408418), and `+0x294` is a counter that converges to zero and has no
    /// writer in the HackCap code. The cap's stay-wall reaction itself is not ported.
    pub fn take_touch_jump(&mut self, sensors: &[PlayerSensor], gate: TouchJumpGate) -> bool {
        if self.nerve != CapNerve::ThrowStay
            || self.trampled.is_some()
            || !gate.hold
            || !gate.jump_possible
            || gate.wall_jump_stored
        {
            return false;
        }
        let touches = sensors.iter().any(|sensor| {
            (sensor.center - self.spot).length() < CATCH_SENSOR_RADIUS + sensor.radius
        });
        if !touches {
            return false;
        }
        self.trampled = Some(CapNerve::TrampleReturn);
        true
    }
    /// The clip the throw's flight plays.
    fn flight_clip(&self) -> &'static str {
        capcatch(flying(self.kind, self.air, self.left), self.capcatch)
    }
    /// Start a clip over.
    fn play(&mut self, action: &'static str) {
        self.action = action;
        self.action_frame = 0;
        self.action_serial += 1;
    }
    /// The nerve the trace names.
    pub fn state_name(&self) -> &'static str {
        self.nerve.name()
    }
    /// The clip the cap plays.
    pub fn clip(&self) -> &'static str {
        self.action
    }
    /// The frame the clip shows (0 on the frame it starts): a clip that is not
    /// a loop holds its last frame.
    pub fn clip_frame(&self) -> f32 {
        let (length, looping) = clip_shape(self.action);
        if looping {
            (self.action_frame % length) as f32
        } else {
            self.action_frame.min(length) as f32
        }
    }
    /// A looping clip came back to its first frame this frame.
    pub fn clip_wrapped(&self) -> bool {
        let (length, looping) = clip_shape(self.action);
        looping && self.action_frame > 0 && self.action_frame.is_multiple_of(length)
    }
    /// Which start of a clip the cap's clip is.
    pub fn clip_serial(&self) -> u32 {
        self.action_serial
    }
    /// A cap put back into place each frame (lockstep) keeps the numbering of
    /// the clip it is in the middle of.
    pub fn continue_clip(&mut self, before: &Cap) {
        // (The spiral keeps circling from the throw direction it began with.)
        self.curve_front = before.curve_front;
        if self.nerve == CapNerve::ThrowSpiral && self.step == 0 {
            // exeThrowSpiral copies the retained launch front to its curve
            // on first execution (0x403a58..0x403a78), not the placement delta.
            self.front = before.curve_front;
        }
        // updateWaterArea retains its finder/splash/effect flags (0x400ba8).
        // ThrowStart selects the launch parameters (0x3ff998..0x3ffb8c);
        // surface displacement and blocked travel do not select a new throw.
        self.water = before.water;
        self.speed = before.speed;
        self.cruise_frames = before.cruise_frames;
        self.brake_step = before.brake_step;
        self.capcatch = before.capcatch;
        if matches!(self.nerve, CapNerve::Throw | CapNerve::ThrowBrake) {
            self.action = self.flight_clip();
        }
        if self.nerve == CapNerve::Return && before.nerve == self.nerve && before.step == self.step
        {
            // exeReturn keeps its prior flight action (0x406fc8), steers
            // from its model pose (0x4071f8..0x40735c), and retains +0x288's
            // speed scalar (0x407368..0x4073b8), apart from lateral momentum.
            // A matching completed row can retain these independently updated
            // values; an older row still uses resume's reconstruction.
            self.pose = before.pose;
            self.return_speed = before.return_speed;
            self.action = before.action;
            self.action_frame = before.action_frame;
        }
        let same_throw_step = before.nerve == self.nerve
            && before.step == self.step
            && matches!(
                self.nerve,
                CapNerve::Throw
                    | CapNerve::ThrowBrake
                    | CapNerve::ThrowRolling
                    | CapNerve::ThrowRollingBrake
                    | CapNerve::ThrowSpiral
            );
        if same_throw_step {
            // A row shows where the cap is, not the axes the terrain helper (0x401d0c) eases
            // from step to step: its up axis, the pose it gave the cap, the ground counter, and
            // the rolling plan. They are the cap's own, as the step just before this row left them.
            self.up = before.up;
            self.pose = before.pose;
            self.ground_counter = before.ground_counter;
            self.rolling = before.rolling;
        }
        if same_throw_step && self.step == 0 {
            // The first row of a throw has no row before it, so the motion it carries is only a
            // guess from the throw point. The cap this player has just started kept what
            // `HackCap::startThrow` gave it: the direction and the speed along it. The row's
            // spot stays the observed one.
            self.velocity = before.velocity;
            self.front = before.front;
            self.contact_probe = before.contact_probe;
        }
        // exeThrowStay's +0x2a9 is a per-throw latch (0x405d4c..0x405d6c),
        // not a new edge each time the actor is restored from a snapshot.
        self.stay_reaction_done = before.stay_reaction_done;
        self.action_serial = before.action_serial + u32::from(before.action != self.action);
        if before.action == self.action {
            self.action_frame = before.action_frame;
            if before.nerve == self.nerve {
                // A player re-choice injects an older row of this actor too.
                // Rewind the continuing action by the same nerve executions;
                // its update will then advance it once, as in the original.
                self.action_frame = self
                    .action_frame
                    .saturating_sub(before.step.saturating_sub(self.step));
            }
        }
        if self.carry_collider {
            self.collider = before.collider.clone();
            // The pose the target gave the cap is what the return that follows leaves along.
            self.pose = before.pose;
        }
        if matches!(self.nerve, CapNerve::Rebound | CapNerve::Return)
            && matches!(before.action, "ReboundStart" | "ReboundEnd")
        {
            self.action = before.action;
            self.action_frame = before.action_frame;
        }
    }
    /// Put the cap into a recorded moment (lockstep).
    pub fn resume(&mut self, moment: &Moment) {
        self.nerve = moment.nerve;
        self.step = moment.step;
        self.spot = moment.spot;
        self.pos = moment.spot * UNIT;
        self.velocity = moment.velocity;
        self.return_speed = moment.return_speed;
        self.phase = phase_of(moment.nerve);
        self.returning = moment.nerve == CapNerve::Return;
        self.carry_collider = moment.nerve == CapNerve::LockOn
            || moment.nerve == CapNerve::Return && moment.after_lock_on;
        if moment.nerve == CapNerve::Return && moment.after_lock_on {
            self.collision_off_steps = (RETURN_COLLISION_DELAY - moment.step as i32).max(0);
        }
        if moment.nerve == CapNerve::Throw && moment.velocity.length() > NEAR_ZERO {
            // The throw cruises at its speed along its front; a blocked move shows a shorter
            // step in the row, never a slower throw.
            self.contact_probe = Some((
                moment.spot - moment.velocity,
                moment.velocity.normalize() * moment.speed,
            ));
        }
        // The throw's speed fixes how long it cruises and how hard it brakes.
        self.speed = moment.speed;
        let brake_frames = param::BRAKE_FRAMES as f32;
        self.cruise_frames = cruise_frames(moment.speed);
        self.brake_step = -moment.speed / brake_frames;
        // The stay and the return start their clips on their first step, so
        // their first row still shows the clip before.
        let (action, frame) = match (moment.nerve, moment.step) {
            (CapNerve::Throw, step) => (self.flight_clip(), step),
            (CapNerve::ThrowBrake, step) => (self.flight_clip(), self.cruise_frames + 1 + step),
            (CapNerve::ThrowRolling, 0) => (thrown(self.kind, self.air, self.left), 0),
            (CapNerve::ThrowRolling, step) => ("FlyingWaitRolling", step - 1),
            (CapNerve::ThrowRollingBrake, step) => (
                "FlyingWaitRolling",
                self.rolling.map_or(0, |r| r.plan.rolling_steps) + 1 + step,
            ),
            (CapNerve::ThrowSpiral, 0) => (thrown(self.kind, self.air, self.left), 0),
            (CapNerve::ThrowSpiral, step) => ("FlyingWaitSpiral", step - 1),
            (CapNerve::ThrowStay, 0) => (self.flight_clip(), 0),
            (CapNerve::ThrowStay, step) => (stay(self.left), step - 1),
            (CapNerve::Trample | CapNerve::TrampleReturn, 0) => (stay(self.left), 0),
            (CapNerve::Trample | CapNerve::TrampleReturn, step) => ("Trample", step - 1),
            (CapNerve::Rebound, 0) => (self.flight_clip(), 0),
            (CapNerve::Rebound, step) => ("ReboundStart", step - 1),
            (CapNerve::LockOn, step) => (self.flight_clip(), step),
            (CapNerve::Return, 0) => (stay(self.left), 0),
            (CapNerve::Return, step) => (flying(self.kind, self.air, self.left), step - 1),
        };
        self.action = action;
        self.action_frame = frame;
        if moment.velocity.length() > NEAR_ZERO {
            self.front = moment.velocity.normalize();
            // (A cap put back in the middle of its return has no pose history: it
            // faces the way it was moving.)
            if moment.nerve == CapNerve::Return && moment.step > 0 {
                self.pose.stand_facing(self.front);
            }
        }
    }
    /// Put a cap into a recorded lock-on (lockstep): the hanger it keeps and the counters the
    /// recorded step implies. The row's position already is the blend of the step.
    pub fn restore_lock_on(&mut self, index: usize, view: &LockTarget, moment: &Moment) {
        let step = moment.step;
        let started = step > LOCK_ON_INTERPOLATION_STEPS;
        let age = step.saturating_sub(LOCK_ON_INTERPOLATION_STEPS + 1);
        self.lock_on = Some(LockOn {
            index,
            from: moment.anchor,
            target: view.target,
            interpolation: step.min(LOCK_ON_INTERPOLATION_STEPS),
            counter: step.min(LOCK_ON_COUNTER_LIMIT),
            started,
            age,
            fresh: false,
        });
        if started {
            self.action = "NoAction";
            self.action_frame = match view.start_action_frames {
                Some(frames) if age > frames => age - frames - 1,
                _ => age,
            };
        }
    }
    /// Run one frame: the current nerve, then the collider's move. Returns
    /// what happened.
    pub fn update(&mut self, world: &CapWorld) -> Vec<CapEvent> {
        if self.hidden {
            return Vec::new();
        }
        self.frames += 1;
        self.age = self.frames as f32 / crate::movement::FPS;
        self.water.fly_started = false;
        self.water.stay_started = false;
        self.water.fly_flash = false;
        self.water.stay_flash = false;
        self.action_frame += 1;
        let head = world.player / UNIT + Vec3::Y * HEAD_HEIGHT;
        let body = world.player / UNIT + Vec3::Y * CATCH_CENTER_HEIGHT;
        let mut events = Vec::new();
        if let (Some((from, velocity)), Some(mesh)) = (self.contact_probe.take(), world.mesh)
            && !mesh.signboards.is_empty()
        {
            self.collider.collide_tilted(mesh, from, velocity, 0., None);
        }
        // (The grounding effect's emit call is made by the steps that roll on a surface.)
        if let Some(rolling) = self.rolling.as_mut() {
            rolling.called = false;
        }
        if let Some(nerve) = self.trampled.take() {
            self.nerve = nerve;
            self.step = 0;
            self.phase = CapPhase::Stay;
        }
        // `HackCap::attackSensor` runs in the sensor phase before the nerve: an accepted lock-on
        // takes the nerve over for this very frame.
        if matches!(self.nerve, CapNerve::Throw | CapNerve::ThrowBrake)
            && let Some(index) = self.try_start_lock_on(world)
        {
            events.push(CapEvent::LockedOn(index));
        }
        let next = match self.nerve {
            CapNerve::Throw => self.throw(&mut events, world),
            CapNerve::ThrowBrake => self.throw_brake(&mut events, world),
            CapNerve::ThrowRolling => self.throw_rolling(),
            CapNerve::ThrowRollingBrake => self.throw_rolling_brake(),
            CapNerve::ThrowSpiral => {
                self.throw_spiral(world.player / UNIT, world.mesh, &mut events)
            }
            CapNerve::ThrowStay => self.throw_stay(world, &mut events),
            CapNerve::Trample | CapNerve::TrampleReturn => self.trample(&mut events),
            CapNerve::Rebound => self.rebound(),
            CapNerve::LockOn => self.lock_on(world, &mut events),
            CapNerve::Return => self.go_back(head, world, &mut events),
        };
        self.step += 1;
        // `prepareLockOn` stops the actor's collision: the nerve places the cap itself.
        let attached = self.nerve == CapNerve::LockOn;
        if let Some(next) = next {
            self.nerve = next;
            self.step = 0;
            self.phase = phase_of(next);
            self.returning = next == CapNerve::Return;
        }
        self.control_water_effects();
        if !attached {
            self.advance(world.mesh);
        }
        // The sensors touch where the cap has come to.
        // (A return that ended this frame hid the cap and turned its hit sensors off.)
        if self.nerve == CapNerve::Return
            && !self.hidden
            && ((self.spot - body).length() < CATCH_SENSOR_RADIUS + BODY_SENSOR_RADIUS
                || (self.spot - head).length() < CATCH_SENSOR_RADIUS + HEAD_SENSOR_RADIUS)
        {
            events.push(CapEvent::Caught);
        }
        events
    }
    /// The collider moves the cap by its velocity: in a stage's collision it
    /// stops at the surfaces it meets, otherwise the move is free.
    fn advance(&mut self, mesh: Option<&TriangleMesh>) {
        match mesh {
            Some(mesh) if self.collision_off_steps == 0 => {
                // A cap that rolls turns its collision shape with its pose.
                let tilt = self.rolling.map(|_| self.pose.read_back());
                let out = self
                    .collider
                    .collide_tilted(mesh, self.spot, self.velocity, 0., tilt);
                self.wall_last_frame = self.collider.wall.is_some();
                self.floor_contact = false;
                if out.collided {
                    self.spot = out.pos;
                } else {
                    self.spot += self.velocity;
                }
            }
            // (Without a stage, or with the collision turned off, the move is free.)
            _ => {
                self.spot += self.velocity;
                // No stage: the floor is the plane the player stood on, and a rolling cap rests
                // on it by the collider's radius.
                if self.rolling.is_some() {
                    let rest = self.floor_y + COLLIDER_RADIUS;
                    self.floor_contact = self.spot.y <= rest;
                    self.spot.y = self.spot.y.max(rest);
                }
            }
        }
        self.pos = self.spot * UNIT;
    }
    /// `HackCap::tryCollideWallReaction` for a wall the cap met on a face
    /// last frame, at the start of the throw and brake nerves: a wall at an
    /// angle (`0x409624`) turns the cap along it at the speed it had and moves
    /// it one centimetre into the wall (the collider pushes it back out); a
    /// head-on wall stops the cap in the original (`stayWallHit`), which is
    /// not reproduced here.
    fn wall_reaction(&mut self, events: &mut Vec<CapEvent>) {
        let Some(wall) = &self.collider.wall else {
            return;
        };
        if wall.location != Location::Face {
            return;
        }
        let Some(normal) = wall.normal.with_y(0.).try_normalize() else {
            return;
        };
        // The same wall as the last reaction's, met again right away, is no
        // new hit. The original decides this with a trigger word of the cap
        // that is not read here; "a wall last frame" is its likely meaning.
        let first = !self.wall_last_frame
            || self.wall_memory.length() < NEAR_ZERO
            || self.wall_memory.dot(normal) <= SAME_WALL;
        let alignment = self.front.dot(normal).abs();
        if alignment >= SLIDE_LIMIT {
            return;
        }
        let speed = self.velocity.length();
        let along = self.velocity - normal * self.velocity.dot(normal);
        let Some(front) = along.try_normalize() else {
            return;
        };
        self.spot -= normal;
        if alignment > GRAZE_LIMIT && first {
            events.push(CapEvent::WallHit(alignment));
        }
        self.front = front;
        self.velocity = front * speed;
        if first {
            self.wall_memory = normal;
        }
    }
    /// `HackCap::exeThrow`: fly on at the speed the throw set, until the
    /// cruise frames are used up.
    ///
    /// The nerve (0x401744) and `exeThrowBrake` (0x402ff8) make two calls after the terrain helper
    /// and `changeThrowParamInWater` that change nothing in single play with button throws, so
    /// they are not ported:
    /// - 0x402c4c steers the flying cap with the second player's stick (`calcCapSeparateMoveInput`
    ///   0x44dda8; it turns the front and keeps the speed). It returns at once when the cap's
    ///   byte `+0x299` is set, when byte 2 of the `PlayerSeparateCapFlag` word is set or when
    ///   its byte 0 is clear (0x402c78..0x402c8c; the cap takes the flag object in its
    ///   constructor and keeps it at `+0x148`). The actor's `movement` refreshes the word every
    ///   frame (0x41f544 -> 0x420438, also `startPlayerPuppet`, 0x421d1c): bytes 0 and 1 are
    ///   cleared (0x4204e4) unless `rs::isSeparatePlay` holds (0x42048c) together with the other
    ///   gates, and byte 0 is written 1 only past them (0x420600, 0x420610), so in single play
    ///   the answer is always false (see `throw_stay`).
    ///   In `exeThrowBrake` a true answer also skips the 0.94 damping, not only the added
    ///   velocity (0x40312c branches to 0x403174, past 0x40315c and 0x403170).
    /// - `addCurveOffset` 0x402e30 moves the cap sideways by `40 * (sin(pi * d1 / reach) -
    ///   sin(pi * d0 / reach))` along front x up (turned by the swing angle) times `+0x264`, the
    ///   side sign `startThrow` stores: `-1`/`+1` from the sign of the left/right vector it is
    ///   handed (0x3fc9f8, when `isThrowTypeLeftRight` holds, i.e. the vector is not near zero)
    ///   or of the stick vector's y (0x3fca1c, when that vector is not near zero). The left/right
    ///   vector is the attack's `+0x48`, which `setupAttackInfo` fills (0x464a7c..0x464a88)
    ///   only for the recorded judge modes 2 and 3 (0x464a54..0x464a64: the motion
    ///   controller's single-hand swings, `isTriggerCapSingleHandThrow` 0x44d0e0 ->
    ///   `isSwingAnyHand`; `PlayerJudgePreInputCapThrow::update` 0x45aa04..0x45aa5c stores the mode
    ///   2 or 3 and `getCapThrowDir` as the vector). Button throws record mode 1 with zero vectors
    ///   (0x45aa88..0x45aaac) and the stick vector is zeroed (0x464998), so for them the sign is
    ///   the 0 stored by 0x3fc9c8 and the offset is zero. The rewrite has no motion input: that
    ///   is a scope limit, not a property of the game, which runs the offset for the swings in
    ///   single play. The offset also adds the cap's speed to the distance flown `+0x274`, which
    ///   nothing else reads.
    fn throw(&mut self, events: &mut Vec<CapEvent>, world: &CapWorld) -> Option<CapNerve> {
        if self.step == 0 {
            events.push(CapEvent::Thrown);
        }
        // exeThrow 0x401834 checks the preceding collider before wall sliding.
        if let Some(next) = self.reflect_collision(world.mesh, events) {
            return Some(next);
        }
        self.wall_reaction(events);
        self.follow_terrain(world.mesh, events);
        self.pose.face_back_along(self.front, self.up);
        self.change_throw_param_in_water(world, events);
        // exeThrow goes on to two calls that do nothing in single play with button throws: 0x402c4c
        // (the second player's steering) and `addCurveOffset` 0x402e30; see the notes at
        // `throw`'s head.
        self.velocity = scale_except_direction(self.front, self.velocity, ACROSS_DAMPING);
        (self.step >= self.cruise_frames).then_some(CapNerve::ThrowBrake)
    }

    fn reflect_collision(
        &self,
        mesh: Option<&TriangleMesh>,
        events: &mut Vec<CapEvent>,
    ) -> Option<CapNerve> {
        let mesh = mesh?;
        // tryCollideReflectReaction 0x401914: ground, wall, then ceiling.
        for contact in [
            &self.collider.floor,
            &self.collider.wall,
            &self.collider.ceiling,
        ]
        .into_iter()
        .flatten()
        {
            let object = mesh.triangles[contact.triangle as usize].object as usize;
            if let Some(index) = mesh.signboards.iter().position(|s| s.object == object) {
                events.push(CapEvent::Reflected(index));
                return Some(CapNerve::Rebound);
            }
        }
        None
    }

    /// `HackCap::attackSensor` against an exported hanger (0x4076fc): the cap's Attack sensor (a
    /// sphere of 100 raised 20, `PlayerFunction::initCapModelActor`) strictly overlapping the hanger's
    /// Body and a hanger in Wait that accepts `CapStartLockOn` (`prepareLockOn` 0x3fbe40). The hanger
    /// accepts `CapIgnoreCollisionCheck` (0x2356c8), so the wall and ceiling arrow between the two
    /// sensors (0x4078d0) is never asked.
    fn try_start_lock_on(&mut self, world: &CapWorld) -> Option<usize> {
        let attack = self.spot + Vec3::Y * ATTACK_SENSOR_HEIGHT;
        let (index, target) = world.targets.iter().enumerate().find(|(_, h)| {
            h.waiting
                && (h.body - attack).length_squared()
                    < (ATTACK_SENSOR_RADIUS + h.body_radius).powi(2)
        })?;
        // The cap stops, enables only the trample sensor and starts the lock-on nerve.
        self.velocity = Vec3::ZERO;
        self.lock_on = Some(LockOn {
            index,
            from: self.spot,
            target: target.target,
            interpolation: 0,
            counter: 0,
            started: false,
            age: 0,
            fresh: true,
        });
        self.nerve = CapNerve::LockOn;
        self.step = 0;
        self.phase = phase_of(CapNerve::LockOn);
        Some(index)
    }

    /// `HackCap::exeLockOn` (0x3fe8c0) with `followTarget` (0x3fd7ec): the interpolation counter
    /// approaches 4 and blends the saved matrix toward the target with weight 0.25 per counter,
    /// the other counter approaches 10, the target's start action waits for the interpolation, and
    /// a target that no longer keeps the cap sends it home (`tryReturn` 0x3fd2e8).
    fn lock_on(&mut self, world: &CapWorld, events: &mut Vec<CapEvent>) -> Option<CapNerve> {
        let mut lock = self.lock_on?;
        // The target matrix follows the actor's model: it is read again by every execution.
        let view = world.targets.get(lock.index);
        if let Some(view) = view {
            lock.target = view.target;
        }
        let interpolating = lock.interpolation < LOCK_ON_INTERPOLATION_STEPS;
        if interpolating {
            lock.interpolation += 1;
            let weight = LOCK_ON_WEIGHT * lock.interpolation as f32;
            self.spot = lock.from + (lock.target - lock.from) * weight;
        } else {
            self.spot = lock.target;
        }
        self.pos = self.spot * UNIT;
        lock.counter = (lock.counter + 1).min(LOCK_ON_COUNTER_LIMIT);
        if !interpolating && let Some(rotation) = view.and_then(|view| view.rotation) {
            self.pose = CapPose::followed(rotation);
        }
        if !interpolating {
            if !lock.started {
                // The hat's start action begins; the cap's own action goes idle.
                lock.started = true;
                self.play("NoAction");
            } else {
                lock.age += 1;
                // `isActionEnd` of the start action is tested before the clip advances.
                if view
                    .and_then(|view| view.start_action_frames)
                    .is_some_and(|frames| lock.age == frames + 1)
                {
                    self.play("NoAction");
                }
            }
        }
        let kept = lock.fresh || view.is_some_and(|h| h.keeps);
        lock.fresh = false;
        self.lock_on = Some(lock);
        // Each execution asks the target to keep the cap; a refusal returns it.
        if !kept {
            self.lock_on = None;
            self.collision_off_steps = RETURN_COLLISION_DELAY;
            events.push(CapEvent::LockReturn);
            return Some(CapNerve::Return);
        }
        None
    }

    fn rebound(&mut self) -> Option<CapNerve> {
        // exeRebound 0x406c48..0x406dc8: ordinary single play. The entry
        // sample retains Throw velocity; only this nerve's first execution clears it.
        if self.step == 0 {
            self.play("ReboundStart");
            self.rolling = None;
            self.spiral_side = 0.;
            self.capcatch = false;
            self.velocity = Vec3::ZERO;
        }
        // 0x406f00..0x406f28 compares step>=3, independent of animation end.
        (self.step >= 3).then_some(CapNerve::Return)
    }
    /// `HackCap::exeThrowRolling`: roll along the surface at the throw's speed for the steps the
    /// distance allows, then brake.
    fn throw_rolling(&mut self) -> Option<CapNerve> {
        let plan = self.rolling?.plan;
        if self.step == 0 {
            // `tryStartActionIfNotPlaying`.
            self.try_play("FlyingWaitRolling");
        }
        self.roll_on_ground();
        self.velocity = cap_rolling::keep_along(self.velocity, self.front);
        (self.step >= plan.rolling_steps).then_some(CapNerve::ThrowRollingBrake)
    }
    /// `HackCap::exeThrowRollingBrake`: keep rolling, slowing by a fixed amount a step; the cap
    /// stays where it stops.
    fn throw_rolling_brake(&mut self) -> Option<CapNerve> {
        let plan = self.rolling?.plan;
        self.roll_on_ground();
        let brake = self.front * plan.brake_step;
        self.velocity = Vec3::new(
            self.velocity.x + brake.x,
            self.velocity.y + brake.y,
            self.velocity.z + brake.z,
        );
        self.velocity = cap_rolling::keep_along(self.velocity, self.front);
        cap_rolling::has_stopped(self.velocity, self.front).then_some(CapNerve::ThrowStay)
    }
    /// `HackCap::rollingGround`: turn the way the cap rolls along what it touches.
    fn roll_on_ground(&mut self) {
        let Some(mut rolling) = self.rolling else {
            return;
        };
        let contact = self.contact_normal();
        let rolled = cap_rolling::roll(self.front, rolling.up, self.velocity, contact);
        rolling.called = false;
        if let Some(rolled) = rolled {
            self.front = rolled.direction;
            self.velocity = rolled.velocity;
            self.pose
                .follow_rolling(rolled.direction, rolling.up, rolled.pose_rate);
            // (The airborne branch stops the grounding effect.)
            rolling.effect = rolled.grounded;
            rolling.called = rolled.grounded;
        }
        self.rolling = Some(rolling);
    }
    /// Start a clip unless it is already playing (`al::tryStartActionIfNotPlaying`).
    fn try_play(&mut self, action: &'static str) {
        if self.action != action {
            self.play(action);
        }
    }
    /// The grounding effect runs while the cap rolls on a surface and until it returns
    /// (`HackCap::control` deletes it then), and its emit call is made in the steps that touch one.
    pub fn grounding_effect(&self) -> (bool, bool) {
        self.rolling.map_or((false, false), |r| {
            (r.effect && self.nerve != CapNerve::Return, r.called)
        })
    }
    /// The normal of what the collider touched in its last move: the floor's, else a wall's, else
    /// the ceiling's (`rolling_ground` reads `rs::getCollided*Normal` in that order).
    fn contact_normal(&self) -> Option<Vec3> {
        if self.floor_contact {
            return Some(Vec3::Y);
        }
        [
            &self.collider.floor,
            &self.collider.wall,
            &self.collider.ceiling,
        ]
        .into_iter()
        .flatten()
        .next()
        .map(|contact| contact.normal)
    }
    /// `HackCap::exeThrowSpiral`: the cap chases a point that circles the player (see
    /// `cap_spiral`) and returns after 105 steps. The order is the nerve's: the point the cap
    /// circles, the terrain helper (`0x401d0c`, as the other throw nerves call it, with the
    /// curve's front carried along with the up axis it eases), then the step.
    fn throw_spiral(
        &mut self,
        feet: Vec3,
        mesh: Option<&TriangleMesh>,
        events: &mut Vec<CapEvent>,
    ) -> Option<CapNerve> {
        if self.step == 0 {
            self.curve_front = self.front;
            self.play("FlyingWaitSpiral");
            self.velocity = Vec3::ZERO;
        }
        // The point it circles follows the player (`+0x2f4`), above it along the cap's up axis.
        let pivot = Vec3::new(
            param::THROW_HEIGHT * self.up.x + feet.x,
            param::THROW_HEIGHT * self.up.y + feet.y,
            param::THROW_HEIGHT * self.up.z + feet.z,
        );
        let previous_up = self.up;
        self.follow_terrain(mesh, events);
        self.curve_front = cap_spiral::align_curve_front(self.curve_front, previous_up, self.up);
        // The helper ends by turning the cap to face back along the way it moved last step.
        self.pose.face_back_along(self.front, self.up);
        let step = cap_spiral::step(
            self.step,
            self.spot,
            pivot,
            self.curve_front,
            self.up,
            self.spiral_side,
        );
        self.velocity = step.velocity;
        if let Some(front) = step.front {
            self.front = front;
        }
        (self.step + 1 >= cap_spiral::FRAMES).then_some(CapNerve::Return)
    }
    /// `HackCap::exeThrowBrake`: take a tenth of the speed off each frame; the
    /// cap stays where it has stopped.
    fn throw_brake(&mut self, events: &mut Vec<CapEvent>, world: &CapWorld) -> Option<CapNerve> {
        if let Some(next) = self.reflect_collision(world.mesh, events) {
            return Some(next);
        }
        self.wall_reaction(events);
        self.follow_terrain(world.mesh, events);
        self.pose.face_back_along(self.front, self.up);
        self.change_throw_param_in_water(world, events);
        // (0x402c4c, and `addCurveOffset` after `tryAppendAttack`: see the notes at `throw`'s head.)
        self.velocity += self.front * self.brake_step;
        self.velocity = scale_except_direction(self.front, self.velocity, ACROSS_DAMPING);
        let stopped = self.velocity.length() < NEAR_ZERO;
        let reversed = self.velocity.dot(self.front) < 0.;
        (stopped || reversed || self.step >= param::BRAKE_FRAMES).then_some(CapNerve::ThrowStay)
    }
    /// The terrain helper the throw nerves share (see `cap_ground`).
    fn follow_terrain(&mut self, mesh: Option<&TriangleMesh>, events: &mut Vec<CapEvent>) {
        let touch = self.touched_surfaces();
        let turn = cap_ground::follow(
            cap_ground::Cap {
                spot: &mut self.spot,
                front: &mut self.front,
                up: &mut self.up,
                velocity: &mut self.velocity,
                counter: &mut self.ground_counter,
            },
            &touch,
            mesh,
            param::THROW_HEIGHT,
        );
        if let Some(degrees) = turn {
            events.push(CapEvent::TerrainTurn(degrees));
        }
    }

    /// What the helper asks the collider after its last move: `rs::isCollidedWallFace` (a wall
    /// met on a face) and `rs::isCollidedGround`, with their normals.
    fn touched_surfaces(&self) -> cap_ground::Touch {
        let wall_face = self
            .collider
            .collided_wall_face()
            .map(|wall| cap_ground::Surface {
                normal: wall.normal,
            });
        let ground = self
            .collider
            .collided_ground()
            .then_some(cap_ground::Surface {
                normal: self.collider.ground_normal,
            });
        cap_ground::Touch { wall_face, ground }
    }

    // ---- The water ---------------------------------------------------------------------------

    /// The first part of `exeThrowStart` that asks the water: the finder looks around the cap's
    /// first position and the water area is updated (`updateWaterArea`, which has no effect to
    /// start in the throw start's own nerve).
    fn start_water(&mut self, mesh: Option<&TriangleMesh>, water: Option<&WaterAreas>) {
        let Some(areas) = water else {
            return;
        };
        let at = stage_frame(mesh, self.spot);
        self.water.finder = areas.update_finder(at, Vec3::Y, WATER_FIND_DISTANCE);
        self.water.splash_found = self.water.finder.found;
        self.water.submerged = areas.is_water(at) && Self::deep(&self.water.finder);
    }

    /// `HackCap::updateWaterArea`: the cap's place in the water and, while it flies or stays
    /// within reach of the surface, the effect and reaction of skimming or resting on it.
    fn update_water_area(&mut self, world: &CapWorld, events: &mut Vec<CapEvent>) {
        let Some(areas) = world.water else {
            return;
        };
        let at = stage_frame(world.mesh, self.spot);
        let in_area = areas.is_water(at);
        let finder = areas.update_finder(at, Vec3::Y, WATER_FIND_DISTANCE);
        self.water.finder = finder;
        self.water.submerged = in_area && Self::deep(&finder);
        // (The throw nerves, and the return; not the trample.)
        let enabled = !self.water.splash_area
            && !self.water.submerged
            && !matches!(self.nerve, CapNerve::Trample | CapNerve::TrampleReturn);
        if enabled && let Some(skim) = self.surface_skim(&finder) {
            if skim {
                if !self.water.fly_effect {
                    self.water.stay_effect = false;
                    self.water.fly_effect = true;
                    self.water.fly_started = true;
                    events.push(CapEvent::WaterFly);
                }
            } else if !self.water.stay_effect {
                self.water.fly_effect = false;
                self.water.stay_effect = true;
                self.water.stay_started = true;
                events.push(CapEvent::WaterStay);
            }
        } else {
            // Out of reach of the surface (or in the water): both effects end.
            self.water.fly_effect = false;
            self.water.stay_effect = false;
        }
        // The splash bookkeeping: the cap that loses the surface forgets it, and a change of the
        // area it is in is recorded. A found surface supplies the effect matrix.
        if self.water.splash_found {
            if finder.found {
                return;
            }
            self.water.splash_found = false;
        }
        if self.water.splash_area != in_area {
            self.water.splash_found = finder.found;
            self.water.splash_area = in_area;
            if finder.found {
                events.push(if in_area {
                    CapEvent::WaterIn
                } else {
                    CapEvent::WaterOut
                });
            }
        }
    }

    /// The cap in a water area counts as sunk when no surface is within reach, or when the surface
    /// is at least `SUBMERGED_DEPTH` above it.
    fn deep(finder: &SurfaceFinder) -> bool {
        !finder.found || finder.distance >= SUBMERGED_DEPTH
    }

    /// Whether the cap skims the surface (`Some(true)`: moving over it, `Some(false)`: resting
    /// on it) or is not close enough for either (`None`): the surface within `SKIM_HEIGHT` of the
    /// cap, either way.
    fn surface_skim(&self, finder: &SurfaceFinder) -> Option<bool> {
        if !finder.found || finder.distance >= SKIM_HEIGHT {
            return None;
        }
        // Moving: the velocity has a direction across the gravity.
        let across = Vec3::new(self.velocity.x, 0., self.velocity.z);
        let (_, moving) = crate::movement::pose_math::try_normalize_or_zero(across);
        Some(moving)
    }

    /// `HackCap::changeThrowParamInWater`: a cap that has not sunk into the water is drawn up
    /// toward the surface (a quarter of the way to 50 above it each step) while it flies.
    fn change_throw_param_in_water(&mut self, world: &CapWorld, events: &mut Vec<CapEvent>) {
        if world.water.is_none() {
            return;
        }
        let was_submerged = self.water.submerged;
        self.update_water_area(world, events);
        if was_submerged || self.water.submerged || !self.water.finder.found {
            return;
        }
        self.spot.y += lift_toward_surface(&self.water.finder);
    }

    /// `HackCap::control`'s share: the skimming effect ends when the nerve is not a throw nor the
    /// return, the resting effect when it is not the stay (the nerve the frame leaves: a nerve
    /// chosen in this frame counts).
    fn control_water_effects(&mut self) {
        let throwing = matches!(
            self.nerve,
            CapNerve::Throw
                | CapNerve::ThrowBrake
                | CapNerve::ThrowRolling
                | CapNerve::ThrowRollingBrake
                | CapNerve::ThrowSpiral
                | CapNerve::ThrowStay
        );
        if self.water.fly_effect && !throwing && self.nerve != CapNerve::Return {
            self.water.fly_effect = false;
            self.water.fly_flash = self.water.fly_started;
        }
        if self.water.stay_effect && self.nerve != CapNerve::ThrowStay {
            self.water.stay_effect = false;
            self.water.stay_flash = self.water.stay_started;
        }
    }

    /// The cap's two surface effects while they run: skimming and resting (the effects the
    /// original emits by name).
    pub fn water_effects(&self) -> (bool, bool) {
        (self.water.fly_effect, self.water.stay_effect)
    }

    /// The same effects when they were started and deleted within the last update.
    pub fn water_flashes(&self) -> (bool, bool) {
        (self.water.fly_flash, self.water.stay_flash)
    }

    /// `HackCap::exeThrowStay`: hover for the stay time, longer while the
    /// button is held, then go back.
    ///
    /// The original's stay nerve carries a second copy of the terrain helper and a pose rebuild
    /// (`0x405660..0x405ae0`), and the port does not run them. They sit behind the
    /// `PlayerSeparateCapFlag` the player refreshes every frame (`0x420438`): the gate is
    /// `+0x299 == 0`, flag byte 0 non-zero and byte 2 zero. Byte 0 is only ever set to 1 after
    /// `rs::isSeparatePlay` answered true, the game's separate-play mode
    /// (`GameDataHolder::setSeparatePlay`, called by the pad-system update, the boot menu, a
    /// sequence change and `rs::changeSeparatePlayMode`), a second player's controller mode the
    /// port does not have. A single player never opens the gate, so the cap keeps the pose of its
    /// last throw step while it stays.
    fn throw_stay(&mut self, world: &CapWorld, events: &mut Vec<CapEvent>) -> Option<CapNerve> {
        let hold = world.hold;
        if self.step == 0 {
            self.play(stay(self.left));
        }
        self.update_water_area(world, events);
        events.push(CapEvent::Staying);
        if hold && !self.stay_reaction_done {
            self.stay_reaction_done = true;
            events.push(CapEvent::StayStarted);
        }
        if self.step <= param::STAY_FRAMES || hold && self.step <= param::STAY_FRAMES_MAX {
            return None;
        }
        events.push(CapEvent::LeftStay);
        Some(CapNerve::Return)
    }
    /// `HackCap::exeTrample`: face the way the joint does, start the trample
    /// clip, stand still for fifteen steps, then return.
    fn trample(&mut self, events: &mut Vec<CapEvent>) -> Option<CapNerve> {
        if self.step == 0 {
            self.face_joint_front();
            self.play("Trample");
            self.velocity = Vec3::ZERO;
            events.push(CapEvent::Trampled);
        }
        (self.step >= TRAMPLE_FRAMES).then_some(CapNerve::Return)
    }
    /// The first step of `exeTrample`: the actor turns to the front of its model's `AllRoot` joint
    /// (`al::calcJointFrontDir`, then `al::faceToDirection`). The joint matrix is the one the
    /// model last updated, which is the pose and the clip frame of the frame before.
    fn face_joint_front(&mut self) {
        let (length, looping) = clip_shape(self.action);
        let before = self.action_frame.saturating_sub(1);
        let frame = if looping {
            before % length
        } else {
            before.min(length)
        };
        let local = crate::cap_joint::all_root_front(self.action, frame as f32);
        let (front, found) = direction_of(self.pose.world_axis(local));
        if found {
            self.pose.face_to_direction(front);
        }
    }
    /// `HackCap::exeReturn` (0x406f4c): fly toward the head, a little faster every frame. The
    /// pose first leans a quarter of the way toward the head (only within `RETURN_SLOW_DISTANCE`
    /// of it, as the speed limit), then its front turns toward the head by at most the turn
    /// limit, and the cap flies along that front. The cap's front is the one it had when the
    /// return began: facing the player after a throw, the joint's front after a trample.
    ///
    /// The nerve writes the pose on every execution, the first one included, and nowhere else:
    /// the lean (`makeQuatFrontUp` 0x4071f8, `slerpQuat` at 0.25 0x407214, `updatePoseQuat`
    /// 0x407220) when the head is within 200 horizontally (0x40717c) and the pose's up axis is
    /// not parallel to the way to it (0x4071d8), then the stand (`makeQuatFrontUp` 0x407350,
    /// `updatePoseQuat` 0x40735c) unless the turned front is parallel to the up axis it takes
    /// (0x407330). Both need a way to the head (`tryNormalizeOrZero` 0x407104, gate 0x407108);
    /// without one the nerve goes straight to its tail (0x407434), which still runs.
    /// A frame in which the stay nerve hands over to this one runs the stay's body and not this
    /// one (`NerveKeeper::update`, 0x9596a4..0x9596dc, switches the nerve after the execute), so
    /// the trace row that already says `Return` at step 0 still holds the stay's pose.
    ///
    /// After the flight comes the tail (`end_return`, `cap_return.rs`): the wall counter and the
    /// ends of the return. `tryCollideWallLockOn` (0x407058, before the flight) answers true when
    /// the cap's collided wall belongs to a collision part whose owner accepts `CapStartLockOn`
    /// (`rs::tryGetCollidedWallSensor` 0x5660fc reads the wall hit's sensor, `+0x138` of the hit's
    /// collision part) and starts the lock-on nerve; the rewrite's lock-on targets (hangers and
    /// switches, `LockTarget`) have no collision part its meshes could hit (the loaders refuse
    /// `IsConnectToCollision`), so it always answers false here. The separate-play block that ends
    /// the nerve (0x4075a4..0x407648) is behind the gate that `throw_stay` documents.
    fn go_back(
        &mut self,
        head: Vec3,
        world: &CapWorld,
        events: &mut Vec<CapEvent>,
    ) -> Option<CapNerve> {
        if self.collision_off_steps > 0 {
            self.collision_off_steps -= 1;
        }
        self.update_water_area(world, events);
        if self.step == 0 {
            // `tryStartActionIfNotPlaying`: a cap coming out of the spiral keeps its clip.
            let action = if self.action == "ReboundStart" {
                // exeReturn 0x406f74..0x406f98.
                "ReboundEnd"
            } else if self.spiral_side != 0. {
                "FlyingWaitSpiral"
            } else {
                thrown(Throw::Forward, self.air, self.left)
            };
            self.try_play(action);
            // The one 8-byte store at +0x284 clears the return speed and the wall counter.
            self.return_speed = 0.;
            self.wall_hits = 0;
        }
        let to_head = head - self.spot;
        let (direction, found) = direction_of(to_head);
        if found {
            self.fly_toward(to_head, direction);
        }
        self.end_return(if found { direction } else { Vec3::ZERO }, events);
        None
    }
    /// The flight of `exeReturn` (0x40710c..0x407430) toward a head `to_head` away, in
    /// `direction`.
    fn fly_toward(&mut self, to_head: Vec3, direction: Vec3) {
        if horizontal_length(to_head) < RETURN_SLOW_DISTANCE {
            self.return_speed = self
                .return_speed
                .clamp(0., (length(to_head) * RETURN_SLOW_RATE).max(0.));
            self.pose.lean_toward(direction);
        }
        // `turnFrontSpherical` writes nothing when its turn gives up (never in the traced
        // returns); the out vector then keeps what the nerve gave it, `sead::Vector3f::ez`
        // (0x407224..0x407238).
        let front = self
            .pose
            .front_turned_toward(direction, param::TURN_LIMIT_DEGREES)
            .unwrap_or(Vec3::Z);
        // A throw type of rolling (`startThrow` stores `Vector2f::ey`, 0x3fca94, so +0x268 is zero
        // and +0x26c is not: the test at 0x40729c..0x4072b0, `isThrowTypeRolling`) keeps its up
        // axis across the new front, like a wheel (0x4072b4..0x40731c); the other throws stand
        // upright (0x407284).
        if self.rolling_throw {
            self.pose.stand_facing_across(front);
        } else {
            self.pose.stand_facing(front);
        }
        self.front = front;
        // (A cap under the water returns at the water's top speed.)
        let top = if self.water.submerged {
            param::WATER_RETURN_SPEED_MAX
        } else {
            param::RETURN_SPEED_MAX
        };
        self.return_speed = (self.return_speed + param::RETURN_STRENGTH).min(top);
        // What the cap still moves across its new heading fades out.
        let across = self.velocity - self.front * self.velocity.dot(self.front);
        let ease = ease_in((self.step as f32 / RETURN_EASE_FRAMES).min(1.));
        self.velocity = self.front * self.return_speed + across * (1. - ease);
    }
    /// The tail of `exeReturn` (0x407434..0x40764c), see `cap_return.rs`: the wall counter from
    /// what the collider touched in its last move, then the step, counter and target ends.
    /// `direction` is the way to the head this execution (zero without one).
    fn end_return(&mut self, direction: Vec3, events: &mut Vec<CapEvent>) {
        let normal = if let Some(wall) = self.collider.collided_wall() {
            wall.normal
        } else if let Some(ceiling) = self.collider.collided_ceiling() {
            ceiling.normal
        } else if self.collider.collided_ground() {
            self.collider.ground_normal
        } else {
            Vec3::ZERO
        };
        self.wall_hits = cap_return::wall_counter(self.wall_hits, normal, direction);
        // The binary's third end, the capture-transfer target within 100 (flag +0x2a4, set by
        // `receiveRequestTransferHack` 0x3fc190; the constant at 0x187fcb8), is not ported: the
        // port has no capture transfer, so the flag is never set.
        if !cap_return::ends(self.step, self.wall_hits) {
            return;
        }
        // `hide` leaves the cap hidden, its hit sensors off and its velocity zero
        // (`setVelocityZero` 0x3fa8f4).
        self.hidden = true;
        self.velocity = Vec3::ZERO;
        events.push(CapEvent::Hidden);
    }
}
/// The cap's flight state, without the collider's bookkeeping: the simulation
/// baseline (`original_simulation_trace`) hashes this text, and the collider
/// only acts against a stage mesh.
impl std::fmt::Debug for Cap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cap")
            .field("pos", &self.pos)
            .field("age", &self.age)
            .field("returning", &self.returning)
            .field("kind", &self.kind)
            .field("air", &self.air)
            .field("phase", &self.phase)
            .field("nerve", &self.nerve)
            .field("step", &self.step)
            .field("frames", &self.frames)
            .field("action", &self.action)
            .field("action_frame", &self.action_frame)
            .field("action_serial", &self.action_serial)
            .field("left", &self.left)
            .field("spot", &self.spot)
            .field("velocity", &self.velocity)
            .field("front", &self.front)
            .field("pose", &self.pose.quat())
            .field("speed", &self.speed)
            .field("cruise_frames", &self.cruise_frames)
            .field("brake_step", &self.brake_step)
            .field("return_speed", &self.return_speed)
            .finish()
    }
}
/// A throw this fast is the continuous one (the third in a row).
pub fn is_continuous_speed(speed: f32) -> bool {
    speed > param::MAX_SPEED + 0.5
}
/// Frames the throw cruises before it brakes: until the braking distance (a
/// triangle, half the speed over the braking time) is all that is left of the
/// range.
fn cruise_frames(speed: f32) -> u32 {
    let brake_frames = param::BRAKE_FRAMES as f32;
    ((param::RANGE - speed * brake_frames / 2.) / speed)
        .floor()
        .max(0.) as u32
}
fn phase_of(nerve: CapNerve) -> CapPhase {
    match nerve {
        CapNerve::Throw
        | CapNerve::ThrowBrake
        | CapNerve::ThrowRolling
        | CapNerve::ThrowRollingBrake
        | CapNerve::ThrowSpiral => CapPhase::Out,
        CapNerve::ThrowStay
        | CapNerve::Trample
        | CapNerve::TrampleReturn
        | CapNerve::Rebound
        | CapNerve::LockOn => CapPhase::Stay,
        CapNerve::Return => CapPhase::Return,
    }
}
/// The clip of a cap in flight: the spiral and rolling throws have their own,
/// the others tell the hands apart.
fn flying(kind: Throw, air: bool, left: bool) -> &'static str {
    match (kind, air) {
        (Throw::Spin, _) => "FlyingWaitSpiral",
        (Throw::Down, false) => "FlyingWaitRolling",
        _ if left => "FlyingWaitL",
        _ => "FlyingWaitR",
    }
}
/// The clip `exeThrowStart` starts: the hand's, whatever the throw (the spiral and rolling nerves
/// switch to their own clip on their first step).
fn thrown(kind: Throw, air: bool, left: bool) -> &'static str {
    match kind {
        Throw::Spin | Throw::Down => {
            if left {
                "FlyingWaitL"
            } else {
                "FlyingWaitR"
            }
        }
        _ => flying(kind, air, left),
    }
}
/// `FlyingWaitR`/`L` of the continuous throw are the `FlyingWaitCapCatch` clips (recorded: the
/// third throw in a row).
fn capcatch(clip: &'static str, continuous: bool) -> &'static str {
    match (continuous, clip) {
        (true, "FlyingWaitR") => "FlyingWaitCapCatchR",
        (true, "FlyingWaitL") => "FlyingWaitCapCatchL",
        _ => clip,
    }
}
fn stay(left: bool) -> &'static str {
    if left { "StayL" } else { "StayR" }
}
/// Frames of a clip of the cap's animation archive and whether it loops
/// (`MarioCapAnimation`: the flying clips run 11 frames, the stay's loop 9, the continuous throw's
/// flying loop 12).
fn clip_shape(name: &str) -> (u32, bool) {
    // MarioCapAnimation original clip lengths (cap-clips source inventory).
    if name == "NoAction" {
        (17, false)
    } else if name == "ReboundStart" {
        (3, false)
    } else if name == "ReboundEnd" {
        (7, false)
    } else if name.starts_with("Stay") {
        (9, true)
    } else if name.starts_with("FlyingWaitCapCatch") {
        // The continuous throw's flying clip loops (12 frames, recorded: cap-throw-triple).
        (12, true)
    } else if name == "Trample" {
        (TRAMPLE_FRAMES, false)
    } else {
        (11, false)
    }
}
/// `al::easeIn`.
fn ease_in(x: f32) -> f32 {
    (-0.5 * x + 1.5) * x * x
}
#[path = "tests/cap.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
