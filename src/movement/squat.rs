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
}
impl Default for SquatControl {
    fn default() -> Self {
        Self {
            turn: TurnControl::crawl(),
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
                t::GRAVITY_AIR
            };
        self.v = along_front + along_side + up;
        if !on_ground {
            self.v.y = self.v.y.max(-t::FALL_MAX);
        }
        self.v.with_y(0.)
    }

    /// `rs::slerpUpFront`: the body swings a share of the way round to face
    /// `to`, the short way.
    pub(super) fn swing_front(&mut self, to: Vec3, share: f32) {
        let Some(to) = to.with_y(0.).try_normalize() else {
            return;
        };
        let angle = self.facing.angle_between(to);
        let turn = if self.facing.cross(to).y < 0. {
            -angle
        } else {
            angle
        };
        self.facing = Quat::from_rotation_y(turn * share) * self.facing;
    }

    /// `PlayerStateSquat::appear`: the nerve starts in `Brake`. Coming out of a
    /// roll the squat-start clip is skipped.
    pub(super) fn start_squat(&mut self) {
        let from_roll = self.clip() == "RollingEnd";
        self.change(
            Move::SquatBrake,
            if from_roll { "SquatWait" } else { "SquatStart" },
            |p| p.brake_first_step(),
        );
    }
    /// The first step of `exeBrake` before the slide: keep the momentum along
    /// the facing, grown by `SquatAccelRate` and dropped when it is too small
    /// to slide.
    fn brake_first_step(&mut self) {
        let vertical = Vec3::Y * self.v.y;
        let mut plane = self.v.with_y(0.);
        let front = self.facing.with_y(0.).normalize_or_zero();
        let mut along = front.dot(plane);
        if along < 0. && !self.on_ground_skate() {
            plane -= front * along;
            along = 0.;
        }
        let speed = plane.length();
        if speed <= t::NORMAL_MAX && along > 0. {
            let mut slid = t::SQUAT_ACCEL_RATE * speed;
            if slid <= t::SQUAT_BRAKE_END {
                slid = 0.;
            }
            plane *= slid / speed;
        }
        self.v = plane + vertical;
    }

    /// One step of the squat nerve, then the actor's decisions (`exeSquat`).
    pub(super) fn squat(&mut self, o: &Orders) {
        match self.mode {
            Move::SquatBrake => self.squat_brake(o),
            Move::Crouch => self.squat_wait(o),
            _ => self.squat_walk(o),
        }
        if !self.nerve.deferred && self.try_force_slope_slide() {
            return;
        }
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
        } else if !o.crouch {
            self.stand_up_squat();
        }
    }
    /// `exeWait`: crouch in place.
    fn squat_wait(&mut self, o: &Orders) {
        self.v.y = -t::GRAVITY;
        self.wait_pose();
        if !o.crouch {
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
        if !o.crouch {
            self.stand_up_squat();
        }
        if !self.is_move() {
            self.v = Vec3::NEG_Y * GRAVITY_MOVE;
            self.pull = GRAVITY_MOVE;
            self.defer(Move::Crouch, "SquatWait", |_| {});
            return;
        }
        let stick = self.ground_stick();
        let (actor_up, actor_front) = self.pose_axes();
        self.squat.turn.update(TurnFrame {
            actor_front,
            actor_up,
            stick,
            up: Vec3::Y,
        });
        let turned = self.squat.turn.direction;
        // Against the facing the crawl goes the stick's way at once while the
        // body turns round.
        let heading = if self.squat.turn.reverse_forced {
            stick.try_normalize().unwrap_or(turned)
        } else {
            turned
        };
        self.set_h(heading * t::SQUAT_WALK);
        self.v.y = -GRAVITY_MOVE;
        self.pull = GRAVITY_MOVE;
        self.swing_front(turned, SLERP_QUAT_RATE);
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
        if self.pre_input_jump_judges() {
            self.pre_jump = 0;
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
