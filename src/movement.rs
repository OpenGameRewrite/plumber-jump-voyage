//! Player controller modelled on the original game's player states.
//!
//! It runs at the original's 60 frames per second in its distance unit (one
//! unit is a centimetre; Mario is 160 tall) and its tuning values. Positions
//! are stored in metres for the rest of the game. States that the public
//! decompilation documents follow it; the rest are inferred from their tuning
//! names and observed behaviour. No original code is reproduced here.
use crate::air_move::{self, AirMove, ContinuousJump, ContinuousLongJump};
use crate::cap::{CapEvent, CapWorld, Launch};
use crate::ceiling_check::{self, CeilingCheck};
use crate::collider::PlayerCollider;
use crate::geometry::{CollisionCodes, TriangleMesh};
use crate::input::Orders;
use crate::nerve::Nerve;
use crate::se_ctrl::SeCtrl;
use crate::spin_input::SpinInput;
use bevy::prelude::*;
use std::sync::Arc;

/// Metres per original distance unit.
pub const UNIT: f32 = 0.01;
/// The fall speed at which the fall distance starts counting (`FallSpeedMax`).
pub(crate) const TERMINAL_FALL_SPEED: f32 = t::FALL_MAX;
/// Simulation rate, matching the original.
pub const FPS: f32 = 60.;
pub const RADIUS: f32 = 0.45;
/// How close to a surface, in metres, a fall counts as having reached it. The
/// box solids of the demo course follow the rule the traces show and the
/// game's collider explains (`collider.rs`): an upright body is held by three
/// arrows that only collide once the feet have gone below the surface (a fall
/// that ends exactly on it lands a row later: backflip, side somersault, triple
/// jump), a crouched body by a sphere whose lowest point is the feet, which
/// touches on arrival (the roll's hop). Positions are single-precision metres,
/// so "exactly" is within this much (`terrain.rs` takes the same grid).
const SURFACE_GRID: f32 = 1e-5;
/// Circumradius (cm) of the hexagonal poles the toy course places, the size
/// of the Cap Kingdom's climbable pole (`pole-climb` recording).
pub const POLE_CIRCUMRADIUS: f32 = 25.;
pub const HEIGHT: f32 = 1.6;
const SQUAT_HEIGHT: f32 = 0.9;
pub const SPAWN: Vec3 = Vec3::new(0.0, 0.0, 5.0);
/// Full-tilt running speed in metres per second.
pub const RUN: f32 = t::NORMAL_MAX * UNIT * FPS;

/// Tuning in original units per frame, following the original player constants.
pub mod player_constants;
use player_constants::PlayerConstants;

mod t {
    pub const GRAVITY: f32 = 3.;
    pub const NORMAL_MAX: f32 = 14.;
    pub const NORMAL_MIN: f32 = 3.;
    pub const ACCEL_FRAMES: f32 = 40.;
    pub const BRAKE_FRAMES: f32 = 10.;
    pub const DASH_JUDGE: f32 = 14.5;
    pub const AFTER_TURN_MAX: f32 = 17.;
    pub const AFTER_TURN_FRAMES: u32 = 30;
    pub const BRAKE_TURN_START: u32 = 5;
    pub const BRAKE_ON_SPEED_RATE: f32 = 0.5;
    pub const BRAKE_ON_COUNTER: u32 = 10;
    pub const QUICK_TURN_JUMP_FRAMES: u32 = 20;
    pub const ROUND_LIMIT_DEG: f32 = 8.5;
    pub const ROUND_LIMIT_DEG_MIN: f32 = 6.5;
    /// `getSlerpQuatRate` and `getHillPoseDegreeMax`: how fast the pose's up
    /// follows the floor's normal, and how far from vertical it may tilt.
    pub const SLERP_QUAT_RATE: f32 = 0.15;
    /// `getSlerpQuatRateWait` and `getWaitPoseDegreeMax`: the standing pose's.
    pub const SLERP_QUAT_RATE_WAIT: f32 = 0.15;
    pub const WAIT_POSE_DEGREE_MAX: f32 = 45.;
    pub const HILL_POSE_DEGREE_MAX: f32 = 45.;
    pub const RUN_SPEED_CONTINUOUS_THROW: f32 = 16.;
    pub const RUN_TIME_CONTINUOUS_THROW: u32 = 20;
    pub const RUN_ACCEL_FRAMES_CONTINUOUS_THROW: f32 = 60.;

    pub const EXTEND_FRAMES: u32 = 10;
    pub const JUMP_POWER_MIN: f32 = 17.;
    pub const JUMP_POWER_MAX: f32 = 19.5;
    pub const JUMP_MOVE_MIN: f32 = 11.;
    pub const JUMP_MOVE_MAX: f32 = 30.;
    pub const JUMP_ACCEL_FRONT: f32 = 0.5;
    pub const JUMP_ACCEL_BACK: f32 = 1.;
    pub const PRE_INPUT_JUMP: u32 = 5;
    pub const CONTINUOUS_JUMP_TIMER: u32 = 10;
    pub const CONTINUOUS_JUMP_POWER_MIN: f32 = 19.5;
    pub const JUMP_POWER_MAX_2ND: f32 = 21.;
    pub const JUMP_POWER_3RD: f32 = 25.;
    pub const FALL_MAX: f32 = 35.;
    pub const LONG_FALL: f32 = 3000.;

    pub const SQUAT_JUMP_POWER: f32 = 32.;
    pub const SQUAT_JUMP_BACK: f32 = 5.;
    pub const SQUAT_JUMP_MOVE_MAX: f32 = 9.;
    pub const TURN_JUMP_POWER: f32 = 32.;
    pub const TURN_JUMP_VEL_H: f32 = 9.;

    pub const LONG_JUMP_ACCEL: f32 = 0.25;
    pub const LONG_JUMP_BRAKE: f32 = 0.5;
    pub const LONG_JUMP_SIDE_ACCEL: f32 = 0.25;
    pub const LONG_JUMP_POWER: f32 = 12.;
    pub const LONG_JUMP_MOVE_POW: f32 = 4.;
    pub const LONG_JUMP_INIT_SPEED: f32 = 14.;
    pub const LONG_JUMP_SPEED: f32 = 23.;
    pub const LONG_JUMP_SPEED_MIN: f32 = 2.5;
    pub const CONTINUOUS_LONG_JUMP_TIMER: u32 = 15;

    pub const SPIN_JUMP_POWER: f32 = 20.;
    pub const SPIN_JUMP_MOVE_MAX: f32 = 8.;
    pub const SPIN_DOWN_FALL_POWER: f32 = 1.5;
    pub const SPIN_DOWN_FALL_MAX: f32 = 45.;
    pub const SPIN_DOWN_FALL_FRAME_MIN: u32 = 5;
    pub const GROUND_SPIN_FRAMES: u32 = 90;
    pub const GROUND_SPIN_MOVE_MAX: f32 = 8.;
    pub const GROUND_SPIN_ACCEL_RATE: f32 = 0.5;
    pub const GROUND_SPIN_BRAKE_RATE: f32 = 0.95;

    pub const HIP_DROP_LAND_CANCEL: u32 = 24;
    pub const HIP_DROP_JUMP_BEGIN: u32 = 5;
    pub const HIP_DROP_JUMP_END: u32 = 30;

    pub const HEAD_SLIDING_SPEED_MIN: f32 = 2.5;
    pub const HEAD_SLIDING_BRAKE: f32 = 0.5;
    pub const HEAD_SLIDING_SIDE_ACCEL: f32 = 0.125;

    pub const ROLL_START: f32 = 20.;
    pub const ROLL_BOOST: f32 = 30.;
    pub const ROLL_MAX: f32 = 35.;
    pub const ROLL_FRAMES_MIN: i32 = 45;
    pub const ROLL_FRAMES_MIN_BOOST: i32 = 30;
    pub const ROLL_START_JUMP: f32 = 12.;
    pub const ROLL_SLOPE_ACCEL: f32 = 0.6;
    pub const ROLL_BRAKE: f32 = 0.998;
    pub const ROLL_END_SPEED: f32 = 17.;
    /// `SlopeRollingEndBrake` and `SlopeRollingEndBrakeEndSpeed`: the winding
    /// down roll's brake and the speed under which it ends.
    pub const ROLL_END_BRAKE: f32 = 0.95;
    pub const ROLL_END_BRAKE_END_SPEED: f32 = 10.;
    pub const ROLL_SIDE_ACCEL: f32 = 0.4;
    pub const ROLL_UNROLL_FRAMES: u32 = 5;
    pub const ROLL_AGAINST: f32 = 0.5;
    pub const ROLL_ANGLE_POWER_MAX: f32 = 30.;
    pub const ROLL_SIDE_BRAKE: f32 = 0.985;
    pub const ROLL_SIDE_MAX: f32 = 10.;
    pub const ROLL_RESTART_MAX_ADD: f32 = 3.;
    pub const ROLL_RESTART_CHARGE: i32 = 40;
    pub const ROLL_RESTART_FORCE: i32 = 60;
    pub const ROLL_ANIM_SPEED_MIN: f32 = 5.;
    pub const ROLL_ANIM_SPEED_MAX: f32 = 35.;
    pub const ROLL_ANIM_RATE_MIN: f32 = 0.2;
    pub const ROLL_ANIM_RATE_MAX: f32 = 1.;
    /// Ground this steep or steeper is not stood on (`StandAngleMin`).
    pub const STAND_ANGLE_MIN: f32 = 60.;
    pub const ROLL_RESTART_ACCEL: f32 = 6.;
    pub const ROLL_RESTART_INTERVAL: i32 = 15;
    /// Frames in the air after which a roll's touchdown raises its reaction.
    pub const ROLL_LAND_AIR_FRAMES: i32 = 20;

    pub const JUMP_ACCEL_TURN: f32 = 0.3;
    pub const SLOPE_SLIDE_ANGLE_START: f32 = 26.;
    pub const SLOPE_SLIDE_ANGLE_END: f32 = 10.;
    pub const SLOPE_SLIDE_ACCEL: f32 = 0.3;
    pub const SLOPE_SLIDE_BRAKE: f32 = 0.9;
    pub const SLOPE_SLIDE_MAX: f32 = 22.;
    pub const SLOPE_SLIDE_SPEED_END: f32 = 3.;
    pub const SLOPE_SLIDE_SIDE_ACCEL: f32 = 0.6;
    pub const SLOPE_SLIDE_SIDE_BRAKE: f32 = 0.98;
    pub const SLOPE_SLIDE_SIDE_MAX: f32 = 10.;
    pub const SLOPE_TURN_DEG: f32 = 5.;
    pub const SLOPE_FORCE_FRAMES: u32 = 30;
    pub const SQUAT_BRAKE_END: f32 = 3.5;
    pub const SQUAT_ACCEL_RATE: f32 = 1.2;
    pub const SQUAT_BRAKE_RATE: f32 = 0.95;
    pub const SQUAT_WALK: f32 = 3.5;

    /// `GravityWallSlide`.
    pub const WALL_KEEP_FRAMES: u32 = 3;
    /// `FallWallScaleVelocity`.
    pub const FALL_WALL_SCALE_VELOCITY: f32 = 0.5;
    pub const WALL_JUMP_POWER: f32 = 23.;
    pub const WALL_JUMP_H: f32 = 8.6;

    /// The jump off a trampled cap (`PlayerStateJump::appear`, request 11):
    /// `TrampleHighJumpPower` and `TrampleHighGravity`.
    pub const TRAMPLE_HIGH_JUMP_POWER: f32 = 25.;
    /// Share of the horizontal speed a rising player keeps when he tramples
    /// with the stick pushed (`TrampleRisingBrakeVelH`).
    pub const TRAMPLE_RISING_BRAKE: f32 = 0.3;
    pub const CAP_CATCH_POP_POWER: f32 = 10.;
    pub const CAP_CATCH_JUMP_POWER: f32 = 22.;
    pub const CAP_CATCH_WINDOW: u32 = 10;
    pub const PRE_INPUT_THROW: u32 = 10;

    pub const SPIN_CAP_THROW_FRAME: u32 = 12;
    pub const SPIN_CAP_THROW_FRAME_AIR: u32 = 8;
    pub const SPIN_CAP_THROW_FRAME_SWING: u32 = 7;
    pub const SPIN_CAP_THROW_FRAME_CONTINUOUS: u32 = 2;
    pub const SPIN_AIR_JUMP: f32 = 6.;
    pub const SPIN_AIR_SPEED_MAX: f32 = 7.;
    pub const SPIN_AIR_GRAVITY: f32 = 0.3;
    pub const SPIN_BRAKE_FRAMES: u32 = 15;
    pub const SPIN_BRAKE_RATE: f32 = 0.95;
    pub const SPIN_ROUND_LIMIT_DEGREE: f32 = 5.;
}

/// The angle a thrown button gives the spine's swing (`PlayerSpinCapAttack` `+0x58`): only the
/// left/right throw gesture sets one, and the rewrite's input has none.
const BUTTON_THROW_SWING_ANGLE: f32 = 0.;

/// The throw direction helper (0x4831b8) leaves a move input alone when it points back (along
/// the actor's negated front) within 15 degrees: the limit of the band it clamps (cos 15
/// degrees, float at 0x187fcd0).
const THROW_BACK_COS: f32 = 0.965_925_8;

#[path = "movement/cap_ground.rs"]
pub(crate) mod cap_ground;
#[path = "movement/cap_pose.rs"]
pub(crate) mod cap_pose;
#[path = "movement/cap_return.rs"]
pub(crate) mod cap_return;
#[path = "movement/cap_rolling.rs"]
pub(crate) mod cap_rolling;
#[path = "movement/cap_spiral.rs"]
pub(crate) mod cap_spiral;
#[path = "movement/cap_throw.rs"]
pub(crate) mod cap_throw;
/// What a pose calculation advances (the controllers that keep state between frames).
pub type ControllerState = (
    ground_pose::GroundPose,
    look_at::LookAt,
    hip_spring::HipSpring,
);
mod abyss;
mod actor_collider;
pub mod air_current;
mod bind;
mod cap_catch_pop;
mod cap_sources;
mod cap_touch;
pub mod cap_trampoline;
pub mod carry;
#[path = "movement/console_libm.rs"]
pub(crate) mod console_libm;
pub mod conveyor_parts;
mod damage;
mod damage_fire;
mod damage_life;
mod damage_swim;
pub mod death;
pub mod dimension;
pub mod dokan;
mod entrance_camera;
pub mod external_velocity;
mod force_run;
#[path = "movement/grab_ceil.rs"]
pub mod grab_ceil;
#[path = "movement/grab_ceil_find.rs"]
mod grab_ceil_find;
#[path = "movement/ground.rs"]
mod ground;
mod ground_inertia;
mod grow_seed;
pub mod key_move_parts;
pub mod kick_stone;
pub mod killer;
mod on_ground;
mod press;
pub mod pukupuku;
mod push_receiver;
mod recovery_dead;
pub mod rocket_flower;
pub mod rotate_parts;
mod safety;
pub mod sand_geyser;
mod sand_sink;
mod spin_flower;
mod stain;
mod surface_run;
use damage::Damage;
use damage_fire::FireControl;
pub use stain::Soot;
pub mod cap_hanger;
pub mod cap_switch;
#[path = "movement/ground_move.rs"]
pub(crate) mod ground_move;
#[path = "movement/ground_pose.rs"]
pub(crate) mod ground_pose;
#[path = "movement/hip_spring.rs"]
pub(crate) mod hip_spring;
#[path = "movement/input_2d.rs"]
mod input_2d;
#[path = "movement/joint_controls.rs"]
pub(crate) mod joint_controls;
#[path = "movement/joint_math.rs"]
pub(crate) mod joint_math;
#[path = "movement/ledge.rs"]
mod ledge;
#[path = "movement/look_at.rs"]
pub(crate) mod look_at;
#[path = "movement/look_joint.rs"]
pub(crate) mod look_joint;
#[path = "movement/move_input.rs"]
pub(crate) mod move_input;
#[path = "movement/plane.rs"]
pub mod plane;
#[path = "movement/pole.rs"]
mod pole;
#[path = "movement/pole_find.rs"]
mod pole_find;
#[path = "movement/pole_pose.rs"]
pub(crate) mod pole_pose;
#[path = "movement/pose_math.rs"]
pub(crate) mod pose_math;
#[path = "movement/rolling.rs"]
mod rolling;
#[path = "movement/run_2d.rs"]
mod run_2d;
#[path = "movement/run_cycle.rs"]
mod run_cycle;
pub mod sign_board;
#[path = "movement/slide_control.rs"]
mod slide_control;
#[path = "movement/slope_slide.rs"]
mod slope_slide;
#[path = "movement/snap.rs"]
mod snap;
#[path = "movement/sounds.rs"]
mod sounds;
#[path = "movement/spin.rs"]
mod spin;
#[path = "movement/spin_air_pose.rs"]
mod spin_air_pose;
#[path = "movement/spin_cap.rs"]
mod spin_cap;
#[path = "movement/spin_cap_appear.rs"]
mod spin_cap_appear;
#[path = "movement/squat.rs"]
mod squat;
#[path = "movement/swim.rs"]
mod swim;
#[path = "movement/swim_const.rs"]
mod swim_const;
pub use swim_const::SPIN_CAP_THROW_FRAME as SWIM_SPIN_CAP_THROW_FRAME;
#[path = "movement/swim_dive.rs"]
mod swim_dive;
#[path = "movement/swim_dive_in.rs"]
mod swim_dive_in;
#[path = "movement/swim_ground.rs"]
mod swim_ground;
#[path = "movement/swim_move.rs"]
mod swim_move;
#[path = "movement/swim_pose.rs"]
pub(crate) mod swim_pose;
#[path = "movement/swim_spin_cap.rs"]
mod swim_spin_cap;
#[path = "movement/swim_trample.rs"]
pub(crate) mod swim_trample;
#[path = "movement/terrain.rs"]
mod terrain;
#[path = "movement/turn_control.rs"]
pub(crate) mod turn_control;
#[path = "movement/vector_lib.rs"]
pub(crate) mod vector_lib;
#[path = "movement/wait.rs"]
mod wait;
#[path = "movement/wall_air.rs"]
mod wall_air;
#[path = "movement/wall_along.rs"]
mod wall_along;
mod wall_push;
#[path = "movement/wall_slide.rs"]
pub(crate) mod wall_slide;
#[path = "movement/wet.rs"]
mod wet;
pub use ground::turn_power;
use ground::{GRAVITY_MOVE, GroundMove, PivotTurn, STICK_DEADZONE, SteerInput};
/// `PlayerConst::mGravity` (`PlayerConst.cpp:6`), the value of the constant's slot 0 (`getGravity`).
const PLAYER_CONST_GRAVITY: f32 = 3.;
use ledge::WallCatchControl;
use plane::along_vector_normal_h;
use pole::PoleControl;
use rolling::{RollControl, RollStart};
pub use run_cycle::RunCycle;
use slope_slide::SlopeSlide;
use squat::SquatControl;
use wait::WaitControl;
pub use wall_air::WallContact;
use wall_air::{WallAirControl, WallHistory};

/// Steps the probe's player has stood before its teleport (it waits for 120 in `Wait`).
const PROBE_STANDING_STEPS: u32 = 120;

/// How much of the way to its target direction the body swings in a frame
/// (`SlerpQuatRate`, used by `rs::slerpUpFront`).
const SLERP_QUAT_RATE: f32 = 0.15;

/// The nerves of the original's run state. Running (also after turning) is
/// the walk/run modes; braking is `Skid`; turning and the pivot are `Turn`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunNerve {
    Run,
    RunAfterTurn,
    WallPush,
    Brake,
    Turn,
    Pivot,
}

/// The speed under which a braking run just stops: the run border less its blend range.
const RUN_BLEND_FLOOR: f32 = 6.;
/// A stick pushed beyond this much counts as held deep.
const DEEP_STICK: f32 = 0.8;
/// The toy course's boxes let the player walk up steps this high (metres);
/// the stage collision has its own rules.
const WALKABLE_STEP: f32 = 0.24;

/// The clip a throw starts with (`PlayerSpinCapAttack`'s clip name builder
/// for a throw without motion controls): the plain throw, the second and
/// third in a row, and the spin (spiral) and up or down throws.
fn throw_clip(
    kind: Throw,
    air: bool,
    count: u32,
    continuous: bool,
    counterclockwise: bool,
) -> &'static str {
    match (kind, air, count, continuous) {
        (Throw::Up, false, _, _) => "SpinCapStartDoubleUp",
        (Throw::Up, true, _, _) => "SpinCapAirStartDoubleUp",
        (Throw::Down, false, _, _) => "SpinCapStartDoubleDown",
        (Throw::Down, true, _, _) => "SpinCapAirStartDoubleDown",
        (Throw::Spin, false, _, _) if counterclockwise => "SpinCapStartDoubleLeft",
        (Throw::Spin, true, _, _) if counterclockwise => "SpinCapAirStartDoubleLeft",
        (Throw::Spin, false, _, _) => "SpinCapStartDoubleRight",
        (Throw::Spin, true, _, _) => "SpinCapAirStartDoubleRight",
        (Throw::Forward, false, 1, false) => "SpinCapStart",
        (Throw::Forward, false, 2, false) => "SpinCapStart2Right",
        (Throw::Forward, false, _, _) => "SpinCapStart3Right",
        (Throw::Forward, true, 1, false) => "SpinCapAirStart",
        // The builder spells the second and third throw out of the ground
        // clip's name even in the air.
        (Throw::Forward, true, 2, false) => "SpinCapStart2Right",
        (Throw::Forward, true, _, _) => "SpinCapStart3Right",
    }
}

/// `HackCap::_268.x` of a throw: the spin-move throw circles clockwise (`DoubleRight`, -1) or
/// counterclockwise (`DoubleLeft`, +1) with the stick's spin (`exeRun` and `exeJump` set the
/// trigger bit 28 for a clockwise spin and 27 for the other, which `setupAttackInfo` turns into the
/// throw vector's sign); every other throw has none.
fn spiral_side_of(kind: Option<Throw>, counterclockwise: bool) -> f32 {
    match kind {
        Some(Throw::Spin) if counterclockwise => 1.,
        Some(Throw::Spin) => -1.,
        _ => 0.,
    }
}

/// The last frame of the upper-body catch animation: the model's clip of that
/// name runs frames 0 to its length (`CatchCapParts` 10, `CatchCapJumpParts` 12).
fn upper_catch_last(clip: &str) -> i32 {
    match clip {
        "CatchCapJumpParts" => 12,
        _ => 10,
    }
}

