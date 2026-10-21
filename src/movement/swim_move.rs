//! The movement routines the swim nerves share (`PlayerStateSwim`'s file-local helpers in the
//! 1.0.0 executable): the stroke on the plane of the water (horizontal speed), the vertical
//! motion at the surface and under it, and the turn of the body.
//!
//! Every routine does its single-precision operations in the executable's order. They are
//! written from the machine code; the library calls (`al::separateVelocityDirHV`,
//! `al::alongVectorNormalH`, `al::scaleVectorExceptDirection`, ...) are the ones of
//! `vector_lib.rs` and `pose_math.rs`.
use super::ground::{TurnSetup, ease_in, rate01};
use super::swim_const as k;
use super::vector_lib as lib;
use super::*;

/// The up axis (`-al::getGravity`).
const UP: Vec3 = Vec3::Y;
/// The cosine of 95 degrees: `PlayerActionFunction::isOppositeVec`.
const OPPOSITE_COS: f32 = -0.087_155_74;
/// `PlayerInput::isMoveDeepDown`: the stick is pushed past 0.8.
const DEEP_DOWN_SQUARED: f32 = 0.640_000_05;

/// How a stroke is made (`PlayerStateSwim`'s horizontal helper): which pair of acceleration
/// and top speed it takes.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Stroke {
    /// The high stroke, allowed for a while after a paddle.
    pub high: bool,
    /// At the surface.
    pub surface: bool,
    /// At the surface with the cap spinning.
    pub spin_cap: bool,
}

/// `al::lerpValue`.
fn lerp_value(a: f32, b: f32, t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    a * (1. - t) + t * b
}

/// `sead::Vector3f::setScaleAdd(t, a, b)`: `t * a + b`.
fn scale_add(t: f32, a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(t * a.x + b.x, t * a.y + b.y, t * a.z + b.z)
}

/// A vector times a number, component by component.
fn scaled(v: Vec3, s: f32) -> Vec3 {
    Vec3::new(s * v.x, s * v.y, s * v.z)
}

/// `al::scaleVectorExceptDirection` as the swim calls it: the part of `vec` along `dir` stays,
/// the rest is scaled.
fn scale_except_direction(dir: Vec3, vec: Vec3, scale: f32) -> Vec3 {
    let along = pose_math::dot(dir, vec);
    let direction = scaled(dir, along);
    let rest = Vec3::new(vec.x - direction.x, vec.y - direction.y, vec.z - direction.z);
    scale_add(scale, rest, direction)
}

/// `al::scaleVectorDirection` as the swim calls it: the part of `vec` along `dir` is scaled.
fn scale_direction(dir: Vec3, vec: Vec3, scale: f32) -> Vec3 {
    let along = pose_math::dot(dir, vec);
    let direction = scaled(dir, along);
    let rest = Vec3::new(vec.x - direction.x, vec.y - direction.y, vec.z - direction.z);
    let kept = scaled(direction, scale);
    Vec3::new(rest.x + kept.x, rest.y + kept.y, rest.z + kept.z)
}

/// `al::limitVectorOppositeDir(out, up, dir, scale)` as the swim calls it with the velocity as
/// `dir`: the part of `dir` against `up` is cancelled, up to `scale` of it.
pub(super) fn limit_vector_opposite_dir(dir: Vec3, up: Vec3, scale: f32) -> Vec3 {
    let against = pose_math::dot(dir, up).clamp(-scale, 0.);
    scale_add(-against, up, dir)
}

/// `PlayerActionFunction::isOppositeVec`.
fn is_opposite_vec(a: Vec3, b: Vec3) -> bool {
    let (a, a_ok) = pose_math::try_normalize_or_zero(a);
    if !a_ok {
        return false;
    }
    let (b, b_ok) = pose_math::try_normalize_or_zero(b);
    if !b_ok {
        return false;
    }
    pose_math::dot(b, a) <= OPPOSITE_COS
}

