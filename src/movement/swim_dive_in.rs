//! The dive into the water from above: `SwimHeadIn` (the head slide that reaches the water,
//! `EActionTrigger 15`) and `SwimDiveIn` (a high fall that started the `DiveInWater` clip,
//! trigger 9), the two inner nerves of `PlayerStateSwim` that `appear` picks for a fast fall
//! into near water.
//!
//! Written from the 1.0.0 executable and checked against `swim-lake-headin` (the head slide).
//! `SwimDiveIn` is the same nerve with the dive-in constants; no recording reaches it (the
//! `DiveInWater` clip needs a fall of 15 m), so it is unverified.
//!
//! Both nerves play `SwimHeadSlidingIn`, hold the fall speed to `SwimFallInSpeed`, brake the
//! dive (`rs::moveDiveSwim`), let the stick push across it (`rs::moveDivingJump` with the
//! travel's own speed as the limit), pitch the body to the direction it travels
//! (`PlayerJointParamSwim`'s first angle) and hand over to the landing, the walk, the surface
//! or the open water.
use super::swim::{SwimEnd, SwimNerve};
use super::swim_const as k;
use super::vector_lib as lib;
use super::wall_slide::angle_on_plane_degree;
use super::*;
use crate::air_move;

const GRAVITY: Vec3 = Vec3::NEG_Y;

/// The axes of the model's joints that the swim nerves read in their first step, as the last
/// pose calculation left them: `al::calcJointSideDir` of `Spine1` (its x axis) and the x and y
/// axes of `JointRoot` (`PlayerAnimator::calcModelJointRootMtx`).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ModelAxes {
    pub spine_side: Vec3,
    pub root_side: Vec3,
    pub root_up: Vec3,
}
impl ModelAxes {
    pub(super) fn of(world: &crate::skeleton_pose::Matrices) -> Self {
        use crate::skeleton_pose::Joint;
        let column = |joint: Joint, c: usize| {
            let m = &world[joint as usize];
            // (`al::normalize` / `tryNormalizeOrZero` of the column.)
            pose_math::try_normalize_or_zero(Vec3::new(m[0][c], m[1][c], m[2][c])).0
        };
        Self {
            spine_side: column(Joint::Spine1, 0),
            root_side: column(Joint::JointRoot, 0),
            root_up: column(Joint::JointRoot, 1),
        }
    }
}

/// The pitch of the body toward the travel is held between these (degrees).
const PITCH_LIMIT: f32 = 60.;

/// What tells the two nerves apart: their brakes and the rise a held stick gives.
#[derive(Clone, Copy)]
struct DiveInKind {
    /// The vertical brake while the dive is faster than `SwimFallSpeedMax` (and while a stick
    /// is held and the body falls).
    brake_v: f32,
    rise_power: f32,
    rise_speed_max: f32,
    /// Standing still, the head-in brakes the horizontal speed (`SwimBrakeRateH`); the dive-in
    /// leaves it.
    brakes_still: bool,
}
const HEAD_IN: DiveInKind = DiveInKind {
    brake_v: k::HEAD_IN_BRAKE_V,
    rise_power: k::HEAD_IN_RISE_POWER,
    rise_speed_max: k::HEAD_IN_RISE_SPEED_MAX,
    brakes_still: true,
};
const DIVE_IN: DiveInKind = DiveInKind {
    brake_v: k::DIVE_IN_BRAKE_V,
    rise_power: k::DIVE_IN_RISE_POWER,
    rise_speed_max: k::DIVE_IN_RISE_SPEED_MAX,
    brakes_still: false,
};

/// `PlayerJudgeDiveInWater`: the fall must have gone on for this far, the surface be this far below
/// and nothing be nearer under it than that (centimetres).
const DIVE_IN_FALL_DISTANCE_MIN: f32 = 10.;
const DIVE_IN_HEIGHT: f32 = 1500.;
/// `updateHeightCheck`'s arrow starts 30 above the root and runs 1735 down: ground further than
/// that is not "below" the root at all (`IUsePlayerHeightCheck::isAboveGround`).
const GROUND_PROBE_START: f32 = 30.;
const GROUND_PROBE_LENGTH: f32 = 1735.;

impl Player {
    // ---- The fall into the water that starts the dive-in ------------------------------------

