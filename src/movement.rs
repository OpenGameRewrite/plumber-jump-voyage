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
use crate::geometry::TriangleMesh;
use crate::input::Orders;
use crate::nerve::Nerve;
use crate::se_ctrl::SeCtrl;
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
/// Radius of the course's climbable poles.
pub const POLE_RADIUS: f32 = 0.18;
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
    pub const GROUND_SPIN_FRAMES: u32 = 90;
    pub const GROUND_SPIN_MOVE_MAX: f32 = 8.;

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
    pub const SLOPE_SLIDE_START_SPEED: f32 = 5.;
    pub const SLOPE_SLIDE_SIDE_ACCEL: f32 = 0.6;
    pub const SLOPE_SLIDE_SIDE_BRAKE: f32 = 0.98;
    pub const SLOPE_SLIDE_SIDE_MAX: f32 = 10.;
    pub const SLOPE_TURN_DEG: f32 = 5.;
    pub const SLOPE_FORCE_FRAMES: u32 = 30;
    pub const SQUAT_BRAKE_END: f32 = 3.5;
    pub const SQUAT_ACCEL_RATE: f32 = 1.2;
    pub const SQUAT_BRAKE_RATE: f32 = 0.95;
    pub const SQUAT_WALK: f32 = 3.5;

    pub const WALL_SLIDE_GRAVITY: f32 = 0.5;
    pub const WALL_SLIDE_MAX: f32 = 10.;
    pub const WALL_KEEP_FRAMES: u32 = 3;
    pub const WALL_JUMP_POWER: f32 = 23.;
    pub const WALL_JUMP_GRAVITY: f32 = 0.95;
    pub const WALL_JUMP_H: f32 = 8.6;
    pub const WALL_JUMP_NO_INPUT: u32 = 25;
    pub const WALL_CATCH_EDGE_TOP: f32 = 120.;
    pub const WALL_CATCH_BOTTOM: f32 = 150.;
    pub const WALL_CLIMB_JUMP_V: f32 = 20.;
    pub const WALL_CLIMB_JUMP_H: f32 = 3.;
    pub const WALL_CLIMB_JUMP_GRAVITY: f32 = 1.5;

    pub const CAP_LEAPFROG_POWER: f32 = 32.;
    pub const CAP_LEAPFROG_POWER_AIR: f32 = 26.;
    pub const CAP_LEAPFROG_GRAVITY: f32 = 1.;
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

    pub const SWIM_GRAVITY: f32 = 0.25;
    pub const SWIM_FALL_MAX: f32 = 6.5;
    pub const SWIM_RISE_POWER: f32 = 0.6;
    pub const SWIM_RISE_MAX: f32 = 7.5;
    pub const SWIM_RISE_FRAMES: u32 = 10;
    pub const SWIM_ACCEL_H: f32 = 0.25;
    pub const SWIM_SPEED_MAX_H: f32 = 7.5;
    pub const SWIM_BRAKE_H: f32 = 0.975;
    pub const SWIM_DIVE_SPEED: f32 = 26.5;
    pub const SWIM_DIVE_BRAKE: f32 = 0.875;

    pub const POLE_UP_DIST: f32 = 100.;
    pub const POLE_UP_FRAMES: u32 = 25;
    pub const POLE_DOWN_SPEED: f32 = 10.;
    pub const POLE_DOWN_SPEED_FAST: f32 = 15.;
    pub const POLE_CATCH_RANGE: f32 = 50.;
    pub const POLE_TURN_DIST: f32 = 40.;
    pub const POLE_TURN_FRAMES: f32 = 15.;
    pub const POLE_UP_MARGIN: f32 = 40.;
    /// The ledge-hang clips hang the body 141 below the actor, which sits on the edge.
    pub const WALL_CATCH_HANG: f32 = 141.;
    pub const WALL_CATCH_INSET: f32 = 8.;
    pub const WALL_CATCH_MOVE: f32 = 70.;
    pub const WALL_CATCH_MOVE_FRAMES: f32 = 20.;
}

