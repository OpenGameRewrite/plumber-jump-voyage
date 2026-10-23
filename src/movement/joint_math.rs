//! The executable's small matrix and scalar routines that the joint controllers are made of, in
//! single precision and in the order it runs them (`rotateMtx*DirDegree` fuse their multiplies
//! and additions as it does).
//!
//! Checked bit for bit against the executable (`PJV_GROUND_CASES`, `tests/ground_pose.rs`).
use super::look_joint::Mat34;
use super::pose_math::{DEGREES_TO_RADIANS, cosf, get_axis_angle_from_two_vec, quat_to_axes, sinf};
use bevy::prelude::{Quat, Vec3};

/// A 3x3 rotation, row-major.
pub type Rows = [[f32; 3]; 3];

/// `al::calcRate01(value, from, to)`: where `value` lies between `from` and `to`, 0 to 1 (1 when
/// the two are less than 0.001 apart).
pub fn calc_rate01(value: f32, from: f32, to: f32) -> f32 {
    let span = to - from;
    if span.abs() < 0.001 {
        return 1.;
    }
    let rate = (value - from) / span;
    if rate < 0. {
        0.
    } else if rate > 1. {
        1.
    } else {
        rate
    }
}

/// `al::easeOut`.
pub fn ease_out(t: f32) -> f32 {
    ((-0.5 * t) * t + 1.5) * t
}

/// `al::converge(value, target, step)`: move toward the target by at most `step`.
pub fn converge(value: f32, target: f32, step: f32) -> f32 {
    if value < target {
        let moved = value + step;
        if moved > target { target } else { moved }
    } else {
        let moved = value - step;
        if moved < target { target } else { moved }
    }
}

/// `al::lerpValue(a, b, t)` with `t` limited to 0..1.
pub fn lerp_value(a: f32, b: f32, t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * b + (1. - t) * a
}

/// `al::lerpVec(out, a, b, t)`, unlimited.
pub fn lerp_vec(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    Vec3::new(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.z + (b.z - a.z) * t,
    )
}

/// The rotation matrix of a quaternion (rows), as the executable's pose routines make it.
pub fn rotation_rows(q: Quat) -> Rows {
    let [x, y, z] = quat_to_axes(q);
    [[x.x, y.x, z.x], [x.y, y.y, z.y], [x.z, y.z, z.z]]
}

/// The inverse of a rotation.
pub fn transposed(m: &Rows) -> Rows {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}

/// `a` times `b`, each element `((a0*b0 + a1*b1) + a2*b2)`.
pub fn mul_rows(a: &Rows, b: &Rows) -> Rows {
    let mut out = [[0.; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            out[r][c] = (a[r][0] * b[0][c] + a[r][1] * b[1][c]) + a[r][2] * b[2][c];
        }
    }
    out
}

/// A rotation (rows) applied to a vector: row by row, `((x*a + y*b) + z*c)`.
pub fn rotate_vec(m: &Rows, v: Vec3) -> Vec3 {
    Vec3::new(
        (v.x * m[0][0] + v.y * m[0][1]) + v.z * m[0][2],
        (v.x * m[1][0] + v.y * m[1][1]) + v.z * m[1][2],
        (v.x * m[2][0] + v.y * m[2][1]) + v.z * m[2][2],
    )
}

/// `al::makeQuatRotationLimit(from, to, max_radians)`: the turn that carries `from` toward `to`
/// by at most `max_radians` (none, the identity, where they are the same direction).
pub fn make_quat_rotation_limit(from: Vec3, to: Vec3, max_radians: f32) -> Quat {
    let Some((axis, angle)) = get_axis_angle_from_two_vec(from, to) else {
        return Quat::IDENTITY;
    };
    let share = (max_radians / angle).clamp(0., 1.);
    let half = (share * angle) * 0.5;
    let (cos, sin) = (cosf(half), sinf(half));
    Quat::from_xyzw(sin * axis.x, sin * axis.y, sin * axis.z, cos)
}

/// `sead::Matrix34::multiply(out, rotation, m)`: the rotation turns the whole matrix, the
/// translation column included.
pub fn rotate_matrix(r: &Rows, m: &Mat34) -> Mat34 {
    let mut out = [[0.; 4]; 3];
    for row in 0..3 {
        for col in 0..4 {
            out[row][col] = (r[row][0] * m[0][col] + r[row][1] * m[1][col]) + r[row][2] * m[2][col];
        }
    }
    out
}

/// A rotation applied in front of the matrix's own (about the axes of the space it lies in),
/// leaving its translation alone: what the tilt routine does to a joint.
pub fn rotate_axes_only(r: &Rows, m: &mut Mat34) {
    let old = *m;
    for row in 0..3 {
        for col in 0..3 {
            m[row][col] =
                (r[row][0] * old[0][col] + r[row][1] * old[1][col]) + r[row][2] * old[2][col];
        }
    }
}

/// `al::rotateMtx{X,Y,Z}DirDegree(m, m, degrees)`: the matrix turned about its own axis (the
/// rotation is made of the sines and cosines of the three angles, and put on the right of the
/// matrix with a fused multiply-add per term).
pub fn rotate_own_axis(m: &mut Mat34, axis: [f32; 3], degrees: f32) {
    let radians = degrees * DEGREES_TO_RADIANS;
    let angle = axis.map(|a| radians * a);
    let (sx, sy, sz) = (sinf(angle[0]), sinf(angle[1]), sinf(angle[2]));
    let (cx, cy, cz) = (cosf(angle[0]), cosf(angle[1]), cosf(angle[2]));
    let sx_sy = sx * sy;
    let turn: Rows = [
        [cy * cz, sx_sy * cz - sz * cx, sx * sz + sy * (cx * cz)],
        [sz * cy, sx_sy * sz + cx * cz, sy * (sz * cx) - sx * cz],
        [-sy, sx * cy, cx * cy],
    ];
    let old = *m;
    for row in 0..3 {
        for col in 0..3 {
            let first = turn[0][col] * old[row][0];
            let second = turn[1][col].mul_add(old[row][1], first);
            let third = turn[2][col].mul_add(old[row][2], second);
            m[row][col] = 0. + third;
        }
    }
}

pub fn rotate_own_x(m: &mut Mat34, degrees: f32) {
    rotate_own_axis(m, [1., 0., 0.], degrees);
}

pub fn rotate_own_y(m: &mut Mat34, degrees: f32) {
    rotate_own_axis(m, [0., 1., 0.], degrees);
}

pub fn rotate_own_z(m: &mut Mat34, degrees: f32) {
    rotate_own_axis(m, [0., 0., 1.], degrees);
}
