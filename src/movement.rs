//! Player controller modelled on the original game's player states.
//!
//! It runs at the original's 60 frames per second in its distance unit (one
//! unit is a centimetre; Mario is 160 tall) and its tuning values. Positions
//! are stored in metres for the rest of the game. States that the public
//! decompilation documents follow it; the rest are inferred from their tuning
//! names and observed behaviour. No original code is reproduced here.
use crate::input::Orders;
use bevy::prelude::*;

/// Metres per original distance unit.
pub const UNIT: f32 = 0.01;
/// Simulation rate, matching the original.
pub const FPS: f32 = 60.;
pub const RADIUS: f32 = 0.45;
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
    pub const ROLL_FRAMES_MIN: u32 = 45;
    pub const ROLL_FRAMES_MIN_BOOST: u32 = 30;
    pub const ROLL_START_JUMP: f32 = 12.;
    pub const ROLL_SLOPE_ACCEL: f32 = 0.6;
    pub const ROLL_BRAKE: f32 = 0.998;
    pub const ROLL_END_SPEED: f32 = 17.;
    pub const ROLL_SIDE_ACCEL: f32 = 0.4;
    pub const ROLL_UNROLL_FRAMES: u32 = 5;
    pub const ROLL_END_BRAKE: f32 = 0.95;
    pub const ROLL_END_BRAKE_END: f32 = 10.;
    pub const ROLL_RESTART_ACCEL: f32 = 6.;
    pub const ROLL_RESTART_INTERVAL: u32 = 15;

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
    pub const SPIN_AIR_JUMP: f32 = 6.;
    pub const SPIN_AIR_SPEED_MAX: f32 = 7.;
    pub const SPIN_AIR_GRAVITY: f32 = 0.3;
    pub const SPIN_BRAKE_FRAMES: u32 = 15;
    pub const SPIN_BRAKE_RATE: f32 = 0.95;

    // Cap flight (the cap's own parameters).
    pub const CAP_HEIGHT: f32 = 70.;
    pub const CAP_SPEED: f32 = 32.;
    pub const CAP_REACH: f32 = 500.;
    pub const CAP_BRAKE_FRAMES: f32 = 10.;
    pub const CAP_STAY: u32 = 20;
    pub const CAP_STAY_MAX: u32 = 120;
    pub const CAP_RETURN_ACCEL: f32 = 18.;
    pub const CAP_RETURN_MAX: f32 = 80.;
    pub const CAP_SPIN_RADIUS: f32 = 250.;
    pub const CAP_SPIN_FRAMES: u32 = 45;
    pub const CAP_ROLL_SPEED: f32 = 30.;
    pub const CAP_ROLL_REACH: f32 = 400.;

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

