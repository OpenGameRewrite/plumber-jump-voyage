//! The stick-spin family: the ground spin (`PlayerStateGroundSpin`), the spin
//! jump (the `JumpSpinGround` nerve of `PlayerStateJump`) and the spin drop
//! (its `JumpSpinGroundDownFall` nerve), with the judge that starts them.
//!
//! The ground spin has a public decompilation; the jump nerves do not, so
//! they are a reading of the 1.0.0 executable (`exeJumpSpinGround`,
//! `exeJumpSpinGroundDownFall`, `updateNerveDownFall`, `appear`) checked
//! against the recordings `spin-ground-left/right`, `spin-jump-left/right`
//! and `spin-jump-drop`.
//!
//! How the three fit together:
//!
//! * A circling stick makes the player's input report a spin
//!   (`crate::spin_input`). Only the run state looks at it
//!   (`PlayerJudgeStartGroundSpin`: a spin and the ground under the feet), in
//!   every step of its nerve after the run's own movement, so the spin can
//!   begin in the middle of a pivot. The ground spin is the run state's other
//!   nerve (`GroundSpin3D`), so the actor nerve and its step count go on
//!   through the change.
//! * The ground spin lasts 90 steps whatever the stick does: the velocity
//!   brakes by 0.95, the stick pushes it by 0.5 within the larger of its
//!   own length and 8, and the body faces the way it moves. It ends in the
//!   wait state, or back in the run when the stick is held.
//! * The actor's own judges stay active during the spin. A jump press makes the
//!   spin jump (the request carries the spin's direction, which picks the
//!   left or right clips); a cap throw is tested first and wins; the crouch
//!   button starts the squat.
//! * The spin jump leaves with `SpinJumpPower`, falls with `SpinJumpGravity`,
//!   steers like any jump but is capped at `SpinJumpMoveSpeedMax`, and the
//!   shared air control does the rest. Pressing the crouch button (the
//!   hip-drop trigger) starts the spin drop instead of the ground pound.
//! * The spin drop falls straight down from `SpinJumpDownFallInitSpeed`,
//!   gaining `SpinJumpDownFallPower` a step up to the maximum. After
//!   `DownFallFrameMin` steps it ends when the button is no longer held: on the
//!   ground it is the landing, in the air the jump nerve restarts (the
//!   `Restart` clip) falling at `FallSpeedMax` with no horizontal speed.
//!
//! Not ported because nothing in the rewrite uses it: the wall-hit inertia
//! (`rs::scaleVelocityInertiaWallHit`), the ceiling and upper-punch
//! reflections of the jump nerve, the hip-drop message counters of the drop
//! (`convergeOnGroundCount`) and the stick-down pose adjustment of its landing.
use super::*;

/// A velocity shorter than this has no direction (`al::isNearZero`).
const NO_DIRECTION: f32 = 0.001;

/// The spin direction a clip of this family belongs to: left clips are
/// counter-clockwise, right clips clockwise.
pub(super) fn clip_is_clockwise(clip: &str) -> Option<bool> {
    match clip {
        "SpinGroundR" | "StartSpinJumpR" | "RestartSpinJumpR" | "SpinJumpDownFallR" => Some(true),
        "SpinGroundL" | "StartSpinJumpL" | "RestartSpinJumpL" | "SpinJumpDownFallL" => Some(false),
        _ => None,
    }
}

impl Player {
    /// The stick-circle detector reads the raw stick every frame; the
    /// controller's `stick` is that stick with +y pointing down the screen.
    pub(super) fn read_spin_input(&mut self, o: &Orders) {
        self.spin_input.update(Vec2::new(o.stick.x, -o.stick.y));
    }

