//! The ground motion of the spin-cap state while it brakes (`PlayerStateSpinCap::updateSpinGroundNerve`
//! on its first `SpinBrakeFrame` steps): `rs::moveInertiaTurn`.
//!
//! The velocity is split along the actor's front, its side and the floor's normal
//! (`PlayerActionVelocityControl`) and only the front part is braked, so a velocity that does not
//! point along the front keeps its sideways part: a throw begun on the run, with the facing turned
//! to the stick, carries on across its new front while the part against it dies away. A velocity
//! along the front (a throw from standing, or along the run) loses `SpinBrakeRate` of itself whole.
//! The stick then turns the direction of that velocity by at most `SpinRoundLimitDegree` a step
//! unless it points against it (`PlayerActionFunction::isOppositeDir`, a hundred degrees or more
//! away); the actor's front turns by the same rotation.
//!
//! Not ported: the nerve's other branch, the ground move control in these steps while the
//! force-run counter is up or the player runs on the water surface (`PlayerCounterForceRun`,
//! `PlayerJudgeWaterSurfaceRun`), and the ice floor's own handling (`isOnGroundSkateCode`).
use super::pose_math::{
    DEGREES_TO_RADIANS, along_vector_normal_h, cross, dot, make_quat_rotation_limit, normalize,
    rotate_by_quat, try_normalize_or_zero,
};
use super::vector_lib::{parallelize_vec, separate_parallel_vertical, verticalize_vec};
use super::*;

/// `PlayerActionFunction::isOppositeDir`: the two unit directions are further apart than the
/// sine of five degrees short of a right angle.
fn is_opposite_dir(a: Vec3, b: Vec3) -> bool {
    dot(b, a) <= -0.087_156
}

impl Player {
    /// One step of `rs::moveInertiaTurn` on the floor: `GRAVITY_MOVE` snaps the velocity to it.
    pub(super) fn spin_cap_inertia(&mut self) {
        let normal = self
            .mesh_ground
            .filter(|_| self.grounded)
            .unwrap_or(Vec3::Y);
        let (actor_up, actor_front) = self.pose_axes();
        // `PlayerActionVelocityControl`: the front is the actor's, carried along the floor.
        let front = normalize(along_vector_normal_h(actor_front, actor_up, normal));
        let side = normalize(cross(normal, front));
        let mut velocity_front = parallelize_vec(front, self.v);
        let mut velocity_side = parallelize_vec(side, self.v);
        let mut velocity_up = self.v - velocity_front - velocity_side;
        velocity_front = verticalize_vec(normal, velocity_front) * t::SPIN_BRAKE_RATE;
        velocity_side = verticalize_vec(normal, velocity_side);
        velocity_up = verticalize_vec(normal, velocity_up);
        // `calcSnap(-normal, GravityMove)`, component by component as the original adds it.
        let snap = -normal;
        velocity_up = Vec3::new(
            snap.x * GRAVITY_MOVE + velocity_up.x,
            snap.y * GRAVITY_MOVE + velocity_up.y,
            snap.z * GRAVITY_MOVE + velocity_up.z,
        );
        let velocity = velocity_front + velocity_side + velocity_up;
        // The velocity's direction along the floor turns toward the stick.
        let (across, along) = {
            let (parallel, vertical) = separate_parallel_vertical(normal, velocity);
            (vertical, parallel)
        };
        let speed = super::vector_lib::length(across);
        let (direction, moving) = try_normalize_or_zero(across);
        let mut turned = direction;
        let mut turn = Quat::IDENTITY;
        let (stick, pushed) = try_normalize_or_zero(verticalize_vec(normal, self.ground_stick()));
        if moving && pushed && !is_opposite_dir(stick, direction) {
            turn = make_quat_rotation_limit(
                direction,
                stick,
                t::SPIN_ROUND_LIMIT_DEGREE * DEGREES_TO_RADIANS,
            );
            turned = rotate_by_quat(direction, turn);
        }
        let result = if moving {
            Vec3::new(
                turned.x * speed + along.x,
                turned.y * speed + along.y,
                turned.z * speed + along.z,
            )
        } else {
            velocity
        };
        self.v = result;
        if turn != Quat::IDENTITY {
            // The state turns the actor's front by the same rotation (`rs::slerpUpFront`).
            let front = rotate_by_quat(actor_front, turn);
            let stored = self.actor_pose().slerp_up_front(
                normal,
                front,
                SLERP_QUAT_RATE,
                t::HILL_POSE_DEGREE_MAX,
            );
            self.set_pose(stored);
        }
    }
}
