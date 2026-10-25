//! The numbers of the wall slide state (`PlayerStateNormalWallSlide`): `appear`, `followNormal`,
//! the press of `exeKeep` and the slide step, in the executable's single-precision order with
//! its library (`console_libm.rs`, `pose_math.rs`, `vector_lib.rs`).
//!
//! The body grabs the wall with a velocity that points into it with the exact length the
//! addition limit gives (not `-normal * 15`), turns its pose toward the wall by a quarter of the
//! way each step through the quaternions (so the direction the slide pushes is the pose's front
//! turned round, which is not a unit vector to the last bit), and slides along the wall with the
//! stick's share. The last bits of these numbers are the velocity the whole slide and the landing
//! after it carry.
//!
//! Checked bit for bit against the executable's routines run in a local interpreter
//! (`PJV_WSLIDE_CASES`, see `tests/wall_slide.rs`).
use super::console_libm::atan2f;
use super::joint_math::converge;
use super::pose_math::{
    ActorPose, NEGATED_GRAVITY, along_vector_normal_h, cross, normalize, quat_front_up, slerp,
    squared_length, try_normalize_or_zero,
};
use super::vector_lib::{
    add_vector_limit, separate_parallel_vertical, turn_vec_to_vec_cos_on_plane, verticalize_vec,
};
use bevy::prelude::*;

/// The pull into the wall and the speed limit of the slide (the literals of the state).
const PRESS: f32 = 15.;
const SPEED_LIMIT: f32 = 15.;
/// The sideways speed aims at this much of the stick along the wall, and moves toward it by
/// 0.1 a step.
const SIDE_SPEED_PER_STICK: f32 = 3.;
const SIDE_SPEED_STEP: f32 = 0.1;
/// The pose turns this share of the way to face the wall each step.
const FACE_WALL_RATE: f32 = 0.25;
/// `WallFollowAngleH` and `WallFollowAngleV`: how far (degrees) the wall's normal may turn from
/// one step to the next around the vertical and across.
pub const FOLLOW_ANGLE_H: f32 = 20.;
pub const FOLLOW_ANGLE_V: f32 = 30.;
/// `0.01`: a vector this near parallel to another (every component of their cross product) has
/// no angle on the plane.
const PARALLEL_LIMIT: f32 = 0.01;
/// `sead::Mathf::rad2deg`'s factor.
const RADIANS_TO_DEGREES: f32 = 57.295_776;
/// The gravity, straight down.
const GRAVITY: Vec3 = Vec3::new(0., -1., 0.);

/// The wall the slide holds and the sideways speed it has.
#[derive(Clone, Copy, Debug, Default)]
pub struct WallSlide {
    /// The wall's normal as the slide last followed it (`+0x40`).
    pub normal_start: Vec3,
    /// The direction the slide pushes by, turned with the pose (`+0x4c`).
    pub normal_end: Vec3,
    /// The speed along the wall (`+0x58`).
    pub side_speed: f32,
}

/// What `appear` leaves of the actor.
#[derive(Clone, Copy, Debug)]
pub struct Grab {
    pub slide: WallSlide,
    /// The quaternion the actor's pose keeper is given.
    pub pose: Quat,
    pub velocity: Vec3,
    /// The actor's translation (cm).
    pub trans: Vec3,
}

/// What `followNormal` leaves of the actor.
#[derive(Clone, Copy, Debug)]
pub struct Follow {
    pub slide: WallSlide,
    pub pose: Quat,
    pub velocity: Vec3,
    /// The state goes on (the wall was followed and the body is off the ground).
    pub holds: bool,
}

fn negated(v: Vec3) -> Vec3 {
    Vec3::new(-v.x, -v.y, -v.z)
}

