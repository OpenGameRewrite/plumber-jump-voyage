//! The head joint's look-at controller (`al::JointLookAtController::calcJointCallback`), as the
//! player's `PlayerJointControlKeeper` configures it for the joint named `Head`.
//!
//! The model calculates its joint matrices every frame; for the head the controller then turns
//! the animated matrix toward the point it was asked to look at. Two clamps keep the turn in
//! the head's range: the direction to the point, in the actor's own frame, is limited to 60
//! degrees to either side and -10 to +50 degrees of elevation; the limited direction is then
//! carried into the head joint's axes and limited again. The turn itself is a quaternion
//! (`wanted`) that the controller eases toward with a fixed share per call (0.15), so the head
//! follows the point over several frames and returns to its animation over several more when
//! nothing is asked. The result is applied on the right of the animated rotation (the head turns
//! about its own axes).
//!
//! Only the configuration the player uses is ported: no parent matrix of the joint's own, and the
//! two per-joint flags that the other append functions set are off. The arithmetic follows the
//! executable's single-precision order, fused multiply-adds included where it uses them, and is
//! checked bit for bit against the executable (`PJV_LOOKAT_CASES`, see `tests/look_joint.rs`).
use super::pose_math::{
    DEGREES_TO_RADIANS, acosf, asinf, atan2f, cosf, cross, dot, sinf, squared_length, to_quat,
    try_normalize_or_zero,
};
use super::vector_lib::is_near_zero;
use bevy::prelude::{Quat, Vec3};

/// A `sead::Matrix34f`: three rows of the rotation and the translation column.
pub type Mat34 = [[f32; 4]; 3];

/// The share of the way to the wanted turn made in one call (`appendJointLookAtController`'s
/// rate argument for the head).
const FOLLOW_SHARE: f32 = 0.15;
/// The head's range in the actor's frame: yaw to either side, elevation down and up.
const YAW_LIMIT_DEGREES: f32 = 60.;
const PITCH_LIMIT_DEGREES: [f32; 2] = [-10., 50.];
/// The actor turning more than this about the vertical in one call resets the controller.
const ACTOR_TURN_LIMIT_DEGREES: f32 = 75.;
/// A target within this cosine of the actor's front is not followed when the controller is not
/// already following it; once it is, the target only has to be in front (cosine over zero).
const START_FOLLOWING_COSINE: f32 = 0.1;
/// `al::isNearZero` limit for the actor's front.
const FRONT_NEAR_ZERO: f32 = 0.001;
/// `al::isParallelDirection` limit for the second clamp's front against the up axis.
const PARALLEL_LIMIT: f32 = 0.01;
/// The quaternion's squared length at which `1 / length^2` stops being applied to the basis
/// quaternion (`1.1920929e-7`).
const BASIS_NORMALISE_LIMIT: f32 = 1.192_092_9e-7;
/// Radians to degrees in single precision (`57.2957763671875`).
const RADIANS_TO_DEGREES: f32 = 57.295_776;
/// The signed elevation `calcSphericalPolarCoordPY` gives a direction along the up axis.
const STRAIGHT_UP_PITCH: [f32; 2] = [-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2];
/// A vector shorter than this (squared) has no direction (`1.0000001e-6`).
const NO_DIRECTION: f32 = 1.000_000_1e-6;

/// The head's axes in the joint: front (x), up (-y) and side (z) as `JointLookAtParam` holds
/// them, and the quaternion of the matrix with those columns, which carries the turn from the
/// frame the angles are measured in (front +z, up +y) into the joint's.
struct Axes {
    front: Vec3,
    up: Vec3,
    side: Vec3,
    /// The basis quaternion as `toQuat` gives it, and the same divided by its squared length
    /// (what `inverse` multiplies by).
    basis: Quat,
    basis_scaled: Quat,
}

