//! Player controller modelled on the original game's player states.
//!
//! It runs at the original's 60 frames per second in its distance unit (one
//! unit is a centimetre; Mario is 160 tall) and its tuning values. Positions
//! are stored in metres for the rest of the game. States that the public
//! decompilation documents follow it; the rest are inferred from their tuning
//! names and observed behaviour. No original code is reproduced here.
use crate::air_move::{self, AirMove, ContinuousJump, ContinuousLongJump};
use crate::cap::{CapEvent, CapWorld, Launch};
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
    pub const JUMP_GRAVITY: f32 = 1.5;
    pub const JUMP_MOVE_MIN: f32 = 11.;
    pub const JUMP_MOVE_MAX: f32 = 30.;
    pub const JUMP_ACCEL_FRONT: f32 = 0.5;
    pub const JUMP_ACCEL_BACK: f32 = 1.;
    pub const JUMP_TURN_LIMIT_DEG: f32 = 6.;
    pub const JUMP_TURN_FAST_DEG: f32 = 135.;
    pub const JUMP_TURN_FAST_LIMIT_DEG: f32 = 25.;
    pub const PRE_INPUT_JUMP: u32 = 5;
    pub const CONTINUOUS_JUMP_TIMER: u32 = 10;
    pub const CONTINUOUS_JUMP_POWER_MIN: f32 = 19.5;
    pub const JUMP_POWER_MAX_2ND: f32 = 21.;
    pub const JUMP_GRAVITY_2ND: f32 = 1.5;
    pub const JUMP_POWER_3RD: f32 = 25.;
    pub const JUMP_GRAVITY_3RD: f32 = 1.;
    pub const GRAVITY_AIR: f32 = 1.5;
    pub const FRICTION_AIR: f32 = 0.99;
    pub const FALL_MAX: f32 = 35.;
    pub const LONG_FALL: f32 = 3000.;

    pub const SQUAT_JUMP_POWER: f32 = 32.;
    pub const SQUAT_JUMP_GRAVITY: f32 = 1.;
    pub const SQUAT_JUMP_BACK: f32 = 5.;
    pub const SQUAT_JUMP_MOVE_MAX: f32 = 9.;
    pub const TURN_JUMP_POWER: f32 = 32.;
    pub const TURN_JUMP_GRAVITY: f32 = 1.;
    pub const TURN_JUMP_VEL_H: f32 = 9.;

    pub const LONG_JUMP_ACCEL: f32 = 0.25;
    pub const LONG_JUMP_BRAKE: f32 = 0.5;
    pub const LONG_JUMP_SIDE_ACCEL: f32 = 0.25;
    pub const LONG_JUMP_GRAVITY: f32 = 0.48;
    pub const LONG_JUMP_POWER: f32 = 12.;
    pub const LONG_JUMP_MOVE_POW: f32 = 4.;
    pub const LONG_JUMP_INIT_SPEED: f32 = 14.;
    pub const LONG_JUMP_SPEED: f32 = 23.;
    pub const LONG_JUMP_SPEED_MIN: f32 = 2.5;
    pub const CONTINUOUS_LONG_JUMP_TIMER: u32 = 15;

    pub const SPIN_JUMP_POWER: f32 = 20.;
    pub const SPIN_JUMP_GRAVITY: f32 = 0.4;
    pub const SPIN_JUMP_MOVE_MAX: f32 = 8.;
    pub const SPIN_DOWN_FALL_INIT: f32 = 35.;
    pub const SPIN_DOWN_FALL_POWER: f32 = 1.5;
    pub const SPIN_DOWN_FALL_MAX: f32 = 45.;
    pub const SPIN_DOWN_FALL_FRAME_MIN: u32 = 5;
    pub const GROUND_SPIN_FRAMES: u32 = 90;
    pub const GROUND_SPIN_MOVE_MAX: f32 = 8.;
    pub const GROUND_SPIN_ACCEL_RATE: f32 = 0.5;
    pub const GROUND_SPIN_BRAKE_RATE: f32 = 0.95;

    pub const HIP_DROP_SPEED: f32 = 45.;
    pub const HIP_DROP_LAND_CANCEL: u32 = 24;
    pub const HIP_DROP_JUMP_BEGIN: u32 = 5;
    pub const HIP_DROP_JUMP_END: u32 = 30;
    pub const HIP_DROP_JUMP_POWER: f32 = 40.;

    pub const HEAD_SLIDING_SPEED: f32 = 20.;
    pub const HEAD_SLIDING_SPEED_MIN: f32 = 2.5;
    pub const HEAD_SLIDING_BRAKE: f32 = 0.5;
    pub const HEAD_SLIDING_SIDE_ACCEL: f32 = 0.125;
    pub const HEAD_SLIDING_JUMP: f32 = 28.;
    pub const HEAD_SLIDING_GRAVITY: f32 = 2.;

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

    pub const DAMAGE_FIRE_JUMP_POWER_1ST: f32 = 20.;
    pub const DAMAGE_FIRE_JUMP_POWER_2ND: f32 = 20.;
    pub const DAMAGE_FIRE_JUMP_MOVE: f32 = 10.;
    pub const DAMAGE_FIRE_GRAVITY: f32 = 0.9;
    pub const DAMAGE_FIRE_NO_GRAVITY_FRAMES: u32 = 20;
    pub const DAMAGE_FIRE_RUN_TIME: u32 = 60;
    pub const DAMAGE_FIRE_RUN_SPEED: f32 = 18.;
    pub const DAMAGE_FIRE_RUN_BRAKE_FRAMES: f32 = 20.;
    pub const DAMAGE_FIRE_RUN_ANIM_RATE: f32 = 0.3;
    // Soot after a burn, from the original stain control's fire entry.
    pub const SOOT_GAIN: f32 = 0.005;
    pub const SOOT_HOLD_FRAMES: u32 = 300;
    pub const SOOT_FADE_FRAMES: f32 = 300.;
    pub const SOOT_SMOKE_ON: f32 = 0.75;
    pub const SOOT_SMOKE_OFF: f32 = 0.25;
    pub const SOOT_WASH: f32 = 0.98;
    /// Mario stays wet this long after leaving deep water.
    pub const WET_FRAMES: u32 = 300;
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
    pub const WALL_SLIDE_GRAVITY: f32 = 0.5;
    pub const WALL_KEEP_FRAMES: u32 = 3;
    /// `FallWallScaleVelocity`.
    pub const FALL_WALL_SCALE_VELOCITY: f32 = 0.5;
    pub const WALL_JUMP_POWER: f32 = 23.;
    pub const WALL_JUMP_GRAVITY: f32 = 0.95;
    pub const WALL_JUMP_H: f32 = 8.6;
    pub const WALL_JUMP_NO_INPUT: u32 = 25;

    /// The jump off a trampled cap (`PlayerStateJump::appear`, request 11):
    /// `TrampleHighJumpPower` and `TrampleHighGravity`.
    pub const TRAMPLE_HIGH_JUMP_POWER: f32 = 25.;
    pub const TRAMPLE_HIGH_GRAVITY: f32 = 1.;
    /// Share of the horizontal speed a rising player keeps when he tramples
    /// with the stick pushed (`TrampleRisingBrakeVelH`).
    pub const TRAMPLE_RISING_BRAKE: f32 = 0.3;
    pub const CAP_CATCH_POP_POWER: f32 = 10.;
    pub const CAP_CATCH_POP_GRAVITY: f32 = 0.8;
    pub const CAP_CATCH_JUMP_POWER: f32 = 22.;
    pub const CAP_CATCH_JUMP_GRAVITY: f32 = 1.3;
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
}

/// The angle a thrown button gives the spine's swing (`PlayerSpinCapAttack` `+0x58`): only the
/// left/right throw gesture sets one, and the rewrite's input has none.
const BUTTON_THROW_SWING_ANGLE: f32 = 0.;

