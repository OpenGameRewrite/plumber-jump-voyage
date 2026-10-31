//! Dimension input and its held camera basis (PlayerInput 0x44b91c / 0x44e8ec).
//! All positions are in the actor's coordinate frame; recordings never seed this memory.
use super::move_input::ViewRows;
use super::pose_math::{
    NO_DIRECTION, atan2f, cosf, cross, dot, is_parallel_direction, normalize, sinf, squared_length,
    try_normalize_or_zero,
};
use super::vector_lib::verticalize_vec;
use bevy::math::{Quat, Vec2, Vec3};

// Original literals: 0x187fc80 at 0x44bc18/0x44bfc0; 0x187fd64 at 0x44e968;
// 0x187fc84 at 0x44ac38/0x44b774; 0x187fe48 at 0x44bcf0.
const NEAR_ZERO: f32 = 0.001;
const PARALLEL: f32 = 0.01;
const DEADZONE: f32 = 0.1;
const GROUND_COS: f32 = -0.707_106_77;

#[derive(Clone, Debug, Default)]
pub struct Input2D {
    pub enabled: bool,
    pub held: bool,
    stick: Vec2,
    mapped_stick: Vec2,
    axes: [Vec3; 2],
    up: Vec3,
    mapped_axes: [Vec3; 2],
    lock: Vec3,
    ground_position: Vec3,
    ground_normal: Vec3,
}

/// The same inputs the original queries from its actor, collision and camera owners.
#[derive(Clone, Debug)]
pub struct Frame {
    pub active: bool,
    pub stick: Vec2,
    pub view: ViewRows,
    pub position: Vec3,
    pub velocity: Vec3,
    pub actor_side: Vec3,
    pub actor_up: Vec3,
    pub actor_front: Vec3,
    pub lock: Vec3,
    pub up: Vec3,
    pub ground: Option<(Vec3, Vec3)>,
    pub just_landed: bool,
}

fn stick_length(v: Vec2) -> f32 {
    (v.x * v.x + v.y * v.y).sqrt()
}

fn normalize_stick(v: Vec2) -> Option<Vec2> {
    let sq = v.x * v.x + v.y * v.y;
    (sq >= NO_DIRECTION).then(|| v * (1. / sq.sqrt()))
}

/// Camera axes in the supplied plane (0x44f630), including the degenerate fallbacks.
pub fn axes(view: &ViewRows, up: Vec3) -> [Vec3; 2] {
    let toward_up = dot(view[1], up);
    let looking = ((up.x * -view[2].x) - up.y * view[2].y) - up.z * view[2].z;
    let reference = if looking.abs() > toward_up.abs() {
        if looking < 0. { -view[1] } else { view[1] }
    } else if toward_up < 0. {
        -view[2]
    } else {
        view[2]
    };
    let (side, usable) = try_normalize_or_zero(cross(up, reference));
    let side = if usable { side } else { view[0] };
    let (ahead, usable) = try_normalize_or_zero(cross(side, up));
    let mut ahead = if usable { ahead } else { view[1] };
    if dot(view[1], ahead) < 0. {
        ahead = -ahead;
    }
    [side, ahead]
}

fn camera_lock(frame: &Frame) -> Vec3 {
    let side = frame.view[2];
    let facing = ((-side.y * frame.lock.y) - side.x * frame.lock.x) - side.z * frame.lock.z;
    if facing.abs() < NEAR_ZERO || facing > 0. {
        -frame.lock
    } else {
        frame.lock
    }
}

fn dominant(stick: Vec2, basis: [Vec3; 2]) -> Vec3 {
    if stick.x.abs() >= stick.y.abs() {
        basis[0] * stick.x
    } else {
        basis[1] * stick.y
    }
}

