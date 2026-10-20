//! The vector and orientation maths the original's ground states run on:
//! motion in the plane of the floor under the feet, and the actor's pose
//! turning to it. Written from the library functions `PlayerActionGroundMoveControl`
//! and `rs::slerpUpFront` call (`al::alongVectorNormalH`, `al::verticalizeVec`,
//! `al::turnVecToVecDegree`, `al::makeQuatUpFront`, `sead::Matrix34::toQuat`,
//! `sead::Quat::slerpTo`), as read from the 1.0.0 executable.
//!
//! Directions are in the game's frame (+Y up, right-handed); the pose is a
//! quaternion whose rotation matrix has the side axis in the first column, the
//! up axis in the second and the front axis in the third, as `al::calcUpDir`
//! and `al::calcFrontDir` read them.
use bevy::prelude::*;

/// `al::isNearZero(vec, 0.001)` on a vector: its length is within the epsilon.
pub const NEAR_ZERO: f32 = 0.001;

/// `al::verticalizeVec(out, normal, vec)`: `vec` without its part along `normal`.
pub fn verticalize(normal: Vec3, vec: Vec3) -> Vec3 {
    vec - normal * normal.dot(vec)
}

/// `al::parallelizeVec(out, dir, vec)`: only the part of `vec` along `dir`.
pub fn parallelize(dir: Vec3, vec: Vec3) -> Vec3 {
    dir * dir.dot(vec)
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

/// The axis and the angle (radians) that turn `from` onto `to`
/// (`al::getAxisAngleFromTwoVec`); `None` for two vectors with no usable turn
/// between them (equal ones).
pub fn axis_angle(from: Vec3, to: Vec3) -> Option<(Vec3, f32)> {
    let dot = from.dot(to);
    let cross = from.cross(to);
    if dot < 0. && cross.abs().max_element() <= 0.01 {
        // Exactly opposed: any axis across them turns one onto the other.
        return Some((from.any_orthonormal_vector(), std::f32::consts::PI));
    }
    if cross.length_squared() < 1.000_000_1e-6 {
        return None;
    }
    let angle = dot.clamp(-1., 1.).acos();
    if angle.abs() < NEAR_ZERO {
        return None;
    }
    Some((cross.normalize(), angle))
}

/// `al::alongVectorNormalH(out, vec, from, to)`: `vec`, without its part along
/// the normal `from`, turned about the axis `from x to` by the angle between the
/// two normals, so a vector in the plane of `from` ends in the plane of `to`.
/// With the same normal twice it is just the part of `vec` across it.
pub fn along_vector_normal_h(vec: Vec3, from: Vec3, to: Vec3) -> Vec3 {
    let cross = from.cross(to);
    if cross.length_squared() < 1.000_000_1e-6 {
        return vec - to * to.dot(vec);
    }
    let across = vec - from * from.dot(vec);
    let angle = cross.length().atan2(from.dot(to));
    Quat::from_axis_angle(cross.normalize(), angle) * across
}

/// `al::turnVecToVecDegree`: `from` turned toward `to` by at most `degrees`,
/// as a unit vector.
pub fn turn_vec_to_vec_degree(from: Vec3, to: Vec3, degrees: f32) -> Vec3 {
    let Some((axis, angle)) = axis_angle(from, to) else {
        return normalized_or_zero(from);
    };
    let limit = degrees * 0.017_453_292;
    let share = (limit / angle).clamp(0., 1.);
    normalized_or_zero(Quat::from_axis_angle(axis, angle * share) * from)
}

/// `al::makeQuatUpFront`: the pose with the given up axis and, across it, the
/// given front.
pub fn quat_up_front(up: Vec3, front: Vec3) -> Quat {
    let y = up.normalize_or_zero();
    let x = normalized_or_zero(y.cross(front));
    let z = x.cross(y);
    quat_from_axes(x, y, z)
}

/// `al::makeQuatFrontUp`: the pose with the given front and, across it, the
/// given up.
pub fn quat_front_up(front: Vec3, up: Vec3) -> Quat {
    let z = front.normalize_or_zero();
    let x = normalized_or_zero(up.cross(z));
    let y = z.cross(x);
    quat_from_axes(x, y, z)
}

/// `sead::Matrix34CalcCommon::toQuat` for the rotation whose columns are
/// `x`, `y` and `z`: the quaternion component that is the largest of the four
/// squares is taken from the matrix diagonal and the others from its
/// differences, which keeps the result's precision and decides its sign.
pub fn quat_from_axes(x: Vec3, y: Vec3, z: Vec3) -> Quat {
    // Row-major matrix entries m[row][column].
    let (m00, m01, m02) = (x.x, y.x, z.x);
    let (m10, m11, m12) = (x.y, y.y, z.y);
    let (m20, m21, m22) = (x.z, y.z, z.z);
    let squares_w = (m00 + m11 + m22 + 1.) * 0.25;
    let squares_z = squares_w - (m00 + m11) * 0.5;
    let squares_y = squares_w - (m00 + m22) * 0.5;
    let squares_x = squares_w - (m11 + m22) * 0.5;
    // Which component is the largest, with the comparison order of the
    // executable (ties go as its selects do).
    let y_or_z = if squares_y > squares_z { 2 } else { 3 };
    let x_or_z = if squares_x > squares_z { 1 } else { 3 };
    let by_x = if squares_x <= squares_y { y_or_z } else { x_or_z };
    let w_or_z = if squares_w > squares_z { 0 } else { 3 };
    let mut largest = if squares_w <= squares_y { y_or_z } else { w_or_z };
    if squares_w <= squares_x {
        largest = by_x;
    }
    match largest & 3 {
        1 => {
            let qx = squares_x.sqrt();
            let k = 0.25 / qx;
            Quat::from_xyzw(qx, (m01 + m10) * k, (m02 + m20) * k, (m21 - m12) * k)
        }
        2 => {
            let qy = squares_y.sqrt();
            let k = 0.25 / qy;
            Quat::from_xyzw((m10 + m01) * k, qy, (m12 + m21) * k, (m02 - m20) * k)
        }
        3 => {
            let qz = squares_z.sqrt();
            let k = 0.25 / qz;
            Quat::from_xyzw((m20 + m02) * k, (m21 + m12) * k, qz, (m10 - m01) * k)
        }
        _ => {
            let qw = squares_w.sqrt();
            let k = 0.25 / qw;
            Quat::from_xyzw((m21 - m12) * k, (m02 - m20) * k, (m10 - m01) * k, qw)
        }
    }
}

/// `al::calcQuatUp`: the second column of the quaternion's rotation.
pub fn quat_up(q: Quat) -> Vec3 {
    Vec3::new(
        2. * q.x * q.y - 2. * q.z * q.w,
        1. - 2. * q.x * q.x - 2. * q.z * q.z,
        2. * q.y * q.z + 2. * q.x * q.w,
    )
}

/// `al::calcQuatFront`: the third column of the quaternion's rotation.
pub fn quat_front(q: Quat) -> Vec3 {
    Vec3::new(
        2. * q.x * q.z + 2. * q.y * q.w,
        2. * q.y * q.z - 2. * q.x * q.w,
        1. - 2. * q.x * q.x - 2. * q.y * q.y,
    )
}

/// `sead::Quat::slerpTo`: the spherical blend of two quaternions, not
/// normalised, taking the shorter way round.
pub fn slerp(from: Quat, to: Quat, t: f32) -> Quat {
    let dot = from.dot(to).min(1.);
    let clamped = dot.max(-1.);
    let theta = clamped.abs().acos();
    let sine = theta.sin();
    let (weight_from, weight_to) = if sine.abs() < 1.192_092_9e-7 {
        (1. - t, t)
    } else {
        let k = 1. / sine;
        (k * (theta - theta * t).sin(), k * (theta * t).sin())
    };
    let weight_to = if clamped < 0. { -weight_to } else { weight_to };
    Quat::from_xyzw(
        weight_from * from.x + weight_to * to.x,
        weight_from * from.y + weight_to * to.y,
        weight_from * from.z + weight_to * to.z,
        weight_from * from.w + weight_to * to.w,
    )
}

/// `rs::slerpUpFront`: the pose `current` turned toward the given up and front.
/// The up it aims for is limited to `max_degrees` from straight up, the pose's
/// up follows it by the slerp `rate`, and the front is set to the given one
/// (carried over from the up the front was meant for to the limited up).
pub fn slerp_up_front(
    current: Quat,
    up: Vec3,
    front: Vec3,
    rate: f32,
    max_degrees: f32,
) -> Quat {
    let world_up = Vec3::Y;
    let target_up = turn_vec_to_vec_degree(world_up, up, max_degrees);
    let meant_up = quat_up(quat_front_up(front, up));
    let across = normalized_or_zero(along_vector_normal_h(front, meant_up, target_up));
    if across != Vec3::ZERO {
        let target = quat_up_front(target_up, across);
        let blended = slerp(current, target, rate);
        return quat_up_front(quat_up(blended), across);
    }
    // No usable front: keep the pose's own (or its up when that is parallel).
    let mut kept = quat_front(current);
    let parallel = target_up.cross(kept).abs().max_element() <= 0.01;
    if parallel {
        kept = quat_up(current);
    }
    let target = quat_up_front(target_up, kept);
    slerp(current, target, rate)
}

#[cfg(test)]
mod tests {
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
        let z = quat_front(q);
        assert!(z.dot(up).abs() < 1.0e-5);
        assert!((q.length() - 1.).abs() < 1.0e-5);
    }
}
