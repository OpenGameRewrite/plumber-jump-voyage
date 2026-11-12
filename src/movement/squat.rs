//! The squat state (`PlayerStateSquat`) and the inertia slide it shares with
//! the roll's stand-up.
//!
//! Written from the decompiled state (`exeBrake`, `exeWait`, `exeWalk`,
//! `exeStandUp`) and from the 1.0.0 executable's `rs::moveInertiaSlide`, whose
//! helper `PlayerActionVelocityControl` is decompiled. The actor's own
//! decisions after the state has run (a roll, a jump) are `squat_judges`.
//!
//! The nerve is `Brake` (slide on the momentum), `Wait` (crouch in place),
//! `Walk` (crawl) and `StandUp`. A sub-state is chosen in one step and runs its
//! first step in the next, like every nerve (`src/nerve.rs`).
use super::ground::{TurnControl, TurnFrame};
use super::*;

/// `SquatBrakeSideAccel`, `SquatBrakeSideRate` and `SquatBrakeSideMaxSpeedRate`:
/// the stick pushes the slide sideways, the side speed otherwise fades and is
/// held to this share of the speed along the facing.
const BRAKE_SIDE_ACCEL: f32 = 0.25;
const BRAKE_SIDE_RATE: f32 = 0.93;
const BRAKE_SIDE_MAX_RATE: f32 = 0.5;
/// A stick component weaker than this does not push (`calcSideVelocityLimit`).
const SIDE_PUSH_EPSILON: f32 = 0.01;

/// What the squat state keeps between steps.
#[derive(Clone, Debug)]
pub(super) struct SquatControl {
    /// The crawl's turn controller (`PlayerActionTurnControl`, set up in the
    /// state's constructor).
    turn: TurnControl,
    /// The appearance decision survives a blocked stand-up returning to Brake.
    skip_start_clip: bool,
}
impl Default for SquatControl {
    fn default() -> Self {
        Self {
            turn: TurnControl::crawl(),
            skip_start_clip: false,
        }
    }
}

/// `PlayerActionFunction::calcStickPow`: a stick component without its dead
/// part, 0 to 1 in either direction.
fn stick_power(component: f32) -> f32 {
    component.signum() * ((component.abs() - 0.1) / 0.9).clamp(0., 1.)
}

impl Player {
    /// `rs::isOnGround`: touching the ground and not moving away from it.
    pub(super) fn is_on_ground(&self) -> bool {
        if !self.grounded {
            return false;
        }
        let away = self.v.normalize_or_zero().y;
        away <= 0. || away.abs() <= 0.001
    }
    /// `PlayerInput::isMove`: the stick is out of its dead zone.
    pub(super) fn is_move(&self) -> bool {
        self.tilt >= STICK_DEADZONE
    }

    /// `rs::moveInertiaSlide` on a level floor: the velocity is split along the
    /// facing, across it and up; the part along the facing loses `rate`, the
    /// part across is pushed by the stick or fades, and the pull of the floor
    /// is put in place of the vertical part. Returns the horizontal velocity.
    pub(super) fn inertia_slide(
        &mut self,
        rate: f32,
        side_accel: f32,
        side_rate: f32,
        side_max_rate: f32,
    ) -> Vec3 {
        let front = self.facing.with_y(0.).normalize_or(Vec3::Z);
        let side = Vec3::Y.cross(front).normalize_or_zero();
        let on_ground = self.is_on_ground();
        let mut along_front = front * self.v.dot(front);
        let mut along_side = side * self.v.dot(side);
        let mut up = self.v - along_front - along_side;
        if on_ground {
            // `calcOnGround`: nothing of the velocity may point out of the floor.
            up = Vec3::ZERO;
        }
        along_front *= rate;
        let push = stick_power(self.ground_stick().dot(side));
        if push.abs() <= SIDE_PUSH_EPSILON {
            along_side *= side_rate;
        } else {
            along_side += side * (push * side_accel);
            along_side = along_side.clamp_length_max(along_front.length() * side_max_rate);
        }
        up += Vec3::NEG_Y
            * if on_ground {
                t::GRAVITY
            } else {
                self.constants.gravity_air
            };
        self.v = along_front + along_side + up;
        if !on_ground {
            self.v.y = self.v.y.max(-t::FALL_MAX);
        }
        self.v.with_y(0.)
    }