impl Axes {
    fn head() -> Self {
        let front = Vec3::X;
        let up = Vec3::new(-0., -1., -0.);
        // JointLookAtParam: the side axis is the cross product of the up and front axes.
        let side = try_normalize_or_zero(cross(up, front)).0;
        let basis = to_quat([side, up, front]);
        let squared =
            ((basis.w * basis.w + basis.x * basis.x) + basis.y * basis.y) + basis.z * basis.z;
        let basis_scaled = if squared > BASIS_NORMALISE_LIMIT {
            let k = 1. / squared;
            Quat::from_xyzw(basis.x * k, basis.y * k, basis.z * k, basis.w * k)
        } else {
            basis
        };
        Self {
            front,
            up,
            side,
            basis,
            basis_scaled,
        }
    }
}

/// `al::calcSphericalPolarCoordPY(out, dir, front, up)`: the elevation of `dir` over the plane
/// across `up` and its angle about `up` from `front` (signed by the side `up x front`), both in
/// radians, as `(pitch, yaw)`. `None` when the direction is too short to have one. A direction
/// along `up` gives a pitch of a quarter turn and no yaw (and the caller is told it failed).
fn polar_pitch_yaw(dir: Vec3, front: Vec3, up: Vec3) -> (Option<(f32, f32)>, bool) {
    if squared_length(dir) < NO_DIRECTION {
        return (None, false);
    }
    let across_up = |a: f32, b: f32, c: f32, d: f32| (a * b - c * d).abs() > PARALLEL_LIMIT;
    let along_up = !(across_up(dir.z, up.y, dir.y, up.z)
        || across_up(dir.x, up.z, dir.z, up.x)
        || across_up(dir.y, up.x, dir.x, up.y));
    if along_up {
        let toward = (up.x * dir.x + up.y * dir.y) + up.z * dir.z;
        let pitch = STRAIGHT_UP_PITCH[usize::from(toward > 0.)];
        return (Some((pitch, 0.)), false);
    }
    // The side axis `up x front` and the direction's components on up, side and front.
    let side = Vec3::new(
        up.y * front.z - up.z * front.y,
        up.z * front.x - front.z * up.x,
        front.y * up.x - up.y * front.x,
    );
    let up_part = (up.x * dir.x + up.y * dir.y) + up.z * dir.z;
    let side_part = (side.x * dir.x + side.y * dir.y) + side.z * dir.z;
    let front_part = (front.x * dir.x + front.y * dir.y) + front.z * dir.z;
    let length = (front_part * front_part + (side_part * side_part + up_part * up_part)).sqrt();
    let across = (side_part * side_part + front_part * front_part).sqrt();
    let pitch = asinf((up_part / length).clamp(-1., 1.));
    let sign = if side_part < 0. {
        -1.
    } else if side_part > 0. {
        1.
    } else {
        side_part
    };
    let yaw = sign * acosf((front_part / across).clamp(-1., 1.));
    (Some((pitch, yaw)), true)
}

/// `al::tryCalcAngleOnPlaneDegree(out, from, to, normal)`: the signed angle in degrees from
/// `from` to `to` after both are flattened onto the plane across `normal`.
fn angle_on_plane_degrees(from: Vec3, to: Vec3, normal: Vec3) -> Option<f32> {
    if squared_length(from) < NO_DIRECTION || squared_length(to) < NO_DIRECTION {
        return None;
    }
    let off_normal = |v: Vec3| {
        let c = Vec3::new(
            v.y * normal.z - v.z * normal.y,
            v.z * normal.x - v.x * normal.z,
            v.x * normal.y - v.y * normal.x,
        );
        c.x.abs() > PARALLEL_LIMIT || c.y.abs() > PARALLEL_LIMIT || c.z.abs() > PARALLEL_LIMIT
    };
    if !off_normal(from) || !off_normal(to) {
        return None;
    }
    let flatten = |v: Vec3| {
        let along = (normal.x * v.x + normal.y * v.y) + normal.z * v.z;
        Vec3::new(
            v.x - normal.x * along,
            v.y - normal.y * along,
            v.z - normal.z * along,
        )
    };
    let (a, b) = (flatten(from), flatten(to));
    let cosine = (a.x * b.x + a.y * b.y) + a.z * b.z;
    let across = Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    );
    let sine = ((across.x * across.x + across.y * across.y) + across.z * across.z).sqrt();
    let angle = atan2f(sine, cosine);
    let turns_about_normal = (normal.x * across.x + normal.y * across.y) + normal.z * across.z;
    let angle = if turns_about_normal < 0. {
        -angle
    } else {
        angle
    };
    Some(angle * RADIANS_TO_DEGREES)
}