/// Frame counts of clips whose end the controller waits for.
pub mod clip {
    /// The pipe's out clip (`DokanOut`): the controller waits for its last frame.
    pub const DOKAN_OUT: u32 = 21;
    /// Original converted Mario/PlayerAnimation DokanSideOut skeletal length.
    pub const DOKAN_SIDE_OUT: u32 = 58;
    pub const HIP_DROP_START: u32 = 24;
    pub const HIP_DROP_LAND: u32 = 44;
    pub const ROLLING_START: u32 = 6;
    pub const ROLLING_END: u32 = 12;
    pub const ROLLING_LAND: u32 = 20;
    pub const ROLLING_STAND_UP: u32 = 9;
    pub const HEAD_SLIDING_START: u32 = 48;
    pub const LAND_ROLLING: u32 = 36;
    /// The idle sequence's one-shot clips (`WaitRelaxStart`, `SnoozeStart`,
    /// `SleepStart`).
    pub const WAIT_RELAX_START: u32 = 150;
    pub const SNOOZE_START: u32 = 507;
    pub const SLEEP_START: u32 = 608;
    /// Lengths of the cap-throw clips (the spin family).
    pub fn throw(name: &str) -> u32 {
        match name {
            "SpinCapStartLeft" | "SpinCapStartRight" => 17,
            "SpinCapStart2Left" | "SpinCapStart2Right" => 32,
            "SpinCapStart3Left" | "SpinCapStart3Right" => 42,
            "SpinCapStartDoubleUp" | "SpinCapStartDoubleDown" | "SpinCapAirStartDoubleUp" => 26,
            "SpinCapAirStartDoubleDown" => 28,
            "SpinCapStartDoubleLeft" | "SpinCapStartDoubleRight" => 44,
            "SpinCapAirStartLeft" | "SpinCapAirStartRight" => 70,
            "SpinCapAirStart" | "SpinCapAirStartDoubleLeft" | "SpinCapAirStartDoubleRight" => 72,
            _ => 22,
        }
    }
    /// Length of the jump clips that stay on their last frame while the
    /// state goes on (the jump state never switches them to the fall clip).
    pub fn held_length(name: &str) -> Option<u32> {
        match name {
            "SpinJumpStart" => Some(26),
            "GrabCeilStart" => Some(51),
            "GrabCeilJump" => Some(46),
            "SquatStart" => Some(3),
            "DamageDown" | "NoDamageDown" => Some(92),
            "DamageLand" => Some(78),
            "DamageDownSwim" | "NoDamageDownSwim" => Some(140),
            "DamageDownSwimSurface" | "NoDamageDownSwimSurface" => Some(53),
            "DamageSwimSurfaceLand" => Some(145),
            // (The fall's clip is 28 frames long and its counter stops one past that.)
            "Fall" => Some(28),
            "JumpDashFast" => Some(30),
            "Jump" | "JumpReverse" => Some(84),
            "Jump2" => Some(42),
            "Jump3" | "JumpBack" => Some(68),
            "JumpTurn" => Some(60),
            "JumpBroad" | "JumpBroad2" | "JumpBroad3" => Some(38),
            "JumpHipDrop" => Some(46),
            // The pipe clips (the converted originals' frame counts).
            "DokanSideIn" => Some(56),
            "DokanOut" => Some(DOKAN_OUT),
            "DokanIn" => Some(54),
            "DokanReady" => Some(4),
            "DokanSideOut" => Some(DOKAN_SIDE_OUT),
            _ => None,
        }
    }
    /// Length of the clips that loop (their frame wraps).
    pub fn loop_length(name: &str) -> Option<f32> {
        match name {
            "SpinJumpLoop" => Some(23.),
            "GrabCeilWait" => Some(90.),
            "Wait" | "Snooze" => Some(180.),
            "WaitRelax" => Some(1200.),
            "WaitHot" => Some(340.),
            "WaitCold" => Some(410.),
            "WaitVeryCold" => Some(36.),
            "DamageWait" => Some(120.),
            "FireRun" => Some(60.),
            "SandWait" | "SandWalk" | "SandWaitNormal" => Some(80.),
            "SandWaitStrong" => Some(30.),
            "Sleep" | "SquatWait" => Some(120.),
            "SquatWalk" => Some(36.),
            "Rolling" => Some(9.),
            // The ledge side moves: 20 frames, run once over each move.
            "WallCatchMoveL" | "WallCatchMoveR" => Some(20.),
            // The spin family's clips cycle: the ground spin and the drop
            // are 10 frames long, the spin jump's body 8.
            "SpinGroundL" | "SpinGroundR" | "SpinJumpDownFallL" | "SpinJumpDownFallR" => Some(10.),
            // The pole's clips (recorded `anim_len`): the climb, the slide
            // down, the turn round it and the handstand's wait and turn.
            "PoleClimb" => Some(25.),
            "PoleFall" => Some(21.),
            "PoleTurn" => Some(50.),
            "PoleHandStandWait" => Some(120.),
            "PoleHandStandTurn" => Some(23.),
            // (Not recorded: the converted model's length.)
            "PoleWait" => Some(180.),
            "StartSpinJumpL" | "StartSpinJumpR" | "RestartSpinJumpL" | "RestartSpinJumpR" => {
                Some(8.)
            }
            // The wall slide's clip (recorded `anim_len` 48, force-waterfall-airpass: the counter
            // shows 48 at row 119 and 1 at row 120). Only the left-hand clip is recorded.
            "WallSlideL" => Some(48.),
            // The swim clips (recorded `anim_len`).
            "SwimStandSurface" => Some(46.),
            "SwimSurfaceNormal" => Some(38.),
            "SwimStandWait" => Some(140.),
            "SwimWalk" => Some(68.),
            "SwimHeadSlidingIn" | "SwimHeadSliding" => Some(20.),
            _ => None,
        }
    }
    /// How far past its length a looping clip's counter runs before it wraps
    /// (the layer's own frame wraps at the length): the side moves' counter
    /// reads 21.03 in the step after the clip loops and 2.08 in the next.
    pub fn loop_slack(name: &str) -> f32 {
        match name {
            "WallCatchMoveL" | "WallCatchMoveR" => 2.,
            // The pole's turning clips show their length for a row (50, then 1;
            // 23, then 1) before they wrap.
            "PoleFall" | "PoleTurn" | "PoleHandStandTurn" | "PoleHandStandWait" | "PoleWait" => 1.,
            // The swim's loops show their length for a row (38, then 1).
            "SpinJumpLoop" | "GrabCeilWait" | "SandWait" | "SandWalk" | "SwimStandSurface"
            | "SwimSurfaceNormal" | "SwimStandWait" | "SwimWalk" | "WaitCold" | "WaitVeryCold"
            | "WallSlideL" => 1.,
            // The spin jump's body clips (length 8) read 1..8 in every recorded row, never 0
            // after the start: updateSync (0x42d170) gives the counter the model's frame plus
            // its rate, so it shows the length for the row the pose wraps in.
            "StartSpinJumpL" | "StartSpinJumpR" | "RestartSpinJumpL" | "RestartSpinJumpR" => 1.,
            _ => 0.,
        }
    }
    /// The swim's clips (recorded `anim_len`).
    pub const SWIM_STAND: u32 = 32;
    pub const SWIM_LAND: u32 = 45;
    pub const SWIM_HIP_DROP_START: u32 = 30;
    pub const SWIM_SPIN_CAP_START: u32 = 50;
    pub const SWIM_TRAMPLE: u32 = 24;
    pub const SWIM_HIP_DROP_LAND: u32 = 74;
    pub const LAND: u32 = 26;
    /// The landing after a long fall (`PlayerStateWait`'s `LandStiffen` nerve).
    pub const LAND_STIFFEN: u32 = 120;
    pub const LAND_JUMP3: u32 = 70;
    pub const SQUAT_END: u32 = 13;
    pub const TURN: u32 = 12;
    pub const RUN_START: u32 = 30;
    pub const BRAKE: u32 = 5;
    /// `JumpCapCatchAir` plays this clip (`ActionAnimCtrl.byml` alias).
    pub const JUMP_CAP_CATCH_COMMON: u32 = 27;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Move {
    #[default]
    Idle,
    Walk,
    Run,
    Skid,
    Turn,
    /// The squat nerve's braking sub-state: the crouch that slides on its
    /// momentum. Only here can a long jump start.
    SquatBrake,
    Crouch,
    Crawl,
    Fall,
    Jump,
    DoubleJump,
    TripleJump,
    LongJump,
    Backflip,
    Sideflip,
    SpinJump,
    SpinFlower,
    SpinDrop,
    GroundSpin,
    GroundPound,
    PoundLand,
    PoundJump,
    Roll,
    RollEnd,
    RollJump,
    Dive,
    SlopeSlide,
    BurnJump,
    BurnBounce,
    FireRun,
    FireFall,
    FireDead,
    SandWait,
    SandRun,
    SandPivot,
    SandJump,
    SandDead,
    SandGeyser,
    Press,
    DamageAir,
    DamageLand,
    DamageDead,
    DamageSwim,
    DamageSurface,
    DamageLandWater,
    DamageSwimDead,
    Dead,
    AbyssFall,
    RecoveryStart,
    RecoveryTravel,
    RecoveryFall,
    WallSlide,
    WallJump,
    Ledge,
    Pole,
    GrabCeil,
    Swim,
    CapBounce,
    CatchJump,
    /// `PlayerStateCapCatchPop`: the pop after a cap caught in the air.
    CapCatchPop,
    Throw,
    /// `PlayerStateBind`: a pipe's puppet controller owns the pose.
    Bind,
    /// `PlayerStateBind` EndFall: the puppet has ended and the air control takes over.
    BindFall,
}
impl Move {
    /// Every state, for mapping names back to states.
    pub const ALL: [Move; 63] = [
        Self::Idle,
        Self::Walk,
        Self::Run,
        Self::Skid,
        Self::Turn,
        Self::SquatBrake,
        Self::Crouch,
        Self::Crawl,
        Self::Fall,
        Self::Jump,
        Self::DoubleJump,
        Self::TripleJump,
        Self::LongJump,
        Self::Backflip,
        Self::Sideflip,
        Self::SpinJump,
        Self::SpinFlower,
        Self::SpinDrop,
        Self::GroundSpin,
        Self::GroundPound,
        Self::PoundLand,
        Self::PoundJump,
        Self::Roll,
        Self::RollEnd,
        Self::RollJump,
        Self::Dive,
        Self::SlopeSlide,
        Self::BurnJump,
        Self::BurnBounce,
        Self::FireRun,
        Self::FireFall,
        Self::FireDead,
        Self::SandWait,
        Self::SandRun,
        Self::SandPivot,
        Self::SandJump,
        Self::SandDead,
        Self::SandGeyser,
        Self::Press,
        Self::DamageAir,
        Self::DamageLand,
        Self::DamageDead,
        Self::DamageSwim,
        Self::DamageSurface,
        Self::DamageLandWater,
        Self::DamageSwimDead,
        Self::Dead,
        Self::AbyssFall,
        Self::RecoveryStart,
        Self::RecoveryTravel,
        Self::RecoveryFall,
        Self::WallSlide,
        Self::WallJump,
        Self::Ledge,
        Self::Pole,
        Self::GrabCeil,
        Self::Swim,
        Self::CapBounce,
        Self::CatchJump,
        Self::CapCatchPop,
        Self::Throw,
        Self::Bind,
        Self::BindFall,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Ready",
            Self::Walk => "Walk",
            Self::Run => "Run",
            Self::Skid => "Skid",
            Self::Turn => "Turn",
            Self::SquatBrake | Self::Crouch => "Crouch",
            Self::Crawl => "Crawl",
            Self::Fall => "Fall",
            Self::Jump => "Jump",
            Self::DoubleJump => "Double jump",
            Self::TripleJump => "Triple jump",
            Self::LongJump => "Long jump",
            Self::Backflip => "Backflip",
            Self::Sideflip => "Side somersault",
            Self::SpinJump => "Spin jump",
            Self::SpinFlower => "Trampoline spin jump",
            Self::SpinDrop => "Spin drop",
            Self::GroundSpin => "Spin",
            Self::GroundPound | Self::PoundLand => "Ground pound",
            Self::PoundJump => "Ground pound jump",
            Self::Roll | Self::RollEnd => "Roll",
            Self::RollJump => "Roll jump",
            Self::Dive => "Dive",
            Self::SlopeSlide => "Slope slide",
            Self::BurnJump => "Hot!",
            Self::BurnBounce => "Hot! (bounce)",
            Self::FireRun => "Fire run",
            Self::FireFall => "Fire fall",
            Self::SandWait | Self::SandRun | Self::SandPivot | Self::SandJump => "Quicksand",
            Self::SandGeyser => "Sand geyser",
            Self::Press
            | Self::SandDead
            | Self::FireDead
            | Self::DamageDead
            | Self::DamageSwimDead
            | Self::Dead
            | Self::AbyssFall => "Dead",
            Self::DamageAir | Self::DamageLand => "Damage",
            Self::DamageSwim | Self::DamageSurface | Self::DamageLandWater => "Water damage",
            Self::RecoveryStart | Self::RecoveryTravel | Self::RecoveryFall => "Recovery",
            Self::WallSlide => "Wall slide",
            Self::WallJump => "Wall jump",
            Self::Ledge => "Ledge hang",
            Self::GrabCeil => "Ceiling bar",
            Self::Pole => "Pole climb",
            Self::Swim => "Swim",
            Self::CapBounce => "Cap jump",
            Self::CatchJump => "Catch jump",
            Self::CapCatchPop => "Cap catch pop",
            Self::Throw => "Cap throw",
            Self::Bind | Self::BindFall => "Bind",
        }
    }
    /// A state that ends by reaching the ground: the contact is flagged in one
    /// step and the state decides what it means in the next.
    fn in_flight(self) -> bool {
        self.airborne_jump()
            || matches!(
                self,
                Self::LongJump
                    | Self::SpinDrop
                    | Self::SpinFlower
                    | Self::GroundPound
                    | Self::Dive
                    | Self::BurnJump
                    | Self::BurnBounce
            )
    }
    /// A state flown by the jump family's shared air control
    /// (`PlayerActionAirMoveControl`): the three jumps, the jumps off the pound,
    /// the cap and a fall. The flips, the long jump and the dive have their own.
    fn uses_air_control(self) -> bool {
        matches!(
            self,
            Self::Jump
                | Self::DoubleJump
                | Self::TripleJump
                | Self::PoundJump
                | Self::SpinJump
                | Self::CapBounce
                | Self::CatchJump
                | Self::Fall
                | Self::WallJump
        )
    }
    fn airborne_jump(self) -> bool {
        matches!(
            self,
            Self::Jump
                | Self::DoubleJump
                | Self::TripleJump
                | Self::Backflip
                | Self::Sideflip
                | Self::SpinJump
                | Self::PoundJump
                | Self::RollJump
                | Self::WallJump
                | Self::CapBounce
                | Self::CatchJump
                | Self::Fall
        )
    }
}
/// Player state a replay can inject before a step (lockstep comparison). It
/// holds what a trace records; internals the trace cannot see keep the values
/// the rewrite's own simulation gave them.
/// The cap as a trace row shows it.
pub struct CapRow {
    pub state: &'static str,
    /// Metres.
    pub pos: Vec3,
    pub clip: &'static str,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub mode: Option<Move>,
    /// Metres.
    pub pos: Vec3,
    /// Units per frame.
    pub v: Vec3,
    pub facing: Vec3,
    pub grounded: bool,
    /// Observed pose and floor contact, needed by pre-movement damage dispatch in lockstep.
    pub pose: Option<Quat>,
    /// The pose is the recording's printed quaternion (six decimals), not the probe's bits.
    pub pose_rounded: bool,
    pub floor_contact: Option<(CollisionCodes, Vec3, u32)>,
    /// SandGeyser disables collision before integration: its collider still stood at this
    /// preceding sample when collision is enabled again. Centimetres, replay-relative.
    pub geyser_collider_from: Option<Vec3>,
    /// Clip and its frame (for "Move", the cycle phase).
    pub anim: Option<(&'static str, f32)>,
    pub state_frames: Option<u32>,
    /// Steps the state's sub-nerve has run (the row's `sub_step`).
    pub sub_steps: Option<u32>,
    /// Counters by the names `Player::counters` lists.
    pub counters: Vec<(String, i64)>,
    /// What a recorded run cycle shows of the run's hidden state.
    pub run: Option<RunHint>,
    /// The clip the next row starts, when the state chosen in this row plays
    /// a new one (the roll's nerves).
    pub next_clip: Option<&'static str>,
    /// The thrown cap in flight.
    pub cap: Option<crate::cap::Moment>,
    /// The spin throw's own progress, while its state runs.
    pub throw: Option<ThrowHint>,
    /// Frames since the cap arrived, while it is caught (-1 on the arrival).
    pub catching: Option<i32>,
    /// The model's sub animation while one plays: name, frame and length.
    pub sub: Option<(&'static str, f32, f32)>,
    /// The row's clip is not the previous row's: the clip started in this row.
    pub clip_started: bool,
    /// While the rows fall at the terminal speed: the height (cm) of the first of them.
    pub fall_start: Option<f32>,
    /// The state is the roll's brake (its clip is the unrolling clip's).
    pub roll_brake: bool,
    /// The state is the spin cap state's fall.
    pub spin_fall: bool,
    /// The quicksand's hidden state as far as the row shows it.
    pub sand: Option<SandHint>,
    /// Steps the recorded slope slide has spent on ground flat enough to end it.
    pub slope_flat_steps: u32,
}
/// What a recorded row shows of the quicksand state (`PlayerSandSinkAffect`): how deep the body
/// sits below the sand's surface, and the turn in progress for the ground nerves.
#[derive(Clone, Copy, Debug, Default)]
pub struct SandHint {
    pub depth: f32,
    pub turn_rate: f32,
    pub last_input: Vec3,
    pub turn_power: f32,
}
/// What a recorded row shows of the spin throw state (`PlayerStateSpinCap`).
#[derive(Clone, Copy, Debug)]
pub struct ThrowHint {
    /// The cap is the spin attack, and for how many frames it has been.
    pub winding: bool,
    pub age: u32,
    /// The throw frame passed: the cap appears in the next frame.
    pub launch: bool,
    pub kind: Throw,
    pub count: u32,
    pub continuous: bool,
}
/// The throw frame of a spin throw (`PlayerSpinCapAttack::getThrowFrameGround` / `getThrowFrameAir`):
/// the continuous throw leaves first, then the swings and the second throw, then the air throw,
/// then the ground throw. A recorded row's step counter is this number plus one on the row of the
/// throw, because it counts the step after the throw step ran.
pub fn throw_frame_of(kind: Throw, count: u32, continuous: bool, air: bool) -> u32 {
    if continuous {
        t::SPIN_CAP_THROW_FRAME_CONTINUOUS
    } else if kind != Throw::Forward || count == 2 {
        t::SPIN_CAP_THROW_FRAME_SWING
    } else if air {
        t::SPIN_CAP_THROW_FRAME_AIR
    } else {
        t::SPIN_CAP_THROW_FRAME
    }
}
/// The run's hidden state as far as a recorded row shows it: the pose's place
/// in the cycle, its weights, and the turn in progress.
#[derive(Clone, Debug, Default)]
pub struct RunHint {
    pub phase: f32,
    pub rate: f32,
    /// RunStart's independent, non-looping model frame (not the run nerve's age).
    pub start_frame: Option<f32>,
    /// Weights of Walk, Run, Dash, DashFast and RunStart.
    pub weights: [f32; 5],
    /// Degrees the facing turned in the recorded frame.
    pub turn_rate: f32,
    /// The stick's direction in the recorded frame.
    pub last_input: Vec3,
    /// The turn power the facing's recent swings have built up.
    pub turn_power: f32,
    /// Steps the current nerve of the run state has taken.
    pub steps: u32,
    /// The run is the stretch after turning round.
    pub after_turn: bool,
    /// The clip a nerve of the run state chose in the recorded frame, which the
    /// next row shows starting (`Brake`, `Turn`, `TurnPoint`, `Move`).
    pub chosen: Option<&'static str>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Throw {
    Forward,
    Up,
    Down,
    Spin,
}
/// Moments the presentation layer reacts to (particles, sound, camera).
#[derive(Clone, Copy, Debug, PartialEq, Message)]
pub enum Fx {
    Jump(Move),
    Crushed,
    /// The dimension keeper changed to 2D: Mario's `2D変身` hit reaction.
    Change2D,
    /// Model changer's exit reaction, 0x45dff0..0x45dff8 (3D変身).
    Change3D {
        bound: bool,
    },
    CarryStart,
    CarryThrow,
    Land {
        hard: bool,
    },
    /// The roll's jump touched down (`PlayerStateRolling::exeJump` raises the plain landing
    /// reaction itself); the state that takes over then raises its own.
    RollJumpLand,
    /// A roll touched down after a long flight: the roll's own landing
    /// reaction (`PlayerStateRolling::control`), not the ground landing's.
    RollLand,
    /// A foot comes down (the sound control decides when, not the clip).
    Step {
        left: bool,
    },
    /// A foot lifts a little before the next step.
    FootLift {
        left: bool,
    },
    /// The rolling clip passed the frame its spin sound starts on.
    RollSpin,
    /// The fall is loud: held every frame, with the frames held so far.
    FallWhoosh {
        frames: u32,
    },
    Skid,
    TurnDust,
    PoundStart,
    PoundLand,
    RollStart,
    /// A restart boost (`restartRolling`); `again` when the roll's restart counter
    /// was already one or more (`[2回目以降]`).
    RollBoost {
        again: bool,
    },
    Dive,
    /// The cap starts flying: its hit reaction.
    Throw(Throw),
    /// The voice reaction of that throw (1, 2 or 3 in a row).
    ThrowVoice(u32),
    /// A step of the cap's stay (it sounds on every one) and the end of it.
    CapStay,
    CapReturn,
    /// The cap turned along a wall (the number is how square the hit was) or onto the surface
    /// its terrain helper found (the turn's angle in degrees).
    CapWallHit(f32),
    CapWallStop(f32),
    /// An exported actor accepted the cap's `CapReflectCollide`: its hit reaction.
    CapReflect,
    /// An exported trampoline accepted the cap's `CapAttack`: the reaction `帽子ヒット`.
    CapHit,
    /// A hanger accepted the cap's `CapStartLockOn` (`帽子セット`).
    CapLockOn,
    /// The cap's lock-on ended and it returns (`帽子戻し`).
    CapLockReturn,
    Catch,
    /// The cap was trampled: its first step in the trample nerve.
    CapBounce,
    /// The foot sensor's trample message was accepted by the cap
    /// (`PlayerActorHakoniwa::attackSensor`): the hit reaction the foot raises.
    CapTrampleHit,
    /// Pukupuku accepted ObjHipDrop and requested the attacker's hit reaction.
    PoundHit,
    /// Accepted land ObjHipDropReflect (SensorMsgFunction.cpp: tryGetHitReactionForMsg).
    HipDropReflectHit,
    /// The stay began while the button is held: the reaction it raises.
    CapStayStart,
    /// Equipment started a forced run (the reaction `強制走り開始`).
    ForceRunStart,
    /// The cap began to skim the water's surface / to rest on it (hit reactions of the cap).
    CapWaterFly,
    CapWaterStay,
    CapWaterIn,
    CapWaterOut,
    WallSlide,
    Ledge,
    Pole,
    /// The player entered the water (`PlayerStateSwim::tryReactionWaterIn`): the splash effect
    /// and the entry sound, the stronger one after a dive or a hip drop.
    WaterIn {
        strong: bool,
    },
    /// The player left the water (`tryReactionWaterOut`): its hit reaction and the splash effect.
    WaterOut,
    /// The head-in dive came up to the surface (the hit reaction `飛び込み後の水面顔出し`).
    SwimHeadOut,
    /// The fall into the water that starts the swim's dive (the hit reaction `入水落下`).
    WaterDive,
    /// The water hip drop reached the bottom (`rs::startHitReactionHipDropLand`).
    SwimHipDropLand,
    /// The camera went under the water (`entered`) or came out of it (`PlayerSeCtrl::update`).
    CameraWater {
        entered: bool,
    },
    /// The ambience of the camera under the water, held while the player is under it too.
    CameraWaterHold,
    SlipIce,
    /// The swim stroke's clip began or looped (`PlayerSeCtrl::update`).
    SwimStrokeSound {
        camera_under: bool,
    },
    /// Hot ground: the leap (false) or the bounce where it lands (true).
    Burned(bool),
    /// Water put the burn out.
    Quenched,
    Abyss,
    RecoveryDamage,
    RecoveryBegin,
    RecoveryWait,
    GrabCeilSwing {
        forward: bool,
    },
}
/// The actor nerve whose `exe` the binary runs in a frame, and whether it is that nerve's first
/// step. A state a sensor phase chooses in the same frame starts next frame, so the cap action
/// history restores of this frame still belong to the nerve that was running.
#[derive(Clone, Copy, Debug)]
struct ActingNerve {
    name: &'static str,
    first_step: bool,
}
/// Clip the presentation should play: an original animation name, or "Move"
/// for the speed-blended walk/run/dash cycle. `serial` changes on restarts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anim {
    pub name: &'static str,
    pub serial: u32,
    /// Original frames elapsed in the clip (for "Move", the 60-frame cycle phase).
    pub frame: f32,
    /// Playback rate in original frames per frame.
    pub rate: f32,
}
impl Anim {
    /// Where the clip will be after the next advance. Animation-frame triggers
    /// (sounds, effects) fire on this frame: the game logs a trigger on clip
    /// frame t in the row whose pose still shows t - 1.
    pub fn held(&self) -> f32 {
        self.frame + self.rate
    }
    /// The frame animation triggers are scanned up to in the trace. A clip's
    /// trigger fires in the row whose pose shows its frame (the footfalls of the
    /// crouch walk, the jump's, the pound's, the landings', the air throw's);
    /// only the run cycle keeps the `held` rule. The ground throw's clip plays
    /// as a sub animation too, and then the model's frame decides
    /// (`Player::trigger_frame`).
    pub fn trigger_frame(&self) -> f32 {
        if self.name.starts_with("Move") {
            self.held()
        } else {
            self.frame
        }
    }
}
pub use crate::cap::{Cap, CapPhase};

/// The throw state's own bookkeeping (`PlayerSpinCapAttack`).
#[derive(Debug, Default)]
struct ThrowControl {
    /// Frames the run keeps running after a throw on the move.
    run: u32,
    /// Throws in a row, each within the catch window of the one before
    /// (`PlayerSpinCapAttack::setupAttackInfo`): the clip and the throw frame follow it.
    count: u32,
    /// This throw is the third in a row: the fast, continuous one.
    continuous: bool,
    /// Which kind the state in progress throws.
    kind: Option<Throw>,
    /// The state in progress began in the air (`exeSpinAir` instead of
    /// `exeSpinGround`), as `PlayerStateSpinCap::appear` chose it (`spin_cap_in_air`): the ground
    /// judges never ask about it.
    in_air: bool,
    /// An air-started state has been on the ground since (`PlayerStateSpinCap` byte +0x78 is set
    /// by `appear` for an air start and cleared by `control` on `rs::isOnGround`, and stays
    /// clear): the actor then restores the cap actions in every step.
    ground_seen: bool,
    /// The ground spin's step ended the state (`updateNerveState` answered true in
    /// `exeSpinCap`, 0x4239c4), which skips the actor's history restore of that step.
    child_ended: bool,
    /// `PlayerTrigger` action 0 was on when the state appeared (`PlayerStateSpinCap` byte +0xa9,
    /// stored by `appear` at 0x4819cc and read by both spins' first step as the throw direction
    /// helper's last argument): the throw was pressed in the wall state's non-air steps.
    after_wall_air: bool,
    /// The direction the cap leaves along, fixed on the state's first step.
    dir: Option<Vec3>,
    /// The up axis the swim spin stores on its first step and hands the cap on the throw frame
    /// (the ground and air spins ask `launch_up_axis` at the throw frame instead). The surface
    /// swim spin's is a -gravity the binary computes at the throw frame; here it is stored too.
    up: Option<Vec3>,
    /// SpinAir preserves the tilted model root, unwinding its translation over ten steps.
    model_shift: Option<Vec3>,
    /// Offset handed to HackCap::startThrow while SpinAir repays its root shift.
    launch_offset: Vec3,
}
/// A partial animation playing on top of the main clip (`PlayerAnimator::startSubAnim`).
/// While one plays the main clip's frame is counted by the animator itself
/// instead of following the model, which holds its first frame one extra frame.
#[derive(Clone, Copy, Debug, Default)]
struct SubClip {
    name: &'static str,
    /// The frame the model's action shows in the sample.
    frame: f32,
    length: f32,
    active: bool,
}

/// The pose of a run on a slope: the orientation, for the facing it was made
/// for (a state that turns the player elsewhere makes it upright again).
#[derive(Clone, Copy, Debug)]
struct PoseTilt {
    pose: Quat,
    facing: Vec3,
}

#[derive(Component, Debug)]
pub struct Player {
    pub constants: &'static PlayerConstants,
    pub dimension: dimension::DimensionKeeper,
    pub dimension_diagnostic: String,
    is_moon: bool,
    /// State changes decided last step, waiting for their first step.
    nerve: Nerve,
    fire: FireControl,
    sand: sand_sink::SandControl,
    geyser: sand_geyser::GeyserControl,
    pub puku_hit: bool,
    pub puku_source: String,
    stage_translation: Option<(Vec3, Vec3)>,
    stage_carry: Option<Vec3>,
    /// `PlayerCounterForceRun`: set by RocketFlower equipment.
    force_run: force_run::ForceRun,
    pub rocket: rocket_flower::RocketFlowers,
    /// The Killer launchers of the stage, their Killers and the switches that start them.
    pub killers: killer::Killers,
    pub trampolines: Vec<cap_trampoline::Trampoline>,
    pub signboards: Vec<sign_board::SignBoard>,
    pub cap_hangers: Vec<cap_hanger::CapHanger>,
    pub cap_switches: Vec<cap_switch::CapSwitch>,
    spin_flower: spin_flower::SpinFlower,
    hip_drop_reflect: bool,
    surface_run: surface_run::SurfaceRun,
    damage: Damage,
    life: damage_life::LifeControl,
    /// The pole climb in progress (`PlayerStatePoleClimb`).
    pole: PoleControl,
    grab_ceil: grab_ceil::GrabCeilControl,
    pub carry: carry::Carry,
    rolling: RollControl,
    /// Where the last stage collision left the body: the metres of `pos` it was stored as, and
    /// the position in centimetres (start-relative, on the game's single-precision grid) the
    /// collider returned. Valid for as long as `pos` is unchanged since.
    collider_end: Option<(Vec3, Vec3)>,
    /// Where the collider stood (cm) when a state moved the actor by hand (`al::setTrans`)
    /// since: the collider's next move carries it from there to the actor first.
    trans_carry: Option<Vec3>,
    /// The forced slope slide (`PlayerStateSlope`).
    slide: SlopeSlide,
    /// Steps the collider has touched the ground in a row (`PlayerCollider::mTimeInAir`, which
    /// counts them up from 1 and which `rs::isJustLand` reads).
    ground_contact: u32,
    squat: SquatControl,
    /// The standing state's idle sequence (`PlayerStateWait`).
    wait: WaitControl,
    throwing: ThrowControl,
    /// The head's look-at (holder, keeper and joint controller).
    pub look: look_at::LookAt,
    /// The joint controllers that pose the body (`PlayerJointControlKeeper`).
    pub joints: joint_controls::JointState,
    /// Metres.
    pub pos: Vec3,
    /// Metres per second, derived each frame from `v`.
    pub vel: Vec3,
    pub facing: Vec3,
    pub mode: Move,
    pub grounded: bool,
    pub cap: Option<Cap>,
    /// A throw started last frame; the cap appears when it updates.
    launch: Option<Launch>,
    /// The throw button was held in the last frame.
    throw_held_before: bool,
    /// An upper-body animation was cleared in this step (not ended by its
    /// length, so the main clip is not restarted).
    upper_cleared: bool,
    /// The cap was put on the head without a catch (`HackCap::forcePutOn`) in
    /// the last frame's sensor phase: the frame is still reported with the
    /// cap, which is gone from the next one.
    cap_put_on: bool,
    /// `HackCap` byte +0x2a3, tested by `isCatched` (0x3fe6b8). The constructor clears it;
    /// a catch/forcePutOn sets it, and startSpinAttack clears it independently of the nerve.
    cap_catched: bool,
    /// The last number given to a clip start of the cap (they count on).
    cap_serial: u32,
    /// The cap is Mario's spin attack, waiting for the throw frame
    /// (the cap's `SpinAttack` nerve).
    winding: bool,
    /// The ground spin's clip is within three frames of its end: from then on the state asks for
    /// the inverse kinematics (`PlayerStateSpinCap`'s flag at `+0xa8`, cleared when the state
    /// appears, read by the actor's movement for the ground pose's rates).
    spin_ik: bool,
    /// The air throw is over but its state runs on: the fall of the spin cap
    /// state (`PlayerStateSpinCap`'s `Fall` nerve) until the player lands.
    spin_fall: bool,
    /// Frames of the cap's catch nerve: -1 on the frame it arrived.
    catching: Option<i32>,
    /// The clip the cap catch plays (it differs with the player's state).
    catch_clip: &'static str,
    /// A hit sensor restarted the actor nerve in this frame's update, after the
    /// nerve's own step counter was reset (the trace's `sub_step` restarts
    /// with it and runs the first step in the same row).
    sensor_restart: bool,
    /// Frames of the upper-body catch animation the player plays when he is
    /// not standing (13 recorded in the air), -1 on the frame it starts.
    upper_catch: Option<i32>,
    /// The sub animation the model plays, and whether the main clip's next
    /// update is skipped (a clip just started under a sub animation).
    sub: SubClip,
    anim_skip: bool,
    /// A running clip's rate was changed by a state (`PlayerAnimator::setAnimRate`): the
    /// animator's frame counter takes the change over once more at its next update (the model
    /// advances by the old rate and the counter is synchronised to it plus the new one).
    anim_rate_change: f32,
    /// The clip began at a frame set by hand (`setAnimFrame`): the animator's
    /// counter, which the trace's `anim_frame` shows, starts over at zero for
    /// the row while the pose shows the frame.
    pub counter_restarted: bool,
    /// Frames the cap has been the spin attack, and the clip of the throw.
    winding_age: u32,
    throw_clip: &'static str,
    /// Seconds in the current mode.
    pub timer: f32,
    pub since_land: f32,
    pub distance: f32,
    pub checkpoint: Vec3,
    pub hard_land: bool,
    /// A completed ground child asks its actor to dispatch after its update.
    ground_state_ended: bool,
    pub wall: Vec3,
    pub hitstop: f32,
    pub events: Vec<Fx>,
    pub anim: Anim,
    /// Upper-body overlay clip while throwing on the run.
    pub sub_anim: Option<Anim>,
    pub soot: Soot,
    /// Worn head metadata, supplied by the installed wardrobe before simulation.
    pub prevent_head_pain: bool,
    pub death: death::Death,
    entrance_camera: entrance_camera::EntranceCamera,
    recovery: recovery_dead::Recovery,
    /// Horizontal speed in original units per frame, for animation blending.
    pub move_speed: f32,
    /// Where to draw the player and cap: between the last two simulation
    /// steps, so the 60 Hz simulation moves smoothly on faster displays.
    pub shown: Vec3,
    pub shown_cap: Option<Vec3>,
    prev: Vec3,
    prev_cap: Option<Vec3>,
    /// Units per frame.
    v: Vec3,
    frame: u32,
    land_frames: u32,
    pre_jump: u32,
    pre_throw: u32,
    /// `PlayerJudgeSpeedCheckFall`: steps the player has gone without ground
    /// under him, counted up to one more than the judge needs.
    ground_lost: u32,
    /// The combo of consecutive jumps and the long-jump count, as the
    /// original's `PlayerContinuousJump` and `PlayerContinuousLongJump`.
    combo: ContinuousJump,
    long_combo: ContinuousLongJump,
    /// Air control of the jump family, the state it was set up for, and the
    /// facts of the jump in progress that its landing needs.
    air: AirMove,
    /// The turn controller the air control keeps (`PlayerActionAirMoveControl::mTurnControl`).
    air_turn: turn_control::TurnControl,
    air_for: Option<Move>,
    jump_power: f32,
    combo_kept: bool,
    /// Velocity when the jump's first step began, before its own impulse.
    prior_v: Vec3,
    /// The horizontal velocity a standard jump takes off with, read when its first step starts.
    takeoff_h: Option<Vec3>,
    /// Distance to the floor below, for the ground pound's height rule.
    ground_height: f32,
    /// The camera's orbit yaw this frame (the first jump's clip mirrors by it).
    camera_yaw: f32,
    jump_held_frames: u32,
    run_frames: u32,
    wall_push_counter: u32,
    after_turn: u32,
    quick_turn: u32,
    catch_window: u32,
    cap_jump_possible: bool,
    /// `PlayerCapActionHistory`'s byte at `+0x39`: the air catch's pop is still possible.
    cap_pop_possible: bool,
    /// The pop's first step restarts the cap's catch with `JumpCapCatch`; the cap's next update
    /// starts it.
    cap_catch_restart: bool,
    /// Frames of the cap's catch restarted by the pop (`HackCap::startCatch`, clip `JumpCapCatch`).
    pop_catch: Option<i32>,
    /// No longer read: it gated the dry airborne dive, which the executable does not have (see
    /// `air_actions`). Kept only because the `counters` trace cell lists it; dropping it needs a
    /// change of the `counters` cell in every rewrite trace.
    dive_possible: bool,
    gravity: f32,
    jump_extend: bool,
    max_air_speed: f32,
    /// The wall slide in progress and what the wall judge remembers of the last one.
    wall_air: WallAirControl,
    wall_history: WallHistory,
    /// The drop the long-fall judge measures (`PlayerColliderHakoniwa::updateFallDistanceCheck`).
    fall: FallDistance,
    /// The stick-circle detector the player's input holds
    /// (`al::SpinInputAnalyzer`), fed every frame.
    spin_input: SpinInput,
    /// The direction of the ground spin in progress, which the spin jump and
    /// the spin drop that follow it keep (left or right clips).
    spin_clockwise: bool,
    stick: Vec3,
    tilt: f32,
    /// The stick as the pad reports it (y up), for the states that read it
    /// without the camera.
    raw_stick: Vec2,
    input_2d: input_2d::Input2D,
    input_2d_frame: Option<input_2d::Frame>,
    ground_2d: GroundMove,
    slope: f32,
    /// The ledge hang in progress (`PlayerStateWallCatch`), and where the
    /// collider stood (cm) after the last step of it.
    catch: WallCatchControl,
    catch_collider_at: Option<Vec3>,
    /// The fall that follows letting go of a ledge (`EActionTrigger 0x20` when
    /// `PlayerStateFallHakoniwa` appears): the air control ignores the stick
    /// for the first steps, and the ledge cannot be caught again for 15 steps
    /// unless the body turned away from the way it faced then.
    fall_after_ledge: Option<Vec3>,
    /// A state chose its own sub-nerve again in this step (the trace's
    /// `sub_step` starts over although the name is the same).
    sub_restarted: bool,
    ended_sub: Option<(&'static str, f32)>,
    /// The clip started in this step does not fade in from the one before it
    /// (`PlayerAnimator::clearInterpolation`).
    interpolation_cleared: bool,
    /// Steps taken since the player was placed (a teleport clears the animator's blend).
    age: u32,
    /// Pole axis base, its height, the climb cycle in progress and a re-grab delay.
    /// Frames since the burn's bounce landed (run and falls together).
    /// Burn leap: horizontal direction its air control works along.
    /// Burn leap frames left without gravity.
    /// Fire run frames since the stick was released, capped at the brake time.
    /// The swim state (`PlayerStateSwim`) and the player's water surface finder.
    swim: swim::Swim,
    water_finder: crate::water::SurfaceFinder,
    /// Axes of the joints as the model last posed them (`al::getJointMtxPtr`, which the swim
    /// nerves read in their first step); the pose calculation keeps them.
    model_axes: swim_dive_in::ModelAxes,
    /// The water effects of the cap that was caught in this step (they end in the next).
    cap_water_left: (bool, bool),
    /// The head slide's flag that the `DiveInWater` judge passed during the slide
    /// (`PlayerStateHeadSliding::isEnableDiveInWater`).
    head_slide_dive_in: bool,
    /// The player's wetness and the drops that run off him (`PlayerWetControl`).
    wet: wet::WetControl,
    /// Where the camera is, in centimetres relative to the replay's start, when the host
    /// tells (the water sounds of the camera need it).
    camera_cm: Option<Vec3>,
    /// The player's root is in water (not a puddle): the water prefix of the material code that
    /// effects and sounds pick their variants by (`PlayerActorHakoniwa::movement`).
    material_water: bool,
    /// `EMaterialChangeTrigger 0`, which the end of the last step's movement set: the root was in
    /// a puddle (the water shallower than the swim's centre offset).
    material_puddle: bool,
    step_phase: f32,
    /// Fall speed the ground stopped in the latest step (zero when it did not
    /// land), kept for the trace: the original still reports it that frame.
    landing_speed: f32,
    /// The fall speed of the step that reached the ground, which the step that
    /// decides the landing (the next one) still knows.
    touchdown_speed: f32,
    /// The run cycle's animation control (the `Move` slot).
    cycle: RunCycle,
    /// The dust the run state's first step started although the same step
    /// chose another state (a jump pressed with the stick): the game logs it
    /// for that frame only.
    run_start_effect: Option<&'static str>,
    /// The floor normal the run was last steered on, with the horizontal
    /// velocity it left (`mGroundNormal`) and the whole velocity the control set on the actor
    /// (`al::setVelocity`, the pull along the normal included): the velocity belongs to that
    /// plane while it is unchanged.
    run_up: Option<(Vec2, Vec3, Vec3)>,
    /// The velocity the last collision moved a ground state with (its horizontal speed and pull
    /// it was made from, then the velocity): the game's `getVelocity` after the step, laid on
    /// the normal the nerve saw, not the one the collision found since.
    ground_move: Option<(Vec2, f32, Vec3)>,
    /// EActionTrigger 3, consumed by syncSensorAndCollision after the nerve.
    collision_expand_pending: bool,
    /// Snapshot injection already owns the legacy run-entry reconstruction.
    snapshot_injected: bool,
    /// The pose when it is tilted off the vertical (a run up a slope); it
    /// belongs to the facing it was made for.
    pose_tilt: Option<PoseTilt>,
    /// The world direction a stick held forward walks along (the camera's
    /// heading).
    forward: Vec3,
    /// The wall the stick is bent along after a contact.
    wall_along: wall_along::WallAlong,
    /// The camera's view rows when the orders carry them: the stick is then laid on the world as
    /// `PlayerInput::calcMoveInput` does (`move_input.rs`).
    view: Option<move_input::ViewRows>,
    camera_view: Option<move_input::ViewRows>,
    snap_move_axis: Option<Vec3>,
    /// The frame (base matrix, inverse) of the collision part the last collision found ground on,
    /// when the stage's export has it: `rs::waitGround` carries the pose along it.
    ground_frame: Option<([f32; 12], [f32; 12])>,
    ground_part: Option<usize>,
    rotating_ground: bool,
    ground_inertia: ground_inertia::GroundInertia,
    area_velocity: Vec3,
    pub area_source: String,
    /// `PlayerExternalVelocity::mExternalVelocity` and the `ByugoBlow` forces `ExternalForceKeeper`
    /// collected since the last update (`external_velocity.rs`).
    external_velocity: Vec3,
    external_keeper: external_velocity::ForceKeeper,
    pub wind_source: String,
    pub wind_update: String,
    pub ground_source: String,
    /// Ground steering of the run and its pivot on the spot.
    ground: GroundMove,
    /// The spin throw's own ground control, which steers once the spin's
    /// braking frames are over (`PlayerStateSpinCap::updateSpinGroundNerve`).
    spin_ground_move: GroundMove,
    pivot: PivotTurn,
    /// Which nerve of the run state is executing.
    run_nerve: RunNerve,
    /// Steps the current run nerve has executed.
    run_steps: u32,
    /// The way the brake and the turn slide on (the facing when braking began).
    brake_dir: Vec3,
    /// Speed the run resumes with after turning round (`_8c`).
    after_turn_speed: f32,
    /// Pull on the ground the latest ground step kept, units per frame.
    pull: f32,
    /// The distance `rs::landGroundPoseAndSnap` adds to a flat landing's first ground pull.
    land_snap: f32,
    /// The sounds the player's code starts itself (footsteps, roll, fall).
    sounds: SeCtrl,
    /// The collider still reports the floor under a hop that leaves it (the
    /// roll's start), though the player is already rising.
    hop_contact: bool,
    /// The real game's player collider, used when the course is a triangle mesh.
    collider: PlayerCollider,
    /// CollisionTrigger9: accepted PlayerRollingWallHitMove, not a squeeze.
    /// Static KCL has no sensor receiver; no supported actor raises it yet.
    collision_wall_reflected: bool,
    pub(crate) own_cap_trample: bool,
    /// `PlayerCeilingCheck` as the last update left it (`PlayerJudgeEnableStandUp` reads it).
    ceiling: CeilingCheck,
    /// The normal of the mesh floor the collider found this step.
    mesh_ground: Option<Vec3>,
    /// The collision codes of the mesh surfaces the body touched this step.
    pub touched: terrain::Touched,
    pub key_move_sources: String,
    pub rotate_sources: String,
    press: press::Press,
    pipes: Vec<bind::DokanActor>,
    bound_to: Option<usize>,
    puppet_hidden: bool,
    puppet_collision: bool,
    puppet_sensors: bool,
    puppet_clip_pending: bool,
    /// The model changed this step: the new model's animator has not synced its counter yet.
    model_switch_row: bool,
    /// The row after a bound model switch: the puppet's first updateSync.
    switch_sync_row: bool,
    puppet_first_row: bool,
    bind_staged: Vec<usize>,
    bind_ready: Vec<usize>,
    /// The main clip looped back to its start in this step, as the game's triggers see it: a
    /// clip whose counter shows its length for a row before it wraps starts its loop in that
    /// row.
    pub clip_wrapped: bool,
    /// The clip's counter wrapped in this step's advance.
    counter_wrapped: bool,
    /// The pose frame (unwrapped) the looping clip of that serial showed in the last step.
    pose_seen: Option<(u32, f32)>,
    /// Steps until the run cycle that follows a dive's landing restarts
    /// (the animator starts it a second time one step after the first).
    cycle_restart_in: u8,
}
impl Default for Player {
    fn default() -> Self {
        // A new player starts the standing state like any other: its clip
        // begins on the first step.
        let mut nerve = Nerve::default();
        nerve.queue("Wait", |_| {});
        let mut dimension = dimension::DimensionKeeper::default();
        // PlayerActorHakoniwa::initAfterPlacement, 1.0.0 0x41dc04.
        dimension.invalidate();
        Self {
            constants: &PlayerConstants::NORMAL,
            dimension,
            dimension_diagnostic: String::new(),
            is_moon: false,
            nerve,
            fire: FireControl::default(),
            sand: sand_sink::SandControl::default(),
            geyser: sand_geyser::GeyserControl::default(),
            puku_hit: false,
            puku_source: String::new(),
            stage_translation: None,
            stage_carry: None,
            force_run: force_run::ForceRun::default(),
            rocket: default(),
            killers: default(),
            trampolines: default(),
            signboards: default(),
            cap_hangers: default(),
            cap_switches: default(),
            spin_flower: default(),
            hip_drop_reflect: false,
            surface_run: default(),
            damage: Damage::default(),
            life: damage_life::LifeControl::default(),
            pole: PoleControl::default(),
            grab_ceil: grab_ceil::GrabCeilControl::default(),
            carry: carry::Carry::default(),
            rolling: RollControl::default(),
            collider_end: None,
            trans_carry: None,
            slide: SlopeSlide::default(),
            ground_contact: 0,
            squat: SquatControl::default(),
            wait: WaitControl::default(),
            throwing: ThrowControl::default(),
            look: look_at::LookAt::default(),
            joints: joint_controls::JointState::default(),
            pos: SPAWN,
            vel: Vec3::ZERO,
            facing: Vec3::NEG_Z,
            mode: Move::Idle,
            grounded: true,
            cap: None,
            launch: None,
            cap_put_on: false,
            cap_catched: false,
            upper_cleared: false,
            throw_held_before: false,
            cap_serial: 0,
            winding: false,
            spin_ik: false,
            spin_fall: false,
            catching: None,
            catch_clip: "CatchCap",
            sensor_restart: false,
            upper_catch: None,
            sub: SubClip::default(),
            anim_skip: false,
            anim_rate_change: 0.,
            counter_restarted: false,
            winding_age: 0,
            throw_clip: "SpinCapStart",
            timer: 0.,
            since_land: 5.,
            distance: 0.,
            checkpoint: SPAWN,
            hard_land: false,
            ground_state_ended: false,
            wall: Vec3::ZERO,
            hitstop: 0.,
            events: Vec::new(),
            anim: Anim {
                name: "Wait",
                serial: 0,
                frame: 0.,
                rate: 1.,
            },
            sub_anim: None,
            soot: Soot::default(),
            prevent_head_pain: false,
            death: death::Death::default(),
            entrance_camera: entrance_camera::EntranceCamera::default(),
            recovery: recovery_dead::Recovery::default(),
            move_speed: 0.,
            shown: SPAWN,
            shown_cap: None,
            prev: SPAWN,
            prev_cap: None,
            v: Vec3::ZERO,
            frame: 0,
            land_frames: 300,
            pre_jump: 0,
            pre_throw: 0,
            ground_lost: 0,
            combo: ContinuousJump::default(),
            long_combo: ContinuousLongJump::default(),
            air: AirMove::default(),
            air_turn: turn_control::TurnControl::air(),
            air_for: None,
            jump_power: 0.,
            combo_kept: false,
            prior_v: Vec3::ZERO,
            takeoff_h: None,
            ground_height: f32::MAX,
            camera_yaw: 0.,
            jump_held_frames: 0,
            run_frames: 0,
            wall_push_counter: 0,
            after_turn: 0,
            quick_turn: 0,
            catch_window: 0,
            cap_jump_possible: true,
            cap_pop_possible: true,
            cap_catch_restart: false,
            pop_catch: None,
            dive_possible: true,
            gravity: PlayerConstants::NORMAL.gravity_air,
            jump_extend: false,
            max_air_speed: t::JUMP_MOVE_MIN,
            wall_air: WallAirControl::default(),
            wall_history: WallHistory::default(),
            fall: FallDistance::default(),
            spin_input: SpinInput::default(),
            spin_clockwise: false,
            stick: Vec3::ZERO,
            tilt: 0.,
            raw_stick: Vec2::ZERO,
            input_2d: input_2d::Input2D::default(),
            input_2d_frame: None,
            ground_2d: GroundMove::two_d(),
            slope: 0.,
            catch: WallCatchControl::default(),
            catch_collider_at: None,
            fall_after_ledge: None,
            sub_restarted: false,
            ended_sub: None,
            interpolation_cleared: false,
            age: 0,
            swim: swim::Swim::default(),
            wet: wet::WetControl::default(),
            camera_cm: None,
            material_water: false,
            material_puddle: false,
            water_finder: crate::water::SurfaceFinder::default(),
            model_axes: swim_dive_in::ModelAxes::default(),
            cap_water_left: (false, false),
            head_slide_dive_in: false,
            step_phase: 0.,
            landing_speed: 0.,
            touchdown_speed: 0.,
            cycle: RunCycle::default(),
            run_start_effect: None,
            run_up: None,
            ground_move: None,
            collision_expand_pending: false,
            snapshot_injected: false,
            pose_tilt: None,
            forward: Vec3::ZERO,
            wall_along: wall_along::WallAlong::default(),
            view: None,
            camera_view: None,
            snap_move_axis: None,
            ground_frame: None,
            ground_part: None,
            rotating_ground: false,
            land_snap: 0.,
            ground_inertia: ground_inertia::GroundInertia::default(),
            area_velocity: Vec3::ZERO,
            area_source: String::new(),
            external_velocity: Vec3::ZERO,
            external_keeper: external_velocity::ForceKeeper::default(),
            wind_source: String::new(),
            wind_update: String::new(),
            ground_source: String::new(),
            ground: GroundMove::default(),
            spin_ground_move: GroundMove::plain(),
            pivot: PivotTurn::default(),
            run_nerve: RunNerve::Run,
            run_steps: 0,
            brake_dir: Vec3::Z,
            after_turn_speed: t::NORMAL_MAX,
            pull: t::GRAVITY,
            sounds: SeCtrl::default(),
            hop_contact: false,
            collider: PlayerCollider::default(),
            collision_wall_reflected: false,
            own_cap_trample: false,
            ceiling: CeilingCheck::OPEN,
            mesh_ground: None,
            touched: terrain::Touched::default(),
            key_move_sources: String::new(),
            rotate_sources: String::new(),
            press: press::Press::default(),
            pipes: Vec::new(),
            bound_to: None,
            puppet_hidden: false,
            puppet_collision: false,
            puppet_sensors: true,
            puppet_clip_pending: false,
            model_switch_row: false,
            switch_sync_row: false,
            puppet_first_row: false,
            bind_staged: Vec::new(),
            bind_ready: Vec::new(),
            clip_wrapped: false,
            counter_wrapped: false,
            pose_seen: None,
            cycle_restart_in: 0,
        }
    }
}
#[derive(Clone)]
pub struct Solid {
    pub center: Vec3,
    pub half: Vec3,
    pub slope: f32,
    pub moving: bool,
    pub floor_code: &'static str,
}
impl Solid {
    pub fn top(&self, x: f32) -> f32 {
        self.center.y + self.half.y + (x - self.center.x) * self.slope
    }
}
/// The boxes as a triangle mesh in centimetres (a sloped box rises along x by
/// its slope), each face wound counter-clockwise seen from outside.
fn boxes_to_faces(solids: &[Solid]) -> Vec<([Vec3; 3], u16)> {
    let mut faces: Vec<[Vec3; 3]> = Vec::new();
    for b in solids {
        let (lo, hi) = (b.center - b.half, b.center + b.half);
        let corner = |x: f32, y: f32, z: f32| Vec3::new(x, y, z) / UNIT;
        let top = |x: f32| b.top(x);
        let c = [
            corner(lo.x, lo.y, lo.z),
            corner(hi.x, lo.y, lo.z),
            corner(hi.x, top(hi.x), lo.z),
            corner(lo.x, top(lo.x), lo.z),
            corner(lo.x, lo.y, hi.z),
            corner(hi.x, lo.y, hi.z),
            corner(hi.x, top(hi.x), hi.z),
            corner(lo.x, top(lo.x), hi.z),
        ];
        let center = (c[0] + c[6]) / 2.;
        // A quad of four corners, as two triangles facing away from the middle.
        let mut quad = |q: [usize; 4]| {
            let (a, b, d) = (c[q[0]], c[q[1]], c[q[2]]);
            let outward = (a + b + d + c[q[3]]) / 4. - center;
            let order = if (b - a).cross(d - a).dot(outward) >= 0. {
                [q[0], q[1], q[2], q[3]]
            } else {
                [q[0], q[3], q[2], q[1]]
            };
            faces.push([c[order[0]], c[order[1]], c[order[2]]]);
            faces.push([c[order[0]], c[order[2]], c[order[3]]]);
        };
        quad([0, 1, 2, 3]);
        quad([4, 5, 6, 7]);
        quad([0, 1, 5, 4]);
        quad([3, 2, 6, 7]);
        quad([0, 3, 7, 4]);
        quad([1, 2, 6, 5]);
    }
    faces.into_iter().map(|corners| (corners, 0)).collect()
}
/// The poles as hexagonal prisms in centimetres, each side face and both caps
/// facing away from the middle, coded as a climbable `Pole` floor.
fn poles_to_faces(poles: &[Pole]) -> Vec<([Vec3; 3], u16)> {
    let mut faces = Vec::new();
    for pole in poles {
        let base = pole.center / UNIT;
        let up = Vec3::Y * (pole.height / UNIT);
        let ring = |k: usize| {
            let angle = (k % 6) as f32 * std::f32::consts::FRAC_PI_3;
            Vec3::new(angle.cos(), 0., angle.sin()) * POLE_CIRCUMRADIUS
        };
        let middle = base + up / 2.;
        let mut face = |a: Vec3, b: Vec3, c: Vec3| {
            let outward = (a + b + c) / 3. - middle;
            if (b - a).cross(c - a).dot(outward) >= 0. {
                faces.push(([a, b, c], 1));
            } else {
                faces.push(([a, c, b], 1));
            }
        };
        for k in 0..6 {
            let (a, b) = (base + ring(k), base + ring(k + 1));
            face(a, b, b + up);
            face(a, b + up, a + up);
            face(base, a, b);
            face(base + up, a + up, b + up);
        }
    }
    faces
}
/// The toy course as one triangle mesh: its boxes and its poles.
fn toy_mesh(solids: &[Solid], poles: &[Pole]) -> TriangleMesh {
    let mut faces = boxes_to_faces(solids);
    faces.extend(poles_to_faces(poles));
    let pole_codes = CollisionCodes {
        floor: "Pole".into(),
        ..default()
    };
    TriangleMesh::from_coded_corners(&faces, vec![default(), pole_codes])
}

#[derive(Clone)]
pub struct Pole {
    pub center: Vec3,
    pub height: f32,
}
#[derive(Resource, Default, Clone)]
pub struct Course {
    pub solids: Vec<Solid>,
    pub poles: Vec<Pole>,
    /// The toy course's pool (a stage's own `WaterArea`s come with its collision mesh).
    pub water: crate::water::WaterAreas,
    /// Hot surfaces (lava): centre of the top face and half extents.
    pub hot: Vec<(Vec3, Vec3)>,
    /// Static Needle-coded tops in the toy course, consumed through collision.
    pub needles: Vec<(Vec3, Vec3)>,
    /// The toy course's quicksand: centre of the surface and half extents. The solid under it
    /// is the sand's bottom; the body sinks into it (`sand_sink.rs`).
    pub sands: Vec<(Vec3, Vec3)>,
    /// Shallow water to stand in: centre of the surface and half extents.
    pub puddles: Vec<(Vec3, Vec3)>,
    pub targets: Vec<Vec3>,
    /// Sensors of other actors the head looks at when the course has no stage of its own
    /// (a rendered replay shows where the recorded stage placed them).
    pub look_sensors: Vec<look_at::LookSensor>,
    /// The real game's stage collision, when a replay loads one. It takes the
    /// place of the solids for the player's contact.
    pub mesh: Option<Arc<TriangleMesh>>,
    /// The solids and poles as triangles, built when the ledge or pole judge
    /// first asks (they shoot arrows at triangles).
    pub solid_mesh: std::sync::OnceLock<Arc<TriangleMesh>>,
    /// The toy course's poles alone, for the player's contact with them.
    pub pole_mesh: std::sync::OnceLock<Arc<TriangleMesh>>,
}
impl Course {
    /// The triangles the ledge judge shoots at: the stage's own, or the
    /// toy course's boxes turned into faces.
    pub fn catch_mesh(&self) -> Option<Arc<TriangleMesh>> {
        if let Some(mesh) = &self.mesh {
            return Some(mesh.clone());
        }
        if self.solids.is_empty() && self.poles.is_empty() {
            return None;
        }
        Some(
            self.solid_mesh
                .get_or_init(|| Arc::new(toy_mesh(&self.solids, &self.poles)))
                .clone(),
        )
    }
    /// The toy course's poles as triangles.
    fn toy_pole_mesh(&self) -> Arc<TriangleMesh> {
        self.pole_mesh
            .get_or_init(|| {
                let codes = CollisionCodes {
                    floor: "Pole".into(),
                    ..default()
                };
                Arc::new(TriangleMesh::from_coded_corners(
                    &poles_to_faces(&self.poles),
                    vec![default(), codes],
                ))
            })
            .clone()
    }
    /// The other actors' sensors the head may look at (the stage's placements the replay
    /// knows; the toy course has none).
    pub fn look_sensors(&self) -> &[look_at::LookSensor] {
        self.mesh
            .as_ref()
            .map_or(&self.look_sensors[..], |mesh| &mesh.look_sensors)
    }
    /// The water areas the player swims in: the stage's own (`WaterArea` placements) or the toy
    /// course's pool.
    pub fn water_areas(&self) -> &crate::water::WaterAreas {
        match &self.mesh {
            Some(mesh) => &mesh.water,
            None => &self.water,
        }
    }
    /// A replay-relative position in centimetres in the frame the water areas are placed in
    /// (a stage's own coordinates; the toy pool lives in the replay's).
    pub fn in_water_frame(&self, relative_cm: Vec3) -> Vec3 {
        match &self.mesh {
            Some(mesh) => mesh.to_game_vec(relative_cm),
            None => relative_cm,
        }
    }
    /// `al::isInWaterPos` for a position in metres.
    pub fn wet(&self, pos: Vec3) -> bool {
        self.water_areas().is_water(self.in_water_frame(pos / UNIT))
    }
    /// Standing on a hot surface.
    pub fn hot(&self, pos: Vec3) -> bool {
        self.hot.iter().any(|(c, h)| {
            (pos.x - c.x).abs() < h.x && (pos.z - c.z).abs() < h.z && (pos.y - c.y).abs() < 0.1
        })
    }
    /// Feet in shallow water, for the installed effect material variants.
    #[cfg(feature = "local-assets")]
    pub fn puddle(&self, pos: Vec3) -> bool {
        self.puddles.iter().any(|(c, h)| {
            (pos.x - c.x).abs() < h.x && (pos.z - c.z).abs() < h.z && (pos.y - c.y).abs() < 0.2
        })
    }
    /// Highest walkable surface at or below `pos`, for shadows and the camera.
    pub fn ground(&self, pos: Vec3) -> Option<f32> {
        if let Some(mesh) = &self.mesh {
            // Seen from above, like the solids: the mesh works in cm.
            let limit = crate::collider::walkable_normal_y();
            return mesh
                .floor_below(pos.x / UNIT, pos.z / UNIT, pos.y / UNIT + 5., limit)
                .map(|y| y * UNIT);
        }
        self.solids
            .iter()
            .filter(|b| {
                (pos.x - b.center.x).abs() < b.half.x && (pos.z - b.center.z).abs() < b.half.z
            })
            .map(|b| b.top(pos.x))
            .filter(|top| *top <= pos.y + 0.05)
            .max_by(f32::total_cmp)
    }
}

/// `PlayerColliderHakoniwa::updateFallDistanceCheck`, which feeds
/// `PlayerJudgeLongFall`: the drop since the player first fell at the terminal
/// speed (`FallSpeedMax`), counted again from nothing whenever he stands on
/// the ground (the step that lands still counts) or falls slower.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct FallDistance {
    /// Height (centimetres) where the terminal speed was first reached.
    start: Option<f32>,
    /// The drop since then, in centimetres.
    distance: f32,
}
impl FallDistance {
    /// Called once per step after the collision with the height (cm) the
    /// player ended at, the vertical velocity he moved with, whether he is on
    /// the ground and whether he reached it in this step.
    fn update(&mut self, height: f32, vy: f32, on_ground: bool, just_landed: bool) {
        if (on_ground && !just_landed) || -vy < t::FALL_MAX {
            *self = Self::default();
            return;
        }
        let start = *self.start.get_or_insert(height);
        self.distance = (start - height).max(0.);
    }
}

/// Steps without ground a ground nerve tolerates before it falls
/// (`PlayerJudgeSpeedCheckFall`: more than four).
const GROUND_LOST_FRAMES: u32 = 4;

pub fn plugin(app: &mut App) {
    app.init_resource::<Course>()
        .add_message::<Fx>()
        .insert_resource(Time::<Fixed>::from_hz(FPS as f64))
        .add_systems(FixedUpdate, simulate)
        .add_systems(Update, present.before(crate::world::animate));
}
fn simulate(
    mut players: Query<&mut Player>,
    mut orders: ResMut<Orders>,
    course: Res<Course>,
    menu: Res<crate::ui::Menu>,
    settings: Res<crate::input::Settings>,
    wardrobe: Res<crate::local_assets::Wardrobe>,
    mut fx: MessageWriter<Fx>,
) {
    if menu.open {
        orders.clear_edges();
        return;
    }
    for mut p in &mut players {
        p.prevent_head_pain = wardrobe.head_prevents_pain(settings.outfit);
        p.step(&orders, &course);
        fx.write_batch(p.events.drain(..));
    }
    orders.clear_edges();
}
/// Present in a rendered replay: show each step's result, not the blend
/// toward it, so a captured frame is the state the trace records.
#[derive(Resource)]
pub struct ExactFrames;
fn present(
    mut players: Query<&mut Player>,
    fixed: Res<Time<Fixed>>,
    exact: Option<Res<ExactFrames>>,
) {
    let a = if exact.is_some() {
        1.
    } else {
        fixed.overstep_fraction().clamp(0., 1.)
    };
    for mut p in &mut players {
        // Teleports (respawn, ledge climbs) snap instead of sliding.
        p.shown = if p.prev.distance(p.pos) > 2. {
            p.pos
        } else {
            p.prev.lerp(p.pos, a)
        };
        p.shown_cap = p.cap.as_ref().map(|c| match p.prev_cap {
            Some(prev) if prev.distance(c.pos) < 3. => prev.lerp(c.pos, a),
            _ => c.pos,
        });
    }
}
/// What a ground jump asks of the air control: the base speed of a jump, the
/// jump state's limits on the take-off speed, and the extend window.
fn jump_takeoff(extend_frames: u32, power: f32, gravity: f32) -> air_move::Takeoff {
    air_move::Takeoff {
        speed_limit: t::JUMP_MOVE_MAX,
        base_speed: t::NORMAL_MAX,
        extend_frames,
        power,
        gravity,
        input_lock: 0,
        speed_window: Some((0., air_move::tune::JUMP_BASE_SPEED_MAX)),
        fall_max: t::FALL_MAX,
    }
}
/// What the air control is set up with for a jump of this state: the
/// spin jump keeps its own, lower, speed limit (`SpinJumpMoveSpeedMax`).
fn takeoff_of(mode: Move, extend_frames: u32, power: f32, gravity: f32) -> air_move::Takeoff {
    let takeoff = jump_takeoff(extend_frames, power, gravity);
    match mode {
        Move::SpinJump => air_move::Takeoff {
            speed_limit: t::SPIN_JUMP_MOVE_MAX,
            ..takeoff
        },
        _ => takeoff,
    }
}
/// A fall (`PlayerStateFallHakoniwa`'s first step): the vertical speed stays,
/// the speed range and gravity are those of a jump, and the speed the player
/// had is kept up to the jump speed limit.
fn fall_takeoff(constants: &PlayerConstants, vertical_speed: f32) -> air_move::Takeoff {
    air_move::Takeoff {
        speed_limit: t::JUMP_MOVE_MAX,
        base_speed: t::JUMP_MOVE_MIN,
        extend_frames: 0,
        power: vertical_speed,
        gravity: constants.jump_gravity,
        input_lock: 0,
        speed_window: None,
        fall_max: t::FALL_MAX,
    }
}
/// What `PlayerStateSpinCap::updateSpinAirNerve` asks of the air control on
/// its first step: speed limit and base speed `SpinAirSpeedMax`, lift
/// `SpinAirJumpPower`, gravity `GravitySpinAir`, no extension, the stick free.
fn spin_air_takeoff() -> air_move::Takeoff {
    air_move::Takeoff {
        speed_limit: t::SPIN_AIR_SPEED_MAX,
        base_speed: t::SPIN_AIR_SPEED_MAX,
        extend_frames: 0,
        power: t::SPIN_AIR_JUMP,
        gravity: t::SPIN_AIR_GRAVITY,
        input_lock: 0,
        speed_window: None,
        fall_max: t::FALL_MAX,
    }
}
impl Player {
    /// Initialization-only placement choice, shared by physics and animation.
    /// The original reads IsMoon once, before constructing the player states.
    pub fn init_placement(&mut self, is_moon: bool) {
        self.is_moon = is_moon;
        self.constants = if is_moon {
            &PlayerConstants::MOON
        } else {
            &PlayerConstants::NORMAL
        };
        self.gravity = self.constants.gravity_air;
        self.cycle.set_moon(is_moon);
    }