/// The signed angle (degrees) from `a` to `b` seen along `n`: both without their part along
/// `n`; `al::calcAngleOnPlaneDegree`.
pub fn angle_on_plane_degree(a: Vec3, b: Vec3, n: Vec3) -> f32 {
    let flat = |v: Vec3| {
        let along = (n.x * v.x + n.y * v.y) + n.z * v.z;
        Vec3::new(v.x - n.x * along, v.y - n.y * along, v.z - n.z * along)
    };
    let (a, b) = (flat(a), flat(b));
    let cosine = (a.x * b.x + a.y * b.y) + a.z * b.z;
    let across = Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    );
    let angle = atan2f(squared_length(across).sqrt(), cosine);
    let side = (n.x * across.x + n.y * across.y) + n.z * across.z;
    (if side < 0. { -angle } else { angle }) * RADIANS_TO_DEGREES
}

/// `al::tryCalcAngleOnPlaneDegree`: the same, but not for a vector without a direction or one
/// along `n`.
pub fn try_angle_on_plane_degree(a: Vec3, b: Vec3, n: Vec3) -> Option<f32> {
    if squared_length(a) < super::pose_math::NO_DIRECTION
        || squared_length(b) < super::pose_math::NO_DIRECTION
    {
        return None;
    }
    let along_n = |v: Vec3| {
        let c = cross(v, n);
        c.x.abs() > PARALLEL_LIMIT || c.y.abs() > PARALLEL_LIMIT || c.z.abs() > PARALLEL_LIMIT
    };
    if !(along_n(a) && along_n(b)) {
        return None;
    }
    Some(angle_on_plane_degree(a, b, n))
}

/// `PlayerStateNormalWallSlide::appear` with the wall the collider reports: its normal, the
/// touched point, whether it was a face, the actor's translation and the collision radius.
pub fn appear(
    wall_normal: Vec3,
    wall_point: Vec3,
    at_face: bool,
    trans: Vec3,
    collision_radius: f32,
) -> Grab {
    let n = wall_normal;
    let pose = quat_front_up(negated(n), NEGATED_GRAVITY);
    // The velocity: nothing, then the pull into the wall under the speed limit.
    let mut velocity = Vec3::ZERO;
    add_vector_limit(
        &mut velocity,
        Vec3::new(n.x * -PRESS, n.y * -PRESS, n.z * -PRESS),
        SPEED_LIMIT,
    );
    let mut trans = trans;
    if !at_face {
        // A corner or an edge: slide along the wall until the contact is straight ahead.
        let offset = Vec3::new(
            trans.x - wall_point.x,
            trans.y - wall_point.y,
            trans.z - wall_point.z,
        );
        let across = verticalize_vec(n, offset);
        let side = normalize(cross(n, GRAVITY));
        let up_wall = normalize(cross(n, side));
        let along_side = negated(verticalize_vec(up_wall, across));
        trans = Vec3::new(
            trans.x + along_side.x,
            trans.y + along_side.y,
            trans.z + along_side.z,
        );
    }
    let into = ((trans.x - wall_point.x) * n.x + (trans.y - wall_point.y) * n.y)
        + (trans.z - wall_point.z) * n.z;
    let pull_back = (into - collision_radius).max(0.);
    trans = Vec3::new(
        trans.x - n.x * pull_back,
        trans.y - n.y * pull_back,
        trans.z - n.z * pull_back,
    );
    Grab {
        slide: WallSlide {
            normal_start: n,
            normal_end: n,
            side_speed: 0.,
        },
        pose,
        velocity,
        trans,
    }
}

/// The speed into the wall comes off the velocity when the state ends: the velocity without its
/// part along the slide's direction.
pub fn take_off_wall_speed(normal_end: Vec3, velocity: Vec3) -> Vec3 {
    verticalize_vec(normal_end, velocity)
}