    /// `PlayerStateSquat::appear`: the nerve starts in `Brake`. Coming out of a
    /// roll the squat-start clip is skipped.
    pub(super) fn start_squat(&mut self) {
        let from_roll = self.clip() == "RollingEnd";
        self.squat.skip_start_clip = from_roll;
        self.change(
            Move::SquatBrake,
            if from_roll { "SquatWait" } else { "SquatStart" },
            |p| p.brake_first_step(),
        );
    }
    /// Restore Brake's unexecuted first step, not just its visible mode.
    pub(super) fn resume_squat(&mut self, snapshot: &Snapshot) {
        if self.mode != Move::SquatBrake || snapshot.sub_steps.or(snapshot.state_frames) != Some(0)
        {
            return;
        }
        // exeBrake tests isFirstStep at 0x483710, so the pending first step is
        // queued again. A nested lockstep re-choice injects this Brake row
        // before choosing LongJump, whose first step reads the adjusted
        // velocity (0x474c48). The observed clip only selects the queued
        // nerve; `skip_start_clip` is set by `appear` alone and stays as is.
        let clip = snapshot.next_clip.unwrap_or(if self.squat.skip_start_clip {
            "SquatWait"
        } else {
            "SquatStart"
        });
        self.nerve.queue(clip, |p| p.brake_first_step());
    }
    /// The first step of `exeBrake` (0x483710) before the slide.
    fn brake_first_step(&mut self) {
        // 0x4836fc calls `rs::calcGroundNormalOrUpDir` (0x55ba38); 0x48379c reads the
        // pose front (`al::calcFrontDir`), which `pose_axes` gives from the stored pose.
        let (pose_up, front) = self.pose_axes();
        let up = self.ground_normal_or_pose_up(pose_up);
        self.v = brake_entry_velocity(self.v, front, up, self.on_ground_skate());
    }

    /// `rs::calcGroundNormalOrUpDir` (0x55ba38): the collided ground's normal
    /// (`isCollidedGround` 0x565e8c, `getCollidedGroundNormal` 0x565fb4) or, with
    /// no ground collided, the actor's own up (`al::calcUpDir`), which is the stored
    /// pose's up and not the world's. The port's collided normal is the mesh
    /// collider's floor normal; a box floor has no mesh normal, so the port
    /// derives one from its slope (`floor_normal`), an approximation of the
    /// normal the binary's collider would report there.
    fn ground_normal_or_pose_up(&self, pose_up: Vec3) -> Vec3 {
        if self.is_collided_ground() {
            self.collided_ground_normal()
        } else {
            pose_up
        }
    }

