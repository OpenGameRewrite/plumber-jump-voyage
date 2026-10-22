//! The executable's vector, matrix and quaternion routines behind an actor's pose, with its
//! single-precision arithmetic in the order it runs it (no fused operations, no library
//! shortcuts): `al::normalize`, `al::tryNormalizeOrZero`, `al::alongVectorNormalH`,
//! `al::turnVecToVecDegree`, `al::makeQuatUpFront` / `FrontUp`, `sead::Matrix34::toQuat`,
//! `sead::Quat::slerpTo`, the matrix `ActorPoseKeeperTQGMSV::updatePoseQuat` derives from the
//! quaternion it is given, and `rs::slerpUpFront` / `rs::waitGround`'s pose part on top of them.
//!
//! The results of a pose function are used to the last bit: a quarter turn is a tie of
//! `toQuat`'s four branches, and which of them the game takes depends on the rounding noise of
//! these routines. `sinf`, `cosf`, `acosf` and `atan2f` are the platform's; they are assumed to
//! agree with the console's library (they are correctly rounded for the values seen).
//!
//! Checked bit for bit against the executable's functions run in a local interpreter
//! (`PJV_POSE_CASES`, see `tests/pose_math.rs`).
use bevy::prelude::{Quat, Vec3};

/// The degree to radian factor in single precision.
pub(super) const DEGREES_TO_RADIANS: f32 = 0.017_453_292;
/// `1.0000001e-6`: the squared length below which a vector has no direction.
pub(super) const NO_DIRECTION: f32 = 1.000_000_1e-6;
/// `al::getAxisAngleFromTwoVec`: the cross product's components within this make two opposed
/// vectors, and an angle under it two equal ones.
const OPPOSED_LIMIT: f32 = 0.01;
const SAME_ANGLE: f32 = 0.001;
/// `isParallelDirection(.., 0.01)`.
pub const PARALLEL_LIMIT: f32 = 0.01;
/// `sead::QuatCalcCommon::slerpTo`: a sine under this takes the linear weights.
const SLERP_LINEAR_SINE: f32 = 1.192_092_9e-7;
/// The negated gravity (`-al::getGravity`, gravity being (0, -1, 0)), signed zeros included.
pub(super) const NEGATED_GRAVITY: Vec3 = Vec3::new(-0., 1., -0.);

/// The single-precision library functions, correctly rounded (computed in double precision and
/// rounded once), which is what the console's `sinf`, `cosf`, `acosf` and `atan2f` return for
/// the arguments seen.
fn sinf(x: f32) -> f32 {
    f64::from(x).sin() as f32
}
pub(super) fn cosf(x: f32) -> f32 {
    f64::from(x).cos() as f32
}
fn acosf(x: f32) -> f32 {
    f64::from(x).acos() as f32
}
pub(super) fn atan2f(y: f32, x: f32) -> f32 {
    f64::from(y).atan2(f64::from(x)) as f32
}

