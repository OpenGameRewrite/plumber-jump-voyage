//! The vector and orientation maths the original's ground states run on:
//! motion in the plane of the floor under the feet, and the actor's pose
//! turning to it. The library routines they call (`al::alongVectorNormalH`,
//! `al::turnVecToVecDegree`, `al::makeQuatUpFront`, `sead::Matrix34::toQuat`,
//! `sead::Quat::slerpTo`, `rs::slerpUpFront`, `rs::waitGround`) are ported bit for bit in
//! `pose_math.rs`; this file keeps the smaller helpers and re-exports those.
//!
//! Directions are in the game's frame (+Y up, right-handed); the pose is a
//! quaternion whose rotation matrix has the side axis in the first column, the
//! up axis in the second and the front axis in the third, as `al::calcUpDir`
//! and `al::calcFrontDir` read them.
use bevy::prelude::*;

/// `al::verticalizeVec(out, normal, vec)`: `vec` without its part along `normal`.
pub fn verticalize(normal: Vec3, vec: Vec3) -> Vec3 {
    vec - normal * normal.dot(vec)
}

/// The vector of the plane with the normal `n` whose horizontal part is `h`.
pub fn lift(h: Vec3, n: Vec3) -> Vec3 {
    if n.y > 0. && n != Vec3::Y {
        Vec3::new(h.x, -(h.x * n.x + h.z * n.z) / n.y, h.z)
    } else {
        h.with_y(0.)
    }
}

/// `al::tryNormalizeOrZero`: the unit vector, or zero for one that is too short
/// to have a direction (the library's epsilon is a squared length of 1e-12
/// times the machine's: anything the vector maths cannot normalise).
pub fn normalized_or_zero(v: Vec3) -> Vec3 {
    let length = v.length();
    if length > 0. {
        v * (1. / length)
    } else {
        Vec3::ZERO
    }
}

pub use super::pose_math::{
    ActorPose, along_vector_normal_h, normalize, quat_to_axes, quat_up_front, slerp,
    try_normalize_or_zero,
};

#[cfg(test)]
mod tests {
    use super::super::pose_math::quat_up;
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1.0e-5
    }

    #[test]
    fn a_flat_floor_leaves_the_horizontal_velocity_alone() {
        let v = Vec3::new(0.2, -31., 9.);
        let along = along_vector_normal_h(v, Vec3::Y, Vec3::Y);
        assert!(close(along, Vec3::new(0.2, 0., 9.)));
    }

    #[test]
    fn a_tilted_floor_gets_the_horizontal_speed_along_it() {
        // (Recorded: the wall jump's landing on the 18 degree floor keeps its
        // horizontal speed as the speed along the slope.)
        let normal = Vec3::new(-0.187801, 0.950026, -0.249363);
        let v = Vec3::new(0.2493, -30., 8.9704);
        let along = along_vector_normal_h(v, Vec3::Y, normal);
        assert!(along.dot(normal).abs() < 1.0e-4);
        assert!((along.length() - v.with_y(0.).length()).abs() < 1.0e-4);
        assert!((along.y - 2.2837).abs() < 5.0e-3, "{along}");
    }

    #[test]
    fn the_pose_quaternion_reads_back_its_axes() {
        let up = Vec3::new(-0.2, 0.95, -0.24).normalize();
        let front = Vec3::new(0.1, 0.2, 0.97);
        let q = quat_up_front(up, front);
        assert!(close(quat_up(q), up));
        let z = ActorPose::new(q).front();
        assert!(z.dot(up).abs() < 1.0e-5);
        assert!((q.length() - 1.).abs() < 1.0e-5);
    }
}