/// Frame counts of clips whose end the controller waits for.
pub mod clip {
    pub const HIP_DROP_START: u32 = 24;
    pub const HIP_DROP_LAND: u32 = 44;
    pub const ROLLING_START: u32 = 6;
    pub const ROLLING_END: u32 = 12;
    pub const HEAD_SLIDING_START: u32 = 48;
    pub const LAND_ROLLING: u32 = 36;
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
    pub const LAND: u32 = 26;
    pub const TURN: u32 = 12;
    pub const WALL_CATCH_START: u32 = 55;
    pub const WALL_CATCH_END: u32 = 46;
    pub const POLE_CATCH: u32 = 8;
    pub const RUN_START: u32 = 30;
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
    Slide,
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
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Ready",
            Self::Walk => "Walk",
            Self::Run => "Run",
            Self::Skid => "Skid",
            Self::Turn => "Turn",
            Self::Crouch => "Crouch",
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
            Self::Slide => "Dive landing",
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
    Step {
        left: bool,
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
    Throw(Throw),
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapPhase {
    Out,
    Stay,
    Return,
}
#[derive(Clone, Debug)]
pub struct Cap {
    /// Metres.
    pub pos: Vec3,
    /// Seconds since the throw, for presentation.
    pub age: f32,
    pub returning: bool,
    pub kind: Throw,
    pub phase: CapPhase,
    /// Units per frame.
    vel: Vec3,
    dir: Vec3,
    travelled: f32,
    frames: u32,
    stay: u32,
}
#[derive(Debug, Default)]
struct ThrowControl {
    run: u32,
    count: u32,
    side: bool,
    timer: u32,
    pending: Option<Throw>,
}

#[derive(Debug, Default)]
struct RollControl {
    boost_frames: u32,
    last_boost: u32,
    unroll: u32,
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
    fire: FireControl,
    climbing: ClimbControl,
    rolling: RollControl,
    throwing: ThrowControl,
    /// Metres.
    pub pos: Vec3,
    /// Metres per second, derived each frame from `v`.
    pub vel: Vec3,
    pub facing: Vec3,
    pub mode: Move,
    pub grounded: bool,
    pub cap: Option<Cap>,
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
    jump_count: u32,
    jump_timer: u32,
    jump_held_frames: u32,
    long_jump_count: u32,
    long_jump_timer: u32,
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
}
impl Default for Player {
    fn default() -> Self {
        Self {
            fire: FireControl::default(),
            climbing: ClimbControl::default(),
            rolling: RollControl::default(),
            throwing: ThrowControl::default(),
            pos: SPAWN,
            vel: Vec3::ZERO,
            facing: Vec3::NEG_Z,
            mode: Move::Idle,
            grounded: true,
            cap: None,
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
            jump_count: 0,
            jump_timer: t::CONTINUOUS_JUMP_TIMER,
            jump_held_frames: 0,
            long_jump_count: 0,
            long_jump_timer: 0,
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
fn present(mut players: Query<&mut Player>, fixed: Res<Time<Fixed>>) {
    let a = fixed.overstep_fraction().clamp(0., 1.);
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
/// Weights of Walk, Run, Dash and DashFast in the run cycle at a speed in
/// units per frame, from the original's border speeds and blend ranges.
pub fn move_weights(speed: f32) -> [f32; 4] {
    let ramp = |border: f32, range: f32| ((speed - border + range) / (2. * range)).clamp(0., 1.);
    let (run, dash, fast) = (ramp(8., 2.), ramp(15., 1.), ramp(20., 1.));
    [1. - run, run * (1. - dash), dash * (1. - fast), fast]
}
/// Run cycle playback rate, in frames of its 60-frame phase per game frame.
pub fn move_rate(speed: f32) -> f32 {
    let speed = speed.min(25.);
    let k = ((speed - 6.) / (26. - 6.)).clamp(0., 1.);
    let w = move_weights(speed);
    let rate = (w[0] + w[1]) * lerp_clamped(1., 3.5, k)
        + w[2] * lerp_clamped(1., 4., k)
        + w[3] * lerp_clamped(1., 5., k);
    // Below the minimum speed the cycle slows with the feet.
    if speed < 6. {
        rate * (speed / 6.).max(0.25)
    } else {
        rate
    }
}
fn approach(value: f32, target: f32, step: f32) -> f32 {
    if value < target {
        (value + step).min(target)
    } else {
        (value - step).max(target)
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
    pub fn reset(&mut self) {
        let checkpoint = self.checkpoint;
        *self = Self::default();
        self.pos = checkpoint;
        self.peak = checkpoint.y;
        self.checkpoint = checkpoint;
    }
    fn set(&mut self, mode: Move) {
        if self.mode != mode {
            self.mode = mode;
            self.frame = 0;
        }
    }
    fn play(&mut self, name: &'static str) {
        self.anim = Anim {
            name,
            serial: self.anim.serial.wrapping_add(1),
            frame: 0.,
            rate: 1.,
        };
    }
    /// Start a clip unless it is already the current one.
    fn keep(&mut self, name: &'static str) {
        if self.anim.name != name {
            self.play(name);
        }
    }
    fn anim_done(&self, frames: u32) -> bool {
        self.anim.frame >= frames as f32 - 1.
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
        if low_ceiling && !matches!(self.mode, Move::Roll | Move::Slide) {
            o.crouch = true;
            o.jump = false;
        }
        self.read_input(&o);
        let was_grounded = self.grounded;
        let wet = scene.wet(self.pos + Vec3::Y * 0.8);
        if wet && !matches!(self.mode, Move::Swim | Move::SwimDash) {
            if self.burning() {
                self.events.push(Fx::Quenched);
            }
            self.events.push(Fx::Splash);
            self.v *= 0.3;
            self.enter(Move::Swim, "SwimFallIn");
        } else if !wet && matches!(self.mode, Move::Swim | Move::SwimDash) {
            self.enter(Move::Fall, "Fall");
        }
        match self.mode {
            Move::Idle | Move::Walk | Move::Run => self.run(&o),
            Move::Skid | Move::Turn => self.brake(&o),
            Move::Crouch | Move::Crawl => self.squat(&o),
            Move::GroundSpin => self.ground_spin(&o),
            Move::Throw => self.throw_state(&o),
            Move::GroundPound => self.hip_drop(&o),
            Move::PoundLand => self.hip_drop_land(&o),
            Move::Roll => self.roll(&o),
            Move::RollEnd => self.roll_end(&o),
            Move::Dive => self.head_sliding(&o),
            Move::Slide => self.belly_slide(&o),
            Move::SlopeSlide => self.slope_slide(&o),
            Move::BurnJump | Move::BurnBounce => self.burn_air(&o),
            Move::FireRun => self.fire_run(&o),
            Move::LongJump => self.long_jump_air(&o),
            Move::WallSlide => self.wall_slide(&o),
            Move::Ledge => self.wall_catch(&o, scene),
            Move::Pole => self.pole_climb(&o, scene),
            Move::Swim | Move::SwimDash => self.swim(&o),
            Move::SpinDrop => self.spin_drop(&o),
            _ => self.air(&o),
        }
        self.update_cap(&o, scene);
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
            self.land(&o);
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
                    | Move::Crouch
                    | Move::Crawl
                    | Move::GroundSpin
                    | Move::Throw
                    | Move::Slide
            )
        {
            self.coyote = 3;
            self.enter(Move::Fall, "Fall");
        }
        self.finish_frame(scene);
    }
    fn read_input(&mut self, o: &Orders) {
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
        self.throwing.timer = self.throwing.timer.saturating_sub(1);
        self.coyote = self.coyote.saturating_sub(1);
        self.long_jump_timer = self.long_jump_timer.saturating_add(1);
        if self.grounded {
            self.jump_timer = self.jump_timer.saturating_add(1);
            self.land_frames = self.land_frames.saturating_add(1);
        }
    }
    fn finish_frame(&mut self, scene: &Course) {
        self.update_soot(scene);
        self.frame += 1;
        if self.anim.name == "Move" {
            self.anim.frame = self.step_phase;
        } else {
            self.anim.frame += self.anim.rate;
            if self.anim.name == "FireRun" && self.anim.frame >= clip::FIRE_RUN {
                self.anim.frame -= clip::FIRE_RUN;
            }
        }
        if let Some(sub) = &mut self.sub_anim {
            sub.frame += 1.;
            if sub.frame >= 16. {
                self.sub_anim = None;
            }
        }
        self.vel = self.v * UNIT * FPS;
        self.move_speed = self.speed_h();
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
    fn enter(&mut self, mode: Move, anim: &'static str) {
        self.set(mode);
        self.frame = 0;
        self.play(anim);
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
        if o.crouch && speed >= t::DASH_JUDGE {
            // Crouching at dash speed goes straight into a roll.
            self.start_roll(speed.max(t::ROLL_START), false);
            return true;
        }
        if o.crouch && !matches!(self.mode, Move::Crouch | Move::Crawl) {
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
                    self.launch_cap(kind);
                } else {
                    self.start_throw(kind);
                }
                return true;
            }
        }
        if self.spin > 0 && self.tilt > 0.5 && self.mode != Move::GroundSpin {
            self.spin = 0;
            self.enter(
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
    fn run(&mut self, o: &Orders) {
        if self.ground_actions(o) {
            return;
        }
        let speed = self.speed_h();
        if self.tilt > 0.05 {
            let wish = self.stick;
            // Reversing at speed: brake, then turn around.
            if speed > t::NORMAL_MAX * t::BRAKE_ON_SPEED_RATE
                && self.run_frames >= t::BRAKE_ON_COUNTER
                && wish.dot(self.facing) < -0.7
            {
                self.enter(Move::Skid, "Brake");
                self.quick_turn = t::QUICK_TURN_JUMP_FRAMES;
                self.events.push(Fx::Skid);
                return;
            }
            let limit = lerp_clamped(
                t::ROUND_LIMIT_DEG,
                t::ROUND_LIMIT_DEG_MIN,
                speed / t::NORMAL_MAX,
            );
            let limit = if speed < t::NORMAL_MIN { 30. } else { limit };
            self.facing = turn(self.facing, wish, limit.to_radians());
            let max = if self.after_turn > 0 {
                t::AFTER_TURN_MAX
            } else if self.throwing.run > 0 {
                t::RUN_SPEED_CONTINUOUS_THROW
            } else {
                t::NORMAL_MAX
            } * self.tilt.max(0.2);
            let accel = if self.throwing.run > 0 {
                t::RUN_SPEED_CONTINUOUS_THROW / t::RUN_ACCEL_FRAMES_CONTINUOUS_THROW
            } else {
                t::NORMAL_MAX / t::ACCEL_FRAMES
            };
            let s = if speed < max {
                (speed.max(t::NORMAL_MIN) + accel).min(max)
            } else {
                // Above the stick's speed (after boosts) ease back down.
                approach(speed, max, accel)
            };
            self.set_h(self.facing * s);
            self.run_frames += 1;
            if !matches!(self.mode, Move::Run | Move::Walk) {
                self.set(Move::Run);
            }
            if self.anim.name != "Move" && self.anim.name != "RunStart" {
                // Starting from a standstill plays the run start.
                if speed < 0.5 && self.anim.name == "Wait" {
                    self.play("RunStart");
                } else {
                    self.play("Move");
                }
            } else if self.anim.name == "RunStart"
                && (self.anim_done(clip::RUN_START) || self.anim.frame > 10.)
            {
                self.play("Move");
            }
            self.mode = if s < 8. { Move::Walk } else { Move::Run };
        } else {
            self.run_frames = 0;
            let s = (speed - t::NORMAL_MAX / t::BRAKE_FRAMES).max(0.);
            self.set_h(self.facing * s);
            if s <= 0. {
                self.set(Move::Idle);
                let landing = self.land_frames < clip::LAND
                    && matches!(
                        self.anim.name,
                        "Land" | "LandJump3" | "LandStiffen" | "LandDownFall" | "LandTurn"
                    );
                if !landing && self.anim.name != "Wait" {
                    self.play("Wait");
                }
            }
        }
        self.v.y = -t::GRAVITY;
        self.update_move_anim();
    }
    /// The original's run cycle: a 60-frame phase that speeds up with speed.
    fn update_move_anim(&mut self) {
        let speed = self.speed_h();
        if self.anim.name == "Move" {
            let rate = move_rate(speed);
            self.anim.rate = rate;
            let before = self.step_phase;
            self.step_phase = (self.step_phase + rate) % 60.;
            // Footfalls at the original's foot-print frames of the cycle.
            for (at, left) in [(4., true), (35., false)] {
                let crossed = if before <= self.step_phase {
                    before < at && self.step_phase >= at
                } else {
                    before < at || self.step_phase >= at
                };
                if crossed && speed > 0.5 {
                    self.events.push(Fx::Step { left });
                }
            }
        } else if self.anim.name == "RunStart" {
            self.anim.rate = 2.;
        }
    }
    fn brake(&mut self, o: &Orders) {
        if self.pre_jump > 0 && self.quick_turn > 0 {
            self.side_somersault();
            return;
        }
        let speed = self.speed_h();
        if self.mode == Move::Skid {
            let s = (speed - t::NORMAL_MAX / t::BRAKE_FRAMES).max(0.);
            self.set_h(self.facing * s);
            if self.frame >= t::BRAKE_TURN_START && s < t::NORMAL_MIN || self.tilt < 0.05 && s <= 0.
            {
                if self.tilt > 0.05 {
                    self.facing = self.stick;
                    self.enter(Move::Turn, "TurnPoint");
                    self.events.push(Fx::TurnDust);
                } else {
                    self.enter(Move::Idle, "Wait");
                }
            }
        } else {
            // Turned around: burst away with the after-turn speed allowance.
            self.after_turn = t::AFTER_TURN_FRAMES;
            if self.tilt > 0.05 {
                self.facing = turn(self.facing, self.stick, 20_f32.to_radians());
            }
            self.set_h(
                self.facing
                    * (speed + t::NORMAL_MAX / t::ACCEL_FRAMES * 2.).min(t::AFTER_TURN_MAX * 0.5),
            );
            if self.frame >= clip::TURN || self.tilt < 0.05 {
                self.run_frames = t::BRAKE_ON_COUNTER;
                self.set(Move::Run);
                self.play("Move");
            }
        }
        if self.pre_jump > 0 {
            self.ground_jump(o);
        }
        self.v.y = -t::GRAVITY;
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
            self.enter(Move::Idle, "Wait");
        }
    }

    // ---- Squat, roll, slides --------------------------------------------
    fn start_squat(&mut self) {
        let from_roll = self.anim.name == "RollingEnd";
        self.set(Move::Crouch);
        self.play(if from_roll { "SquatWait" } else { "SquatStart" });
        let speed = self.speed_h();
        let forward = self.v.with_y(0.).dot(self.facing);
        // Crouching keeps forward momentum as a short crouch slide.
        let s = if speed <= t::NORMAL_MAX && forward > 0. {
            let s = speed * t::SQUAT_ACCEL_RATE;
            if s <= t::SQUAT_BRAKE_END { 0. } else { s }
        } else {
            forward.max(0.)
        };
        self.set_h(self.facing * s);
    }
    /// Ground at least this many degrees steep.
    fn steep(&self, degrees: f32) -> bool {
        self.grounded && self.slope.abs().atan().to_degrees() >= degrees
    }
    fn start_slope_slide(&mut self) {
        let down = Vec3::new(-self.slope.signum(), 0., 0.);
        let mut h = self.v.with_y(0.);
        // Start at least gently downhill, never faster than the slide's limit.
        let along = h.dot(down);
        if along < t::SLOPE_SLIDE_START_SPEED {
            h += down * (t::SLOPE_SLIDE_START_SPEED - along);
        }
        self.set_h(h.clamp_length_max(t::SLOPE_SLIDE_MAX));
        self.enter(Move::SlopeSlide, "Slide");
    }
    /// Sliding down a slope on the seat: speeds up downhill, steers a little,
    /// and brakes to a stop once the ground flattens out.
    fn slope_slide(&mut self, o: &Orders) {
        if self.pre_jump > 0 {
            self.ground_jump(o);
            return;
        }
        if (self.pre_throw > 0 || o.throw_pressed) && self.grounded {
            self.pre_throw = 0;
            self.start_roll(self.speed_h().max(t::ROLL_START), false);
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
                    self.enter(Move::Idle, "Wait");
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
        self.enter(Move::BurnJump, "FireRunStart");
        // The leap keeps the current speed, turned along the stick; with the
        // stick at rest it goes straight up.
        let h = if self.tilt > 0.05 {
            self.stick * self.speed_h()
        } else {
            Vec3::ZERO
        };
        self.fire_leap(
            h,
            t::DAMAGE_FIRE_JUMP_POWER_1ST,
            t::DAMAGE_FIRE_NO_GRAVITY_FRAMES,
        );
        self.events.push(Fx::Burned(false));
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
        self.enter(Move::BurnBounce, "FireRunStart");
        let h = if self.tilt > 0.05 {
            self.stick * self.speed_h()
        } else {
            Vec3::ZERO
        };
        self.fire_leap(h, t::DAMAGE_FIRE_JUMP_POWER_2ND, 0);
        self.events.push(Fx::Burned(true));
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
        self.launch_cap(if kind == Throw::Spin {
            Throw::Forward
        } else {
            kind
        });
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
        self.set(Move::FireRun);
        self.frame = 0;
        if self.anim.name != "FireRun" {
            self.play("FireRun");
            self.anim.rate = t::DAMAGE_FIRE_RUN_SPEED * t::DAMAGE_FIRE_RUN_ANIM_RATE;
        }
        self.fire.brake = t::DAMAGE_FIRE_RUN_BRAKE_FRAMES as u32;
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
            self.set(Move::Run);
            self.play(if self.speed_h() > 0.5 { "Move" } else { "Wait" });
        } else {
            self.enter(Move::Fall, "Fall");
        }
    }
    fn squat(&mut self, o: &Orders) {
        if o.crouch && self.steep(t::SLOPE_SLIDE_ANGLE_START) {
            self.start_slope_slide();
            return;
        }
        let speed = self.speed_h();
        let sliding = speed > t::SQUAT_BRAKE_END;
        if self.pre_jump > 0 {
            self.pre_jump = 0;
            if sliding && self.v.dot(self.facing) > 0. {
                self.long_jump();
            } else {
                self.backflip();
            }
            return;
        }
        if self.pre_throw > 0 || o.throw_pressed {
            self.pre_throw = 0;
            self.start_roll(speed.max(t::ROLL_START), false);
            return;
        }
        if !o.crouch {
            self.enter(Move::Idle, "SquatEnd");
            return;
        }
        if sliding {
            let s = speed * t::SQUAT_BRAKE_RATE;
            self.set_h(self.v.with_y(0.).normalize_or(self.facing) * s);
        } else if self.tilt > 0.05 {
            self.facing = turn(self.facing, self.stick, 30_f32.to_radians() / 10.);
            self.set_h(self.facing * t::SQUAT_WALK);
            self.set(Move::Crawl);
            self.keep("SquatWalk");
            for (at, left) in [(2., true), (20., false)] {
                if (self.anim.frame % 36.) < at && ((self.anim.frame + 1.) % 36.) >= at {
                    self.events.push(Fx::Step { left });
                }
            }
        } else {
            self.set_h(Vec3::ZERO);
            self.set(Move::Crouch);
            if self.anim.name != "SquatStart" || self.anim.frame > 2. {
                self.keep("SquatWait");
            }
        }
        self.v.y = -t::GRAVITY;
    }
    fn start_roll(&mut self, speed: f32, boosted: bool) {
        if let Some(w) = (self.tilt > 0.1).then_some(self.stick) {
            self.facing = w;
        }
        self.enter(Move::Roll, "RollingStart");
        self.set_h(self.facing * speed.min(t::ROLL_MAX));
        self.v.y = t::ROLL_START_JUMP * 0.25;
        self.rolling.boost_frames = if boosted { t::ROLL_FRAMES_MIN_BOOST } else { 0 };
        self.rolling.last_boost = 0;
        self.rolling.unroll = 0;
        self.events.push(Fx::RollStart);
    }
    fn roll(&mut self, o: &Orders) {
        if self.pre_jump > 0 && self.grounded {
            self.pre_jump = 0;
            if o.throw_pressed {
                // Roll cancel: stand up at speed with a cap throw.
                let s = self.speed_h().min(t::RUN_SPEED_CONTINUOUS_THROW);
                self.enter(Move::Run, "Move");
                self.set_h(self.facing * s);
                self.throwing.run = t::RUN_TIME_CONTINUOUS_THROW;
                if self.cap.is_none() {
                    self.launch_cap(Throw::Forward);
                }
                return;
            }
            let s = self.speed_h();
            self.jump_with(
                Move::RollJump,
                "RollingJump",
                t::JUMP_POWER_MAX,
                t::JUMP_GRAVITY,
            );
            self.max_air_speed = s.min(t::JUMP_MOVE_MAX);
            return;
        }
        if self.anim.name == "RollingStart" && self.anim_done(clip::ROLLING_START) {
            self.play("Rolling");
        }
        self.rolling.last_boost += 1;
        if (self.pre_throw > 0 || o.throw_pressed)
            && self.grounded
            && self.rolling.last_boost >= t::ROLL_RESTART_INTERVAL
        {
            self.pre_throw = 0;
            // Boost: a hop and a push; pressing on the roll's rhythm keeps it going.
            let perfect = self.rolling.last_boost <= t::ROLL_RESTART_INTERVAL * 3;
            let s = (self.speed_h() + t::ROLL_RESTART_ACCEL).clamp(t::ROLL_BOOST, t::ROLL_MAX);
            self.set_h(self.facing * s);
            self.v.y = t::ROLL_START_JUMP;
            self.grounded = false;
            self.rolling.last_boost = 0;
            self.rolling.boost_frames = 0;
            self.frame = 0;
            self.events.push(Fx::RollBoost { perfect });
        }
        let mut s = self.speed_h();
        if self.tilt > 0.1 {
            // Steering: the side push turns the roll gradually.
            let max = (t::ROLL_SIDE_ACCEL / s.max(1.)).atan();
            self.facing = turn(self.facing, self.stick, max);
        }
        if self.grounded {
            let downhill = -self.slope * self.facing.x;
            s = s * if self.slick { 0.9995 } else { t::ROLL_BRAKE }
                + downhill * t::ROLL_SLOPE_ACCEL * 4.;
            if s < 0. {
                self.facing = -self.facing;
                s = -s;
            }
            self.v.y = -t::GRAVITY;
        } else {
            self.v.y = (self.v.y - t::GRAVITY_AIR).max(-t::FALL_MAX);
        }
        s = s.min(t::ROLL_MAX);
        self.set_h(self.facing * s);
        self.rolling.unroll = if o.crouch { 0 } else { self.rolling.unroll + 1 };
        let min_frames = if self.rolling.boost_frames > 0 {
            t::ROLL_FRAMES_MIN_BOOST
        } else {
            t::ROLL_FRAMES_MIN
        };
        if self.grounded
            && (self.rolling.unroll >= t::ROLL_UNROLL_FRAMES
                || self.frame >= min_frames
                    && s < t::ROLL_END_SPEED
                    && -self.slope * self.facing.x <= 0.05)
        {
            self.enter(Move::RollEnd, "RollingEnd");
        }
        self.anim.rate = lerp_clamped(0.2, 1., (s - 5.) / (35. - 5.));
    }
    fn roll_end(&mut self, o: &Orders) {
        let s = self.speed_h();
        let s = if s > t::ROLL_END_BRAKE_END {
            s * t::ROLL_END_BRAKE
        } else {
            (s - 1.).max(0.)
        };
        self.set_h(self.facing * s);
        self.v.y = -t::GRAVITY;
        if self.anim_done(clip::ROLLING_END) && s <= t::ROLL_END_BRAKE_END {
            if o.crouch {
                self.set(Move::Crouch);
                self.play("SquatWait");
            } else {
                self.enter(Move::Idle, "Wait");
            }
        }
    }
    /// A dive's landing: the forward roll on the ground, slowing to a stop.
    fn belly_slide(&mut self, o: &Orders) {
        if self.pre_jump > 0 {
            self.pre_jump = 0;
            self.jump_with(Move::Jump, "Jump", t::JUMP_POWER_MIN, t::JUMP_GRAVITY);
            return;
        }
        let s = (self.speed_h() - t::HEAD_SLIDING_BRAKE).max(0.);
        if self.tilt > 0.1 {
            self.facing = turn(self.facing, self.stick, 0.02);
        }
        self.set_h(self.facing * s);
        self.v.y = -t::GRAVITY;
        if self.anim_done(clip::LAND_ROLLING) {
            if o.crouch {
                self.start_squat();
            } else if self.tilt > 0.05 && s > t::NORMAL_MIN {
                self.set(Move::Run);
                self.play("Move");
            } else {
                self.enter(Move::Idle, "Wait");
            }
        }
    }

    // ---- Jumps ------------------------------------------------------------
    fn jump_with(&mut self, mode: Move, anim: &'static str, power: f32, gravity: f32) {
        self.enter(mode, anim);
        self.v.y = power;
        self.gravity = gravity;
        self.jump_extend = true;
        self.max_air_speed = self.speed_h().clamp(t::JUMP_MOVE_MIN, t::JUMP_MOVE_MAX);
        self.grounded = false;
        self.coyote = 0;
        self.pre_jump = 0;
        self.events.push(Fx::Jump(mode));
    }
    fn ground_jump(&mut self, _o: &Orders) {
        let speed = self.speed_h();
        if self.mode == Move::PoundLand
            && (t::HIP_DROP_JUMP_BEGIN..=t::HIP_DROP_JUMP_END).contains(&self.frame)
        {
            self.jump_with(
                Move::PoundJump,
                "JumpHipDrop",
                t::HIP_DROP_JUMP_POWER,
                t::JUMP_GRAVITY,
            );
            self.set_h(Vec3::ZERO);
            self.jump_extend = false;
            return;
        }
        if self.catch_window > 0 {
            self.jump_with(
                Move::CatchJump,
                "JumpCapCatchCommon",
                t::CAP_CATCH_JUMP_POWER,
                t::CAP_CATCH_JUMP_GRAVITY,
            );
            self.jump_extend = false;
            return;
        }
        if self.spin > 0 {
            self.spin = 0;
            self.jump_with(
                Move::SpinJump,
                "SpinJumpStart",
                t::SPIN_JUMP_POWER,
                t::SPIN_JUMP_GRAVITY,
            );
            self.jump_extend = false;
            self.max_air_speed = t::SPIN_JUMP_MOVE_MAX;
            return;
        }
        if matches!(self.mode, Move::Skid | Move::Turn)
            || self.quick_turn > 0 && self.after_turn > 0
        {
            self.side_somersault();
            return;
        }
        // Consecutive jumps: land and jump again within the window.
        let next = if self.jump_timer < t::CONTINUOUS_JUMP_TIMER && speed > t::NORMAL_MIN {
            (self.jump_count + 1) % 3
        } else {
            0
        };
        let next = if next == 2 && speed < 8. { 1 } else { next };
        self.jump_count = next;
        let rate = speed / t::NORMAL_MAX;
        match next {
            1 => self.jump_with(
                Move::DoubleJump,
                "Jump2",
                lerp_clamped(t::CONTINUOUS_JUMP_POWER_MIN, t::JUMP_POWER_MAX_2ND, rate),
                t::JUMP_GRAVITY_2ND,
            ),
            2 => {
                self.jump_with(
                    Move::TripleJump,
                    "Jump3",
                    t::JUMP_POWER_3RD,
                    t::JUMP_GRAVITY_3RD,
                );
                self.jump_extend = false;
            }
            _ => {
                // The first jump mirrors with whichever foot is planted in the run cycle.
                let anim = if speed > t::NORMAL_MIN && (4. ..35.).contains(&self.step_phase) {
                    "JumpReverse"
                } else {
                    "Jump"
                };
                self.jump_with(
                    Move::Jump,
                    anim,
                    lerp_clamped(t::JUMP_POWER_MIN, t::JUMP_POWER_MAX, rate),
                    t::JUMP_GRAVITY,
                )
            }
        }
    }
    fn side_somersault(&mut self) {
        if self.tilt > 0.05 {
            self.facing = self.stick;
        }
        self.jump_with(
            Move::Sideflip,
            "JumpTurn",
            t::TURN_JUMP_POWER,
            t::TURN_JUMP_GRAVITY,
        );
        self.set_h(self.facing * t::TURN_JUMP_VEL_H);
        self.max_air_speed = t::TURN_JUMP_VEL_H;
        self.jump_extend = false;
        self.quick_turn = 0;
    }
    fn backflip(&mut self) {
        self.jump_with(
            Move::Backflip,
            "JumpBack",
            t::SQUAT_JUMP_POWER,
            t::SQUAT_JUMP_GRAVITY,
        );
        self.set_h(-self.facing * t::SQUAT_JUMP_BACK);
        self.max_air_speed = t::SQUAT_JUMP_MOVE_MAX;
        self.jump_extend = false;
    }
    fn long_jump(&mut self) {
        const NAMES: [&str; 3] = ["JumpBroad", "JumpBroad2", "JumpBroad3"];
        if self.long_jump_timer >= t::CONTINUOUS_LONG_JUMP_TIMER {
            self.long_jump_count = 0;
        }
        let name = NAMES[self.long_jump_count as usize % 3];
        self.long_jump_count = (self.long_jump_count + 1) % 3;
        let speed = self.speed_h();
        self.jump_with(
            Move::LongJump,
            name,
            t::LONG_JUMP_POWER,
            t::LONG_JUMP_GRAVITY,
        );
        let s = (speed + t::LONG_JUMP_MOVE_POW)
            .max(t::LONG_JUMP_INIT_SPEED)
            .max(t::LONG_JUMP_SPEED_MIN);
        self.set_h(self.facing * s);
        self.jump_extend = false;
    }
    /// Long jump flight: slow forward accel, stronger brake, light steering.
    fn long_jump_air(&mut self, o: &Orders) {
        if self.air_actions(o) {
            return;
        }
        let h = self.v.with_y(0.);
        let dir = h.normalize_or(self.facing);
        let mut s = h.length();
        let mut new_dir = dir;
        if self.tilt > 0.05 {
            let along = self.stick.dot(dir);
            if along < -0.3 {
                s = (s - t::LONG_JUMP_BRAKE * self.tilt).max(t::LONG_JUMP_SPEED_MIN);
            } else if along > 0.3 {
                s = (s + t::LONG_JUMP_ACCEL * self.tilt)
                    .min(t::LONG_JUMP_SPEED.max(s.min(t::LONG_JUMP_SPEED)));
            }
            let side = (t::LONG_JUMP_SIDE_ACCEL * self.tilt / s.max(1.)).atan();
            new_dir = turn(dir, self.stick, side);
        }
        self.facing = new_dir;
        self.set_h(new_dir * s.min(t::LONG_JUMP_SPEED.max(s)));
        self.v.y = (self.v.y - t::LONG_JUMP_GRAVITY).max(-t::FALL_MAX);
        if self.anim.frame >= 37. && self.v.y < 0. && self.anim.name.starts_with("JumpBroad") {
            self.play("Fall");
        }
    }
    /// Generic airborne control for jumps and falls.
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
        if !extending {
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
                self.enter(Move::SpinDrop, "SpinJumpDownFall");
                self.v = Vec3::NEG_Y * t::SPIN_DOWN_FALL_INIT;
            } else {
                self.start_hip_drop();
            }
            return true;
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
        self.enter(Move::GroundPound, "HipDropStart");
        self.v = Vec3::ZERO;
        self.events.push(Fx::PoundStart);
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
                self.play("HipDrop");
                self.v = Vec3::NEG_Y * t::HIP_DROP_SPEED;
            }
        } else {
            self.v = Vec3::NEG_Y * t::HIP_DROP_SPEED;
        }
    }
    fn hip_drop_land(&mut self, o: &Orders) {
        self.set_h(Vec3::ZERO);
        self.v.y = -t::GRAVITY;
        if self.pre_jump > 0
            && (t::HIP_DROP_JUMP_BEGIN..=t::HIP_DROP_JUMP_END).contains(&self.frame)
        {
            self.ground_jump(o);
            return;
        }
        if (self.pre_throw > 0 || o.throw_pressed) && self.frame <= t::HIP_DROP_JUMP_END {
            self.pre_throw = 0;
            if o.crouch {
                // Roll straight out of the pound with a strong start.
                self.start_roll(t::ROLL_BOOST, true);
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
                self.enter(Move::Idle, "Wait");
            }
        }
    }
    fn dive(&mut self) {
        self.enter(Move::Dive, "HeadSlidingStart");
        let mut h = self.v.with_y(0.).normalize_or(self.facing);
        if self.tilt > 0.1 {
            h = self.stick;
        }
        self.facing = h;
        let up = self.v.y.max(0.) + t::HEAD_SLIDING_JUMP;
        self.v = h * t::HEAD_SLIDING_SPEED + Vec3::Y * up;
        self.dive_possible = false;
        self.events.push(Fx::Dive);
    }
    fn head_sliding(&mut self, _o: &Orders) {
        // Diving flight: no forward accel, a light side push, braking when pulled back.
        let mut h = self.v.with_y(0.);
        let dir = h.normalize_or(self.facing);
        let mut s = h.length();
        if self.tilt > 0.1 {
            if self.stick.dot(dir) < -0.3 {
                s = (s - t::HEAD_SLIDING_BRAKE).max(t::HEAD_SLIDING_SPEED_MIN);
            }
            let side = (t::HEAD_SLIDING_SIDE_ACCEL / s.max(1.)).atan();
            h = turn(dir, self.stick, side) * s;
        } else {
            h = dir * s;
        }
        self.facing = h.normalize_or(self.facing);
        self.set_h(h);
        self.v.y = (self.v.y - t::HEAD_SLIDING_GRAVITY).max(-t::FALL_MAX);
        if self.anim.name == "HeadSlidingStart" && self.anim_done(clip::HEAD_SLIDING_START) {
            self.play("HeadSliding");
        }
    }

    // ---- Walls, ledges, poles --------------------------------------------
    fn wall_slide(&mut self, o: &Orders) {
        if self.pre_jump > 0 {
            self.facing = self.wall;
            self.jump_with(
                Move::WallJump,
                "WallJump",
                t::WALL_JUMP_POWER,
                t::WALL_JUMP_GRAVITY,
            );
            self.set_h(self.wall * t::WALL_JUMP_H);
            self.max_air_speed = t::WALL_JUMP_H;
            self.jump_extend = false;
            self.dive_possible = true;
            return;
        }
        if o.crouch_pressed || self.wall == Vec3::ZERO {
            self.pos += self.wall * 0.1;
            self.enter(Move::Fall, "Fall");
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
        if self.anim.name == "WallCatchStart" && self.anim_done(clip::WALL_CATCH_START) {
            self.play("WallCatch");
        }
        if self.anim.name == "WallCatchEnd" {
            if self.anim_done(clip::WALL_CATCH_END) {
                self.grounded = true;
                self.enter(Move::Idle, "Wait");
            }
            return;
        }
        let hang_out = self.wall * (RADIUS + 0.05 + t::WALL_CATCH_INSET * UNIT);
        if o.crouch_pressed || toward < -0.4 {
            self.pos = self.ledge + hang_out - Vec3::Y * t::WALL_CATCH_HANG * UNIT;
            self.enter(Move::Fall, "Fall");
        } else if self.frame > 8 && self.pre_jump > 0 {
            self.jump_with(
                Move::Jump,
                "WallCatchEndJump",
                t::WALL_CLIMB_JUMP_V,
                t::WALL_CLIMB_JUMP_GRAVITY,
            );
            self.set_h(-self.wall * t::WALL_CLIMB_JUMP_H);
            self.jump_extend = false;
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
                self.enter(Move::Pole, "PoleCatch");
                self.climbing.base = pole.center;
                self.climbing.height = pole.height;
                self.climbing.frame = 0;
                self.facing = (-d).normalize_or(self.facing);
                // The actor sits on the pole's axis; the clips hang the body behind it.
                self.pos = Vec3::new(pole.center.x, self.pos.y, pole.center.z);
                self.v = Vec3::ZERO;
                self.events.push(Fx::Pole);
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
            self.jump_with(Move::Jump, "Jump", t::WALL_JUMP_POWER, t::WALL_JUMP_GRAVITY);
            self.set_h(away * t::WALL_JUMP_H);
            self.jump_extend = false;
            return;
        }
        if o.crouch_pressed {
            self.pos = axis - self.facing * clear;
            self.climbing.cooldown = 20;
            self.enter(Move::Fall, "Fall");
            return;
        }
        if self.anim.name == "PoleCatch" && !self.anim_done(clip::POLE_CATCH) {
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
                self.enter(Move::Idle, "Wait");
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
            self.enter(Move::SwimDash, "SwimDive");
            self.set_h(self.facing * t::SWIM_DIVE_SPEED);
            self.events.push(Fx::SwimStroke);
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
    fn start_throw(&mut self, kind: Throw) {
        if self.cap.is_some() {
            return;
        }
        let air = !self.grounded;
        // Repeated throws alternate sides and use the follow-up clips.
        self.throwing.count = if self.throwing.timer > 0 {
            (self.throwing.count + 1).min(2)
        } else {
            0
        };
        self.throwing.side = !self.throwing.side;
        let name = match (kind, air) {
            (Throw::Up, false) => "SpinCapStartDoubleUp",
            (Throw::Up, true) => "SpinCapAirStartDoubleUp",
            (Throw::Down, false) => "SpinCapStartDoubleDown",
            (Throw::Down, true) => "SpinCapAirStartDoubleDown",
            (Throw::Spin, false) => {
                if self.throwing.side {
                    "SpinCapStartDoubleRight"
                } else {
                    "SpinCapStartDoubleLeft"
                }
            }
            (Throw::Spin, true) => {
                if self.throwing.side {
                    "SpinCapAirStartDoubleRight"
                } else {
                    "SpinCapAirStartDoubleLeft"
                }
            }
            (Throw::Forward, true) => {
                if self.throwing.side {
                    "SpinCapAirStart"
                } else {
                    "SpinCapAirStartLeft"
                }
            }
            (Throw::Forward, false) => match (self.throwing.count, self.throwing.side) {
                (1, true) => "SpinCapStart2Right",
                (1, false) => "SpinCapStart2Left",
                (2, true) => "SpinCapStart3Right",
                (2, false) => "SpinCapStart3Left",
                (_, true) => "SpinCapStart",
                (_, false) => "SpinCapStartLeft",
            },
        };
        self.enter(Move::Throw, name);
        if air && !self.air_throw_used {
            // One small hang per airtime.
            self.v.y = self.v.y.max(t::SPIN_AIR_JUMP);
            self.air_throw_used = true;
        }
        self.throwing.pending = Some(kind);
    }
    fn throw_state(&mut self, o: &Orders) {
        let air = !self.grounded;
        let release = if air {
            t::SPIN_CAP_THROW_FRAME_AIR
        } else if self.throwing.count > 0 {
            2
        } else {
            t::SPIN_CAP_THROW_FRAME
        };
        if let Some(kind) = self.throwing.pending
            && self.frame >= release
        {
            self.throwing.pending = None;
            self.launch_cap(kind);
        }
        if air {
            let h = self.v.with_y(0.).clamp_length_max(
                t::SPIN_AIR_SPEED_MAX.max(self.max_air_speed.min(t::SPIN_AIR_SPEED_MAX)),
            );
            self.set_h(h);
            self.v.y = (self.v.y - t::SPIN_AIR_GRAVITY).max(-t::FALL_MAX);
            if self.frame >= 24
                || self.throwing.pending.is_none()
                    && self.frame >= release + 6
                    && (self.pre_jump > 0 || o.crouch_pressed)
            {
                self.set(Move::Fall);
                self.gravity = t::GRAVITY_AIR;
                if o.crouch_pressed {
                    self.start_hip_drop();
                }
            }
        } else {
            if self.frame < t::SPIN_BRAKE_FRAMES {
                let h = self.v.with_y(0.) * t::SPIN_BRAKE_RATE;
                self.set_h(h);
            }
            self.v.y = -t::GRAVITY;
            // Jumping or crouching cancels once the cap is out. Moving cancels only
            // the tail of a plain throw; spin and motion throws play out in full.
            let length = clip::throw(self.anim.name);
            let after = self.throwing.pending.is_none() && self.frame >= release + 2;
            let tail = !self.anim.name.contains("Double") && self.frame * 4 >= length * 3;
            if after && (self.pre_jump > 0 || o.crouch)
                || tail && self.tilt > 0.3
                || self.anim_done(length)
            {
                self.throwing.timer = 30;
                self.enter(Move::Idle, "Wait");
                if self.pre_jump > 0 {
                    self.ground_jump(o);
                }
            }
        }
    }
    fn launch_cap(&mut self, kind: Throw) {
        let hand = self.pos + Vec3::Y * t::CAP_HEIGHT * UNIT;
        let dir = match kind {
            Throw::Up => Vec3::Y,
            Throw::Down if self.grounded => self.facing,
            Throw::Down => (self.facing * 0.5 - Vec3::Y).normalize(),
            _ => self.facing,
        };
        self.cap = Some(Cap {
            pos: hand,
            age: 0.,
            returning: false,
            kind,
            phase: CapPhase::Out,
            vel: dir * t::CAP_SPEED,
            dir,
            travelled: 0.,
            frames: 0,
            stay: 0,
        });
        self.events.push(Fx::Throw(kind));
    }
    fn update_cap(&mut self, o: &Orders, scene: &Course) {
        let hand = self.pos + Vec3::Y * t::CAP_HEIGHT * UNIT;
        let facing = self.facing;
        let Some(cap) = &mut self.cap else {
            return;
        };
        cap.frames += 1;
        cap.age = cap.frames as f32 / FPS;
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
            cap.dir = target
                .map(|t| (*t - cap.pos).normalize_or_zero())
                .or_else(|| Vec3::new(o.direction.x, 0., o.direction.y).try_normalize())
                .unwrap_or(cap.dir);
            cap.phase = CapPhase::Out;
            cap.travelled = t::CAP_REACH * 0.4;
            cap.vel = cap.dir * t::CAP_SPEED;
        }
        match cap.phase {
            CapPhase::Out => {
                if cap.kind == Throw::Spin {
                    let a = cap.frames as f32 * 0.25;
                    let r = t::CAP_SPIN_RADIUS * (cap.frames as f32 / 8.).min(1.) * UNIT;
                    cap.pos = hand + Vec3::new(a.cos(), 0., a.sin()) * r;
                    if cap.frames >= t::CAP_SPIN_FRAMES {
                        cap.phase = CapPhase::Return;
                    }
                } else {
                    let (reach, speed) = if cap.kind == Throw::Down && cap.dir.y.abs() < 0.1 {
                        (t::CAP_ROLL_REACH, t::CAP_ROLL_SPEED)
                    } else {
                        (t::CAP_REACH, t::CAP_SPEED)
                    };
                    // Constant speed, then a brake over the final stretch.
                    let brake_dist = speed * t::CAP_BRAKE_FRAMES / 2.;
                    let left = reach - cap.travelled;
                    let s = if left > brake_dist {
                        speed
                    } else {
                        (speed * left / brake_dist).max(1.5)
                    };
                    cap.vel = cap.dir * s;
                    cap.pos += cap.vel * UNIT;
                    cap.travelled += s;
                    if cap.travelled >= reach - 1. {
                        cap.phase = CapPhase::Stay;
                        cap.vel = Vec3::ZERO;
                    }
                }
            }
            CapPhase::Stay => {
                cap.stay += 1;
                if cap.stay >= t::CAP_STAY_MAX || cap.stay >= t::CAP_STAY && !o.throw_held {
                    cap.phase = CapPhase::Return;
                }
            }
            CapPhase::Return => {}
        }
        if cap.phase == CapPhase::Return {
            cap.returning = true;
            let to = hand - cap.pos;
            let speed = (cap.vel.length() + t::CAP_RETURN_ACCEL).min(t::CAP_RETURN_MAX);
            cap.vel = to.normalize_or_zero() * speed;
            let step = (speed * UNIT).min(to.length());
            cap.pos += to.normalize_or_zero() * step;
            if cap.pos.distance(hand) < 0.4 {
                self.cap = None;
                self.events.push(Fx::Catch);
                self.catch_window = t::CAP_CATCH_WINDOW;
                self.throwing.timer = 30;
                if !self.grounded && self.mode.airborne_jump() && self.v.y < 0. {
                    // Catching in midair gives a small pop.
                    self.v.y = t::CAP_CATCH_POP_POWER;
                    self.gravity = t::CAP_CATCH_POP_GRAVITY;
                    self.play("JumpCapCatchCommon");
                }
            }
        }
        let _ = facing;
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
        if ground {
            self.jump_with(
                Move::CapVault,
                "JumpCapLeapFrog",
                t::CAP_LEAPFROG_POWER,
                t::CAP_LEAPFROG_GRAVITY,
            );
        } else {
            self.jump_with(
                Move::CapBounce,
                "JumpCapLeapFrog",
                t::CAP_LEAPFROG_POWER_AIR,
                t::CAP_LEAPFROG_GRAVITY,
            );
        }
        self.set_h(self.facing * keep.min(t::JUMP_MOVE_MAX));
        self.max_air_speed = keep.clamp(t::JUMP_MOVE_MIN, t::JUMP_MOVE_MAX);
        self.jump_extend = false;
        self.cap_jump_possible = false;
        self.dive_possible = true;
        self.air_throw_used = false;
        self.hitstop = 0.035;
        self.cap = None;
        self.events.push(Fx::CapBounce);
    }

    // ---- Landing and collision ------------------------------------------------
    fn land(&mut self, o: &Orders) {
        let fall = (self.peak - self.pos.y) / UNIT;
        let speed = self.speed_h();
        self.land_frames = 0;
        self.jump_timer = 0;
        self.long_jump_timer = 0;
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
            Move::GroundPound => {
                self.enter(Move::PoundLand, "HipDropLand");
                self.hitstop = 0.05;
                self.events.push(Fx::PoundLand);
                return;
            }
            Move::SpinDrop => {
                self.enter(Move::Idle, "LandDownFall");
                self.events.push(Fx::PoundLand);
                return;
            }
            // Holding crouch turns the momentum into a real roll; otherwise a
            // dive lands in its forward roll and stands up.
            Move::Dive | Move::LongJump | Move::RollJump if o.crouch && speed > t::NORMAL_MIN => {
                self.start_roll(speed.max(t::ROLL_START), false);
            }
            Move::Dive => self.enter(Move::Slide, "LandRolling"),
            Move::Roll | Move::SlopeSlide => {}
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
                    self.enter(Move::Idle, "LandStiffen");
                } else if o.crouch {
                    self.start_squat();
                } else if self.tilt > 0.05 && speed > 0.5 {
                    self.set(Move::Run);
                    self.run_frames = t::BRAKE_ON_COUNTER;
                    self.play("Move");
                } else {
                    self.set_h(self.v.with_y(0.) * 0.3);
                    let anim = match mode {
                        Move::TripleJump => "LandJump3",
                        Move::Sideflip | Move::Backflip => "LandTurn",
                        _ => "Land",
                    };
                    self.enter(Move::Idle, anim);
                }
            }
        }
        self.events.push(Fx::Land {
            hard: self.hard_land,
        });
    }
    fn collide(&mut self, o: &Orders, scene: &Course) {
        let crouched = matches!(
            self.mode,
            Move::Crouch | Move::Crawl | Move::Roll | Move::RollEnd | Move::Slide | Move::Dive
        ) || self.grounded && o.crouch;
        let height = if crouched { SQUAT_HEIGHT } else { HEIGHT };
        let old = self.pos;
        let was_grounded = self.grounded;
        self.grounded = false;
        self.wall = Vec3::ZERO;
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
                            && self.pos.y <= top
                            && self.pos.y + height > lo.y
                        {
                            self.pos.y = top;
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
                            self.enter(Move::RollEnd, "RollingEnd");
                        }
                        if !was_grounded && self.v.y < 0. && into > 0.1 {
                            self.touch_wall(top, o);
                        }
                    }
                }
            }
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
            self.enter(Move::Fall, "Fall");
        }
        // Stay glued to ramps and steps down.
        if !self.grounded
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
            self.enter(Move::Ledge, "WallCatchStart");
            // The edge point, just inside the top, straight ahead of the body.
            let corner = self.pos - self.wall * (RADIUS + t::WALL_CATCH_INSET * UNIT);
            self.ledge = Vec3::new(corner.x, top, corner.z);
            self.facing = -self.wall;
            self.v = Vec3::ZERO;
            self.events.push(Fx::Ledge);
        } else if self.mode != Move::WallSlide {
            self.enter(Move::WallSlide, "WallKeep");
            self.facing = self.wall;
            self.v.y = self.v.y.max(-t::WALL_SLIDE_MAX * 0.5);
            self.events.push(Fx::WallSlide);
        }
    }
}

#[path = "tests/movement.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
