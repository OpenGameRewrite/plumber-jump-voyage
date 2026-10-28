//! The turn controller (`PlayerActionTurnControl`): how fast the facing swings toward the stick.
//!
//! `update` is the executable's own routine in its single-precision order, with the library
//! functions it calls (`console_libm.rs`, `pose_math.rs`, `vector_lib.rs`): the facing is read
//! from the actor's pose and carried onto the floor's plane, the stick's own turning is averaged
//! over its last five frames (newest first, as the executable sums them), the turn rate ramps
//! up and down, and the facing is rotated by the quaternion the executable builds. The last bit
//! of the turn rate and of the direction decides the sign of comparisons further on (the speed
//! limit that depends on it, the drift of the velocity), so none of it is simplified.
//!
//! Checked bit for bit against the executable's routine run in a local interpreter
//! (`PJV_TURN_CASES`, see `tests/turn_control.rs`). `update_sky` also enables the ceiling-side
//! branch used by air control (its byte at +0x89).
use super::console_libm::{acosf, cosf, sinf};
use super::joint_math::{calc_rate01, converge, make_quat_rotation_limit};
use super::pose_math::{
    along_vector_normal_h, cross, dot, normalize, rotate_by_quat, squared_length,
    try_normalize_or_zero,
};
use super::vector_lib::{calc_angle_degree, is_near_zero};
use bevy::prelude::*;

/// Radians to degrees and back, with the original's constants.
const DEGREES: f32 = 57.295_776;
const RADIANS: f32 = 0.017_453_292;
/// Turn tuning of the original's run: the smallest angle that turns at all,
/// the angle above which the turn is quick, and the frames of its ramps.
pub const ROUND_MIN_DEGREES: f32 = 0.5;
const ROUND_FAST_DEGREES: f32 = 45.;
const ROUND_ACCEL_FRAMES: i32 = 20;
const ROUND_ACCEL_FRAMES_FAST: i32 = 5;
const ROUND_BRAKE_FRAMES: i32 = 20;
/// The crawl of the squat state sets the same controller up differently
/// (`PlayerStateSquat`'s constructor): quick turns only past 180 degrees, so
/// never, at `SquatWalkTurnSpeed` degrees a frame over `SquatWalkTurnFrame`.
const CRAWL_TURN_DEGREES: f32 = 30.;
const CRAWL_FAST_DEGREES: f32 = 180.;
const CRAWL_ACCEL_FRAMES: i32 = 10;
const CRAWL_ACCEL_FRAMES_FAST: i32 = 1;
const CRAWL_BRAKE_FRAMES: i32 = 10;
/// A reversing stick is only a reversal when it did not sweep round smoothly:
/// the average turn of the stick over five frames, in degrees per frame.
const REVERSE_SMOOTH_RATE: f32 = 1.;
/// A stick direction that jumps more than this much between frames restarts
/// the stick's turn average (cosine of 70 degrees).
const STICK_JUMP_COS: f32 = 0.342_020_15;
/// `PlayerActionFunction::isOppositeDir`: the cosine of 95 degrees.
const OPPOSITE_COS: f32 = -0.087_155_74;
/// `al::isNearZero(vec, 0.001)` on the facing, the stick's last direction and the turn axis.
const NEAR_ZERO: f32 = 0.001;
/// The stick's turns that are averaged (the ring buffer's size).
const STICK_TURN_FRAMES: usize = 5;
/// The axis of the previous turn and a stick that still swings the same way: above this average
/// turn of the stick, or this many degrees in a step, the facing keeps swinging about the
/// vertical.
const SWING_AVERAGE_DEGREES: f32 = 10.;
const SWING_STEP_DEGREES: f32 = 3.;
/// The stick's length at which the turn is quickest, and the least tilt that is scaled.
const TILT_FULL_LENGTH: f32 = 0.8;
const TILT_LEAST: f32 = 0.25;
/// `isReverseDirection` tolerance: a facing this near the opposite of the stick turns about
/// the vertical.
const OPPOSED_AXIS_TOLERANCE: f32 = 0.01;

/// `PlayerActionFunction::accel`: a value below its target gains `step` up to it; a value above
/// is left alone.
fn accel(value: f32, target: f32, step: f32) -> f32 {
    if value > target {
        return value;
    }
    let raised = value + step;
    if raised > target { target } else { raised }
}