    /// One step of the squat nerve, then the actor's decisions (`exeSquat`).
    pub(super) fn squat(&mut self, o: &Orders, scene: &Course) {
        match self.mode {
            Move::SquatBrake => self.squat_brake(o),
            Move::Crouch => self.squat_wait(o),
            _ => self.squat_walk(o),
        }
        if !self.nerve.deferred && (self.try_force_slope_slide(scene) || self.try_force_rolling()) {
            return;
        }
        self.ground_inertia.request = 1;
        self.squat_judges(o);
    }
    /// `exeBrake`: slide on the momentum until it is spent.
    fn squat_brake(&mut self, o: &Orders) {
        let slide = if self.on_ground_skate() {
            self.inertia_slide(0.985, 0.1, 0.975, BRAKE_SIDE_MAX_RATE)
        } else {
            self.inertia_slide(
                t::SQUAT_BRAKE_RATE,
                BRAKE_SIDE_ACCEL,
                BRAKE_SIDE_RATE,
                BRAKE_SIDE_MAX_RATE,
            )
        };
        // `rs::slerpUp`: the pose's up follows the floor's normal.
        let (_, front) = self.pose_axes();
        self.steer_pose(self.mesh_ground.unwrap_or(Vec3::Y), front);
        if slide.length() <= t::SQUAT_BRAKE_END {
            // `al::setVelocityZeroH`: of the pull along the floor's normal only
            // its vertical part is left (all of it on level ground).
            let normal = self.mesh_ground.unwrap_or(Vec3::Y);
            self.v = Vec3::new(0., -t::GRAVITY * normal.y, 0.);
            self.defer(Move::Crouch, "SquatWait", |_| {});
        } else if !o.crouch && self.judge_enable_stand_up() {
            self.stand_up_squat();
        }
    }
    /// `exeWait`: crouch in place.
    fn squat_wait(&mut self, o: &Orders) {
        self.v.y = -t::GRAVITY;
        self.wait_pose();
        // A blocked stand-up does not end the crouch: the nerve goes on to the crawl.
        if !o.crouch && self.judge_enable_stand_up() {
            self.stand_up_squat();
        } else if self.is_move() {
            self.defer(Move::Crawl, "SquatWalk", |p| p.squat.turn.reset());
        }
    }
    /// `exeWalk`: crawl along the stick. The original forgets to leave after
    /// choosing the stand-up, so releasing crouch and the stick together
    /// chooses the stand-up and then the wait, which chooses the stand-up again
    /// in the next step.
    fn squat_walk(&mut self, o: &Orders) {
        let up = self.mesh_ground.unwrap_or(Vec3::Y);
        if !o.crouch && self.judge_enable_stand_up() {
            self.stand_up_squat();
        }
        if !self.is_move() {
            self.v = Vec3::NEG_Y * GRAVITY_MOVE;
            self.pull = GRAVITY_MOVE;
            self.defer(Move::Crouch, "SquatWait", |_| {});
            return;
        }
        let stick = self.plane_stick(up);
        let (actor_up, actor_front) = self.pose_axes();
        self.squat.turn.update(TurnFrame {
            actor_front,
            actor_up,
            stick,
            up,
        });
        let turned = self.squat.turn.direction;
        // Against the facing the crawl goes the stick's way at once while the
        // body turns round.
        let heading = if self.squat.turn.reverse_forced {
            stick.try_normalize().unwrap_or(turned)
        } else {
            turned
        };
        let velocity = heading * t::SQUAT_WALK;
        self.set_h(velocity);
        self.v.y = -GRAVITY_MOVE;
        self.pull = GRAVITY_MOVE;
        self.run_up = Some((
            Vec2::new(self.v.x, self.v.z),
            up,
            velocity - up * GRAVITY_MOVE,
        ));
        self.steer_pose(up, turned);
    }
    /// `PlayerJudgeEnableStandUp::judge` (0x459514): the ceiling check lets the body stand, or
    /// the collider has no ground under it (the check only matters while on the floor).
    pub(super) fn judge_enable_stand_up(&self) -> bool {
        self.ceiling.enable_stand_up || !self.is_collided_ground()
    }
    /// A blocked `exeStandUp` returns to Brake without calling appear again.
    /// Its first step uses the clip decision saved when the squat began.
    pub(super) fn stand_up_blocked(&mut self) {
        let clip = if self.squat.skip_start_clip {
            "SquatWait"
        } else {
            "SquatStart"
        };
        self.change(Move::SquatBrake, clip, |p| p.brake_first_step());
    }
    /// `setNerveStandUpOrKill`: the stand-up clip and the stop come with the
    /// next step; this step still acts.
    fn stand_up_squat(&mut self) {
        self.switch(Move::Idle, "SquatEnd", |p| p.set_h(Vec3::ZERO));
    }

    /// What the actor decides once the squat state has run: slide down a steep
    /// slope, roll (crouch held and the cap button pressed), or jump.
    fn squat_judges(&mut self, o: &Orders) {
        // (On a stage's collision the squat does not slide down a steep floor
        // although crouch is held: the recordings on the Cap Kingdom's dome stay
        // in the squat. Only the toy course's boxes have that rule.)
        if o.crouch && self.mesh_ground.is_none() && self.steep(t::SLOPE_SLIDE_ANGLE_START) {
            self.start_slope_slide();
            return;
        }
        if o.crouch && o.throw_pressed && self.grounded {
            self.pre_throw = 0;
            self.start_roll(RollStart::Normal);
            return;
        }
        // exeSquat resets an accepted pre-input before testing standing
        // clearance (0x422c40); a blocked press is consumed without jumping.
        if self.pre_input_jump_judges() {
            self.pre_jump = 0;
            if !self.judge_enable_stand_up() {
                return;
            }
            // Only the braking sub-state allows the long jump, and only while
            // moving forward (`PlayerStateSquat::isEnableLongJump`).
            if self.mode == Move::SquatBrake && self.v.dot(self.facing) > 0. {
                self.long_jump();
            } else {
                self.backflip();
            }
        }
    }
}

