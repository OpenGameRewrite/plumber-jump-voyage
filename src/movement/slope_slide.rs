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
    /// Lockstep: the counter a recorded slope slide has built up.
    pub(super) fn resume_slope_slide(&mut self, s: &Snapshot) {
        if self.mode == Move::SlopeSlide {
            self.slide.force_counter =
                (s.slope_flat_steps as i32).min(t::SLOPE_FORCE_FRAMES as i32);
        }
    }
    /// PlayerStateSlope::isEnableCancelSandSink (0x481328).
    pub(super) fn sand_slope_cancel(&self) -> bool {
        self.slide.force_counter > 15
    }
    /// `PlayerJudgePreInputJump::judge`: a jump press still buffered counts, except while the
    /// forced slope slide judges (the buffer runs down all the same).
    pub(super) fn pre_input_jump_judges(&self) -> bool {
        self.pre_jump > 0 && !self.on_ground_force_slide_code(self.units_per_frame())
    }

    /// The actor's `PlayerJudgeForceSlopeSlide` (+0x3f8): standing on a floor with the `Slide`
    /// code takes the actor to the `Slope` nerve. True when it did. Every actor nerve that asks
    /// this judge (the wait 0x4225a4, the run 0x4230e8, the squat 0x422b08, the jump, fall, hip
    /// drop, head sliding, rise, damage, sand sink, hack end, bind and spin-cap states, and
    /// `setNerveOnGround`) cancels and releases a carried object before it chooses the slide, and
    /// before the slide's `appear` takes the velocity (0x42e9b0 behind `isCarry`).
    pub(super) fn try_force_slope_slide(&mut self, scene: &Course) -> bool {
        if self.nerve.deferred || !self.on_ground_force_slide_code(self.units_per_frame()) {
            return false;
        }
        self.start_cancel_and_release(scene.mesh.as_deref());
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

    /// `exeSlope` (0x4235b0): the state's step, whose end hands the actor to `setNerveOnGround`
    /// (0x4235e4), then the judges the actor asks.
    pub(super) fn slope_slide(&mut self, o: &Orders, scene: &Course) {
        if self.slope_slide_step() {
            self.set_nerve_on_ground(o, scene); // exeSlope 0x4235e4.
            return;
        }
        self.slope_slide_judges(o);
    }

    /// The judges `exeSlope` asks after the state's step, in its order (0x4235ec..0x4236c8):
    /// `PlayerJudgeForceRolling` (+0x400, starts through 0x4229d0), the sand sink (+0x450) while
    /// `PlayerStateSlope::isEnableCancelSandSink` allows it (0x481328, 0x423610..0x42362c),
    /// `PlayerJudgeStartRolling` (+0x470), then the jump (collided ground and
    /// `rs::judgeAndResetReturnTrue` of `PlayerJudgePreInputJump`, +0x4a0, 0x423660..0x423680). The
    /// water entry (+0x410) is handled by the actor's general `judge_in_water` sweep here.
    /// [V] The last judge (+0x430, 0x4236c8..0x4236d0) is `PlayerJudgeInvalidateInputFall`: no
    /// ground contact and the position in an `InvalidateInputFallArea` (0x459c14 -> 0x42d670)
    /// silences the input and selects Fall (0x4236d4..0x4236f8, table 0x1d78910); it follows the
    /// general water sweep, so `judge_invalidate_input_fall_after_sweeps` asks it (see
    /// `invalidate_input.rs`). The jump the executable asks is the one recorded refused on the
    /// forced slide in `slope-slide-forced-jump`.
    fn slope_slide_judges(&mut self, o: &Orders) {
        if self.try_force_rolling() {
            return;
        }
        if self.sand_slope_cancel() && self.judge_sand_sink() {
            self.appear_sand_sink();
            return;
        }
        // `PlayerJudgeStartRolling::judge` (decomp, `PlayerJudgeStartRolling.cpp`): nothing in
        // the hands, not the 2D model, a collided ground that does not force the slide, then
        // `PlayerInput::isTriggerRolling`: crouch plus action, or a motion swing, or the
        // force-rolling floor argument. The floor case ran above; the swing branch remains
        // unrepresented by the input policy (R17).
        if !self.is_carry()
            && !self.model_2d()
            && self.is_collided_ground()
            && !self.on_ground_force_slide_code(self.v)
            && o.crouch
            && o.throw_pressed
        {
            self.pre_throw = 0;
            self.start_roll(RollStart::Normal);
            return;
        }
        // [V] exeSlope 0x423650..0x42365c: actor vtable +0xf8 is getPlayerCollision
        // (0x42241c, returns this+0x170), passed straight to isCollidedGround (0x423660).
        // It has no side effects; is_collided_ground below already uses that collision state.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sliding() -> Player {
        let mut p = Player {
            mode: Move::SlopeSlide,
            grounded: true,
            ground_contact: 2,
            v: Vec3::new(6., -3., 0.),
            ..default()
        };
        p.slide.force_counter = 16;
        p
    }

    /// exeSlope asks the sand sink (0x423610..0x42362c, while `isEnableCancelSandSink` holds, more
    /// than 15 counted steps) before the roll and the jump (0x423654..0x423680): a press of
    /// either on the frame the slide reaches sand sinks the player instead.
    #[test]
    fn the_sand_sink_is_asked_before_the_roll_and_the_jump() {
        let press = Orders {
            crouch: true,
            throw_pressed: true,
            ..default()
        };
        let mut p = sliding();
        p.sand.depth = 1.;
        p.pre_jump = 4;
        p.slope_slide_judges(&press);
        assert_eq!(p.mode, Move::SandWait);
        assert_eq!(
            p.pre_jump, 4,
            "the sink took the frame, the jump was not asked"
        );

        // At 15 counted steps the cancel is not allowed yet: the roll goes first, then the jump.
        let mut p = sliding();
        p.slide.force_counter = 15;
        p.sand.depth = 1.;
        p.pre_throw = 3;
        p.slope_slide_judges(&press);
        assert_eq!(p.mode, Move::Roll);
        assert_eq!(p.pre_throw, 0);
        let mut p = sliding();
        p.slide.force_counter = 15;
        p.sand.depth = 1.;
        p.pre_jump = 4;
        p.slope_slide_judges(&Orders::default());
        assert!(
            !matches!(p.mode, Move::SlopeSlide | Move::SandWait),
            "{:?}",
            p.mode
        );
        assert_eq!(p.pre_jump, 0);
    }

    /// `PlayerJudgeStartRolling::judge` refuses a carrying player and the 2D model.
    #[test]
    fn the_roll_judge_refuses_a_carried_object_and_the_2d_model() {
        let press = Orders {
            crouch: true,
            throw_pressed: true,
            ..default()
        };
        let mut p = sliding();
        p.slope_slide_judges(&press);
        assert_eq!(
            p.mode,
            Move::Roll,
            "the control: crouch and the trigger roll"
        );

        let mut p = sliding();
        p.carry.held = true;
        p.slope_slide_judges(&press);
        assert_eq!(p.mode, Move::SlopeSlide);

        let mut p = sliding();
        p.dimension.is_2d = true;
        p.slope_slide_judges(&press);
        assert_eq!(p.mode, Move::SlopeSlide);
    }
}