impl WallSlide {
    /// `followNormal` for a wall contact with the normal `wall_normal` (`None` for none, or
    /// one whose action code forbids holding it): the pose turns a quarter of the way to face
    /// it, the direction the slide pushes by is the pose's front turned round, and the velocity
    /// follows it. `stored` is the quaternion the actor's pose keeper holds, `limits` the
    /// wall-follow angles (horizontal, vertical).
    pub fn follow_normal(
        &self,
        wall_normal: Option<Vec3>,
        stored: Quat,
        velocity: Vec3,
        grounded: bool,
        limits: (f32, f32),
    ) -> Follow {
        let lost = Follow {
            slide: *self,
            pose: stored,
            // The speed into the wall comes off.
            velocity: take_off_wall_speed(self.normal_end, velocity),
            holds: false,
        };
        let Some(n) = wall_normal else {
            return lost;
        };
        let tangent = normalize(cross(n, GRAVITY));
        let angle_h = angle_on_plane_degree(self.normal_start, n, GRAVITY);
        let angle_h = if angle_h > 0. { angle_h } else { -angle_h };
        let angle_v = try_angle_on_plane_degree(self.normal_start, n, tangent).unwrap_or(0.);
        let angle_v = if angle_v > 0. { angle_v } else { -angle_v };
        if angle_h > limits.0 || angle_v > limits.1 {
            return lost;
        }
        let front = negated(n);
        let target = quat_front_up(front, NEGATED_GRAVITY);
        let current = ActorPose::new(stored).calc_quat();
        let pose = slerp(current, target, FACE_WALL_RATE);
        let axis = normalize(cross(n, tangent));
        let now = ActorPose::new(pose).front();
        let turned = turn_vec_to_vec_cos_on_plane(front, now, axis, -1.)
            .map_or(Vec3::ZERO, |(vector, _)| vector);
        let normal_end = negated(turned);
        let mut velocity = along_vector_normal_h(velocity, self.normal_end, normal_end);
        if grounded {
            // On the ground the state is over: the speed into the wall comes off.
            velocity = take_off_wall_speed(normal_end, velocity);
        }
        Follow {
            slide: WallSlide {
                normal_start: n,
                normal_end,
                side_speed: self.side_speed,
            },
            pose,
            velocity,
            holds: !grounded,
        }
    }

    /// The press of `exeKeep`'s first steps: the pull into the wall under the speed limit.
    pub fn press(&self, velocity: Vec3) -> Vec3 {
        let mut velocity = velocity;
        let n = self.normal_end;
        add_vector_limit(
            &mut velocity,
            Vec3::new(n.x * -PRESS, n.y * -PRESS, n.z * -PRESS),
            SPEED_LIMIT,
        );
        velocity
    }

    /// The slide step (`0x476580`): gravity along the wall, the pull into it and the speed along
    /// it the stick (as `PlayerInput::calcMoveInput` gives it on the level) asks for.
    /// `slide_gravity` is `PlayerConst::getGravityWallSlide`.
    pub fn slide(&mut self, velocity: Vec3, stick: Vec3, slide_gravity: f32) -> Vec3 {
        let n = self.normal_end;
        let down = {
            let (unit, ok) = try_normalize_or_zero(verticalize_vec(n, GRAVITY));
            if ok { unit } else { GRAVITY }
        };
        // The velocity across the wall's normal and along it, the first again across `down`.
        let (mut into, across) = separate_parallel_vertical(n, velocity);
        let (mut along, _) = separate_parallel_vertical(down, across);
        let tangent = try_normalize_or_zero(cross(n, down)).0;
        let target = ((stick.x * tangent.x + stick.y * tangent.y) + stick.z * tangent.z)
            * SIDE_SPEED_PER_STICK;
        self.side_speed = converge(self.side_speed, target, SIDE_SPEED_STEP);
        add_vector_limit(
            &mut into,
            Vec3::new(n.x * -PRESS, n.y * -PRESS, n.z * -PRESS),
            SPEED_LIMIT,
        );
        add_vector_limit(
            &mut along,
            Vec3::new(
                down.x * slide_gravity,
                down.y * slide_gravity,
                down.z * slide_gravity,
            ),
            SPEED_LIMIT,
        );
        let sideways = Vec3::new(
            self.side_speed * tangent.x,
            self.side_speed * tangent.y,
            self.side_speed * tangent.z,
        );
        Vec3::new(
            (into.x + along.x) + sideways.x,
            (into.y + along.y) + sideways.y,
            (into.z + along.z) + sideways.z,
        )
    }
}

#[path = "../tests/wall_slide.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
