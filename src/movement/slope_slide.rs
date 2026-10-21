//! The slope slide (`PlayerStateSlope`, nerve `Slide`) and the actor's `Slope` nerve around it
//! (`PlayerActorHakoniwa::exeSlope`), with the judge that forces it
//! (`PlayerJudgeForceSlopeSlide`, the floor code `Slide`) and the jump block that goes with it
//! (`PlayerJudgePreInputJump::judge`).
//!
//! `appear` lays the velocity along the way down and raises it to a speed of 5; every step
//! `PlayerActionSlopeSlideControl::update` (see `slide_control.rs`) speeds it up, steers it and
//! lets gravity pull in the air. On flat ground the state ends once the slide is slow
//! enough and `SlopeForceFrame` steps have passed; a landing from a long enough flight on
//! flat ground ends it at once. The velocity is the game's own, the pull into the floor
//! included, and the collider moves the body by all of it.
//!
//! Written from the decompilation (`PlayerStateSlope.cpp`) and the 1.0.0 machine code of
//! `exeSlope` and `rs::isOnGround*`, checked on the recordings `slope-slide-forced` and
//! `slope-slide-forced-jump`.
use super::slide_control::{SlideControl, SlideParams};
use super::vector_lib as lib;
use super::*;

/// `appear` raises the velocity along the way down to this (a literal of the function).
const START_SPEED: f32 = 5.;
/// Steps of flight the state remembers: a landing after this many ends the slide on flat
/// ground (`mTimeInAir` converges to it).
const AIR_FRAMES: i32 = 30;
/// `isEnableCancelSandSink`'s counter limit is not needed here (no sand).
/// The forced slide's side control and turn (`SlopeSlideForce*`).
const FORCE_SIDE_ACCEL: f32 = 0.2;
const FORCE_SIDE_BRAKE: f32 = 0.96;
const FORCE_SIDE_MAX_SPEED: f32 = 3.;
const FORCE_TURN_DEGREE: f32 = 15.;

/// What `PlayerStateSlope` keeps.
#[derive(Clone, Debug, Default)]
pub(super) struct SlopeSlide {
    control: SlideControl,
    /// `mTimeInAir`: steps since the last one on the ground, up to `AIR_FRAMES`.
    time_in_air: i32,
    /// `mSlopeForceCounter`: steps spent on ground flat enough to end the slide.
    force_counter: i32,
    /// `mIsForceSlide`: the floor has had the `Slide` code at some time in this state.
    is_force: bool,
    /// Steps the `Slide` nerve has executed (`al::getNerveStep` of the state).
    pub(super) steps: u32,
}

/// `al::addVelocityToDirection`: add `force` along the unit vector of `dir`.
fn add_velocity_to_direction(velocity: Vec3, dir: Vec3, force: f32) -> Vec3 {
    let (unit, _) = pose_math::try_normalize_or_zero(dir);
    Vec3::new(
        force * unit.x + velocity.x,
        force * unit.y + velocity.y,
        force * unit.z + velocity.z,
    )
}

impl Player {
    /// The slope slide is the actor's nerve: its velocity is the game's own and the collider
    /// takes the squat form, from the step that chose it on.
    pub(super) fn slide_running(&self) -> bool {
        self.mode == Move::SlopeSlide
    }

    /// `PlayerJudgePreInputJump::judge`: a jump press still buffered counts, except while the
    /// forced slope slide judges (the buffer runs down all the same).
    pub(super) fn pre_input_jump_judges(&self) -> bool {
        self.pre_jump > 0 && !self.on_ground_force_slide_code(self.units_per_frame())
    }

    /// The actor's `PlayerJudgeForceSlopeSlide` in a ground nerve: standing on a floor with the
    /// `Slide` code takes the actor to the `Slope` nerve. True when it did.
    pub(super) fn try_force_slope_slide(&mut self) -> bool {
        if self.nerve.deferred || !self.on_ground_force_slide_code(self.units_per_frame()) {
            return false;
        }
        self.start_slope_slide();
        true
    }

    /// Choose the `Slope` nerve. The nerve machine starts the state in the same update that
    /// chose it, after the choosing nerve has run: `PlayerStateSlope::appear` takes the velocity
    /// the old state left, and the step's collision already moves the body by the velocity it
    /// sets. The first `exeSlide` comes with the next step.
    pub(super) fn start_slope_slide(&mut self) {
        self.slope_slide_appear();
        self.go(Move::SlopeSlide, "Slide");
    }