    fn move_animation(&self) -> &'static str {
        // PlayerAnimControlRun constructor (0x42c5b4) selects the slot from
        // initPlayer's IsMoon; ActionAnimCtrl supplies its RunMoon layer.
        if self.is_moon && !self.model_2d() {
            "MoveMoon"
        } else {
            "Move"
        }
    }

    pub fn flat_speed(&self) -> f32 {
        self.vel.with_y(0.).length()
    }
    fn speed_h(&self) -> f32 {
        self.v.with_y(0.).length()
    }
    /// A player standing at `pos` (metres) facing `yaw_deg` degrees (0 looks along +z, 90
    /// along +x), as the trace probe places the real game's: its pose is the quaternion
    /// `(0, sin(yaw/2), 0, cos(yaw/2))` of the exact angle, which the first `waitGround` leaves
    /// as it reads back.
    pub fn spawn_facing(pos: Vec3, yaw_deg: f32) -> Self {
        let yaw = yaw_deg.to_radians();
        let mut player = Self::spawn_at(pos, Vec3::new(yaw.sin(), 0., yaw.cos()));
        // The probe teleports a player that has stood for at least 120 steps; the collider's
        // count of steps in contact goes on from there (`rs::isJustLand` reads it).
        player.ground_contact = PROBE_STANDING_STEPS;
        let half = f64::from(yaw_deg).to_radians() / 2.;
        player.pose_tilt = Some(PoseTilt {
            pose: Quat::from_xyzw(0., half.sin() as f32, 0., half.cos() as f32),
            facing: player.facing,
        });
        player
    }
    /// Start the next update with no ground contact. The probe's placement calls
    /// `rs::resetCollision` (0x565ce8), which invalidates the ground result in
    /// `PlayerCollider::onInvalidate` (0x42fe8c). `rs::isOnGround` reads that result
    /// at 0x5687b0. The airborne recording enters Fall on the fifth step.
    pub fn start_in_air(&mut self) {
        self.grounded = false;
        self.ground_contact = 0;
    }
    pub fn prepare_recorded_teleport(&mut self) {
        self.joints.ground = ground_pose::GroundPose::settled_wait();
    }
    /// A player standing at `pos` (metres) facing the horizontal direction `facing`.
    pub fn spawn_at(pos: Vec3, facing: Vec3) -> Self {
        Self {
            pos,
            prev: pos,
            shown: pos,
            fall: FallDistance::default(),
            checkpoint: pos,
            facing: facing.with_y(0.).normalize_or(Vec3::NEG_Z),
            ..Self::default()
        }
    }
    /// Velocity in original units per frame, after the last step. On the
    /// ground the controller cancels the downward push inside the step; the
    /// original keeps reporting it, as the ground-following velocity. In the
    /// step that lands, it still reports the fall speed that moved the player.
    pub fn units_per_frame(&self) -> Vec3 {
        // (The pole's hold has no pull into the floor under it, and the slope slide keeps the
        // game's own velocity, the pull included.)
        if self.grounded
            && !self.in_damage_state()
            && !self.in_sand_state()
            && !matches!(
                self.mode,
                Move::Pole
                    | Move::Roll
                    | Move::RollEnd
                    | Move::SlopeSlide
                    | Move::Swim
                    | Move::BurnJump
                    | Move::BurnBounce
                    | Move::FireRun
                    | Move::FireFall
                    | Move::FireDead
                    | Move::Press
                    | Move::Dead
                    | Move::Bind
                    | Move::BindFall
            )
            && (self.v.y == 0. || self.v.y == -self.pull)
        {
            if self.landing_speed < 0. {
                self.v.with_y(self.landing_speed)
            } else if let Some((h, pull, velocity)) = self.ground_move
                && h == Vec2::new(self.v.x, self.v.z)
                && pull == self.pull
            {
                velocity
            } else {
                self.ground_following()
            }
        } else {
            self.v
        }
    }
    /// The velocity a ground state moves with: its horizontal speed laid onto
    /// the floor under it (`al::alongVectorNormalH`) and the pull into the
    /// floor along the floor's normal, which is straight down on level ground
    /// (recorded standing and running on a floor tilted by half a degree:
    /// `-pull * normal` at rest).
    fn ground_following(&self) -> Vec3 {
        // A run keeps its velocity in the plane it was steered on (the floor
        // the collider has found since is for the next step).
        let normal = match self.run_up {
            Some((h, _, velocity))
                if h == Vec2::new(self.v.x, self.v.z) && self.pull == GRAVITY_MOVE =>
            {
                // The velocity the control set, as it set it.
                return velocity;
            }
            _ => self.mesh_ground.unwrap_or(Vec3::Y),
        };
        let mut along = self.v.with_y(0.);
        if normal.y > 0. {
            along.y = -along.dot(normal) / normal.y;
        }
        along - normal * self.pull
    }
    /// The clip frame animation triggers (sounds, effects) are scanned up to.
    /// While a sub animation plays the same clip, the model's own action
    /// frame decides, not the animator's counter.
    pub fn trigger_frame(&self) -> f32 {
        match self.model_frame() {
            Some(frame) => frame,
            None => self
                .loop_pose_frame()
                .unwrap_or_else(|| self.anim.trigger_frame()),
        }
    }
    /// The frame effect entries (start and end frames) are matched against: the trigger frame
    /// within the clip, a loop that has just run past the clip's length counted from its start.
    pub fn effect_frame(&self) -> f32 {
        let frame = self.trigger_frame();
        match self.loop_len(self.anim.name) {
            Some(length) if self.loop_slack_of(self.anim.name) > 0. && frame >= length => {
                frame - length
            }
            _ => frame,
        }
    }
    /// The pose's frame of a clip whose counter shows its length for a row before it wraps,
    /// without the wrap: the counter plus the rate change it has still to take over. The loop
    /// restarts for the triggers when this reaches the clip's length.
    fn loop_pose_frame(&self) -> Option<f32> {
        let length = self.loop_len(self.anim.name)?;
        (self.loop_slack_of(self.anim.name) > 0.).then(|| {
            let wrapped = if self.counter_wrapped { length } else { 0. };
            self.anim.frame + self.anim_rate_change + wrapped
        })
    }
    /// A looping clip with a slack starts its loop for the triggers in the step its pose frame
    /// reaches the clip's length (the pose wraps there, the counter a row later).
    fn update_loop_start(&mut self) {
        let Some(length) = self.loop_len(self.anim.name) else {
            self.pose_seen = None;
            return;
        };
        let Some(pose) = self.loop_pose_frame() else {
            self.pose_seen = None;
            return;
        };
        self.clip_wrapped = matches!(self.pose_seen,
            Some((serial, before)) if serial == self.anim.serial && before < length && pose >= length);
        self.pose_seen = Some((self.anim.serial, pose));
    }
    /// The frame of the model's action when a sub animation is playing the
    /// main clip too.
    pub fn model_frame(&self) -> Option<f32> {
        if self.crushed() {
            return Some(self.press.frozen_frame);
        }
        (self.sub.active && self.sub.name == self.anim.name).then_some(self.sub.frame)
    }
    /// The cap's surface effects as they run (skimming, resting).
    pub fn cap_water_effects(&self) -> (bool, bool) {
        match &self.cap {
            Some(cap) => cap.water_effects(),
            // The cap that arrived this frame is gone from the player, its effects end with its
            // next control.
            None if self.catching == Some(-1) => self.cap_water_left,
            None => (false, false),
        }
    }
    /// The cap's surface effects that were started and deleted within this step.
    pub fn cap_water_flashes(&self) -> (bool, bool) {
        self.cap
            .as_ref()
            .map_or((false, false), |cap| cap.water_flashes())
    }
    /// What the trace shows of the cap: its nerve, position (metres) and clip.
    /// Besides the flight this covers the spin attack before the throw frame
    /// and the catch after the arrival, when the cap sits on the player.
    pub fn cap_row(&self) -> Option<CapRow> {
        if let Some(cap) = &self.cap {
            return Some(CapRow {
                state: cap.state_name(),
                pos: cap.pos,
                clip: cap.clip(),
            });
        }
        if self.pop_catch.is_some() {
            return Some(CapRow {
                state: "Catch",
                pos: self.prev,
                clip: "JumpCapCatch",
            });
        }
        if let Some(frames) = self.catching {
            return Some(CapRow {
                state: if frames < 0 {
                    "Return"
                } else if self.winding && self.winding_age > 0 {
                    "SpinAttack"
                } else {
                    "Catch"
                },
                // The cap moves before the player, so it sits where the
                // player was when the frame began.
                pos: self.prev,
                // (A throw begun in the catch's rows has started the cap's spin
                // attack action already; the cap's nerve follows a frame later.)
                clip: if self.winding {
                    self.throw_clip
                } else {
                    self.catch_clip
                },
            });
        }
        // The spin attack shows once the cap's nerve has been applied, the frame
        // after the state's first step, and until the cap is flying.
        // The cap, which moves before the player, follows where the player was
        // when the frame began.
        (self.winding && self.winding_age > 0 || self.launch.is_some()).then_some(CapRow {
            state: "SpinAttack",
            pos: self.prev,
            clip: self.throw_clip,
        })
    }
    /// The model's sub animation while it plays: its name and frame.
    pub fn ended_model_sub(&self) -> Option<(&'static str, f32)> {
        self.ended_sub
    }