    /// The clip for the spin's direction.
    fn spin_clip(&self, left: &'static str, right: &'static str) -> &'static str {
        if self.spin_clockwise { right } else { left }
    }

    /// `PlayerJudgeStartGroundSpin::judge`, asked by every step of the run
    /// state: a spin in the input and the ground under the feet. Returns
    /// whether the spin was chosen.
    pub(super) fn try_start_ground_spin(&mut self) -> bool {
        if !(self.spin_input.is_spinning() && self.grounded) {
            return false;
        }
        self.spin_clockwise = self.spin_input.is_clockwise();
        // `PlayerStateGroundSpin::appear` runs in the step that chooses the
        // spin (`al::NerveKeeper::update` applies a changed nerve at the end
        // of the step): its ground move control keeps only the part of the
        // velocity along the floor and no pull, so for this one step the
        // player has no downward push and the ground contact is lost.
        self.v.y = 0.;
        self.pull = 0.;
        self.defer(
            Move::GroundSpin,
            self.spin_clip("SpinGroundL", "SpinGroundR"),
            |_| {},
        );
        true
    }

    /// One step of the spin (`PlayerStateGroundSpin::exeGroundSpin`).
    pub(super) fn ground_spin(&mut self, o: &Orders) {
        // The ground move control hands over the velocity along the floor; the
        // spin brakes it, lets the stick push it, and keeps it within the
        // larger of its own length and the spin's speed.
        let mut h = self.v.with_y(0.) * t::GROUND_SPIN_BRAKE_RATE;
        let limit = h.length().max(t::GROUND_SPIN_MOVE_MAX);
        h += self.ground_stick() * t::GROUND_SPIN_ACCEL_RATE;
        h = h.clamp_length_max(limit);
        self.set_h(h);
        self.v.y = -GRAVITY_MOVE;
        self.pull = GRAVITY_MOVE;
        // `rs::slerpUpFront` smooths only the up axis; the front is the
        // direction of travel at once (a velocity too small to have a
        // direction leaves it).
        if h.length() >= NO_DIRECTION {
            self.facing = h.normalize();
        }
        if self.frame >= t::GROUND_SPIN_FRAMES {
            if self.is_move() {
                // `exeGroundSpin3D`: with the stick held the run state carries
                // on in its run nerve. GUESS: it restarts the run clip with the
                // speed the spin left (no recording ends a spin this way).
                self.pull = GRAVITY_MOVE;
                self.defer(Move::Walk, "Move", |_| {});
            } else {
                // The state ends and the actor chooses the wait state.
                self.end_run();
                return;
            }
        }
        self.ground_actions(o);
    }

    /// The jump press during the ground spin: the actor fills the jump
    /// request with the spin's direction (`jumpType` 12) and enters the jump
    /// state, which starts `JumpSpinGround`.
    pub(super) fn spin_jump(&mut self) {
        // The combo bookkeeping `appear` does for every jump that is not a
        // special one: the jump counts toward the combo when it lands.
        self.pick_jump_kind();
        let clip = self.spin_clip("StartSpinJumpL", "StartSpinJumpR");
        self.jump_with(
            Move::SpinJump,
            clip,
            t::SPIN_JUMP_POWER,
            t::SPIN_JUMP_GRAVITY,
            |p| p.jump_extend = false,
        );
    }

    /// The hip-drop trigger during the spin jump (`isTriggerHipDrop`, checked
    /// right after the air control's update): the jump nerve hands over to the
    /// fall. The spin drop is a nerve of the jump state, so the actor nerve's
    /// steps go on.
    pub(super) fn start_spin_drop(&mut self) {
        let clip = self.spin_clip("SpinJumpDownFallL", "SpinJumpDownFallR");
        self.defer(Move::SpinDrop, clip, |p| {
            // `setVelocityToGravity`: straight down, whatever the jump had.
            p.v = Vec3::NEG_Y * t::SPIN_DOWN_FALL_INIT;
        });
    }

    /// One step of the spin drop (`PlayerStateJump::updateNerveDownFall`).
    pub(super) fn spin_drop(&mut self, o: &Orders) {
        // `addVelocityToGravityLimit`.
        air_move::add_vector_limit(
            &mut self.v,
            Vec3::NEG_Y * t::SPIN_DOWN_FALL_POWER,
            t::SPIN_DOWN_FALL_MAX,
        );
        // Once the minimum is over and the button is let go, a drop that has
        // not reached the ground hands back to `JumpSpinGround`. Reaching the
        // ground ends the drop instead; the step that sees the contact is the
        // landing, which `land` handles before this runs.
        if self.frame >= t::SPIN_DOWN_FALL_FRAME_MIN && !o.crouch {
            let clip = self.spin_clip("RestartSpinJumpL", "RestartSpinJumpR");
            self.defer(Move::SpinJump, clip, |p| {
                // The nerve's first step with its restart flag: the air
                // control starts over, then the velocity is set straight down
                // at the fall limit, so no horizontal speed is left.
                p.arm_air_control(Move::SpinJump, t::SPIN_JUMP_POWER, t::SPIN_JUMP_GRAVITY);
                p.v = Vec3::NEG_Y * t::FALL_MAX;
            });
        }
    }
}
