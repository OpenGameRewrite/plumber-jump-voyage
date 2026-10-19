//! Air movement of the original player, rewritten from how the 1.0.0
//! executable behaves (traces and a reading of its machine code; no original
//! code is reproduced).
//!
//! The original has one helper object for the jump family (`PlayerAction-
//! AirMoveControl`: take-off velocity, hold-to-extend, gravity, stick
//! acceleration, speed limits) and three movement helpers that the flips, the
//! long jump and the dive call every frame (`rs::moveParallelJump`,
//! `rs::moveDivingJump` with `rs::controlDirectionalVelocity`). This module
//! holds the same arithmetic on plain vectors, in the original's units (a
//! distance unit per frame), with gravity straight down.
//!
//! Positions and facing stay with the caller; these functions only change the
//! velocity (and, for the diving jump, the facing the caller passes in).
use bevy::prelude::*;

/// Gravity points straight down in this game.
const DOWN: Vec3 = Vec3::NEG_Y;

/// The horizontal part of a vector (the ground is always level here).
fn flat(v: Vec3) -> Vec3 {
    v.with_y(0.)
}
/// `al::limitVelocityDir`: the part of `v` along `dir` is kept within `limit`.
fn limit_along(v: &mut Vec3, dir: Vec3, limit: f32) {
    let Some(dir) = dir.try_normalize() else {
        return;
    };
    let along = v.dot(dir);
    if along.abs() > limit {
        *v += dir * (along.signum() * limit - along);
    }
}
/// `al::addVectorLimit`: add `add` to `v` without letting the part of `v`
/// along `add` pass `limit`. Returns whether anything was added.
pub fn add_vector_limit(v: &mut Vec3, add: Vec3, limit: f32) -> bool {
    let Some(dir) = add.try_normalize().filter(|_| add.length_squared() >= 1e-6) else {
        return false;
    };
    let along = v.dot(dir);
    if along >= limit {
        return false;
    }
    *v += dir * (limit - along).min(add.length());
    true
}
/// `al::limitLength`.
fn limit_length(v: Vec3, limit: f32) -> Vec3 {
    if v.length() > limit {
        v.normalize() * limit
    } else {
        v
    }
}
/// `PlayerActionFunction::calcJumpSpeed`: the jump power between its two
/// bounds, from where the horizontal speed sits between its own bounds.
pub fn jump_power(
    speed: f32,
    speed_min: f32,
    speed_max: f32,
    power_min: f32,
    power_max: f32,
) -> f32 {
    if (power_max - power_min).abs() < 0.001 {
        return power_max;
    }
    let along = (speed.clamp(speed_min, speed_max) - speed_min) / (speed_max - speed_min);
    (power_max - power_min) * along.clamp(0., 1.) + power_min
}

/// Values of the player's tuning table (`PlayerConst`) that the original's
/// jump code reads and the rest of this controller has no use for.
pub mod tune {
    /// `JumpBaseSpeedMax`: the horizontal speed a jump keeps at most.
    pub const JUMP_BASE_SPEED_MAX: f32 = 24.;
    /// `ContinuousJumpCount`: jumps in a combo before it starts over.
    pub const CONTINUOUS_JUMP_COUNT: u32 = 3;
    /// `ContinuousLongJumpCount`.
    pub const CONTINUOUS_LONG_JUMP_COUNT: u32 = 3;
    /// A jump must reach this share of the top speed for the third of a combo.
    pub const THIRD_JUMP_SPEED_RATE: f32 = 0.99;
    /// The third jump needs the run within this angle of the last jump's
    /// direction (cosine of 45 degrees).
    pub const THIRD_JUMP_DIRECTION_DOT: f32 = std::f32::consts::FRAC_1_SQRT_2;
    /// `GravityMove`: the pull the ground states keep on the player.
    pub const GRAVITY_MOVE: f32 = 7.;
    /// `SquatJumpMovePowerFront/Side`, `SquatJumpMoveSpeedMax`.
    pub const SQUAT_JUMP_MOVE_POWER: f32 = 0.2;
    /// `TurnJumpAccel`, `TurnJumpBrake`, `TurnJumpSideAccel`.
    pub const TURN_JUMP_ACCEL: f32 = 0.25;
    pub const TURN_JUMP_BRAKE: f32 = 0.5;
    pub const TURN_JUMP_SIDE_ACCEL: f32 = 0.075;
    /// `SlerpQuatGrav`: how fast a diving jump turns toward its travel.
    pub const SLERP_QUAT_GRAV: f32 = 0.4;
    /// `HipDropHeight`: the pound needs this much room above the ground.
    pub const HIP_DROP_HEIGHT: f32 = 40.;
    /// The air control's turning of the facing (`PlayerActionTurnControl`, set
    /// up from the `JumpTurn*` values): degrees a frame, angle the slow turn
    /// starts at, angle beyond which the fast one takes over, their speed
    /// limits and the frames each takes to build up, and to brake.
    pub const TURN_ANGLE_START: f32 = 1.;
    pub const TURN_ANGLE_LIMIT: f32 = 6.;
    pub const TURN_ANGLE_FAST: f32 = 135.;
    pub const TURN_ANGLE_FAST_LIMIT: f32 = 25.;
    pub const TURN_ACCEL_FRAMES: f32 = 20.;
    pub const TURN_ACCEL_FRAMES_FAST: f32 = 1.;
    pub const TURN_BRAKE_FRAMES: f32 = 10.;
}