    /// `PlayerJudgeDiveInWater::judge`: a fall that has gone on, with the water's surface 1500 to
    /// 1660 below the root and no ground nearer under it than the surface plus the body's height.
    pub(super) fn judge_dive_in_water(&self, scene: &Course) -> bool {
        if self.fall.distance < DIVE_IN_FALL_DISTANCE_MIN {
            return false;
        }
        let above_ground = self.ground_height + GROUND_PROBE_START <= GROUND_PROBE_LENGTH;
        if above_ground && self.ground_height < DIVE_IN_HEIGHT {
            return false;
        }
        let find_distance = (k::TALL + DIVE_IN_HEIGHT) * 0.5;
        let at = scene.in_water_frame(self.position_cm());
        let start = Vec3::new(
            at.x + find_distance * GRAVITY.x,
            at.y + find_distance * GRAVITY.y,
            at.z + find_distance * GRAVITY.z,
        );
        let Some((surface, _)) = scene
            .water_areas()
            .find_surface(start, Vec3::Y, find_distance)
        else {
            return false;
        };
        let surface_distance = pose_math::dot(at - surface, Vec3::Y);
        if surface_distance < DIVE_IN_HEIGHT {
            return false;
        }
        !(above_ground && self.ground_height < surface_distance + k::TALL)
    }

    /// `PlayerActionDiveInWater::judgeEnableDiveInWater`: the judge, unless the clip is already
    /// playing.
    pub(super) fn dive_in_water_enabled(&self, scene: &Course) -> bool {
        self.anim.name != "DiveInWater" && self.judge_dive_in_water(scene)
    }

    /// `PlayerActionDiveInWater::tryChangeDiveInWaterAnim`, which the jump states call every step:
    /// the fall into deep water plays its own clip (and the water entry then raises trigger 9).
    /// GUESS for the clip's play: no recording has the clip (the head slide's judge is recorded).
    pub(super) fn try_change_dive_in_water_anim(&mut self, scene: &Course) {
        if self.dive_in_water_enabled(scene) {
            self.start_clip("DiveInWater");
        }
    }

    /// The first step of both nerves: the clip, the fall speed held to the fall-in speed and
    /// the body's pitch starting from the angle the spine has with the body's front.
    fn start_dive_in(&mut self) {
        self.swim_anim("SwimHeadSlidingIn");
        self.v = super::swim_dive::limit_velocity_dir(self.v, GRAVITY, k::FALL_IN_SPEED);
        // `rs::calcSpineSideDir` is the side axis of the `Spine1` joint as the model last
        // posed it (`Player::pose_skeleton` keeps it).
        let pose = self.actor_pose();
        let angle = angle_on_plane_degree(pose.front(), self.model_axes.spine_side, pose.side())
            .clamp(-PITCH_LIMIT, PITCH_LIMIT);
        self.joints.swim.start_dive_in(angle);
        self.swim.dive_in_steer = true;
        self.swim.dive_in_keep = 0;
    }

    /// The helper both nerves call after their move (`0x488410`): the pitch of the body follows
    /// the direction it travels, seen from the side.
    fn follow_dive_in_travel(&mut self) {
        let pose = self.actor_pose();
        let (front, side) = (pose.front(), pose.side());
        // (The external velocity half added to the travel is nil in the water.)
        let travel = lib::verticalize_vec(side, self.v);
        let (mut dir, has_direction) = pose_math::try_normalize_or_zero(travel);
        if !has_direction || pose_math::is_parallel_direction(GRAVITY, dir, 0.01) {
            dir = front;
        }
        let angle = angle_on_plane_degree(front, dir, side).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        self.joints.swim.follow_travel(angle);
    }

    /// What the two nerves do to the velocity and the body every step. Returns how fast the
    /// body falls (the part of the velocity along the gravity) and whether it has slowed enough
    /// for the nerve's exits to be asked.
    fn dive_in_motion(&mut self, kind: DiveInKind) -> (f32, bool) {
        self.swim.ground_normal = self.ground_normal_or_up();
        let (down, across) = lib::separate_parallel_vertical(GRAVITY, self.v);
        let speed_h = lib::length(across);
        let falling = pose_math::dot(GRAVITY, down);
        let moving = self.is_move();
        let brake_h = if speed_h > k::HIGH_SPEED_MAX_H {
            // (The head-in reads the dive-in's brake: the executable calls that getter.)
            k::DIVE_IN_BRAKE_H
        } else if kind.brakes_still && !moving {
            k::BRAKE_RATE_H
        } else {
            1.
        };
        let (brake_v, accel, limit, slowed);
        if falling > k::FALL_SPEED_MAX {
            // Still fast: only the brakes work (the gravity added is nothing).
            brake_v = kind.brake_v;
            accel = 0.;
            limit = -falling;
            slowed = false;
        } else if moving && self.swim.dive_in_steer {
            // A held stick lifts the body toward the surface.
            brake_v = if falling <= 0. { 1. } else { kind.brake_v };
            accel = -kind.rise_power;
            limit = kind.rise_speed_max;
            slowed = true;
        } else {
            brake_v = 1.;
            accel = k::GRAVITY;
            limit = k::FALL_SPEED_MAX;
            self.swim.dive_in_steer = false;
            slowed = true;
        }
        self.move_dive_swim(brake_h, brake_v, 0.);
        lib::add_vector_limit(
            &mut self.v,
            Vec3::new(GRAVITY.x * accel, GRAVITY.y * accel, GRAVITY.z * accel),
            limit,
        );
        let input = self.swim_stick();
        let mut facing = self.facing;
        air_move::move_diving_jump(
            &mut self.v,
            &mut facing,
            input,
            &air_move::DivingJump {
                accel: 0.,
                brake: 0.,
                speed_max: speed_h,
                speed_min: 0.,
                side_accel: k::HEAD_SLIDING_SIDE_ACCEL,
                gravity: 0.,
                fall_max: 0.,
                turn_rate: k::SLERP_QUAT_GRAV,
            },
        );
        self.facing = facing;
        self.follow_dive_in_travel();
        (falling, slowed)
    }