/// `PlayerActionFunction::brake`: lose `base / frames` of the magnitude.
pub fn brake(value: f32, frames: i32, base: f32) -> f32 {
    if value == 0. {
        return 0.;
    }
    let step = base / frames as u32 as f32;
    if value > 0. {
        let lowered = value - step;
        if lowered >= 0. { lowered } else { 0. }
    } else {
        let raised = step + value;
        if raised > 0. { 0. } else { raised }
    }
}

/// `PlayerActionFunction::isOppositeDir`.
pub fn is_opposite_dir(a: Vec3, b: Vec3) -> bool {
    dot(a, b) <= OPPOSITE_COS
}

/// `al::isReverseDirection(a, b, tolerance)`: opposed to within `tolerance` on every axis of their
/// cross product.
fn is_reverse_direction(a: Vec3, b: Vec3, tolerance: f32) -> bool {
    let across = cross(a, b);
    dot(a, b) < 0.
        && across.x.abs() <= tolerance
        && across.y.abs() <= tolerance
        && across.z.abs() <= tolerance
}

/// What a turn controller is set up with (`PlayerActionTurnControl::setup`):
/// the ramps of the turn rate and whether a stick pushed the opposite way
/// brakes (the run) or is simply turned towards (the crawl, `_88`).
#[derive(Clone, Copy, Debug)]
struct TurnTuning {
    min_degrees: f32,
    fast_degrees: f32,
    accel_frames: i32,
    accel_frames_fast: i32,
    brake_frames: i32,
    forced: bool,
}
impl Default for TurnTuning {
    fn default() -> Self {
        Self {
            min_degrees: ROUND_MIN_DEGREES,
            fast_degrees: ROUND_FAST_DEGREES,
            accel_frames: ROUND_ACCEL_FRAMES,
            accel_frames_fast: ROUND_ACCEL_FRAMES_FAST,
            brake_frames: ROUND_BRAKE_FRAMES,
            forced: false,
        }
    }
}

/// The arguments of `PlayerActionTurnControl::setup`.
#[derive(Clone, Copy, Debug)]
pub struct TurnSetup {
    pub min_degrees: f32,
    pub fast_degrees: f32,
    pub accel_frames: i32,
    pub accel_frames_fast: i32,
    pub brake_frames: i32,
    pub limit: f32,
    pub fast_limit: f32,
}

/// The facing and floor a turn controller works on.
#[derive(Clone, Copy, Debug)]
pub struct TurnFrame {
    /// The actor's front and up axes (`al::calcFrontDir`, `al::calcUpDir`).
    pub actor_front: Vec3,
    pub actor_up: Vec3,
    /// The stick as a vector of the floor's plane, its length the tilt.
    pub stick: Vec3,
    /// The floor's normal (`mGroundNormal`).
    pub up: Vec3,
}

/// The turn controller: how fast the facing swings toward the stick.
#[derive(Clone, Debug, Default)]
pub struct TurnControl {
    tuning: TurnTuning,
    pub(super) limit: f32,
    pub(super) fast_limit: f32,
    /// Degrees the facing turns by each frame, ramped up and down.
    pub(super) rate: f32,
    last_direction: Vec3,
    last_axis: Vec3,
    /// The ground normal of the last update (`_24`): the plane `last_direction`
    /// lies in.
    last_up: Vec3,
    /// The stick's recent turns, newest last (a ring of five).
    recent: Vec<f32>,
    /// Their average (`_30`), degrees per frame.
    stick_turn: f32,
    /// The facing after the latest update.
    pub direction: Vec3,
    /// The stick asked for the opposite way while this controller was not
    /// forced: the run brakes instead of turning.
    pub reverse: bool,
    /// The same while it was forced (`_69`): the facing keeps turning, and
    /// whoever moves goes the stick's way at once.
    pub reverse_forced: bool,
}
impl TurnControl {
    pub fn setup_force_run(&mut self) {
        self.tuning.forced = true;
        self.setup(TurnSetup {
            min_degrees: 0.5,
            fast_degrees: 45.,
            accel_frames: 20,
            accel_frames_fast: 10,
            brake_frames: 3,
            limit: 4.,
            fast_limit: 4.,
        });
    }
    /// Skate forces the facing through reversals instead of entering Brake.
    pub fn setup_ice(&mut self) {
        self.tuning.forced = true;
        self.setup(TurnSetup {
            min_degrees: 3.,
            fast_degrees: 45.,
            accel_frames: 20,
            accel_frames_fast: 1,
            brake_frames: 30,
            limit: 25.,
            fast_limit: 25.,
        });
    }