#[path = "movement/cap_pose.rs"]
pub(crate) mod cap_pose;
#[path = "movement/cap_throw.rs"]
pub(crate) mod cap_throw;
/// What a pose calculation advances (the controllers that keep state between frames).
pub type ControllerState = (
    ground_pose::GroundPose,
    look_at::LookAt,
    hip_spring::HipSpring,
);
#[path = "movement/console_libm.rs"]
pub(crate) mod console_libm;
#[path = "movement/ground.rs"]
mod ground;
#[path = "movement/ground_move.rs"]
pub(crate) mod ground_move;
#[path = "movement/ground_pose.rs"]
pub(crate) mod ground_pose;
#[path = "movement/hip_spring.rs"]
pub(crate) mod hip_spring;
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
#[path = "movement/run_cycle.rs"]
mod run_cycle;
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
#[path = "movement/squat.rs"]
mod squat;
#[path = "movement/swim.rs"]
mod swim;
#[path = "movement/swim_const.rs"]
mod swim_const;
#[path = "movement/swim_dive.rs"]
mod swim_dive;
#[path = "movement/swim_ground.rs"]
mod swim_ground;
#[path = "movement/swim_move.rs"]
mod swim_move;
#[path = "movement/swim_pose.rs"]
pub(crate) mod swim_pose;
#[path = "movement/terrain.rs"]
mod terrain;
#[path = "movement/turn_control.rs"]
pub(crate) mod turn_control;
#[path = "movement/vector_lib.rs"]
mod vector_lib;
#[path = "movement/wait.rs"]
mod wait;
#[path = "movement/wall_air.rs"]
mod wall_air;
#[path = "movement/wall_along.rs"]
mod wall_along;
#[path = "movement/wall_slide.rs"]
pub(crate) mod wall_slide;
#[path = "movement/wet.rs"]
mod wet;
pub use ground::turn_power;
use ground::{GRAVITY_MOVE, GroundMove, PivotTurn, STICK_DEADZONE, SteerInput};
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
fn throw_clip(kind: Throw, air: bool, count: u32, continuous: bool) -> &'static str {
    match (kind, air, count, continuous) {
        (Throw::Up, false, _, _) => "SpinCapStartDoubleUp",
        (Throw::Up, true, _, _) => "SpinCapAirStartDoubleUp",
        (Throw::Down, false, _, _) => "SpinCapStartDoubleDown",
        (Throw::Down, true, _, _) => "SpinCapAirStartDoubleDown",
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
            "SquatStart" => Some(3),
            // (The fall's clip is 28 frames long and its counter stops one past that.)
            "Fall" => Some(28),
            "Jump" | "JumpReverse" => Some(84),
            "Jump2" => Some(42),
            "Jump3" | "JumpBack" => Some(68),
            "JumpTurn" => Some(60),
            "JumpBroad" | "JumpBroad2" | "JumpBroad3" => Some(38),
            "JumpHipDrop" => Some(46),
            _ => None,
        }
    }
    /// Length of the clips that loop (their frame wraps).
    pub fn loop_length(name: &str) -> Option<f32> {
        match name {
            "Wait" | "Snooze" => Some(180.),
            "WaitRelax" => Some(1200.),
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
            // The swim clips (recorded `anim_len`).
            "SwimStandSurface" => Some(46.),
            "SwimSurfaceNormal" => Some(38.),
            "SwimStandWait" => Some(140.),
            "SwimWalk" => Some(68.),
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
            "SwimStandSurface" | "SwimSurfaceNormal" | "SwimStandWait" | "SwimWalk" => 1.,
            _ => 0.,
        }
    }
    /// The swim's clips (recorded `anim_len`).
    pub const SWIM_STAND: u32 = 32;
    pub const SWIM_LAND: u32 = 45;
    pub const SWIM_HIP_DROP_START: u32 = 30;
    pub const SWIM_HIP_DROP_LAND: u32 = 74;
    pub const LAND: u32 = 26;
    /// The landing after a long fall (`PlayerStateWait`'s `LandStiffen` nerve).
    pub const LAND_STIFFEN: u32 = 120;
    pub const LAND_JUMP3: u32 = 70;
    pub const SQUAT_END: u32 = 13;
    pub const TURN: u32 = 12;
    pub const RUN_START: u32 = 30;
    pub const BRAKE: u32 = 5;
    pub const JUMP_CAP_LEAP_FROG: u32 = 36;
    /// The fire run loop; its footsteps repeat every cycle.
    pub const FIRE_RUN: f32 = 60.;
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
    WallSlide,
    WallJump,
    Ledge,
    Pole,
    Swim,
    CapBounce,
    CatchJump,
    Throw,
}
impl Move {
    /// Every state, for mapping names back to states.
    pub const ALL: [Move; 37] = [
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
        Self::WallSlide,
        Self::WallJump,
        Self::Ledge,
        Self::Pole,
        Self::Swim,
        Self::CapBounce,
        Self::CatchJump,
        Self::Throw,
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
            Self::WallSlide => "Wall slide",
            Self::WallJump => "Wall jump",
            Self::Ledge => "Ledge hang",
            Self::Pole => "Pole climb",
            Self::Swim => "Swim",
            Self::CapBounce => "Cap jump",
            Self::CatchJump => "Catch jump",
            Self::Throw => "Cap throw",
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
    /// The state is the roll's brake (its clip is the unrolling clip's).
    pub roll_brake: bool,
    /// The state is the spin cap state's fall.
    pub spin_fall: bool,
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
/// The run's hidden state as far as a recorded row shows it: the pose's place
/// in the cycle, its weights, and the turn in progress.
#[derive(Clone, Debug, Default)]
pub struct RunHint {
    pub phase: f32,
    pub rate: f32,
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
    /// The cap turned along a wall; the number is how square the hit was.
    CapWallHit(f32),
    Catch,
    /// The cap was trampled: its first step in the trample nerve.
    CapBounce,
    /// The stay began while the button is held: the reaction it raises.
    CapStayStart,
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
    /// The swim stroke's clip began or looped (`PlayerSeCtrl::update`).
    SwimStrokeSound {
        camera_under: bool,
    },
    /// Hot ground: the leap (false) or the bounce where it lands (true).
    Burned(bool),
    /// Water put the burn out.
    Quenched,
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
/// Soot left by a burn, as the original's stain control: it builds up while
/// burning, stays for five seconds, then fades over five more. Water washes
/// it off quickly, and keeps doing so while Mario drips afterwards.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Soot {
    /// Stain strength on the body and on the worn cap, 0 to 1.
    pub body: f32,
    pub cap: f32,
    /// Smoke wisps rise from a strong stain until it fades or gets wet.
    pub smoking: bool,
    hold: u32,
    wet: u32,
    washing: bool,
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
    /// `exeSpinGround`): the ground judges never ask about it.
    in_air: bool,
    /// The direction the cap leaves along, fixed on the state's first step.
    dir: Option<Vec3>,
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

#[derive(Debug)]
struct FireControl {
    frames: u32,
    direction: Vec3,
    extend: u32,
    brake: u32,
}
impl Default for FireControl {
    fn default() -> Self {
        Self {
            frames: 0,
            direction: Vec3::Z,
            extend: 0,
            brake: 0,
        }
    }
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
    /// State changes decided last step, waiting for their first step.
    nerve: Nerve,
    fire: FireControl,
    /// The pole climb in progress (`PlayerStatePoleClimb`).
    pole: PoleControl,
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
    pub wall: Vec3,
    pub hitstop: f32,
    pub events: Vec<Fx>,
    pub anim: Anim,
    /// Upper-body overlay clip while throwing on the run.
    pub sub_anim: Option<Anim>,
    pub soot: Soot,
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
    after_turn: u32,
    quick_turn: u32,
    catch_window: u32,
    cap_jump_possible: bool,
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
    slope: f32,
    slick: bool,
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
    /// The frame (base matrix, inverse) of the collision part the last collision found ground on,
    /// when the stage's export has it: `rs::waitGround` carries the pose along it.
    ground_frame: Option<([f32; 12], [f32; 12])>,
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
    /// The sounds the player's code starts itself (footsteps, roll, fall).
    sounds: SeCtrl,
    /// The collider still reports the floor under a hop that leaves it (the
    /// roll's start), though the player is already rising.
    hop_contact: bool,
    /// The real game's player collider, used when the course is a triangle mesh.
    collider: PlayerCollider,
    /// The normal of the mesh floor the collider found this step.
    mesh_ground: Option<Vec3>,
    /// The collision codes of the mesh surfaces the body touched this step.
    pub touched: terrain::Touched,
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
        Self {
            nerve,
            fire: FireControl::default(),
            pole: PoleControl::default(),
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
            air_for: None,
            jump_power: 0.,
            combo_kept: false,
            prior_v: Vec3::ZERO,
            takeoff_h: None,
            ground_height: f32::MAX,
            camera_yaw: 0.,
            jump_held_frames: 0,
            run_frames: 0,
            after_turn: 0,
            quick_turn: 0,
            catch_window: 0,
            cap_jump_possible: true,
            dive_possible: true,
            gravity: t::GRAVITY_AIR,
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
            slope: 0.,
            slick: false,
            catch: WallCatchControl::default(),
            catch_collider_at: None,
            fall_after_ledge: None,
            sub_restarted: false,
            interpolation_cleared: false,
            age: 0,
            swim: swim::Swim::default(),
            wet: wet::WetControl::default(),
            camera_cm: None,
            material_water: false,
            material_puddle: false,
            water_finder: crate::water::SurfaceFinder::default(),
            step_phase: 0.,
            landing_speed: 0.,
            touchdown_speed: 0.,
            cycle: RunCycle::default(),
            run_start_effect: None,
            run_up: None,
            pose_tilt: None,
            forward: Vec3::ZERO,
            wall_along: wall_along::WallAlong::default(),
            view: None,
            ground_frame: None,
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
            mesh_ground: None,
            touched: terrain::Touched::default(),
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
    pub slippery: bool,
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
    /// Feet in shallow water.
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
    mut fx: MessageWriter<Fx>,
) {
    if menu.open {
        orders.clear_edges();
        return;
    }
    for mut p in &mut players {
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
fn yaw(v: Vec3) -> f32 {
    v.x.atan2(v.z)
}
fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}
/// Rotate a flat direction toward another by at most `max` radians.
fn turn(from: Vec3, to: Vec3, max: f32) -> Vec3 {
    let a = yaw(from);
    let d = wrap(yaw(to) - a).clamp(-max, max);
    Vec3::new((a + d).sin(), 0., (a + d).cos())
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
fn fall_takeoff(vertical_speed: f32) -> air_move::Takeoff {
    air_move::Takeoff {
        speed_limit: t::JUMP_MOVE_MAX,
        base_speed: t::JUMP_MOVE_MIN,
        extend_frames: 0,
        power: vertical_speed,
        gravity: t::JUMP_GRAVITY,
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
fn lerp_clamped(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0., 1.)
}
impl Player {
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
    /// The probe's teleport put the player in the air, over no floor the collider reaches (the
    /// collision was reset with it): no ground contact is counted when the first step runs, so
    /// the fall judge counts that step as one without ground (recorded: the fall state is
    /// chosen in the fifth step, not the sixth).
    pub fn start_in_air(&mut self) {
        self.grounded = false;
        self.ground_contact = 0;
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
            && !matches!(self.mode, Move::Pole | Move::SlopeSlide | Move::Swim)
            && (self.v.y == 0. || self.v.y == -self.pull)
        {
            if self.landing_speed < 0. {
                self.v.with_y(self.landing_speed)
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
        match clip::loop_length(self.anim.name) {
            Some(length) if clip::loop_slack(self.anim.name) > 0. && frame >= length => {
                frame - length
            }
            _ => frame,
        }
    }
    /// The pose's frame of a clip whose counter shows its length for a row before it wraps,
    /// without the wrap: the counter plus the rate change it has still to take over. The loop
    /// restarts for the triggers when this reaches the clip's length.
    fn loop_pose_frame(&self) -> Option<f32> {
        let length = clip::loop_length(self.anim.name)?;
        (clip::loop_slack(self.anim.name) > 0.).then(|| {
            let wrapped = if self.counter_wrapped { length } else { 0. };
            self.anim.frame + self.anim_rate_change + wrapped
        })
    }
    /// A looping clip with a slack starts its loop for the triggers in the step its pose frame
    /// reaches the clip's length (the pose wraps there, the counter a row later).
    fn update_loop_start(&mut self) {
        let Some(length) = clip::loop_length(self.anim.name) else {
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
        (self.sub.active && self.sub.name == self.anim.name).then_some(self.sub.frame)
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
        if let Some(frames) = self.catching {
            return Some(CapRow {
                state: if frames < 0 { "Return" } else { "Catch" },
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
        if self.anim.name == "Move" {
            self.cycle.shown
        } else {
            self.anim.frame
        }
    }
    /// The body hangs on a ledge or clings to a pole and touches ground there
    /// (the top of the pole, the ledge the climb ends on): that contact is no
    /// landing (only the climb jump's and the pole jump's are).
    fn stands_on_ledge(&self) -> bool {
        (self.mode == Move::Ledge && self.catch_nerve() != ledge::CatchNerve::Jump)
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
        self.nerve.clear();
        self.pos = s.pos;
        self.prev = s.pos;
        self.v = s.v;
        self.facing = s.facing.with_y(0.).normalize_or(self.facing);
        self.grounded = s.grounded;
        if let Some(mode) = s.mode {
            self.mode = mode;
            self.gravity = match mode {
                Move::DoubleJump => t::JUMP_GRAVITY_2ND,
                Move::TripleJump => t::JUMP_GRAVITY_3RD,
                Move::Backflip | Move::Sideflip => t::TURN_JUMP_GRAVITY,
                Move::LongJump => t::LONG_JUMP_GRAVITY,
                Move::SpinJump => t::SPIN_JUMP_GRAVITY,
                Move::PoundJump => t::JUMP_GRAVITY,
                _ => t::GRAVITY_AIR,
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
            Move::PoundLand | Move::GroundSpin | Move::SpinDrop
        ) && let Some(steps) = s.sub_steps
        {
            self.frame = steps;
        }
        if let Some(steps) = s.sub_steps {
            self.set_wait_steps(steps);
        }
        // On the ground in a state that flies: the contact row, whose landing
        // the next step decides.
        self.nerve.touchdown = self.grounded && self.mode.in_flight();
        if self.nerve.touchdown {
            // The row reports the fall speed that reached the ground; the
            // ground has already stopped it.
            self.v.y = 0.;
        }
        self.spin_fall = s.spin_fall;
        self.nerve.actor = self.actor_name();
        self.jump_extend = matches!(self.mode, Move::Jump | Move::DoubleJump)
            && self.frame < t::EXTEND_FRAMES
            && self.v.y > 0.;
        self.air_for = None;
        if self.mode.uses_air_control() && !self.grounded {
            self.resume_air_control(s);
        }
        if self.mode == Move::WallSlide {
            self.resume_wall_slide(s);
        }
        if let Some((name, frame)) = s.anim {
            if let Some(clockwise) = spin::clip_is_clockwise(name) {
                self.spin_clockwise = clockwise;
            }
            if self.anim.name != name {
                self.start_clip(name);
            }
            self.anim.frame = frame;
            if name == "Move" {
                self.step_phase = frame;
            }
        }
        self.pull = if matches!(self.mode, Move::Walk | Move::Run | Move::Skid | Move::Turn) {
            GRAVITY_MOVE
        } else {
            t::GRAVITY
        };
        if matches!(self.mode, Move::Walk | Move::Run | Move::Skid | Move::Turn) {
            self.run_nerve = match self.mode {
                Move::Skid => RunNerve::Brake,
                Move::Turn if self.anim.name == "TurnPoint" => RunNerve::Pivot,
                Move::Turn => RunNerve::Turn,
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
                self.cycle = RunCycle::from_recorded(hint.phase, hint.rate, hint.weights, steps);
                self.step_phase = hint.phase;
                self.ground.resume(
                    self.speed_h(),
                    steps,
                    hint.turn_rate,
                    hint.last_input,
                    hint.turn_power,
                );
                // A nerve chosen in this row starts in the next step.
                if let Some(clip) = hint.chosen {
                    let (mode, nerve) = match clip {
                        "Brake" => (Move::Skid, RunNerve::Brake),
                        "Turn" => (Move::Turn, RunNerve::Turn),
                        "TurnPoint" => (Move::Turn, RunNerve::Pivot),
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
        self.resume_cap(s);
        for (name, value) in &s.counters {
            self.set_counter(name, *value);
        }
        // How far the recorded fall had gone is not in the trace; one that is
        // already at the terminal speed counts from here.
        self.fall = FallDistance::default();
        if !self.grounded && self.v.y <= -t::FALL_MAX {
            self.fall.start = Some(self.pos.y);
        }
    }
    /// The cap and the spin throw as a recorded row shows them.
    fn resume_cap(&mut self, s: &Snapshot) {
        let previous = self.cap.take();
        self.launch = None;
        self.winding = false;
        self.winding_age = 0;
        self.catching = s.catching;
        self.catch_clip = self.clip_of_catch();
        self.upper_catch = None;
        self.sub = SubClip::default();
        if let Some((name, frame, length)) = s.sub {
            self.sub = SubClip {
                name,
                frame,
                length,
                active: true,
            };
            // A clip just started under a sub animation holds its first frame.
            self.anim_skip = frame == 0. && s.anim.is_some_and(|(_, frame)| frame == 0.);
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
        };
        if s.throw.is_some_and(|hint| hint.launch) {
            self.launch = Some(launch);
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
        *self = Self::default();
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
        self.restart_wait_steps(name);
        self.anim = Anim {
            name,
            serial: self.anim.serial.wrapping_add(1),
            frame: 0.,
            rate: 1.,
        };
        self.anim_skip = true;
        if name == "Move" {
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
        if self.mode == Move::Swim {
            return crate::game_names::known(self.swim.nerve.name());
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
        if self.anim.name == "Move" {
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
            if let Some(length) = clip::held_length(self.anim.name) {
                self.anim.frame = self.anim.frame.min(length as f32 + 1.);
            }
            if self.anim.name == "FireRun" && self.anim.frame >= clip::FIRE_RUN {
                self.anim.frame -= clip::FIRE_RUN;
            }
            if let Some(length) = clip::loop_length(self.anim.name) {
                let slack = clip::loop_slack(self.anim.name);
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
                if whole_body && self.anim.name == "Move" {
                    // The landing clip covered the legs; their cycle starts
                    // over underneath it, the counter running on.
                    self.cycle.restart_pose();
                }
            }
        }
    }
    /// First step of a state chosen last step: start its clip, then do the
    /// work only its first step does.
    fn begin_step(&mut self) {
        self.counter_restarted = false;
        self.run_start_effect = None;
        self.roll_forget_effect_call();
        let first_step = self.nerve.step == 0;
        self.advance_clips();
        let (clip, work) = self.nerve.start();
        if let Some(clip) = clip {
            self.start_clip(clip);
        }
        if self.cycle_restart_in > 0 {
            self.cycle_restart_in -= 1;
            if self.cycle_restart_in == 0 && self.anim.name == "Move" {
                self.cycle.start();
                self.anim.frame = 0.;
                self.step_phase = 0.;
            }
        }
        if let Some(work) = work {
            work(self);
        }
        self.refresh_cap_jump(first_step);
    }
    /// A cap jump is possible again (`PlayerCapActionHistory::
    /// clearLandLimitStandAngle` and `clearLandLimit`): the first step of the
    /// ground nerves on level ground, every step of the spin throw's ground
    /// state and of the pound, and the first step of the climbing and swimming
    /// ones.
    fn refresh_cap_jump(&mut self, first_step: bool) {
        let restored = match self.actor_name() {
            "Wait" | "Squat" | "Run" | "Rolling" => self.grounded && first_step,
            "SpinCap" => self.grounded,
            "HipDrop" => self.grounded,
            "WallCatch" | "PoleClimb" | "GrabCeil" | "Swim" => first_step,
            _ => false,
        };
        self.cap_jump_possible |= restored;
    }
    fn set_h(&mut self, h: Vec3) {
        self.v.x = h.x;
        self.v.z = h.z;
    }

    /// Advance one original frame.
    pub fn step(&mut self, o: &Orders, scene: &Course) {
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
        self.note_frame_start_for_joints();
        let low_ceiling = self.grounded
            && scene.solids.iter().any(|b| {
                let bottom = b.center.y - b.half.y;
                bottom > self.pos.y + 0.2
                    && bottom < self.pos.y + HEIGHT
                    && (self.pos.x - b.center.x).abs() < b.half.x + RADIUS
                    && (self.pos.z - b.center.z).abs() < b.half.z + RADIUS
            });
        let mut o = o.clone();
        if low_ceiling && self.mode != Move::Roll {
            o.crouch = true;
            o.jump = false;
        }
        self.read_input(&o);
        self.landing_speed = 0.;
        self.nerve.deferred = false;
        self.nerve.renewed = false;
        self.sensor_restart = false;
        self.sub_restarted = false;
        self.interpolation_cleared = false;
        self.upper_cleared = false;
        self.age = self.age.saturating_add(1);
        let was_grounded = self.grounded;
        // The game animates first, then the state runs; a state chosen last
        // frame starts now.
        // The cap updates before the player does, within the same frame.
        self.update_cap(&o, scene);
        self.cap_sensor_phase(&o);
        self.begin_step();
        if !matches!(self.mode, Move::Walk | Move::Run | Move::Skid | Move::Turn)
            && !(self.mode == Move::Throw && self.grounded)
        {
            self.pull = t::GRAVITY;
        }
        self.change_from_surroundings(scene, &o);
        // A state chosen above leaves its own first step to the next frame.
        if !self.nerve.deferred {
            self.run_state(&o, scene);
            self.judge_fall();
            self.judge_pole_climb(&o, scene);
            self.judge_wall_catch(scene);
            self.judge_wall_keep(scene);
            self.judge_in_water(scene);
        }
        let moved = self.v;
        self.collide(&o, scene);
        self.update_water_finder(scene);
        self.fall.update(
            self.pos.y / UNIT,
            moved.y,
            self.grounded,
            self.grounded && !was_grounded,
        );
        // Hot ground takes over any landing; shallow water puts a burn out.
        let hot = self.grounded && scene.hot(self.pos);
        let quench = self.grounded && !hot && self.burning() && scene.puddle(self.pos);
        if quench {
            self.events.push(Fx::Quenched);
            self.end_burn();
        } else if self.grounded
            && !was_grounded
            && !hot
            && self.mode != Move::GroundSpin
            && self.mode != Move::SlopeSlide
            && self.mode != Move::Swim
            && !self.stands_on_ledge()
            && !matches!(self.mode, Move::Walk | Move::Run | Move::Skid | Move::Turn)
        {
            // The contact is flagged now; the next step decides what it means.
            // (The ground spin only regains the floor it let go of when it was
            // chosen, and the run nerve runs on over a short drop: it judges
            // the fall only after five steps without ground; neither looks at
            // the contact as a landing.)
            self.nerve.touchdown = true;
            self.touchdown_speed = self.landing_speed;
        }
        if hot {
            self.burn();
        }
        self.finish_frame(scene);
    }
    /// Changes of state the surroundings force, decided before the state runs: the ground
    /// reached last step. (Water is asked after the state's own step, see `judge_in_water`.)
    fn change_from_surroundings(&mut self, _scene: &Course, o: &Orders) {
        if std::mem::take(&mut self.nerve.touchdown) {
            self.land(o);
        }
    }
    /// `PlayerJudgeSpeedCheckFall::update`, once a step before the nerve runs
    /// (it reads the contact the last step ended with): the steps without
    /// ground under the player count up to five, any ground resets them. Only
    /// the ground contact is read; the original also asks that the floor be
    /// within the run angle (`rs::isCollidedGroundRunAngle`), which a steeper
    /// floor than the stages' flat ground would change (not ported).
    fn update_ground_lost(&mut self) {
        self.ground_lost = if self.grounded {
            0
        } else {
            (self.ground_lost + 1).min(GROUND_LOST_FRAMES + 1)
        };
    }
    /// The actor's fall judge (`PlayerJudgeSpeedCheckFall::judge`), asked by
    /// the standing, running and squatting nerves after their own step and
    /// after the jump they allow: a ground nerve that has run on without
    /// ground for more than four steps hands over to the fall. Only the run
    /// is recorded (walking and running off a terrace); the other ground
    /// states are assumed to ask the same judge.
    fn judge_fall(&mut self) {
        if self.ground_lost > GROUND_LOST_FRAMES
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
            self.go(Move::Fall, "Fall");
        }
    }
    /// Run the nerve of the current state for this frame.
    fn run_state(&mut self, o: &Orders, scene: &Course) {
        self.update_ground_height(scene);
        match self.mode {
            Move::Idle | Move::Walk | Move::Run => self.run(o),
            Move::Skid | Move::Turn => self.brake(o),
            Move::SquatBrake | Move::Crouch | Move::Crawl => self.squat(o),
            Move::GroundSpin => self.ground_spin(o),
            Move::Throw => self.throw_state(o),
            Move::GroundPound => self.hip_drop(o),
            Move::PoundLand => self.hip_drop_land(o),
            Move::Roll => self.roll(o),
            Move::RollEnd => self.roll_stand_up_step(o),
            Move::RollJump => self.roll_jump_step(o),
            Move::Dive => self.head_sliding(o),
            Move::SlopeSlide => self.slope_slide(o),
            Move::BurnJump | Move::BurnBounce => self.burn_air(o),
            Move::FireRun => self.fire_run(o),
            Move::LongJump => self.long_jump_air(o),
            Move::Backflip => self.back_jump_air(o),
            Move::Sideflip => self.turn_jump_air(o),
            Move::WallJump => self.wall_jump(o),
            mode if mode.uses_air_control() => self.jump_air(o),
            Move::WallSlide => self.wall_slide(o),
            Move::Ledge => self.wall_catch(o, scene),
            Move::Pole => self.pole_climb(o, scene),
            Move::Swim => self.swim(o, scene),
            Move::SpinDrop => self.spin_drop(o),
            _ => self.air(o),
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
    fn finish_frame(&mut self, scene: &Course) {
        self.update_combo_counters();
        self.update_soot(scene);
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
        // (`PlayerWetControl::update` ends the actor's movement; the player is never in a wet area.)
        self.wet.update();
        self.update_material_code(scene);
        self.update_sounds(scene);
        self.timer = self.frame as f32 / FPS;
        self.since_land = self.land_frames as f32 / FPS;
        // The toy course has nothing below it: falling out of it brings the
        // player back to the checkpoint. (The game has its own abyss handling,
        // the `Abyss` nerve and the recovery point, which is not ported; a
        // stage's collision is left to itself.)
        let out_of_course = scene.mesh.is_none() && self.pos.y < -18.;
        if out_of_course || !self.pos.is_finite() || !self.v.is_finite() {
            self.reset();
        }
        // The head's look-at runs after the state (`PlayerActorHakoniwa::movement`).
        // (It is off while the animator plays a sub animation: `movement` asks the
        // animator's sub-animation flag before the state's `isEnableLookAt`.)
        let enabled = look_at::look_enabled(self.nerve_name().name)
            && !self.run_nerve_blocks_look()
            && !self.sub.active;
        let actor = self.pose_matrix();
        self.look.step(&actor, enabled, scene.look_sensors());
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
        if o.crouch && self.slope_slide_judge() {
            self.start_slope_slide();
            return true;
        }
        if o.crouch && !matches!(self.mode, Move::SquatBrake | Move::Crouch | Move::Crawl) {
            self.start_squat();
            return true;
        }
        if self.pre_throw > 0 && !o.crouch || o.up_throw || o.down_throw || o.spin_throw {
            self.pre_throw = 0;
            let kind = self.throw_kind(o);
            if self.cap.is_none() {
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
    fn run(&mut self, o: &Orders) {
        if self.mode == Move::Idle {
            // Every nerve of the standing state settles the pose first; the
            // judges that leave it (the run, the jump) come after.
            self.wait_pose();
            self.stand(o);
        } else {
            self.run_step(o);
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
    fn stand(&mut self, o: &Orders) {
        // (The forced slope slide comes before the state's own start-run judge.)
        if self.try_force_slope_slide() {
            return;
        }
        if self.tilt >= STICK_DEADZONE {
            // The state's own start-run judge comes before the actor's (jump,
            // crouch, throw): a jump pressed with the stick is taken by the
            // run state's first step (`pole-climb` rows 10-11).
            // The run state's appearance already pulls the player to the floor.
            self.pull = GRAVITY_MOVE;
            self.go(Move::Walk, "Move");
            return;
        }
        if !self.land_stiffen_holds() && self.ground_actions(o) {
            return;
        }
        self.run_frames = 0;
        let step = self.count_wait_step();
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
        self.v.y = if self.grounded {
            -t::GRAVITY
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
        if let Some(view) = &self.view {
            return self
                .wall_along
                .snap(move_input::move_input(self.raw_stick, view, up), up);
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
        let (side, up, front) = (pose.side(), pose.up(), pose.front());
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
        crate::skeleton_pose::world_pose(&actor, locals, &mut controls)
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
    fn run_step(&mut self, o: &Orders) {
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
        let steer = self.ground.update(&SteerInput {
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
        self.run_steps += 1;
        self.joints
            .run
            .run_step(steer.speed, self.ground.turn_tilt_rate(up));
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
            self.cycle.reset(steer.speed, after_turn);
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
                self.end_run();
                true
            }
        } else if steer.pivot {
            self.switch(Move::Turn, "TurnPoint", |p| p.enter_nerve(RunNerve::Pivot));
            false
        } else if steer.stopped {
            self.end_run();
            true
        } else {
            false
        };
        // The run state's own judge for the ground spin comes before the
        // actor's (jump, crouch, throw), which look at the state's result.
        if !ended {
            self.try_start_ground_spin();
            if !self.try_force_slope_slide() {
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
                self.facing = -self.brake_dir;
                self.events.push(Fx::TurnDust);
            }
            RunNerve::Pivot => {
                self.joints.run.stopped();
                let front = self.facing;
                self.pivot.reset(front);
            }
            RunNerve::Run | RunNerve::RunAfterTurn => {}
        }
    }
    /// `PlayerStateRunHakoniwa2D3D::isEnableLookAt` hands `Run3D` to the base run state's own
    /// `isEnableLookAt`, which answers for its nerve: the run, the brake and the run after a
    /// turn may look; the turn and the pivot (`TurnPoint`) may not. The nerve counts from the
    /// step that chooses it (the keeper reads it in that step, a step before the clip starts).
    fn run_nerve_blocks_look(&self) -> bool {
        self.mode == Move::Turn
    }
    /// The run state dies and the actor chooses the standing state.
    fn end_run(&mut self) {
        // The standing state's first step leaves no speed over from the run (a
        // braking run that is cut short stops dead).
        self.change(Move::Idle, "Wait", |p| p.set_h(Vec3::ZERO));
    }
    /// The original's run cycle: weights and pace by speed and tilt, and the
    /// footfalls the pose passes.
    fn update_move_anim(&mut self, speed: f32) {
        if self.anim.name != "Move" {
            return;
        }
        let tilt = self.ground_stick().length();
        self.cycle.update(speed, tilt);
        self.anim.rate = self.cycle.rate;
        self.step_phase = self.cycle.phase;
    }
    /// The brake, the turn after it and the pivot on the spot: the rest of
    /// the run state's nerves.
    fn brake(&mut self, o: &Orders) {
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
        if self.try_force_slope_slide() {
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
        let before = self.speed_h();
        let speed = ground::slide_speed(before);
        self.set_h(self.brake_dir * speed);
        self.v.y = -GRAVITY_MOVE;
        self.pull = GRAVITY_MOVE;
        before
    }
    fn brake_step(&mut self, step: u32) {
        if step == 0 {
            self.brake_dir = self.facing;
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
            self.end_run();
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
        let step = self
            .pivot
            .update(self.ground_stick(), self.facing, flat_velocity);
        self.facing = step.front;
        self.set_h(step.velocity);
        self.v.y = -t::GRAVITY;
        self.pull = t::GRAVITY;
        if self.pivot.done {
            if self.pivot.has_input {
                self.switch(Move::Walk, "Move", |p| p.enter_nerve(RunNerve::Run));
            } else {
                self.end_run();
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
    /// Hot ground, as the original's fire damage: Mario leaps clutching his
    /// seat, bounces once where he lands, then runs flat out for a second
    /// before the normal run takes over. Touching hot ground again restarts it.
    fn burn(&mut self) {
        self.change(Move::BurnJump, "FireRunStart", |p| {
            // The leap keeps the current speed, turned along the stick; with the
            // stick at rest it goes straight up.
            let h = if p.tilt > 0.05 {
                p.stick * p.speed_h()
            } else {
                Vec3::ZERO
            };
            p.fire_leap(
                h,
                t::DAMAGE_FIRE_JUMP_POWER_1ST,
                t::DAMAGE_FIRE_NO_GRAVITY_FRAMES,
            );
            p.events.push(Fx::Burned(false));
        });
    }
    fn burning(&self) -> bool {
        matches!(self.mode, Move::BurnJump | Move::BurnBounce | Move::FireRun)
    }
    /// The soot stain: every burning frame darkens it, up to full strength
    /// after 200 frames, and restarts the hold. Water multiplies it down each
    /// frame and cancels the hold, so a quick dip clears it within seconds.
    fn update_soot(&mut self, scene: &Course) {
        let cap_on = self.cap.is_none();
        let burning = self.burning();
        let s = &mut self.soot;
        let submerged = scene.wet(self.pos + Vec3::Y * 0.8);
        s.wet = if submerged {
            t::WET_FRAMES
        } else {
            s.wet.saturating_sub(1)
        };
        let soaked = submerged || s.wet > 0 || self.grounded && scene.puddle(self.pos);
        if soaked {
            // A puff of steam when water first reaches a visible stain.
            if !s.washing && s.body > t::SOOT_SMOKE_OFF && !self.events.contains(&Fx::Quenched) {
                self.events.push(Fx::Quenched);
            }
            s.smoking = false;
            s.hold = 0;
            s.body *= t::SOOT_WASH;
            s.cap *= t::SOOT_WASH;
        } else if burning {
            s.body = (s.body + t::SOOT_GAIN).min(1.);
            s.cap = if cap_on {
                (s.cap + t::SOOT_GAIN).min(1.)
            } else {
                0.
            };
            s.hold = t::SOOT_HOLD_FRAMES;
            s.smoking |= s.body >= t::SOOT_SMOKE_ON;
        } else if s.hold > 0 {
            s.hold -= 1;
            if !cap_on {
                s.cap = 0.;
            }
        } else {
            let step = 1. / t::SOOT_FADE_FRAMES;
            s.body = (s.body - step).max(0.);
            s.cap = if cap_on { (s.cap - step).max(0.) } else { 0. };
        }
        s.washing = soaked;
        if s.body <= t::SOOT_SMOKE_OFF {
            s.smoking = false;
        }
        if s.body < 0.001 && s.cap < 0.001 {
            s.body = 0.;
            s.cap = 0.;
        }
    }
    fn fire_leap(&mut self, h: Vec3, power: f32, extend: u32) {
        let h = h.clamp_length_max(t::DAMAGE_FIRE_JUMP_MOVE);
        self.fire.direction = h.normalize_or(self.facing);
        self.v = h + Vec3::Y * power;
        self.fire.extend = extend;
        self.grounded = false;
        self.cap_jump_possible = true;
    }
    /// The bounce after the leap: a second, lower hop without the float.
    fn fire_bounce(&mut self) {
        self.change(Move::BurnBounce, "FireRunStart", |p| {
            let h = if p.tilt > 0.05 {
                p.stick * p.speed_h()
            } else {
                Vec3::ZERO
            };
            p.fire_leap(h, t::DAMAGE_FIRE_JUMP_POWER_2ND, 0);
            p.events.push(Fx::Burned(true));
        });
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
            self.start_throw(kind);
            return true;
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
    fn burn_air(&mut self, o: &Orders) {
        if self.fire_throws(o) {
            return;
        }
        self.fire_air(t::DAMAGE_FIRE_GRAVITY);
    }
    /// Air control during the burn: the stick pushes along and across the
    /// leap's direction, within the leap's speed.
    fn fire_air(&mut self, gravity: f32) {
        if self.fire.extend > 0 {
            self.fire.extend -= 1;
        } else {
            self.v.y = (self.v.y - gravity).max(-t::FALL_MAX);
        }
        let mut h = self.v.with_y(0.);
        if h.dot(self.fire.direction) < 0. {
            self.fire.direction = -self.fire.direction;
        }
        let dir = self.fire.direction;
        let side = Vec3::Y.cross(dir);
        let input = self.stick * self.tilt;
        let along = input.dot(dir);
        let accel = if along > 0. || along.abs() < 0.001 {
            t::JUMP_ACCEL_FRONT
        } else {
            t::JUMP_ACCEL_BACK
        };
        h += dir * along * accel + (input - dir * along) * t::JUMP_ACCEL_TURN;
        let limit = t::DAMAGE_FIRE_JUMP_MOVE;
        self.set_h(dir * h.dot(dir).clamp(-limit, limit) + side * h.dot(side).clamp(-limit, limit));
        if self.tilt > 0.05 {
            self.facing = turn(self.facing, self.stick, 0.1);
        }
    }
    fn start_fire_run(&mut self) {
        self.fire.frames = 0;
        self.fire_run_landed();
    }
    /// The run phase restarts on every landing; the burn's clock does not.
    fn fire_run_landed(&mut self) {
        let brake = t::DAMAGE_FIRE_RUN_BRAKE_FRAMES as u32;
        if self.mode == Move::FireRun {
            // Landing again during the run: the clip loops on.
            self.frame = 0;
            self.fire.brake = brake;
            return;
        }
        self.change(Move::FireRun, "FireRun", move |p| {
            p.anim.rate = t::DAMAGE_FIRE_RUN_SPEED * t::DAMAGE_FIRE_RUN_ANIM_RATE;
            p.fire.brake = brake;
        });
    }
    /// Running on the burn: full fire speed while the stick is held, easing to
    /// a stop when released. Only now can Mario jump out of it.
    fn fire_run(&mut self, o: &Orders) {
        if self.fire.frames >= t::DAMAGE_FIRE_RUN_TIME {
            self.end_burn();
            return;
        }
        self.fire.frames += 1;
        if self.fire_throws(o) {
            return;
        }
        if !self.grounded {
            // Off a ledge: a plain fall within the leap's speed, still burning.
            if self.frame == 0 {
                self.fire_leap(self.v.with_y(0.), 0., 0);
            }
            self.frame = 1;
            self.fire_air(t::GRAVITY_AIR);
            return;
        }
        if self.pre_jump > 0 {
            self.ground_jump(o);
            return;
        }
        if self.tilt > 0.05 {
            let limit = lerp_clamped(
                t::ROUND_LIMIT_DEG,
                t::ROUND_LIMIT_DEG_MIN,
                t::DAMAGE_FIRE_RUN_SPEED / t::NORMAL_MAX,
            );
            self.facing = turn(self.facing, self.stick, limit.to_radians());
            self.fire.brake = 0;
        } else {
            self.fire.brake = (self.fire.brake + 1).min(t::DAMAGE_FIRE_RUN_BRAKE_FRAMES as u32);
        }
        let k = self.fire.brake as f32 / t::DAMAGE_FIRE_RUN_BRAKE_FRAMES;
        let ease = k * (1.5 - 0.5 * k * k);
        self.set_h(self.facing * t::DAMAGE_FIRE_RUN_SPEED * (1. - ease).clamp(0., 1.));
        self.v.y = -t::GRAVITY;
    }
    fn end_burn(&mut self) {
        if self.grounded {
            self.run_frames = t::BRAKE_ON_COUNTER;
            self.go(
                Move::Run,
                if self.speed_h() > 0.5 { "Move" } else { "Wait" },
            );
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
        anim: &'static str,
        power: f32,
        gravity: f32,
        then: impl FnOnce(&mut Player) + Send + Sync + 'static,
    ) {
        self.pre_jump = 0;
        self.change(mode, anim, move |p| {
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
        self.events.push(Fx::Jump(mode));
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
        let takeoff = takeoff_of(mode, extend, power, gravity);
        self.air.setup(&mut self.v, self.facing, &takeoff);
        self.air_for = Some(mode);
        self.jump_power = power;
    }
    /// A state flown by the air control that no take-off armed (a fall, or a
    /// recorded state being replayed): set it up from the velocity it has.
    fn arm_fall_control(&mut self) {
        let mut takeoff = fall_takeoff(self.v.y);
        if self.fall_after_ledge.is_some() {
            takeoff.input_lock = ledge::LET_GO_INPUT_LOCK;
        }
        self.air.setup(&mut self.v, self.facing, &takeoff);
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
        let (extend, gravity) = match self.mode {
            Move::DoubleJump => (t::EXTEND_FRAMES, t::JUMP_GRAVITY_2ND),
            Move::TripleJump => (t::EXTEND_FRAMES, t::JUMP_GRAVITY_3RD),
            Move::Jump => (t::EXTEND_FRAMES, t::JUMP_GRAVITY),
            Move::CapBounce => (0, t::TRAMPLE_HIGH_GRAVITY),
            Move::CatchJump => (0, t::CAP_CATCH_JUMP_GRAVITY),
            Move::SpinJump => (0, t::SPIN_JUMP_GRAVITY),
            Move::WallJump => (0, t::WALL_JUMP_GRAVITY),
            _ => (0, t::JUMP_GRAVITY),
        };
        let takeoff = if self.mode == Move::Fall {
            fall_takeoff(self.v.y)
        } else if self.mode == Move::WallJump {
            wall_air::wall_jump_takeoff()
        } else {
            takeoff_of(self.mode, extend, self.v.y, gravity)
        };
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
        if !self.combo_kept {
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
        let look = Vec3::new(-self.camera_yaw.sin(), 0., -self.camera_yaw.cos());
        let side = Vec3::new(self.facing.z, 0., -self.facing.x);
        side.dot(look) < -1e-4
    }
    /// The first, second or third jump of the combo. Which one it is, and
    /// what that does to the combo, is decided as the jump state appears (when
    /// the jump is chosen); the take-off follows with its first step.
    fn start_standard_jump(&mut self) {
        self.pre_jump = 0;
        let kind = self.pick_jump_kind();
        let (mode, clip, power_min, power_max, gravity) = match kind {
            2 => (
                Move::TripleJump,
                "Jump3",
                t::CONTINUOUS_JUMP_POWER_MIN,
                t::JUMP_POWER_3RD,
                t::JUMP_GRAVITY_3RD,
            ),
            1 => (
                Move::DoubleJump,
                "Jump2",
                t::CONTINUOUS_JUMP_POWER_MIN,
                t::JUMP_POWER_MAX_2ND,
                t::JUMP_GRAVITY_2ND,
            ),
            _ => (
                Move::Jump,
                if self.side_faces_camera() {
                    "Jump"
                } else {
                    "JumpReverse"
                },
                t::JUMP_POWER_MIN,
                t::JUMP_POWER_MAX,
                t::JUMP_GRAVITY,
            ),
        };
        // The power follows the speed along the facing.
        let forward = self.v.dot(self.facing).abs();
        let power =
            air_move::jump_power(forward, t::NORMAL_MIN, t::NORMAL_MAX, power_min, power_max);
        let ground_up = self.ground_pose_up_for_jump();
        self.jump_with(mode, clip, power, gravity, move |p| {
            if let Some(horizontal) = p.takeoff_h {
                p.set_h(horizontal);
            }
            if let Some(up) = ground_up {
                p.slerp_pose_up(up, 1., 180.);
            }
        });
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
    /// The turn of a body in the air and its pose: `PlayerActionTurnControl::update` turns the
    /// front (the actor's front carried to the horizontal plane) toward the stick, and
    /// `rs::slerpUpFront` relaxes a tilted pose toward the world's up and that front at the
    /// state's `SlerpQuatRate`. An upright pose just faces the turned front.
    fn turn_in_air(&mut self, push: Vec3) {
        let Some(stored) = self.pose_quat() else {
            self.facing = self.air.turn_facing(self.facing, push);
            return;
        };
        let pose = plane::ActorPose::new(stored);
        let front = along_vector_normal_h(pose.front(), pose.up(), Vec3::Y);
        let turned = self.air.turn_facing(front, push);
        self.set_pose(pose.slerp_up_front(Vec3::Y, turned, t::SLERP_QUAT_RATE, 0.));
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
                t::CAP_CATCH_JUMP_GRAVITY,
                |p| p.jump_extend = false,
            );
            return;
        }
        if matches!(self.mode, Move::Skid | Move::Turn)
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
            t::HIP_DROP_JUMP_POWER,
            t::JUMP_GRAVITY,
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
            t::TURN_JUMP_GRAVITY,
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
            t::SQUAT_JUMP_GRAVITY,
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
        self.jump_with(mode, name, t::LONG_JUMP_POWER, t::LONG_JUMP_GRAVITY, |p| {
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
        });
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
                gravity: t::LONG_JUMP_GRAVITY,
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
                gravity: t::SQUAT_JUMP_GRAVITY,
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
                gravity: t::TURN_JUMP_GRAVITY,
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
        if self.air_for != Some(self.mode) {
            self.arm_fall_control();
        }
        self.scale_velocity_at_wall();
        let steer = air_move::Steering {
            front: t::JUMP_ACCEL_FRONT,
            back: t::JUMP_ACCEL_BACK,
            turn: t::JUMP_ACCEL_TURN,
        };
        let push = self.push();
        self.air.update(&mut self.v, push, o.jump_held, &steer);
        self.turn_in_air(push);
        self.air_actions(o);
    }
    /// Free flight of the states that keep the older approximation of the
    /// air control: the roll jump and the wall jump.
    fn air(&mut self, o: &Orders) {
        if self.air_actions(o) {
            return;
        }
        let mut h = self.v.with_y(0.);
        let speed = h.length();
        if self.tilt > 0.05 {
            let wish = self.stick;
            let dir = h.normalize_or(self.facing);
            let angle = wrap(yaw(wish) - yaw(dir)).abs().to_degrees();
            let limit = if angle > t::JUMP_TURN_FAST_DEG {
                t::JUMP_TURN_FAST_LIMIT_DEG
            } else {
                t::JUMP_TURN_LIMIT_DEG
            };
            if wish.dot(dir) < -0.2 {
                h = dir * (speed - t::JUMP_ACCEL_BACK * self.tilt).max(0.);
            } else {
                let cap = self.max_air_speed.max(t::JUMP_MOVE_MIN * self.tilt);
                let s = if speed < cap {
                    (speed + t::JUMP_ACCEL_FRONT * self.tilt).min(cap)
                } else {
                    speed
                };
                h = turn(dir, wish, limit.to_radians()) * s;
            }
            if !matches!(self.mode, Move::Backflip | Move::Sideflip) {
                self.facing = turn(self.facing, wish, 0.1);
            }
        } else {
            h *= t::FRICTION_AIR;
        }
        self.set_h(h);
        let extending =
            self.jump_extend && o.jump_held && self.frame < t::EXTEND_FRAMES && self.v.y > 0.;
        if !o.jump_held || self.frame >= t::EXTEND_FRAMES {
            self.jump_extend = false;
        }
        // The first step of the three ground jumps applies the push and no
        // gravity, whether or not jump is held (the game's first rows after a
        // tap show the full jump power); the flips and the long jump do apply it.
        let takeoff = self.frame == 0
            && matches!(self.mode, Move::Jump | Move::DoubleJump | Move::TripleJump);
        if !extending && !takeoff {
            self.v.y -= self.gravity;
        }
        self.v.y = self.v.y.max(-t::FALL_MAX);
        if self.v.y < 0. && self.gravity != t::GRAVITY_AIR {
            self.gravity = t::GRAVITY_AIR;
        }
        // Jump clips end in a fall pose; then the fall loop takes over.
        let ends: Option<u32> = match self.anim.name {
            "Jump" | "JumpReverse" => Some(84),
            "Jump2" => Some(42),
            "Jump3" | "JumpBack" => Some(68),
            "JumpTurn" => Some(60),
            "RollingJump" => Some(28),
            "JumpHipDrop" => Some(46),
            "JumpCapLeapFrog" => Some(clip::JUMP_CAP_LEAP_FROG),
            "JumpCapCatchCommon" => Some(27),
            n if n.starts_with("SpinCapAirStart") => Some(clip::throw(n)),
            _ => None,
        };
        if ends.is_some_and(|n| self.anim_done(n)) && self.v.y < 0. {
            self.play("Fall");
        }
    }
    /// Airborne transitions shared by every free-flight state.
    fn air_actions(&mut self, o: &Orders) -> bool {
        if self.try_cap_catch_pop(o) {
            return true;
        }
        if (self.pre_throw > 0 || o.throw_pressed) && o.crouch && self.dive_possible {
            self.pre_throw = 0;
            self.dive();
            return true;
        }
        if o.crouch_pressed && !matches!(self.mode, Move::Throw) {
            if self.mode == Move::SpinJump {
                self.start_spin_drop();
                return true;
            }
            // The pound needs room above the ground (`PlayerJudgeStartHipDrop`).
            if self.ground_height >= air_move::tune::HIP_DROP_HEIGHT {
                self.start_hip_drop();
                return true;
            }
        }
        if (self.pre_throw > 0 && !o.crouch || o.up_throw || o.down_throw || o.spin_throw)
            && self.cap.is_none()
        {
            self.pre_throw = 0;
            let kind = self.throw_kind(o);
            self.start_throw(kind);
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
        // A throw during the pound turns it into a dive.
        if self.pre_throw > 0 || o.throw_pressed {
            self.pre_throw = 0;
            if self.tilt > 0.1 {
                self.facing = self.stick;
            }
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
            self.v = Vec3::NEG_Y * t::HIP_DROP_SPEED;
        }
    }
    /// `PlayerStateHipDrop::isEnableLandCancel`: the pound's landing nerve may
    /// be cancelled into a jump (or a throw) from its fifth to its thirtieth
    /// step. The actor reads the nerve's step after the landing nerve's own
    /// update has counted the current frame, so the frame in progress counts.
    fn pound_jump_permitted(&self) -> bool {
        (t::HIP_DROP_JUMP_BEGIN..=t::HIP_DROP_JUMP_END).contains(&(self.frame + 1))
    }
    fn hip_drop_land(&mut self, o: &Orders) {
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
        if (self.pre_throw > 0 || o.throw_pressed) && self.frame <= t::HIP_DROP_JUMP_END {
            self.pre_throw = 0;
            if o.crouch {
                // Roll straight out of the pound with a strong start.
                self.start_roll(RollStart::Boost);
            } else if self.cap.is_none() {
                self.start_throw(Throw::Down);
            }
            return;
        }
        // From its 24th step (`isEnableMove`, counted like the cancel window)
        // the landing hands over to the run when the stick is pushed, and to
        // an ordinary jump when one is buffered (the pound's jump needed the
        // window above).
        let can_move = self.frame + 1 >= t::HIP_DROP_LAND_CANCEL;
        if can_move && self.pre_jump > 0 && self.tilt <= 0.1 && !o.crouch {
            self.ground_jump(o);
            return;
        }
        if can_move && (self.tilt > 0.1 || o.crouch) || self.anim_done(clip::HIP_DROP_LAND) {
            if o.crouch {
                self.start_squat();
            } else {
                self.go(Move::Idle, "Wait");
            }
        }
    }
    fn dive(&mut self) {
        self.change(Move::Dive, "HeadSlidingStart", |p| {
            let mut h = p.v.with_y(0.).normalize_or(p.facing);
            if p.tilt > 0.1 {
                h = p.stick;
            }
            p.facing = h;
            let up = p.v.y.max(0.) + t::HEAD_SLIDING_JUMP;
            p.v = h * t::HEAD_SLIDING_SPEED + Vec3::Y * up;
            p.dive_possible = false;
            p.events.push(Fx::Dive);
        });
    }
    fn head_sliding(&mut self, _o: &Orders) {
        self.head_sliding_motion();
        if self.anim.name == "HeadSlidingStart" && self.anim_done(clip::HEAD_SLIDING_START) {
            self.play("HeadSliding");
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
                speed_max: t::HEAD_SLIDING_SPEED,
                speed_min: t::HEAD_SLIDING_SPEED_MIN,
                side_accel: t::HEAD_SLIDING_SIDE_ACCEL,
                gravity: t::HEAD_SLIDING_GRAVITY,
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
            // throw reads (`PlayerTrigger::isOnSpinMoveCapThrow`); which clip
            // and flight it picks is a guess (`throw_clip`).
            Throw::Spin
        } else {
            Throw::Forward
        }
    }
    /// The spin throw begins (`PlayerStateSpinCap` appearing and its first
    /// step): the throw is counted among those in a row, the clip is chosen,
    /// and the cap waits in its spin attack for the throw frame.
    fn start_throw(&mut self, kind: Throw) {
        if self.cap.is_some() || self.launch.is_some() {
            return;
        }
        let air = !self.grounded;
        // `PlayerSpinCapAttack::setupAttackInfo`: a throw soon after a catch
        // continues the count; the third one is the continuous throw.
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
        self.throwing.kind = Some(kind);
        self.throwing.in_air = air;
        let name = throw_clip(kind, air, self.throwing.count, self.throwing.continuous);
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
            self.v.y = -GRAVITY_MOVE;
        }
        self.throwing.dir = None;
        self.change(Move::Throw, name, move |p| {
            // First step of the state (`updateSpinGroundNerve` and
            // `updateSpinAirNerve`): the actor turns to the throw direction.
            p.face_throw_direction();
            // Both spin starts clear an upper-body animation still attached
            // (`isUpperBodyAnimAttached`, `clearUpperBodyAnim`): a catch's.
            if p.upper_catch.take().is_some() {
                p.upper_cleared = true;
            }
            if air {
                // `PlayerActionAirMoveControl::setup` with the spin's values:
                // the small lift of every air throw.
                p.air.setup(&mut p.v, p.facing, &spin_air_takeoff());
                p.air_for = Some(Move::Throw);
            }
            p.winding = true;
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
        let dir = if self.tilt >= STICK_DEADZONE {
            self.stick
        } else {
            self.facing
        };
        let up = self
            .mesh_ground
            .filter(|_| self.grounded)
            .unwrap_or(Vec3::Y);
        let stored = self
            .actor_pose()
            .slerp_up_front(up, dir, 1., t::HILL_POSE_DEGREE_MAX);
        self.set_pose(stored);
        self.throwing.dir = Some(dir);
    }
    /// Start a sub animation on the model; `frame` is where its counter
    /// stands before this frame's advance.
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
        if self.throwing.continuous {
            t::SPIN_CAP_THROW_FRAME_CONTINUOUS
        } else if kind != Throw::Forward || self.throwing.count == 2 {
            t::SPIN_CAP_THROW_FRAME_SWING
        } else if air {
            t::SPIN_CAP_THROW_FRAME_AIR
        } else {
            t::SPIN_CAP_THROW_FRAME
        }
    }
    /// The throw state (`PlayerStateSpinCap`): `exeSpinGround` on the ground,
    /// `exeSpinAir` in the air.
    fn throw_state(&mut self, o: &Orders) {
        let kind = self.throwing.kind.unwrap_or(Throw::Forward);
        let air = !self.grounded;
        let step = self.frame;
        // The cap leaves on the throw frame, if it is still the spin attack.
        if self.winding && self.launch.is_none() && step == self.throw_frame(kind, air) {
            self.launch_cap(kind, self.throwing.continuous);
        }
        if air {
            self.spin_air(o, step);
        } else {
            self.spin_ground(o, step);
        }
    }
    /// `PlayerStateSpinCap::exeSpinGround` and the actor's cancels
    /// (`PlayerActorHakoniwa::exeSpinCap`).
    fn spin_ground(&mut self, o: &Orders, step: u32) {
        // `updateSpinGroundNerve`: the spin brakes the momentum for its first
        // frames; from then on the state's ground move control steers, from
        // the speed the player has (and so moves off at once when the stick
        // is held, a plain `max / frames` per frame, see `GroundMove::plain`).
        // The floor pulls with the ground control's gravity throughout.
        if step < t::SPIN_BRAKE_FRAMES {
            let h = self.v.with_y(0.) * t::SPIN_BRAKE_RATE;
            self.set_h(h);
        } else {
            if step == t::SPIN_BRAKE_FRAMES {
                self.spin_ground_move.reset(self.v, self.facing, Vec3::Y);
            }
            let (actor_up, actor_front) = self.pose_axes();
            let steer = self.spin_ground_move.update(&SteerInput {
                up: Vec3::Y,
                wall: None,
                velocity: self.v,
                actor_front,
                actor_up,
                stick: self.ground_stick(),
                max_speed: t::NORMAL_MAX,
                accel_frames: t::ACCEL_FRAMES as i32,
            });
            if steer.front.length_squared() > 0.5 {
                self.facing = steer.front;
            }
            self.set_h(steer.velocity);
        }
        self.v.y = -GRAVITY_MOVE;
        self.pull = GRAVITY_MOVE;
        // The tail of `updateSpinGroundNerve`: within three frames of the clip's end the state
        // keeps asking for the inverse kinematics until it is chosen again.
        if clip::throw(self.anim.name) as f32 - self.anim.frame <= 3. {
            self.spin_ik = true;
        }
        // Once the cap is away the other actions may cancel the spin
        // (`isEnableCancelGround`): jump, crouch, then the run.
        if self.winding {
            return;
        }
        if self.pre_jump > 0 {
            self.ground_jump(o);
        } else if o.crouch {
            self.start_squat();
        } else if self.anim_done(clip::throw(self.anim.name)) {
            // The state ends when its clip has, once the cap is away
            // (`exeSpinGround`), and the actor picks the next state
            // (`setNerveOnGround`): the run when the stick is held.
            if self.tilt >= STICK_DEADZONE {
                self.go(Move::Walk, "Move");
            } else {
                self.go(Move::Idle, "Wait");
            }
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
        self.facing = self.air.turn_facing(self.facing, push);
        let release = self.throw_frame(self.throwing.kind.unwrap_or(Throw::Forward), true);
        if step > 22 && !self.winding
            || !self.winding && step >= release + 6 && (self.pre_jump > 0 || o.crouch_pressed)
        {
            self.set(Move::Fall);
            self.spin_fall = true;
            self.gravity = t::GRAVITY_AIR;
            if o.crouch_pressed {
                self.start_hip_drop();
            }
        }
    }
    fn throw_launch(&self, kind: Throw, continuous: bool) -> Launch {
        Launch {
            front: self
                .throwing
                .dir
                .unwrap_or(self.facing)
                .with_y(0.)
                .normalize_or(Vec3::NEG_Z),
            kind,
            left: false,
            air: !self.grounded,
            continuous,
        }
    }
    /// `HackCap::startThrow`: the cap starts flying on the next frame.
    fn launch_cap(&mut self, kind: Throw, continuous: bool) {
        self.launch = Some(self.throw_launch(kind, continuous));
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
        if std::mem::take(&mut self.cap_put_on) {
            self.cap = None;
        }
        // A throw started last frame: the cap's throw start places it and
        // sets it flying; the throw nerve itself runs from the next frame.
        if let Some(launch) = self.launch.take() {
            self.winding = false;
            let cap = Cap::start(launch, self.pos, self.cap_serial + 1);
            self.cap_serial = cap.clip_serial();
            self.cap = Some(cap);
            return;
        }
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
                CapEvent::WallHit(strength) => self.events.push(Fx::CapWallHit(strength)),
                CapEvent::Staying => self.events.push(Fx::CapStay),
                CapEvent::StayStarted => self.events.push(Fx::CapStayStart),
                CapEvent::Trampled => self.events.push(Fx::CapBounce),
                CapEvent::LeftStay => self.events.push(Fx::CapReturn),
                CapEvent::Caught => {
                    self.catch_window = t::CAP_CATCH_WINDOW;
                    if !self.catch_reacts() {
                        // The cap goes on the head (`putOnCapPlayer`): no catch
                        // animation, hit reaction or effect. It is reported
                        // once more where this frame left it.
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
                        self.start_sub("CatchCap", 10., -1.);
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
        self.upper_catch
            .filter(|frame| *frame >= 0)
            .map(|frame| (self.catch_clip, frame as f32))
    }
    /// `PlayerJudgeCapCatchPop::judge`, asked by `tryChangeNerveFromAir`: in the
    /// catch window (`PlayerCounterAfterCapCatch::isCapCatch`), with the cap
    /// back on the head and no ground under the feet, a jump press pops the
    /// player up (`PlayerStateCapCatchPop`: `CapCatchPopPower`, then
    /// `CapCatchPopGravity`). Without the press the catch leaves the jump alone
    /// (recorded: cap-catch-air). The pop's clip and state label are guesses.
    fn try_cap_catch_pop(&mut self, o: &Orders) -> bool {
        if self.catch_window == 0 || !o.jump || self.grounded || !self.mode.airborne_jump() {
            return false;
        }
        self.catch_window = 0;
        self.v.y = t::CAP_CATCH_POP_POWER;
        self.gravity = t::CAP_CATCH_POP_GRAVITY;
        self.air.set_gravity(t::CAP_CATCH_POP_GRAVITY);
        self.play("JumpCapCatchCommon");
        true
    }
    /// The hit sensors' phase of the frame, after the cap moved and before the
    /// player does: the foot sensor against the hovering cap's `Trample` sensor
    /// (`PlayerActorHakoniwa::attackSensor` and `HackCap::receiveMsg`), found
    /// with the player where the last frame left him. When the cap takes the
    /// trample the jump nerve restarts inside this frame's update, so the
    /// jump's first step is this frame's.
    fn cap_sensor_phase(&mut self, o: &Orders) {
        if !self.can_trample() {
            return;
        }
        let Some(cap) = &mut self.cap else {
            return;
        };
        let foot = self.pos / UNIT + Vec3::Y * crate::cap::FOOT_SENSOR_Y;
        if !cap.take_trample(foot, o.throw_held, self.cap_jump_possible) {
            return;
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
    }
    /// The actor nerve was restarted by a hit sensor in this frame and has run
    /// its first step already.
    pub fn restarted_by_sensor(&self) -> bool {
        self.sensor_restart
    }
    /// The states the player's foot can trample the cap in
    /// (`PlayerActorHakoniwa::attackSensor`): not while swimming, on a pole,
    /// grabbing a ledge or ceiling, rolling, pounding, or spinning the cap in
    /// the air.
    fn can_trample(&self) -> bool {
        !matches!(
            self.mode,
            Move::Swim
                | Move::Pole
                | Move::Ledge
                | Move::GroundPound
                | Move::PoundLand
                | Move::Roll
                | Move::RollEnd
                | Move::RollJump
        ) && !(self.mode == Move::Throw && !self.grounded)
    }
    /// The jump off the cap (`PlayerStateJump::appear`, request 11, shared
    /// with the jump on an enemy): `TrampleHighJumpPower` with
    /// `TrampleHighGravity`, no extension, the ordinary jump clip, and the
    /// player's speed kept when the stick is pushed, scaled down when he was
    /// still rising, and dropped when it is not.
    fn trample_jump(&mut self) {
        self.combo_kept = true;
        let clip = if self.side_faces_camera() {
            "Jump"
        } else {
            "JumpReverse"
        };
        let falling = self.v.y <= 0.;
        let pushing = self.tilt >= STICK_DEADZONE;
        self.jump_with(
            Move::CapBounce,
            clip,
            t::TRAMPLE_HIGH_JUMP_POWER,
            t::TRAMPLE_HIGH_GRAVITY,
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
                // Guesses: the new jump may be left with a dive and the air
                // throw is available again.
                p.dive_possible = true;
            },
        );
    }

    // ---- Landing and collision ------------------------------------------------
    fn land(&mut self, o: &Orders) {
        if self.mode == Move::WallSlide {
            self.leave_wall_slide();
        }
        // A floor with the `Slide` code takes the actor to the slope slide before any landing
        // state (`setNerveOnGround`, `tryChangeNerveFromAir`).
        if self.try_force_slope_slide() {
            return;
        }
        // The jump state's last frame: its air control still steers, and a jump
        // that ends on the ground counts toward the combo (a fall does not).
        let air_step_ran = self.mode.uses_air_control() && self.air_for == Some(self.mode);
        if air_step_ran {
            let mut v = self.v;
            let steer = air_move::Steering {
                front: t::JUMP_ACCEL_FRONT,
                back: t::JUMP_ACCEL_BACK,
                turn: t::JUMP_ACCEL_TURN,
            };
            let push = self.push();
            self.air.update(&mut v, push, o.jump_held, &steer);
            self.set_h(v);
            self.facing = self.air.turn_facing(self.facing, push);
        }
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
        let speed = self.speed_h();
        self.land_frames = 0;
        self.dive_possible = true;
        if self.mode == Move::Dive {
            // `exeHeadSliding` clears the cap jump limit on every step it has
            // the ground (`clearLandLimitStandAngle`), the landing step included.
            self.cap_jump_possible = true;
        }
        self.gravity = t::GRAVITY_AIR;
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
                // Nothing has slowed the fall: the row still reports it.
                self.v.y = -t::HIP_DROP_SPEED;
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
            // The spin jump's drop (the `JumpSpinGroundDownFall` nerve, ZL held) ends on the
            // ground: `PlayerActorHakoniwa::exeJump` sets action trigger 11 for
            // `isEndJumpDownFallLand`, which makes `PlayerJudgeForceLand` true, so no run or
            // squat starts and the wait state appears in its `LandStiffen` nerve. That nerve
            // plays `LandDownFall` after a drop and raises no landing reaction.
            Move::SpinDrop => {
                self.go(Move::Idle, "LandDownFall");
                return;
            }
            // Holding crouch turns the momentum into a real roll; otherwise a
            // dive lands in its forward roll and stands up.
            Move::Dive | Move::LongJump if rolls => self.start_roll(RollStart::Landing),
            Move::RollJump if rolls => self.roll_land_from_jump(),
            // With the stick held the dive lands straight into the run, which
            // plays the roll's stand-up as an overlay on its legs.
            Move::Dive if self.is_move() => {
                self.run_frames = t::BRAKE_ON_COUNTER;
                // The run state pulls to the floor from the step it is chosen.
                self.pull = GRAVITY_MOVE;
                self.run_appear(true);
                self.go(Move::Run, "Move");
                self.cycle_restart_in = 2;
                self.sub_anim = Some(Anim {
                    name: "RollingStandUp",
                    serial: self.anim.serial.wrapping_add(7),
                    frame: 2.,
                    rate: 0.6,
                });
            }
            // The roll raises its own landing reaction when it has flown long
            // enough (`PlayerStateRolling::control`).
            Move::Roll => return,
            Move::SlopeSlide => {}
            // The leap lands in a bounce, keeping speed only along the stick.
            Move::BurnJump => {
                if self.tilt <= 0.05 {
                    self.set_h(Vec3::ZERO);
                }
                self.fire_bounce();
                return;
            }
            Move::BurnBounce => {
                if self.tilt <= 0.05 {
                    self.set_h(Vec3::ZERO);
                }
                self.start_fire_run();
                return;
            }
            Move::FireRun => {
                self.fire_run_landed();
                return;
            }
            _ => {
                // `PlayerActorHakoniwa::setNerveOnGround` asks its judges in
                // this order: the buffered jump, `PlayerJudgeStartRun` (which
                // the force-land judge vetoes), `PlayerJudgeStartSquat` (also
                // vetoed), then the wait state. So a stick held at touchdown
                // runs first and the run state starts the crouch one step
                // later.
                if self.pre_jump > 0 {
                    // A jump pressed just before landing goes straight on.
                    self.ground_jump(o);
                } else if !self.hard_land && self.tilt > 0.05 && speed > 0.5 {
                    self.run_frames = t::BRAKE_ON_COUNTER;
                    self.pull = GRAVITY_MOVE;
                    self.run_appear(true);
                    self.go(Move::Run, "Move");
                } else if !self.hard_land && o.crouch {
                    self.start_squat();
                } else if self.hard_land {
                    // `PlayerStateWait::appear` with the force-land judge true
                    // keeps the velocity along the floor and raises no landing
                    // reaction; the `LandStiffen` nerve's first step stops the
                    // player and raises the high-fall one.
                    self.change(Move::Idle, "LandStiffen", |p| {
                        p.set_h(Vec3::ZERO);
                        p.events.push(Fx::Land { hard: true });
                    });
                    return;
                } else {
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
                    self.change(Move::Idle, anim, |p| p.set_h(Vec3::ZERO));
                }
            }
        }
        self.events.push(Fx::Land {
            hard: self.hard_land,
        });
    }
    fn collide(&mut self, o: &Orders, scene: &Course) {
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
        }
        if self.mode == Move::WallSlide
            && (self.wall == Vec3::ZERO || self.grounded)
            && !self.grounded
        {
            // Lost the wall: keep falling.
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
                    let top = b.top(self.pos.x.clamp(lo.x, hi.x));
                    if axis == 1 {
                        if self.v.y <= 0.
                            && before.y >= b.top(before.x.clamp(lo.x, hi.x)) - 0.15
                            && self.pos.y < top + contact_reach
                            && self.pos.y + height > lo.y
                        {
                            self.pos.y = top;
                            self.landing_speed = self.landing_speed.min(self.v.y);
                            if self.mode != Move::SlopeSlide {
                                // (The slope slide keeps the game's own velocity, the pull into
                                // the floor included.)
                                self.v.y = 0.;
                            }
                            self.grounded = true;
                            self.slope = if (self.pos.x - b.center.x).abs() < b.half.x {
                                b.slope
                            } else {
                                0.
                            };
                            self.slick = b.slippery;
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

#[path = "tests/movement.rs"]
mod checks;
#[path = "tests/ledge.rs"]
mod ledge_checks;
#[path = "tests/spin.rs"]
mod spin_checks;
#[path = "tests/wall_air.rs"]
mod wall_air_checks;

pub fn self_check() {
    checks::run();
    spin_checks::run();
    wall_air_checks::run();
    ledge_checks::run();
    console_libm::self_check();
    turn_control::self_check();
    ground_move::self_check();
    move_input::self_check();
    wall_along::self_check();
    wall_slide::self_check();
    pose_math::self_check();
    look_joint::self_check();
    ground_pose::self_check();
    swim_pose::self_check();
    swim_move::self_check();
    pole_pose::self_check();
    cap_throw::self_check();
    hip_spring::self_check();
    look_at::self_check();
}
