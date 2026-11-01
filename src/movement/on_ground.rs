//! Actor `setNerveOnGround` (SMO 1.0.0, 0x42184c..0x421a90).
//! Selection is shared by completed child states and landings; each state's
//! existing appear/first-step code still owns its animation and movement.
use super::*;

pub(super) enum GroundNerve {
    Wait,
    Run,
    Squat,
    Jump,
    /// A higher-priority branch already selected its actor nerve.
    Changed,
}

impl Player {
    /// Original order, including the stand-up gate before buffered jump.
    pub(super) fn ground_nerve(&mut self, o: &Orders, scene: &Course) -> GroundNerve {
        // +0x488 StartWaterSurfaceRun, then +0x410 InWater (0x42185c..0x421880).
        if self.start_surface_run() {
            return GroundNerve::Run;
        }
        if self.is_in_water(scene, swim::ACTOR_JUDGE) {
            self.start_swim(scene);
            return GroundNerve::Changed;
        }
        // +0x1d0 PlayerCounterForceRun: positive uses isOnGround (0x568788),
        // otherwise isOnGroundRunAngle (0x56989c); 0x421884..0x4218b4.
        let on_ground = self.is_on_ground_by(self.v)
            && (self.force_run.is_active() || self.fire_run_angle_contact());
        if !on_ground {
            self.go(Move::Fall, "Fall");
            return GroundNerve::Changed;
        }
        // +0x3f8 ForceSlopeSlide precedes +0x400 ForceRolling (0x4218b8..0x421918).
        if self.on_ground_force_slide_code(self.v) {
            // startCancelAndRelease (0x42e9b0) is not ported. Preserve its
            // priority and held ownership, and expose the unsupported release.
            self.ground_cancel_carry();
            self.start_slope_slide();
            return GroundNerve::Changed;
        }
        if self.try_force_rolling() {
            return GroundNerve::Changed;
        }
        // +0x450 SandSink, +0x498 PoleClimb, +0x408 GrabCeil, in that order
        // (0x42191c, 0x421938, 0x421978). These ask the judges directly,
        // without the live actor nerve's cancellation restrictions.
        if self.judge_sand_sink() {
            self.appear_sand_sink();
            return GroundNerve::Changed;
        }
        self.ground_pole_climb(o, scene);
        if self.nerve.deferred {
            return GroundNerve::Changed;
        }
        self.ground_grab_ceil(scene);
        if self.nerve.deferred {
            return GroundNerve::Changed;
        }
        // +0x3e8 EnableStandUp, then judgeAndResetReturnTrue(+0x4a0)
        // PreInputJump (0x4219b0..0x4219d8). A low ceiling consumes no jump.
        if !self.judge_enable_stand_up() {
            self.ground_cancel_carry();
            self.cut_velocity_ground_normal();
            return GroundNerve::Squat;
        }
        if self.pre_input_jump_judges() {
            self.pre_jump = 0;
            return GroundNerve::Jump;
        }
        // +0x478 StartRun already includes ForceLand and forced-run/skate
        // gates (judge 0x45b784). +0x480 StartSquat, then cached ForceLand
        // (+0x3f0) vetoes it (0x421a1c..0x421a68).
        let force_land = self.force_land_on_ground();
        if self.is_collided_ground()
            && (self.force_run.is_active()
                || !force_land && (self.is_move() || self.auto_run_skate()))
        {
            return GroundNerve::Run;
        }
        if !self.is_carry() && o.crouch && !self.force_run.is_active() && !force_land {
            // JustLand, then actor LongJump (+0x78): only that combination
            // bypasses cutVerticalVelocityGroundNormal (0x421a6c..0x421a90).
            if !self.is_just_land() || self.mode != Move::LongJump {
                self.cut_velocity_ground_normal();
            }
            return GroundNerve::Squat;
        }
        GroundNerve::Wait
    }

    pub(super) fn set_nerve_on_ground(&mut self, o: &Orders, scene: &Course) {
        let from = self.mode;
        match self.ground_nerve(o, scene) {
            GroundNerve::Changed => {}
            GroundNerve::Jump => self.ground_jump(o),
            GroundNerve::Squat => {
                self.start_squat();
            }
            GroundNerve::Run => {
                self.run_frames = t::BRAKE_ON_COUNTER;
                self.pull = GRAVITY_MOVE;
                self.run_appear(false);
                self.go(Move::Run, "Move");
            }
            GroundNerve::Wait => {
                if from == Move::RollEnd {
                    self.wait_state_appears_after_roll(self.v);
                }
                self.change(Move::Idle, "Wait", |p| p.set_h(Vec3::ZERO));
            }
        }
    }

    /// ForceLand: LongFall (0x45a004) or action trigger 11 (0x4595dc).
    /// Bind/hack and nonvertical-gravity compositions retain their existing
    /// port limits; do not manufacture a bind/hack sensor from a movement mode.
    fn force_land_on_ground(&self) -> bool {
        self.mode == Move::SpinDrop
            || !self.is_dead() && !self.model_2d() && self.fall.distance >= t::LONG_FALL
    }

    /// rs::cutVerticalVelocityGroundNormal (0x55ea50..0x55eaa0) calls
    /// al::parallelizeVec (0x923fe0): keep the normal component, not the tangent.
    fn cut_velocity_ground_normal(&mut self) {
        if self.is_collided_ground() {
            self.v = vector_lib::parallelize_vec(self.collided_ground_normal(), self.v);
        }
    }

    fn ground_cancel_carry(&mut self) {
        if self.is_carry() {
            self.carry.reconstruction_release_unavailable = true;
        }
    }
}
