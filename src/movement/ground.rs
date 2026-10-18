//! Ground steering of the run: speed, direction and the braking decisions.
//!
//! Written from the original's ground movement controller
//! (`PlayerActionGroundMoveControl::updateNormalMove`, with its helpers
//! `calcAccelRate` and `calcInitBrakeOnCounter`), its turn controller
//! (`PlayerActionTurnControl::update`) and the pivot controller
//! (`PlayerActionPivotTurnControl::update`), as read from the 1.0.0 executable
//! and confirmed against recorded runs. The maths is in the plane of the
//! floor: the rewrite's ground movement is horizontal and leaves slopes to its
//! collision, so the controller's ground normal is always up here. The
//! original's hill speed bonus, wall response and forced-run input have no
//! counterpart yet and are left out.
use super::t;
use bevy::prelude::*;

/// Radians to degrees and back, with the original's constants.
const DEGREES: f32 = 57.295776;
const RADIANS: f32 = 0.017453292;
/// Frames that bring the stick back from a held direction when the stick is
/// pushed over a smaller amount (`mStickOnBrakeFrame`).
const STICK_ON_BRAKE_FRAMES: i32 = 120;
/// The pull the ground move control keeps on the player, in units per frame
/// (`mGravityMove`): what the trace reports as `vy` while running.
pub const GRAVITY_MOVE: f32 = 7.;
/// The speed ramp's average rate relative to a straight line (`mRunAccelAverageScale`).
const RUN_ACCEL_AVERAGE_SCALE: f32 = 0.5;
/// Speed kept after turning round, relative to the speed the brake began at.
pub const RUN_AFTER_TURN_SCALE: f32 = 0.5;
/// The dash border of the run-cycle blend, and its half range; the turn limit
/// narrows around them.
const DASH_BORDER: f32 = 15.;
const DASH_RANGE: f32 = 1.;
/// Turn tuning of the original's run: the smallest angle that turns at all,
/// the angle above which the turn is quick, and the frames of its ramps.
const ROUND_MIN_DEGREES: f32 = 0.5;
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
const STICK_JUMP_COS: f32 = 0.3420201;
/// Opposite directions: the cosine of 95 degrees.
const OPPOSITE_COS: f32 = -0.08715574;
/// Horizontal velocity below which the player counts as standing still.
const STANDING_SPEED: f32 = 0.1;
/// The stick deadzone of the original's move input.
pub const STICK_DEADZONE: f32 = 0.1;

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    (1. - t) * a + t * b
}
/// `al::calcRate01`: where `value` lies between `from` and `to`, 0 to 1.
pub fn rate01(value: f32, from: f32, to: f32) -> f32 {
    let span = to - from;
    if span.abs() < 0.001 {
        return 1.;
    }
    ((value - from) / span).clamp(0., 1.)
}
fn ease_in(x: f32) -> f32 {
    (-0.5 * x + 1.5) * x * x
}
/// `PlayerActionFunction::accel`: a value below its target gains `step` up to
/// it; a value above is left alone.
fn accel(value: f32, target: f32, step: f32) -> f32 {
    if value > target {
        value
    } else {
        (value + step).min(target)
    }
}
/// `PlayerActionFunction::brake`: lose `base / frames` of the magnitude.
fn brake(value: f32, frames: i32, base: f32) -> f32 {
    if value == 0. {
        return 0.;
    }
    let step = base / frames.max(1) as f32;
    if value > 0. {
        (value - step).max(0.)
    } else {
        (value + step).min(0.)
    }
}
/// `al::converge`: move toward the target by at most `step`.
fn converge(value: f32, target: f32, step: f32) -> f32 {
    if value < target {
        (value + step).min(target)
    } else {
        (value - step).max(target)
    }
}
fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0., v.z)
}
/// `al::tryNormalizeOrZero`.
fn unit_or_zero(v: Vec3) -> Vec3 {
    if v.length_squared() < 1.0e-6 {
        Vec3::ZERO
    } else {
        v.normalize()
    }
}
/// `al::isNearZero` on a vector.
fn is_zero(v: Vec3) -> bool {
    v.length_squared() < 1.0e-6
}
fn is_opposite(a: Vec3, b: Vec3) -> bool {
    a.dot(b) <= OPPOSITE_COS
}
/// `al::isReverseDirection`: opposed to within `tolerance` on every axis of
/// their cross product.
fn is_reverse(a: Vec3, b: Vec3, tolerance: f32) -> bool {
    let cross = a.cross(b);
    a.dot(b) < 0. && cross.abs().max_element() <= tolerance
}
fn angle_degrees(a: Vec3, b: Vec3) -> f32 {
    a.cross(b).length().atan2(a.dot(b)) * DEGREES
}
/// Turn `v` about the vertical by `degrees` (positive turns +z toward +x).
fn rotate(v: Vec3, axis: Vec3, degrees: f32) -> Vec3 {
    Quat::from_axis_angle(axis, degrees * RADIANS) * v
}