/// `al::calcQuatFront(out, q)`: the quaternion's third column (the rotated +z axis).
fn quat_front(q: Quat) -> Vec3 {
    let xz2 = q.x * q.z + q.x * q.z;
    let wy2 = q.w * q.y + q.w * q.y;
    let zy2 = q.z * q.y + q.z * q.y;
    let xw2 = q.x * q.w + q.x * q.w;
    let xx2 = q.x * q.x + q.x * q.x;
    let yy2 = q.y * q.y + q.y * q.y;
    Vec3::new(xz2 + wy2, zy2 - xw2, (1. - xx2) - yy2)
}

/// The quaternion that turns by `yaw` about the up axis and then undoes a turn of `pitch` about
/// the front axis of the joint (the order and signs of the executable's product), from angles in
/// radians; its `+z` axis is the direction the angles describe.
fn quat_from_pitch_yaw(pitch: f32, yaw: f32, axes: &Axes) -> Quat {
    // The yaw turns about +y and the pitch about the joint's front axis; both are written as
    // (axis * sin(half), cos(half)).
    let half_yaw = yaw * 0.5;
    let (yaw_cos, yaw_sin) = (cosf(half_yaw), sinf(half_yaw));
    let (y_x, y_y, y_z, y_w) = (yaw_sin * 0., yaw_sin * 1., yaw_sin * 0., yaw_cos);
    let half_pitch = pitch * 0.5;
    let (pitch_cos, pitch_sin) = (cosf(half_pitch), sinf(half_pitch));
    let (p_x, p_y, p_z, p_w) = (
        axes.front.x * pitch_sin,
        axes.front.y * pitch_sin,
        axes.front.z * pitch_sin,
        pitch_cos,
    );
    let minus_p_y = -(axes.front.y * pitch_sin);
    let w = ((y_w * p_w + y_x * p_x) + y_y * p_y) + y_z * p_z;
    let x = ((y_x * p_w - y_w * p_x) - y_y * p_z) + y_z * p_y;
    let y = (y_y * p_w + (y_x * p_z - y_w * p_y)) - y_z * p_x;
    let z = y_z * p_w + ((y_x * minus_p_y - y_w * p_z) + y_y * p_x);
    Quat::from_xyzw(x, y, z, w)
}

/// The executable's `q * basis^-1` step on `basis * current`, in its term order: the rotation
/// `basis * current * conj(basis_scaled)`.
fn conjugated_by_basis(current: Quat, axes: &Axes) -> Quat {
    let b = axes.basis;
    let c = current;
    let first = Quat::from_xyzw(
        ((b.x * c.w + b.w * c.x) + b.y * c.z) - b.z * c.y,
        b.z * c.x + (b.y * c.w + (b.w * c.y - b.x * c.z)),
        b.z * c.w + ((b.x * c.y + b.w * c.z) - b.y * c.x),
        ((b.w * c.w - b.x * c.x) - b.y * c.y) - b.z * c.z,
    );
    let p = axes.basis_scaled;
    Quat::from_xyzw(
        ((p.w * first.x - first.w * p.x) - first.y * p.z) + first.z * p.y,
        (p.w * first.y + (first.x * p.z - first.w * p.y)) - first.z * p.x,
        p.w * first.z + ((first.w * -p.z - first.x * p.y) + first.y * p.x),
        ((p.w * first.w + first.x * p.x) + first.y * p.y) + first.z * p.z,
    )
}