impl Player {
    /// `al::calcSpeedH`: the speed of the velocity without its part along the gravity.
    pub(super) fn swim_speed_h(&self) -> f32 {
        lib::length(lib::verticalize_vec(Vec3::NEG_Y, self.v))
    }

    /// `PlayerInput::calcMoveInput` against the world's up axis: the stick on the plane of the
    /// water, as long as the tilt.
    pub(super) fn swim_stick(&self) -> Vec3 {
        self.plane_stick(UP)
    }

    /// `rs::calcAlongDirFront`: the pose's front laid onto the plane with the normal `UP`, and
    /// whether it has a direction.
    fn along_dir_front(&self) -> (Vec3, bool) {
        let (pose_up, pose_front) = self.pose_axes();
        let along = along_vector_normal_h(pose_front, pose_up, UP);
        pose_math::try_normalize_or_zero(along)
    }

    /// The stroke on the plane (the helper both the surface and the underwater nerves end with):
    /// the horizontal velocity `velocity` brakes, is steered by the pull of the stick and
    /// limited, and is returned.
    pub(super) fn swim_horizontal(&self, velocity: Vec3, stroke: Stroke) -> Vec3 {
        let brake = k::BRAKE_RATE_H;
        if !self.is_move() {
            return scaled(velocity, brake);
        }
        let input = self.swim_stick();
        let (direction, _) = pose_math::try_normalize_or_zero(input);
        let mut v = velocity;
        if is_opposite_vec(direction, v) {
            v = scaled(v, brake);
        } else {
            let (front, has_front) = self.along_dir_front();
            if has_front {
                v = scale_except_direction(front, v, brake);
                if self.tilt * self.tilt <= DEEP_DOWN_SQUARED {
                    v = scale_direction(front, v, lerp_value(1., brake, lib::length(input)));
                }
            }
        }
        let (accel, max_speed) = if stroke.spin_cap {
            (k::SURFACE_ACCEL_H, k::SURFACE_SPIN_CAP_SPEED_MAX_H)
        } else if stroke.surface {
            (k::SURFACE_ACCEL_H, k::SURFACE_SPEED_MAX_H)
        } else if self.is_on_ground_by(self.v) {
            (k::FLOOR_ACCEL_H, k::FLOOR_SPEED_MAX_H)
        } else if stroke.high {
            (k::HIGH_ACCEL_H, k::HIGH_SPEED_MAX_H)
        } else {
            (k::LOW_ACCEL_H, k::LOW_SPEED_MAX_H)
        };
        let push = scaled(input, accel);
        let mut limit = max_speed;
        let speed = lib::length(v);
        if speed > limit {
            let braked = brake * speed;
            if braked >= limit {
                limit = braked;
            }
        }
        v = Vec3::new(push.x + v.x, push.y + v.y, push.z + v.z);
        lib::limit_length(v, limit)
    }

    /// The vertical speed of the surface spring (the helper at 0x48afe8): the speed along the
    /// water's up axis, damped and pulled toward the height the root floats at under the
    /// surface. `moving` is the stick being pushed; `damper` the counter that fades the spring
    /// in.
    fn surface_spring(&self, speed: f32, moving: bool) -> f32 {
        let finder = &self.water_finder;
        let ramp = (self.swim.damper - k::SURFACE_DAMPER_START) as f32 / k::SURFACE_DAMPER_FRAME as f32;
        let ramp = if ramp < 0. { 0. } else if ramp > 1. { 1. } else { ramp };
        if !finder.found {
            return speed;
        }
        let (damped, pull) = if moving {
            let damped = k::SURFACE_MOVE_DAMPER * speed;
            let pull = (finder.distance - k::SURFACE_MOVE_BASE_HEIGHT) * k::SURFACE_MOVE_SPRING;
            (damped, pull)
        } else {
            let rest = 1. - ramp;
            let damped = speed * (ramp + rest * k::SURFACE_DAMPER);
            let pull = rest * ((finder.distance - k::SURFACE_BASE_HEIGHT) * k::SURFACE_SPRING);
            (damped, pull)
        };
        let speed = damped + pull;
        if speed > k::RISE_SPEED_MAX {
            k::RISE_SPEED_MAX
        } else {
            speed
        }
    }