    pub fn model_sub(&self) -> Option<(&'static str, f32)> {
        self.sub.active.then_some((self.sub.name, self.sub.frame))
    }
    /// The normal of the ground under the feet, when standing on it.
    pub fn floor_normal(&self) -> Option<Vec3> {
        // A ramp's height rises along +x by `slope` per metre; on a triangle
        // mesh the collider's average floor normal is the answer.
        (self.grounded || self.hop_contact).then(|| {
            self.mesh_ground
                .unwrap_or_else(|| Vec3::new(-self.slope, 1., 0.).normalize())
        })
    }
    /// The dust a one-step run state started (see `run_start_effect`).
    pub fn run_start_effect(&self) -> Option<&'static str> {
        self.run_start_effect
    }
    /// The run cycle's animation control, for the trace and the renderer.
    pub fn run_cycle(&self) -> &RunCycle {
        &self.cycle
    }
    /// The clip frame a trace row shows: the animator's counter, which for the
    /// run cycle is the previous pose moved on, not wrapped.
    pub fn shown_frame(&self) -> f32 {
        if self.is_run_cycle() {
            self.cycle.shown
        } else {
            self.anim.frame
        }
    }
    /// The 8-bit model is the visible one (`PlayerModelChangerHakoniwa` 2D model): the
    /// keeper has changed to 2D.
    pub fn model_2d(&self) -> bool {
        self.dimension.is_2d
    }
    /// Mario2D has no effect user/table in EffectDataBase (init at 0x4455cc).
    /// Actor hit reactions retain their own PlayerHitReactionHakoniwa keeper.
    pub fn model_effect_action(&self) -> &'static str {
        if self.model_2d() { "" } else { self.anim.name }
    }
    pub fn is_run_cycle(&self) -> bool {
        !self.model_2d() && matches!(self.anim.name, "Move" | "MoveMoon")
    }
    /// Length of a clip that loops, in the model that is on screen: the 8-bit model's own
    /// action lengths (`PlayerAnimation2D`, as converted: Wait is 60 frames) replace the 3D
    /// ones.
    pub fn loop_len(&self, name: &str) -> Option<f32> {
        if self.model_2d() {
            return crate::model_2d::action_length(name)
                .filter(|(_, looping)| *looping)
                .map(|(frames, _)| frames as f32);
        }
        clip::loop_length(name)
    }
    /// How long a looping clip's counter shows its length before it wraps, in the model on
    /// screen (the 8-bit Wait shows its 60 for a row).
    pub fn loop_slack_of(&self, name: &str) -> f32 {
        // How far past its length the 8-bit loop's counter runs before it wraps. Without a
        // sub animation `PlayerAnimator::updateAnimFrame` (0x42a214, flag +0x1a2 set by
        // startSubAnim 0x42a74c, cleared by endSubAnim 0x42a898) calls
        // `PlayerAnimFrameCtrl::updateSync` (0x42d170): counter = the model action's own
        // frame plus its rate. The model's loop wraps at its length, so the counter leads
        // the pose by the action rate and reads the length for one row.
        if self.model_2d() {
            return if self.loop_len(name).is_some() {
                self.anim.rate
            } else {
                0.
            };
        }
        clip::loop_slack(name)
    }
    pub fn held_length_of(&self, name: &str) -> Option<u32> {
        if self.model_2d() {
            // PlayerAnimation2D's visibility actions: the animator's counter runs their length.
            return crate::model_2d::action_length(name)
                .filter(|(_, looping)| !*looping)
                .map(|(frames, _)| frames);
        }
        clip::held_length(name)
    }
    /// Last frame the pose layer shows of a held clip. The 8-bit model's Jump, Fall and
    /// JumpTrample skeletal clips are one frame long in the original PlayerAnimation2D
    /// archive (the installed Mario2D curves), while their visibility actions (the animator's
    /// counter) are five frames long: the layer holds frame 1 as the counter runs on.
    pub fn layer_hold_length_of(&self, name: &str) -> Option<u32> {
        let held = self.held_length_of(name)?;
        if self.model_2d() {
            let clip = crate::model_2d::clip_frames(name)?;
            return Some(held.min(clip as u32));
        }
        Some(held)
    }
    /// The original Mario2D and PlayerAnimation2D archives have no AnimInterp
    /// resource. ModelAnimInterpInfoHolder initializes its default to zero
    /// (0x8028fc); missing AnimInterp returns without replacing it (0x802958),
    /// and a missing transition returns that default (0x802f6c).
    pub fn interp_frames_of(&self, to: &str, from: &str) -> u32 {
        if self.model_2d() {
            return 0;
        }
        crate::local_assets::interp_frames(to, from) as u32
    }
    /// The row of a model switch: the new animator's counter still reads 0 while its pose
    /// layer shows the synced frame (`PlayerModelChangerHakoniwa` restarts the action).
    pub fn model_switch_row(&self) -> bool {
        self.model_switch_row
    }
    /// The row after a bound model switch: the counter reads the synced frame.
    pub fn switch_sync_row(&self) -> bool {
        self.switch_sync_row
    }
    /// The first row of a clip a pipe's puppet action started (see `play_puppet_action`).
    pub fn puppet_synced_row(&self) -> bool {
        self.puppet_first_row && self.anim.frame == 0.
    }
    /// The body hangs on a ledge or clings to a pole and touches ground there
    /// (the top of the pole, the ledge the climb ends on): that contact is no
    /// landing (only the climb jump's and the pole jump's are).
    fn stands_on_ledge(&self) -> bool {
        (self.mode == Move::GrabCeil && self.grab_ceil.is_form())
            || (self.mode == Move::Ledge && self.catch_nerve() != ledge::CatchNerve::Jump)
            || (self.mode == Move::Pole && !self.pole.is_pole_jump())
    }
    /// A sub-nerve was chosen again in this step: the row's `sub_step` is 0.
    pub fn sub_restarted(&self) -> bool {
        self.sub_restarted
    }
    /// The clip this step started does not blend in from the previous one.
    pub fn interpolation_cleared(&self) -> bool {
        self.interpolation_cleared
    }
    /// The swim state has a nerve the actor chose after the state's update this step (the cap
    /// throw): the trace shows the old one and a step counter of -1.
    pub fn nerve_chosen_late(&self) -> bool {
        self.death.abyss_pending
            || self.mode == Move::Swim && self.swim.chosen_after_update.is_some()
    }
    /// Steps the actor nerve has executed, as the game's `state_step` counts
    /// them: 0 in the frame a nerve is chosen, 1 after its first step.
    pub fn state_frames(&self) -> u32 {
        self.nerve.step
    }
    /// The counters that gate moves, by name (frames left or elapsed, uses).
    pub fn counters(&self) -> Vec<(&'static str, i64)> {
        vec![
            ("pre_jump", self.pre_jump as i64),
            ("pre_throw", self.pre_throw as i64),
            ("ground_lost", self.ground_lost as i64),
            ("jump_count", self.combo.count as i64),
            ("jump_timer", self.combo.timer as i64),
            ("long_jump_count", self.long_combo.count as i64),
            ("long_jump_timer", self.long_combo.timer as i64),
            ("run_frames", self.run_frames as i64),
            ("after_turn", self.after_turn as i64),
            ("quick_turn", self.quick_turn as i64),
            ("catch_window", self.catch_window as i64),
            ("land_frames", self.land_frames as i64),
            ("cap_jump_possible", self.cap_jump_possible as i64),
            ("dive_possible", self.dive_possible as i64),
            ("run_steps", self.run_steps as i64),
        ]
    }
    /// Set one of the counters `counters` lists; false for any other name.
    pub fn set_counter(&mut self, name: &str, value: i64) -> bool {
        let frames = value.max(0) as u32;
        match name {
            "pre_jump" => self.pre_jump = frames,
            "pre_throw" => self.pre_throw = frames,
            "ground_lost" => self.ground_lost = frames,
            "jump_count" => self.combo.count = frames,
            "jump_timer" => self.combo.timer = frames,
            "long_jump_count" => self.long_combo.count = frames,
            "long_jump_timer" => self.long_combo.timer = frames,
            "run_frames" => self.run_frames = frames,
            "after_turn" => self.after_turn = frames,
            "quick_turn" => self.quick_turn = frames,
            "catch_window" => self.catch_window = frames,
            "land_frames" => self.land_frames = frames,
            "cap_jump_possible" => self.cap_jump_possible = value != 0,
            "dive_possible" => self.dive_possible = value != 0,
            "run_steps" => self.run_steps = frames,
            _ => return false,
        }
        true
    }
    /// Put the player into the recorded state. State the trace does not show
    /// (gravity of the jump in progress, how far a jump may still extend,
    /// air-speed cap) is set to what entering that state would have given.
    pub fn inject(&mut self, s: &Snapshot) {
        // A live injected snapshot overrides our dead status. Guess: health is not exported,
        // so a stage restart in lockstep initializes the standard three-HP keeper. This is
        // snapshot reconstruction only; free replay deliberately does not simulate respawn.
        // (A fall into a death area that was just decided keeps its health: its nerve starts
        // in the step that follows.)
        if self.is_dead()
            && !self.death.abyss_pending_for_trace()
            && s.mode.is_some_and(|m| {
                !matches!(
                    m,
                    Move::FireDead
                        | Move::Press
                        | Move::Dead
                        | Move::AbyssFall
                        | Move::DamageDead
                        | Move::DamageSwimDead
                        | Move::SandDead
                )
            })
        {
            self.damage = Damage::default();
            self.soot = Soot::default();
        }
        self.nerve.clear();
        self.pos = s.pos;
        self.snapshot_injected = true;
        self.stage_translation = None;
        self.stage_carry = None;
        self.prev = s.pos;
        self.v = s.v;
        if s.mode == Some(Move::SandGeyser) {
            self.collider.invalidate(self.facing.x.atan2(self.facing.z));
            self.trans_carry = s.geyser_collider_from;
        }
        // A recording without the probe's bits prints the pose rounded to six decimals. The pose
        // the preceding step left, when it prints as the same text, is that very pose: keep it,
        // not its rounding (a decision that reads the side axis against the camera's look
        // direction, `rs::isPlayerSideFaceToCameraZ`, can sit within the rounding of the zero).
        let prints_as_recorded = |own: Quat, recorded: Quat| {
            let print = |q: Quat| q.to_array().map(|v| (f64::from(v) * 1e6).round() as i64);
            print(own) == print(recorded)
        };
        let keeps_own_pose = s.pose_rounded
            && s.pose.zip(self.pose_tilt).is_some_and(|(recorded, own)| {
                // (A facing that turned since the pose was stored is not the pose's own.)
                prints_as_recorded(own.pose, recorded) && own.facing == self.facing
            });
        if !keeps_own_pose {
            self.facing = s.facing.with_y(0.).normalize_or(self.facing);
        }
        self.grounded = s.grounded;
        if let Some(pose) = s.pose
            && !keeps_own_pose
        {
            self.set_pose(pose);
        }
        if let Some((codes, normal, steps)) = &s.floor_contact {
            self.touched.active = true;
            self.touched.floor = Some(codes.clone());
            self.mesh_ground = Some(*normal);
            self.ground_contact = *steps;
        }
        if self.rotating_ground && self.collider.floor.is_some() {
            // The recorded decimal normal loses the bit at the run-angle
            // boundary. Keep the independently simulated source/KCL normal.
            self.mesh_ground = Some(self.collider.ground_normal);
        }
        // The jump off the cap (`PlayerStateJump` request 11, `trample_jump`) is the actor's `Jump`
        // nerve in a recording: the request that chose its gravity and window is the state's own.
        // The one the preceding step started, in the same step of the state, is that jump.
        let continues_cap_jump = self.mode == Move::CapBounce
            && s.mode == Some(Move::Jump)
            && s.state_frames == Some(self.frame);
        if let Some(mode) = s.mode.filter(|_| !continues_cap_jump) {
            self.mode = mode;
            if matches!(
                mode,
                Move::AbyssFall
                    | Move::DamageDead
                    | Move::DamageSwimDead
                    | Move::SandDead
                    | Move::Press
            ) {
                // The recorded state is the death-area fall: the actor is dead (health 0).
                self.damage.hp = 0;
            }
            self.gravity = match mode {
                Move::DoubleJump => self.constants.jump_gravity2nd,
                Move::TripleJump => self.constants.jump_gravity3rd,
                Move::Backflip | Move::Sideflip => self.constants.turn_jump_gravity,
                Move::LongJump => self.constants.long_jump_gravity,
                Move::SpinJump => self.constants.spin_jump_gravity,
                Move::PoundJump => self.constants.jump_gravity,
                _ => self.constants.gravity_air,
            };
            self.max_air_speed = match mode {
                Move::Sideflip => t::TURN_JUMP_VEL_H,
                Move::Backflip => t::SQUAT_JUMP_MOVE_MAX,
                Move::SpinJump => t::SPIN_JUMP_MOVE_MAX,
                _ => self.speed_h().clamp(t::JUMP_MOVE_MIN, t::JUMP_MOVE_MAX),
            };
        }
        if let Some(frames) = s.state_frames {
            self.frame = frames;
            self.nerve.step = frames;
        }
        // The pound's landing, the ground spin and the spin drop count their own
        // steps (the cancel window, the spin's ninety, the drop's minimum are
        // read from them), not the actor nerve's.
        if matches!(
            self.mode,
            Move::PoundLand
                | Move::GroundSpin
                | Move::SpinDrop
                | Move::DamageAir
                | Move::DamageLand
                | Move::DamageDead
                | Move::DamageSwim
                | Move::DamageSurface
                | Move::DamageLandWater
                | Move::DamageSwimDead
        ) && let Some(steps) = s.sub_steps
        {
            self.frame = steps;
        }
        if let Some(steps) = s.sub_steps {
            if self.burning() || self.is_dead() || self.in_sand_state() {
                self.frame = steps;
            }
            self.set_wait_steps(steps);
        }
        self.resume_recovery_clock(s);
        // On the ground in a state that flies: the contact row, whose landing
        // the next step decides.
        self.nerve.touchdown =
            self.grounded && self.mode.in_flight() && !self.burning() && !self.is_dead();
        // The row reports the fall speed that reached the ground; the ground
        // has already stopped it (the bubble's fall lands by itself).
        if self.nerve.touchdown || self.grounded && self.mode == Move::RecoveryFall {
            self.touchdown_speed = self.v.y;
            self.v.y = 0.;
        }
        self.spin_fall = s.spin_fall;
        self.nerve.actor = self.actor_name();
        self.jump_extend = matches!(self.mode, Move::Jump | Move::DoubleJump)
            && self.frame < t::EXTEND_FRAMES
            && self.v.y > 0.;
        let keep_forced_air = s.anim.is_some_and(|(name, _)| name == "JumpDashFast")
            && self.air_for == Some(self.mode);
        if !keep_forced_air {
            self.air_for = None;
            if self.mode.uses_air_control() && (!self.grounded || self.nerve.touchdown) {
                self.resume_air_control(s);
            }
        }
        if self.mode == Move::WallSlide {
            self.resume_wall_slide(s);
        }
        if let Some((name, frame)) = s.anim {
            // A puppet-started action's first sample is updateSync's counter
            // (0x42d170), one rate ahead of its model pose. Injection must restore
            // the pose counter, otherwise the following update advances it twice.
            let frame = if (self.model_switch_row || self.counter_restarted)
                && self.anim.name == name
            {
                // copyAnimLocal starts the counter again while carrying the
                // prior model's action pose (0x42c1cc..0x42c250). UnRoll
                // likewise copies Rolling's frame to RollingEnd while its
                // exported counter restarts (0x47c470..0x47c498).
                self.anim.frame
            } else if (self.puppet_synced_row() || self.switch_sync_row) && self.anim.name == name {
                frame - self.anim.rate
            } else {
                frame
            };
            // A looping clip the port wraps at its length (no `loop_slack`) reads past it in
            // the recorded row its pose wraps in (the burning run's 64.8, then 10.2): the
            // port's counter is already at the wrapped value, and wrapping it again at the
            // next update would start the loop's triggers a second time.
            let frame = match self.loop_len(name) {
                Some(length) if self.loop_slack_of(name) == 0. && frame >= length => frame - length,
                _ => frame,
            };
            if let Some(clockwise) = spin::clip_is_clockwise(name) {
                self.spin_clockwise = clockwise;
            }
            if self.anim.name != name {
                self.start_clip(name);
            }
            self.anim.frame = frame;
            if matches!(name, "Move" | "MoveMoon") {
                self.step_phase = frame;
            }
        }
        // (`exeSpinGround` pulls with the ground control's gravity like the run states do.)
        self.pull = if matches!(self.mode, Move::Walk | Move::Run | Move::Skid | Move::Turn)
            || self.mode == Move::Throw && !self.throwing.in_air
        {
            GRAVITY_MOVE
        } else {
            t::GRAVITY
        };
        if self.grounded {
            // A snapshot holds native actor velocity; the port stores
            // horizontal travel separately from its floor-normal pull.
            // Preserve the independently simulated run's steering plane,
            // which predates the collision normal visible in the same row.
            let travel = if self.rotating_ground && self.mode == Move::Idle && !self.land_nerve() {
                Some(Vec3::ZERO)
            } else if self.mode == Move::Crouch
                && s.sub_steps.or(s.state_frames) == Some(0)
                && s.anim.is_some_and(|(clip, _)| clip == "SquatWalk")
            {
                // exeWalk's no-input branch sets only -normal * GravityMove
                // before choosing Wait (0x484164..0x4841ac). Nested re-choice
                // can inject this row directly; none of its XZ is travel.
                Some(Vec3::ZERO)
            } else if matches!(self.mode, Move::Idle | Move::Crouch)
                && !self.land_nerve()
                && s.sub_steps
                    .or(s.state_frames)
                    .is_some_and(|steps| steps > 0)
            {
                // Executed waitGround rows contain -normal * Gravity
                // (0x56575c..0x56577c). Undo that addition before restoring
                // our horizontal travel; otherwise Squat applies it twice
                // and a deferred Jump inherits it as tangent velocity.
                // Step-zero choices and Land's snap have different owners.
                let normal = self.mesh_ground.unwrap_or(Vec3::Y);
                Some((s.v + normal * self.pull).with_y(0.))
            } else if self.rotating_ground && matches!(self.mode, Move::Walk | Move::Run) {
                self.run_up.map(|(_, normal, _)| {
                    let travel = (s.v + normal * self.pull).with_y(0.);
                    self.run_up = Some((Vec2::new(travel.x, travel.z), normal, s.v));
                    travel
                })
            } else if matches!(self.mode, Move::Walk | Move::Run)
                && self.mesh_ground.is_some_and(|normal| normal.y < 1.)
            {
                // A Run row's velocity is its travel plus -normal * GravityMove; on a slope
                // that pull has horizontal parts the steering would take for travel.
                let normal = self.mesh_ground.unwrap_or(Vec3::Y);
                Some((s.v + normal * self.pull).with_y(0.))
            } else {
                None
            };
            if let Some(travel) = travel {
                self.v = travel;
                self.ground_move = Some((Vec2::new(travel.x, travel.z), self.pull, s.v));
            }
        }
        // The trace's velocity includes the floor pull a grounded state applies; the
        // body carries only what is left of it, so a state chosen next (a run
        // pulling harder) starts from zero as it does in free replay.
        if self.grounded
            && self.v.y == -self.pull
            && !self.mode.in_flight()
            && !matches!(self.mode, Move::Roll | Move::RollEnd)
            && !self.in_sand_state()
        {
            self.v.y = 0.;
        }
        if matches!(self.mode, Move::Walk | Move::Run | Move::Skid | Move::Turn) {
            self.run_nerve = match self.mode {
                Move::Skid => RunNerve::Brake,
                Move::Turn if self.anim.name == "TurnPoint" => RunNerve::Pivot,
                Move::Turn => RunNerve::Turn,
                _ if self.anim.name == "Push" => RunNerve::WallPush,
                _ => RunNerve::Run,
            };
            self.brake_dir = match self.mode {
                Move::Turn if self.run_nerve == RunNerve::Turn => -self.facing,
                _ => self.facing,
            };
            let steps = s.run.as_ref().map_or(self.frame, |hint| hint.steps);
            self.run_steps = steps;
            if let Some(hint) = &s.run {
                if hint.after_turn {
                    self.run_nerve = RunNerve::RunAfterTurn;
                }
                self.cycle
                    .resume(hint.phase, hint.rate, hint.weights, hint.start_frame, steps);
                self.cycle.set_moon(self.is_moon);
                self.step_phase = hint.phase;
                // Forced/surface steering carries hidden turn acceleration;
                // the printed facing delta is not that controller's turn rate.
                if !(self.force_run.is_active() || self.surface_run.result) {
                    self.ground.resume(
                        self.speed_h(),
                        steps,
                        hint.turn_rate,
                        hint.last_input,
                        hint.turn_power,
                        self.on_ground_skate(),
                    );
                }
                // A nerve chosen in this row starts in the next step.
                if let Some(clip) = hint.chosen {
                    let (mode, nerve) = match clip {
                        "Brake" => (Move::Skid, RunNerve::Brake),
                        "Turn" => (Move::Turn, RunNerve::Turn),
                        "TurnPoint" => (Move::Turn, RunNerve::Pivot),
                        // exePivot chooses Run once the pivot completes with input
                        // (0x47ef98..0x47efb4); Brake would print the same label.
                        _ if self.anim.name == "TurnPoint" => (Move::Walk, RunNerve::Run),
                        _ if self.anim.name == "Turn" => (Move::Walk, RunNerve::RunAfterTurn),
                        _ => (self.mode, RunNerve::Run),
                    };
                    self.mode = mode;
                    self.run_steps = 0;
                    self.brake_dir = self.facing;
                    self.nerve.queue(clip, move |p| p.enter_nerve(nerve));
                }
            }
        }
        if matches!(self.mode, Move::Roll | Move::RollEnd) {
            self.resume_roll(
                s.state_frames.unwrap_or(0),
                s.sub_steps.unwrap_or(0),
                s.next_clip,
                s.roll_brake,
            );
        }
        self.resume_squat(s);
        self.resume_sand(s);
        self.resume_slope_slide(s);
        self.resume_cap(s);
        for (name, value) in &s.counters {
            if self.rotating_ground && name == "ground_lost" {
                // The snapshot infers this from physical contact rows. A
                // rotating floor can fail the original run-angle judge while
                // still touching, so retain that independently updated judge.
                continue;
            }
            self.set_counter(name, *value);
        }
        // The fall distance counts from the height where the terminal speed was first reached,
        // which the rows of that fall show (`Snapshot::fall_start`, centimetres).
        self.fall = match s.fall_start {
            Some(start) => FallDistance {
                start: Some(start),
                distance: (start - self.pos.y / UNIT).max(0.),
            },
            None => FallDistance::default(),
        };
    }
    /// The cap and the spin throw as a recorded row shows them.
    fn resume_cap(&mut self, s: &Snapshot) {
        let previous = self.cap.take();
        let pending_launch = self.launch.take();
        self.winding = false;
        self.winding_age = 0;
        let continued_catch = self.catching.is_some() && s.catching.is_some();
        self.catching = s.catching;
        // Catch rows establish +0x2a3 (receiveMsg 0x408b2c). A hidden cap in a snapshot
        // does not expose that byte: refuse the pop until an actual catch establishes it.
        self.cap_catched =
            s.catching.is_some() && !s.throw.is_some_and(|hint| hint.winding || hint.launch);
        if !continued_catch && self.upper_catch.is_none() {
            self.catch_clip = self.clip_of_catch();
        }
        // The partial catch started by movement (0x41deac) survives a main
        // or model-sub change until clearUpperBodyAnim (0x42b00c) or its end.
        self.sub = SubClip::default();
        if let Some((name, frame, length)) = s.sub {
            self.sub = SubClip {
                name,
                frame,
                length,
                active: true,
            };
            // A clip just started under a sub animation holds its first frame; the clip may start
            // under a sub animation already in progress (the area clip under a catch), then the
            // row where the nerve's step restarts is the one that started it.
            let first_frame = s.anim.is_some_and(|(_, clip_frame)| clip_frame == 0.);
            // (`PlayerAnimFrameCtrl::update` skips once after `startAction`, whichever frame
            // of the sub animation the clip starts under.)
            self.anim_skip =
                first_frame && (frame == 0. || s.sub_steps == Some(0) || s.clip_started);
        } else {
            self.anim_skip = false;
        }
        if let Some(hint) = s.throw {
            self.throwing.kind = Some(hint.kind);
            self.throwing.count = hint.count;
            self.throwing.continuous = hint.continuous;
            self.winding = hint.winding;
            self.winding_age = hint.age;
            self.throw_clip = s.anim.map_or("SpinCapStart", |(name, _)| name);
        }
        let front = self.facing.with_y(0.).normalize_or(Vec3::NEG_Z);
        let launch = Launch {
            front,
            kind: self.throwing.kind.unwrap_or(Throw::Forward),
            left: false,
            air: !self.grounded,
            continuous: self.throwing.continuous,
            spiral_side: spiral_side_of(self.throwing.kind, self.throw_clip.ends_with("Left")),
            up: Vec3::Y,
        };
        if s.throw.is_some_and(|hint| hint.launch) {
            // A launch the preceding step handed the cap (its direction, up axis and side are the
            // throw frame's own) stays: the row only says that one is pending.
            self.launch = pending_launch.or(Some(launch));
        }
        if let Some(moment) = &s.cap {
            let launch = Launch {
                front: moment.velocity.with_y(0.).normalize_or(front),
                continuous: crate::cap::is_continuous_speed(moment.speed),
                ..launch
            };
            let mut cap = Cap::start(launch, self.pos, self.cap_serial + 1);
            cap.resume(moment);
            cap.frames = moment.step + 1;
            // The same clip keeps playing from one injected row to the next.
            if let Some(previous) = &previous {
                cap.continue_clip(previous);
            }
            // A cap in a lock-on is attached to the target nearest to it: the target's own state
            // (it may have released this very frame) does not say which one it is.
            if moment.nerve == crate::cap::CapNerve::LockOn {
                let targets = self.lock_targets();
                if let Some((index, view)) = targets.iter().enumerate().min_by(|a, b| {
                    (a.1.target - moment.spot)
                        .length_squared()
                        .total_cmp(&(b.1.target - moment.spot).length_squared())
                }) {
                    cap.restore_lock_on(index, view, moment);
                }
            }
            self.cap_serial = cap.clip_serial();
            self.cap = Some(cap);
        }
    }
    /// Tell the player where the camera is, in centimetres relative to the replay's start
    /// (`None` when the host has no camera): the sound control hears the camera dip into the
    /// water.
    pub fn set_camera_position(&mut self, camera_cm: Option<Vec3>) {
        self.camera_cm = camera_cm;
    }
    /// The material code's water prefix: the root is in a water area and the water is deeper
    /// than a puddle (`PlayerActorHakoniwa::movement`: `isInWater(trans)` and not
    /// `rs::isInPuddleHeight`).
    pub fn material_water(&self) -> bool {
        self.material_water
    }
    /// Drops run off the player after the water (the `StateWet` effect emits).
    pub fn is_dripping(&self) -> bool {
        self.wet.dripping
    }
    pub fn reset(&mut self) {
        let checkpoint = self.checkpoint;
        let assist = self.recovery.assist;
        let prevent_head_pain = self.prevent_head_pain;
        *self = Self::default();
        self.recovery.assist = assist;
        self.prevent_head_pain = prevent_head_pain;
        self.pos = checkpoint;
        self.checkpoint = checkpoint;
    }
    /// Move to another state of the same actor nerve and act in it right
    /// away: the run's walk, skid and turn are one nerve in the game, so the
    /// step counter keeps going and the new state's physics start this frame.
    fn set(&mut self, mode: Move) {
        if self.mode != mode {
            self.mode = mode;
            self.frame = 0;
        }
    }
    /// Choose the next state, as a nerve does with `setNerve`: this frame's row
    /// already reads the new state at step 0, while the clip and everything the
    /// state does (`work`) wait for its first step, the next frame.
    fn change(
        &mut self,
        mode: Move,
        clip: &'static str,
        work: impl FnOnce(&mut Player) + Send + Sync + 'static,
    ) {
        self.mode = mode;
        self.frame = 0;
        if mode == Move::Idle {
            self.wait_state_appears_on_platform();
        }
        self.nerve.choose(true, clip, work);
    }
    /// The next sub-state of the running nerve, started on the next step like a
    /// new nerve but without restarting the nerve's own step counter.
    fn defer(
        &mut self,
        mode: Move,
        clip: &'static str,
        work: impl FnOnce(&mut Player) + Send + Sync + 'static,
    ) {
        self.mode = mode;
        self.frame = 0;
        self.nerve.choose(false, clip, work);
    }
    /// A new sub-state of the running nerve (the run's skid and turn): the
    /// nerve keeps its step count and keeps acting this frame, but the
    /// sub-state's clip and first-step work come next frame.
    fn switch(
        &mut self,
        mode: Move,
        clip: &'static str,
        work: impl FnOnce(&mut Player) + Send + Sync + 'static,
    ) {
        self.mode = mode;
        self.frame = 0;
        self.nerve.queue(clip, work);
    }
    /// A state with nothing to do on its first step but start its clip.
    fn go(&mut self, mode: Move, clip: &'static str) {
        self.change(mode, clip, |_| {});
    }
    /// Start a clip now: the first step of a state, or an injected snapshot.
    fn start_clip(&mut self, name: &'static str) {
        let name = if name == "Move" {
            self.move_animation()
        } else {
            name
        };
        self.restart_wait_steps(name);
        self.anim = Anim {
            name,
            serial: self.anim.serial.wrapping_add(1),
            frame: 0.,
            rate: 1.,
        };
        self.anim_skip = true;
        if matches!(name, "Move" | "MoveMoon") {
            // The run cycle starts over in the nerve that plays it.
            self.cycle.start();
            self.step_phase = 0.;
            self.run_nerve = RunNerve::Run;
            self.run_steps = 0;
        }
    }
    /// A state asks for another clip (its sub-phase began): it starts on the
    /// next step, like the clip a nerve's first step starts.
    fn play(&mut self, name: &'static str) {
        self.nerve.queue_clip(name);
    }
    /// The rate change the animator's counter has still to take over (see `anim_rate_change`).
    pub fn anim_rate_change(&self) -> f32 {
        self.anim_rate_change
    }
    /// The clip the state plays or is about to play.
    fn clip(&self) -> &'static str {
        self.nerve.pending_clip().unwrap_or(self.anim.name)
    }
    /// The clip the trace names the state by: the state's own phase, which
    /// leads the visible clip by the frame the clip waits.
    pub fn phase_clip(&self) -> &'static str {
        self.clip()
    }
    /// The game's name for the state the player is in.
    pub fn nerve_name(&self) -> crate::game_names::Nerve {
        if self.model_2d() && matches!(self.mode, Move::Walk | Move::Run | Move::Skid | Move::Turn)
        {
            return crate::game_names::known("Run/Run2D");
        }
        if self.spin_fall && self.mode == Move::Fall {
            return crate::game_names::known("SpinCap/Fall");
        }
        if self.is_roll_brake() {
            return crate::game_names::known("Rolling/Brake");
        }
        if self.mode == Move::Throw && self.throwing.in_air {
            return crate::game_names::known("SpinCap/SpinAir");
        }
        if self.mode == Move::Ledge {
            return crate::game_names::known(self.catch_nerve().name());
        }
        if self.mode == Move::Pole {
            return crate::game_names::known(self.pole_nerve().name());
        }
        if self.mode == Move::GrabCeil {
            return crate::game_names::known(self.grab_ceil_name());
        }
        if self.mode == Move::Swim {
            return crate::game_names::known(self.swim.shown_nerve().name());
        }
        crate::game_names::nerve(self.mode, self.clip())
    }
    /// The actor nerve of that state (the part before the sub-state).
    fn actor_name(&self) -> &'static str {
        let name = self.nerve_name().name;
        name.split('/').next().unwrap_or(name)
    }
    /// The clip has played to its last frame (`isAnimEnd`): its frame count is
    /// reached, which the row shows one frame before the state reacts.
    fn anim_done(&self, frames: u32) -> bool {
        self.anim.frame >= frames as f32
    }
    /// A standing state is still playing a clip the idle loop waits for: a
    /// landing or the stand-up. Standing up is the end of the squat nerve and
    /// the landings are the first sub-state of the wait nerve.
    fn standing_clip_playing(&self) -> bool {
        let frames = match self.clip() {
            "Land" | "LandTurn" | "LandDownFall" => clip::LAND,
            "LandStiffen" => clip::LAND_STIFFEN,
            "LandRolling" => clip::LAND_ROLLING,
            "LandJump3" => clip::LAND_JUMP3,
            "SquatEnd" => clip::SQUAT_END,
            _ => return false,
        };
        self.nerve.pending_clip().is_some() || !self.anim_done(frames)
    }
    /// Advance the clips one frame, before the state runs, as the game does
    /// (`PlayerAnimator::updateAnimFrame` inside its movement).
    fn advance_clips(&mut self) {
        self.clip_wrapped = false;
        self.counter_wrapped = false;
        if self.bound_to.is_some() && (self.model_switch_row || self.switch_sync_row) {
            return;
        }
        if self.is_dead()
            && self.death.kind == death::Kind::Press
            && (self.frame > 0 || self.mode == Move::Dead)
        {
            self.anim.frame = self.press.frozen_frame;
            return;
        }
        if matches!(self.anim.name, "Move" | "MoveMoon") && !self.model_2d() {
            // The dive's landing clip covers the legs for a while; the run's
            // pose does not move under it.
            if self.sub_anim.is_some_and(|s| s.name == "RollingStandUp") {
                self.cycle.begin_frame_covered();
            } else {
                self.cycle.begin_frame();
            }
            self.step_phase = self.cycle.phase;
            self.anim.frame = self.cycle.shown % 60.;
        } else {
            // A clip started under a sub animation holds its first frame for
            // one extra update (`PlayerAnimFrameCtrl::update` skips once after
            // `startAction`); without a sub animation the frame follows the
            // model instead (`updateSync`) and nothing is skipped.
            let skipped = std::mem::take(&mut self.anim_skip) && self.sub.active;
            // The unrolling clip holds its last frame; the counter goes on
            // from there with the rate.
            if self.anim.name == "RollingEnd" {
                self.anim.frame = self.anim.frame.min(clip::ROLLING_END as f32);
            }
            if !skipped {
                self.anim.frame += self.anim.rate + std::mem::take(&mut self.anim_rate_change);
            }
            // A jump clip that has played out holds a frame past its end.
            if let Some(length) = self.held_length_of(self.anim.name) {
                self.anim.frame = self.anim.frame.min(length as f32 + 1.);
            }
            if let Some(length) = self.loop_len(self.anim.name) {
                let slack = self.loop_slack_of(self.anim.name);
                // (A clip whose counter shows its length for a row starts its loop for the
                // triggers when its pose does: `update_loop_start`.)
                if slack == 0. {
                    self.clip_wrapped = self.anim.frame >= length;
                }
                if self.anim.frame >= length + slack {
                    self.anim.frame -= length;
                    self.counter_wrapped = true;
                }
            }
        }
        // The upper-body catch animation runs the length of its clip.
        if let Some(frame) = &mut self.upper_catch {
            *frame += 1;
            if *frame > upper_catch_last(self.catch_clip) {
                self.upper_catch = None;
            }
        }
        // The model's sub animation advances one frame; once it has reached
        // its last frame the actor's control ends it (`PlayerActorHakoniwa::control`
        // checks `isSubAnimEnd` before the model advances).
        if self.sub.active {
            if self.sub.frame >= self.sub.length {
                self.sub.active = false;
                if self.sub.name == "CarryFrontThrow" {
                    // PlayerAnimator::endSubAnim (0x42a87c) starts the main action again;
                    // startAction selects that action's exported interpolation settings.
                    self.ended_sub = Some((self.sub.name, self.sub.frame));
                }
            } else {
                self.sub.frame += 1.;
            }
        }
        if let Some(sub) = &mut self.sub_anim {
            sub.frame += sub.rate;
            // The cap-throw overlay lasts 16 frames, the roll's stand-up 8.
            let length = if sub.name == "RollingStandUp" {
                8.
            } else {
                16.
            };
            if sub.held() >= length {
                let whole_body = sub.name == "RollingStandUp";
                self.sub_anim = None;
                if whole_body && matches!(self.anim.name, "Move" | "MoveMoon") {
                    // The landing clip covered the legs; their cycle starts
                    // over underneath it, the counter running on.
                    self.cycle.restart_pose();
                }
            }
        }
    }
    /// First step of a state chosen last step: start its clip, then do the
    /// work only its first step does.
    fn begin_step(&mut self, acting: ActingNerve) {
        self.counter_restarted = false;
        self.run_start_effect = None;
        self.roll_forget_effect_call();
        self.advance_clips();
        let (clip, work) = self.nerve.start();
        self.puppet_first_row = std::mem::take(&mut self.puppet_clip_pending) && clip.is_some();
        if let Some(clip) = clip {
            self.start_clip(clip);
        }
        if self.cycle_restart_in > 0 {
            self.cycle_restart_in -= 1;
            if self.cycle_restart_in == 0 && matches!(self.anim.name, "Move" | "MoveMoon") {
                self.cycle.start();
                self.anim.frame = 0.;
                self.step_phase = 0.;
            }
        }
        if let Some(work) = work {
            work(self);
        }
        self.refresh_cap_jump(acting);
    }
    /// `PlayerCapActionHistory::clearLandLimit`: the cap jump and the air catch's pop are both
    /// possible again.
    fn restore_cap_actions(&mut self) {
        self.cap_jump_possible = true;
        self.cap_pop_possible = true;
    }
    /// The cap jump and the air catch's pop are possible again, by the three history calls the
    /// states make (all 19 direct call sites of 0x42ddb0, 0x42ddd8 and 0x42de30), each with its
    /// own caller's guard:
    ///
    /// - `clearLandLimitStandAngle` (0x42ddd8) always sets the pop's byte (+0x39) and sets the
    ///   cap jump's (+0x38) only on `rs::isOnGroundLessAngle(StandAngleMin)`. Callers: the first
    ///   step of `exeWait` (0x422480), `exeSquat` (0x422aac), `exeRun` (0x422ff0),
    ///   `exeSandSink` (0x4256f4) and `exeDamage` (0x425e90); every step of `exeRolling`
    ///   (0x423754), `exeHipDrop` (0x424bc4), `exeHeadSliding` (0x424f30) and `exeDamageFire`
    ///   (0x426340) on which `rs::isCollidedGround` holds, all of them before their state's
    ///   `updateNerveState` (`refresh_cap_jump`, keyed on the nerve running at the start of the
    ///   frame); every step of `exeSpinCap` while its state byte +0x78 is clear (0x4239cc..0x4239d8:
    ///   ground start, or the ground touched since), but after `updateNerveState` and not when
    ///   that ended the state (0x4239c4; `restore_after_spin_cap`).
    /// - `clearLandLimit` (0x42ddb0) sets both bytes: every step of `exeSwim` (0x425b38; only
    ///   its wall-history reset is first-step gated) and the first step of `exeWallCatch`
    ///   (0x4247e4), `exeGrabCeil` (0x424ab4), `exePoleClimb` (0x424b28), `exeBind` (0x426ca4)
    ///   and `exeDamageSwim` (0x42619c).
    /// - `clearWallAirLimit` (0x42de30) sets the pop's byte only: the first step of `exeWallAir`
    ///   (0x424590).
    ///
    /// Not ported (no such state here): `exeRise` (0x4259ac, `clearLandLimit` on its first
    /// step; the water-exit state does not exist) and `endPlayerPuppet` (0x421f78,
    /// `clearLandLimit`, unguarded; reached from `endDemoPuppetable` 0x421e44 and
    /// `endDemoShineGet` 0x4221fc, the demo puppets, which the port has no state for).
    fn refresh_cap_jump(&mut self, acting: ActingNerve) {
        let ActingNerve { name, first_step } = acting;
        if name == "SpinCap" && self.is_on_ground_by(self.v) {
            self.throwing.ground_seen = true;
        }
        let stand_angle_call = match name {
            "Wait" | "Squat" | "Run" | "SandSink" | "Damage" => first_step,
            "Rolling" | "HipDrop" | "HeadSliding" | "DamageFire" => self.is_collided_ground(),
            _ => false,
        };
        if stand_angle_call {
            self.restore_stand_angle_cap_actions();
        }
        let land_limit_call = match name {
            "Swim" => true,
            "WallCatch" | "PoleClimb" | "GrabCeil" | "Bind" | "DamageSwim" => first_step,
            _ => false,
        };
        if land_limit_call {
            self.restore_cap_actions();
        }
        if name == "WallAir" && first_step {
            self.cap_pop_possible = true;
        }
    }
    /// `exeSpinCap` asks for its restore after `updateNerveState` (0x4239c0) and skips it when
    /// that answered true, the ground spin having ended (0x4239c4); the rest of the actors ask
    /// before their state's update (`refresh_cap_jump`).
    fn restore_after_spin_cap(&mut self, acting: ActingNerve) {
        let child_ended = std::mem::take(&mut self.throwing.child_ended);
        if acting.name == "SpinCap"
            && !child_ended
            && (!self.throwing.in_air || self.throwing.ground_seen)
        {
            self.restore_stand_angle_cap_actions();
        }
    }
    /// `clearLandLimitStandAngle`: the pop always, the cap jump on `isOnGroundLessAngle`.
    fn restore_stand_angle_cap_actions(&mut self) {
        self.cap_pop_possible = true;
        if self.is_on_ground_less_angle(t::STAND_ANGLE_MIN) {
            self.cap_jump_possible = true;
        }
    }
    /// The actor nerve whose `exe` runs this frame, as the history restores read it.
    fn acting_nerve(&self) -> ActingNerve {
        ActingNerve {
            name: self.actor_name(),
            first_step: self.nerve.step == 0,
        }
    }
    fn set_h(&mut self, h: Vec3) {
        self.v.x = h.x;
        self.v.z = h.z;
    }

    /// Advance one original frame.
    pub fn step(&mut self, o: &Orders, scene: &Course) {
        self.step_with_sources(o, scene, true);
    }

    /// Reconstruct Mario's chosen nerve without advancing independently owned
    /// actors or delivering their sensor messages a second time.
    pub(crate) fn repeat_step_for_lockstep(&mut self, o: &Orders, scene: &Course) {
        // PlayerDamageKeeper::update (0x43f4c4) already ran for this row.
        // Re-choice must not advance its blink again: show (0x94c2b4)
        // clears the outgoing animation blend when visibility returns.
        let damage = self.damage.clone();
        // PlayerJointControlGroundPose::update follows its IK targets once per
        // actor update (0x4541f8..0x454268). The completed row already moved
        // them, so the repeated choice must not add a second step.
        let ground_pose = self.joints.ground.clone();
        // PlayerJointParamSwim's interpolation has already run for this row
        // (0x486af8..0x486bc4), including surface/landing ease-off.
        let swim_pose = self.joints.swim;
        // The partial model animation advances once per actor frame as well.
        let upper_catch = (self.upper_catch, self.catch_clip);
        // PlayerSeCtrl::update (0x463064) counts its frames moved once per update.
        let sounds = self.sounds.clone();
        // PlayerStainControl::update runs once per update as well (smoke starts by its timer).
        let soot = self.soot;
        self.step_with_sources(o, scene, false);
        self.damage = damage;
        self.joints.ground = ground_pose;
        self.joints.swim = swim_pose;
        (self.upper_catch, self.catch_clip) = upper_catch;
        self.sounds = sounds;
        self.soot = soot;
    }

    /// Restore the independently completed cap after reconstructing only Mario's nerve choice.
    pub(crate) fn restore_completed_cap(&mut self, cap: Option<Cap>) {
        if let Some(cap) = &cap {
            self.cap_serial = cap.clip_serial();
        }
        self.cap = cap;
    }

    fn step_with_sources(&mut self, o: &Orders, scene: &Course, advance_sources: bool) {
        if self.death.restart_pending {
            self.restart_after_death(scene);
        }
        if advance_sources {
            // The pipe actor updates after the player within a frame.
            self.dokan_update(scene, o);
            self.key_move_sources = scene
                .mesh
                .as_ref()
                .map(|m| {
                    m.key_move_parts
                        .iter()
                        .map(|p| p.diagnostic())
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .unwrap_or_default();
            self.rotate_sources = scene
                .mesh
                .as_ref()
                .map(|m| {
                    m.rotate_parts
                        .iter()
                        .map(|p| p.diagnostic())
                        .chain(m.conveyors.iter().map(|c| c.diagnostic()))
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .unwrap_or_default();
        }
        self.rotating_ground = false;
        if let Some(mesh) = &scene.mesh
            && let Some(source) = mesh
                .rotate_parts
                .iter()
                .find(|s| Some(s.part) == self.ground_part)
        {
            self.rotating_ground = true;
            self.ground_frame = Some((mesh.parts[source.part].base, source.previous.inverse));
        }
        self.update_ground_inertia(scene);
        if advance_sources {
            self.update_external_velocity(scene);
            self.update_area_velocity(scene);
        }
        self.death.update_judge(self.is_dead());
        self.prev = self.pos;
        self.prev_cap = self.cap.as_ref().map(|c| c.pos);
        if self.hitstop > 0. {
            self.hitstop -= 1. / FPS;
            return;
        }
        if o.reset {
            self.reset();
            return;
        }
        if let Some(mesh) = &scene.mesh {
            self.dimension
                .update(&mesh.dimension, mesh.to_game_vec(self.position_cm()));
            self.dimension_diagnostic = self.dimension.diagnostic(&mesh.dimension);
            // copyAnimLocal restarts the new model's animator (PlayerAnimFrameCtrl::
            // startAction: counter 0, first update skipped); while a puppet owns the
            // clip, its updateSync (counter = action frame + rate) follows one row
            // later, and the pose is not advanced in either row.
            self.switch_sync_row = self.model_switch_row && self.bound_to.is_some();
            self.model_switch_row = self.dimension.change_2d || self.dimension.change_3d;
            if self.dimension.change_2d {
                self.events.push(Fx::Change2D);
            } else if self.dimension.change_3d {
                self.events.push(Fx::Change3D {
                    bound: self.bound_to.is_some(),
                });
                // Dimension helper's change-out path, 0x420cd4..0x420d14.
                self.dimension.invalidate();
                self.dimension_diagnostic = self.dimension.diagnostic(&mesh.dimension);
            }
        }
        self.note_frame_start_for_joints();
        self.own_cap_trample = false;
        // `PlayerColliderHakoniwa::updateCeilingCheck` runs from the pose the last step left.
        self.ceiling = scene.catch_mesh().map_or(CeilingCheck::OPEN, |mesh| {
            ceiling_check::update(
                &mesh,
                self.position_cm(),
                Vec3::Y,
                ceiling_check::TALL,
                ceiling_check::COLLISION_RADIUS_SQUAT,
                0.,
                0.,
            )
        });
        let mut o = o.clone();
        self.snap_move_axis = scene.mesh.as_ref().and_then(|mesh| {
            mesh.snap_move
                .iter()
                .rev()
                .find(|area| {
                    !self.grounded
                        && area
                            .volume_at(mesh.to_game_vec(self.position_cm()))
                            .is_some()
                })
                .map(|_| Vec3::X)
        });
        if let Some(axis) = self.snap_move_axis {
            let input = Vec3::new(o.direction.x, 0., o.direction.y);
            let input = vector_lib::verticalize_vec(axis, input);
            o.direction = Vec2::new(input.x, input.z);
        }
        self.read_input(&o);
        self.update_input_2d(scene);
        self.landing_speed = 0.;
        // The nerve whose `exe` the binary runs in this frame: the sensor phases below may choose
        // another one, which starts next frame.
        let acting = self.acting_nerve();
        self.nerve.deferred = false;
        self.nerve.renewed = false;
        self.sensor_restart = false;
        self.sub_restarted = false;
        self.ended_sub = None;
        self.interpolation_cleared = false;
        self.upper_cleared = false;
        self.age = self.age.saturating_add(1);
        let was_grounded = self.grounded;
        // The game animates first, then the state runs; a state chosen last
        // frame starts now.
        // The cap updates before the player does, within the same frame.
        if advance_sources {
            self.init_killers(scene);
            self.tick_rocket_flowers(scene);
        }
        self.update_cap(&o, scene);
        if advance_sources {
            self.rocket_sensor_phase(scene);
        }
        self.query_equipment_force_run();
        if advance_sources {
            self.trampoline_sensor_phase(scene);
            for sign in &mut self.signboards {
                sign.tick();
            }
            self.tick_lock_on_sources(o.throw_held);
        }
        self.cap_sensor_phase(&o);
        self.puku_sensor_phase(scene);
        self.start_pending_abyss();
        self.check_recovery_area_exit(scene);
        self.dispatch_sand_death();
        self.receive_candle_fire(scene);
        self.receive_sand_geyser();
        self.dispatch_damage(scene);
        self.receive_grab_ceil_force(scene);
        if advance_sources {
            self.update_flower_seed(scene);
            self.update_kick_stone(scene);
        }
        self.carry_sensor_phase(&o, scene);
        if self.is_carry() {
            // Carry input belongs to the keeper, not the actor's buffered cap throw.
            self.pre_throw = 0;
        }
        self.flower_sensor_phase(scene);
        if advance_sources {
            self.dokan_sensor_phase(scene, &o);
        }
        self.begin_step(acting);
        if !matches!(self.mode, Move::Walk | Move::Run | Move::Skid | Move::Turn)
            && !(self.mode == Move::Throw && !self.throwing.in_air)
        {
            self.pull = t::GRAVITY;
        }
        self.change_from_surroundings(scene, &o);
        // A state chosen above leaves its own first step to the next frame.
        if !self.nerve.deferred {
            self.run_state(&o, scene);
            self.judge_sand_entry();
            self.judge_wall_push(scene);
            self.judge_fall();
            self.judge_grab_ceil(scene);
            self.judge_pole_climb(&o, scene);
            self.judge_wall_catch(scene);
            self.judge_wall_keep(scene);
            self.judge_in_water(scene);
            self.judge_rolling_wall_hit_down();
        }
        self.restore_after_spin_cap(acting);
        self.update_sand_affect(scene);
        // LiveActor::movement runs the actor nerve before control's updateCarry (0x420920).
        // A state-selected Release must replace the old keeper execution (0x42e664..0x42e684).
        self.update_carry(&o, advance_sources);
        self.finish_carry_release(scene);
        let moved = self.v;
        if self.mode != Move::Bind || self.puppet_collision {
            self.collide(&o, scene);
            if matches!(self.mode, Move::Bind | Move::BindFall) {
                // The puppet fall keeps the velocity the air control gave it; the
                // collision response does not clear it.
                self.v = moved;
            }
        }
        self.update_stone_collide_lock_up(scene);
        self.update_water_finder(scene);
        self.fall.update(
            self.pos.y / UNIT,
            moved.y,
            self.grounded,
            self.grounded && !was_grounded,
        );
        if self.grounded
            && !was_grounded
            // exeWait keeps its state when its own short airborne pull reaches ground.
            // Migrate this source composition without changing older snapshot contracts.
            && (self.mode != Move::Idle || self.carry.stone.is_none())
            && !self.burning()
            && !self.is_dead()
            && !self.recovering()
            && self.mode != Move::GroundSpin
            && self.mode != Move::SlopeSlide
            && self.mode != Move::Swim
            && !matches!(self.mode, Move::Bind | Move::BindFall)
            && !self.in_damage_state()
            && !self.in_sand_state()
            && !self.stands_on_ledge()
            && !(self.mode == Move::Idle && self.standing_clip_playing())
            && !matches!(self.mode, Move::Walk | Move::Run | Move::Skid | Move::Turn)
        {
            self.nerve.touchdown = true;
            self.touchdown_speed = self.landing_speed;
        }
        // Fall can keep touching a steep face for several frames. Its landing
        // judge accepts a usable floor even without a new collision edge.
        if self.mode == Move::Fall && was_grounded && self.fire_run_angle_contact() {
            self.nerve.touchdown = true;
            self.touchdown_speed = self.v.y;
        }
        self.queue_sand_geyser(scene);
        self.sample_grab_ceil_sources(scene);
        self.check_death_area(scene);
        self.update_recovery_point(scene);
        self.update_force_run_counter();
        self.finish_frame(scene);
        self.carry.push.clear();
        self.snapshot_injected = false;
        if advance_sources {
            self.dokan_sensor_trail(scene);
        }
        // The camera updates after the actors of the frame.
        self.entrance_camera.update(self.position_cm(), o.recenter);
        if advance_sources {
            // The enemies, the stage-switch watchers and the switch areas follow the player
            // (`al::ExecuteDirector` entries 37 and 65, then `SwitchAreaDirector::update`).
            self.tick_killers(scene);
        }
    }
    /// Changes of state the surroundings force, decided before the state runs: the ground
    /// reached last step. (Water is asked after the state's own step, see `judge_in_water`.)
    fn change_from_surroundings(&mut self, _scene: &Course, o: &Orders) {
        if std::mem::take(&mut self.nerve.touchdown) {
            if self.judge_sand_sink() {
                if self.mode.uses_air_control() && self.air_for == Some(self.mode) {
                    let push = self.push();
                    self.air.update(
                        &mut self.v,
                        push,
                        o.jump_held,
                        &air_move::Steering {
                            front: t::JUMP_ACCEL_FRONT,
                            back: t::JUMP_ACCEL_BACK,
                            turn: t::JUMP_ACCEL_TURN,
                        },
                    );
                    self.turn_in_air(push);
                }
                self.appear_sand_sink();
            } else {
                if self.mode == Move::GrabCeil {
                    self.grab_ceil_step(o, _scene);
                } else {
                    self.land(o, _scene);
                }
            }
        }
    }
    /// `PlayerJudgeSpeedCheckFall::update`, once a step before the nerve runs
    /// (it reads the contact the last step ended with): the steps without
    /// usable ground under the player count up to five. A contact must also
    /// satisfy the original speed-dependent standing angle (0x45b2cc).
    /// A successful water-surface start judge vetoes the lost-ground counter.
    fn update_ground_lost(&mut self) {
        let contact = if self.start_surface_run() {
            true
        } else {
            self.fire_run_angle_contact()
        };
        self.ground_lost = if contact {
            0
        } else {
            (self.ground_lost + 1).min(GROUND_LOST_FRAMES + 1)
        };
    }
    /// The actor's fall judge (`PlayerJudgeSpeedCheckFall::judge`), asked by
    /// the standing, running and squatting nerves after their own step and
    /// after the jump they allow: a ground nerve that has run on without
    /// ground for more than four steps hands over to the fall. Actor +0x458
    /// is consumed by Wait 0x422560, Squat 0x422af0 and Run 0x423384;
    /// each consumer keeps its own position after its child update.
    fn judge_fall(&mut self) {
        if !(self.start_surface_run()
            || self.surface_run.result && matches!(self.mode, Move::Walk | Move::Run))
            && self.ground_lost > GROUND_LOST_FRAMES
            && !self.nerve.deferred
            && (matches!(
                self.mode,
                Move::Idle
                    | Move::Walk
                    | Move::Run
                    | Move::Skid
                    | Move::Turn
                    | Move::SquatBrake
                    | Move::Crouch
                    | Move::Crawl
                    | Move::GroundSpin
            ) || self.mode == Move::Throw && !self.throwing.in_air)
        {
            if self.grounded && self.is_collided_ground() {
                // waitGround has already applied this step's ground normal.
                // units_per_frame may still hold the previous collision's
                // reporting cache, which is stale on a rotating support.
                self.v = if self.rotating_ground {
                    self.ground_following()
                } else {
                    self.units_per_frame()
                };
            }
            if matches!(self.actor_name(), "Wait" | "Run") {
                self.feedback_ground_inertia(false);
            }
            self.go(Move::Fall, "Fall");
        }
    }
    /// Run the nerve of the current state for this frame.
    fn run_state(&mut self, o: &Orders, scene: &Course) {
        if matches!(self.actor_name(), "Wait" | "Run") {
            self.ground_inertia.request = 1;
        }
        let geyser_gave_wait_power = self.follow_geyser_ground(scene);
        if self.mode == Move::Idle && !geyser_gave_wait_power {
            self.wait_state_platform_control();
        }
        self.update_ground_height(scene);
        if self.mode == Move::Idle && (self.nerve.step == 0 || self.wait.area_clip.is_none()) {
            let temperature = scene.mesh.as_ref().and_then(|m| {
                m.temperature
                    .iter()
                    .rev()
                    .find(|(a, _)| a.volume_at(m.to_game_vec(self.position_cm())).is_some())
                    .map(|(_, t)| *t)
            });
            self.wait.shade_query_unavailable = temperature == Some(-4);
            self.wait.area_clip = match temperature {
                // Original initAreaAnim (0x48ccdc): -4 first queries the CPU
                // shadow texture, then uses -2 in shade or 0 outside shade.
                // That renderer query is UNPORTED; neither result is asserted.
                Some(-4) => None,
                Some(2..) => Some("WaitHot"),
                Some(-2) => Some("WaitCold"),
                Some(i32::MIN..=-3) => Some("WaitVeryCold"),
                _ => None,
            };
        }
        match self.mode {
            Move::Idle | Move::Walk | Move::Run => {
                let running = self.mode != Move::Idle;
                self.run(o, scene);
                if running {
                    self.respond_surface_run(scene);
                }
            }
            Move::Skid | Move::Turn => self.brake(o, scene),
            Move::SquatBrake | Move::Crouch | Move::Crawl => self.squat(o, scene),
            Move::GroundSpin => self.ground_spin(o),
            Move::Throw => self.throw_state(o, scene),
            Move::GroundPound => self.hip_drop(o),
            Move::PoundLand => self.hip_drop_land(o, scene),
            Move::Roll => self.roll(o),
            Move::RollEnd => self.roll_stand_up_step(o, scene),
            Move::RollJump => self.roll_jump_step(o),
            Move::Dive => self.head_sliding(o, scene),
            Move::SlopeSlide => self.slope_slide(o, scene),
            Move::BurnJump | Move::BurnBounce | Move::FireRun | Move::FireFall | Move::FireDead => {
                self.damage_fire_step(o, scene)
            }
            Move::DamageAir | Move::DamageLand | Move::DamageDead => {
                self.damage_life_step(o, scene)
            }
            Move::DamageSwim
            | Move::DamageSurface
            | Move::DamageLandWater
            | Move::DamageSwimDead => self.damage_swim_step(o, scene),
            Move::SandWait | Move::SandRun | Move::SandPivot | Move::SandJump | Move::SandDead => {
                self.sand_step(o, scene)
            }
            Move::SandGeyser => self.sand_geyser_step(o, scene),
            Move::Press => self.press_step(),
            Move::Bind => self.bind_state(),
            Move::BindFall => self.bind_fall_state(o, scene),
            Move::Dead => {}
            Move::AbyssFall => self.abyss_fall(),
            Move::RecoveryStart | Move::RecoveryTravel | Move::RecoveryFall => {
                self.recovery_step(o, scene)
            }
            Move::LongJump => self.long_jump_air(o),
            Move::Backflip => self.back_jump_air(o),
            Move::Sideflip => self.turn_jump_air(o),
            Move::WallJump => self.wall_jump(o),
            // Every state the air control flies (`WallJump` has its own port above).
            Move::Fall
            | Move::Jump
            | Move::DoubleJump
            | Move::TripleJump
            | Move::PoundJump
            | Move::SpinJump
            | Move::CapBounce
            | Move::CatchJump => self.jump_air(o),
            Move::WallSlide => self.wall_slide(o, scene),
            Move::Ledge => self.wall_catch(o, scene),
            Move::Pole => self.pole_climb(o, scene),
            Move::GrabCeil => self.grab_ceil_step(o, scene),
            Move::Swim => self.swim(o, scene),
            Move::SpinDrop => self.spin_drop(o),
            Move::SpinFlower => self.spin_flower_step(),
            Move::CapCatchPop => self.cap_catch_pop_step(o, scene),
        }
        if std::mem::take(&mut self.ground_state_ended) {
            self.nerve.deferred = false;
            self.set_nerve_on_ground(o, scene);
        }
        // The jump states ask for the `DiveInWater` clip at the end of their steps
        // (`tryChangeDiveInWaterAnim` in `exeJump`, `exeJumpTurn`).
        if matches!(
            self.mode,
            Move::Jump
                | Move::DoubleJump
                | Move::TripleJump
                | Move::LongJump
                | Move::Sideflip
                | Move::WallJump
        ) {
            self.try_change_dive_in_water_anim(scene);
        }
    }
    fn read_input(&mut self, o: &Orders) {
        self.camera_yaw = o.camera_yaw;
        let stick = Vec3::new(o.direction.x, 0., o.direction.y).clamp_length_max(1.);
        self.forward = Vec3::new(o.forward.x, 0., o.forward.y).normalize_or_zero();
        self.tilt = stick.length();
        self.stick = stick.normalize_or_zero();
        self.raw_stick = Vec2::new(o.stick.x, -o.stick.y);
        self.view = o.view;
        self.camera_view = o.camera_view;
        self.wall_along.update(
            self.touched
                .wall_contact
                .as_ref()
                .filter(|wall| wall.at_face)
                .map(|wall| wall.normal),
            self.raw_stick,
        );
        self.pre_jump = if o.jump {
            t::PRE_INPUT_JUMP
        } else {
            self.pre_jump.saturating_sub(1)
        };
        self.pre_throw = if o.throw_pressed {
            t::PRE_INPUT_THROW
        } else {
            self.pre_throw.saturating_sub(1)
        };
        self.jump_held_frames = if o.jump_held {
            self.jump_held_frames + 1
        } else {
            0
        };
        self.read_spin_input(o);
        self.catch_window = self.catch_window.saturating_sub(1);
        self.after_turn = self.after_turn.saturating_sub(1);
        self.quick_turn = self.quick_turn.saturating_sub(1);
        self.throwing.run = self.throwing.run.saturating_sub(1);
        self.update_ground_lost();
        if self.grounded {
            self.land_frames = self.land_frames.saturating_add(1);
        }
    }
    fn update_input_2d(&mut self, scene: &Course) {
        let active = self.dimension.is_2d && self.dimension.in_area;
        let lock = scene.mesh.as_ref().map_or(Vec3::ZERO, |mesh| {
            self.dimension
                .lock(&mesh.dimension, mesh.to_game_vec(self.position_cm()))
                .0
        });
        let pose = self.actor_pose();
        let frame = input_2d::Frame {
            active,
            stick: self.raw_stick,
            view: self
                .view
                .unwrap_or_else(|| move_input::locked_view(self.camera_yaw.to_degrees())),
            position: self.position_cm(),
            velocity: self.v,
            actor_side: pose.side(),
            actor_up: pose.up(),
            actor_front: pose.front(),
            lock,
            up: self.mesh_ground.unwrap_or(pose_math::NEGATED_GRAVITY),
            ground: self.grounded.then(|| {
                (
                    self.collider.ground_position(),
                    self.mesh_ground.unwrap_or(Vec3::Y),
                )
            }),
            just_landed: self.is_just_land(),
        };
        self.input_2d.update(&frame);
        self.input_2d_frame = active.then_some(frame);
        if let Some(frame) = &self.input_2d_frame {
            let push = self.input_2d.move_input(frame, Vec3::Y);
            self.tilt = pose_math::squared_length(push).sqrt();
            self.stick = pose_math::try_normalize_or_zero(push).0;
        }
    }
    fn finish_frame(&mut self, scene: &Course) {
        self.update_combo_counters();
        // Original movement updates wetness before it records wet stain suppression.
        self.wet.update();
        self.update_soot(scene);
        let dead = self.is_dead();
        // ModelKeeper::show resets AnimPlayerSkl interpolation after a blink.
        let was_hidden = self.damage.hidden;
        self.damage.update(dead);
        if was_hidden && !self.damage.hidden {
            self.interpolation_cleared = true;
        }
        self.recovery.push_invalid = self.recovery.push_invalid.saturating_sub(1);
        self.death.update_scene(dead);
        // A state chosen this step has run no step yet: its first one is next.
        // The nerve's own steps restart with a new actor nerve, chosen or
        // reached by a sub-state that belongs to another one.
        if self.mode != Move::Fall {
            self.spin_fall = false;
            self.fall_after_ledge = None;
        }
        let actor = self.actor_name();
        if self.nerve.renewed || actor != self.nerve.actor {
            self.nerve.actor = actor;
            self.nerve.step = 0;
        } else {
            self.nerve.step += 1;
        }
        if !self.nerve.deferred {
            self.frame += 1;
        }
        self.vel = self.v * UNIT * FPS;
        self.move_speed = self.speed_h();
        self.update_loop_start();

        self.update_material_code(scene);
        self.update_sounds(scene);
        self.timer = self.frame as f32 / FPS;
        self.since_land = self.land_frames as f32 / FPS;
        if !self.pos.is_finite() || !self.v.is_finite() {
            self.reset();
        }
        // The head's look-at runs after the state (`PlayerActorHakoniwa::movement`).
        // (It is off while the animator plays a sub animation: `movement` asks the
        // animator's sub-animation flag before the state's `isEnableLookAt`.)
        // (The state's `isEnableLookAt` asks the nerve it has chosen, shown or not.)
        let look_label = match self.mode {
            Move::Swim => self.swim.nerve.name(),
            _ => self.nerve_name().name,
        };
        // (And while the scene's entrance camera plays: `entrance_camera.rs`.)
        let enabled = look_at::look_enabled(look_label)
            && !self.run_nerve_blocks_look()
            && !self.sub.active
            && !self.entrance_camera.playing()
            // The keeper is idle in the player's very first movement: every recorded run's
            // first preroll row reads mode=0 (inferred from the rows, the flag is unresolved).
            && self.age > 1;
        let actor = self.pose_matrix();
        let sensors = self.rocket_look_sensors(scene.look_sensors(), scene.mesh.as_deref());
        let killer_sensors = self.killer_sensor_count(scene.mesh.as_deref());
        if scene
            .mesh
            .as_ref()
            .and_then(|m| m.pukupuku.as_ref())
            .is_some_and(|s| s.phase != 0)
        {
            self.look.disregard_candidate(scene.look_sensors().len());
        }
        for switch in &self.cap_switches {
            if let Some(index) = switch.look_sensor
                && !switch.sensors_valid()
            {
                self.look.invalidate_sensor(index);
            }
        }
        for (i, f) in self.rocket.flowers.iter().enumerate() {
            if f.clipped {
                self.look
                    .invalidate_source_sensor(scene.look_sensors().len() + i * 2);
            }
            if !f.valid || f.clipped {
                self.look
                    .invalidate_source_sensor(scene.look_sensors().len() + i * 2 + 1);
            }
        }
        if self.carry.seed.as_ref().is_some_and(|seed| {
            self.is_carry() || self.carry.held_during_sensor_phase || !seed.hit_sensors_valid()
        }) || (self.carry.stone.is_some()
            && (self.is_carry() || self.carry.held_during_sensor_phase))
        {
            self.look
                .invalidate_source_sensor(sensors.len() - 1 - killer_sensors);
        }
        for (i, killer) in self.killers.sensors().iter().enumerate() {
            if !killer.alive {
                self.look
                    .invalidate_sensor(sensors.len() - killer_sensors + i);
            }
        }
        self.look.step(&actor, enabled, &sensors);
        self.run_state_frame();
        self.update_joint_controls();
    }

    // ---- Ground movement ------------------------------------------------
    /// Shared ground transitions. Returns true when a new state took over.
    fn ground_actions(&mut self, o: &Orders) -> bool {
        let speed = self.speed_h();
        if self.pre_input_jump_judges() {
            self.ground_jump(o);
            return true;
        }
        if !self.is_carry() && o.crouch && self.slope_slide_judge() {
            self.start_slope_slide();
            return true;
        }
        if !self.is_carry()
            && !self.force_run.is_active()
            && o.crouch
            && !matches!(self.mode, Move::SquatBrake | Move::Crouch | Move::Crawl)
        {
            self.start_squat();
            return true;
        }
        if !self.model_2d()
            && (self.pre_throw > 0 && !o.crouch || o.up_throw || o.down_throw || o.spin_throw)
        {
            let kind = self.throw_kind(o);
            // A throw press with the cap still out is refused and stays buffered
            // (`PlayerJudgePreInputCapThrow`: only `recordJudgeAndReset` after a throw empties it), so
            // the throw starts on the frame the cap arrives (`HackCap::isEnableSpinAttack`).
            if self.cap.is_none() {
                self.pre_throw = 0;
                if matches!(self.mode, Move::Run | Move::Walk)
                    && kind == Throw::Forward
                    && speed > t::NORMAL_MIN
                {
                    // Throwing on the run keeps the legs running; continuous
                    // throws push the run up to dash speed.
                    self.throwing.run = t::RUN_TIME_CONTINUOUS_THROW;
                    self.sub_anim = Some(Anim {
                        name: "ThrowCapParts",
                        serial: self.anim.serial.wrapping_add(7),
                        frame: 0.,
                        rate: 1.,
                    });
                    self.launch_cap(kind, false);
                } else {
                    self.start_throw(kind);
                }
                return true;
            }
        }
        false
    }
    /// The stick as the ground move control reads it: the world direction
    /// scaled by the tilt, nothing inside the deadzone.
    fn ground_stick(&self) -> Vec3 {
        if self.tilt >= STICK_DEADZONE {
            self.stick * self.tilt.min(1.)
        } else {
            Vec3::ZERO
        }
    }
    /// The standing or running state (the original's wait and run nerves).
    fn run(&mut self, o: &Orders, scene: &Course) {
        if self.mode == Move::Idle && self.clip() == "SquatEnd" {
            // exeStandUp runs waitGround before testing clearance, StartRun
            // and clip completion; its actor dispatches a killed child.
            self.wait_pose();
            if !self.judge_enable_stand_up() {
                self.stand_up_blocked();
            } else if self.ground_start_run() || self.anim_done(clip::SQUAT_END) {
                if self.grounded {
                    // StandUp stopped horizontal travel on entry. waitGround
                    // rebounds its normal pull before adding gravity again
                    // (0x5656ec..0x56577c); keep only the port's separate pull,
                    // rather than reusing the snapshot's normal XZ as travel.
                    self.set_h(Vec3::ZERO);
                    self.v.y = -t::GRAVITY;
                } else {
                    self.v.y = (self.v.y - t::GRAVITY).max(-t::FALL_MAX);
                }
                self.set_nerve_on_ground(o, scene); // exeSquat 0x422ae4.
            } else {
                self.stand(o, scene);
            }
        } else if self.mode == Move::Idle {
            // Every nerve of the standing state settles the pose first; the
            // judges that leave it (the run, the jump) come after.
            if self.land_nerve() {
                self.land_ground(scene);
            } else {
                self.wait_pose();
                self.land_snap = self.flat_land_snap(scene);
            }
            self.stand(o, scene);
        } else if self.model_2d() {
            self.run_2d_step(o);
        } else if self.run_nerve == RunNerve::WallPush {
            self.wall_push_step(o);
        } else {
            self.run_step(o, scene);
        }
    }
    /// `PlayerStateWait::isEnableCancelAction` in the `LandStiffen` nerve: the hard landing
    /// cannot be cancelled until its clip ends, and the one after a spin drop (cancel frame
    /// `HipDropLandCancelFrame`, read at the nerve's first step) from its 24th step on.
    fn land_stiffen_holds(&self) -> bool {
        match self.clip() {
            "LandStiffen" => self.standing_clip_playing(),
            "LandDownFall" => self.nerve.step + 1 < t::HIP_DROP_LAND_CANCEL,
            _ => false,
        }
    }
    /// Standing: the stick starts the run (the start-run judge), which is a
    /// new nerve, chosen now and moving from its first step.
    fn stand(&mut self, o: &Orders, scene: &Course) {
        if self.model_2d() && self.grounded {
            // exeWait -> waitGround 0x5656ec..0x565700 rebounds with three
            // zero coefficients. A grounded 2D Wait has no Land child to stop it.
            self.set_h(Vec3::ZERO);
        }
        if self.rotating_ground && self.wait.steps == 0 && self.grounded && self.clip() == "Wait" {
            // exeWait calls waitGround before its actor judges (0x48e0e8).
            // On entry from Land its remaining normal pull is raw velocity,
            // which reboundVelocityFromCollision cancels; it is not run speed.
            self.set_h(Vec3::ZERO);
        }
        // (The forced slope slide comes before the state's own start-run judge.)
        if self.try_wait_run_forced_action(scene) {
            return;
        }
        if self.force_run.is_active() || self.tilt >= STICK_DEADZONE || self.auto_run_skate() {
            if self.force_run.is_active() && self.sub.active {
                self.ended_sub = Some((self.sub.name, self.sub.frame));
                self.sub.active = false;
            }
            // The state's own start-run judge comes before the actor's (jump,
            // crouch, throw): a jump pressed with the stick is taken by the
            // run state's first step (`pole-climb` rows 10-11).
            // The run state's appearance already pulls the player to the floor.
            let raw_ground_velocity = self.grounded
                && !self.snapshot_injected
                && self.ground_move.is_none()
                && self.v.y != 0.
                && self.v.y != -self.pull;
            self.pull = GRAVITY_MOVE;
            if raw_ground_velocity {
                // GroundMoveControl::appear sets its pull immediately, even
                // when Wait/Land left full actor velocity in the controller.
                self.run_appear(false);
                self.v.y = 0.;
            }
            self.go(Move::Walk, "Move");
            return;
        }
        if !self.land_stiffen_holds() && self.ground_actions(o) {
            return;
        }
        self.run_frames = 0;
        let step = self.count_wait_step();
        if self.land_nerve() {
            self.wait_nerve(step);
            return;
        }
        if !self.grounded && self.standing_clip_playing() {
            self.wait_nerve(step);
            self.v.y = (self.v.y - t::GRAVITY).max(-t::FALL_MAX);
            return;
        }
        let s = (self.speed_h() - t::NORMAL_MAX / t::BRAKE_FRAMES).max(0.);
        self.set_h(self.facing * s);
        if s <= 0. {
            // Standing again; the landing and stand-up clips play out first,
            // then the idle sequence runs (`PlayerStateWait`).
            self.set(Move::Idle);
            self.wait_nerve(step);
        }
        // `rs::waitGround` adds the pull every step; the floor takes it back at once, but in
        // the air it builds up to the fall speed limit (a player placed in the air falls
        // 3, 6, 9... per step until the fall state takes over).
        let snap = std::mem::take(&mut self.land_snap);
        self.v.y = if self.grounded {
            -(t::GRAVITY + snap)
        } else {
            (self.v.y - t::GRAVITY).max(-t::FALL_MAX)
        };
    }
    /// The velocity of the run in the plane of the floor with the normal `up`
    /// (`updateNormalAndSnap`): the velocity the last step left in the plane of
    /// the floor it steered on, turned onto this one.
    fn velocity_in_plane(&self, up: Vec3) -> Vec3 {
        match self.run_up {
            Some((h, from, velocity))
                if h == Vec2::new(self.v.x, self.v.z) && self.pull == GRAVITY_MOVE =>
            {
                along_vector_normal_h(velocity, from, up)
            }
            Some((h, from, _)) if h == Vec2::new(self.v.x, self.v.z) => {
                along_vector_normal_h(plane::lift(self.v, from), from, up)
            }
            None if self.rotating_ground
                && self.ground_move.is_some_and(|(h, pull, _)| {
                    h == Vec2::new(self.v.x, self.v.z) && pull == self.pull
                }) =>
            {
                // The run control's first update (0x418390) reads the actual
                // actor velocity. Its pull can still use the previous floor
                // normal when the support rotated since the run was chosen.
                along_vector_normal_h(self.ground_move.unwrap().2, up, up)
            }
            _ => along_vector_normal_h(plane::lift(self.v, up), up, up),
        }
    }
    /// The run's velocity is laid in the plane of the floor it was steered on.
    fn run_follows_floor(&self) -> bool {
        matches!(self.run_up, Some((h, _, _)) if h == Vec2::new(self.v.x, self.v.z))
    }
    /// The stick in the plane of the floor with the normal `up`
    /// (`PlayerInput::calcMoveInput`): the camera's forward axis is tipped
    /// along the slope as the camera sees it, the side axis is across that.
    fn plane_stick(&self, up: Vec3) -> Vec3 {
        if let Some(frame) = &self.input_2d_frame {
            return self.input_2d.move_input(frame, up);
        }
        if let Some(view) = &self.view {
            let input = self
                .wall_along
                .snap(move_input::move_input(self.raw_stick, view, up), up);
            return self
                .snap_move_axis
                .map_or(input, |axis| vector_lib::verticalize_vec(axis, input));
        }
        let stick = self.ground_stick();
        let input = if up == Vec3::Y {
            stick
        } else {
            let forward = if self.forward == Vec3::ZERO {
                stick.normalize_or_zero()
            } else {
                self.forward
            };
            let right = forward.cross(Vec3::Y);
            let seen = plane::normalized_or_zero(plane::verticalize(right, up));
            let seen = if seen == Vec3::ZERO { up } else { seen };
            let ahead = plane::normalized_or_zero(along_vector_normal_h(forward, Vec3::Y, seen));
            let across = plane::normalized_or_zero(ahead.cross(up));
            across * stick.dot(right) + ahead * stick.dot(forward)
        };
        self.wall_along.snap(input, up)
    }
    /// `PlayerActionGroundMoveControl::appear` for a run that starts on
    /// sloped ground: the velocity the body had is laid into the plane of
    /// the floor, turned from the one it fell through (straight up when the
    /// floor was only just reached).
    fn run_appear(&mut self, landing: bool) {
        let Some(up) = self.mesh_ground else { return };
        let from = if landing { Vec3::Y } else { up };
        let velocity = along_vector_normal_h(self.v, from, up);
        self.set_h(velocity);
        let set = velocity - up * GRAVITY_MOVE;
        self.run_up = Some((Vec2::new(self.v.x, self.v.z), up, set));
    }
    /// The pose's up and front: tilted while a run on a slope has it so.
    pub fn pose_axes(&self) -> (Vec3, Vec3) {
        match self.pose_tilt {
            Some(tilt) if tilt.facing == self.facing => {
                let pose = plane::ActorPose::new(tilt.pose);
                (pose.up(), pose.front())
            }
            _ => (Vec3::Y, self.facing),
        }
    }
    /// The quaternion the actor's pose keeper was last given, when it is tilted.
    pub fn pose_quat(&self) -> Option<Quat> {
        self.pose_tilt
            .filter(|tilt| tilt.facing == self.facing)
            .map(|tilt| tilt.pose)
    }
    /// What `al::calcQuat` reads back from a tilted pose (the trace's quaternion): `toQuat` of
    /// the rotation matrix the stored quaternion makes.
    pub fn pose_read_back(&self) -> Option<Quat> {
        self.pose_quat()
            .map(|stored| plane::ActorPose::new(stored).calc_quat())
    }
    /// The actor's stored pose: the tilted one, or the upright pose that faces `facing`.
    fn actor_pose(&self) -> plane::ActorPose {
        plane::ActorPose::new(
            self.pose_quat()
                .unwrap_or_else(|| plane::quat_up_front(Vec3::Y, self.facing)),
        )
    }
    /// The actor's pose as the model's matrix: the pose's side, up and front axes as columns and
    /// the position (cm, replay frame), the third column being the front the head looks along.
    pub fn pose_matrix(&self) -> look_joint::Mat34 {
        let pose = self.actor_pose();
        let scale = self.press_scale();
        let (side, up, front) = (
            pose.side() * scale.x,
            pose.up() * scale.y,
            pose.front() * scale.z,
        );
        let feet = self.pos / UNIT;
        [
            [side.x, up.x, front.x, feet.x],
            [side.y, up.y, front.y, feet.y],
            [side.z, up.z, front.z, feet.z],
        ]
    }
    /// The skeleton posed for this frame: the model's world matrices for the clips' local ones,
    /// with the joint controllers bending each joint as the model's calculation gives it to them.
    pub fn pose_skeleton(
        &mut self,
        locals: &crate::skeleton_pose::Matrices,
    ) -> crate::skeleton_pose::Matrices {
        let actor = self.pose_matrix();
        let mut controls = joint_controls::Controls {
            swim: &self.joints.swim,
            ground: &mut self.joints.ground,
            look: &mut self.look,
            cap: &self.joints.cap,
            pole: &self.joints.pole,
            spring: &mut self.joints.spring,
            actor,
        };
        let world = crate::skeleton_pose::world_pose(&actor, locals, &mut controls);
        self.grab_ceil.root_offset =
            look_joint::translation(&world[crate::skeleton_pose::Joint::AllRoot as usize])
                - self.position_cm();
        // The next step's nerves read the joint axes this calculation leaves.
        self.model_axes = swim_dive_in::ModelAxes::of(&world);
        self.rocket.head = Some(world[crate::skeleton_pose::Joint::Head as usize]);
        self.record_hold_pose(&world);
        world
    }

    /// The controllers' state a pose calculation advances: the ground pose's, the look-at's and
    /// the hip spring's.
    /// A renderer that shows one step's pose over several frames keeps a copy from before the
    /// calculation and poses with it (`pose_skeleton_from`), so that the state moves once a step.
    #[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
    pub fn controllers(&self) -> ControllerState {
        (
            self.joints.ground.clone(),
            self.look.clone(),
            self.joints.spring,
        )
    }

    /// `pose_skeleton` with the controllers' state from `controllers`, which it advances; the
    /// player's own does not move.
    #[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
    pub fn pose_skeleton_from(
        &self,
        controllers: &mut ControllerState,
        locals: &crate::skeleton_pose::Matrices,
    ) -> crate::skeleton_pose::Matrices {
        let actor = self.pose_matrix();
        let mut controls = joint_controls::Controls {
            swim: &self.joints.swim,
            ground: &mut controllers.0,
            look: &mut controllers.1,
            cap: &self.joints.cap,
            pole: &self.joints.pole,
            spring: &mut controllers.2,
            actor,
        };
        crate::skeleton_pose::world_pose(&actor, locals, &mut controls)
    }

    /// The steps taken since the player was placed (a renderer poses once per step).
    #[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
    pub fn step_count(&self) -> u32 {
        self.age
    }
    /// `rs::waitGround`'s pose (every nerve of the standing state): the pose's
    /// up follows the floor's normal (at most 45 degrees from vertical) by the
    /// slerp rate; its front is the one it has.
    fn wait_pose(&mut self) {
        self.follow_ground_part();
        let up = self.mesh_ground.unwrap_or(Vec3::Y);
        let stored =
            self.actor_pose()
                .wait_ground(up, t::SLERP_QUAT_RATE_WAIT, t::WAIT_POSE_DEGREE_MAX);
        self.set_pose(stored);
    }
    /// The start of `rs::waitGround`: with ground under the feet (`rs::isCollidedGround`) the
    /// pose is carried along the collision part (`al::followRotateFrontAxisUp`, which stores the
    /// pose it makes). The first step after a teleport has found no ground yet.
    pub(super) fn follow_ground_part(&mut self) {
        if let Some((base, inverse)) = self.ground_frame {
            let pose = self.actor_pose().follow_part(&inverse, &base);
            self.set_pose(pose);
        }
    }
    /// The actor takes the orientation `pose` (`al::updatePoseQuat`); its facing is the
    /// front's horizontal direction.
    fn set_pose(&mut self, pose: Quat) {
        let facing = plane::ActorPose::new(pose)
            .front()
            .with_y(0.)
            .normalize_or_zero();
        if facing != Vec3::ZERO {
            self.facing = facing;
        }
        self.pose_tilt = Some(PoseTilt {
            pose,
            facing: self.facing,
        });
    }
    /// `al::faceToDirectionSupportUp` (0x8e9af0): the pose takes `front` with the
    /// pose's own up (`al::calcUpDir`), unless the two are parallel (limit 0.01).
    fn face_to_direction_support_up(&mut self, front: Vec3) {
        let (up, _) = self.pose_axes();
        if !pose_math::is_parallel_direction(front, up, pose_math::PARALLEL_LIMIT) {
            self.set_pose(pose_math::quat_front_up(front, up));
        }
    }
    /// `updatePoseUpFront` at the end of a run step: the pose follows the
    /// floor's normal, its front is the way the player means to go.
    fn steer_pose(&mut self, up: Vec3, front: Vec3) {
        if front.length_squared() <= 0.5 {
            return;
        }
        let stored = self.actor_pose().slerp_up_front(
            up,
            front,
            t::SLERP_QUAT_RATE,
            t::HILL_POSE_DEGREE_MAX,
        );
        self.set_pose(stored);
    }
    /// One step of the run nerve (`PlayerStateRunHakoniwa::exeRun`): the
    /// ground move control steers, the run cycle animates, then the nerve
    /// decides whether to brake, pivot or end.
    fn run_step(&mut self, o: &Orders, scene: &Course) {
        let step = self.run_steps;
        // The floor under the player and everything the control works on in
        // its plane: the velocity (`updateNormalAndSnap`), the facing and the
        // stick.
        let up = self.mesh_ground.unwrap_or(Vec3::Y);
        let in_plane = self.velocity_in_plane(up);
        let (pose_up, pose_front) = self.pose_axes();
        if step == 0 {
            self.ground.reset(in_plane, pose_front, up);
        }
        let after_turn = self.run_nerve == RunNerve::RunAfterTurn;
        // Top speed and ramp length of this step: the stretch after a turn
        // ramps to the speed the brake began at, a continued throw runs faster.
        let (max_speed, accel_frames) = if after_turn && step <= t::AFTER_TURN_FRAMES {
            (self.after_turn_speed, t::AFTER_TURN_FRAMES as i32)
        } else if self.throwing.run > 0 {
            (
                t::RUN_SPEED_CONTINUOUS_THROW,
                t::RUN_ACCEL_FRAMES_CONTINUOUS_THROW as i32,
            )
        } else {
            (t::NORMAL_MAX, t::ACCEL_FRAMES as i32)
        };
        self.update_surface_run();
        self.ground.force_speed = self.force_run.is_active().then_some(self.force_run.speed);
        self.ground.surface_run = self.surface_run.result;
        let steer = self.ground.update(&SteerInput {
            skate: self.on_ground_skate(),
            velocity: in_plane,
            up,
            actor_front: pose_front,
            actor_up: pose_up,
            stick: self.plane_stick(up),
            wall: self
                .touched
                .wall_contact
                .as_ref()
                .map(|wall| (wall.normal, wall.at_face)),
            max_speed,
            accel_frames,
        });
        if steer.slip_ice {
            self.events.push(Fx::SlipIce);
        }
        self.run_steps += 1;
        // exeRun replaces its speed local before both animation and IK (0x47f184).
        let pose_speed = if self.on_ground_skate() {
            t::DASH_JUDGE + 5.
        } else {
            steer.speed
        };
        let turn_tilt = if self.on_ground_skate() {
            0.
        } else {
            self.ground.turn_tilt_rate(up)
        };
        self.joints.run.run_step(pose_speed, turn_tilt);
        self.steer_pose(up, steer.front);
        self.set_h(steer.velocity);
        self.v.y = -GRAVITY_MOVE;
        self.pull = GRAVITY_MOVE;
        self.run_up = Some((Vec2::new(self.v.x, self.v.z), up, steer.set_velocity));
        self.mode = if steer.speed < 8. {
            Move::Walk
        } else {
            Move::Run
        };
        self.run_frames = self.ground.brake_counter as u32;
        if step == 0 {
            self.cycle.reset(pose_speed, after_turn);
        }
        self.update_move_anim(steer.speed);
        if step == 0 {
            self.run_start_effect = self.cycle.effect;
        }
        // A fast run asked to brake or reverse hands over to the brake nerve; a
        // slow one just stops and ends. A pull against the facing at a crawl
        // pivots. Speed spent with the stick released ends the run.
        let ended = if steer.brake_run {
            if steer.speed > RUN_BLEND_FLOOR {
                if self.ground.turn.reverse {
                    self.quick_turn = t::QUICK_TURN_JUMP_FRAMES;
                }
                self.switch(Move::Skid, "Brake", |p| p.enter_nerve(RunNerve::Brake));
                false
            } else {
                self.set_h(Vec3::ZERO);
                self.end_ground_child();
                true
            }
        } else if steer.pivot {
            self.switch(Move::Turn, "TurnPoint", |p| p.enter_nerve(RunNerve::Pivot));
            false
        } else if steer.stopped {
            self.end_ground_child();
            true
        } else {
            false
        };
        // The run state's own judge for the ground spin comes before the
        // actor's (jump, crouch, throw), which look at the state's result.
        if !ended {
            self.try_start_ground_spin();
            if !self.try_wait_run_forced_action(scene) {
                self.ground_actions(o);
            }
        }
    }
    /// The first step of one of the run state's nerves.
    fn enter_nerve(&mut self, nerve: RunNerve) {
        self.run_nerve = nerve;
        self.run_steps = 0;
        match nerve {
            RunNerve::Brake => {
                self.joints.run.stopped();
                self.events.push(Fx::Skid)
            }
            RunNerve::Turn => {
                self.joints.run.stopped();
                // The turn faces the other way at once and slides on.
                self.face_to_direction_support_up(-self.brake_dir);
                self.events.push(Fx::TurnDust);
            }
            RunNerve::Pivot => {
                self.joints.run.stopped();
                let front = self.facing;
                self.pivot.reset(front);
            }
            RunNerve::WallPush => {
                self.joints.run.stopped();
                self.cycle.effect = None;
            }
            RunNerve::Run | RunNerve::RunAfterTurn => {
                self.wall_push_counter = 0;
            }
        }
    }
    /// `PlayerStateRunHakoniwa2D3D::isEnableLookAt` hands `Run3D` to the base run state's own
    /// `isEnableLookAt`, which answers for its nerve: the run, the brake and the run after a
    /// turn may look; the turn and the pivot (`TurnPoint`) may not. The nerve counts from the
    /// step that chooses it (the keeper reads it in that step, a step before the clip starts).
    fn run_nerve_blocks_look(&self) -> bool {
        self.mode == Move::Turn
            || matches!(self.mode, Move::Walk | Move::Run) && self.run_nerve == RunNerve::WallPush
    }
    /// Child completion is observed after updateNerveState (0x959968): Run
    /// 0x423014, WallCatch 0x424830/0x424a50, PoleClimb 0x424b4c.
    /// Keep this update result separate from the child object's persistent dead
    /// byte, which can outlive the active NerveStateCtrl child (0x959a90).
    fn end_ground_child(&mut self) {
        self.ground_state_ended = true;
        self.nerve.deferred = true;
    }
    /// The original's run cycle: weights and pace by speed and tilt, and the
    /// footfalls the pose passes.
    fn update_move_anim(&mut self, speed: f32) {
        if !matches!(self.anim.name, "Move" | "MoveMoon") {
            return;
        }
        let tilt = self.ground_stick().length();
        let speed = if self.on_ground_skate() {
            t::DASH_JUDGE + 5.
        } else {
            speed
        };
        self.cycle.update(speed, tilt, self.constants);
        self.anim.rate = self.cycle.rate;
        self.step_phase = self.cycle.phase;
    }
    /// The brake, the turn after it and the pivot on the spot: the rest of
    /// the run state's nerves.
    fn brake(&mut self, o: &Orders, scene: &Course) {
        let step = self.run_steps;
        self.run_steps += 1;
        match self.run_nerve {
            RunNerve::Pivot => self.pivot_step(),
            RunNerve::Turn => self.turn_step(step),
            _ => self.brake_step(step),
        }
        // A pivot that ends the state has no spin judge.
        if !self.nerve.deferred {
            self.try_start_ground_spin();
        }
        // The actor's judges look at the state's result: the brake of this
        // frame has already slowed the player when a turning jump is chosen.
        if self.try_wait_run_forced_action(scene) {
            return;
        }
        if self.pre_input_jump_judges() && self.quick_turn > 0 {
            self.side_somersault();
        } else if self.pre_input_jump_judges() {
            self.ground_jump(o);
        }
    }
    /// `rs::moveBrakeRun`: slide on the way the brake began, losing speed.
    fn slide_on(&mut self) -> f32 {
        // New flower-source exports use the original ground-plane brake (0x565ae4).
        // Existing captures retain their established reconstruction arithmetic.
        if self.carry.seed.is_some() || self.carry.stone.is_some() {
            let up = self.ground_normal_or_up();
            let previous = self.run_up.map_or(up, |(_, n, _)| n);
            let velocity = self.velocity_in_plane(up);
            let before = vector_lib::length(velocity);
            let speed = ground::slide_speed(before);
            self.brake_dir =
                pose_math::normalize(along_vector_normal_h(self.brake_dir, previous, up));
            self.slerp_pose_up(up, t::SLERP_QUAT_RATE, t::WAIT_POSE_DEGREE_MAX);
            let velocity = self.brake_dir * speed;
            self.set_h(velocity);
            self.v.y = -GRAVITY_MOVE;
            self.pull = GRAVITY_MOVE;
            self.run_up = Some((
                Vec2::new(self.v.x, self.v.z),
                up,
                velocity - up * GRAVITY_MOVE,
            ));
            return before;
        }
        let before = self.speed_h();
        let speed = ground::slide_speed(before);
        self.set_h(self.brake_dir * speed);
        self.v.y = -GRAVITY_MOVE;
        self.pull = GRAVITY_MOVE;
        before
    }
    fn brake_step(&mut self, step: u32) {
        if step == 0 {
            self.brake_dir = if self.carry.seed.is_some() || self.carry.stone.is_some() {
                self.pose_axes().1
            } else {
                self.facing
            };
        }
        let before = self.slide_on();
        if step == 0 {
            self.after_turn_speed = before.clamp(t::NORMAL_MAX, t::AFTER_TURN_MAX);
        }
        // The brake's clip holds its last frame.
        if self.anim.name == "Brake" {
            self.anim.frame = self.anim.frame.min(clip::BRAKE as f32 + 1.);
        }
        let want = self.ground_stick();
        let deep = want.length() > DEEP_STICK;
        let along = self.brake_dir.dot(want.normalize_or_zero());
        // A stick held far the other way allows the turn-round jump.
        if deep && along <= 0. {
            self.quick_turn = t::QUICK_TURN_JUMP_FRAMES;
        }
        if deep && along > 0. {
            // Pushed on in the way of travel: run again.
            self.mode = if self.speed_h() < 8. {
                Move::Walk
            } else {
                Move::Run
            };
            self.switch(self.mode, "Move", |p| p.enter_nerve(RunNerve::Run));
        } else if deep && step >= t::BRAKE_TURN_START {
            self.switch(Move::Turn, "Turn", |p| p.enter_nerve(RunNerve::Turn));
        } else if step >= t::BRAKE_FRAMES as u32 {
            self.end_ground_child();
        }
    }
    fn turn_step(&mut self, _step: u32) {
        self.slide_on();
        if self.anim_done(clip::TURN) {
            // Turned round: run off the other way at half the speed the brake
            // began at, ramping up again.
            self.after_turn = t::AFTER_TURN_FRAMES;
            self.set_h(self.facing * (self.after_turn_speed * ground::RUN_AFTER_TURN_SCALE));
            self.mode = Move::Walk;
            self.switch(Move::Walk, "Move", |p| {
                p.enter_nerve(RunNerve::RunAfterTurn)
            });
        }
    }
    /// `PlayerStateRunHakoniwa::exePivot`: turn on the spot toward the stick.
    fn pivot_step(&mut self) {
        self.joints.run.pivoting();
        let flat_velocity = Vec3::new(self.v.x, 0., self.v.z);
        let step = if self.carry.seed.is_some() || self.carry.stone.is_some() {
            let up = self.ground_normal_or_up();
            let step = self.pivot.update_on_plane(
                self.plane_stick(up),
                self.pose_axes().1,
                self.velocity_in_plane(up),
                up,
            );
            self.steer_pose(up, step.front);
            step
        } else {
            let step = self
                .pivot
                .update(self.ground_stick(), self.facing, flat_velocity);
            // `PlayerActionPivotTurnControl::update` hands its front to
            // `rs::slerpUpFront` (0x418fc0, 0x4190a0), which stores the pose.
            self.steer_pose(self.ground_normal_or_up(), step.front);
            step
        };
        self.set_h(step.velocity);
        self.v.y = -t::GRAVITY;
        self.pull = t::GRAVITY;
        if self.carry.seed.is_some() || self.carry.stone.is_some() {
            let up = self.ground_normal_or_up();
            self.run_up = Some((
                Vec2::new(self.v.x, self.v.z),
                up,
                step.velocity - up * t::GRAVITY,
            ));
        }
        if self.pivot.done {
            if self.pivot.has_input {
                self.switch(Move::Walk, "Move", |p| p.enter_nerve(RunNerve::Run));
            } else {
                self.end_ground_child();
            }
        }
    }
    // ---- Squat, roll, slides --------------------------------------------
    /// Ground at least this many degrees steep.
    fn steep(&self, degrees: f32) -> bool {
        if let (true, Some(normal)) = (self.grounded, self.mesh_ground) {
            return normal.y <= degrees.to_radians().cos();
        }
        self.grounded && self.slope.abs().atan().to_degrees() >= degrees
    }
    /// `PlayerJudgeSlopeSlideRun::update` without the forced-slide floor code:
    /// the crouch button held, a floor steeper than the slide's start angle
    /// (`rs::isOnGroundSlopeSlideStart`: the velocity may not leave it), and a
    /// horizontal velocity that does not head uphill. The velocity is the
    /// actor's own, the pull into the floor included: a run up the slope's
    /// side, or down it slowly, still has a horizontal part against the
    /// slide's direction and no slide starts (the dome of `slope-slide`).
    fn slope_slide_judge(&self) -> bool {
        let Some(normal) = self.mesh_ground else {
            return self.steep(t::SLOPE_SLIDE_ANGLE_START);
        };
        if !self.grounded || !self.steep(t::SLOPE_SLIDE_ANGLE_START) {
            return false;
        }
        let velocity = self.units_per_frame();
        let leaving = plane::normalized_or_zero(velocity).dot(normal);
        if leaving > 0. && leaving.abs() >= 0.001 {
            return false;
        }
        let heading = plane::normalized_or_zero(velocity.with_y(0.));
        let downhill = plane::normalized_or_zero(normal.with_y(0.));
        heading != Vec3::ZERO && downhill != Vec3::ZERO && heading.dot(downhill) >= 0.
    }
    /// Throws while burning: the cap flies without interrupting the burn, except
    /// a spin throw once Mario has landed, which ends it.
    fn fire_throws(&mut self, o: &Orders) -> bool {
        let pressed = self.pre_throw > 0 && !o.crouch || o.up_throw || o.down_throw || o.spin_throw;
        if !pressed || self.cap.is_some() {
            return false;
        }
        self.pre_throw = 0;
        let kind = self.throw_kind(o);
        if kind == Throw::Spin && self.mode != Move::BurnJump {
            // The actor tries this cancellation after the child state update.
            return false;
        }
        self.sub_anim = Some(Anim {
            name: "ThrowCapParts",
            serial: self.anim.serial.wrapping_add(7),
            frame: 0.,
            rate: 1.,
        });
        self.launch_cap(
            if kind == Throw::Spin {
                Throw::Forward
            } else {
                kind
            },
            false,
        );
        false
    }
    /// Completed DamageFire child: direct Run unless SpeedCheckFall judges
    /// (0x4264e0..0x426534). Only puddle cancellation uses setNerveOnGround.
    fn end_burn(&mut self) {
        if self.ground_lost < 5 {
            self.run_frames = t::BRAKE_ON_COUNTER;
            self.go(Move::Run, "Move");
        } else {
            self.go(Move::Fall, "Fall");
        }
    }

    // ---- Jumps ------------------------------------------------------------
    /// Take off into a jump state. The take-off itself (the impulse, the
    /// sounds, the dust) is the state's first step, so it happens next frame;
    /// `then` adds what the particular jump does besides.
    fn jump_with(
        &mut self,
        mode: Move,
        anim: impl Into<Option<&'static str>>,
        power: f32,
        gravity: f32,
        then: impl FnOnce(&mut Player) + Send + Sync + 'static,
    ) {
        self.convert_velocity_on_just_land_jump(mode);
        self.enter_jump_state(mode, anim, power, gravity, then);
    }
    /// The rest of the jump state's appearance, once `convert_velocity_on_just_land_jump` has
    /// run. A caller that reads the velocity for its own request (the jump's power, the cap
    /// bounce's rising test) converts first, as `appear` does before every request's branch.
    fn enter_jump_state(
        &mut self,
        mode: Move,
        anim: impl Into<Option<&'static str>>,
        power: f32,
        gravity: f32,
        then: impl FnOnce(&mut Player) + Send + Sync + 'static,
    ) {
        self.hip_drop_reflect = false;
        self.pre_jump = 0;
        let requested_clip = anim.into();
        // Only special requests author a clip in appear. The ordinary jump selects its side
        // in exeJump's first step (0x47268c), before the take-off pose is installed.
        self.change(mode, requested_clip.unwrap_or("Jump"), move |p| {
            if requested_clip.is_none() {
                p.start_clip(p.standard_jump_clip());
            }
            p.prior_v = p.v;
            // (Read before the take-off sets the vertical speed: the ground-following velocity
            // is told by it.)
            // (A jump out of the water has its own rule for the floor's plane, `leave_ground`.)
            p.takeoff_h = p
                .swim
                .jump_from_floor
                .is_none()
                .then(|| p.takeoff_horizontal_velocity());
            p.leave_ground(mode, power, gravity);
            then(p);
            p.arm_air_control(mode, power, gravity);
        });
        if requested_clip.is_none() {
            // The first-step work starts an ordinary jump's clip itself.
            self.nerve.keep_clip();
        }
    }
    /// `PlayerStateJump::appear` (0x470c34..0x470cf4): a jump chosen on the step the collider
    /// just touched the ground (`rs::isJustLand`, 0x470c44) carries the velocity over from the
    /// plane it fell to the floor's: `al::alongVectorNormalH(v, -gravity, groundNormal)` less the
    /// floor's normal times the player constant's `Gravity` (`PlayerConst` slot 0, 3.0 at
    /// `PlayerConst.cpp:6`, not overridden by PlayerConstMoon). The block opens the one `appear`
    /// (0x470b9c), ahead of the request table (0x18db678), so it runs for every request that
    /// state takes: the ordinary, double and triple jumps, the cap's bounce and catch jumps, the
    /// spin and pound jumps, the backflip and the side somersault (their clips are the state's own
    /// nerves `exeJumpBack` 0x47392c and `exeJumpTurn` 0x473630). The long jump and the roll's jump
    /// are `PlayerStateLongJump`, which has an `appear` of its own. The jump's first step has not
    /// read the velocity yet, so the take-off sees the converted one.
    fn convert_velocity_on_just_land_jump(&mut self, mode: Move) {
        if matches!(mode, Move::LongJump | Move::RollJump) || !self.is_just_land() {
            return;
        }
        let normal = self.collided_ground_normal();
        self.v = pose_math::along_vector_normal_h(self.v, Vec3::Y, normal)
            - normal * PLAYER_CONST_GRAVITY;
    }
    fn leave_ground(&mut self, mode: Move, power: f32, gravity: f32) {
        // The jump nerve's first step resets its water judge (`PlayerJudgeOutInWater`).
        self.swim.out_in_inside = true;
        if let Some(incoming) = self.swim.jump_from_floor.take()
            && self.is_collided_ground()
        {
            let normal = self.collided_ground_normal();
            self.v = incoming - normal * normal.dot(incoming);
        }
        self.v.y = power;
        self.gravity = gravity;
        self.jump_extend = true;
        self.max_air_speed = self.speed_h().clamp(t::JUMP_MOVE_MIN, t::JUMP_MOVE_MAX);
        self.grounded = false;
        self.pre_jump = 0;
        // startHitReactionLandJumpIfLanding receives the model-2D flag at
        // 0x470bdc and suppresses the ordinary 3D landing/jump reaction.
        if !self.model_2d() {
            self.events.push(Fx::Jump(mode));
        }
    }
    /// Hand a jump of the shared family to its air control, as the jump
    /// state's first step does (`PlayerActionAirMoveControl::setup`): the
    /// horizontal velocity keeps its direction and the vertical one becomes
    /// the jump power.
    fn arm_air_control(&mut self, mode: Move, power: f32, gravity: f32) {
        if !mode.uses_air_control() {
            return;
        }
        let extend = if self.jump_extend {
            t::EXTEND_FRAMES
        } else {
            0
        };
        let mut takeoff = takeoff_of(mode, extend, power, gravity);
        if self.model_2d() && mode == Move::Jump {
            // exeJump 0x472830..0x472840 selects NormalMaxSpeed2D as base speed.
            takeoff.base_speed = 10.;
        }
        if self.force_run.is_active()
            && matches!(mode, Move::Jump | Move::DoubleJump | Move::TripleJump)
        {
            takeoff.speed_window = Some((
                0.,
                air_move::tune::JUMP_BASE_SPEED_MAX.max(self.force_run.speed),
            ));
            takeoff.speed_limit = takeoff.speed_limit.max(self.force_run.speed);
        }
        self.air.setup(&mut self.v, self.facing, &takeoff);
        // rs::calcJumpInertia (0x55ca7c) is separate from the external
        // velocity's one-frame departure request. It uses the moving ground
        // at takeoff, scaled by JumpInertiaRate and forward agreement.
        if self.collider.floor.is_some() {
            let force = self.ground_inertia.jump_force.with_y(0.);
            if force != Vec3::ZERO {
                let (direction, _) = pose_math::try_normalize_or_zero(force);
                let launch = if self.speed_h() < t::NORMAL_MIN {
                    Vec3::ZERO
                } else {
                    pose_math::try_normalize_or_zero(self.v.with_y(0.)).0
                };
                let rate = if launch == Vec3::ZERO {
                    0.7
                } else {
                    0.7 * pose_math::dot(direction, launch).max(0.)
                };
                self.air
                    .add_ground_inertia(&mut self.v, force * rate, &takeoff);
            }
        }
        self.air_turn.reset();
        self.air_for = Some(mode);
        self.jump_power = power;
    }
    /// A state flown by the air control that no take-off armed (a fall, or a
    /// recorded state being replayed): set it up from the velocity it has.
    fn arm_fall_control(&mut self) {
        if self.is_collided_ground() {
            // AirMove::setup projects initial horizontal inertia off the
            // contact normal before removing its vertical component (0x4148d0).
            let vertical = self.v.y;
            self.v = vector_lib::verticalize_vec(self.collided_ground_normal(), self.v);
            self.v.y = vertical;
        }
        let mut takeoff = fall_takeoff(self.constants, self.v.y);
        if self.fall_after_ledge.is_some() {
            takeoff.input_lock = ledge::LET_GO_INPUT_LOCK;
        }
        self.air.setup(&mut self.v, self.facing, &takeoff);
        self.air_turn.reset();
        self.air_for = Some(self.mode);
    }
    /// Put the air control into the middle of a recorded jump (lockstep).
    fn resume_air_control(&mut self, s: &Snapshot) {
        // The wall jump counts its steps from the sub-state, not the nerve's.
        let frames = if self.mode == Move::WallJump {
            s.sub_steps
        } else {
            s.state_frames
        }
        .unwrap_or(0);
        let (extend, mut gravity) = match self.mode {
            Move::DoubleJump => (t::EXTEND_FRAMES, self.constants.jump_gravity2nd),
            Move::TripleJump => (t::EXTEND_FRAMES, self.constants.jump_gravity3rd),
            Move::Jump if self.hip_drop_reflect => (0, self.constants.trample_hip_drop_gravity),
            Move::Jump => (t::EXTEND_FRAMES, self.constants.jump_gravity),
            Move::CapBounce => (0, self.constants.trample_high_gravity),
            Move::CatchJump => (0, self.constants.jump_gravity_cap_catch),
            Move::CapCatchPop => (0, self.constants.cap_catch_pop_gravity),
            Move::SpinJump => (0, self.constants.spin_jump_gravity),
            Move::WallJump => (0, self.constants.wall_jump_gravity),
            _ => (0, self.constants.jump_gravity),
        };
        let forced_jump = s.anim.is_some_and(|(name, _)| name == "JumpDashFast");
        if forced_jump {
            gravity = self.constants.jump_gravity_force_run;
            self.gravity = gravity;
        }
        let mut takeoff = if self.mode == Move::Fall {
            fall_takeoff(self.constants, self.v.y)
        } else if self.mode == Move::WallJump {
            wall_air::wall_jump_takeoff(self.constants)
        } else {
            takeoff_of(self.mode, extend, self.v.y, gravity)
        };
        if forced_jump {
            takeoff.speed_window = Some((
                0.,
                air_move::tune::JUMP_BASE_SPEED_MAX.max(self.force_run.speed),
            ));
            takeoff.speed_limit = takeoff.speed_limit.max(self.force_run.speed);
        }
        // The window is still open when the button was held since take-off.
        let held = self.jump_held_frames > frames;
        self.air
            .resume(self.v, self.facing, &takeoff, frames, held && self.v.y > 0.);
        self.air_for = Some(self.mode);
    }
    /// Which jump of the combo the next one is, as the jump state decides when
    /// it appears: the combo's count, except that the third needs a fast run
    /// along the direction of the last jump, or the combo starts over.
    fn pick_jump_kind(&mut self) -> u32 {
        if self.combo.count < 2 {
            self.combo_kept = true;
            return self.combo.count;
        }
        let along = self.v.dot(self.facing);
        let direction = self.facing * along.signum();
        let fast = along.abs() >= t::NORMAL_MAX * air_move::tune::THIRD_JUMP_SPEED_RATE;
        let aligned =
            direction.dot(self.combo.last_dir()) >= air_move::tune::THIRD_JUMP_DIRECTION_DOT;
        if fast && aligned {
            self.combo_kept = true;
            2
        } else {
            self.combo.clear(t::CONTINUOUS_JUMP_TIMER);
            self.combo_kept = false;
            0
        }
    }
    /// A jump that ends on the ground counts toward the combo, or ends it
    /// (`PlayerStateJump::tryCountUpContinuousJump`).
    fn count_jump_combo(&mut self) {
        // tryCountUpContinuousJump 0x471f7c..0x471fa8 clears for 2D models.
        if self.model_2d() || !self.combo_kept {
            self.combo.clear(t::CONTINUOUS_JUMP_TIMER);
            return;
        }
        let direction = self.v.with_y(0.).normalize_or_zero();
        let strong = self.jump_power >= t::JUMP_POWER_MAX * air_move::tune::THIRD_JUMP_SPEED_RATE;
        let aligned =
            direction.dot(self.combo.last_dir()) >= air_move::tune::THIRD_JUMP_DIRECTION_DOT;
        if self.combo.count == 0 || strong && aligned {
            self.combo.count_up(direction);
        } else {
            self.combo.clear(t::CONTINUOUS_JUMP_TIMER);
        }
    }
    /// What the original updates once the frame's nerve has run: the combo
    /// counts down only while standing or running (any other state ends it),
    /// and the long jump's count runs down outside the long jump.
    fn update_combo_counters(&mut self) {
        let nerve = self.actor_name();
        if nerve != "Jump" {
            self.hip_drop_reflect = false;
            self.combo
                .update(matches!(nerve, "Wait" | "Run"), t::CONTINUOUS_JUMP_TIMER);
        }
        // `PlayerActorHakoniwa::movement` skips it in the long jump's nerve
        // and in the roll's jump (`PlayerStateRolling::isRollingJump`).
        if !matches!(self.mode, Move::LongJump | Move::RollJump) {
            self.long_combo.update(t::CONTINUOUS_LONG_JUMP_TIMER);
        }
    }
    /// The player's side turned toward the camera's view direction
    /// (`rs::isPlayerSideFaceToCameraZ`): the first jump then plays the clip
    /// that is not mirrored.
    fn side_faces_camera(&self) -> bool {
        // rs::isPlayerSideFaceToCameraZ (0x55dc7c) reads the visible camera through
        // calcCameraLookDir/calcCameraUpDir, independently of getPlayerViewMtx's input override.
        // Project the actor matrix's side off camera up, normalize, strict dot<0.
        let view = self
            .camera_view
            .or(self.view)
            .unwrap_or_else(|| move_input::locked_view(self.camera_yaw.to_degrees()));
        let side = self.actor_pose().side();
        let side = pose_math::try_normalize_or_zero(vector_lib::verticalize_vec(view[1], side)).0;
        pose_math::dot(side, -view[2]) < 0.
    }
    fn standard_jump_clip(&self) -> &'static str {
        // PlayerStateJump::exeJump's 2D model branch at 0x472674.
        if self.model_2d() {
            return "Jump";
        }
        if self.side_faces_camera() {
            "Jump"
        } else {
            "JumpReverse"
        }
    }
    /// The first, second or third jump of the combo. Which one it is, and
    /// what that does to the combo, is decided as the jump state appears (when
    /// the jump is chosen); the take-off follows with its first step.
    fn start_standard_jump(&mut self) {
        self.pre_jump = 0;
        let kind = self.pick_jump_kind();
        let (mode, clip, mut power_min, mut power_max, gravity) = match kind {
            2 => (
                Move::TripleJump,
                "Jump3",
                t::CONTINUOUS_JUMP_POWER_MIN,
                t::JUMP_POWER_3RD,
                self.constants.jump_gravity3rd,
            ),
            1 => (
                Move::DoubleJump,
                "Jump2",
                t::CONTINUOUS_JUMP_POWER_MIN,
                t::JUMP_POWER_MAX_2ND,
                self.constants.jump_gravity2nd,
            ),
            _ => (
                Move::Jump,
                "Jump",
                t::JUMP_POWER_MIN,
                t::JUMP_POWER_MAX,
                self.constants.jump_gravity,
            ),
        };
        // `appear` converts the velocity the power is then read from.
        self.convert_velocity_on_just_land_jump(mode);
        let (speed_min, speed_max) = if self.model_2d() && self.dimension.in_area {
            // calcJumpPowerMin/Max and BorderSpeedMin/Max: 0x471cdc..0x471e84.
            // Border getters0x43d02c/0x43d034 read PlayerConst's 12/18 values.
            power_min = self.constants.jump_power_min2d_area;
            power_max = self.constants.jump_power_max2d_area;
            (12., 18.)
        } else {
            (t::NORMAL_MIN, t::NORMAL_MAX)
        };
        // The power follows the speed along the facing.
        let forward = self.v.dot(self.facing).abs();
        let power = air_move::jump_power(forward, speed_min, speed_max, power_min, power_max);
        let (clip, power, gravity) = if self.force_run.is_active() {
            ("JumpDashFast", 18., self.constants.jump_gravity_force_run)
        } else {
            (clip, power, gravity)
        };
        let ground_up = self.ground_pose_up_for_jump();
        let requested_clip = (kind != 0 || self.force_run.is_active()).then_some(clip);
        self.enter_jump_state(mode, requested_clip, power, gravity, move |p| {
            if let Some(horizontal) = p.takeoff_h {
                p.set_h(horizontal);
            }
            if let Some(up) = ground_up {
                p.turn_pose_up_for_jump(up);
            }
        });
    }
    fn turn_pose_up_for_jump(&mut self, up: Vec3) {
        // PlayerStateJump::exeJump (0x472790): a large first-step pose
        // correction requests resetCollisionExpandCheck, consumed only after
        // air-control setup has read the takeoff contact and its inertia.
        self.request_collision_reset_after_turn(self.pose_axes().0, up);
        self.slerp_pose_up(up, 1., 180.);
    }
    fn request_collision_reset_after_turn(&mut self, before: Vec3, after: Vec3) {
        let limit = pose_math::cosf(20. * pose_math::DEGREES_TO_RADIANS);
        if pose_math::dot(before, after) <= limit {
            self.collision_expand_pending = true;
        }
    }
    /// `PlayerStateJump::appear` keeps the ground pose's up (`PlayerJointControlKeeper::
    /// calcGroundPoseUp`) when the previous step ended the run or the standing state's body
    /// (`EActionTrigger 0x15`, which `exeRun` and `exeWait` set), and the jump's first step turns
    /// the actor's pose to it (`rs::slerpUp` with rate 1). The run reads the pose it began its
    /// step with: its own pose update comes after the jump is chosen.
    fn ground_pose_up_for_jump(&self) -> Option<Vec3> {
        let actor_up = match self.actor_name() {
            "Run" => self.joints.frame_start_pose_up,
            "Wait" => self.pose_axes().0,
            _ => return None,
        };
        Some(self.joints.ground.ground_pose_up(actor_up))
    }
    /// `rs::slerpUp`: the pose turns to `up`, keeping its front (the side crossed with the up
    /// when that is parallel to it).
    fn slerp_pose_up(&mut self, up: Vec3, rate: f32, max_degrees: f32) {
        let pose = self.actor_pose();
        let mut front = pose.front();
        if pose_math::is_parallel_direction(up, front, 0.01) {
            front = pose_math::normalize(pose_math::cross(pose.side(), up));
        }
        let stored = pose.slerp_up_front(up, front, rate, max_degrees);
        self.set_pose(stored);
    }
    /// `rs::slerpGravity` (0x55d6e4) with the air control's rate: `PlayerActionAirMoveControl::update`
    /// ends with it, and without the turn controller, when its flag (+0x30) is set (0x41508c..0x4150a8);
    /// the rate is `getSlerpQuatRate` (the const's vtable slot +0x30, 0x4150a0). An upright pose is
    /// already the slerp's target (its up is the negated gravity, its front the pose's own), so
    /// only a tilted pose changes.
    fn slerp_gravity(&mut self, rate: f32) {
        if let Some(stored) = self.pose_quat() {
            let relaxed = plane::ActorPose::new(stored).slerp_gravity(rate);
            self.set_pose(relaxed);
        }
    }
    /// The turn of a body in the air and its pose: `PlayerActionTurnControl::update` turns the
    /// front (the actor's front carried to the horizontal plane) toward the stick, and
    /// `rs::slerpUpFront` relaxes a tilted pose toward the world's up and that front at the
    /// state's `SlerpQuatRate`. An upright pose just faces the turned front.
    fn turn_in_air(&mut self, push: Vec3) {
        let (actor_up, actor_front) = self.pose_axes();
        let stick = if self.air.steered() { push } else { Vec3::ZERO };
        self.air_turn.update(turn_control::TurnFrame {
            actor_front,
            actor_up,
            stick,
            up: Vec3::Y,
        });
        let turned = self.air_turn.direction;
        match self.pose_quat() {
            Some(stored) => {
                let relaxed = plane::ActorPose::new(stored).slerp_up_front(
                    Vec3::Y,
                    turned,
                    t::SLERP_QUAT_RATE,
                    0.,
                );
                self.set_pose(relaxed);
            }
            None => self.facing = turned,
        }
    }
    /// The horizontal velocity a jump takes off with (`PlayerActionAirMoveControl::setup`): the
    /// velocity the actor holds, with the part along the normal of the ground it stands on taken
    /// off (the collider's present normal, up where there is none), then the part along gravity.
    /// A run on a slope holds the velocity of the plane it was steered on in its own horizontal
    /// part; the jump takes the one of the plane under it now (swim-hipdrop-in row 85: 17.6814 for
    /// the new floor against 17.6919 for the one the last step ran on).
    fn takeoff_horizontal_velocity(&self) -> Vec3 {
        let normal = if self.is_collided_ground() {
            self.collided_ground_normal()
        } else {
            Vec3::Y
        };
        let on_plane = vector_lib::verticalize_vec(normal, self.units_per_frame());
        vector_lib::verticalize_vec(Vec3::NEG_Y, on_plane).with_y(0.)
    }
    fn ground_jump(&mut self, _o: &Orders) {
        // Wait and Run cancel the explicit contribution before selecting Jump.
        // Squat's long/back jump branches do not call this feedback helper.
        if matches!(self.actor_name(), "Wait" | "Run") {
            self.feedback_ground_inertia(self.actor_name() == "Run");
        }
        // The actor's jump judge during the ground spin fills the request of
        // the spin jump before any other kind is considered.
        if self.mode == Move::GroundSpin {
            self.spin_jump();
            return;
        }
        if self.mode == Move::PoundLand && self.pound_jump_permitted() {
            self.start_pound_jump();
            return;
        }
        if self.catch_window > 0 {
            self.jump_with(
                Move::CatchJump,
                "JumpCapCatchCommon",
                t::CAP_CATCH_JUMP_POWER,
                self.constants.jump_gravity_cap_catch,
                |p| p.jump_extend = false,
            );
            return;
        }
        let pivot_without_turn_jump =
            self.mode == Move::Turn && self.clip() == "TurnPoint" && self.quick_turn == 0;
        // tryTurnJump (0x47ed24) requires the quick-turn counter in Pivot.
        // Naming Pivot in this run update does not by itself enable JumpTurn.
        if matches!(self.mode, Move::Skid | Move::Turn) && !pivot_without_turn_jump
            || self.quick_turn > 0 && self.after_turn > 0
        {
            self.side_somersault();
            return;
        }
        self.start_standard_jump();
    }
    /// The jump out of a hip drop's landing (the jump request 17, whether the landing was on
    /// the ground or in the water): `HipDropJumpPower`, no extension, no horizontal speed.
    fn start_pound_jump(&mut self) {
        self.jump_with(
            Move::PoundJump,
            "JumpHipDrop",
            self.constants.jump_hip_drop_power,
            self.constants.jump_gravity,
            |p| {
                p.set_h(Vec3::ZERO);
                p.jump_extend = false;
            },
        );
    }
    fn side_somersault(&mut self) {
        // The direction is the request's, taken from the stick as the jump is
        // chosen; a turning jump does not continue the combo.
        let direction = if self.tilt > 0.05 {
            self.stick
        } else {
            self.facing
        };
        self.combo.clear(t::CONTINUOUS_JUMP_TIMER);
        self.combo_kept = false;
        self.jump_with(
            Move::Sideflip,
            "JumpTurn",
            t::TURN_JUMP_POWER,
            self.constants.turn_jump_gravity,
            move |p| {
                p.facing = direction;
                p.set_h(direction * t::TURN_JUMP_VEL_H);
                p.jump_extend = false;
                p.quick_turn = 0;
            },
        );
    }
    fn backflip(&mut self) {
        self.combo_kept = true;
        self.jump_with(
            Move::Backflip,
            "JumpBack",
            t::SQUAT_JUMP_POWER,
            self.constants.squat_jump_gravity,
            |p| {
                p.set_h(-p.facing * t::SQUAT_JUMP_BACK);
                p.jump_extend = false;
                p.jump_power = t::SQUAT_JUMP_POWER;
            },
        );
    }
    fn long_jump(&mut self) {
        self.long_jump_as(Move::LongJump);
    }
    /// A long jump in the state `mode` (the roll's jump is one too).
    fn long_jump_as(&mut self, mode: Move) {
        const NAMES: [&str; 3] = ["JumpBroad", "JumpBroad2", "JumpBroad3"];
        let name = NAMES[self.long_combo.count as usize % 3];
        self.jump_with(
            mode,
            name,
            t::LONG_JUMP_POWER,
            self.constants.long_jump_gravity,
            |p| {
                p.long_combo.count_up();
                p.v = air_move::long_jump_velocity(
                    p.prior_v,
                    &air_move::LongJumpTakeoff {
                        move_pow: t::LONG_JUMP_MOVE_POW,
                        speed_min: t::LONG_JUMP_SPEED_MIN,
                        init_speed: t::LONG_JUMP_INIT_SPEED,
                        power: t::LONG_JUMP_POWER,
                    },
                );
                p.jump_extend = false;
            },
        );
    }
    /// The stick as a push in the world (length up to one).
    fn push(&self) -> Vec3 {
        self.stick * self.tilt
    }
    /// Long jump flight (`rs::moveDivingJump` with the long jump's values):
    /// slow forward acceleration, a stronger brake, light steering, and the
    /// facing follows the travel.
    fn long_jump_air(&mut self, o: &Orders) {
        self.long_jump_motion();
        self.air_actions(o);
    }
    /// The movement of a long jump step, without what the actor decides after.
    fn long_jump_motion(&mut self) {
        let mut facing = self.facing;
        let push = self.push();
        air_move::move_diving_jump(
            &mut self.v,
            &mut facing,
            push,
            &air_move::DivingJump {
                accel: t::LONG_JUMP_ACCEL,
                brake: t::LONG_JUMP_BRAKE,
                speed_max: t::LONG_JUMP_SPEED,
                speed_min: t::LONG_JUMP_SPEED_MIN,
                side_accel: t::LONG_JUMP_SIDE_ACCEL,
                gravity: self.constants.long_jump_gravity,
                fall_max: t::FALL_MAX,
                turn_rate: air_move::tune::SLERP_QUAT_GRAV,
            },
        );
        self.facing = facing;
    }
    /// Backflip flight (`rs::moveParallelJump`): a light push along and across
    /// the travel, a low speed cap and the squat jump's gravity.
    fn back_jump_air(&mut self, o: &Orders) {
        let push = self.push();
        air_move::move_parallel_jump(
            &mut self.v,
            push,
            &air_move::ParallelJump {
                front: air_move::tune::SQUAT_JUMP_MOVE_POWER,
                back: air_move::tune::SQUAT_JUMP_MOVE_POWER,
                side: air_move::tune::SQUAT_JUMP_MOVE_POWER,
                speed_max: t::SQUAT_JUMP_MOVE_MAX,
                gravity: self.constants.squat_jump_gravity,
                fall_max: t::FALL_MAX,
            },
        );
        self.air_actions(o);
    }
    /// Side somersault flight (`rs::moveDivingJump` with the turn jump's
    /// values); the body keeps the facing the jump started with.
    fn turn_jump_air(&mut self, o: &Orders) {
        let mut facing = self.facing;
        let push = self.push();
        air_move::move_diving_jump(
            &mut self.v,
            &mut facing,
            push,
            &air_move::DivingJump {
                accel: air_move::tune::TURN_JUMP_ACCEL,
                brake: air_move::tune::TURN_JUMP_BRAKE,
                speed_max: t::TURN_JUMP_VEL_H,
                speed_min: 0.,
                side_accel: air_move::tune::TURN_JUMP_SIDE_ACCEL,
                gravity: self.constants.turn_jump_gravity,
                fall_max: t::FALL_MAX,
                turn_rate: 0.,
            },
        );
        self.air_actions(o);
    }
    /// Flight of the jump family and of a fall: the shared air control moves
    /// the velocity, then the actor looks for what the air allows next (the
    /// ground pound, a throw), which is why the frame's gravity is already in.
    fn jump_air(&mut self, o: &Orders) {
        if self.air_for != Some(self.mode)
            || self.mode == Move::Fall && self.frame == 0 && self.is_collided_ground()
        {
            self.arm_fall_control();
        }
        self.scale_velocity_at_wall();
        // `PlayerStateJump::exeJump` (0x4725a0): a ceiling the last step collided with, after
        // the nerve's first step, is reflected (`rs::reflectCeiling` with no bounce keeps only a
        // downward speed) and ends the extend window and the held-jump flag.
        if self.mode == Move::Jump && self.frame > 0 && self.touched.ceiling.is_some() {
            self.v.y = self.v.y.min(0.);
            self.air.cancel_extend();
            self.combo_kept = false;
        }
        let steer = air_move::Steering {
            front: t::JUMP_ACCEL_FRONT,
            back: t::JUMP_ACCEL_BACK,
            turn: t::JUMP_ACCEL_TURN,
        };
        if let Some(frame) = &self.input_2d_frame {
            // Shared Jump/Fall call this before every air update (0x4729b4/0x46b498).
            self.air.verticalize_start_direction(frame.lock);
        }
        let push = self.push();
        self.air.update(&mut self.v, push, o.jump_held, &steer);
        self.turn_in_air(push);
        // The jump child handles the spin drop before the actor's cap-throw judge
        // (0x473494..0x4734ac, then 0x424174). A successful throw may still replace it.
        if self.mode == Move::SpinJump && o.crouch_pressed {
            self.start_spin_drop();
        }
        self.air_actions(o);
    }
    /// `PlayerStateJump::isEnableHipDropStart` (0x472368): the active spin-flower and
    /// ground-spin jump nerves, including their DownFall children, refuse the ordinary pound.
    /// The flower's own crouch transition is still missing: in the game a crouch press in
    /// `exeJumpSpinFlower` (isTriggerHipDrop 0x44c7ac) enters JumpSpinFlowerDownFall
    /// (0x1d7e560), which this controller does not model yet.
    fn hip_drop_start_enabled(&self) -> bool {
        !matches!(
            self.mode,
            Move::SpinJump | Move::SpinDrop | Move::SpinFlower
        )
    }
    /// Airborne transitions shared by every free-flight state.
    fn air_actions(&mut self, o: &Orders) -> bool {
        if self.try_cap_catch_pop(o) {
            return true;
        }
        // There is no dry dive: the only `setNerve(HeadSliding)` is `exeHipDrop`'s (0x424e8c), so
        // the head slide starts from the pound alone (`hip_drop`). A throw press in the air is the
        // cap throw whether or not crouch is held: `tryActionCapSpinAttackImpl` (0x4294cc, called
        // by `exeJump` 0x424174, `exeFall` 0x425254 and `tryChangeNerveFromAir` 0x423e50) and
        // `PlayerJudgePreInputCapThrow::update/judge` (0x45a95c/0x45ab14) read no crouch input.
        // The cap throw precedes the pound judge (0x424174/0x424228, 0x425254/0x4254bc,
        // 0x423e50/0x423e74). The shared helper skips the throw in SpinCap itself
        // (base+0x30 = 0x1d78940, 0x423e44): Throw and its SpinCap/Fall continuation.
        if self.actor_name() != "SpinCap"
            && (self.pre_throw > 0 || o.up_throw || o.down_throw || o.spin_throw)
            && self.cap.is_none()
        {
            self.pre_throw = 0;
            let kind = self.throw_kind(o);
            self.start_throw(kind);
            return true;
        }
        // The pound needs room above the ground (`PlayerJudgeStartHipDrop`).
        if o.crouch_pressed
            && self.hip_drop_start_enabled()
            && self.ground_height >= air_move::tune::HIP_DROP_HEIGHT
        {
            self.start_hip_drop();
            return true;
        }
        false
    }

    // ---- Ground pound and dive ----------------------------------------------
    fn start_hip_drop(&mut self) {
        self.change(Move::GroundPound, "HipDropStart", |p| {
            p.v = Vec3::ZERO;
            p.stand_pose_upright();
            p.events.push(Fx::PoundStart);
        });
    }
    /// `PlayerStateHipDrop::exeStart`'s first step turns the actor's pose to the world's up
    /// (`al::turnQuatYDirRadian` by a half turn at most), whatever tilt a jump left.
    fn stand_pose_upright(&mut self) {
        let Some(stored) = self.pose_quat() else {
            return;
        };
        let current = plane::ActorPose::new(stored).calc_quat();
        self.set_pose(pose_math::turn_quat_y_dir_radian(
            current,
            Vec3::Y,
            std::f32::consts::PI,
        ));
    }
    fn hip_drop(&mut self, o: &Orders) {
        // Only the active Start/Fall children reach this function; Land has its own handler
        // (`isEnableHeadSliding`, 0x470298). exeHipDrop (0x424e44..0x424e90) also requires
        // no carry and no ground. isTriggerHeadSliding (0x44c7e8) reads this frame's action
        // trigger (or a hand swing), not the cap throw judge's pre-input buffer.
        if !self.is_carry() && !self.is_on_ground_by(self.v) && o.throw_pressed {
            self.pre_throw = 0;
            self.dive();
            return;
        }
        if self.anim.name == "HipDropStart" {
            self.v = Vec3::ZERO;
            if self.anim_done(clip::HIP_DROP_START) {
                // The fall starts with its sub-state's first step.
                self.play("HipDrop");
            }
        } else {
            // exeFall (0x470658): initial HipDropSpeed, then HipDropGravity,
            // both limited by the unchanged HipDropSpeedMax (45).
            if self.anim.frame == 0. {
                self.v.y = (self.v.y - self.constants.hip_drop_speed).max(-45.);
            }
            self.v.y = (self.v.y - self.constants.hip_drop_gravity).max(-45.);
        }
    }
    /// `PlayerStateHipDrop::isEnableLandCancel`: the pound's landing nerve may
    /// be cancelled into a jump (or a throw) from its fifth to its thirtieth
    /// step. The actor reads the nerve's step after the landing nerve's own
    /// update has counted the current frame, so the frame in progress counts.
    fn pound_jump_permitted(&self) -> bool {
        (t::HIP_DROP_JUMP_BEGIN..=t::HIP_DROP_JUMP_END).contains(&(self.frame + 1))
    }
    fn hip_drop_land(&mut self, o: &Orders, scene: &Course) {
        // The landing keeps the ground pull of the walking states
        // (`rs::waitGround` with `GravityMove`).
        self.set_h(Vec3::ZERO);
        self.v.y = -air_move::tune::GRAVITY_MOVE;
        // The pull runs along the floor's normal, as the standing state's does (the pound
        // landing on the pond's slope, swim-hipdrop-in row 137: -7 times the normal).
        self.pull = GRAVITY_MOVE;
        // The landing also settles the pose to the floor (`rs::waitGround`'s slerp), on any floor
        // the standing states hold (`rs::isOnGroundRunAngle`; the slow body here is at the
        // flat end of its angle limit).
        if self.ground_flatter_than(t::STAND_ANGLE_MIN) {
            self.wait_pose();
        }
        if self.pre_jump > 0 && self.pound_jump_permitted() {
            self.ground_jump(o);
            return;
        }
        // The landing cancel window (`isEnableLandCancel`): a press before it opens stays in the
        // input buffer (`PlayerJudgePreInputCapThrow`) and acts when it does.
        if (self.pre_throw > 0 || o.throw_pressed) && self.pound_jump_permitted() {
            if o.crouch {
                // Roll straight out of the pound with a strong start.
                self.pre_throw = 0;
                self.start_roll(RollStart::Boost);
                return;
            }
            if self.cap.is_none() {
                self.pre_throw = 0;
                self.start_throw(Throw::Down);
                return;
            }
        }
        // From its 24th step (`isEnableMove`, counted like the cancel window)
        // the landing hands over to the run when the stick is pushed, and to
        // an ordinary jump when one is buffered (the pound's jump needed the
        // window above).
        let can_move = self.frame + 1 >= t::HIP_DROP_LAND_CANCEL;
        // `isEnableMove` tries the cap throw first, with no trigger: the plain throw (recorded:
        // cap-throw-rolling-late, a press at the 35th step of the landing).
        if can_move && (self.pre_throw > 0 || o.throw_pressed) && !o.crouch && self.cap.is_none() {
            self.pre_throw = 0;
            self.start_throw(Throw::Forward);
            return;
        }
        if can_move && self.pre_jump > 0 && self.tilt <= 0.1 && !o.crouch {
            self.ground_jump(o);
            return;
        }
        if self.anim_done(clip::HIP_DROP_LAND) {
            self.set_nerve_on_ground(o, scene); // exeHipDrop 0x424c48.
        } else if can_move && (self.tilt > 0.1 || o.crouch) {
            if o.crouch {
                self.start_squat();
            } else {
                self.go(Move::Idle, "Wait");
            }
        }
    }
    fn dive(&mut self) {
        self.change(Move::Dive, "HeadSlidingStart", |p| {
            // `PlayerStateHeadSliding::exeDive`'s first step (0x46fe78..0x46fe94): the horizontal
            // part of the velocity, or the actor's front when there is none. The stick is not read.
            let (along, moving) = pose_math::try_normalize_or_zero(p.v.with_y(0.));
            let h = if moving { along } else { p.facing };
            p.facing = h;
            let up = p.v.y.max(0.) + p.constants.head_sliding_jump;
            p.v = h * p.constants.head_sliding_speed + Vec3::Y * up;
            p.dive_possible = false;
            p.head_slide_dive_in = false;
            p.events.push(Fx::Dive);
        });
    }
    fn head_sliding(&mut self, _o: &Orders, scene: &Course) {
        self.head_sliding_motion();
        if self.anim.name == "HeadSlidingStart" && self.anim_done(clip::HEAD_SLIDING_START) {
            self.play("HeadSliding");
        }
        // `PlayerStateHeadSliding::exeDive` ends by judging the fall into water; the state keeps
        // the answer for the water entry (`isEnableDiveInWater`).
        if self.dive_in_water_enabled(scene) {
            self.head_slide_dive_in = true;
        }
    }
    /// Diving flight (`rs::moveDivingJump` with the dive's values): no
    /// forward acceleration, braking when pulled back, a light side push.
    fn head_sliding_motion(&mut self) {
        let mut facing = self.facing;
        let push = self.push();
        air_move::move_diving_jump(
            &mut self.v,
            &mut facing,
            push,
            &air_move::DivingJump {
                accel: 0.,
                brake: t::HEAD_SLIDING_BRAKE,
                speed_max: self.constants.head_sliding_speed,
                speed_min: t::HEAD_SLIDING_SPEED_MIN,
                side_accel: t::HEAD_SLIDING_SIDE_ACCEL,
                gravity: self.constants.head_sliding_gravity_air,
                fall_max: t::FALL_MAX,
                turn_rate: air_move::tune::SLERP_QUAT_GRAV,
            },
        );
        self.facing = facing;
    }

    // ---- Cap ---------------------------------------------------------------
    fn throw_kind(&self, o: &Orders) -> Throw {
        if o.up_throw {
            Throw::Up
        } else if o.down_throw {
            Throw::Down
        } else if o.spin_throw
            || matches!(
                self.mode,
                Move::GroundSpin | Move::SpinJump | Move::SpinDrop
            )
        {
            // A throw during a spin sets the trigger the cap's spin move
            // throw reads (`PlayerTrigger::isOnSpinMoveCapThrow`). The clip comes
            // from the attack's name builder (`throw_clip`) and the flight is the spiral
            // recorded in `cap-throw-spiral*` (`cap_spiral.rs`).
            Throw::Spin
        } else {
            Throw::Forward
        }
    }
    /// `PlayerSpinCapAttack::setupAttackInfo`: a throw soon after a catch continues the count;
    /// the third one is the continuous throw.
    fn setup_attack_info(&mut self) {
        let recent = self.catch_window > 0;
        self.throwing.count = if self.throwing.count != 0 && recent {
            self.throwing.count + 1
        } else {
            1
        };
        self.throwing.continuous = recent && self.throwing.count >= 3;
        if self.throwing.continuous {
            self.throwing.count = 0;
        }
    }
    /// The spin throw begins (`PlayerStateSpinCap` appearing and its first
    /// step): the throw is counted among those in a row, the clip is chosen,
    /// and the cap waits in its spin attack for the throw frame.
    fn start_throw(&mut self, kind: Throw) {
        if self.cap.is_some() || self.launch.is_some() || !self.sand_cap_enabled() {
            return;
        }
        // The child `PlayerStateSpinCap::appear` picks for the actor as it is now.
        let air = self.spin_cap_in_air();
        // The velocity it hands the ground child after a fall that lands within the run angle
        // (trigger 1), read before anything below changes the actor.
        let landing_velocity = self.spin_cap_fall_landing_velocity();
        self.setup_attack_info();

        // `exeWallAir` (0x4245f8, the only writer of `PlayerTrigger` action 0 and `appear` the only
        // reader, xref over the executable) sets the trigger when the throw is pressed in the
        // wall state's steps that are not in the air: the slide, and the wall jump's first step
        // (`PlayerStateWallAir::isAir` needs the air nerve past its step 0). The set and the
        // appear belong to the same step (`NerveKeeper::update` changes the nerve at its tail) and
        // the next `movement` clears it (0x41e000), so the state keeps the byte it read.
        self.throwing.after_wall_air =
            self.mode == Move::WallJump && self.frame == 0 && !self.nerve.deferred;
        if self.throwing.after_wall_air {
            // The press first resets the input's wall-along memory (`PlayerInput::resetAlongWall`,
            // 0x4245ec: frames and weight to zero), so the helper's move input is not bent
            // along the wall the jump left, unless the next frame's update remembers a wall again.
            self.wall_along.reset();
            // Then it runs `PlayerStateWallAir::startSlideSpinAttack` (0x4245fc): the wall jump is
            // recorded again, from 20 below the body (0x48f6c8), and the actor turns to face
            // back, upright (0x48f6dc..0x48f740, `makeQuatFrontUp`).
            self.record_wall_jump_throw();
            let front = self.actor_pose().front();
            self.set_pose(pose_math::quat_front_up(-front, pose_math::NEGATED_GRAVITY));
        }
        self.throwing.kind = Some(kind);
        self.throwing.up = None;
        self.throwing.in_air = air;
        self.throwing.ground_seen = false;
        let name = throw_clip(
            kind,
            air,
            self.throwing.count,
            self.throwing.continuous,
            !self.spin_clockwise,
        );
        // `exeSpinGround` and `exeSpinAir` ask the keeper for the spine's swing in their first step;
        // a thrown button has no swing angle.
        let swing = if air {
            cap_throw::Swing::in_air(self.throwing.count)
        } else {
            cap_throw::Swing::on_ground(self.throwing.count)
        };
        if let Some(swing) = swing {
            self.joints.cap.ask(swing, BUTTON_THROW_SWING_ANGLE);
        }
        if !air {
            // The state's ground control pulls the player to the floor from
            // the frame it is chosen.
            self.pull = GRAVITY_MOVE;
            match landing_velocity {
                // The brake of the first step then works on the velocity carried onto the ground.
                Some(velocity) => self.v = velocity,
                None => self.v.y = -GRAVITY_MOVE,
            }
        }
        self.throwing.dir = None;
        self.change(Move::Throw, name, move |p| {
            // First step of the state (`updateSpinGroundNerve` and
            // `updateSpinAirNerve`): the actor turns to the throw direction.
            p.throwing.model_shift = None;
            if air
                && !p.is_collided_ground()
                && p.model_axes.root_up.y < 0.5
                && p.model_axes.root_up != Vec3::ZERO
            {
                p.start_spin_air_model_pose();
            } else {
                p.face_throw_direction();
            }
            // Both spin starts clear an upper-body animation still attached
            // (`isUpperBodyAnimAttached`, `clearUpperBodyAnim`): a catch's.
            if p.upper_catch.take().is_some() {
                p.upper_cleared = true;
            }
            if air {
                // `PlayerActionAirMoveControl::setup` with the spin's values:
                // the small lift of every air throw.
                p.air.setup(&mut p.v, p.facing, &spin_air_takeoff());
                p.air_turn.reset();
                p.air_for = Some(Move::Throw);
            }
            p.winding = true;
            // HackCap::startSpinAttack (0x3fc59c) clears +0x2a3 as well as setting its nerve.
            p.cap_catched = false;
            p.winding_age = 0;
            p.spin_ik = false;
            p.throw_clip = name;
            if air {
                // The air spin has no sub animation (`startCapSpinAttackAir`).
                p.sub.active = false;
            } else {
                p.start_sub(name, clip::throw(name) as f32, 0.);
            }
        });
    }
    /// The first step of a spin: `PlayerInput::calcCapThrowInput` is the
    /// stick's world direction (the actor's own front when it is not pushed),
    /// the actor takes it at once (`rs::slerpUpFront` with rate 1) and the cap
    /// will leave along it whatever the facing does later.
    fn face_throw_direction(&mut self) {
        let up = self
            .mesh_ground
            .filter(|_| self.is_collided_ground())
            .unwrap_or(Vec3::Y);
        // The ground the collider holds (`rs::isCollidedGround` alone, whatever the actor's own
        // grounded state says), as `launch_up_axis` hands it to the cap.
        let ground = self
            .is_collided_ground()
            .then(|| self.collided_ground_normal())
            .filter(|normal| *normal != Vec3::Y);
        let dir = if self.throwing.after_wall_air {
            // The helper's ground is the collided one, else the actor's own up axis.
            let normal = if self.is_collided_ground() {
                self.collided_ground_normal()
            } else {
                self.actor_pose().up()
            };
            self.throw_direction_across(normal, true)
        } else if let Some(normal) = ground {
            self.throw_direction_across(normal, false)
        } else if self.tilt >= STICK_DEADZONE {
            self.stick
        } else {
            self.facing
        };
        let stored = self
            .actor_pose()
            .slerp_up_front(up, dir, 1., t::HILL_POSE_DEGREE_MAX);
        self.set_pose(stored);
        self.throwing.dir = Some(dir);
    }
    /// The throw direction on sloped ground (the helper at 0x4831b8, called by both spin
    /// states' first step): `PlayerInput::calcCapThrowInput` is the move input in the plane of
    /// the ground normal (`calcMoveInputImpl`, 0x44dd84); without a direction the actor's front
    /// is carried from its up axis onto that plane (0x4832c8..0x483300). On level ground the
    /// stick is already the direction. With `after_wall_air` (the helper's last argument, the
    /// `PlayerTrigger` 0 branch, 0x4831fc..0x4832b8) the move input that points back within the
    /// band between 90 and 15 degrees loses its part along the direction behind the actor (its
    /// negated front carried onto the plane of the normal), so the throw cannot turn back into
    /// the wall the jump left. The comparison uses the move input as it is, not its direction, so
    /// a half-tilted stick reaches the band at a smaller angle.
    fn throw_direction_across(&self, normal: Vec3, after_wall_air: bool) -> Vec3 {
        let pose = self.actor_pose();
        let mut input = self.plane_stick(normal);
        if after_wall_air {
            let (behind, has_behind) = pose_math::try_normalize_or_zero(along_vector_normal_h(
                -pose.front(),
                pose.up(),
                normal,
            ));
            if has_behind {
                let toward = pose_math::dot(input, behind);
                if toward > 0. && toward < THROW_BACK_COS {
                    input = vector_lib::verticalize_vec(behind, input);
                }
            }
        }
        let (input, has_direction) = pose_math::try_normalize_or_zero(input);
        if has_direction {
            return input;
        }
        pose_math::normalize(pose_math::along_vector_normal_h(
            pose.front(),
            pose.up(),
            normal,
        ))
    }
    /// Start a sub animation at the supplied counter. Callers before `begin_step` account
    /// for its same-frame advance; the post-actor carry keeper starts at frame 0
    /// (skeleton start 0x8001f4..0x800210).
    fn start_sub(&mut self, name: &'static str, length: f32, frame: f32) {
        self.sub = SubClip {
            name,
            frame,
            length,
            active: true,
        };
    }
    /// The step of the throw state at which the cap leaves
    /// (`PlayerSpinCapAttack::getThrowFrameGround` and `getThrowFrameAir`).
    fn throw_frame(&self, kind: Throw, air: bool) -> u32 {
        throw_frame_of(kind, self.throwing.count, self.throwing.continuous, air)
    }
    /// The throw state (`PlayerStateSpinCap`): `exeSpinGround` or `exeSpinAir`, whichever child
    /// `appear` started (0x481958: `setNerve` at 0x481ad4 / 0x481af0 choose it, and nothing in
    /// the class changes it afterwards but `exeSpinAir`'s own end, the fall nerve after step 22,
    /// 0x482a38). A ground spin whose actor leaves the floor keeps the ground child's throw frame
    /// and control, and an air spin that touches the floor keeps the air child's.
    fn throw_state(&mut self, o: &Orders, scene: &Course) {
        let kind = self.throwing.kind.unwrap_or(Throw::Forward);
        let air = self.throwing.in_air;
        let step = self.frame;
        // The cap leaves on the throw frame, if it is still the spin attack.
        if self.winding && self.launch.is_none() && step == self.throw_frame(kind, air) {
            self.launch_cap(kind, self.throwing.continuous);
            if air && let Some(shift) = self.throwing.model_shift {
                // exeSpinAir (0x4828b0) supplies the un-repaid shift after this step.
                self.throwing.launch_offset = -shift * 9_u32.saturating_sub(step) as f32;
            }
        }
        if air {
            self.spin_air(o, step);
        } else {
            self.spin_ground(o, step, scene);
        }
    }
    /// `PlayerStateSpinCap::exeSpinGround` and the actor's cancels
    /// (`PlayerActorHakoniwa::exeSpinCap`).
    fn spin_ground(&mut self, o: &Orders, step: u32, scene: &Course) {
        // `updateSpinGroundNerve`: the spin brakes the momentum for its first
        // frames; from then on the state's ground move control steers, from
        // the speed the player has (and so moves off at once when the stick
        // is held, a plain `max / frames` per frame, see `GroundMove::plain`).
        // The floor pulls with the ground control's gravity throughout.
        //
        // The braking steps belong to the inertia only while the force-run counter is down and
        // the player is not running on the water's surface (0x48242c..0x482474: the counter's
        // value above zero, else `rs::isJudge` of the surface-run judge, then
        // `isLessStep(SpinBrakeFrame)`); otherwise the control steers from the first step, as
        // it does for every step from `SpinBrakeFrame` on. [V] The ground state's appear calls
        // control.appear (0x481a74..0x481a78 -> 0x416a7c); its tail initializes the brake counter
        // and resets the turn controller (0x416b78..0x416b84 -> 0x419ba8).
        // [I] We defer that reset to the first controlled step at step 0; the original does
        // not reset here in updateSpinGroundNerve (0x482690..0x4827a4). Equivalence assumes
        // no intervening control update or change to the velocity/facing used for the reset.
        let controlled = self.force_run.is_active() || self.surface_run.result;
        if step < t::SPIN_BRAKE_FRAMES && !controlled {
            self.spin_cap_inertia();
        } else {
            if step == t::SPIN_BRAKE_FRAMES || step == 0 {
                self.spin_ground_move.reset(self.v, self.facing, Vec3::Y);
            }
            let (actor_up, actor_front) = self.pose_axes();
            let steer = self.spin_ground_move.update(&SteerInput {
                skate: self.on_ground_skate(),
                up: Vec3::Y,
                wall: None,
                velocity: self.v,
                actor_front,
                actor_up,
                stick: self.ground_stick(),
                max_speed: t::NORMAL_MAX,
                accel_frames: t::ACCEL_FRAMES as i32,
            });
            // SpinCap enables the ground control's turn tilt (+0xbb at
            // 0x481844); updateNormalMove applies calcTilt before
            // updatePoseUpFront (0x418294..0x4182d0). Run disables that flag,
            // but its appearance retains this outgoing pose.
            let pose_up = self.spin_ground_move.turn.tilted_up(steer.up, 20.);
            self.steer_pose(pose_up, steer.front);
            self.set_h(steer.velocity);
        }
        self.v.y = -GRAVITY_MOVE;
        self.pull = GRAVITY_MOVE;
        // The tail of `updateSpinGroundNerve`: within three frames of the clip's end the state
        // keeps asking for the inverse kinematics until it is chosen again.
        if clip::throw(self.anim.name) as f32 - self.anim.frame <= 3. {
            self.spin_ik = true;
        }
        // Once the cap is away the actor's ground chain may cancel the spin
        // (`isEnableCancelGround`, `exeSpinCap` 0x423aa4..0x423d80). The step the state ends is
        // not one of them: `exeSpinGround` calls `kill` (0x481b08) on the clip's end, so
        // `updateNerveState` (0x4239c0) answers true and 0x4239c4 branches straight to
        // `setNerveOnGround` (0x423b08), skipping the chain and the history restore. That
        // function runs its own judges (the run-angle gate, the forced slide, rolling, sand,
        // pole, grab, then the stand-up squat with its velocity cut, the jump, the run and the
        // squat button), which `set_nerve_on_ground` carries.
        if !self.winding && self.anim_done(clip::throw(self.anim.name)) {
            self.throwing.child_ended = true;
            self.set_nerve_on_ground(o, scene); // exeSpinCap 0x423b18.
            return;
        }
        // 0x423a1c: a carried object (picked up before the first child step or after launch) ends the
        // spin at once through `setNerveOnGround` (0x423a24 -> 0x423b08), ahead of the water
        // judge and of every cancel of the chain below. The child's end has priority. The binary
        // restores history before this diversion (0x4239d4); the port schedules that restore
        // after run_state and the post-state judges, in restore_after_spin_cap.
        if self.is_carry() {
            // The state ends: `PlayerStateSpinCap::kill` (0x481b08) takes a cap that has not left
            // back (`HackCap::cancelCapState`, then put on). Only the step between choosing the
            // spin and its first step can have an object picked up with the cap still winding.
            if std::mem::take(&mut self.winding) {
                self.winding_age = 0;
            }
            self.set_nerve_on_ground(o, scene);
            return;
        }
        if self.winding {
            return;
        }
        // Of the chain the port carries two judges, in the binary's order: the stand-up judge
        // (+0x3e8, 0x423cb0) sets the Squat nerve when a ceiling forbids standing on a collided
        // floor, then the jump (+0x4a0, 0x423d6c) behind `rs::isOnGroundRunAngle` (0x423d60).
        // No button is read here: the crouch press (StartSquat, +0x480) is asked only by
        // `setNerveOnGround` (0x421a44). Not ported, in the binary's order: the rethrow
        // (`isEnableReThrowCap`), the forced slope slide (+0x3f8), the forced rolling (+0x400),
        // sand sink (+0x450), pole climb (+0x498), grab ceiling (+0x408) and, after the
        // stand-up judge, the water-surface run (+0x488) and the dive (+0x410). The wall catch
        // (+0x4d8, asked before them) is `judge_wall_catch`. The carry diversion of 0x423a1c
        // (`isCarry`, then `setNerveOnGround`) is the diversion above. A spin never starts while
        // carrying (`tryActionCapSpinAttackImpl` refuses at 0x4295d0), but `attackSensor`
        // (0x427818) starts a carry in NrvSpinCap once `PlayerStateSpinCap::isEnableCancelCarry`
        // (0x481f10) holds, which is in SpinGround after the cap has left
        // (`spin_cap_enable_cancel_carry`).
        if !self.judge_enable_stand_up() {
            // 0x423cdc..0x423cfc: release a carried object, then the Squat nerve.
            self.start_cancel_and_release(scene.mesh.as_deref());
            self.start_squat();
        } else if self.pre_jump > 0 && self.is_on_ground_run_angle() {
            self.ground_jump(o);
        }
    }
    /// `PlayerStateSpinCap::exeSpinAir`: the small hang, the cap leaving on
    /// the air throw frame, then the fall once the cap is away.
    fn spin_air(&mut self, o: &Orders, step: u32) {
        // `updateSpinAirNerve`: the ordinary air control, set up on the first
        // step with the spin's speed limit, lift and gravity.
        let steer = air_move::Steering {
            front: t::JUMP_ACCEL_FRONT,
            back: t::JUMP_ACCEL_BACK,
            turn: t::JUMP_ACCEL_TURN,
        };
        let push = self.push();
        self.air.update(&mut self.v, push, o.jump_held, &steer);
        if let Some(shift) = self.throwing.model_shift {
            if step < 10 {
                self.pos -= shift * UNIT;
            }
            let pose = self.actor_pose();
            let ceiling = self.air_turn.update_sky(turn_control::TurnFrame {
                actor_front: pose.front(),
                actor_up: pose.up(),
                stick: if self.air.steered() && push != Vec3::ZERO {
                    push
                } else {
                    self.throwing.dir.unwrap_or(self.facing)
                },
                up: Vec3::Y,
            });
            let target = self.air_turn.direction;
            // slerpSkyPoseAir retains the ceiling-side hemisphere until the
            // front or up faces the requested direction (0x55d040).
            self.set_pose(
                if !ceiling || pose_math::dot(pose.front(), target) > 0. || pose.up().y > 0. {
                    pose.slerp_up_front(Vec3::Y, target, 0.1, 0.)
                } else {
                    pose_math::slerp(
                        pose.calc_quat(),
                        pose_math::quat_up_front(Vec3::Y, target),
                        0.1,
                    )
                },
            );
        } else {
            self.turn_in_air(push);
        }
        // The actor's cancels (`PlayerActorHakoniwa::exeSpinCap`): while the air throw runs its
        // first 23 steps only the pound may cancel it (`isEnableCancelHipDrop`: once the cap has
        // left its spin attack); a jump press does nothing (recorded: cap-throw-air-jump). From
        // the 24th step the state falls and every air judge may act (`isEnableCancelAir`).
        let can_pound = self.ground_height >= air_move::tune::HIP_DROP_HEIGHT;
        if !self.winding && o.crouch_pressed && can_pound {
            // cancelPoseInterpole (0x481c08) ends the tilted handover at once.
            if self.throwing.model_shift.is_some() && step <= 20 {
                self.set_pose(pose_math::quat_up_front(
                    Vec3::Y,
                    self.throwing.dir.unwrap_or(self.facing),
                ));
            }
            self.start_hip_drop();
            return;
        }
        if step > 22 && !self.winding {
            self.set(Move::Fall);
            self.spin_fall = true;
            self.gravity = self.constants.gravity_air;
        }
    }
    /// The up axis the ground and air spins hand the cap (`PlayerSpinCapAttack::startCapThrow`'s
    /// second argument): `exeSpinAir` hands `-al::getGravity` (0x4829d4) without asking the
    /// collider; `exeSpinGround` asks `rs::calcGroundNormalOrGravityDir` (0x482174 -> 0x55baac),
    /// which asks `rs::isCollidedGround` alone (the collider's depth, 0x565e8c): the collided
    /// ground's normal, or `-gravity` when the collider holds no ground, whatever the actor's own
    /// grounded state is. (The swim spins hand the vector they stored, 0x489ed4 and 0x48a424:
    /// `launch_cap_across`.)
    ///
    /// `air_child` is the child `PlayerStateSpinCap::appear` chose (`spin_cap_in_air`).
    fn launch_up_axis(&self, air_child: bool) -> Vec3 {
        if !air_child && self.is_collided_ground() {
            self.collided_ground_normal()
        } else {
            pose_math::NEGATED_GRAVITY
        }
    }
    fn throw_launch(&self, kind: Throw, continuous: bool, up: Vec3, air_child: bool) -> Launch {
        let dir = self.throwing.dir.unwrap_or(self.facing);
        // A throw whose up axis is not the world's flies along the direction as given (the
        // throw's direction already lies across that axis); the others along the horizontal.
        let front = if up == Vec3::Y {
            dir.with_y(0.).normalize_or(Vec3::NEG_Z)
        } else {
            dir
        };
        Launch {
            front,
            kind,
            left: false,
            air: air_child,
            continuous,
            spiral_side: spiral_side_of(Some(kind), !self.spin_clockwise),
            up,
        }
    }
    /// `HackCap::startThrow`: the cap starts flying on the next frame.
    fn launch_cap(&mut self, kind: Throw, continuous: bool) {
        // The spin state chose its child when it appeared (`start_throw`). The throws the port
        // launches without a state of their own (the run throw, the burn jump's) went through the
        // same `appear`: ask its choice now.
        let air_child = if self.mode == Move::Throw {
            self.throwing.in_air
        } else {
            self.spin_cap_in_air()
        };
        self.launch_cap_across(kind, continuous, self.launch_up_axis(air_child), air_child);
    }
    /// [`Self::launch_cap`] with the up axis the thrower hands over.
    fn launch_cap_across(&mut self, kind: Throw, continuous: bool, up: Vec3, air_child: bool) {
        self.throwing.launch_offset = Vec3::ZERO;
        self.launch = Some(self.throw_launch(kind, continuous, up, air_child));
    }
    /// The cap's frame, which comes before the player's
    /// (`HackCap::movement` before `PlayerActorHakoniwa::movement`).
    fn update_cap(&mut self, o: &Orders, scene: &Course) {
        // The cap reads the throw button as the last frame left it (recorded: a
        // press in the stay's first step starts the hold reaction one step later).
        let hold = std::mem::replace(&mut self.throw_held_before, o.throw_held);
        if self.winding {
            self.winding_age += 1;
        }
        // The catch nerve counts its frames after the arrival.
        if let Some(frames) = self.catching {
            let frames = frames + 1;
            self.catching =
                (frames <= crate::cap::catch_frames(self.catch_clip) as i32).then_some(frames);
        }
        if let Some(frames) = self.pop_catch {
            let frames = frames + 1;
            self.pop_catch =
                (frames <= crate::cap::catch_frames("JumpCapCatch") as i32).then_some(frames);
        }
        // The pop's first step started the cap's catch; the cap's nerve runs it from this frame.
        if std::mem::take(&mut self.cap_catch_restart) {
            self.catching = None;
            self.pop_catch = Some(0);
        }
        if std::mem::take(&mut self.cap_put_on) {
            self.cap = None;
        }
        // A throw started last frame: the cap's throw start places it and
        // sets it flying; the throw nerve itself runs from the next frame.
        if let Some(launch) = self.launch.take() {
            self.winding = false;
            let cap = Cap::start_in(
                launch,
                self.pos + self.throwing.launch_offset * UNIT,
                self.cap_serial + 1,
                scene.mesh.as_deref(),
                Some(scene.water_areas()),
            );
            self.cap_serial = cap.clip_serial();
            self.cap = Some(cap);
            self.throwing.up = None;
            return;
        }
        let targets = self.lock_targets();
        let cap_hits = self.cap_hit_targets();
        let Some(cap) = &mut self.cap else {
            return;
        };
        if o.homing && cap.phase != CapPhase::Return && cap.kind != Throw::Spin {
            // A flick redirects the cap toward a nearby target or the stick.
            let target = scene
                .targets
                .iter()
                .filter(|t| t.distance(cap.pos) < 9.)
                .min_by(|a, b| {
                    a.distance_squared(cap.pos)
                        .total_cmp(&b.distance_squared(cap.pos))
                });
            let dir = target
                .map(|t| (*t - cap.pos).normalize_or_zero())
                .or_else(|| Vec3::new(o.direction.x, 0., o.direction.y).try_normalize())
                .unwrap_or(Vec3::ZERO);
            cap.redirect(dir);
        }
        let world = CapWorld {
            player: self.pos,
            hold,
            mesh: scene.mesh.as_deref(),
            water: Some(scene.water_areas()),
            targets: &targets,
            cap_hits: &cap_hits,
        };
        let moments = cap.update(&world);
        self.cap_serial = cap.clip_serial();
        for moment in moments {
            match moment {
                CapEvent::Thrown => {
                    self.events.push(Fx::Throw(cap.kind));
                    // The voice follows the throws in a row; the continuous
                    // one is counted 0 and sounds as the third.
                    let voice = if self.throwing.continuous {
                        3
                    } else {
                        self.throwing.count.clamp(1, 2)
                    };
                    self.events.push(Fx::ThrowVoice(voice));
                }
                CapEvent::LockedOn(index) => {
                    match self.cap_hangers.get_mut(index) {
                        Some(hanger) => hanger.start(),
                        None => self.cap_switches[index - self.cap_hangers.len()].start(),
                    }
                    self.events.push(Fx::CapLockOn);
                }
                CapEvent::LockReturn => self.events.push(Fx::CapLockReturn),
                CapEvent::Attacked(index) => {
                    self.trampolines[index].receive_cap_attack();
                    self.events.push(Fx::CapHit);
                    // MarioCapCommonInfo/ActorHitReactionCtrl.byml, 帽子ヒット:
                    // IsStopSceneForHitEffect with StopSceneFrame 2.
                    self.rocket.request_scene_stop(2);
                }
                CapEvent::Reflected(index) => {
                    self.signboards[index].reflect();
                    self.events.push(Fx::CapReflect);
                    // MarioCapCommonInfo/ActorHitReactionCtrl.byml, 帽子反射ヒット:
                    // IsStopSceneForHitEffect with StopSceneFrame 2.
                    self.rocket.request_scene_stop(2);
                }
                CapEvent::WallHit(strength) => self.events.push(Fx::CapWallHit(strength)),
                CapEvent::WallStopped(speed) => self.events.push(Fx::CapWallStop(speed)),
                CapEvent::TerrainTurn(degrees) => self.events.push(Fx::CapWallHit(degrees)),
                CapEvent::Staying => self.events.push(Fx::CapStay),
                CapEvent::WaterFly => self.events.push(Fx::CapWaterFly),
                CapEvent::WaterStay => self.events.push(Fx::CapWaterStay),
                CapEvent::WaterIn => self.events.push(Fx::CapWaterIn),
                CapEvent::WaterOut => self.events.push(Fx::CapWaterOut),
                CapEvent::StayStarted => self.events.push(Fx::CapStayStart),
                CapEvent::Trampled => self.events.push(Fx::CapBounce),
                CapEvent::LeftStay => self.events.push(Fx::CapReturn),
                CapEvent::Caught | CapEvent::Hidden => {
                    // A catch is receiveMsg's PlayerCapCatch accept (0x408b2c..0x408b34), which
                    // sets both flag bytes, and the player's `attackSensor` then restarts its
                    // after-catch counter (the trigger set at 0x428528, after `startCatch`
                    // 0x42859c). A hidden cap (the return ended away from the player,
                    // `hide(false)`, see `cap_return.rs`) is taken in the player's `movement`
                    // instead (`isNoPutOnHide` 0x41dd6c), by the same choice
                    // (`isEnableReactionCapCatch`, 0x41f8f4): `startCatch` sets the put-on byte
                    // only (0x3fcdc0) and no trigger, so the catch window and the byte
                    // `isCatched` reads stay as the throw left them.
                    let by_sensor = moment == CapEvent::Caught;
                    self.cap_water_left = cap.water_effects();
                    if by_sensor {
                        self.cap_catched = true;
                        self.catch_window = t::CAP_CATCH_WINDOW;
                    }
                    if !self.catch_reacts() {
                        // The cap goes on the head (`putOnCapPlayer`): no catch
                        // animation, hit reaction or effect. It is reported
                        // once more where this frame left it. `forcePutOn` sets the catch
                        // byte as well (0x3fce6c), even when the player skips the reaction.
                        self.cap_catched = true;
                        self.cap_put_on = true;
                        break;
                    }
                    self.cap = None;
                    self.catching = Some(-1);
                    self.events.push(Fx::Catch);
                    // Standing in the wait nerve, the catch plays its sub
                    // animation; otherwise it plays an upper-body one. Either
                    // starts before this frame's advance.
                    self.catch_clip = self.clip_of_catch();
                    if self.catch_clip == "CatchCap" {
                        // The skip after `startAction` belongs to a main clip that began under
                        // a sub animation; one that began without (the area idle after a
                        // throw) is not held by the sub the catch starts now.
                        if !self.sub.active {
                            self.anim_skip = false;
                        }
                        self.start_sub("CatchCap", 10., -1.);
                        // movement starts the partial too (0x41de54 -> 0x41deac).
                        // It survives an early endSubAnim when Run interrupts the catch.
                        self.upper_catch = Some(-1);
                    } else {
                        self.upper_catch = Some(-1);
                    }
                    break;
                }
            }
        }
    }
    /// Whether the catch shows (`PlayerActorHakoniwa::isEnableReactionCapCatch`,
    /// asked by its `attackSensor` after the cap's `Catch` sensor was taken):
    /// standing, running, spinning, falling, in the ground pound, the long jump,
    /// a plain jump or wall jump, and swimming play the catch; in any other
    /// state (squat, roll, dive, flips, spin jump, slide, ledge, pole) the cap
    /// is simply put on.
    fn catch_reacts(&self) -> bool {
        let label = self.nerve_name().name;
        let (actor, sub) = label.split_once('/').unwrap_or((label, ""));
        match actor {
            "Wait" | "Run" | "SpinCap" | "HipDrop" | "LongJump" | "Fall" | "Swim" => true,
            "Jump" => sub == "Jump",
            "WallAir" => sub == "Jump",
            _ => false,
        }
    }
    /// The catch clip of the player's state.
    fn clip_of_catch(&self) -> &'static str {
        if self.mode == Move::Idle && self.grounded {
            "CatchCap"
        } else if self.grounded {
            "CatchCapParts"
        } else {
            "CatchCapJumpParts"
        }
    }
    /// An upper-body animation was cleared in this step (a throw began).
    pub fn upper_cleared(&self) -> bool {
        self.upper_cleared
    }
    /// The cap went on the head without a catch in this frame (see `cap_put_on`).
    pub fn cap_put_on(&self) -> bool {
        self.cap_put_on
    }
    /// The upper-body catch animation while it plays.
    pub fn upper_catch(&self) -> Option<&'static str> {
        self.upper_catch.map(|_| self.catch_clip)
    }
    /// The partial animation (`partial0`) on screen: the catch's upper-body clip and the frame
    /// it shows, from the row that starts it (frame 0) to its last frame.
    pub fn partial_anim(&self) -> Option<(&'static str, f32)> {
        if self.carry.held {
            return self.carry.upper;
        }
        self.upper_catch
            .filter(|frame| *frame >= 0)
            .map(|frame| (self.catch_clip, frame as f32))
    }
    /// `PlayerJudgeCapCatchPop::judge`, asked by `tryChangeNerveFromAir` (see
    /// `cap_catch_pop.rs`): the pop is its own nerve. Without the press the catch leaves the jump
    /// alone (recorded: cap-catch-air). No recording presses jump in the catch window.
    fn try_cap_catch_pop(&mut self, o: &Orders) -> bool {
        if !self.mode.airborne_jump() || !self.judge_cap_catch_pop(o) {
            return false;
        }
        self.start_cap_catch_pop();
        true
    }
    /// The hit sensors' phase of the frame, after the cap moved and before the
    /// player does, with the player where the last frame left him: the foot
    /// sensor against the hovering cap's `Trample` sensor, then the body sensors
    /// against its `Catch` sensor (the touch jump, `cap_touch.rs`). The foot is
    /// asked first; the original answers the first pair its sensor director
    /// visits, whose order is not read. When the cap takes either the jump
    /// nerve restarts inside this frame's update, so the jump's first step is
    /// this frame's.
    fn cap_sensor_phase(&mut self, o: &Orders) {
        if self.cap.is_none() {
            return;
        }
        if self.foot_trample_phase(o) {
            return;
        }
        self.cap_touch_jump_phase(o);
    }
    /// `PlayerActorHakoniwa::attackSensor` for the foot sensor
    /// (`rs::sendMsgPlayerCapTrample`, 0x427f10), answered by `HackCap::receiveMsg` on its
    /// `Trample` sensor. Returns whether the cap took it.
    fn foot_trample_phase(&mut self, o: &Orders) -> bool {
        if !self.can_trample() {
            return false;
        }
        let Some(cap) = &mut self.cap else {
            return false;
        };
        let foot = self.pos / UNIT + Vec3::Y * crate::cap::FOOT_SENSOR_Y;
        let swimming = self.mode == Move::Swim;
        if !cap.take_trample(foot, o.throw_held, self.cap_jump_possible) {
            return false;
        }
        if swimming {
            // `attackSensor` 0x427f34: the accepted PlayerCapTrample raises the foot's hit
            // reaction and sets attack triggers 1 and 2 together. The swimmer reads them in its
            // own update (`exeSwimMove` and `exeSwimPaddle` test 2 only). The recordings keep
            // the cap-jump counter set through the bounce, so it is not spent here.
            self.swim.attack.hit = true;
            self.swim.attack.trample = true;
            self.own_cap_trample = true;
            self.events.push(Fx::CapTrampleHit);
            return true;
        }
        self.cap_jump_possible = false;
        self.trample_jump();
        // The nerve was named before this frame's update, which runs its
        // first step now: no frame passes in the chosen state.
        self.nerve.deferred = false;
        self.nerve.renewed = false;
        self.nerve.step = 0;
        self.nerve.actor = "Jump";
        self.sensor_restart = true;
        true
    }
    /// The actor nerve was restarted by a hit sensor in this frame and has run
    /// its first step already.
    pub fn restarted_by_sensor(&self) -> bool {
        self.sensor_restart
    }
    /// The states the player's foot can trample the cap in
    /// (`PlayerActorHakoniwa::attackSensor`): not on a pole,
    /// grabbing a ledge or ceiling, rolling, pounding, or spinning the cap in
    /// the air (`PlayerStateSpinCap::isSpinAttackAir`, 0x481e40: asked of the air child, whatever
    /// the floor under the actor is).
    fn can_trample(&self) -> bool {
        if self.mode == Move::Swim {
            return !matches!(
                self.swim.nerve,
                swim::SwimNerve::HipDropHeadSliding | swim::SwimNerve::HipDropHeadSlidingEnd
            );
        }
        !matches!(
            self.mode,
            Move::Pole
                | Move::Ledge
                | Move::GroundPound
                | Move::PoundLand
                | Move::Roll
                | Move::RollEnd
                | Move::RollJump
        ) && !(self.mode == Move::Throw && self.throwing.in_air)
    }
    /// The jump off the cap (`PlayerStateJump::appear`, request 11, shared
    /// with the jump on an enemy): `TrampleHighJumpPower` with
    /// `TrampleHighGravity`, no extension, the ordinary jump clip, and the
    /// player's speed kept when the stick is pushed, scaled down when he was
    /// still rising, and dropped when it is not.
    fn trample_jump(&mut self) {
        // The combo flag (+0xb4) is decided by `appear` for every request but 1, 13 and 16
        // (0x470e58..0x470fc4): request 11 takes the combo's path, as the ordinary jump does.
        self.pick_jump_kind();
        let clip = if self.side_faces_camera() {
            "Jump"
        } else {
            "JumpReverse"
        };
        // The rising test reads the velocity `appear` has converted (0x47116c, after 0x470c34).
        self.convert_velocity_on_just_land_jump(Move::CapBounce);
        let falling = self.v.y <= 0.;
        let pushing = self.tilt >= STICK_DEADZONE;
        self.enter_jump_state(
            Move::CapBounce,
            clip,
            t::TRAMPLE_HIGH_JUMP_POWER,
            self.constants.trample_high_gravity,
            move |p| {
                match (pushing, falling) {
                    (true, true) => {}
                    (true, false) => {
                        let h = p.v.with_y(0.) * t::TRAMPLE_RISING_BRAKE;
                        p.set_h(h);
                    }
                    (false, _) => p.set_h(Vec3::ZERO),
                }
                p.max_air_speed = p.speed_h().clamp(t::JUMP_MOVE_MIN, t::JUMP_MOVE_MAX);
                p.jump_extend = false;
                p.dive_possible = true;
            },
        );
    }

    // ---- Landing and collision ------------------------------------------------
    fn land(&mut self, o: &Orders, scene: &Course) {
        if self.model_2d()
            && matches!(
                self.mode,
                Move::Fall | Move::Jump | Move::DoubleJump | Move::TripleJump
            )
        {
            // Wait::appear 0x48c93c..0x48c968 skips Land for a 2D model.
            // startHitReactionLandIfLanding 0x56a3ec also rejects that model.
            self.v.y = self.touchdown_speed;
            self.jump_air(o);
            if self.actor_name() == "Jump" {
                self.count_jump_combo();
            }
            self.air_for = None;
            self.land_frames = 0;
            self.set_nerve_on_ground(o, scene);
            return;
        }
        if self.mode == Move::Fall && !self.is_just_land() {
            // A continuing floor contact enters Wait directly; only a fresh
            // landing chooses the Land child (PlayerStateWait::appear).
            self.jump_air(o);
            self.set_nerve_on_ground(o, scene); // exeFall 0x425410.
            return;
        }
        if self.mode == Move::WallSlide {
            self.leave_wall_slide();
        }
        if self.mode == Move::Fall && self.air_for != Some(Move::Fall) {
            // Fall's first execution still sets up its air control when the
            // preceding collision already found ground (0x46b3ec..0x46b4a0).
            // The original actor retains its incoming vertical velocity.
            self.v.y = self.touchdown_speed;
            self.arm_fall_control();
        }
        // The child still updates on touchdown: exeJump 0x4240a4 -> air update 0x4729c4
        // and landing limit 0x472a1c..0x472a64; exeFall 0x425240 -> 0x46b4a0. This
        // precedes forced-slide HoldCancel (0x4241c4 / 0x42532c), which reads its velocity.
        let air_step_ran = (self.mode.uses_air_control() || self.mode == Move::CapCatchPop)
            && self.air_for == Some(self.mode);
        let mut airborne_velocity = None;
        if air_step_ran {
            let mut v = self.v.with_y(self.touchdown_speed);
            let steer = air_move::Steering {
                front: t::JUMP_ACCEL_FRONT,
                back: t::JUMP_ACCEL_BACK,
                turn: t::JUMP_ACCEL_TURN,
            };
            let push = self.push();
            self.air.update(&mut v, push, o.jump_held, &steer);
            if self.actor_name() == "Jump" {
                self.air.limit_landing_speed(&mut v);
            }
            self.set_h(v);
            airborne_velocity = Some(v);
            // (`exePop` only updates the air control; the jump state turns the actor itself.)
            if self.mode != Move::CapCatchPop {
                self.turn_in_air(push);
            }
        }
        // Forced judges and appear read the stored actor velocity after the air child, as does
        // the seed's synchronous response (0x29a734..0x29a758 -> 0x56fa70..0x56fa80).
        // Cache that full velocity too: units_per_frame must not reconstruct floor-normal pull
        // if its Y happens to equal the ground representation's zero or -pull.
        let before = self.v;
        let ground_move = self.ground_move;
        if let Some(velocity) = airborne_velocity {
            self.v = velocity;
            self.ground_move = Some((Vec2::new(velocity.x, velocity.z), self.pull, velocity));
        }
        // Slide retains priority over rolling and the remaining landing decisions.
        let forced_slide = self.try_force_slope_slide(scene);
        if !forced_slide && airborne_velocity.is_none() {
            self.v.y = self.touchdown_speed;
        }
        let forced = forced_slide || self.try_force_rolling();
        self.ground_move = ground_move;
        if forced {
            return;
        }
        self.v = before;
        if self.actor_name() == "Jump" {
            self.count_jump_combo();
        }
        self.air_for = None;
        // The last step of a dive or a long jump ran before the landing was
        // seen. The roll that follows (`PlayerJudgeDirectRolling`, the roll
        // jump's own landing) judges and keeps the velocity that step left,
        // gravity included; the states that follow otherwise set their own.
        let flown = (self.v, self.facing);
        let arrival = self.velocity_when_landing_state_is_chosen(flown.0, air_step_ran);
        self.note_velocity_at_run_choice(arrival);
        if o.crouch {
            self.v.y = self.touchdown_speed;
            match self.mode {
                Move::Dive => self.head_sliding_motion(),
                Move::LongJump | Move::RollJump => self.long_jump_motion(),
                _ => {}
            }
        }
        let rolls = o.crouch
            && match self.mode {
                Move::Dive | Move::LongJump => self.speed_h() >= t::DASH_JUDGE,
                Move::RollJump => self.roll_goes_on_after_jump(o.crouch),
                _ => false,
            };
        if !rolls {
            (self.v, self.facing) = flown;
        }
        self.land_frames = 0;
        self.dive_possible = true;
        if self.mode == Move::Dive {
            // `exeHeadSliding` clears the cap jump limit on every step it has
            // the ground (`clearLandLimitStandAngle`), the landing step included.
            self.restore_cap_actions();
        }
        self.gravity = self.constants.gravity_air;
        self.hard_land = self.fall.distance >= t::LONG_FALL
            && !matches!(
                self.mode,
                Move::GroundPound | Move::Dive | Move::BurnJump | Move::BurnBounce | Move::FireRun
            );
        let mode = self.mode;
        if mode == Move::RollJump && !rolls {
            self.events.push(Fx::RollJumpLand);
        }
        match mode {
            // The pound's landing is a sub-state of its nerve: the impact and
            // the landing clip are its first step (the game's logic does not
            // freeze there).
            Move::GroundPound => {
                // Execute the last Fall pull with the incoming velocity that
                // collision bookkeeping saved (the original collider retains it).
                self.v.y = (self.touchdown_speed - self.constants.hip_drop_gravity).max(-45.);
                self.defer(Move::PoundLand, "HipDropLand", |p| {
                    p.events.push(Fx::PoundLand);
                    // `PlayerActorHakoniwa::exeHipDrop`, on the landing, wets the body that
                    // came down in a puddle.
                    if p.material_puddle {
                        p.wet.record_heavy_land_puddle();
                    }
                });
                return;
            }
            // Holding crouch turns the momentum into a real roll; otherwise a
            // dive lands in its forward roll and stands up.
            Move::Dive | Move::LongJump if rolls => self.start_roll(RollStart::Landing),
            Move::RollJump if rolls => self.roll_land_from_jump(),
            // The roll raises its own landing reaction when it has flown long
            // enough (`PlayerStateRolling::control`).
            Move::Roll => return,
            Move::SlopeSlide => {}
            Move::BurnJump
            | Move::BurnBounce
            | Move::FireRun
            | Move::FireFall
            | Move::FireDead
            | Move::Dead => return,
            _ => {
                // `PlayerActorHakoniwa::setNerveOnGround` asks its judges in
                // this order: the buffered jump, `PlayerJudgeStartRun` (which
                // the force-land judge vetoes), `PlayerJudgeStartSquat` (also
                // vetoed), then the wait state. So a stick held at touchdown
                // runs first and the run state starts the crouch one step
                // later.
                use on_ground::GroundNerve;
                let choice = self.ground_nerve(o, scene);
                if matches!(choice, GroundNerve::Changed) {
                    return;
                }
                if matches!(choice, GroundNerve::Jump) {
                    self.ground_jump(o);
                } else if matches!(choice, GroundNerve::Run) {
                    self.run_frames = t::BRAKE_ON_COUNTER;
                    self.pull = GRAVITY_MOVE;
                    self.run_appear(true);
                    self.go(Move::Run, "Move");
                    if mode == Move::Dive {
                        self.cycle_restart_in = 2;
                        self.sub_anim = Some(Anim {
                            name: "RollingStandUp",
                            serial: self.anim.serial.wrapping_add(7),
                            frame: 2.,
                            rate: 0.6,
                        });
                    }
                } else if matches!(choice, GroundNerve::Squat) {
                    self.start_squat();
                } else {
                    self.appear_ground_wait(arrival);
                    return;
                }
            }
        }
        // Rolling::appear already raised its landing reaction.
        if !matches!(mode, Move::Dive | Move::LongJump) || !rolls {
            self.events.push(Fx::Land {
                hard: self.hard_land,
            });
        }
    }
    /// PlayerStateWait::appear (0x48c824..0x48c968): ForceLand before
    /// JustLand; shared consumers retain the established Land entry/pose.
    pub(super) fn appear_ground_wait(&mut self, arrival: Vec3) {
        if self.mode == Move::SpinDrop {
            self.go(Move::Idle, "LandDownFall");
            return;
        }
        if self.force_land_on_ground() {
            self.change(Move::Idle, "LandStiffen", |p| {
                p.set_h(Vec3::ZERO);
                p.events.push(Fx::Land { hard: true });
            });
            return;
        }
        // The landing clip depends on the clip the jump was playing
        // (`PlayerStateWait::exeLand`); landing stops the player.
        let anim = match self.clip() {
            "JumpTurn" => "LandTurn",
            "JumpBroad" => "LandRolling",
            "Jump3" => "LandJump3",
            _ => "Land",
        };
        // (The triple jump's landing gives the body no sway.)
        if anim != "LandJump3" {
            self.wait_state_appears_after_landing(arrival);
        }
        // Wait::appear (0x48c824) sets the arrival's horizontal
        // velocity along the new floor before adding Gravity.
        // Keep its horizontal part in the ground-state storage;
        // ground_following supplies the normal pull.
        let up = self.ground_normal_or_up();
        if up != Vec3::Y {
            // The slope Land path keeps full velocity until its
            // snap step. Flat Land retains its existing stop.
            let along = along_vector_normal_h(arrival, Vec3::Y, up);
            self.set_h(along);
            self.v.y = 0.;
            self.pull = t::GRAVITY;
        }
        self.change(Move::Idle, anim, |p| {
            if p.grounded {
                p.set_h(Vec3::ZERO);
            }
        });
        self.events.push(Fx::Land { hard: false });
    }
    fn collide(&mut self, o: &Orders, scene: &Course) {
        // DeadFire disables movement collision; retain the last contact report.
        if matches!(
            self.mode,
            Move::AbyssFall | Move::RecoveryStart | Move::RecoveryTravel | Move::SandGeyser
        ) {
            if self.mode == Move::SandGeyser {
                // updateCollider's no-collision branch resets at the pre-integration pose.
                // The next enabled collision sweeps from that pose before its own velocity.
                self.collider.invalidate(self.facing.x.atan2(self.facing.z));
                self.trans_carry = Some(self.position_cm());
            }
            self.move_without_collision(scene);
            return;
        }
        if self.is_dead() {
            return;
        }
        let old = self.pos;
        let was_grounded = self.grounded;
        self.grounded = false;
        // The collider's contact report lives until the next collision: a body that took off
        // keeps `isCollidedGround` for the judges of the next step.
        self.hop_contact = false;
        self.landing_speed = 0.;
        self.wall = Vec3::ZERO;
        self.touched = terrain::Touched::default();
        match &scene.mesh {
            Some(mesh) => self.collide_mesh(mesh, was_grounded),
            None => self.collide_solids(o, scene),
        }
        if scene.mesh.is_none() {
            self.collide_toy_poles(scene);
            if self.grounded
                && scene.needles.iter().any(|(c, h)| {
                    (self.pos.x - c.x).abs() < h.x
                        && (self.pos.z - c.z).abs() < h.z
                        && self.pos.y == c.y
                })
            {
                self.touched.active = true;
                self.touched.floor = Some(CollisionCodes {
                    floor: "Needle".into(),
                    ..default()
                });
            }
            self.mark_toy_sand_floor(scene);
        }
        if scene.mesh.is_none()
            && self.mode == Move::WallSlide
            && (self.wall == Vec3::ZERO || self.grounded)
            && !self.grounded
        {
            // The toy course's rule: lost the wall, keep falling. On a stage mesh the wall slide
            // ends in its own step (`PlayerStateNormalWallSlide::followNormal` reads the contact
            // the last move left, so the state holds one more step after the wall is gone).
            self.go(Move::Fall, "Fall");
        }
        // Stay glued to ramps and steps down (the box solids' own rule; the
        // mesh's ground is held by the collider's arrows alone).
        if scene.mesh.is_none()
            && !self.grounded
            && was_grounded
            && self.v.y <= 0.
            && let Some(ground) = scene.ground(self.pos)
            && self.pos.y - ground < 0.3
        {
            self.pos.y = ground;
            self.grounded = true;
        }
        // `PlayerCollider::collide` ends by counting the steps in a row with ground contact.
        self.ground_contact = if self.is_collided_ground() {
            self.ground_contact.saturating_add(1)
        } else {
            0
        };
        self.distance += (self.pos - old).with_y(0.).length();
    }
    /// The player's contact with the course's box solids (the windowed demo).
    fn collide_solids(&mut self, o: &Orders, scene: &Course) {
        let crouched = matches!(
            self.mode,
            Move::SquatBrake | Move::Crouch | Move::Crawl | Move::Roll | Move::RollEnd | Move::Dive
        ) || self.grounded && o.crouch;
        let height = if crouched { SQUAT_HEIGHT } else { HEIGHT };
        // How far past the surface (negative: short of it) the body must be.
        let contact_reach = if crouched {
            SURFACE_GRID
        } else {
            -SURFACE_GRID
        };
        let motion = self.v * UNIT;
        // Sub-steps keep each move shorter than the collider radius.
        let steps = (motion.length() / (RADIUS * 0.8)).ceil().max(1.) as usize;
        for _ in 0..steps {
            let step = motion / steps as f32;
            for axis in [0, 2, 1] {
                let before = self.pos;
                self.pos[axis] += step[axis];
                for b in &scene.solids {
                    let lo = b.center - b.half;
                    let hi = b.center + b.half;
                    if self.pos.x + RADIUS <= lo.x
                        || self.pos.x - RADIUS >= hi.x
                        || self.pos.z + RADIUS <= lo.z
                        || self.pos.z - RADIUS >= hi.z
                    {
                        continue;
                    }
                    // (A solid under the toy course's sand lets the body sink into it. Inside the
                    // sand only the patch's edge holds a sunk body back, as the pit's walls do;
                    // a shallow one steps out over the edge like over any short step.)
                    let nominal = b.top(self.pos.x.clamp(lo.x, hi.x));
                    let sand_here = Self::toy_sand_surface_at(scene, before)
                        .is_some_and(|y| (y - nominal).abs() < 1e-3)
                        && self.sand.depth > 0.;
                    if axis != 1 && sand_here {
                        let left = Self::toy_sand_surface_at(scene, self.pos).is_none();
                        if left && self.sand.depth * UNIT > WALKABLE_STEP {
                            self.pos[axis] = before[axis];
                            self.v[axis] = 0.;
                        }
                        if !left || self.sand.depth * UNIT > WALKABLE_STEP {
                            continue;
                        }
                    }
                    let sink = self.toy_sand_sink(scene, nominal);
                    let top = b.top(self.pos.x.clamp(lo.x, hi.x)) - sink;
                    if axis == 1 {
                        if self.v.y <= 0.
                            && before.y >= b.top(before.x.clamp(lo.x, hi.x)) - sink - 0.15
                            && self.pos.y < top + contact_reach
                            && self.pos.y + height > lo.y
                        {
                            self.pos.y = top;
                            self.landing_speed = self.landing_speed.min(self.v.y);
                            if self.mode != Move::SlopeSlide && !self.in_damage_state() {
                                // (The slope slide keeps the game's own velocity, the pull into
                                // the floor included.)
                                self.v.y = 0.;
                            }
                            self.grounded = true;
                            self.touched.active = true;
                            self.touched.floor = Some(CollisionCodes {
                                floor: b.floor_code.into(),
                                floor_index: if b.floor_code == "Skate" { 22 } else { 8 },
                                ..default()
                            });
                            self.slope = if (self.pos.x - b.center.x).abs() < b.half.x {
                                b.slope
                            } else {
                                0.
                            };
                        } else if self.v.y > 0.
                            && before.y + height <= lo.y + 0.1
                            && self.pos.y + height >= lo.y
                        {
                            self.pos.y = lo.y - height;
                            self.v.y = 0.;
                            // A hit head ends the jump's extend window.
                            self.air.cancel_extend();
                        }
                    } else if self.pos.y + 0.12 < top && self.pos.y + height > lo.y + 0.05 {
                        // Short steps are walkable.
                        if before.y >= top - WALKABLE_STEP
                            && self.v.y <= 0.
                            && !matches!(self.mode, Move::GroundPound | Move::Dive)
                        {
                            self.pos.y = top;
                            self.v.y = 0.;
                            self.grounded = true;
                            continue;
                        }
                        let sign = if before[axis] < b.center[axis] {
                            -1.
                        } else {
                            1.
                        };
                        self.pos[axis] = b.center[axis] + sign * (b.half[axis] + RADIUS);
                        let into = -self.v[axis] * sign;
                        self.wall = Vec3::ZERO;
                        self.wall[axis] = sign;
                        // (The toy course's box has a flat face and a plain
                        // contact point, at the height of the body sphere like
                        // the collider's; the keep-the-velocity rule of the
                        // mesh path does not apply to it.)
                        self.touched.wall_contact = Some(WallContact {
                            normal: self.wall,
                            position: (self.pos - self.wall * RADIUS + Vec3::Y * 0.65) / UNIT,
                            at_face: true,
                        });
                        self.v[axis] = 0.;
                        if self.mode == Move::Roll && into > 12. {
                            // Rolling into a wall knocks the roll back out.
                            self.facing = (self.facing
                                - self.wall * 2. * self.facing.dot(self.wall))
                            .normalize_or(self.wall);
                            self.set_h(self.facing * 6.);
                            self.set(Move::RollEnd);
                            self.play("RollingStandUp");
                        }
                    }
                }
            }
        }
    }
}