/// The 3x3 rotation matrix of a quaternion as the executable builds it, as rows.
fn rotation_rows(q: Quat) -> [[f32; 3]; 3] {
    let x2 = q.x + q.x;
    let w2 = q.w + q.w;
    let y2 = q.y + q.y;
    let zz = q.z * (q.z + q.z);
    let yy = q.y * y2;
    let xx = q.x * x2;
    let xy = q.y * x2;
    let xz = q.z * x2;
    let yz = q.z * y2;
    let zw = q.z * w2;
    let yw = q.y * w2;
    let xw = q.x * w2;
    [
        [(1. - yy) - zz, xy - zw, xz + yw],
        [xy + zw, (1. - xx) - zz, yz - xw],
        [xz - yw, yz + xw, (1. - xx) - yy],
    ]
}

/// A joint controller for the head: the point to look at, the controller's mode flags and the
/// head's state (`JointLookAtInfo`).
#[derive(Clone, Debug)]
pub struct HeadLookAt {
    /// The point asked for, in world space (kept when nothing is asked).
    target: Vec3,
    /// A point was asked for since the last `end_frame` (`requested` this frame).
    asked: bool,
    /// The previous frame's request, which is what the callback reads.
    requested: bool,
    /// The controller keeps the turn it had when the target leaves the range (mode flag).
    keeps_turn: bool,
    /// The next callback puts the head straight on its wanted turn.
    snap: bool,
    /// The turn the head currently has, in the polar frame, and the one it eases toward.
    current: Quat,
    wanted: Quat,
    /// The head followed the target at the last call (a wider range keeps it).
    following: bool,
    /// The actor's front at the last call.
    last_front: Vec3,
    /// The rotation the last call put on the joint (applied on the right of its matrix).
    turn: Quat,
}

impl Default for HeadLookAt {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            asked: false,
            requested: false,
            keeps_turn: false,
            snap: false,
            current: Quat::IDENTITY,
            wanted: Quat::IDENTITY,
            following: false,
            last_front: Vec3::Z,
            turn: Quat::IDENTITY,
        }
    }
}

/// What the callback needs to know that does not change: the head's axes and limits.
fn axes() -> &'static Axes {
    use std::sync::OnceLock;
    static AXES: OnceLock<Axes> = OnceLock::new();
    AXES.get_or_init(Axes::head)
}

/// Limit `value` to `[low, high]` the way the executable's compare-and-select does.
fn limit(value: f32, low: f32, high: f32) -> f32 {
    if value < low {
        low
    } else if value > high {
        high
    } else {
        value
    }
}

impl HeadLookAt {
    /// `requestJointLookAt`: look at `point` (world space) at the next callback.
    pub fn request(&mut self, point: Vec3) {
        self.asked = true;
        self.target = point;
    }

    /// The mode flag of the controller (`PlayerJointControlKeeper::updateLookAt` sets it each
    /// frame): while set, a target that leaves the head's range keeps the turn it had.
    pub fn set_keeps_turn(&mut self, keeps_turn: bool) {
        self.keeps_turn = keeps_turn;
    }

    /// The end of the keeper's frame: what was asked is what the callback sees this frame.
    pub fn end_frame(&mut self) {
        self.requested = self.asked;
        self.asked = false;
    }

    /// Whether the callback of this frame sees a request (the probe's `requested`).
    pub fn requested(&self) -> bool {
        self.requested
    }

    pub fn keeps_turn(&self) -> bool {
        self.keeps_turn
    }

    /// The turn the head currently has (test hook and trace).
    pub fn current_turn(&self) -> Quat {
        self.current
    }