    /// The motion of the surface nerves (the helper at 0x4860cc): the vertical speed follows
    /// the spring, the paddle's rise or the surface gravity; the stroke steers the rest. The
    /// ground normal the nerve keeps (`this+0xd8`) is updated on the way.
    pub(super) fn swim_surface_motion(&mut self, stroke: Stroke) {
        let old_normal = self.swim.ground_normal;
        let (vertical, horizontal) = lib::separate_parallel_vertical(old_normal, self.v);
        let normal = if self.is_collided_ground() {
            self.collided_ground_normal()
        } else {
            UP
        };
        self.swim.ground_normal = normal;
        let horizontal = along_vector_normal_h(horizontal, old_normal, normal);
        let along = pose_math::dot(old_normal, vertical);
        let from_old = scaled(normal, along);
        let along_new = from_old.x * normal.x + from_old.y * normal.y + from_old.z * normal.z;
        let speed = self.surface_spring(along_new, self.is_move());
        let speed = if (k::RISE_FRAME as u32) > self.swim.since_paddle as u32 {
            let rising = k::RISE_POWER + speed;
            if rising > k::RISE_SPEED_MAX {
                k::RISE_SPEED_MAX
            } else {
                rising
            }
        } else {
            let falling = speed - k::SURFACE_GRAVITY;
            if falling < -k::FALL_SPEED_MAX {
                -k::FALL_SPEED_MAX
            } else {
                falling
            }
        };
        let vertical = scaled(normal, speed);
        let horizontal = self.swim_horizontal(horizontal, stroke);
        self.v = Vec3::new(
            horizontal.x + vertical.x,
            horizontal.y + vertical.y,
            horizontal.z + vertical.z,
        );
    }

    /// The body turns toward the stick and its pose follows (the helper at 0x486308).
    /// `rate` scales the turn speed.
    pub(super) fn swim_turn(&mut self, rate: f32) {
        let speed_h = self.swim_speed_h();
        let by_speed = ease_in(rate01(speed_h, k::ROT_SPEED_CHANGE_START, k::LOW_SPEED_MAX_H));
        let by_paddle = rate01(
            self.swim.since_paddle as f32,
            0.,
            k::HIGH_ACCEL_PERMIT_FRAME as f32,
        );
        let speed = lerp_value(k::ROT_SPEED_MAX, k::ROT_SPEED_FORWARD, by_speed * by_paddle) * rate;
        self.swim.turn.setup(TurnSetup {
            min_degrees: k::ROT_START_ANGLE,
            fast_degrees: k::ROT_FAST_ANGLE,
            accel_frames: k::ROT_ACCEL_FRAME,
            accel_frames_fast: k::ROT_ACCEL_FRAME_FAST,
            brake_frames: k::ROT_BRAKE_FRAME,
            limit: speed,
            fast_limit: speed,
        });
        let stick = self.swim_stick();
        let (_, front) = self.pose_axes();
        self.swim.turn.update(front, stick, UP);
        let pose = pose_math::quat_front_up(self.swim.turn.direction, UP);
        self.set_pose(pose);
    }