    /// `PlayerStateSlope::appear`.
    fn slope_slide_appear(&mut self) {
        let mut velocity = self.units_per_frame();
        let mut state = SlopeSlide {
            control: self.slide_control_setup(),
            ..SlopeSlide::default()
        };
        if self.on_ground_force_slide_code(velocity) {
            velocity = Self::lay_along(state.control.dir_slide, velocity);
            state.is_force = true;
        }
        let dir = state.control.dir_slide;
        let along = pose_math::dot(dir, velocity);
        if along < START_SPEED {
            velocity = add_velocity_to_direction(velocity, dir, START_SPEED - along);
        }
        self.v = lib::limit_length(velocity, t::SLOPE_SLIDE_MAX);
        self.slide = state;
    }

    /// The velocity along the way down only (zero where there is none).
    fn lay_along(dir: Vec3, velocity: Vec3) -> Vec3 {
        if lib::is_near_zero(dir, 0.001) {
            Vec3::ZERO
        } else {
            lib::parallelize_vec(dir, velocity)
        }
    }

    /// `exeSlope`: the state's step, then the judges the actor asks.
    pub(super) fn slope_slide(&mut self, o: &Orders) {
        if self.slope_slide_step() {
            self.end_slope_slide(o);
            return;
        }
        // (The ground roll's start is refused on a floor that forces the slide.)
        if o.crouch
            && o.throw_pressed
            && self.is_collided_ground()
            && !self.on_ground_force_slide_code(self.v)
        {
            self.pre_throw = 0;
            self.start_roll(RollStart::Normal);
            return;
        }
        if self.is_collided_ground() && self.pre_input_jump_judges() {
            self.ground_jump(o);
        }
    }

    /// `PlayerStateSlope::exeSlide`. True when the state killed itself.
    fn slope_slide_step(&mut self) -> bool {
        let steps = self.slide.steps;
        self.slide.steps += 1;
        if steps == 0 {
            self.slide.time_in_air = 0;
            self.slide.force_counter = 0;
        }
        let force_code = self.on_ground_force_slide_code(self.v);
        if !self.slide.is_force && force_code {
            self.v = Self::lay_along(self.slide.control.dir_slide, self.v);
            self.slide.is_force = true;
        }
        let params = SlideParams {
            accel: t::SLOPE_SLIDE_ACCEL,
            brake: if force_code { 0. } else { t::SLOPE_SLIDE_BRAKE },
            against: 0.,
            angle_power_max: 0.,
            max_speed: t::SLOPE_SLIDE_MAX,
            side_accel: if force_code {
                FORCE_SIDE_ACCEL
            } else {
                t::SLOPE_SLIDE_SIDE_ACCEL
            },
            side_brake: if force_code {
                FORCE_SIDE_BRAKE
            } else {
                t::SLOPE_SLIDE_SIDE_BRAKE
            },
            side_max_speed: if force_code {
                FORCE_SIDE_MAX_SPEED
            } else {
                t::SLOPE_SLIDE_SIDE_MAX
            },
            turn_degree: if self.slide.is_force {
                FORCE_TURN_DEGREE
            } else {
                t::SLOPE_TURN_DEG
            },
            gravity: GRAVITY_MOVE,
            rolling: false,
        };
        let mut control = self.slide.control;
        self.v = self.slide_control_update(&mut control, self.v, &params);
        self.slide.control = control;

        if self.on_ground_slope_slide_end(self.v) {
            if self.is_just_land() && self.slide.time_in_air >= AIR_FRAMES {
                return true;
            }
            self.slide.force_counter =
                lib::converge(self.slide.force_counter, t::SLOPE_FORCE_FRAMES as i32, 1);
            let fast = self.slide.control.horizontal_speed() > t::SLOPE_SLIDE_SPEED_END;
            if !fast && steps >= t::SLOPE_FORCE_FRAMES {
                return true;
            }
        } else {
            self.slide.force_counter = 0;
            self.slide.time_in_air = if self.is_collided_ground() {
                0
            } else {
                lib::converge(self.slide.time_in_air, AIR_FRAMES, 1)
            };
        }
        false
    }

    /// The state killed itself: `setNerveOnGround` picks what the ground allows. Only the
    /// outcomes the controller has are chosen (the floor that still forces the slide, a buffered
    /// jump, the squat, the run and standing).
    fn end_slope_slide(&mut self, o: &Orders) {
        if self.on_ground_force_slide_code(self.v) {
            self.start_slope_slide();
        } else if self.pre_input_jump_judges() {
            self.pre_jump = 0;
            self.ground_jump(o);
        } else if o.crouch {
            self.start_squat();
        } else if self.is_move() {
            self.run_frames = t::BRAKE_ON_COUNTER;
            self.pull = GRAVITY_MOVE;
            self.run_appear(false);
            self.go(Move::Run, "Move");
        } else {
            self.change(Move::Idle, "Wait", |p| p.set_h(Vec3::ZERO));
        }
    }
}