    /// The rotation the last call put on the right of the joint's matrix (none when it left the
    /// joint alone): a joint's local rotation is turned the same way.
    #[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
    pub fn turn(&self) -> Quat {
        self.turn
    }

    /// How far that turn is from none, in degrees.
    pub fn turn_degrees(&self) -> f32 {
        2. * self.current.w.abs().min(1.).acos() * RADIANS_TO_DEGREES
    }

    /// `JointLookAtController::calcJointCallback` for the head joint. `joint` is the head's
    /// animated world matrix and is replaced by the turned one; `actor` is the player's pose
    /// matrix (its third column is the front).
    pub fn apply(&mut self, joint: &mut Mat34, actor: &Mat34) {
        self.turn = Quat::IDENTITY;
        let axes = axes();
        let to_point = Vec3::new(
            self.target.x - joint[0][3],
            self.target.y - joint[1][3],
            self.target.z - joint[2][3],
        );
        let (to_point, usable) = try_normalize_or_zero(to_point);
        if !usable {
            return;
        }
        let threshold = if self.following {
            0.
        } else {
            START_FOLLOWING_COSINE
        };
        let front = Vec3::new(actor[0][2], actor[1][2], actor[2][2]);
        if !is_near_zero(front, FRONT_NEAR_ZERO) {
            // The actor swinging round faster than the head can follow starts the head over.
            let before = self.last_front;
            self.last_front = front;
            let turn = angle_on_plane_degrees(before, front, Vec3::Y);
            if turn.is_none_or(|degrees| degrees.abs() > ACTOR_TURN_LIMIT_DEGREES) {
                self.current = Quat::IDENTITY;
                self.wanted = Quat::IDENTITY;
                return;
            }
        }
        let follows = threshold < dot(to_point, front);
        let wanted = if !self.requested {
            self.following = false;
            Quat::IDENTITY
        } else if !follows {
            self.following = false;
            if self.keeps_turn {
                self.wanted
            } else {
                Quat::IDENTITY
            }
        } else {
            match self.wanted_turn(joint, actor, to_point, axes) {
                Some(wanted) => wanted,
                None => return,
            }
        };
        self.wanted = wanted;
        if self.snap {
            self.current = wanted;
            self.snap = false;
        } else {
            self.current = super::pose_math::slerp(self.current, wanted, FOLLOW_SHARE);
        }
        let turn = conjugated_by_basis(self.current, axes);
        self.turn = turn;
        let rotation = rotation_rows(turn);
        // The matrix times the turn, each element a product and two fused multiply-adds.
        let old = *joint;
        for row in 0..3 {
            for column in 0..3 {
                let first = rotation[0][column] * old[row][0];
                let second = rotation[1][column].mul_add(old[row][1], first);
                let third = rotation[2][column].mul_add(old[row][2], second);
                joint[row][column] = 0. + third;
            }
            // The translation column is added to a zero lane that went through the same
            // operations, which only matters for the sign of a zero.
            let lane = 0.0f32 * old[row][0];
            let lane = 0.0f32.mul_add(old[row][1], lane);
            let lane = 0.0f32.mul_add(old[row][2], lane);
            joint[row][3] = old[row][3] + lane;
        }
    }