/// Project the camera basis into the movement area (0x44fa60). The smaller projection
/// borrows the larger one's direction; front/velocity resolves a camera looking at the edge.
fn map_axes(frame: &Frame, stick: Vec2, basis: [Vec3; 2], up: Vec3) -> [Vec3; 2] {
    let (travel, usable) = try_normalize_or_zero(verticalize_vec(up, frame.velocity));
    let travel = if usable { travel } else { frame.actor_front };
    let (travel, usable) = try_normalize_or_zero(verticalize_vec(frame.lock, travel));
    let travel = if usable { travel } else { frame.actor_front };
    let signs = [
        if stick.x < 0. { -1. } else { 1. },
        if stick.y < 0. { -1. } else { 1. },
    ];
    let mut out = basis.map(|axis| verticalize_vec(frame.lock, axis));
    if squared_length(out[0]).sqrt() < squared_length(out[1]).sqrt() {
        out[1] = normalize(out[1]);
        let align = dot(frame.view[0], out[1]);
        out[0] = if align.abs() < NEAR_ZERO {
            if stick.x.abs() > stick.y.abs() && stick.y.abs() >= NEAR_ZERO {
                out[1] * (signs[0] * signs[1])
            } else {
                travel * signs[0]
            }
        } else if align < 0. {
            -out[1]
        } else {
            out[1]
        };
        out[0] = normalize(out[0]);
    } else {
        out[0] = normalize(out[0]);
        let vertical = dot(frame.view[1], out[0]);
        let back = frame.view[2];
        let forward = ((-back.y * out[0].y) - back.x * out[0].x) - back.z * out[0].z;
        let align = if vertical.abs() < forward.abs() {
            forward
        } else {
            vertical
        };
        out[1] = if align.abs() < NEAR_ZERO {
            if stick.y.abs() > stick.x.abs() && stick.x.abs() >= NEAR_ZERO {
                out[0] * (signs[0] * signs[1])
            } else {
                travel * signs[1]
            }
        } else if align > 0. {
            out[0]
        } else {
            -out[0]
        };
        out[1] = normalize(out[1]);
    }
    out
}

// makeQuatAxisRotation 0x926efc: keep the degree/radian round trip (literals
// 0x187fe74 and 0x187fd4c, loaded at 0x927020 and 0x927034).
fn axis_rotation(from: Vec3, to: Vec3, axis: Vec3) -> Quat {
    let from = verticalize_vec(axis, from);
    let to = verticalize_vec(axis, to);
    let turn = cross(from, to);
    let angle = atan2f(squared_length(turn).sqrt(), dot(from, to));
    let angle = if dot(axis, turn) < 0. { -angle } else { angle };
    let half = ((angle * 57.295_776) * 0.017_453_292) * 0.5;
    let sine = sinf(half);
    Quat::from_xyzw(sine * axis.x, sine * axis.y, sine * axis.z, cosf(half))
}

// The two inlined quaternion products in calcHoldMoveInput2D associate their sums
// differently. Retain each order instead of using the SIMD quaternion operation.
fn rotate(v: Vec3, q: Quat, second: bool) -> Vec3 {
    let x = (q.y * v.z - q.z * v.y) + q.w * v.x;
    let y = q.w * v.y + (q.z * v.x - q.x * v.z);
    let z = q.w * v.z + (q.x * v.y - q.y * v.x);
    let w = (-v.x * q.x - q.y * v.y) - q.z * v.z;
    let out_x = if second {
        q.y * z + (q.w * x - q.z * y)
    } else {
        (q.w * x - q.z * y) + q.y * z
    };
    let out_y = if second {
        q.z * x + q.w * y
    } else {
        q.z * x + y * q.w
    };
    Vec3::new(
        out_x - q.x * w,
        (out_y - z * q.x) - w * q.y,
        ((q.x * y - x * q.y) + z * q.w) - w * q.z,
    )
}

impl Input2D {
    fn held_input(&self, frame: &Frame, up: Vec3) -> Vec3 {
        let basis = axes(&frame.view, up);
        let axis = if is_parallel_direction(self.up, up, PARALLEL) {
            let side = frame.actor_side;
            if is_parallel_direction(self.up, side, PARALLEL) {
                return dominant(frame.stick, map_axes(frame, frame.stick, basis, up));
            }
            side
        } else {
            cross(self.up, up)
        };
        let rotation = axis_rotation(self.up, up, normalize(axis));
        let mut mapped = self.mapped_axes.map(|v| rotate(v, rotation, false));
        let lock = camera_lock(frame);
        if is_parallel_direction(self.lock, lock, PARALLEL) {
            if dot(self.lock, lock) < 0. {
                mapped = mapped.map(|v| -v);
            }
        } else {
            let rotation = axis_rotation(self.lock, lock, normalize(cross(self.lock, lock)));
            mapped = mapped.map(|v| rotate(v, rotation, true));
        }
        dominant(self.mapped_stick, mapped.map(normalize))
    }