    pub fn setup_ground(&mut self, forced: bool) {
        self.tuning = TurnTuning {
            forced,
            ..TurnTuning::default()
        };
    }
    /// The controller the squat state keeps for crawling.
    pub fn crawl() -> Self {
        Self {
            tuning: TurnTuning {
                min_degrees: ROUND_MIN_DEGREES,
                fast_degrees: CRAWL_FAST_DEGREES,
                accel_frames: CRAWL_ACCEL_FRAMES,
                accel_frames_fast: CRAWL_ACCEL_FRAMES_FAST,
                brake_frames: CRAWL_BRAKE_FRAMES,
                forced: true,
            },
            limit: CRAWL_TURN_DEGREES,
            fast_limit: CRAWL_TURN_DEGREES,
            ..Self::default()
        }
    }
    /// The controller the swim state keeps (`PlayerStateSwim`'s constructor): forced like the
    /// crawl's (it keeps turning when the stick pulls the opposite way). Every step sets it up
    /// anew with [`Self::setup`].
    pub fn swim() -> Self {
        Self {
            tuning: TurnTuning {
                forced: true,
                ..TurnTuning::default()
            },
            ..Self::default()
        }
    }
    /// The controller the air control builds (`PlayerActionAirMoveControl`'s constructor): forced
    /// like the swim's, set up with the jump's turn constants (`JumpTurn*`).
    pub fn air() -> Self {
        let mut turn = Self::swim();
        turn.setup(TurnSetup {
            min_degrees: 1.,
            fast_degrees: 135.,
            accel_frames: 20,
            accel_frames_fast: 1,
            brake_frames: 10,
            limit: 6.,
            fast_limit: 25.,
        });
        turn
    }
    /// `PlayerActionTurnControl::setup`: the angles and ramps of the turn, the turn rate it may
    /// reach and the one it may reach for a quick turn.
    pub fn setup(&mut self, setup: TurnSetup) {
        self.tuning = TurnTuning {
            min_degrees: setup.min_degrees,
            fast_degrees: setup.fast_degrees,
            accel_frames: setup.accel_frames,
            accel_frames_fast: setup.accel_frames_fast,
            brake_frames: setup.brake_frames,
            forced: self.tuning.forced,
        };
        self.limit = setup.limit;
        self.fast_limit = setup.fast_limit;
    }
    /// The run's turn limit, which the ground move control sets every frame.
    pub fn set_limit(&mut self, limit: f32) {
        self.limit = limit;
        self.fast_limit = limit;
    }
    /// `PlayerActionTurnControl::reset`.
    pub fn reset(&mut self) {
        self.rate = 0.;
        self.last_direction = Vec3::ZERO;
        self.last_axis = Vec3::ZERO;
        self.last_up = Vec3::ZERO;
        self.recent.clear();
        self.stick_turn = 0.;
    }
    /// A run picked up from a recorded row (lockstep): the turn rate and the stick's last
    /// direction the row hints at.
    pub fn resume(&mut self, rate: f32, last_direction: Vec3) {
        self.rate = rate;
        self.last_direction = last_direction;
        self.last_axis = Vec3::ZERO;
        self.recent.clear();
        self.stick_turn = 0.;
    }
    /// How far into a turn the controller is, 0 to 1 (`calcTurnPowerRate`).
    pub(super) fn power(&self) -> f32 {
        ease_in(calc_rate01(self.rate, self.tuning.min_degrees, self.limit))
    }
    /// `PlayerActionTurnControl::calcTurnPowerRate(up)`: the turn power, negative when the turn's
    /// axis points up the same way as `up`.
    pub fn power_signed(&self, up: Vec3) -> f32 {
        let power = self.power();
        if dot(up, self.last_axis) > 0. {
            -power
        } else {
            power
        }
    }