#[path = "tests/cap_launch.rs"]
mod cap_launch_checks;
#[path = "tests/cap_touch.rs"]
mod cap_touch_checks;
#[path = "tests/carry_release.rs"]
mod carry_release_checks;
#[path = "tests/movement.rs"]
mod checks;
#[path = "tests/ledge.rs"]
mod ledge_checks;
#[path = "tests/spin_carry.rs"]
mod spin_carry_checks;
#[path = "tests/spin.rs"]
mod spin_checks;
#[path = "tests/wall_air.rs"]
mod wall_air_checks;

pub fn self_check() {
    dokan::self_check();
    dimension::self_check();
    cap_trampoline::self_check();
    spin_flower::self_check();
    sand_geyser::self_check();
    recovery_dead::self_check();
    stain::self_check();
    checks::run();
    cap_launch_checks::run();
    carry_release_checks::run();
    cap_touch_checks::run();
    spin_carry_checks::run();
    spin_checks::run();
    wall_air_checks::run();
    ledge_checks::run();
    console_libm::self_check();
    turn_control::self_check();
    ground_move::self_check();
    move_input::self_check();
    input_2d::self_check();
    wall_along::self_check();
    wall_slide::self_check();
    pose_math::self_check();
    look_joint::self_check();
    ground_pose::self_check();
    swim_pose::self_check();
    swim_move::self_check();
    pole_pose::self_check();
    grab_ceil::self_check();
    cap_throw::self_check();
    hip_spring::self_check();
    look_at::self_check();
    damage::self_check();
    key_move_parts::self_check();
    killer::self_check();
    ground_inertia::self_check();
    joint_controls::self_check();
    external_velocity::self_check();
    air_current::self_check();
    rotate_parts::self_check();
    conveyor_parts::self_check();
    press::self_check();
    force_run::self_check();
    rocket_flower::self_check();
    surface_run::self_check();
}