pub(super) fn dot(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

pub(super) fn squared_length(v: Vec3) -> f32 {
    v.x * v.x + v.y * v.y + v.z * v.z
}

/// The cross product as the library computes it (`a.y*b.z - a.z*b.y`, ...).
pub(super) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

/// `1 / length` times each component, when the length is positive (`al::normalize`).
pub(super) fn scaled_by_inverse_length(v: Vec3, length: f32) -> Vec3 {
    if length > 0. {
        let inverse = 1. / length;
        Vec3::new(inverse * v.x, inverse * v.y, inverse * v.z)
    } else {
        v
    }
}

/// `al::normalize`: the vector scaled by the inverse of its length (unchanged when that is not
/// positive).
pub fn normalize(v: Vec3) -> Vec3 {
    scaled_by_inverse_length(v, squared_length(v).sqrt())
}

/// `al::tryNormalizeOrZero(Vector3f*)`: the unit vector and true, or zero and false for a vector
/// whose squared length is under 1e-6.
pub fn try_normalize_or_zero(v: Vec3) -> (Vec3, bool) {
    let squared = squared_length(v);
    if squared < NO_DIRECTION {
        return (Vec3::ZERO, false);
    }
    (scaled_by_inverse_length(v, squared.sqrt()), true)
}

/// `al::isParallelDirection(a, b, limit)`: every component of the cross product is within the
/// limit.
pub fn is_parallel_direction(a: Vec3, b: Vec3, limit: f32) -> bool {
    let c = cross(a, b);
    c.x.abs() <= limit && c.y.abs() <= limit && c.z.abs() <= limit
}

/// `al::getAxisAngleFromTwoVec`: the unit axis and the angle (radians) that turn `from` onto
/// `to`; `None` when they are the same direction. Exactly opposed vectors turn by a half turn
/// about an axis across them.
pub fn get_axis_angle_from_two_vec(from: Vec3, to: Vec3) -> Option<(Vec3, f32)> {
    let cosine = dot(from, to);
    let axis = cross(from, to);
    if cosine < 0.
        && axis.x.abs() <= OPPOSED_LIMIT
        && axis.y.abs() <= OPPOSED_LIMIT
        && axis.z.abs() <= OPPOSED_LIMIT
    {
        // (`al::calcDirVerticalAny`, whose choice of axis is not ported: any axis across `from`
        // turns it onto its opposite. Guess: only ever reached by a ceiling-facing normal.)
        return Some((from.any_orthonormal_vector(), std::f32::consts::PI));
    }
    if squared_length(axis) < NO_DIRECTION {
        return None;
    }
    let axis = scaled_by_inverse_length(axis, squared_length(axis).sqrt());
    let clamped = cosine.clamp(-1., 1.);
    let angle = acosf(clamped);
    if angle.abs() < SAME_ANGLE {
        return None;
    }
    Some((axis, angle))
}

/// The vector `v` turned by the unit quaternion `q`, as `alongVectorNormalH` and
/// `turnVecToVecDegree` do it.
fn rotate_by_quat(v: Vec3, q: Quat) -> Vec3 {
    let (qx, qy, qz, qw) = (q.x, q.y, q.z, q.w);
    let tx = qw * v.x + (qy * v.z - qz * v.y);
    let ty = qw * v.y + (qz * v.x - qx * v.z);
    let tz = qw * v.z + (qx * v.y - qy * v.x);
    let minus_dot = (-(qx * v.x) - qy * v.y) - qz * v.z;
    Vec3::new(
        (qy * tz + (qw * tx - qz * ty)) - qx * minus_dot,
        ((qz * tx + qw * ty) - qx * tz) - qy * minus_dot,
        (qw * tz + (qx * ty - qy * tx)) - qz * minus_dot,
    )
}

/// `al::alongVectorNormalH(out, vec, from, to)`: `vec` without its part along the normal `from`,
/// turned about the axis `from x to` by the angle between the two normals, so a vector in the
/// plane of `from` ends in the plane of `to`. With (nearly) the same normal it is just the part
/// of `vec` across it.
pub fn along_vector_normal_h(vec: Vec3, from: Vec3, to: Vec3) -> Vec3 {
    let axis = cross(from, to);
    let squared = squared_length(axis);
    if squared < NO_DIRECTION {
        let along = dot(to, vec);
        return Vec3::new(
            vec.x - to.x * along,
            vec.y - to.y * along,
            vec.z - to.z * along,
        );
    }
    let along = dot(from, vec);
    let across = Vec3::new(
        vec.x - from.x * along,
        vec.y - from.y * along,
        vec.z - from.z * along,
    );
    let axis = scaled_by_inverse_length(axis, squared.sqrt());
    // The angle is measured with a second cross product, ordered as the library has it.
    let turn = Vec3::new(
        from.y * to.z - to.y * from.z,
        to.x * from.z - from.x * to.z,
        from.x * to.y - to.x * from.y,
    );
    let angle = atan2f(squared_length(turn).sqrt(), dot(from, to));
    let half = angle * 0.5;
    let sine = sinf(half);
    let q = Quat::from_xyzw(axis.x * sine, axis.y * sine, axis.z * sine, cosf(half));
    rotate_by_quat(across, q)
}

/// `al::turnVecToVecDegree(out, from, to, degrees)`: `from` turned toward `to` by at most
/// `degrees`, as a unit vector (it stays `from` normalised when there is nothing to turn by).
pub fn turn_vec_to_vec_degree(from: Vec3, to: Vec3, degrees: f32) -> Vec3 {
    let q = match get_axis_angle_from_two_vec(from, to) {
        Some((axis, angle)) => {
            let limit = degrees * DEGREES_TO_RADIANS;
            let share = (limit / angle).clamp(0., 1.);
            let half = (angle * share) * 0.5;
            let sine = sinf(half);
            Quat::from_xyzw(sine * axis.x, sine * axis.y, sine * axis.z, cosf(half))
        }
        None => Quat::from_xyzw(0., 0., 0., 1.),
    };
    normalize(rotate_by_quat(from, q))
}

/// The rotation matrix as its three columns: the side, the up and the front axis.
pub type Axes = [Vec3; 3];

/// `al::makeMtxUpFront`: the up axis normalised, the side across it and the front (the cross
/// product uses the up as given), the front as side x up.
pub fn make_mtx_up_front(up: Vec3, front: Vec3) -> Axes {
    let y = normalize(up);
    let x = normalize(cross(up, front));
    let z = cross(x, y);
    [x, y, z]
}

/// `al::makeMtxFrontUp`: the front axis normalised, the side across it and the up (as given),
/// the up as front x side.
pub fn make_mtx_front_up(front: Vec3, up: Vec3) -> Axes {
    let z = normalize(front);
    let x = normalize(cross(up, z));
    let y = cross(z, x);
    [x, y, z]
}

/// `sead::Matrix34CalcCommon::toQuat`: the quaternion component that is the largest of the four
/// squares is taken from the matrix diagonal and the others from its differences, which keeps
/// the result's precision and decides its sign.
pub fn to_quat([x, y, z]: Axes) -> Quat {
    // Row-major matrix entries m[row][column].
    let (m00, m01, m02) = (x.x, y.x, z.x);
    let (m10, m11, m12) = (x.y, y.y, z.y);
    let (m20, m21, m22) = (x.z, y.z, z.z);
    let squares_w = (m00 + m11 + m22 + 1.) * 0.25;
    let squares_z = squares_w - (m00 + m11) * 0.5;
    let squares_y = squares_w - (m00 + m22) * 0.5;
    let squares_x = squares_w - (m11 + m22) * 0.5;
    // Which component is the largest, with the comparison order of the executable (ties go as
    // its selects do).
    let y_or_z = if squares_y > squares_z { 2 } else { 3 };
    let x_or_z = if squares_x > squares_z { 1 } else { 3 };
    let by_x = if squares_x <= squares_y {
        y_or_z
    } else {
        x_or_z
    };
    let w_or_z = if squares_w > squares_z { 0 } else { 3 };
    let mut largest = if squares_w <= squares_y {
        y_or_z
    } else {
        w_or_z
    };
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

/// `al::makeQuatUpFront`.
pub fn quat_up_front(up: Vec3, front: Vec3) -> Quat {
    to_quat(make_mtx_up_front(up, front))
}

/// `al::makeQuatFrontUp`.
pub fn quat_front_up(front: Vec3, up: Vec3) -> Quat {
    to_quat(make_mtx_front_up(front, up))
}

/// The rotation matrix `ActorPoseKeeperTQGMSV::updatePoseQuat` stores next to the quaternion it
/// is given (the quaternion is used as it comes, normalised or not).
pub fn quat_to_axes(q: Quat) -> Axes {
    let x2 = q.x + q.x;
    let y2 = q.y + q.y;
    let z2 = q.z + q.z;
    let w2 = q.w + q.w;
    let xx = q.x * x2;
    let yy = q.y * y2;
    let zz = q.z * z2;
    let xy = q.y * x2;
    let xz = q.z * x2;
    let yz = y2 * q.z;
    let xw = q.x * w2;
    let yw = q.y * w2;
    let zw = q.z * w2;
    [
        Vec3::new((1. - yy) - zz, xy + zw, xz - yw),
        Vec3::new(xy - zw, (1. - xx) - zz, yz + xw),
        Vec3::new(xz + yw, yz - xw, (1. - xx) - yy),
    ]
}

/// `al::calcQuatUp(Vector3f*, const Quat&)`: the second column of the quaternion's rotation,
/// straight from the quaternion (`slerpUpFront` reads the blended one this way).
pub fn quat_up(q: Quat) -> Vec3 {
    Vec3::new(
        2. * (q.x * q.y) - 2. * (q.z * q.w),
        (1. - 2. * (q.x * q.x)) - 2. * (q.z * q.z),
        2. * (q.x * q.w) + 2. * (q.y * q.z),
    )
}

/// `sead::QuatCalcCommon::slerpTo` (`al::slerpQuat`): the spherical blend of two quaternions, not
/// normalised, taking the shorter way round.
pub fn slerp(from: Quat, to: Quat, t: f32) -> Quat {
    let dot = ((from.x * to.x + from.y * to.y) + from.z * to.z) + from.w * to.w;
    let clamped = dot.clamp(-1., 1.);
    let theta = acosf(clamped.abs());
    let sine = sinf(theta);
    let (weight_from, weight_to) = if sine.abs() < SLERP_LINEAR_SINE {
        (1. - t, t)
    } else {
        let inverse = 1. / sine;
        let by_rate = theta * t;
        (inverse * sinf(theta - by_rate), inverse * sinf(by_rate))
    };
    let weight_to = if clamped < 0. { -weight_to } else { weight_to };
    Quat::from_xyzw(
        weight_from * from.x + weight_to * to.x,
        weight_from * from.y + weight_to * to.y,
        weight_from * from.z + weight_to * to.z,
        weight_from * from.w + weight_to * to.w,
    )
}

/// An actor's stored pose (`ActorPoseKeeperTQGMSV`): the quaternion it was last given and the
/// rotation matrix made from it. The game reads the pose back through the matrix: the front and
/// up directions are its columns and `al::calcQuat` runs `toQuat` over it, so a quaternion read
/// back is not the one that was stored.
#[derive(Clone, Copy, Debug)]
pub struct ActorPose {
    axes: Axes,
}

impl ActorPose {
    /// `al::updatePoseQuat`.
    pub fn new(stored: Quat) -> Self {
        Self {
            axes: quat_to_axes(stored),
        }
    }

    /// `al::calcQuat(actor)`: what the pose reads back as.
    pub fn calc_quat(&self) -> Quat {
        to_quat(self.axes)
    }

    /// `al::calcUpDir(actor)`.
    pub fn up(&self) -> Vec3 {
        self.axes[1]
    }

    /// `al::calcFrontDir(actor)`.
    pub fn front(&self) -> Vec3 {
        self.axes[2]
    }

    /// `al::calcSideDir(actor)`.
    pub fn side(&self) -> Vec3 {
        self.axes[0]
    }

    /// `rs::slerpUpFront(actor, up, front, rate, max_degrees)`: the pose turned toward `up` (at
    /// most `max_degrees` from the world's up) by the slerp `rate`, its front the given one
    /// carried from the up it was meant for to the limited up. Returns the quaternion to store.
    pub fn slerp_up_front(&self, up: Vec3, front: Vec3, rate: f32, max_degrees: f32) -> Quat {
        let target_up = turn_vec_to_vec_degree(NEGATED_GRAVITY, up, max_degrees);
        let meant_up = quat_up(quat_front_up(front, up));
        let (across, usable) =
            try_normalize_or_zero(along_vector_normal_h(front, meant_up, target_up));
        if usable {
            let target = quat_up_front(target_up, across);
            let blended = slerp(self.calc_quat(), target, rate);
            return quat_up_front(quat_up(blended), across);
        }
        // No usable front: keep the pose's own (its up when that is parallel).
        let mut kept = self.front();
        if is_parallel_direction(target_up, kept, PARALLEL_LIMIT) {
            kept = self.up();
        }
        let target = quat_up_front(target_up, kept);
        slerp(self.calc_quat(), target, rate)
    }

    /// The pose part of `rs::waitGround`: the up turns toward the floor's `normal` by at most
    /// `max_degrees`, the front stays (the pose's up when that is parallel to it), the whole
    /// slerps by `rate`. Returns the quaternion to store (not normalised).
    pub fn wait_ground(&self, normal: Vec3, rate: f32, max_degrees: f32) -> Quat {
        let target_up = turn_vec_to_vec_degree(NEGATED_GRAVITY, normal, max_degrees);
        let mut front = self.front();
        if is_parallel_direction(target_up, front, PARALLEL_LIMIT) {
            front = self.up();
        }
        let target = quat_up_front(target_up, front);
        slerp(self.calc_quat(), target, rate)
    }
}

#[path = "../tests/pose_math.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