    /// PlayerActionTurnControl::calcTilt (0x41a6d0): rotate the support up about
    /// the controlled front by the signed eased turn power and the requested tilt angle.
    pub fn tilted_up(&self, up: Vec3, max_degrees: f32) -> Vec3 {
        let half = ((self.power_signed(up) * max_degrees) * RADIANS) * 0.5;
        let (sin, cos) = (sinf(half), cosf(half));
        let axis = self.direction;
        rotate_by_quat(
            up,
            Quat::from_xyzw(axis.x * sin, axis.y * sin, axis.z * sin, cos),
        )
    }
    /// The turn of the stick since its last direction, pushed on the ring of five, and the
    /// average of the ring (summed newest first).
    fn track_stick(&mut self, last: Vec3, new: Vec3, skip_tracking: bool) {
        if skip_tracking || dot(new, last) < STICK_JUMP_COS {
            self.recent.clear();
            self.stick_turn = 0.;
            return;
        }
        if self.recent.len() >= STICK_TURN_FRAMES {
            self.recent.remove(0);
        }
        let degrees = calc_angle_degree(last, new);
        self.recent
            .push(if degrees > 0. { degrees } else { -degrees });
        let mut sum = 0.;
        for turn in self.recent.iter().rev().take(STICK_TURN_FRAMES) {
            sum += turn;
        }
        self.stick_turn = sum / STICK_TURN_FRAMES as f32;
    }

    /// One frame of `PlayerActionTurnControl::update`.
    pub fn update(&mut self, frame: TurnFrame) {
        self.update_frame(frame, false);
    }

    /// Air control enables the ceiling-side branch (`+0x89`); the returned
    /// `+0x6a` flag selects slerpSkyPoseAir instead of slerpUpFront.
    pub fn update_sky(&mut self, frame: TurnFrame) -> bool {
        let ceiling = dot(frame.actor_up, Vec3::Y) < 0.087_155_744;
        self.update_frame(frame, ceiling);
        ceiling
    }