    /// The turn that carries the head toward `to_point`: the direction in the actor's frame is
    /// limited, carried into the head's axes and limited again.
    fn wanted_turn(
        &mut self,
        joint: &Mat34,
        actor: &Mat34,
        to_point: Vec3,
        axes: &Axes,
    ) -> Option<Quat> {
        let yaw_limit = YAW_LIMIT_DEGREES * DEGREES_TO_RADIANS;
        let pitch_low = PITCH_LIMIT_DEGREES[0] * DEGREES_TO_RADIANS;
        let pitch_high = PITCH_LIMIT_DEGREES[1] * DEGREES_TO_RADIANS;
        let in_actor = Vec3::new(
            (actor[0][0] * to_point.x + actor[1][0] * to_point.y) + actor[2][0] * to_point.z,
            (actor[0][1] * to_point.x + actor[1][1] * to_point.y) + actor[2][1] * to_point.z,
            (actor[0][2] * to_point.x + actor[1][2] * to_point.y) + actor[2][2] * to_point.z,
        );
        let (angles, _) = polar_pitch_yaw(in_actor, Vec3::Z, Vec3::Y);
        let (pitch, yaw) = angles.unwrap_or((0., 0.));
        let yaw = limit(yaw, -yaw_limit, yaw_limit);
        let pitch = limit(pitch, pitch_low, pitch_high);
        self.following = true;
        // The limited direction back in world space, then in the joint's own axes.
        let limited = quat_front(quat_from_pitch_yaw(pitch, yaw, axes));
        let in_world = Vec3::new(
            (limited.x * actor[0][0] + limited.y * actor[0][1]) + limited.z * actor[0][2],
            (limited.x * actor[1][0] + limited.y * actor[1][1]) + limited.z * actor[1][2],
            (limited.x * actor[2][0] + limited.y * actor[2][1]) + limited.z * actor[2][2],
        );
        let axis_in_joint = |axis: Vec3| {
            Vec3::new(
                (axis.x * joint[0][0] + axis.y * joint[0][1]) + axis.z * joint[0][2],
                (axis.x * joint[1][0] + axis.y * joint[1][1]) + axis.z * joint[1][2],
                (axis.x * joint[2][0] + axis.y * joint[2][1]) + axis.z * joint[2][2],
            )
        };
        let (side, up, front) = (
            axis_in_joint(axes.side),
            axis_in_joint(axes.up),
            axis_in_joint(axes.front),
        );
        let in_head = Vec3::new(
            (side.x * in_world.x + side.y * in_world.y) + side.z * in_world.z,
            (up.x * in_world.x + up.y * in_world.y) + up.z * in_world.z,
            (front.x * in_world.x + front.y * in_world.y) + front.z * in_world.z,
        );
        let (in_head, usable) = try_normalize_or_zero(in_head);
        if !usable {
            return None;
        }
        let (angles, _) = polar_pitch_yaw(in_head, Vec3::Z, Vec3::Y);
        let (pitch, yaw) = angles.unwrap_or((0., 0.));
        let yaw = limit(yaw, -yaw_limit, yaw_limit);
        let pitch = limit(pitch, pitch_low, pitch_high);
        let front = quat_front(quat_from_pitch_yaw(pitch, yaw, axes));
        // `makeQuatFrontSide` for a front along the up axis cannot happen inside the head's range.
        Some(super::pose_math::quat_front_up(front, Vec3::Y))
    }
}

/// A joint's matrix in the frame of the actor's pose: the actor's pose matrix (a rotation with
/// the position in its last column) applied to the joint's matrix in the actor's frame.
pub fn world_matrix(actor: &Mat34, local: &Mat34) -> Mat34 {
    let mut out = [[0.; 4]; 3];
    for r in 0..3 {
        for c in 0..3 {
            out[r][c] = (0..3).map(|k| actor[r][k] * local[k][c]).sum();
        }
        out[r][3] = (0..3).map(|k| actor[r][k] * local[k][3]).sum::<f32>() + actor[r][3];
    }
    out
}

/// The matrix's translation column.
pub fn translation(m: &Mat34) -> Vec3 {
    Vec3::new(m[0][3], m[1][3], m[2][3])
}

/// The matrix `m` and the quaternion it turns into; used by the trace for `head_q`.
pub fn quat_of_rotation(m: &Mat34) -> Quat {
    let columns = [
        Vec3::new(m[0][0], m[1][0], m[2][0]),
        Vec3::new(m[0][1], m[1][1], m[2][1]),
        Vec3::new(m[0][2], m[1][2], m[2][2]),
    ];
    to_quat(columns)
}

#[path = "../tests/look_joint.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
