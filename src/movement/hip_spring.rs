//! The hip's spring while the player hangs on a wall (`PlayerJointControlGrabDynamics`): a
//! `al::JointSpringController` on the joint `Hip` that lets the body's lower half swing from the
//! hip as the grip moves it.
//!
//! The spring follows a point 100 cm out along the hip's own x axis (`WallCatchHipLocalOffset`)
//! with a velocity it keeps (`WallCatchHipFriction` of it each step, pushed by
//! `WallCatchHipStability` of the way to the point's present place). Each step the hip is turned
//! about its own position so that its direction to the point turns toward the direction to the
//! spring's lagging place, by at most `WallCatchHipLimitDegree` times the control rate. The
//! wall catch state steers the rate through the keeper's block: whole when the grip starts,
//! down to a fifth while the player moves along the wall or climbs, nothing otherwise
//! (`GrabDynamics::update`).
//!
//! The arithmetic follows the executable's single-precision order (`calcJointCallback`,
//! `canCompute`, `compute` and `al::turnQuat` of the spring controller).
use super::joint_math::{converge, make_quat_rotation_limit};
use super::look_joint::Mat34;
use super::pose_math::{DEGREES_TO_RADIANS, to_quat, try_normalize_or_zero};
use super::vector_lib::is_near_zero_value;
use crate::skeleton_pose::Joint;
use bevy::prelude::{Quat, Vec3};

/// `PlayerConst` defaults (Mario has no player-constant file): the point's place on the hip's
/// x axis, how hard the spring pulls, how much of the speed it keeps, and the turn's limit.
const CHILD_LOCAL_OFFSET: f32 = 100.;
const STABILITY: f32 = 0.1;
const FRICTION: f32 = 0.9;
const LIMIT_DEGREES: f32 = 75.;
/// `al::isNearZero`'s limit for the control rate and the matrix scale.
const NEAR_ZERO: f32 = 0.001;
/// The point's place in the hip's own frame.
const CHILD: Vec3 = Vec3::new(CHILD_LOCAL_OFFSET, 0., 0.);

/// What the wall catch state hands the keeper (`PlayerJointParamGrab`'s first two fields) and the
/// control rate `GrabDynamics` keeps from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrabDynamics {
    target: f32,
    rate: f32,
    control: f32,
}

impl Default for GrabDynamics {
    fn default() -> Self {
        // The keeper's block starts with a target of nothing, followed at once (rate 1).
        Self {
            target: 0.,
            rate: 1.,
            control: 0.,
        }
    }
}

impl GrabDynamics {
    /// The wall catch's `appear`: the spring is whole and follows at once.
    pub fn appear(&mut self) {
        self.target = 1.;
        self.rate = 1.;
    }

    /// The wall catch's `kill` (and the state not running): no spring.
    pub fn release(&mut self) {
        self.target = 0.;
        self.rate = 1.;
    }

    /// A move along the wall or the climb: the target eases down to a fifth (`converge(target,
    /// 0.2, 0.2)`).
    pub fn ease_down(&mut self) {
        self.target = converge(self.target, 0.2, 0.2);
    }

    /// The end of a move along the wall: the target is whole again.
    pub fn restore(&mut self) {
        self.target = 1.;
    }

    /// `GrabDynamics::update`: the control rate follows the target by the block's rate, once a
    /// step. Returns the rate the spring is given.
    pub fn update(&mut self) -> f32 {
        self.control = converge(self.control, self.target, self.rate);
        self.control
    }
}

/// The spring controller's state (`al::JointSpringController`): where its point was last step,
/// the velocity it has kept and the control rate it was given.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HipSpring {
    /// `+0x3c`: the point's place (the spring's own).
    place: Vec3,
    /// `+0x48`: its velocity.
    velocity: Vec3,
    /// `+0x100`: the first calculation has been made.
    started: bool,
    /// `+0x60`.
    control_rate: f32,
}

impl HipSpring {
    pub fn set_control_rate(&mut self, rate: f32) {
        self.control_rate = rate;
    }

    /// `calcJointCallback`: bend the hip's matrix `world` if the spring is on.
    pub fn calc(&mut self, joint: Joint, world: &mut Mat34) {
        if joint != Joint::Hip {
            return;
        }
        let Some(scale) = self.can_compute(world) else {
            return;
        };
        let at = Vec3::new(world[0][3], world[1][3], world[2][3]);
        self.compute(world, scale, at, point_of(world));
    }

    /// `canCompute`: the scale of the matrix when the spring is to act. Before its first
    /// calculation it only notes where the point is; while the control rate is nothing it keeps
    /// the point at the hip and lets the velocity die away.
    fn can_compute(&mut self, world: &Mat34) -> Option<Vec3> {
        if !self.started {
            self.place = point_of(world);
            self.started = true;
            return None;
        }
        if is_near_zero_value(self.control_rate, NEAR_ZERO) {
            self.place = Vec3::new(world[0][3], world[1][3], world[2][3]);
            self.velocity = Vec3::new(
                FRICTION * self.velocity.x,
                FRICTION * self.velocity.y,
                FRICTION * self.velocity.z,
            );
            return None;
        }
        let scale = matrix_scale(world);
        let flat = is_near_zero_value(scale.x, NEAR_ZERO) || is_near_zero_value(scale.y, NEAR_ZERO);
        (!flat && !is_near_zero_value(scale.z, NEAR_ZERO)).then_some(scale)
    }