#[path = "movement/ground.rs"]
mod ground;
#[path = "movement/rolling.rs"]
mod rolling;
#[path = "movement/run_cycle.rs"]
mod run_cycle;
#[path = "movement/sounds.rs"]
mod sounds;
#[path = "movement/squat.rs"]
mod squat;
#[path = "movement/terrain.rs"]
mod terrain;
#[path = "movement/wait.rs"]
mod wait;
pub use ground::turn_power;
use ground::{GRAVITY_MOVE, GroundMove, PivotTurn, STICK_DEADZONE, SteerInput};
use rolling::{RollControl, RollStart};
pub use run_cycle::RunCycle;
use squat::SquatControl;
use wait::WaitControl;

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
        (Throw::Forward, true, 2, false) => "SpinCapAirStart2Right",
        (Throw::Forward, true, _, _) => "SpinCapAirStart3Right",
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
            "SpinCapAirStart2Right" | "SpinCapAirStart3Right" => 72,
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
            _ => None,
        }
    }
    pub const LAND: u32 = 26;
    pub const LAND_JUMP3: u32 = 70;
    pub const SQUAT_END: u32 = 13;
    pub const TURN: u32 = 12;
    pub const WALL_CATCH_START: u32 = 55;
    pub const WALL_CATCH_END: u32 = 46;
    pub const POLE_CATCH: u32 = 8;
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
    SwimDash,
    CapBounce,
    CapVault,
    CatchJump,
    Throw,
}
impl Move {
    /// Every state, for mapping names back to states.
    pub const ALL: [Move; 39] = [
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
        Self::SwimDash,
        Self::CapBounce,
        Self::CapVault,
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
            Self::SwimDash => "Swim dash",
            Self::CapBounce => "Cap jump",
            Self::CapVault => "Cap vault",
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
                | Self::CapBounce
                | Self::CapVault
                | Self::CatchJump
                | Self::Fall
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
                | Self::CapVault
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
    RollBoost {
        perfect: bool,
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
    CapBounce,
    WallSlide,
    Ledge,
    Pole,
    Splash,
    /// Hot ground: the leap (false) or the bounce where it lands (true).
    Burned(bool),
    /// Water put the burn out.
    Quenched,
    SwimStroke,
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

#[derive(Debug, Default)]
struct ClimbControl {
    base: Vec3,
    height: f32,
    frame: u32,
    cooldown: u32,
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

#[derive(Component, Debug)]
pub struct Player {
    /// State changes decided last step, waiting for their first step.
    nerve: Nerve,
    fire: FireControl,
    climbing: ClimbControl,
    rolling: RollControl,
    squat: SquatControl,
    /// The standing state's idle sequence (`PlayerStateWait`).
    wait: WaitControl,
    throwing: ThrowControl,
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
    /// The last number given to a clip start of the cap (they count on).
    cap_serial: u32,
    /// The cap is Mario's spin attack, waiting for the throw frame
    /// (the cap's `SpinAttack` nerve).
    winding: bool,
    /// The cap reached the head this frame; its landing work is left.
    caught: bool,
    /// The air throw is over but its state runs on: the fall of the spin cap
    /// state (`PlayerStateSpinCap`'s `Fall` nerve) until the player lands.
    spin_fall: bool,
    /// Frames of the cap's catch nerve: -1 on the frame it arrived.
    catching: Option<i32>,
    /// The sub animation the model plays, and whether the main clip's next
    /// update is skipped (a clip just started under a sub animation).
    sub: SubClip,
    anim_skip: bool,
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
    coyote: u32,
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
    air_throw_used: bool,
    dive_possible: bool,
    gravity: f32,
    jump_extend: bool,
    max_air_speed: f32,
    peak: f32,
    spin: u32,
    spin_turn: f32,
    last_stick: Vec2,
    stick: Vec3,
    tilt: f32,
    slope: f32,
    slick: bool,
    ledge: Vec3,
    /// Pole axis base, its height, the climb cycle in progress and a re-grab delay.
    /// Frames since the burn's bounce landed (run and falls together).
    /// Burn leap: horizontal direction its air control works along.
    /// Burn leap frames left without gravity.
    /// Fire run frames since the stick was released, capped at the brake time.
    swim_rise: u32,
    step_phase: f32,
    /// Fall speed the ground stopped in the latest step (zero when it did not
    /// land), kept for the trace: the original still reports it that frame.
    landing_speed: f32,
    /// The fall speed of the step that reached the ground, which the step that
    /// decides the landing (the next one) still knows.
    touchdown_speed: f32,
    /// The run cycle's animation control (the `Move` slot).
    cycle: RunCycle,
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
    /// The main clip looped back to its start in this step.
    pub clip_wrapped: bool,
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
            climbing: ClimbControl::default(),
            rolling: RollControl::default(),
            squat: SquatControl::default(),
            wait: WaitControl::default(),
            throwing: ThrowControl::default(),
            pos: SPAWN,
            vel: Vec3::ZERO,
            facing: Vec3::NEG_Z,
            mode: Move::Idle,
            grounded: true,
            cap: None,
            launch: None,
            cap_serial: 0,
            winding: false,
            caught: false,
            spin_fall: false,
            catching: None,
            sub: SubClip::default(),
            anim_skip: false,
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
            coyote: 0,
            combo: ContinuousJump::default(),
            long_combo: ContinuousLongJump::default(),
            air: AirMove::default(),
            air_for: None,
            jump_power: 0.,
            combo_kept: false,
            prior_v: Vec3::ZERO,
            ground_height: f32::MAX,
            camera_yaw: 0.,
            jump_held_frames: 0,
            run_frames: 0,
            after_turn: 0,
            quick_turn: 0,
            catch_window: 0,
            cap_jump_possible: true,
            air_throw_used: false,
            dive_possible: true,
            gravity: t::GRAVITY_AIR,
            jump_extend: false,
            max_air_speed: t::JUMP_MOVE_MIN,
            peak: SPAWN.y,
            spin: 0,
            spin_turn: 0.,
            last_stick: Vec2::ZERO,
            stick: Vec3::ZERO,
            tilt: 0.,
            slope: 0.,
            slick: false,
            ledge: Vec3::ZERO,
            swim_rise: 0,
            step_phase: 0.,
            landing_speed: 0.,
            touchdown_speed: 0.,
            cycle: RunCycle::default(),
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
#[derive(Clone)]
pub struct Pole {
    pub center: Vec3,
    pub height: f32,
}
#[derive(Resource, Default, Clone)]
pub struct Course {
    pub solids: Vec<Solid>,
    pub poles: Vec<Pole>,
    pub water: Option<(Vec3, Vec3)>,
    /// Hot surfaces (lava): centre of the top face and half extents.
    pub hot: Vec<(Vec3, Vec3)>,
    /// Shallow water to stand in: centre of the surface and half extents.
    pub puddles: Vec<(Vec3, Vec3)>,
    pub targets: Vec<Vec3>,
    /// The real game's stage collision, when a replay loads one. It takes the
    /// place of the solids for the player's contact.
    pub mesh: Option<Arc<TriangleMesh>>,
}
impl Course {
    pub fn wet(&self, pos: Vec3) -> bool {
        self.water.is_some_and(|(c, h)| {
            (pos.x - c.x).abs() < h.x && (pos.z - c.z).abs() < h.z && pos.y < c.y
        })
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
    /// A player standing at `pos` (metres) facing the horizontal direction `facing`.
    pub fn spawn_at(pos: Vec3, facing: Vec3) -> Self {
        Self {
            pos,
            prev: pos,
            shown: pos,
            peak: pos.y,
            checkpoint: pos,
            facing: facing.with_y(0.).normalize_or(Vec3::NEG_Z),
            ..Self::default()
        }
    }
    /// Velocity in original units per frame, after the last step. On the
    /// ground the controller cancels the downward push inside the step; the
    /// original keeps reporting it, so it is shown as the gravity pull. In the
    /// step that lands, it still reports the fall speed that moved the player.
    pub fn units_per_frame(&self) -> Vec3 {
        if self.grounded && (self.v.y == 0. || self.v.y == -self.pull) {
            let down = if self.landing_speed < 0. {
                self.landing_speed
            } else {
                -self.pull
            };
            self.v.with_y(down)
        } else {
            self.v
        }
    }
    /// The clip frame animation triggers (sounds, effects) are scanned up to.
    /// While a sub animation plays the same clip, the model's own action
    /// frame decides, not the animator's counter.
    pub fn trigger_frame(&self) -> f32 {
        match self.model_frame() {
            Some(frame) => frame,
            None => self.anim.trigger_frame(),
        }
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
                pos: self.pos,
                clip: "CatchCap",
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
            ("coyote", self.coyote as i64),
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
            ("air_throw_used", self.air_throw_used as i64),
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
            "coyote" => self.coyote = frames,
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
            "air_throw_used" => self.air_throw_used = value != 0,
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
        if let Some((name, frame)) = s.anim {
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
        self.peak = if self.grounded {
            self.pos.y
        } else {
            self.peak.max(self.pos.y)
        };
    }
    /// The cap and the spin throw as a recorded row shows them.
    fn resume_cap(&mut self, s: &Snapshot) {
        let previous = self.cap.take();
        self.launch = None;
        self.winding = false;
        self.winding_age = 0;
        self.caught = false;
        self.catching = s.catching;
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
    pub fn reset(&mut self) {
        let checkpoint = self.checkpoint;
        *self = Self::default();
        self.pos = checkpoint;
        self.peak = checkpoint.y;
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
        crate::game_names::nerve(self.mode, self.clip())
    }
    /// The actor nerve of that state (the part before the sub-state).
    fn actor_name(&self) -> &'static str {
        let name = self.nerve_name().name;
        name.split('/').next().unwrap_or(name)
    }
    /// Ask for a clip unless the state is already playing or about to play it.
    fn keep(&mut self, name: &'static str) {
        if self.clip() != name {
            self.play(name);
        }
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
            "Land" | "LandTurn" | "LandStiffen" | "LandDownFall" => clip::LAND,
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
                self.anim.frame += self.anim.rate;
            }
            // A jump clip that has played out holds a frame past its end.
            if let Some(length) = clip::held_length(self.anim.name) {
                self.anim.frame = self.anim.frame.min(length as f32 + 1.);
            }
            if self.anim.name == "FireRun" && self.anim.frame >= clip::FIRE_RUN {
                self.anim.frame -= clip::FIRE_RUN;
            }
            if let Some(length) = clip::loop_length(self.anim.name)
                && self.anim.frame >= length
            {
                self.anim.frame -= length;
                self.clip_wrapped = true;
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
        self.roll_forget_effect_call();
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
        self.hop_contact = false;
        self.nerve.deferred = false;
        self.nerve.renewed = false;
        let was_grounded = self.grounded;
        // The game animates first, then the state runs; a state chosen last
        // frame starts now.
        // The cap updates before the player does, within the same frame.
        self.update_cap(&o, scene);
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
        }
        self.finish_catch();
        if matches!(self.mode, Move::Ledge | Move::Pole) {
            self.finish_frame(scene);
            return;
        }
        self.grab_pole(scene);
        if self.mode == Move::Pole {
            self.finish_frame(scene);
            return;
        }
        self.cap_contact();
        self.collide(&o, scene);
        // Hot ground takes over any landing; shallow water puts a burn out.
        let hot = self.grounded && scene.hot(self.pos);
        let quench = self.grounded && !hot && self.burning() && scene.puddle(self.pos);
        if quench {
            self.events.push(Fx::Quenched);
            self.end_burn();
        } else if self.grounded && !was_grounded && !hot {
            // The contact is flagged now; the next step decides what it means.
            self.nerve.touchdown = true;
            self.touchdown_speed = self.landing_speed;
        }
        if hot {
            self.burn();
        } else if !self.grounded
            && was_grounded
            && self.v.y <= 0.
            && matches!(
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
                    | Move::Throw
            )
        {
            // Walked off an edge: likewise, the fall is chosen next step.
            self.nerve.liftoff = true;
        }
        self.finish_frame(scene);
    }
    /// Changes of state the surroundings force, decided before the state runs:
    /// water, the ground reached or lost last step.
    fn change_from_surroundings(&mut self, scene: &Course, o: &Orders) {
        let wet = scene.wet(self.pos + Vec3::Y * 0.8);
        if wet && !matches!(self.mode, Move::Swim | Move::SwimDash) {
            if self.burning() {
                self.events.push(Fx::Quenched);
            }
            self.events.push(Fx::Splash);
            self.v *= 0.3;
            self.nerve.touchdown = false;
            self.go(Move::Swim, "SwimFallIn");
        } else if !wet && matches!(self.mode, Move::Swim | Move::SwimDash) {
            self.go(Move::Fall, "Fall");
        } else if std::mem::take(&mut self.nerve.touchdown) {
            self.land(o);
        } else if std::mem::take(&mut self.nerve.liftoff) {
            self.coyote = 3;
            self.go(Move::Fall, "Fall");
        }
    }
    /// Run the nerve of the current state for this frame.
    fn run_state(&mut self, o: &Orders, scene: &Course) {
        self.ground_height = scene
            .ground(self.pos)
            .map_or(f32::MAX, |floor| (self.pos.y - floor) / UNIT);
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
            mode if mode.uses_air_control() => self.jump_air(o),
            Move::WallSlide => self.wall_slide(o),
            Move::Ledge => self.wall_catch(o, scene),
            Move::Pole => self.pole_climb(o, scene),
            Move::Swim | Move::SwimDash => self.swim(o),
            Move::SpinDrop => self.spin_drop(o),
            _ => self.air(o),
        }
    }
    fn read_input(&mut self, o: &Orders) {
        self.camera_yaw = o.camera_yaw;
        let stick = Vec3::new(o.direction.x, 0., o.direction.y).clamp_length_max(1.);
        self.tilt = stick.length();
        self.stick = stick.normalize_or_zero();
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
        // Circling the stick arms a spin, as in the original's spin jump/throw.
        if o.direction.length_squared() > 0.25 && self.last_stick.length_squared() > 0.25 {
            let angle = self.last_stick.angle_to(o.direction);
            if angle.abs() < 1.2
                && (self.spin_turn == 0. || angle.signum() == self.spin_turn.signum())
            {
                self.spin_turn += angle;
            } else {
                self.spin_turn = 0.;
            }
            if self.spin_turn.abs() > 5.5 {
                self.spin = 30;
                self.spin_turn = 0.;
            }
        } else {
            self.spin_turn = 0.;
        }
        if o.spin_throw {
            self.spin = 30;
        }
        self.spin = self.spin.saturating_sub(1);
        self.last_stick = o.direction;
        self.catch_window = self.catch_window.saturating_sub(1);
        self.climbing.cooldown = self.climbing.cooldown.saturating_sub(1);
        self.after_turn = self.after_turn.saturating_sub(1);
        self.quick_turn = self.quick_turn.saturating_sub(1);
        self.throwing.run = self.throwing.run.saturating_sub(1);
        self.coyote = self.coyote.saturating_sub(1);
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
        self.update_sounds();
        self.timer = self.frame as f32 / FPS;
        self.since_land = self.land_frames as f32 / FPS;
        if self.grounded {
            self.peak = self.pos.y;
        } else {
            self.peak = self.peak.max(self.pos.y);
        }
        if self.pos.y < -18. || !self.pos.is_finite() || !self.v.is_finite() {
            self.reset();
        }
    }

    // ---- Ground movement ------------------------------------------------
    /// Shared ground transitions. Returns true when a new state took over.
    fn ground_actions(&mut self, o: &Orders) -> bool {
        let speed = self.speed_h();
        if self.pre_jump > 0 {
            self.ground_jump(o);
            return true;
        }
        if o.crouch && self.steep(t::SLOPE_SLIDE_ANGLE_START) {
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
        if self.spin > 0 && self.tilt > 0.5 && self.mode != Move::GroundSpin {
            self.spin = 0;
            self.go(
                Move::GroundSpin,
                if self.spin_turn >= 0. {
                    "SpinGroundL"
                } else {
                    "SpinGroundR"
                },
            );
            return true;
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
            self.stand(o);
        } else {
            self.run_step(o);
        }
    }
    /// Standing: the stick starts the run (the start-run judge), which is a
    /// new nerve, chosen now and moving from its first step.
    fn stand(&mut self, o: &Orders) {
        if self.ground_actions(o) {
            return;
        }
        if self.tilt >= STICK_DEADZONE {
            // The run state's appearance already pulls the player to the floor.
            self.pull = GRAVITY_MOVE;
            self.go(Move::Walk, "Move");
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
        self.v.y = -t::GRAVITY;
    }
    /// One step of the run nerve (`PlayerStateRunHakoniwa::exeRun`): the
    /// ground move control steers, the run cycle animates, then the nerve
    /// decides whether to brake, pivot or end.
    fn run_step(&mut self, o: &Orders) {
        let step = self.run_steps;
        if step == 0 {
            self.ground.reset(self.v, self.facing);
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
            velocity: self.v,
            front: self.facing,
            stick: self.ground_stick(),
            max_speed,
            accel_frames,
        });
        self.run_steps += 1;
        if steer.front.length_squared() > 0.5 {
            self.facing = steer.front;
        }
        self.set_h(steer.velocity);
        self.v.y = -GRAVITY_MOVE;
        self.pull = GRAVITY_MOVE;
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
        // The actor's judges (jump, crouch, throw) look at the state's result.
        if !ended {
            self.ground_actions(o);
        }
    }
    /// The first step of one of the run state's nerves.
    fn enter_nerve(&mut self, nerve: RunNerve) {
        self.run_nerve = nerve;
        self.run_steps = 0;
        match nerve {
            RunNerve::Brake => self.events.push(Fx::Skid),
            RunNerve::Turn => {
                // The turn faces the other way at once and slides on.
                self.facing = -self.brake_dir;
                self.events.push(Fx::TurnDust);
            }
            RunNerve::Pivot => {
                let front = self.facing;
                self.pivot.reset(front);
            }
            RunNerve::Run | RunNerve::RunAfterTurn => {}
        }
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
        // The actor's judges look at the state's result: the brake of this
        // frame has already slowed the player when a turning jump is chosen.
        if self.pre_jump > 0 && self.quick_turn > 0 {
            self.side_somersault();
        } else if self.pre_jump > 0 {
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
    fn ground_spin(&mut self, o: &Orders) {
        if self.pre_jump > 0 {
            self.spin = 30;
            self.ground_jump(o);
            return;
        }
        if self.pre_throw > 0 && self.cap.is_none() {
            self.pre_throw = 0;
            self.start_throw(Throw::Spin);
            return;
        }
        let speed = self.speed_h();
        let s = if self.tilt > 0.05 {
            self.facing = turn(self.facing, self.stick, 0.2);
            (speed + 0.5).min(t::GROUND_SPIN_MOVE_MAX)
        } else {
            speed * 0.95
        };
        self.set_h(self.facing * s);
        self.v.y = -t::GRAVITY;
        if self.frame >= 20 && self.tilt < 0.05 || self.frame >= t::GROUND_SPIN_FRAMES {
            self.go(Move::Idle, "Wait");
        }
    }

    // ---- Squat, roll, slides --------------------------------------------
    /// Ground at least this many degrees steep.
    fn steep(&self, degrees: f32) -> bool {
        self.grounded && self.slope.abs().atan().to_degrees() >= degrees
    }
    fn start_slope_slide(&mut self) {
        self.change(Move::SlopeSlide, "Slide", |p| {
            let down = Vec3::new(-p.slope.signum(), 0., 0.);
            let mut h = p.v.with_y(0.);
            // Start at least gently downhill, never faster than the slide's limit.
            let along = h.dot(down);
            if along < t::SLOPE_SLIDE_START_SPEED {
                h += down * (t::SLOPE_SLIDE_START_SPEED - along);
            }
            p.set_h(h.clamp_length_max(t::SLOPE_SLIDE_MAX));
        });
    }
    /// Sliding down a slope on the seat: speeds up downhill, steers a little,
    /// and brakes to a stop once the ground flattens out.
    fn slope_slide(&mut self, o: &Orders) {
        if self.pre_jump > 0 {
            self.ground_jump(o);
            return;
        }
        if o.crouch && o.throw_pressed && self.grounded {
            self.pre_throw = 0;
            self.start_roll(RollStart::Normal);
            return;
        }
        if !self.grounded {
            self.v.y = (self.v.y - t::GRAVITY_AIR).max(-t::FALL_MAX);
            return;
        }
        let h = self.v.with_y(0.);
        let mut sliding = h;
        if self.steep(t::SLOPE_SLIDE_ANGLE_END) {
            let down = Vec3::new(-self.slope.signum(), 0., 0.);
            let side_dir = Vec3::Y.cross(down);
            let mut along = h.dot(down);
            let mut side = h.dot(side_dir);
            if along < 0. {
                along *= t::SLOPE_SLIDE_BRAKE;
            }
            along = (along + t::SLOPE_SLIDE_ACCEL).min(t::SLOPE_SLIDE_MAX);
            side *= t::SLOPE_SLIDE_SIDE_BRAKE;
            if self.tilt > 0.05 {
                side = (side + self.stick.dot(side_dir) * self.tilt * t::SLOPE_SLIDE_SIDE_ACCEL)
                    .clamp(-t::SLOPE_SLIDE_SIDE_MAX, t::SLOPE_SLIDE_SIDE_MAX);
            }
            sliding = down * along + side_dir * side;
        } else {
            sliding *= t::SLOPE_SLIDE_BRAKE;
            if sliding.length() <= t::SLOPE_SLIDE_SPEED_END && self.frame >= t::SLOPE_FORCE_FRAMES {
                if o.crouch {
                    self.start_squat();
                } else {
                    self.go(Move::Idle, "Wait");
                }
                return;
            }
        }
        let sliding = sliding.clamp_length_max(t::SLOPE_SLIDE_MAX);
        if sliding.length() > 0.5 {
            self.facing = turn(
                self.facing,
                sliding.normalize(),
                t::SLOPE_TURN_DEG.to_radians(),
            );
        }
        self.set_h(sliding);
        self.v.y = -t::GRAVITY;
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
        self.coyote = 0;
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
            p.leave_ground(mode, power, gravity);
            then(p);
            p.arm_air_control(mode, power, gravity);
        });
    }
    fn leave_ground(&mut self, mode: Move, power: f32, gravity: f32) {
        self.v.y = power;
        self.gravity = gravity;
        self.jump_extend = true;
        self.max_air_speed = self.speed_h().clamp(t::JUMP_MOVE_MIN, t::JUMP_MOVE_MAX);
        self.grounded = false;
        self.coyote = 0;
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
        let takeoff = jump_takeoff(extend, power, gravity);
        self.air.setup(&mut self.v, self.facing, &takeoff);
        self.air_for = Some(mode);
        self.jump_power = power;
    }
    /// A state flown by the air control that no take-off armed (a fall, or a
    /// recorded state being replayed): set it up from the velocity it has.
    fn arm_fall_control(&mut self) {
        let takeoff = fall_takeoff(self.v.y);
        self.air.setup(&mut self.v, self.facing, &takeoff);
        self.air_for = Some(self.mode);
    }
    /// Put the air control into the middle of a recorded jump (lockstep).
    fn resume_air_control(&mut self, s: &Snapshot) {
        let frames = s.state_frames.unwrap_or(0);
        let (extend, gravity) = match self.mode {
            Move::DoubleJump => (t::EXTEND_FRAMES, t::JUMP_GRAVITY_2ND),
            Move::TripleJump => (t::EXTEND_FRAMES, t::JUMP_GRAVITY_3RD),
            Move::Jump => (t::EXTEND_FRAMES, t::JUMP_GRAVITY),
            Move::CapBounce | Move::CapVault => (0, t::CAP_LEAPFROG_GRAVITY),
            Move::CatchJump => (0, t::CAP_CATCH_JUMP_GRAVITY),
            _ => (0, t::JUMP_GRAVITY),
        };
        let takeoff = if self.mode == Move::Fall {
            fall_takeoff(self.v.y)
        } else {
            jump_takeoff(extend, self.v.y, gravity)
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
        self.jump_with(mode, clip, power, gravity, |_| {});
    }
    fn ground_jump(&mut self, _o: &Orders) {
        if self.mode == Move::PoundLand && self.pound_jump_permitted() {
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
        if self.spin > 0 {
            self.spin = 0;
            self.jump_with(
                Move::SpinJump,
                "SpinJumpStart",
                t::SPIN_JUMP_POWER,
                t::SPIN_JUMP_GRAVITY,
                |p| {
                    p.jump_extend = false;
                    p.max_air_speed = t::SPIN_JUMP_MOVE_MAX;
                },
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
        let steer = air_move::Steering {
            front: t::JUMP_ACCEL_FRONT,
            back: t::JUMP_ACCEL_BACK,
            turn: t::JUMP_ACCEL_TURN,
        };
        let push = self.push();
        self.air.update(&mut self.v, push, o.jump_held, &steer);
        if self.tilt > 0.05 {
            // A stick more than `JumpTurnAngleFast` from the facing swings it
            // by up to `JumpTurnAngleFastLimit` a frame (the recorded air
            // throw with the stick pulled back turned exactly 25 degrees).
            let apart = wrap(yaw(self.stick) - yaw(self.facing)).abs().to_degrees();
            let limit = if apart > t::JUMP_TURN_FAST_DEG {
                t::JUMP_TURN_FAST_LIMIT_DEG.to_radians()
            } else {
                0.1
            };
            self.facing = turn(self.facing, self.stick, limit);
        }
        self.air_actions(o);
    }
    /// Free flight of the states that keep the older approximation of the
    /// air control: the spin jump, the roll jump and the wall jump.
    fn air(&mut self, o: &Orders) {
        if self.air_actions(o) {
            return;
        }
        let mut h = self.v.with_y(0.);
        let speed = h.length();
        let locked = self.mode == Move::WallJump && self.frame < t::WALL_JUMP_NO_INPUT;
        if self.tilt > 0.05 && !locked {
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
        } else if !locked {
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
        let max = if self.mode == Move::SpinJump && self.v.y < 0. {
            4.
        } else {
            t::FALL_MAX
        };
        self.v.y = self.v.y.max(-max);
        if self.v.y < 0. && self.gravity != t::GRAVITY_AIR && !matches!(self.mode, Move::SpinJump) {
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
            "WallJump" => Some(37),
            "JumpCapLeapFrog" => Some(clip::JUMP_CAP_LEAP_FROG),
            "JumpCapCatchCommon" => Some(27),
            n if n.starts_with("SpinCapAirStart") => Some(clip::throw(n)),
            _ => None,
        };
        if ends.is_some_and(|n| self.anim_done(n)) && self.v.y < 0. {
            self.play("Fall");
        }
        if self.mode == Move::SpinJump && self.anim.name == "SpinJumpStart" && self.anim_done(26) {
            self.play("SpinJumpLoop");
        }
    }
    /// Airborne transitions shared by every free-flight state.
    fn air_actions(&mut self, o: &Orders) -> bool {
        if self.coyote > 0 && self.pre_jump > 0 {
            self.coyote = 0;
            self.ground_jump(o);
            return true;
        }
        if (self.pre_throw > 0 || o.throw_pressed) && o.crouch && self.dive_possible {
            self.pre_throw = 0;
            self.dive();
            return true;
        }
        if o.crouch_pressed && !matches!(self.mode, Move::Throw) {
            if self.mode == Move::SpinJump {
                self.change(Move::SpinDrop, "SpinJumpDownFall", |p| {
                    p.v = Vec3::NEG_Y * t::SPIN_DOWN_FALL_INIT;
                });
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
            p.events.push(Fx::PoundStart);
        });
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
        let can_move = self.frame >= t::HIP_DROP_LAND_CANCEL;
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

    // ---- Walls, ledges, poles --------------------------------------------
    fn wall_slide(&mut self, o: &Orders) {
        if self.pre_jump > 0 {
            self.facing = self.wall;
            let away = self.wall;
            self.jump_with(
                Move::WallJump,
                "WallJump",
                t::WALL_JUMP_POWER,
                t::WALL_JUMP_GRAVITY,
                move |p| {
                    p.set_h(away * t::WALL_JUMP_H);
                    p.max_air_speed = t::WALL_JUMP_H;
                    p.jump_extend = false;
                    p.dive_possible = true;
                },
            );
            return;
        }
        if o.crouch_pressed || self.wall == Vec3::ZERO {
            self.pos += self.wall * 0.1;
            self.go(Move::Fall, "Fall");
            return;
        }
        if self.frame >= t::WALL_KEEP_FRAMES {
            self.keep("WallSlideL");
        }
        self.set_h(-self.wall * 2.);
        self.v.y = (self.v.y - t::WALL_SLIDE_GRAVITY).max(-t::WALL_SLIDE_MAX);
    }
    /// Hanging from a ledge. The actor sits on the edge itself; the hang and
    /// climb clips carry the body below and up over it, as in the original.
    fn wall_catch(&mut self, o: &Orders, scene: &Course) {
        self.v = Vec3::ZERO;
        self.pos = self.ledge;
        let toward = self.stick.dot(-self.wall) * self.tilt;
        let right = self.facing.cross(Vec3::Y);
        let along = self.stick.dot(right) * self.tilt;
        if self.clip() == "WallCatchStart" && self.anim_done(clip::WALL_CATCH_START) {
            self.play("WallCatch");
        }
        if self.clip() == "WallCatchEnd" {
            if self.anim_done(clip::WALL_CATCH_END) {
                self.grounded = true;
                self.go(Move::Idle, "Wait");
            }
            return;
        }
        let hang_out = self.wall * (RADIUS + 0.05 + t::WALL_CATCH_INSET * UNIT);
        if o.crouch_pressed || toward < -0.4 {
            self.pos = self.ledge + hang_out - Vec3::Y * t::WALL_CATCH_HANG * UNIT;
            self.go(Move::Fall, "Fall");
        } else if self.frame > 8 && self.pre_jump > 0 {
            let back = -self.wall;
            self.jump_with(
                Move::Jump,
                "WallCatchEndJump",
                t::WALL_CLIMB_JUMP_V,
                t::WALL_CLIMB_JUMP_GRAVITY,
                move |p| {
                    p.set_h(back * t::WALL_CLIMB_JUMP_H);
                    p.jump_extend = false;
                },
            );
        } else if self.frame > 8 && toward > 0.5 {
            self.play("WallCatchEnd");
        } else if self.frame > 8 && along.abs() > 0.5 {
            // Shimmy along the edge while it continues.
            let step =
                right * along.signum() * t::WALL_CATCH_MOVE / t::WALL_CATCH_MOVE_FRAMES * UNIT;
            let next = self.ledge + step;
            if scene
                .ground(next + Vec3::Y * 0.02)
                .is_some_and(|g| (g - self.ledge.y).abs() < 0.05)
            {
                self.ledge = next;
                self.pos = next;
                self.keep(if along > 0. {
                    "WallCatchMoveR"
                } else {
                    "WallCatchMoveL"
                });
            }
        } else if self.anim.name.starts_with("WallCatchMove") {
            self.play("WallCatch");
        }
    }
    fn grab_pole(&mut self, scene: &Course) {
        if self.grounded
            || self.climbing.cooldown > 0
            || matches!(
                self.mode,
                Move::Pole | Move::GroundPound | Move::Dive | Move::Swim | Move::SwimDash
            )
        {
            return;
        }
        for pole in &scene.poles {
            let d = (self.pos - pole.center).with_y(0.);
            let top = pole.center.y + pole.height - t::POLE_UP_MARGIN * UNIT;
            if d.length() < POLE_RADIUS + t::POLE_CATCH_RANGE * UNIT
                && (pole.center.y..top).contains(&self.pos.y)
            {
                // Holding on is physical and immediate; the catch clip and its
                // sound are the state's first step.
                self.change(Move::Pole, "PoleCatch", |p| p.events.push(Fx::Pole));
                self.climbing.base = pole.center;
                self.climbing.height = pole.height;
                self.climbing.frame = 0;
                self.facing = (-d).normalize_or(self.facing);
                // The actor sits on the pole's axis; the clips hang the body behind it.
                self.pos = Vec3::new(pole.center.x, self.pos.y, pole.center.z);
                self.v = Vec3::ZERO;
                return;
            }
        }
    }
    fn pole_climb(&mut self, o: &Orders, scene: &Course) {
        self.v = Vec3::ZERO;
        let axis = Vec3::new(self.climbing.base.x, self.pos.y, self.climbing.base.z);
        // Climbing stops below the top margin, or under anything the pole passes through.
        let ceiling = scene
            .solids
            .iter()
            .filter(|b| {
                (axis.x - b.center.x).abs() < b.half.x + 0.3
                    && (axis.z - b.center.z).abs() < b.half.z + 0.3
                    && b.center.y - b.half.y > self.pos.y
            })
            .map(|b| b.center.y - b.half.y - 0.8)
            .fold(f32::MAX, f32::min);
        let (bottom, top) = (
            self.climbing.base.y,
            (self.climbing.base.y + self.climbing.height - t::POLE_UP_MARGIN * UNIT - 0.3)
                .min(ceiling),
        );
        let clear = POLE_RADIUS + RADIUS + 0.05;
        if self.pre_jump > 0 {
            // Kick off away from the pole.
            let away = -self.facing;
            self.facing = away;
            self.pos = axis + away * clear;
            self.climbing.cooldown = 20;
            self.jump_with(
                Move::Jump,
                "Jump",
                t::WALL_JUMP_POWER,
                t::WALL_JUMP_GRAVITY,
                move |p| {
                    p.set_h(away * t::WALL_JUMP_H);
                    p.jump_extend = false;
                },
            );
            return;
        }
        if o.crouch_pressed {
            self.pos = axis - self.facing * clear;
            self.climbing.cooldown = 20;
            self.go(Move::Fall, "Fall");
            return;
        }
        if self.clip() == "PoleCatch" && !self.anim_done(clip::POLE_CATCH) {
            return;
        }
        // Screen up climbs, down slides, left and right swing round the pole.
        let (stick, yaw) = if o.stick == Vec2::ZERO {
            (o.direction, 0.)
        } else {
            (o.stick, o.camera_yaw)
        };
        let toward = -stick.y;
        let right = crate::input::rotate(Vec2::new(stick.x, 0.), yaw);
        let side = Vec3::new(right.x, 0., right.y);
        if self.climbing.frame > 0 || toward > 0.5 && self.pos.y < top && toward > stick.x.abs() {
            if self.climbing.frame == 0 {
                self.climbing.frame = t::POLE_UP_FRAMES;
                self.play("PoleClimb");
            }
            self.climbing.frame -= 1;
            self.pos.y += t::POLE_UP_DIST / t::POLE_UP_FRAMES as f32 * UNIT;
            if self.pos.y >= top {
                self.pos.y = top;
                self.climbing.frame = 0;
            }
        } else if toward < -0.5 && -toward > stick.x.abs() {
            if self.pos.y <= bottom + 0.01 {
                self.pos = axis - self.facing * clear;
                self.climbing.cooldown = 20;
                self.grounded = true;
                self.go(Move::Idle, "Wait");
                return;
            }
            let speed = if o.crouch {
                t::POLE_DOWN_SPEED_FAST
            } else {
                t::POLE_DOWN_SPEED
            };
            self.keep("PoleFall");
            self.pos.y = (self.pos.y - speed * UNIT).max(bottom);
        } else if side.length() > 0.5 {
            // Swing round the pole toward the stick, the hands' arc at the original rate.
            let hands = -self.facing * 0.3;
            let moved = hands + side.normalize() * t::POLE_TURN_DIST / t::POLE_TURN_FRAMES * UNIT;
            self.facing = (-moved).with_y(0.).normalize_or(self.facing);
            self.keep("PoleWait");
        } else {
            self.keep("PoleWait");
        }
    }

    // ---- Swimming ---------------------------------------------------------
    fn swim(&mut self, o: &Orders) {
        if self.pre_throw > 0 || o.throw_pressed {
            self.pre_throw = 0;
            self.change(Move::SwimDash, "SwimDive", |p| {
                p.set_h(p.facing * t::SWIM_DIVE_SPEED);
                p.events.push(Fx::SwimStroke);
            });
            return;
        }
        if self.pre_jump > 0 {
            self.pre_jump = 0;
            self.swim_rise = t::SWIM_RISE_FRAMES;
            self.play("SwimWalk");
            self.events.push(Fx::SwimStroke);
        }
        let mut h = self.v.with_y(0.);
        if self.mode == Move::SwimDash {
            h *= t::SWIM_DIVE_BRAKE;
            if h.length() < t::SWIM_SPEED_MAX_H {
                self.set(Move::Swim);
            }
        } else if self.tilt > 0.05 {
            self.facing = turn(self.facing, self.stick, 0.08);
            h = (h + self.stick * t::SWIM_ACCEL_H * self.tilt)
                .clamp_length_max(t::SWIM_SPEED_MAX_H);
        } else {
            h *= t::SWIM_BRAKE_H;
        }
        self.set_h(h);
        if self.swim_rise > 0 {
            self.swim_rise -= 1;
            self.v.y = (self.v.y + t::SWIM_RISE_POWER * 4.).min(t::SWIM_RISE_MAX);
        } else if o.crouch {
            self.v.y = (self.v.y - t::SWIM_GRAVITY * 2.).max(-t::SWIM_FALL_MAX);
        } else {
            self.v.y = (self.v.y - t::SWIM_GRAVITY).max(-t::SWIM_FALL_MAX);
        }
        if self.mode == Move::Swim && self.anim.name != "SwimWalk" || self.anim.frame > 60. {
            self.keep(if h.length() > 1. {
                "SwimWalk"
            } else {
                "SwimWait"
            });
        }
    }
    fn spin_drop(&mut self, _o: &Orders) {
        self.set_h(Vec3::ZERO);
        self.v.y = (self.v.y - t::SPIN_DOWN_FALL_POWER).max(-t::SPIN_DOWN_FALL_MAX);
    }

    // ---- Cap ---------------------------------------------------------------
    fn throw_kind(&self, o: &Orders) -> Throw {
        if o.up_throw {
            Throw::Up
        } else if o.down_throw {
            Throw::Down
        } else if o.spin_throw || self.spin > 0 {
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
        let name = throw_clip(kind, air, self.throwing.count, self.throwing.continuous);
        let hang = air && !self.air_throw_used;
        if hang {
            self.air_throw_used = true;
        }
        if !air {
            // The state's ground control pulls the player to the floor from
            // the frame it is chosen.
            self.pull = GRAVITY_MOVE;
            self.v.y = -GRAVITY_MOVE;
        }
        self.change(Move::Throw, name, move |p| {
            if hang {
                // One small hang per airtime.
                p.v.y = p.v.y.max(t::SPIN_AIR_JUMP);
            }
            p.winding = true;
            p.winding_age = 0;
            p.throw_clip = name;
            if air {
                // The air spin has no sub animation (`startCapSpinAttackAir`).
                p.sub.active = false;
            } else {
                p.start_sub(name, clip::throw(name) as f32, 0.);
            }
        });
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
                self.spin_ground_move.reset(self.v, self.facing);
            }
            let steer = self.spin_ground_move.update(&SteerInput {
                velocity: self.v,
                front: self.facing,
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
        // Once the cap is away the other actions may cancel the spin
        // (`isEnableCancelGround`): jump, crouch, then the run.
        if self.winding {
            return;
        }
        if self.pre_jump > 0 {
            self.ground_jump(o);
        } else if o.crouch {
            self.start_squat();
        } else if self.tilt >= STICK_DEADZONE {
            self.go(Move::Walk, "Move");
        } else if self.anim_done(clip::throw(self.anim.name)) {
            // The state ends when its clip has, once the cap is away.
            self.go(Move::Idle, "Wait");
        }
    }
    /// `PlayerStateSpinCap::exeSpinAir`: the small hang, the cap leaving on
    /// the air throw frame, then the fall once the cap is away.
    fn spin_air(&mut self, o: &Orders, step: u32) {
        let h = self.v.with_y(0.).clamp_length_max(
            t::SPIN_AIR_SPEED_MAX.max(self.max_air_speed.min(t::SPIN_AIR_SPEED_MAX)),
        );
        self.set_h(h);
        self.v.y = (self.v.y - t::SPIN_AIR_GRAVITY).max(-t::FALL_MAX);
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
            front: self.facing.with_y(0.).normalize_or(Vec3::NEG_Z),
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
        self.caught = false;
        if self.winding {
            self.winding_age += 1;
        }
        // The catch nerve counts its frames after the arrival.
        if let Some(frames) = self.catching {
            let frames = frames + 1;
            self.catching = (frames <= crate::cap::CATCH_FRAMES as i32).then_some(frames);
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
            hold: o.throw_held,
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
                CapEvent::LeftStay => self.events.push(Fx::CapReturn),
                CapEvent::Caught => {
                    self.cap = None;
                    self.caught = true;
                    self.catching = Some(-1);
                    self.events.push(Fx::Catch);
                    self.catch_window = t::CAP_CATCH_WINDOW;
                    // Standing in the wait nerve, the catch plays its sub
                    // animation; it starts before this frame's advance.
                    if self.mode == Move::Idle && self.grounded {
                        self.start_sub("CatchCap", 10., -1.);
                    }
                    break;
                }
            }
        }
    }
    /// What the catch leaves for the player's own step: in midair the catch
    /// gives a small pop.
    fn finish_catch(&mut self) {
        if !std::mem::take(&mut self.caught) {
            return;
        }
        if !self.grounded && self.mode.airborne_jump() && self.v.y < 0. {
            self.v.y = t::CAP_CATCH_POP_POWER;
            self.gravity = t::CAP_CATCH_POP_GRAVITY;
            self.air.set_gravity(t::CAP_CATCH_POP_GRAVITY);
            self.play("JumpCapCatchCommon");
        }
    }
    fn cap_contact(&mut self) {
        let Some(cap) = &self.cap else {
            return;
        };
        if !self.cap_jump_possible
            || cap.returning
            || cap.frames < 6
            || cap.kind == Throw::Up
            || matches!(
                self.mode,
                Move::GroundPound
                    | Move::Ledge
                    | Move::Pole
                    | Move::Swim
                    | Move::SwimDash
                    | Move::Throw
                    | Move::PoundLand
            )
        {
            return;
        }
        let body = self.pos + Vec3::Y * 0.8;
        let d = cap.pos - body;
        if d.with_y(0.).length() > 0.95 || d.y.abs() > 1.1 {
            return;
        }
        let ground = self.grounded || matches!(self.mode, Move::Roll);
        if !ground && self.v.y > 8. && self.mode != Move::Dive {
            return;
        }
        let keep = self.speed_h();
        let (mode, power) = if ground {
            (Move::CapVault, t::CAP_LEAPFROG_POWER)
        } else {
            (Move::CapBounce, t::CAP_LEAPFROG_POWER_AIR)
        };
        self.jump_with(
            mode,
            "JumpCapLeapFrog",
            power,
            t::CAP_LEAPFROG_GRAVITY,
            move |p| {
                p.set_h(p.facing * keep.min(t::JUMP_MOVE_MAX));
                p.max_air_speed = keep.clamp(t::JUMP_MOVE_MIN, t::JUMP_MOVE_MAX);
                p.jump_extend = false;
                p.dive_possible = true;
                p.air_throw_used = false;
                p.events.push(Fx::CapBounce);
            },
        );
        self.cap_jump_possible = false;
        self.hitstop = 0.035;
        self.cap = None;
    }

    // ---- Landing and collision ------------------------------------------------
    fn land(&mut self, o: &Orders) {
        let fall = (self.peak - self.pos.y) / UNIT;
        // The jump state's last frame: its air control still steers, and a jump
        // that ends on the ground counts toward the combo (a fall does not).
        if self.mode.uses_air_control() && self.air_for == Some(self.mode) {
            let mut v = self.v;
            let steer = air_move::Steering {
                front: t::JUMP_ACCEL_FRONT,
                back: t::JUMP_ACCEL_BACK,
                turn: t::JUMP_ACCEL_TURN,
            };
            let push = self.push();
            self.air.update(&mut v, push, o.jump_held, &steer);
            self.set_h(v);
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
        self.cap_jump_possible = true;
        self.dive_possible = true;
        self.air_throw_used = false;
        self.gravity = t::GRAVITY_AIR;
        self.hard_land = fall > t::LONG_FALL
            && !matches!(
                self.mode,
                Move::GroundPound | Move::Dive | Move::BurnJump | Move::BurnBounce | Move::FireRun
            );
        let mode = self.mode;
        match mode {
            // The pound's landing is a sub-state of its nerve: the impact and
            // the landing clip are its first step (the game's logic does not
            // freeze there).
            Move::GroundPound => {
                // Nothing has slowed the fall: the row still reports it.
                self.v.y = -t::HIP_DROP_SPEED;
                self.defer(Move::PoundLand, "HipDropLand", |p| {
                    p.events.push(Fx::PoundLand);
                });
                return;
            }
            Move::SpinDrop => {
                self.go(Move::Idle, "LandDownFall");
                self.events.push(Fx::PoundLand);
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
                if self.hard_land {
                    self.set_h(Vec3::ZERO);
                    self.go(Move::Idle, "LandStiffen");
                } else if o.crouch {
                    self.start_squat();
                } else if self.pre_jump > 0 {
                    // A jump pressed just before landing goes straight on.
                    self.ground_jump(o);
                } else if self.tilt > 0.05 && speed > 0.5 {
                    self.run_frames = t::BRAKE_ON_COUNTER;
                    self.pull = GRAVITY_MOVE;
                    self.go(Move::Run, "Move");
                } else {
                    // The landing clip depends on the clip the jump was playing
                    // (`PlayerStateWait::exeLand`); landing stops the player.
                    let anim = match self.clip() {
                        "JumpTurn" => "LandTurn",
                        "JumpBroad" => "LandRolling",
                        "Jump3" => "LandJump3",
                        _ => "Land",
                    };
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
        self.landing_speed = 0.;
        self.wall = Vec3::ZERO;
        match &scene.mesh {
            Some(mesh) => self.collide_mesh(o, mesh, was_grounded),
            None => self.collide_solids(o, scene, was_grounded),
        }
        if self.mode == Move::Ledge {
            // Just caught an edge: the actor moves onto it.
            self.pos = self.ledge;
            self.grounded = false;
            self.v = Vec3::ZERO;
            return;
        }
        // Poles are solid when not held.
        for pole in &scene.poles {
            let d = (self.pos - pole.center).with_y(0.);
            let min = POLE_RADIUS + RADIUS;
            if d.length() < min
                && self.pos.y < pole.center.y + pole.height
                && self.pos.y + HEIGHT > pole.center.y
            {
                self.pos += d.normalize_or(-self.facing) * (min - d.length());
            }
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
        self.distance += (self.pos - old).with_y(0.).length();
    }
    /// The player's contact with the course's box solids (the windowed demo).
    fn collide_solids(&mut self, o: &Orders, scene: &Course, was_grounded: bool) {
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
                            self.v.y = 0.;
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
                        if before.y >= top - t::WALL_CATCH_EDGE_TOP * UNIT * 0.2
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
                        if !was_grounded && self.v.y < 0. && into > 0.1 {
                            self.touch_wall(top, o);
                        }
                    }
                }
            }
        }
    }
    fn touch_wall(&mut self, top: f32, o: &Orders) {
        if matches!(
            self.mode,
            Move::Dive
                | Move::GroundPound
                | Move::LongJump
                | Move::RollJump
                | Move::Roll
                | Move::WallSlide
                | Move::Throw
                | Move::SpinDrop
        ) {
            return;
        }
        let reach = top - self.pos.y;
        if reach * 100. < t::WALL_CATCH_BOTTOM
            && reach * 100. > t::WALL_CATCH_EDGE_TOP * 0.6
            && o.direction.length() > 0.1
        {
            // Holding the edge is physical and immediate; the catch clip and its
            // sound are the state's first step.
            self.change(Move::Ledge, "WallCatchStart", |p| p.events.push(Fx::Ledge));
            // The edge point, just inside the top, straight ahead of the body.
            let corner = self.pos - self.wall * (RADIUS + t::WALL_CATCH_INSET * UNIT);
            self.ledge = Vec3::new(corner.x, top, corner.z);
            self.facing = -self.wall;
            self.v = Vec3::ZERO;
        } else if self.mode != Move::WallSlide {
            self.change(Move::WallSlide, "WallKeep", |p| {
                p.events.push(Fx::WallSlide)
            });
            self.facing = self.wall;
            self.v.y = self.v.y.max(-t::WALL_SLIDE_MAX * 0.5);
        }
    }
}

#[path = "tests/movement.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
