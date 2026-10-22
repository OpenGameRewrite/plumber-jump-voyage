//! The thrown cap's pose, as the original's `HackCap` keeps and turns it.
//!
//! The cap is an actor with a pose quaternion like any other. Three of its
//! nerves read or write it and the flight depends on that: `startThrow` and
//! the throw nerves set it to face back toward the player, `exeTrample` turns
//! the actor to the front of its model's `AllRoot` joint (`al::faceToDirection`),
//! and `exeReturn` leans it toward the player's head and then turns its front
//! toward the head by a limited angle each frame (`al::turnFrontSpherical`) before
//! flying along that front. All of it is done in the single-precision routines of
//! `pose_math.rs` and `vector_lib.rs`, in the executable's order.
use super::pose_math::{
    ActorPose, NEGATED_GRAVITY, PARALLEL_LIMIT, cross, is_parallel_direction, normalize,
    quat_front_up, quat_up_front, slerp, squared_length, try_normalize_or_zero,
};
use super::vector_lib::{cos_of_degrees, is_near_zero, turn_vec_to_vec_cos_on_plane};
use bevy::prelude::{Quat, Vec3};

/// The share of the way the return's pose blend goes toward the head each frame
/// (`exeReturn` hands `al::slerpQuat` 0.25).
const LEAN_RATE: f32 = 0.25;
/// `al::isNearZero(axis, 0.001)` in `al::turnFrontSpherical`: a turn axis this short has no
/// direction.
const AXIS_TOLERANCE: f32 = 0.001;
/// The world's gravity, (0, -1, 0).
const GRAVITY: Vec3 = Vec3::new(0., -1., 0.);

/// The cap actor's pose: the quaternion it was last given and the matrix made from it.
#[derive(Clone, Copy, Debug)]
pub struct CapPose {
    stored: Quat,
    matrix: ActorPose,
}

impl CapPose {
    /// `al::updatePoseQuat`.
    fn given(stored: Quat) -> Self {
        Self {
            stored,
            matrix: ActorPose::new(stored),
        }
    }

    /// The pose `HackCap::startThrow` and `exeThrowStart` give a cap thrown along `front`: the cap
    /// faces back toward the thrower, upright.
    pub fn thrown_along(front: Vec3) -> Self {
        Self::given(quat_front_up(-front, NEGATED_GRAVITY))
    }

    /// The quaternion the pose keeper holds (`al::getQuat`).
    pub fn quat(&self) -> Quat {
        self.stored
    }

    /// `al::faceToDirection`: upright, facing `direction` (nothing happens for a direction along
    /// the gravity).
    pub fn face_to_direction(&mut self, direction: Vec3) {
        if !is_parallel_direction(direction, GRAVITY, PARALLEL_LIMIT) {
            *self = Self::given(quat_up_front(NEGATED_GRAVITY, direction));
        }
    }

    /// A model joint's local front axis (its matrix's third column, scale included) carried into
    /// the world by the actor's pose.
    pub fn world_axis(&self, local: Vec3) -> Vec3 {
        let (side, up, front) = (self.matrix.side(), self.matrix.up(), self.matrix.front());
        side * local.x + up * local.y + front * local.z
    }

    /// The first part of `exeReturn`'s pose work: the pose blends a quarter of the way toward
    /// facing `direction` (a unit vector) with its own up, unless that is parallel to it.
    pub fn lean_toward(&mut self, direction: Vec3) {
        let up = self.matrix.up();
        if is_parallel_direction(up, direction, PARALLEL_LIMIT) {
            return;
        }
        let target = quat_front_up(direction, up);
        *self = Self::given(slerp(self.stored, target, LEAN_RATE));
    }

    /// `al::turnFrontSpherical(actor, out, direction, cos(limit))`: the pose's front turned toward
    /// the unit `direction` by at most `limit_degrees`, about the axis across the two. `None`
    /// when the library writes nothing.
    pub fn front_turned_toward(&self, direction: Vec3, limit_degrees: f32) -> Option<Vec3> {
        let front = self.matrix.front();
        let mut axis = cross(front, direction);
        if is_near_zero(axis, AXIS_TOLERANCE) {
            axis = self.matrix.up();
        }
        let axis = normalize(axis);
        turn_vec_to_vec_cos_on_plane(
            normalize(front),
            direction,
            axis,
            cos_of_degrees(limit_degrees),
        )
        .map(|(turned, _)| turned)
    }

    /// The last part of `exeReturn`'s pose work: stand upright facing `front`, unless that is
    /// along the gravity.
    pub fn stand_facing(&mut self, front: Vec3) {
        if !is_parallel_direction(front, NEGATED_GRAVITY, PARALLEL_LIMIT) {
            *self = Self::given(quat_front_up(front, NEGATED_GRAVITY));
        }
    }
}

/// The length of a vector as the library computes it (`sead::Vector3f::length`).
pub fn length(v: Vec3) -> f32 {
    squared_length(v).sqrt()
}

/// The length of a vector's horizontal part, `sqrt(x * x + z * z)`.
pub fn horizontal_length(v: Vec3) -> f32 {
    ((v.x * v.x) + (v.z * v.z)).sqrt()
}

/// `al::tryNormalizeOrZero`: the unit vector and whether there was a direction.
pub fn direction_of(v: Vec3) -> (Vec3, bool) {
    try_normalize_or_zero(v)
}