    /// calcMoveInputImpl's 2D path. The no-hold branch returns directly; the held
    /// branch rescales to the raw stick length (0x44b744..0x44b7d4).
    pub fn move_input(&self, frame: &Frame, up: Vec3) -> Vec3 {
        if !self.enabled {
            return Vec3::ZERO;
        }
        if !self.held {
            return dominant(
                frame.stick,
                map_axes(frame, frame.stick, axes(&frame.view, up), up),
            );
        }
        let push = self.held_input(frame, up);
        let tilt = stick_length(frame.stick).min(1.);
        if tilt < DEADZONE {
            return Vec3::ZERO;
        }
        let length = squared_length(push).sqrt();
        if length > 0. {
            push * (tilt / length)
        } else {
            push
        }
    }

    fn direct(frame: &Frame, up: Vec3) -> Vec3 {
        let [side, ahead] = axes(&frame.view, up);
        let push = side * frame.stick.x + ahead * frame.stick.y;
        let tilt = stick_length(frame.stick).min(1.);
        if tilt < DEADZONE {
            return Vec3::ZERO;
        }
        let length = squared_length(push).sqrt();
        if length > 0. {
            push * (tilt / length)
        } else {
            push
        }
    }

    fn remember_ground(&mut self, frame: &Frame) {
        (self.ground_position, self.ground_normal) =
            frame.ground.unwrap_or((frame.position, frame.up));
    }

    pub fn update(&mut self, frame: &Frame) {
        self.enabled = stick_length(frame.stick) >= DEADZONE;
        if !frame.active {
            if self.held {
                *self = Self {
                    enabled: self.enabled,
                    ..Self::default()
                };
            }
            return;
        }
        // isSameStickMove 0x44ddcc: normalized dot >= cos(10 degrees).
        // 10 is passed at 0x44ba8c; the radian literal is 0x187fd4c.
        let same = self.enabled
            && normalize_stick(self.stick)
                .zip(normalize_stick(frame.stick))
                .is_some_and(|(a, b)| a.x * b.x + a.y * b.y >= cosf(10. * 0.017_453_292));
        let mut recheck_vertical = true;
        if same {
            let old_position = self.ground_position;
            let old_normal = self.ground_normal;
            let old_length = stick_length(self.mapped_stick);
            let length = stick_length(frame.stick);
            if old_length < length && old_length > 0. {
                self.mapped_stick *= length / old_length;
            }
            if frame.ground.is_some() {
                self.remember_ground(frame);
            }
            let usable = try_normalize_or_zero(self.move_input(frame, frame.up)).1;
            let consistent = if frame.just_landed {
                dot(self.ground_position - old_position, old_normal) < NEAR_ZERO
                    || dot(old_normal, self.ground_normal) > -NEAR_ZERO
            } else if frame.ground.is_some() {
                if usable {
                    return;
                }
                true
            } else {
                dot(frame.position - old_position, old_normal) < NEAR_ZERO
                    || dot(old_normal, frame.up) >= GROUND_COS
            };
            if usable && consistent {
                return;
            }
            recheck_vertical = consistent || !usable;
        }
        if recheck_vertical {
            let input = try_normalize_or_zero(Self::direct(frame, normalize(frame.actor_side))).0;
            let alignment = dot(input, normalize(frame.actor_up));
            // 0x44db8c / 0x44dbf0 / 0x44dbfc load 0x18804a0,
            // 0x1880214 and 0x187ffa4 respectively.
            let vertical = alignment < -0.866_025_4
                || alignment
                    >= if frame.ground.is_some() {
                        0.984_807_7
                    } else {
                        0.866_025_4
                    };
            self.enabled &= !vertical;
        }
        if !self.enabled {
            *self = Self::default();
            return;
        }
        self.held = true;
        self.stick = frame.stick;
        let side = normalize(frame.actor_side);
        let basis = axes(&frame.view, side);
        let input = try_normalize_or_zero(Self::direct(frame, side)).0;
        let input = verticalize_vec(normalize(frame.actor_up), input);
        self.mapped_stick = Vec2::new(dot(basis[0], input), dot(input, basis[1]));
        let length = stick_length(self.mapped_stick);
        if length > 0. {
            self.mapped_stick *= stick_length(frame.stick) / length;
        }
        self.axes = axes(&frame.view, frame.up);
        self.up = normalize(frame.up);
        self.mapped_axes = map_axes(frame, self.mapped_stick, self.axes, frame.up);
        self.lock = camera_lock(frame);
        self.remember_ground(frame);
    }
}

#[path = "../tests/input_2d.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