/// Turn power of a facing that swung by the rate in degrees in a frame, under the
/// standard limit.
pub fn turn_power(rate: f32) -> f32 {
    ease_in(rate01(rate, ROUND_MIN_DEGREES, t::ROUND_LIMIT_DEG))
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

/// The turn controller: how fast the facing swings toward the stick.
#[derive(Clone, Debug, Default)]
pub struct TurnControl {
    tuning: TurnTuning,
    limit: f32,
    fast_limit: f32,
    /// Degrees the facing turns by each frame, ramped up and down.
    rate: f32,
    last_direction: Vec3,
    last_axis: Vec3,
    /// The stick's recent turns, newest last (a ring of five).
    recent: Vec<f32>,
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
    pub fn reset(&mut self) {
        self.rate = 0.;
        self.last_direction = Vec3::ZERO;
        self.last_axis = Vec3::ZERO;
        self.recent.clear();
    }
    /// The average of the stick's last five turns, in degrees per frame.
    fn stick_turn(&self) -> f32 {
        self.recent.iter().sum::<f32>() / 5.
    }
    /// How far into a turn the controller is, 0 to 1 (`calcTurnPowerRate`).
    fn power(&self) -> f32 {
        ease_in(rate01(self.rate, self.tuning.min_degrees, self.limit))
    }
    /// One frame of `PlayerActionTurnControl::update` on a level floor.
    pub fn update(&mut self, facing: Vec3, stick: Vec3) {
        let front = unit_or_zero(flat(facing));
        let new = unit_or_zero(stick);
        let has_input = !is_zero(new);
        let last = unit_or_zero(self.last_direction);
        let have_both = has_input && !is_zero(last);
        let skip_tracking = is_zero(last) || !has_input;
        let front_invalid = is_zero(front);
        // The stick's own turning, averaged over five frames; a jump of more
        // than 70 degrees restarts it.
        if skip_tracking || new.dot(last) < STICK_JUMP_COS {
            self.recent.clear();
        } else {
            if self.recent.len() >= 5 {
                self.recent.remove(0);
            }
            self.recent.push(angle_degrees(last, new).abs());
        }
        let stick_turn = self.stick_turn();
        // A stick pushed the opposite way: the run brakes rather than turns;
        // a forced controller keeps turning and only flags it.
        let opposed = has_input
            && !front_invalid
            && is_opposite(new, front)
            && (skip_tracking || (have_both && stick_turn <= REVERSE_SMOOTH_RATE));
        let reverse = opposed && !self.tuning.forced;
        self.reverse_forced = opposed && self.tuning.forced;
        self.reverse = reverse;
        let turning = has_input && !front_invalid && !reverse;
        let out;
        if turning {
            let angle_to_stick = if have_both {
                last.dot(new)
            } else {
                front.dot(new)
            }
            .clamp(-1., 1.)
            .acos();
            let remaining = front.dot(new).clamp(-1., 1.).acos() * DEGREES;
            let stick_axis = unit_or_zero(last.cross(new));
            let mut extra_turn = false;
            let mut follow_axis = false;
            if have_both && !is_zero(self.last_axis) {
                let stick_degrees = angle_to_stick * DEGREES;
                if stick_axis.dot(self.last_axis) > 0. {
                    // The stick keeps swinging the same way.
                    let face_axis = unit_or_zero(front.cross(new));
                    if !is_zero(face_axis) && face_axis.dot(stick_axis) < 0. {
                        extra_turn = stick_turn > 10. || stick_degrees > 3.;
                        follow_axis = stick_turn <= 10. && stick_degrees <= 3.;
                    }
                } else if stick_degrees >= self.tuning.min_degrees.max(self.rate) {
                    // The stick doubled back by a large step: the turn restarts.
                    self.rate = 0.;
                } else {
                    // A small step the other way eats into the turn.
                    self.rate = converge(self.rate, 0., stick_degrees);
                    follow_axis = stick_degrees < self.tuning.min_degrees;
                }
            }
            // The turn rate ramps up toward what the stick's tilt and the
            // angle left allow, and down when that is reached.
            let tilt = rate01(stick.length(), 0., 0.8).clamp(0.25, 1.);
            if remaining < self.tuning.min_degrees {
                self.rate = brake(self.rate, self.tuning.brake_frames, self.limit);
            } else if remaining < self.tuning.fast_degrees {
                let target = tilt * self.limit;
                self.rate = if self.rate >= target {
                    brake(self.rate, self.tuning.brake_frames, self.fast_limit)
                } else {
                    accel(self.rate, target, target / self.tuning.accel_frames as f32)
                };
            } else {
                self.rate = accel(
                    self.rate,
                    self.fast_limit,
                    self.fast_limit / self.tuning.accel_frames_fast as f32,
                );
            }
            if extra_turn || is_reverse(front, new, 0.01) {
                // Swing about the vertical the way the stick is swinging.
                let axis = if stick_axis.dot(Vec3::Y) > 0. {
                    Vec3::Y
                } else {
                    Vec3::NEG_Y
                };
                out = unit_or_zero(rotate(front, axis, self.rate));
                self.last_axis = axis;
            } else {
                let angle = front.angle_between(new) * DEGREES;
                let swing = self.rate.min(angle);
                let axis = unit_or_zero(front.cross(new));
                let turned = if is_zero(axis) {
                    front
                } else {
                    rotate(front, axis, swing)
                };
                out = unit_or_zero(turned);
                let mut axis = axis;
                if follow_axis && axis.dot(self.last_axis) < 0. {
                    axis = -axis;
                }
                self.last_axis = axis;
            }
            self.last_direction = new;
        } else if has_input {
            // Stick against the facing: keep the facing, stop turning. (With
            // an unusable facing the stick's own direction is taken.)
            out = if reverse { front } else { new };
            if reverse {
                self.rate = 0.;
            }
        } else {
            out = if front_invalid { flat(facing) } else { front };
            self.last_direction = Vec3::ZERO;
            self.last_axis = Vec3::ZERO;
            self.rate = brake(self.rate, self.tuning.brake_frames, self.limit);
        }
        self.direction = out;
    }
}

/// Speed left after a frame of braking on the way the brake began
/// (`rs::moveBrakeRun`): the normal brake over ten frames.
pub fn slide_speed(speed: f32) -> f32 {
    brake(speed, t::BRAKE_FRAMES as i32, t::NORMAL_MAX).max(0.)
}

/// What the ground move control needs of the player each frame.
pub struct SteerInput {
    /// Horizontal velocity, units per frame.
    pub velocity: Vec3,
    /// The facing the player has now.
    pub front: Vec3,
    /// The stick in world space, its length the tilt (zero in the deadzone).
    pub stick: Vec3,
    /// Top speed and ramp frames of this frame's nerve.
    pub max_speed: f32,
    pub accel_frames: i32,
}
/// What one frame of ground movement decided.
#[derive(Clone, Copy, Debug, Default)]
pub struct Steer {
    pub velocity: Vec3,
    pub front: Vec3,
    pub speed: f32,
    /// A fast run asked to brake or reverse: the run hands over to the brake.
    pub brake_run: bool,
    /// The stick pulled against the facing at a crawl: turn on the spot.
    pub pivot: bool,
    /// Speed ran out with the stick released.
    pub stopped: bool,
}

/// The ground move control of the run, or of another state that owns one.
///
/// Every state builds its own control object, and the run's constructor
/// switches on two features the default one lacks (`PlayerStateRunHakoniwa`:
/// the speed ramp of `calcAccelRate` and the sideways drift of
/// `updateNormalMove`). The spin throw's control (`PlayerStateSpinCap`) keeps
/// the plain defaults: a straight `max / frames` acceleration and a velocity
/// that follows the facing at once.
#[derive(Clone, Debug)]
pub struct GroundMove {
    ramp: bool,
    drift: bool,
    pub turn: TurnControl,
    /// Frames the speed has been above the brake speed, up to the border:
    /// only a run that built up speed brakes into a skid.
    pub brake_counter: i32,
    /// The turn power smoothed over frames (`_c0`).
    turn_power: f32,
}
impl Default for GroundMove {
    /// The run's control.
    fn default() -> Self {
        Self {
            ramp: true,
            drift: true,
            turn: TurnControl::default(),
            brake_counter: 0,
            turn_power: 0.,
        }
    }
}
impl GroundMove {
    /// The control of the spin throw's ground state: no ramp, no drift.
    pub fn plain() -> Self {
        Self {
            ramp: false,
            drift: false,
            ..Self::default()
        }
    }
    /// `calcInitBrakeOnCounter` and the turn controller's reset, at the start
    /// of a run state.
    pub fn reset(&mut self, velocity: Vec3, front: Vec3) {
        self.turn.reset();
        self.brake_counter = 0;
        let ground = flat(velocity);
        let speed = ground.length();
        let brake_speed = t::NORMAL_MAX * t::BRAKE_ON_SPEED_RATE;
        if speed >= brake_speed {
            let heading = unit_or_zero(ground);
            let front = unit_or_zero(flat(front));
            if !is_zero(heading) && !is_zero(front) && !is_opposite(front, heading) {
                self.brake_counter = t::BRAKE_ON_COUNTER as i32;
            }
        }
        self.turn_power = 0.;
    }
    /// A run picked up from a recorded row (lockstep): the hidden state the
    /// row hints at.
    pub fn resume(
        &mut self,
        speed: f32,
        steps: u32,
        turn_rate: f32,
        last_input: Vec3,
        turn_power: f32,
    ) {
        let border = t::BRAKE_ON_COUNTER as i32;
        self.brake_counter = if speed > t::NORMAL_MAX * t::BRAKE_ON_SPEED_RATE {
            (steps as i32).min(border)
        } else {
            0
        };
        self.turn.rate = turn_rate;
        self.turn.last_direction = unit_or_zero(last_input);
        self.turn.last_axis = Vec3::ZERO;
        self.turn.recent.clear();
        self.turn_power = turn_power;
    }
    /// `calcAccelRate`: the speed gained this frame. The ramp is a straight
    /// start plus a growing push, so it reaches the top speed in `frames`.
    fn accel_rate(&self, max: f32, frames: i32, speed: f32) -> f32 {
        let n = frames.max(1) as f32;
        if !self.ramp {
            return max / n;
        }
        if max > speed {
            let start = max / n * RUN_ACCEL_AVERAGE_SCALE;
            let left = max - start * n;
            let push = (left + left) / (n * n);
            if push.abs() < 1.0e-9 {
                return start;
            }
            let steps = ((start * start + (push + push) * speed).sqrt() - start) / push + 1.;
            start + push * steps
        } else {
            max / n
        }
    }
    /// One frame of `updateNormalMove` for the run.
    pub fn update(&mut self, input: &SteerInput) -> Steer {
        let mut ground = flat(input.velocity);
        let measured = ground.length();
        if measured < STANDING_SPEED {
            ground = Vec3::ZERO;
        }
        let heading = unit_or_zero(ground);
        // The turn limit narrows toward the dash speed.
        let dash = 1. - rate01(measured, DASH_BORDER - DASH_RANGE, DASH_BORDER + DASH_RANGE);
        let limit = lerp(t::ROUND_LIMIT_DEG_MIN, t::ROUND_LIMIT_DEG, dash);
        self.turn.limit = limit;
        self.turn.fast_limit = limit;
        self.turn.update(input.front, input.stick);
        let wanted = self.turn.direction;
        let stick_dir = unit_or_zero(input.stick);
        let has_input = !is_zero(stick_dir);
        let wanted_zero = is_zero(wanted);
        let along = wanted.dot(heading);
        let opposed = !wanted_zero && has_input && is_opposite(stick_dir, wanted);
        let no_input = !has_input;
        let reversing = self.turn.reverse;
        let braking_request = reversing || no_input;

        let mut speed = measured;
        let mut target = 0.;
        if along >= 0. && !braking_request {
            // Accelerate toward what the stick's tilt allows.
            let tilt = input.stick.length().min(1.);
            target = tilt * input.max_speed + (1. - tilt) * t::NORMAL_MIN;
            let rate = self.accel_rate(input.max_speed, input.accel_frames, speed);
            speed = accel(speed, target, rate);
        }
        // Frames that speed above the brake speed have lasted.
        let border = t::BRAKE_ON_COUNTER as i32;
        if speed > t::NORMAL_MAX * t::BRAKE_ON_SPEED_RATE {
            self.brake_counter = (self.brake_counter + 1).min(border);
        } else if self.brake_counter < border {
            self.brake_counter = 0;
        }
        // How many frames the speed above the target is taken off over: a
        // stick held against the way of travel or released brakes over the
        // brake frames once speed has built up and stops at once otherwise;
        // a stick merely eased off lets go slowly; a stick pointing away from
        // the travel without being opposite stops at once.
        let moving_on = along >= 0.;
        let brake_frames = if !moving_on && opposed {
            t::BRAKE_FRAMES as i32
        } else if !(no_input || moving_on) {
            1
        } else if !(opposed || no_input) {
            STICK_ON_BRAKE_FRAMES
        } else if self.brake_counter >= border {
            t::BRAKE_FRAMES as i32
        } else {
            1
        };
        if speed > target {
            speed = brake(speed, brake_frames, input.max_speed).max(target);
        }

        let mut steer = Steer {
            front: wanted,
            speed,
            ..Steer::default()
        };
        // A reversal or release at speed: run on along the facing and let the
        // brake state take over.
        if braking_request && !(self.brake_counter < border || wanted_zero) {
            steer.velocity = wanted * speed;
            steer.brake_run = true;
            return steer;
        }
        // Turning the other way at a crawl: pivot on the spot.
        if opposed && !(wanted_zero || (along >= 0. && speed > t::NORMAL_MIN)) {
            steer.velocity = heading * speed;
            steer.pivot = true;
            return steer;
        }
        steer.stopped = speed.abs() < 0.001 && !has_input;
        // The velocity swings toward the facing only partly: a run keeps its
        // sideways drift, more of it the less the turn is under way.
        let mut velocity = wanted * speed;
        if self.drift && (heading.length() - 1.).abs() <= 0.001 {
            let power = self.turn.power();
            let smooth = (power - self.turn_power).clamp(0., 1.);
            self.turn_power = lerp(self.turn_power, power, 0.15);
            let side = unit_or_zero(heading - wanted * heading.dot(wanted));
            if !is_zero(side) {
                let keep = lerp(0.3, 0.9, smooth);
                let forward = (speed * wanted.dot(heading)).max(t::NORMAL_MIN);
                let drift = side * heading.dot(side) * speed * keep;
                let mix = wanted * forward + drift;
                let length = mix.length();
                velocity = if length > 0. {
                    mix * (speed / length)
                } else {
                    mix
                };
            }
        } else {
            self.turn_power = 0.;
        }
        steer.velocity = velocity;
        steer
    }
}

/// The pivot on the spot (`PlayerActionPivotTurnControl`): the facing swings
/// toward the stick in steps that grow each frame and snaps to it when close.
#[derive(Clone, Debug, Default)]
pub struct PivotTurn {
    target: Vec3,
    steps: i32,
    /// The stick is still held.
    pub has_input: bool,
    /// The facing reached the stick.
    pub done: bool,
}
/// What a pivot frame did to the player.
pub struct PivotStep {
    pub front: Vec3,
    pub velocity: Vec3,
}
impl PivotTurn {
    pub fn reset(&mut self, front: Vec3) {
        self.target = front;
        self.steps = 0;
        self.has_input = false;
        self.done = false;
    }
    pub fn update(&mut self, stick: Vec3, front: Vec3, velocity: Vec3) -> PivotStep {
        let wanted = unit_or_zero(stick);
        self.has_input = !is_zero(wanted);
        if self.has_input {
            self.target = wanted;
        }
        let target = unit_or_zero(self.target);
        let front = unit_or_zero(flat(front));
        let mut step = PivotStep { front, velocity };
        if is_zero(target) || is_zero(front) {
            self.done = true;
            return step;
        }
        self.steps = (self.steps + 1).min(10);
        let share = self.steps as f32 / 10.;
        let angle = front.cross(target).length().atan2(front.dot(target)) * DEGREES;
        let sign = if front.cross(target).dot(Vec3::Y) < 0. {
            -1.
        } else {
            1.
        };
        let swung = unit_or_zero(rotate(front, Vec3::Y, sign * angle * share));
        let left = swung.cross(target).length().atan2(swung.dot(target));
        if left > 5_f32.to_radians() {
            step.front = swung;
        } else {
            // Close enough: face the stick and carry the speed that way.
            let speed = flat(velocity).length();
            step.front = target;
            step.velocity = target * speed + Vec3::new(0., velocity.y, 0.);
            self.done = true;
        }
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_speed_ramp_starts_gently_and_ends_on_time() {
        // The first frames of a run from a standstill: 0.18375, then 0.37646.
        let ground = GroundMove::default();
        let first = ground.accel_rate(14., 40, 0.);
        assert!((first - 0.18375).abs() < 1.0e-5, "{first}");
        let second = ground.accel_rate(14., 40, first);
        assert!((first + second - 0.376458).abs() < 1.0e-4);
        let mut speed = 0.;
        let mut frames = 0;
        while speed < 14. && frames < 100 {
            speed = accel(speed, 14., ground.accel_rate(14., 40, speed));
            frames += 1;
        }
        assert_eq!(frames, 40);
    }

    #[test]
    fn the_spin_throw_control_gains_a_flat_rate() {
        // `PlayerStateSpinCap` keeps the default control: no ramp, so a speed of
        // 14 over 40 frames is 0.35 a frame from the first (recorded in the
        // cap-bounce run), and the velocity follows the facing at once.
        let mut control = GroundMove::plain();
        control.reset(Vec3::ZERO, Vec3::Z);
        let mut velocity = Vec3::ZERO;
        for frame in 1..=3 {
            let steer = control.update(&SteerInput {
                velocity,
                front: Vec3::Z,
                stick: Vec3::Z,
                max_speed: 14.,
                accel_frames: 40,
            });
            velocity = steer.velocity;
            assert!(
                (velocity.z - 0.35 * frame as f32).abs() < 1.0e-5,
                "{velocity}"
            );
        }
    }

    #[test]
    fn a_stick_reversal_brakes_instead_of_turning() {
        let mut turn = TurnControl {
            limit: 8.5,
            fast_limit: 8.5,
            ..default()
        };
        turn.update(Vec3::Z, Vec3::Z);
        turn.update(Vec3::Z, Vec3::NEG_Z);
        assert!(turn.reverse);
        assert_eq!(turn.direction, Vec3::Z);
    }
}
