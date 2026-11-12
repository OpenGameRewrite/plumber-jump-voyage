//! Actor `setNerveOnGround` (SMO 1.0.0, 0x42184c..0x421a90).
//! Selection is shared by completed child states and landings; each state's
//! existing appear/first-step code still owns its animation and movement.
use super::*;

#[derive(Debug, PartialEq)]
pub(super) enum GroundNerve {
    Wait,
    Run,
    Squat,
    Jump,
    /// A higher-priority branch already selected its actor nerve.
    Changed,
}

impl Player {
    /// Continuing Wait/Run actor chains, after the child update and before pole/grab:
    /// ForceSlopeSlide, ForceRolling, SandSink, EnableStandUp (0x4225a4..0x4226b4,
    /// 0x4230e8..0x4231f8). Run's brake and turn children use the same actor chain.
    pub(super) fn try_wait_run_forced_action(&mut self, scene: &Course) -> bool {
        if self.nerve.deferred {
            return false;
        }
        if self.try_force_slope_slide(scene) || self.try_force_rolling() {
            return true;
        }
        if self.judge_sand_sink() {
            self.appear_sand_sink();
            return true;
        }
        if !self.judge_enable_stand_up() {
            // HoldCancel at 0x4226a4 / 0x4231d4, Squat at 0x4226b0 / 0x4231e0.
            // These chains do not cut velocity along the ground normal (unlike 0x4219f4).
            self.start_cancel_and_release(scene.mesh.as_deref());
            self.start_squat();
            return true;
        }
        false
    }

    /// `rs::isOnGroundRunAngle` (0x56989c): a collided floor the velocity is not leaving
    /// (the same departing test as `rs::isOnGround`), then the speed-dependent run angle
    /// of 0x569734 that `fire_run_angle_contact` evaluates, with the just-land axis.
    pub(super) fn is_on_ground_run_angle(&self) -> bool {
        self.is_on_ground_by(self.v) && self.fire_run_angle_contact()
    }

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
        let on_ground = if self.force_run.is_active() {
            self.is_on_ground_by(self.v)
        } else {
            self.is_on_ground_run_angle()
        };
        if !on_ground {
            self.go(Move::Fall, "Fall");
            return GroundNerve::Changed;
        }
        // +0x3f8 ForceSlopeSlide precedes +0x400 ForceRolling (0x4218b8..0x421918).
        if self.on_ground_force_slide_code(self.v) {
            // 0x4218c0..0x4218e8: a carried object is cancelled and released first.
            self.start_cancel_and_release(scene.mesh.as_deref());
            self.start_slope_slide();
            return GroundNerve::Changed;
        }
        if self.on_ground_force_rolling() {
            self.start_forced_rolling();
            return GroundNerve::Changed;
        }
        // +0x450 SandSink, +0x498 PoleClimb, +0x408 GrabCeil, in that order
        // (0x42191c, 0x421938, 0x421978). These ask the judges directly,
        // without the live actor nerve's cancellation restrictions.
        if self.judge_sand_sink() {
            self.appear_sand_sink();
            return GroundNerve::Changed;
        }
        if self.ground_pole_climb(o, scene) {
            return GroundNerve::Changed;
        }
        if self.ground_grab_ceil(scene) {
            return GroundNerve::Changed;
        }
        // +0x3e8 EnableStandUp, then judgeAndResetReturnTrue(+0x4a0)
        // PreInputJump (0x4219b0..0x4219d8). A low ceiling consumes no jump.
        if !self.judge_enable_stand_up() {
            // 0x4219e0: the squat cancels and releases a carried object (0x4219ec).
            self.start_cancel_and_release(scene.mesh.as_deref());
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
        if self.ground_start_run() {
            return GroundNerve::Run;
        }
        if !self.is_carry() && o.crouch && !self.force_run.is_active() && !force_land {
            // cutVerticalVelocityGroundNormal runs only on a just-land entry
            // from any actor nerve but LongJump (+0x78); other Squat entries
            // keep their velocity (0x421a6c..0x421a90).
            if self.is_just_land() && self.mode != Move::LongJump {
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
                // Run::appear raises its landing reaction before the ground
                // control changes velocity (0x47e898), including SandJump exits.
                if self.is_just_land() && !self.model_2d() {
                    self.note_velocity_at_run_choice(self.v);
                    self.events.push(Fx::Land { hard: false });
                }
                self.pull = GRAVITY_MOVE;
                self.run_appear(self.is_just_land());
                // GroundMove::appear writes tangent minus normal*GravityMove
                // (0x416b1c..0x416b5c). Store its pull separately like stand().
                self.v.y = if self.grounded { 0. } else { -GRAVITY_MOVE };
                // calcInitBrakeOnCounter (0x416be8), rather than assuming a
                // full brake counter even when the previous state stopped.
                let up = self.collided_ground_normal();
                self.ground
                    .reset(self.velocity_in_plane(up), self.pose_axes().1, up);
                self.run_frames = self.ground.brake_counter as u32;
                self.go(Move::Run, "Move");
            }
            GroundNerve::Wait => {
                if from == Move::RollEnd {
                    self.wait_state_appears_after_roll(self.v);
                }
                if self.force_land_on_ground() || self.is_just_land() && !self.model_2d() {
                    self.appear_ground_wait(self.v);
                } else {
                    self.change(Move::Idle, "Wait", |p| p.set_h(Vec3::ZERO));
                }
            }
        }
    }