/// `exeBrake`'s first-step velocity adjustment (0x483760..0x48391c). `up` is
/// `rs::calcGroundNormalOrUpDir` (0x4836fc) and `front` is the pose
/// front, `al::calcFrontDir` (0x48379c). The velocity is separated into the part
/// along `up` and the rest (0x483788); the front is verticalized against
/// `up` (0x4837a0..0x4837ac) and normalized (0x4837b4). Moving backwards off
/// a skate floor drops the backward part (0x4837dc..0x483808). A rest that is
/// no faster than `NormalMaxSpeed` (0x483854) and moves forward (0x48384c) is
/// scaled by `SquatAccelRate` (0x483860), or stopped when that is not above
/// `SquatBrakeEnd` (0x4838c0..0x4838c8). The part along `up` is added back
/// unscaled (0x4838f0..0x48391c).
fn brake_entry_velocity(velocity: Vec3, front: Vec3, up: Vec3, on_skate: bool) -> Vec3 {
    let (along_up, mut plane) = vector_lib::separate_parallel_vertical(up, velocity);
    let front = vector_lib::verticalize_vec(up, front).normalize_or_zero();
    let mut along = front.dot(plane);
    if along < 0. && !on_skate {
        plane -= front * along;
        along = 0.;
    }
    let speed = plane.length();
    if speed <= t::NORMAL_MAX && along > 0. && speed > 0. {
        let mut slid = t::SQUAT_ACCEL_RATE * speed;
        if slid <= t::SQUAT_BRAKE_END {
            slid = 0.;
        }
        plane *= slid / speed;
    }
    plane + along_up
}

#[cfg(test)]
mod tests {
    use super::*;

    const TILTED_UP: Vec3 = Vec3::new(0.6, 0.8, 0.);

    /// On a floor tilted about Z the split is by its normal: the part along the
    /// normal is kept, the rest is scaled by `SquatAccelRate` (0x483860).
    #[test]
    fn brake_entry_splits_by_the_ground_normal() {
        let front = Vec3::new(0.8, -0.6, 0.);
        let rest = front * 5.;
        let along_up = TILTED_UP * 2.;
        let out = brake_entry_velocity(rest + along_up, front, TILTED_UP, false);
        let expected = rest * t::SQUAT_ACCEL_RATE + along_up;
        assert!((out - expected).length() < 1e-5, "{out:?} vs {expected:?}");
    }

    /// A flat facing on a slope is verticalized against the normal (0x4837a0):
    /// backward motion is cut along the slope front, leaving the normal part.
    #[test]
    fn brake_entry_cuts_backward_motion_along_the_slope_front() {
        let flat_facing = Vec3::X;
        let slope_front = vector_lib::verticalize_vec(TILTED_UP, flat_facing).normalize();
        let along_up = TILTED_UP * 1.5;
        let out = brake_entry_velocity(-slope_front * 3. + along_up, flat_facing, TILTED_UP, false);
        assert!((out - along_up).length() < 1e-5, "{out:?}");
    }

    /// With a diagonal front on a tilted floor, the pose front (in the floor
    /// plane, `calcFrontDir` 0x48379c) and the horizontal facing project to
    /// different directions, so only the pose front cuts this backward motion.
    #[test]
    fn brake_entry_reads_the_pose_front_on_a_diagonal_slope() {
        let root_half = std::f32::consts::FRAC_1_SQRT_2;
        let pose_front = Vec3::new(0.8, -0.6, 0.) * root_half + Vec3::Z * root_half;
        let facing = Vec3::new(1., 0., 1.).normalize();
        let along_up = TILTED_UP * 1.5;
        let velocity = -pose_front * 3. + along_up;
        let from_pose = brake_entry_velocity(velocity, pose_front, TILTED_UP, false);
        assert!((from_pose - along_up).length() < 1e-5, "{from_pose:?}");
        let from_facing = brake_entry_velocity(velocity, facing, TILTED_UP, false);
        assert!((from_facing - along_up).length() > 0.1, "{from_facing:?}");
    }