    /// `rs::scaleVelocityInertiaWallHit`: swimming against a wall squarely (more than 120
    /// degrees from the wall's normal) shortens the horizontal speed to `ratio` of itself, but
    /// not under `min_speed` (or the speed itself, when that is lower) and not over `max_speed`.
    /// (Walls that move, `rs::calcMovePowerWall`, are not ported.)
    fn scale_velocity_inertia_wall_hit(&mut self, ratio: f32, min_speed: f32, max_speed: f32) {
        let Some(wall) = self.collider.wall.as_ref() else {
            return;
        };
        let wall_normal = lib::verticalize_vec(Vec3::NEG_Y, wall.normal);
        let (wall_normal, has_normal) = pose_math::try_normalize_or_zero(wall_normal);
        if !has_normal {
            return;
        }
        let (vertical, mut horizontal) = lib::separate_parallel_vertical(Vec3::NEG_Y, self.v);
        let (heading, _) = pose_math::try_normalize_or_zero(horizontal);
        if pose_math::dot(heading, wall_normal) < -0.5 {
            let length = lib::length(horizontal);
            let mut speed = if length < min_speed { length } else { min_speed };
            let scaled_length = length * ratio;
            if !(scaled_length < speed) {
                speed = scaled_length;
                if scaled_length > max_speed {
                    speed = max_speed;
                }
            }
            let length = lib::length(horizontal);
            if length > 0. {
                horizontal = scaled(horizontal, speed / length);
            }
        }
        self.v = Vec3::new(
            horizontal.x + vertical.x,
            horizontal.y + vertical.y,
            horizontal.z + vertical.z,
        );
    }

    /// The vertical speed under the surface (the helper at 0x48b154): the speed along
    /// `normal` rises while the paddle's rise lasts and falls with `gravity` after, within
    /// the limits. Ground below stops the fall.
    pub(super) fn swim_vertical(&self, vertical: Vec3, normal: Vec3, gravity: f32) -> Vec3 {
        let along = pose_math::dot(vertical, normal);
        let speed = if self.is_collided_ground() && along < 0. {
            0.
        } else {
            along
        };
        // (The trample triggers of the attack sensors, `SwimTrample`, are not ported.)
        let speed = if (k::RISE_FRAME as u32) > self.swim.since_paddle as u32 {
            let rising = speed + k::RISE_POWER;
            if rising > k::RISE_SPEED_MAX {
                k::RISE_SPEED_MAX
            } else {
                rising
            }
        } else {
            let falling = speed - gravity;
            if falling < -k::FALL_SPEED_MAX {
                -k::FALL_SPEED_MAX
            } else {
                falling
            }
        };
        scaled(normal, speed)
    }

    /// The motion of the nerves under the surface (the helper at 0x486834): the velocity is
    /// laid on the ground when the player is on it (and the stroke is the slow one), and
    /// the vertical speed and the stroke follow their rules.
    pub(super) fn swim_underwater_motion(&mut self) {
        let old_normal = self.swim.ground_normal;
        let on_ground = self.is_on_ground_by(self.v);
        let normal = if on_ground {
            self.collided_ground_normal()
        } else {
            UP
        };
        self.swim.ground_normal = normal;
        let (vertical, horizontal, gravity);
        if on_ground {
            let (parallel, rest) = lib::separate_parallel_vertical(old_normal, self.v);
            horizontal = along_vector_normal_h(rest, old_normal, normal);
            vertical = scaled(normal, pose_math::dot(old_normal, parallel));
            gravity = k::GRAVITY_WALK;
        } else {
            self.scale_velocity_inertia_wall_hit(
                0.75,
                k::WALL_HIT_SPEED_MIN_H,
                k::HIGH_SPEED_MAX_H,
            );
            (vertical, horizontal) = lib::separate_parallel_vertical(normal, self.v);
            gravity = k::GRAVITY;
        }
        let vertical = self.swim_vertical(vertical, normal, gravity);
        if std::env::var("PJV_DBG").is_ok() {
            eprintln!("DBG under on_ground {on_ground} old {old_normal:?} new {normal:?} h {horizontal:?} v {vertical:?} stick {:?} tilt {} v_in {:?}", self.swim_stick(), self.tilt, self.v);
        }
        let high = (k::HIGH_ACCEL_PERMIT_FRAME as u32) > self.swim.since_paddle as u32;
        let horizontal = self.swim_horizontal(
            horizontal,
            Stroke {
                high,
                ..Stroke::default()
            },
        );
        self.v = Vec3::new(
            horizontal.x + vertical.x,
            horizontal.y + vertical.y,
            horizontal.z + vertical.z,
        );
    }
}