    /// PlayerJudgeStartRun::judge (0x45b784), shared with Squat's StandUp child.
    pub(super) fn ground_start_run(&self) -> bool {
        self.is_collided_ground()
            && (self.force_run.is_active()
                || !self.force_land_on_ground() && (self.is_move() || self.auto_run_skate()))
    }

    /// ForceLand: LongFall (0x45a004) or action trigger 11 (0x4595dc).
    /// Bind/hack and nonvertical-gravity compositions retain their existing
    /// port limits; do not manufacture a bind/hack sensor from a movement mode.
    pub(super) fn force_land_on_ground(&self) -> bool {
        self.mode == Move::SpinDrop
            || !self.is_dead()
                && !self.model_2d()
                && self.bound_to.is_none()
                && self.fall.distance >= t::LONG_FALL
    }

    /// rs::cutVerticalVelocityGroundNormal (0x55ea50..0x55eaa0) calls
    /// al::parallelizeVec (0x923fe0): keep the normal component, not the tangent.
    fn cut_velocity_ground_normal(&mut self) {
        if self.is_collided_ground() {
            self.v = vector_lib::parallelize_vec(self.collided_ground_normal(), self.v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grounded() -> Player {
        Player {
            grounded: true,
            ground_contact: 2,
            v: Vec3::new(10., -3., 0.),
            ..default()
        }
    }

    #[test]
    fn original_priority_and_velocity_cut() {
        let scene = Course::default();
        let crouch = Orders {
            crouch: true,
            ..default()
        };
        let mut p = grounded();
        p.grounded = false;
        p.pre_jump = 4;
        assert_eq!(p.ground_nerve(&crouch, &scene), GroundNerve::Changed);
        assert_eq!(p.mode, Move::Fall);
        assert_eq!(p.pre_jump, 4, "no ground means no jump-buffer consumption");

        let mut p = grounded();
        p.v.y = 3.;
        assert_eq!(p.ground_nerve(&crouch, &scene), GroundNerve::Changed);
        assert_eq!(p.mode, Move::Fall, "departing contact is not on-ground");

        let mut p = grounded();
        p.ceiling.enable_stand_up = false;
        p.pre_jump = 4;
        p.tilt = 1.;
        assert_eq!(p.ground_nerve(&crouch, &scene), GroundNerve::Squat);
        assert_eq!(p.pre_jump, 4, "stand-up gate precedes jump");
        assert_eq!(p.v, Vec3::new(0., -3., 0.));

        let mut p = grounded();
        p.tilt = 1.;
        assert_eq!(p.ground_nerve(&crouch, &scene), GroundNerve::Run);
        p.pre_jump = 4;
        assert_eq!(p.ground_nerve(&crouch, &scene), GroundNerve::Jump);
        assert_eq!(p.pre_jump, 0);

        let mut p = grounded();
        p.mode = Move::LongJump;
        p.ground_contact = 1;
        let incoming = p.v;
        assert_eq!(p.ground_nerve(&crouch, &scene), GroundNerve::Squat);
        assert_eq!(p.v, incoming, "just-land LongJump skips the cut");
        p.ground_contact = 2;
        assert_eq!(p.ground_nerve(&crouch, &scene), GroundNerve::Squat);
        assert_eq!(
            p.v, incoming,
            "a Squat entry without just-land keeps its velocity"
        );
        p.mode = Move::Fall;
        p.ground_contact = 1;
        assert_eq!(p.ground_nerve(&crouch, &scene), GroundNerve::Squat);
        assert_eq!(
            p.v,
            Vec3::new(0., -3., 0.),
            "just-land from another nerve cuts"
        );

        let mut p = grounded();
        p.fall.distance = t::LONG_FALL;
        p.tilt = 1.;
        assert_eq!(p.ground_nerve(&crouch, &scene), GroundNerve::Wait);
        p.pre_jump = 4;
        assert_eq!(p.ground_nerve(&crouch, &scene), GroundNerve::Jump);
    }

    #[test]
    fn environment_priorities_precede_jump_and_crouch() {
        let scene = Course::default();
        let o = Orders {
            crouch: true,
            ..default()
        };
        for (floor, expected) in [
            ("Slide", Move::SlopeSlide),
            ("Rolling", Move::Roll),
            ("SandSink", Move::SandWait),
        ] {
            let mut p = grounded();
            p.mesh_ground = Some(Vec3::new(0., 0.8, -0.6));
            p.touched.floor = Some(crate::geometry::CollisionCodes {
                floor: floor.into(),
                ..default()
            });
            p.pre_jump = 4;
            p.set_nerve_on_ground(&o, &scene);
            assert_eq!(p.mode, expected, "{floor} precedes buffered jump");
            assert_eq!(p.pre_jump, 4);
        }

        let mut p = grounded();
        p.grounded = false;
        p.force_run.setup(2, 38.);
        p.water_finder.found = true;
        p.water_finder.distance = 0.;
        assert_eq!(
            p.ground_nerve(&o, &scene),
            GroundNerve::Run,
            "surface-run precedes the absent-ground Fall branch"
        );
        p.set_nerve_on_ground(&o, &scene);
        assert_eq!(
            p.v.y, -GRAVITY_MOVE,
            "an airborne surface Run keeps the raw pull"
        );

        let mut p = grounded();
        p.mesh_ground = Some(Vec3::new(0., 0.25, -0.968_245_86));
        assert!(!p.fire_run_angle_contact());
        p.force_run.setup(2, 38.);
        assert_eq!(
            p.ground_nerve(&o, &scene),
            GroundNerve::Run,
            "positive ForceRun uses contact instead of the standing-angle gate"
        );
    }

    #[test]
    fn completed_stand_up_does_not_reapply_the_recorded_normal_pull_as_travel() {
        let mut p = grounded();
        let normal = Vec3::new(0., 0.8, -0.6);
        p.mesh_ground = Some(normal);
        p.v = -normal * t::GRAVITY;
        p.nerve.clear();
        p.start_clip("SquatEnd");
        p.anim.frame = clip::SQUAT_END as f32;
        p.run(&Orders::default(), &Course::default());
        assert_eq!(p.mode, Move::Idle);
        assert_eq!(p.units_per_frame(), -normal * t::GRAVITY);
    }
}