    /// Faster than `NormalMaxSpeed` (0x483854) the rest is left as it is.
    #[test]
    fn brake_entry_leaves_speeds_above_normal_max() {
        let velocity = Vec3::Z * (t::NORMAL_MAX + 1.);
        assert_eq!(
            brake_entry_velocity(velocity, Vec3::Z, Vec3::Y, false),
            velocity
        );
    }

    /// The scaled speed is kept only when it is above `SquatBrakeEnd` (`fcsel hi`
    /// at 0x4838c8), so one at or below it is stopped. The speeds are the two
    /// neighbouring floats where the scaled speed crosses that limit, found with
    /// the same product, so the test does not rely on the limit being representable.
    #[test]
    fn brake_entry_stops_when_the_scaled_speed_is_not_above_the_brake_end() {
        let scaled = |speed: f32| t::SQUAT_ACCEL_RATE * speed;
        let mut stopped_speed = t::SQUAT_BRAKE_END / t::SQUAT_ACCEL_RATE;
        while scaled(stopped_speed) > t::SQUAT_BRAKE_END {
            stopped_speed = f32::from_bits(stopped_speed.to_bits() - 1);
        }
        while scaled(f32::from_bits(stopped_speed.to_bits() + 1)) <= t::SQUAT_BRAKE_END {
            stopped_speed = f32::from_bits(stopped_speed.to_bits() + 1);
        }
        let kept_speed = f32::from_bits(stopped_speed.to_bits() + 1);
        assert!(scaled(stopped_speed) <= t::SQUAT_BRAKE_END);
        assert!(scaled(kept_speed) > t::SQUAT_BRAKE_END);
        let out = brake_entry_velocity(Vec3::Z * stopped_speed, Vec3::Z, Vec3::Y, false);
        assert_eq!(out, Vec3::ZERO);
        let out = brake_entry_velocity(Vec3::Z * kept_speed, Vec3::Z, Vec3::Y, false);
        assert!((out - Vec3::Z * scaled(kept_speed)).length() < 1e-5);
    }

    /// A tilted-floor Player whose stored pose faces the diagonal `front`
    /// (in the floor plane) while its horizontal facing differs from it.
    fn player_on_tilted_floor(front: Vec3) -> Player {
        let mut player = Player {
            grounded: true,
            mesh_ground: Some(TILTED_UP),
            ..Player::default()
        };
        player.set_pose(pose_math::quat_up_front(TILTED_UP, front));
        player
    }

    fn diagonal_floor_front() -> Vec3 {
        let root_half = std::f32::consts::FRAC_1_SQRT_2;
        Vec3::new(0.8, -0.6, 0.) * root_half + Vec3::Z * root_half
    }

    /// The first step reads the pose front and the collided floor's normal at its
    /// call site: backward motion along the pose front is cut and the part along
    /// the normal stays. With the horizontal facing as the front it would not be.
    #[test]
    fn the_first_brake_step_reads_the_pose_front_and_the_floor_normal() {
        let front = diagonal_floor_front();
        let mut player = player_on_tilted_floor(front);
        let along_up = TILTED_UP * 1.5;
        player.v = -front * 3. + along_up;
        player.brake_first_step();
        assert!((player.v - along_up).length() < 1e-4, "{:?}", player.v);
    }

    /// With no ground collided the up is the pose's (`al::calcUpDir`), not a stale
    /// mesh normal and not the world's up.
    #[test]
    fn the_first_brake_step_uses_the_pose_up_without_collided_ground() {
        let front = diagonal_floor_front();
        let mut player = player_on_tilted_floor(front);
        player.grounded = false;
        player.mesh_ground = Some(Vec3::Y);
        let along_up = TILTED_UP * 1.5;
        player.v = -front * 3. + along_up;
        player.brake_first_step();
        assert!((player.v - along_up).length() < 1e-4, "{:?}", player.v);
    }

    /// Off a skate floor backward motion is dropped (0x4837dc..0x483808); on one
    /// it is kept and, not moving forward, left unscaled.
    #[test]
    fn brake_entry_keeps_backward_motion_on_skate() {
        let velocity = -Vec3::Z * 5.;
        assert_eq!(
            brake_entry_velocity(velocity, Vec3::Z, Vec3::Y, true),
            velocity
        );
        assert_eq!(
            brake_entry_velocity(velocity, Vec3::Z, Vec3::Y, false),
            Vec3::ZERO
        );
    }
}