/// `PlayerActionFunction::accel`: `value` moves toward `target` by `step`,
/// never past it, and is left alone when already above it.
fn accel(value: f32, target: f32, step: f32) -> f32 {
    if value > target {
        value
    } else {
        (value + step).min(target)
    }
}
/// `PlayerActionFunction::brake`: `value` falls toward zero by `limit` over
/// `frames` frames.
fn brake(value: f32, frames: f32, limit: f32) -> f32 {
    if value == 0. {
        return 0.;
    }
    let step = limit / frames;
    if value > 0. {
        (value - step).max(0.)
    } else {
        (value + step).min(0.)
    }
}

/// What the take-off of one jump asks of the air control
/// (`PlayerActionAirMoveControl::setup`).
#[derive(Clone, Copy, Debug)]
pub struct Takeoff {
    /// The horizontal speed the jump may never exceed.
    pub speed_limit: f32,
    /// Added to the inertia the ground gives; with it the speed cap.
    pub base_speed: f32,
    /// Frames in which holding the button keeps gravity off.
    pub extend_frames: u32,
    pub power: f32,
    pub gravity: f32,
    /// Frames at the start in which the stick does nothing.
    pub input_lock: u32,
    /// Range the take-off speed is held to (the jump state limits it to the
    /// base speed of the jump); `None` keeps the speed as it is.
    pub speed_window: Option<(f32, f32)>,
    pub fall_max: f32,
}
/// Stick acceleration of the jump family.
#[derive(Clone, Copy, Debug)]
pub struct Steering {
    pub front: f32,
    pub back: f32,
    pub turn: f32,
}