    /// `compute`: spring the point and turn the hip's rotation toward it.
    fn compute(&mut self, world: &mut Mat34, scale: Vec3, joint: Vec3, point: Vec3) {
        let place = self.place;
        let velocity = self.velocity;
        let kept = Vec3::new(
            FRICTION * (velocity.x + (point.x - place.x) * STABILITY),
            FRICTION * (velocity.y + (point.y - place.y) * STABILITY),
            FRICTION * (velocity.z + (point.z - place.z) * STABILITY),
        );
        self.velocity = kept;
        let sprung = Vec3::new(place.x + kept.x, place.y + kept.y, place.z + kept.z);
        let (to_spring, found) = try_normalize_or_zero(Vec3::new(
            sprung.x - joint.x,
            sprung.y - joint.y,
            sprung.z - joint.z,
        ));
        if !found {
            return;
        }
        let (to_point, found) = try_normalize_or_zero(Vec3::new(
            point.x - joint.x,
            point.y - joint.y,
            point.z - joint.z,
        ));
        if !found {
            return;
        }
        pre_scale(world, Vec3::new(1. / scale.x, 1. / scale.y, 1. / scale.z));
        let rotation = to_quat([
            Vec3::new(world[0][0], world[1][0], world[2][0]),
            Vec3::new(world[0][1], world[1][1], world[2][1]),
            Vec3::new(world[0][2], world[1][2], world[2][2]),
        ]);
        let limit = (LIMIT_DEGREES * self.control_rate) * DEGREES_TO_RADIANS;
        let turned = turn_quat(rotation, to_point, to_spring, limit);
        *world = matrix_of(turned, joint);
        pre_scale(world, scale);
        self.place = point_of(world);
    }
}

/// `calcChildPos`: the point 100 cm along the matrix's x axis, in the world.
fn point_of(m: &Mat34) -> Vec3 {
    let row = |r: usize| {
        let sum = (CHILD.x * m[r][0] + CHILD.y * m[r][1]) + CHILD.z * m[r][2];
        m[r][3] + sum
    };
    Vec3::new(row(0), row(1), row(2))
}

/// `al::calcMtxScale`: the lengths of the matrix's columns.
fn matrix_scale(m: &Mat34) -> Vec3 {
    let length = |c: usize| ((m[0][c] * m[0][c] + m[1][c] * m[1][c]) + m[2][c] * m[2][c]).sqrt();
    Vec3::new(length(0), length(1), length(2))
}

/// `al::preScaleMtx`: each column scaled.
fn pre_scale(m: &mut Mat34, scale: Vec3) {
    for row in m.iter_mut() {
        row[0] *= scale.x;
        row[1] *= scale.y;
        row[2] *= scale.z;
    }
}

/// `al::turnQuat(out, q, from, to, limit)`: `q` turned so that `from` moves toward `to` by at
/// most `limit` radians. (Two opposed directions pick a random axis in the original; here the
/// axis `get_axis_angle_from_two_vec` gives.)
fn turn_quat(q: Quat, from: Vec3, to: Vec3, limit: f32) -> Quat {
    let (from, _) = try_normalize_or_zero(from);
    let (to, _) = try_normalize_or_zero(to);
    let t = make_quat_rotation_limit(from, to, limit);
    // The product t * q, in the executable's order of operations.
    let w = ((t.w * q.w - t.x * q.x) - t.y * q.y) - t.z * q.z;
    let x = ((t.x * q.w + t.w * q.x) + t.y * q.z) - t.z * q.y;
    let y = t.z * q.x + (t.y * q.w + (t.w * q.y - t.x * q.z));
    let z = t.z * q.w + ((t.x * q.y + t.w * q.z) - t.y * q.x);
    let squared = (((w * w) + (x * x)) + (y * y)) + (z * z);
    let length = squared.sqrt();
    if length > 0. {
        let inverse = 1. / length;
        Quat::from_xyzw(inverse * x, inverse * y, inverse * z, inverse * w)
    } else {
        Quat::from_xyzw(x, y, z, w)
    }
}

/// The matrix of a unit quaternion at `at`.
fn matrix_of(q: Quat, at: Vec3) -> Mat34 {
    let (x, y, z, w) = (q.x, q.y, q.z, q.w);
    let (two_x, two_y, two_z, two_w) = (x + x, y + y, z + z, w + w);
    let zz = z * two_z;
    let xy = y * two_x;
    let yy = y * two_y;
    let xx = x * two_x;
    let xz = z * two_x;
    let yz = two_y * z;
    let zw = z * two_w;
    let xw = x * two_w;
    let yw = y * two_w;
    [
        [(1. - yy) - zz, xy - zw, xz + yw, at.x],
        [xy + zw, (1. - xx) - zz, yz - xw, at.y],
        [xz - yw, yz + xw, (1. - xx) - yy, at.z],
    ]
}

#[path = "../tests/hip_spring.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