    /// Landing on the bottom before the dive is over: the walk when the stick is held, else the
    /// standing landing. Returns whether the body is on the ground.
    fn dive_in_landed(&mut self) -> bool {
        if !self.is_on_ground_by(self.v) {
            return false;
        }
        if self.is_move() {
            self.swim_walk_velocity();
            self.set_swim_nerve(SwimNerve::Walk);
        } else {
            self.swim_wait_ground(k::GRAVITY, k::FALL_SPEED_MAX, 0., 0.);
            self.set_swim_nerve(SwimNerve::Land);
        }
        true
    }

    /// `PlayerStateSwim::exeSwimHeadIn`: the dive of a head slide that reached the water.
    pub(super) fn exe_swim_head_in(
        &mut self,
        first: bool,
        o: &Orders,
        _scene: &Course,
    ) -> Option<SwimEnd> {
        if first {
            self.start_dive_in();
        }
        let (falling, slowed) = self.dive_in_motion(HEAD_IN);
        if self.dive_in_landed() || !slowed {
            return None;
        }
        let finder = self.water_finder;
        if finder.found && finder.distance < k::SURFACE_START_DIST {
            if self.start_swim_jump_judged(o) {
                return Some(SwimEnd::Jump);
            }
            // Rising to the surface of shallow enough water: the face comes out.
            if falling < 0. && finder.distance <= k::HEAD_IN_SURFACE_HEIGHT {
                self.events.push(Fx::SwimHeadOut);
                self.set_swim_nerve(SwimNerve::Surface);
            }
            return None;
        }
        if o.jump {
            self.set_swim_nerve(SwimNerve::Paddle);
        } else if !self.is_move() || self.collider.ceiling.is_some() {
            self.set_swim_nerve(SwimNerve::Move);
        }
        None
    }

    /// `PlayerStateSwim::exeSwimDiveIn`: the dive of a high fall (the `DiveInWater` clip).
    pub(super) fn exe_swim_dive_in(
        &mut self,
        first: bool,
        o: &Orders,
        _scene: &Course,
    ) -> Option<SwimEnd> {
        if first {
            self.start_dive_in();
        }
        let (falling, slowed) = self.dive_in_motion(DIVE_IN);
        if self.dive_in_landed() {
            return None;
        }
        // The steps the body has risen count toward keeping the stroke.
        if falling < 0. {
            self.swim.dive_in_keep = lib::converge(self.swim.dive_in_keep, k::DIVE_IN_KEEP_FRAME, 1);
        }
        if !slowed {
            return None;
        }
        let kept = self.swim.dive_in_keep >= k::DIVE_IN_KEEP_FRAME;
        let finder = self.water_finder;
        let moving = self.is_move();
        if finder.found && finder.distance < k::SURFACE_START_DIST {
            if self.start_swim_jump_judged(o) {
                return Some(SwimEnd::Jump);
            }
            if o.crouch_pressed {
                self.swim.surface_hold = 0;
                self.set_swim_nerve(SwimNerve::HipDropStart);
                return None;
            }
            let near = falling < 0. && finder.distance <= k::DIVE_IN_SURFACE_HEIGHT;
            if !moving || near || kept {
                self.set_swim_nerve(SwimNerve::Surface);
            }
            return None;
        }
        if o.jump {
            self.set_swim_nerve(SwimNerve::Paddle);
        } else if o.crouch_pressed {
            self.swim.surface_hold = 0;
            self.set_swim_nerve(SwimNerve::HipDropStart);
        } else if !moving || kept {
            self.set_swim_nerve(SwimNerve::Move);
        }
        None
    }
}