/// The air control of the jump family: carries the direction the jump started
/// in, applies gravity (skipped while the extend window holds), pushes the
/// velocity with the stick and keeps its parts inside the speed limits.
#[derive(Clone, Debug)]
pub struct AirMove {
    /// Direction of travel, flipped to follow the velocity.
    dir: Vec3,
    /// Across the direction the jump started in.
    side: Vec3,
    speed_cap: f32,
    gravity: f32,
    fall_max: f32,
    extend_frames: u32,
    frames: u32,
    /// The button has been held every frame of the extend window so far.
    holding: bool,
    input_lock: u32,
    /// The stick steered the velocity in the last update (it is ignored in the
    /// frames the take-off locks it), which the turning of the facing follows.
    steered: bool,
    /// Degrees the facing turns by in a frame (`PlayerActionTurnControl::_8`).
    turn_speed: f32,
}
impl Default for AirMove {
    fn default() -> Self {
        Self {
            dir: Vec3::Z,
            side: Vec3::X,
            speed_cap: 0.,
            gravity: 0.,
            fall_max: 0.,
            extend_frames: 0,
            frames: 0,
            holding: false,
            input_lock: 0,
            steered: false,
            turn_speed: 0.,
        }
    }
}
impl AirMove {
    /// Start a jump from velocity `v`: the horizontal part keeps its direction
    /// (or follows `front` when still), the vertical part becomes the power.
    pub fn setup(&mut self, v: &mut Vec3, front: Vec3, t: &Takeoff) {
        self.gravity = t.gravity;
        self.input_lock = t.input_lock;
        self.fall_max = t.fall_max;
        let horizontal = flat(*v);
        self.dir = horizontal
            .try_normalize()
            .or_else(|| flat(front).try_normalize())
            .unwrap_or(Vec3::Z);
        self.side = self.dir.cross(DOWN).normalize_or_zero();
        let speed = horizontal.length();
        let speed = match t.speed_window {
            Some((low, high)) => speed.clamp(low, high),
            None => speed,
        };
        let launch = speed.min(t.speed_limit);
        self.speed_cap = launch.max((t.base_speed).min(t.speed_limit));
        *v = self.dir * launch + Vec3::Y * t.power;
        self.holding = t.extend_frames > 0;
        self.extend_frames = t.extend_frames;
        self.frames = 0;
        self.steered = false;
        self.turn_speed = 0.;
    }
    /// Pick a jump up in the middle (a recorded state being replayed): the
    /// direction and cap are those the take-off would have left, `frames` of
    /// the extend window have passed.
    pub fn resume(&mut self, v: Vec3, front: Vec3, t: &Takeoff, frames: u32, held: bool) {
        let horizontal = flat(v);
        self.gravity = t.gravity;
        self.fall_max = t.fall_max;
        self.input_lock = t.input_lock.saturating_sub(frames);
        self.dir = horizontal
            .try_normalize()
            .or_else(|| flat(front).try_normalize())
            .unwrap_or(Vec3::Z);
        self.side = self.dir.cross(DOWN).normalize_or_zero();
        self.speed_cap = horizontal.length().min(t.speed_limit).max(t.base_speed);
        self.extend_frames = t.extend_frames;
        self.frames = frames.min(t.extend_frames + 1);
        self.holding = held && frames < t.extend_frames;
        self.steered = false;
        self.turn_speed = 0.;
    }
    /// Stop extending the jump (a ceiling was hit).
    pub fn cancel_extend(&mut self) {
        self.extend_frames = 0;
    }
    /// Change the gravity (a cap catch's pop is lighter).
    pub fn set_gravity(&mut self, gravity: f32) {
        self.gravity = gravity;
    }
    /// One frame of air control. `input` is the stick as a world-space push
    /// (length up to 1); `held` whether the jump button is down.
    pub fn update(&mut self, v: &mut Vec3, input: Vec3, held: bool, steer: &Steering) {
        self.frames = (self.frames + 1).min(self.extend_frames + 1);
        if self.holding && self.frames <= self.extend_frames {
            // Inside the window gravity waits, and the button decides whether
            // the window stays open for the next frame.
            self.holding = held;
        } else {
            v.y -= self.gravity;
        }
        self.steered = self.input_lock == 0;
        if self.input_lock == 0 {
            // The direction follows the velocity, whichever way it points. A
            // jump with no horizontal speed keeps the axis it was set up with
            // (the facing) and points it the way the stick pushes: the
            // recorded air throw out of a standing jump with the stick pulled
            // back gains 0.5 along the stick, not the braking 1, and the fall
            // after letting go of a ledge, with the stick pulled back and a
            // little to one side, gains 0.5 along the axis plus a side part of
            // 0.3 of what the stick has across it.
            if flat(*v).length_squared() < 1e-8 {
                if input.dot(self.dir) < 0. {
                    self.dir = -self.dir;
                }
            } else if v.dot(self.dir) < 0. {
                self.dir = -self.dir;
            }
            let along = input.dot(self.dir);
            let accel = if along > 0. || along.abs() < 0.001 {
                steer.front
            } else {
                steer.back
            };
            *v += self.dir * (along * accel);
            *v += (input - self.dir * along) * steer.turn;
        }
        limit_along(v, self.dir, self.speed_cap);
        limit_along(v, self.side, self.speed_cap);
        if v.dot(DOWN) > 0. {
            limit_along(v, DOWN, self.fall_max);
        }
        self.input_lock = self.input_lock.saturating_sub(1);
    }
    /// The facing after this frame's turn (`PlayerActionTurnControl::update`,
    /// which the air control runs after its steering): the stick's direction
    /// is the target, the speed of the turn builds up toward the slow limit
    /// (a fraction of it for a light stick) over 20 frames, jumps to the fast
    /// limit at once when the target is more than 135 degrees away, and
    /// brakes by a tenth of the fast limit a frame while it is above what the
    /// angle allows. Without a stick (or while the take-off locks it) the
    /// facing stays and the speed brakes. Not ported: the turn direction memory
    /// that keeps a swing from reversing (`_18`, `_30`) and the pose tilt.
    pub fn turn_facing(&mut self, front: Vec3, input: Vec3) -> Vec3 {
        let input = if self.steered {
            flat(input)
        } else {
            Vec3::ZERO
        };
        let Some(target) = input.try_normalize() else {
            self.turn_speed = brake(
                self.turn_speed,
                tune::TURN_BRAKE_FRAMES,
                tune::TURN_ANGLE_LIMIT,
            );
            return front;
        };
        let front = flat(front).try_normalize().unwrap_or(target);
        let angle = front.dot(target).clamp(-1., 1.).acos().to_degrees();
        let light_stick = (input.length() / 0.8).clamp(0.25, 1.);
        let slow = tune::TURN_ANGLE_LIMIT * light_stick;
        let speed = self.turn_speed;
        self.turn_speed = if angle < tune::TURN_ANGLE_START {
            brake(speed, tune::TURN_BRAKE_FRAMES, tune::TURN_ANGLE_LIMIT)
        } else if angle >= tune::TURN_ANGLE_FAST {
            let step = tune::TURN_ANGLE_FAST_LIMIT / tune::TURN_ACCEL_FRAMES_FAST;
            accel(speed, tune::TURN_ANGLE_FAST_LIMIT, step)
        } else if speed >= slow {
            brake(speed, tune::TURN_BRAKE_FRAMES, tune::TURN_ANGLE_FAST_LIMIT)
        } else {
            accel(speed, slow, slow / tune::TURN_ACCEL_FRAMES)
        };
        // Rotate toward the target by the speed, never past it.
        let from = front.x.atan2(front.z);
        // A target straight behind has no side to turn to: the swing goes
        // about the downward axis, which lowers the yaw (the original takes the
        // side of the last swing when there was one; not ported).
        let mut delta = if front.dot(target) <= -(0.01f32).cos() {
            -std::f32::consts::PI
        } else {
            let diff = target.x.atan2(target.z) - from;
            (diff + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
        };
        let step = self.turn_speed.to_radians();
        delta = delta.clamp(-step, step);
        let yaw = from + delta;
        Vec3::new(yaw.sin(), 0., yaw.cos())
    }
}

/// Parameters of one frame of the backflip's air control
/// (`rs::moveParallelJump`): the stick pushes along the travel direction and
/// across it, the speed is capped, and gravity pulls within the fall limit.
#[derive(Clone, Copy, Debug)]
pub struct ParallelJump {
    pub front: f32,
    pub back: f32,
    pub side: f32,
    pub speed_max: f32,
    pub gravity: f32,
    pub fall_max: f32,
}
pub fn move_parallel_jump(v: &mut Vec3, input: Vec3, p: &ParallelJump) {
    let mut horizontal = flat(*v);
    let mut vertical = *v - horizontal;
    match horizontal.try_normalize() {
        Some(dir) => {
            let side = dir.cross(DOWN).normalize_or_zero();
            let along = input.dot(dir);
            let accel = if along < 0. { p.back } else { p.front };
            horizontal += dir * (along * accel) + side * (input.dot(side) * p.side);
        }
        None => horizontal += flat(input) * p.front,
    }
    horizontal = limit_length(horizontal, p.speed_max);
    add_vector_limit(&mut vertical, DOWN * p.gravity, p.fall_max);
    *v = horizontal + vertical;
}

/// Parameters of one frame of the diving jump (`rs::moveDivingJump`), used by
/// the side somersault, the long jump and the dive.
#[derive(Clone, Copy, Debug)]
pub struct DivingJump {
    pub accel: f32,
    pub brake: f32,
    pub speed_max: f32,
    pub speed_min: f32,
    pub side_accel: f32,
    pub gravity: f32,
    pub fall_max: f32,
    /// How fast the facing turns toward the travel direction (0 keeps it).
    pub turn_rate: f32,
}
/// `rs::controlDirectionalVelocity`: push the horizontal velocity along the
/// facing (acceleration, or braking against it) within the speed bounds, and
/// across it with the stick.
fn control_directional_velocity(h: &mut Vec3, facing: Vec3, input: Vec3, p: &DivingJump) {
    let side = Vec3::Y.cross(facing).normalize_or_zero();
    let along_input = facing.dot(input);
    let push = along_input * if along_input < 0. { p.brake } else { p.accel };
    let along = facing.dot(*h);
    let rest = *h - facing * along;
    let along = (along + push).clamp(p.speed_min, p.speed_max.max(p.speed_min));
    *h = rest + facing * along;
    if let Some(stick) = input.try_normalize() {
        *h += side * (side.dot(stick) * p.side_accel);
    }
}
pub fn move_diving_jump(v: &mut Vec3, facing: &mut Vec3, input: Vec3, p: &DivingJump) {
    let mut horizontal = flat(*v);
    control_directional_velocity(&mut horizontal, *facing, flat(input), p);
    let mut vertical = *v - flat(*v);
    add_vector_limit(&mut vertical, DOWN * p.gravity, p.fall_max);
    *v = horizontal + vertical;
    if p.turn_rate > 0. {
        let target = horizontal.try_normalize().unwrap_or(*facing);
        *facing = turn_toward(*facing, target, p.turn_rate);
    }
}
/// Turn a flat direction toward another by a fraction of the angle between
/// them (`rs::slerpUpFront` with the actor held upright).
pub fn turn_toward(from: Vec3, to: Vec3, rate: f32) -> Vec3 {
    let from_yaw = from.x.atan2(from.z);
    let mut delta = to.x.atan2(to.z) - from_yaw;
    delta = (delta + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    let yaw = from_yaw + delta * rate;
    Vec3::new(yaw.sin(), 0., yaw.cos())
}

/// Take-off of the long jump (`rs::setupLongJumpVelocity`): the horizontal
/// speed becomes the old speed plus a push, held between the minimum and the
/// initial speed; an upward speed already there is kept; the jump power is
/// added and the whole is held to the length the initial speed and power make.
pub struct LongJumpTakeoff {
    pub move_pow: f32,
    pub speed_min: f32,
    pub init_speed: f32,
    pub power: f32,
}
pub fn long_jump_velocity(v: Vec3, t: &LongJumpTakeoff) -> Vec3 {
    let horizontal = flat(v);
    let speed =
        (horizontal.length() + t.move_pow).clamp(t.speed_min, t.init_speed.max(t.speed_min));
    // A player standing still has no direction to scale, as in the original.
    let along = horizontal
        .try_normalize()
        .map_or(Vec3::ZERO, |dir| dir * speed);
    // Falling speed is dropped, rising speed is kept.
    let rising = if v.y > 0. { Vec3::Y * v.y } else { Vec3::ZERO };
    limit_length(
        along + rising + Vec3::Y * t.power,
        t.init_speed.hypot(t.power),
    )
}

/// `PlayerContinuousJump`: the combo of consecutive jumps. A jump that ends
/// on the ground counts up; the timer then runs while the player stands or
/// runs, and the combo is gone when it expires or any other state is entered.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContinuousJump {
    pub count: u32,
    pub timer: u32,
    last_dir: Vec3,
}
impl ContinuousJump {
    /// `update(shouldCountDown)`: once per frame outside the jump state.
    pub fn update(&mut self, counting: bool, window: u32) {
        if !counting {
            self.clear(window);
        } else if self.count > 0 {
            self.timer += 1;
            if self.timer >= window {
                self.clear(window);
            }
        }
    }
    pub fn clear(&mut self, window: u32) {
        self.last_dir = Vec3::ZERO;
        self.count = 0;
        self.timer = window;
    }
    /// A jump ended on the ground travelling along `dir`.
    pub fn count_up(&mut self, dir: Vec3) {
        self.last_dir = dir;
        self.count = (self.count + 1) % tune::CONTINUOUS_JUMP_COUNT;
        self.timer = 0;
    }
    /// Direction of the last counted jump.
    pub fn last_dir(&self) -> Vec3 {
        self.last_dir
    }
}

/// `PlayerContinuousLongJump`: counts long jumps (which pick the clip) and
/// forgets them a while after landing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContinuousLongJump {
    pub count: u32,
    pub timer: u32,
}
impl ContinuousLongJump {
    pub fn count_up(&mut self) {
        self.count = (self.count + 1) % tune::CONTINUOUS_LONG_JUMP_COUNT;
        self.timer = 0;
    }
    /// Once per frame outside the long jump.
    pub fn update(&mut self, window: u32) {
        if self.count != 0 {
            self.timer += 1;
            if self.timer >= window {
                self.count = 0;
                self.timer = 0;
            }
        }
    }
}

#[path = "tests/air_move.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