    fn update_frame(&mut self, frame: TurnFrame, ceiling: bool) {
        let TurnFrame {
            actor_front,
            actor_up,
            stick,
            up,
        } = frame;
        // The facing, carried from the actor's own up onto the floor's plane.
        let (front, _) = try_normalize_or_zero(along_vector_normal_h(actor_front, actor_up, up));
        // The stick's last direction, carried from the plane it was stored in onto this one.
        let mut last = Vec3::ZERO;
        if !(is_near_zero(self.last_direction, NEAR_ZERO) && is_near_zero(self.last_up, NEAR_ZERO))
        {
            last =
                try_normalize_or_zero(along_vector_normal_h(self.last_direction, self.last_up, up))
                    .0;
        }
        let (new, has_input) = try_normalize_or_zero(stick);
        let front_invalid = is_near_zero(front, NEAR_ZERO);
        let last_invalid = is_near_zero(last, NEAR_ZERO);
        let have_both = has_input && !last_invalid;
        let skip_tracking = last_invalid || !has_input;
        // The stick's own turning, averaged over five frames; a jump of more
        // than 70 degrees restarts it.
        self.track_stick(last, new, skip_tracking);
        let stick_turn = self.stick_turn;
        // A stick pushed the opposite way: the run brakes rather than turns;
        // a forced controller keeps turning and only flags it.
        let mut reverse = false;
        let mut reverse_forced = false;
        if has_input && !front_invalid && !ceiling && is_opposite_dir(new, front) {
            let smooth = skip_tracking || (have_both && stick_turn <= REVERSE_SMOOTH_RATE);
            if smooth {
                reverse = !self.tuning.forced;
                reverse_forced = self.tuning.forced;
            }
        }
        self.reverse = reverse;
        self.reverse_forced = reverse_forced;
        let turning = has_input && !front_invalid && !ceiling && !reverse;
        let direction;
        if turning {
            let angle_to_stick = acosf(
                (if have_both {
                    dot(last, new)
                } else {
                    dot(front, new)
                })
                .clamp(-1., 1.),
            );
            let remaining_radians = acosf(dot(front, new).clamp(-1., 1.));
            let stick_axis = try_normalize_or_zero(cross(last, new)).0;
            let mut extra_turn = false;
            let mut follow_axis = false;
            if have_both && !is_near_zero(self.last_axis, NEAR_ZERO) {
                let stick_degrees = angle_to_stick * DEGREES;
                if dot(stick_axis, self.last_axis) > 0. {
                    // The stick keeps swinging the same way.
                    let (face_axis, ok) = try_normalize_or_zero(cross(front, new));
                    if ok && dot(face_axis, stick_axis) < 0. {
                        extra_turn = stick_turn > SWING_AVERAGE_DEGREES
                            || stick_degrees > SWING_STEP_DEGREES;
                        follow_axis = stick_turn <= SWING_AVERAGE_DEGREES
                            && stick_degrees <= SWING_STEP_DEGREES;
                    }
                } else {
                    let floor = if self.tuning.min_degrees > self.rate {
                        self.tuning.min_degrees
                    } else {
                        self.rate
                    };
                    if stick_degrees >= floor {
                        // The stick doubled back by a large step: the turn restarts.
                        self.rate = 0.;
                    } else {
                        // A small step the other way eats into the turn.
                        self.rate = converge(self.rate, 0., stick_degrees);
                        follow_axis = stick_degrees < self.tuning.min_degrees;
                    }
                }
            }
            // The turn rate ramps up toward what the stick's tilt and the
            // angle left allow, and down when that is reached.
            let tilt = calc_rate01(squared_length(stick).sqrt(), 0., TILT_FULL_LENGTH);
            let tilt = tilt.clamp(TILT_LEAST, 1.);
            let remaining = remaining_radians * DEGREES;
            if remaining < self.tuning.min_degrees {
                self.rate = brake(self.rate, self.tuning.brake_frames, self.limit);
            } else if remaining >= self.tuning.fast_degrees {
                self.rate = accel(
                    self.rate,
                    self.fast_limit,
                    self.fast_limit / self.tuning.accel_frames_fast as f32,
                );
            } else {
                let target = tilt * self.limit;
                self.rate = if self.rate >= target {
                    brake(self.rate, self.tuning.brake_frames, self.fast_limit)
                } else {
                    accel(self.rate, target, target / self.tuning.accel_frames as f32)
                };
            }
            let axis;
            if extra_turn || is_reverse_direction(front, new, OPPOSED_AXIS_TOLERANCE) {
                // Swing about the vertical the way the stick is swinging.
                let about = if dot(stick_axis, up) > 0. { up } else { -up };
                let half = (self.rate * RADIANS) * 0.5;
                let (cos, sin) = (cosf(half), sinf(half));
                let quat = Quat::from_xyzw(about.x * sin, about.y * sin, about.z * sin, cos);
                direction = try_normalize_or_zero(rotate_by_quat(front, quat)).0;
                axis = about;
            } else {
                let quat = make_quat_rotation_limit(front, new, self.rate * RADIANS);
                direction = normalize(rotate_by_quat(front, quat));
                let across = try_normalize_or_zero(cross(front, new)).0;
                axis = if follow_axis && dot(across, self.last_axis) < 0. {
                    -across
                } else {
                    across
                };
            }
            self.last_axis = axis;
            self.last_direction = new;
            self.last_up = up;
        } else if has_input {
            // Stick against the facing: keep the facing, stop turning. (With
            // an unusable facing the stick's own direction is taken.)
            if reverse {
                direction = front;
                self.rate = 0.;
            } else {
                direction = new;
                if ceiling {
                    self.rate = self.fast_limit;
                }
            }
        } else {
            // No stick: the facing stays (the actor's own, when it cannot be carried onto the
            // floor), the stick's memory is cleared and the turn rate brakes.
            direction = if ceiling {
                self.rate = self.fast_limit;
                if dot(actor_up, Vec3::Y) < 0. {
                    -actor_front
                } else {
                    actor_front
                }
            } else if front_invalid {
                actor_front
            } else {
                front
            };
            self.last_direction = Vec3::ZERO;
            self.last_axis = Vec3::ZERO;
            self.last_up = Vec3::ZERO;
            if !ceiling {
                self.rate = brake(self.rate, self.tuning.brake_frames, self.limit);
            }
        }
        self.direction = direction;
    }
}

/// `al::easeIn`.
pub fn ease_in(x: f32) -> f32 {
    (-0.5 * x + 1.5) * x * x
}

#[path = "../tests/turn_control.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
