//! Ground steering of the run: speed, direction and the braking decisions.
//!
//! Written from the original's ground movement controller
//! (`PlayerActionGroundMoveControl::updateNormalMove`, with its helpers
//! `calcAccelRate` and `calcInitBrakeOnCounter`), its turn controller
//! (`PlayerActionTurnControl::update`) and the pivot controller
//! (`PlayerActionPivotTurnControl::update`), as read from the 1.0.0 executable
//! and confirmed against recorded runs. The maths is in the plane of the
//! floor under the player (`mGroundNormal`, which is straight up on level
//! ground): velocities, the facing and the stick are vectors of that plane.
//! The original's forced-run input has no counterpart yet and is left out.
use super::plane::verticalize;
use super::t;
pub use super::turn_control::{ROUND_MIN_DEGREES, TurnControl, TurnFrame, TurnSetup};
use bevy::prelude::*;

/// Radians to degrees and back, with the original's constants.
const DEGREES: f32 = 57.295776;
const RADIANS: f32 = 0.017453292;
/// Frames that bring the stick back from a held direction when the stick is
/// pushed over a smaller amount (`mStickOnBrakeFrame`).
const STICK_ON_BRAKE_FRAMES: i32 = 120;
/// `getHillAccelAddFrame` / `getHillAccelSubFrame` / `getHillAccelSubAngle{Min,Max}`
/// (the hill factor eases toward its target over these) and `getHillAddSpeed` /
/// `getHillSubSpeed` / `getStandAngleMin` (what it adds to the top speed, and
/// the slope at which it is full).
const HILL_ACCEL_ADD_FRAMES: i32 = 60;
const HILL_ACCEL_SUB_FRAMES: i32 = 100;
const HILL_ACCEL_SUB_ANGLE_MIN: f32 = 0.;
const HILL_ACCEL_SUB_ANGLE_MAX: f32 = 20.;
const HILL_ADD_SPEED: f32 = 15.;
const HILL_SUB_SPEED: f32 = 0.;
const STAND_ANGLE_MIN: f32 = 60.;
/// Moving into a wall this squarely (cosine of 100 degrees against the wall's
/// normal) takes the speed bonus away and brakes in one frame.
const WALL_INTO_COS: f32 = -0.173_648_18;
/// `0.9`, the share of the pull that counts against a wall's upward push.
const WALL_PULL_SHARE: f32 = 0.9;
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
/// Opposite directions: the cosine of 95 degrees.
const OPPOSITE_COS: f32 = -0.087_155_74;
/// `al::isNearZero(x, 0.001)` on a speed.
const NEAR_ZERO_SPEED: f32 = 0.001;
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
pub fn ease_in(x: f32) -> f32 {
    (-0.5 * x + 1.5) * x * x
}
fn ease_out(x: f32) -> f32 {
    (-0.5 * x * x + 1.5) * x
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
/// Turn `v` about the vertical by `degrees` (positive turns +z toward +x).
fn rotate(v: Vec3, axis: Vec3, degrees: f32) -> Vec3 {
    Quat::from_axis_angle(axis, degrees * RADIANS) * v
}

/// Turn power of a facing that swung by the rate in degrees in a frame, under the
/// standard limit.
pub fn turn_power(rate: f32) -> f32 {
    ease_in(rate01(rate, ROUND_MIN_DEGREES, t::ROUND_LIMIT_DEG))
}

/// Speed left after a frame of braking on the way the brake began
/// (`rs::moveBrakeRun`): the normal brake over ten frames.
pub fn slide_speed(speed: f32) -> f32 {
    brake(speed, t::BRAKE_FRAMES as i32, t::NORMAL_MAX).max(0.)
}

/// What the ground move control needs of the player each frame.
pub struct SteerInput {
    /// The velocity in the plane of the floor (`updateNormalAndSnap`), units
    /// per frame.
    pub velocity: Vec3,
    /// The normal of that floor (`mGroundNormal`).
    pub up: Vec3,
    /// The actor's front and up axes (`al::calcFrontDir`, `al::calcUpDir`): the facing the
    /// player has now, in the pose it was last given.
    pub actor_front: Vec3,
    pub actor_up: Vec3,
    /// The stick in the plane of the floor, its length the tilt (zero in the
    /// deadzone).
    pub stick: Vec3,
    /// The wall the body was pressed into the frame before: its normal, and
    /// whether the contact was on the wall's face rather than its edge.
    pub wall: Option<(Vec3, bool)>,
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
    /// Whether the slope speeds the run up and slows it down (`_78`, which the
    /// run's constructor switches on) and the factor it keeps (`_7c`): above
    /// zero downhill, below it uphill.
    hills: bool,
    hill: f32,
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
            hills: true,
            hill: 0.,
        }
    }
}
impl GroundMove {
    /// The control of the spin throw's ground state: no ramp, no drift.
    pub fn plain() -> Self {
        Self {
            ramp: false,
            drift: false,
            hills: false,
            ..Self::default()
        }
    }
    /// `calcInitBrakeOnCounter` and the turn controller's reset, at the start
    /// of a run state.
    pub fn reset(&mut self, velocity: Vec3, front: Vec3) {
        self.turn.reset();
        self.brake_counter = 0;
        self.hill = 0.;
        let speed = velocity.length();
        let brake_speed = t::NORMAL_MAX * t::BRAKE_ON_SPEED_RATE;
        if speed >= brake_speed {
            let heading = unit_or_zero(velocity);
            let front = unit_or_zero(front);
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
        self.turn.resume(turn_rate, unit_or_zero(last_input));
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
    /// `updateHillAffect`: the hill factor eases toward what the slope asks
    /// of this frame: downhill along the way the player wants to go it
    /// grows with the slope and how directly it runs down it, in any other
    /// case it is a penalty that grows with the slope; it leaves its old
    /// value slowly when it has to fall and quickly when it has to rise.
    fn update_hill(&mut self, up: Vec3, wanted: Vec3, has_input: bool) {
        if !self.hills {
            return;
        }
        let slope = up.y.clamp(-1., 1.).acos();
        // The way down in the floor's plane: gravity with its part along the normal taken off
        // (`al::verticalizeVec(out, normal, gravity)`).
        let downhill = unit_or_zero(verticalize(up, Vec3::NEG_Y));
        let (mut target, mut tilt_in_degrees) = (0., 0.);
        let mut rising = false;
        if !is_zero(downhill) {
            let degrees = slope * DEGREES;
            let rate = (degrees / STAND_ANGLE_MIN).clamp(0., 1.);
            let along = if has_input { downhill.dot(wanted) } else { 0. };
            if has_input && along > 0. {
                target = along * ease_out(rate);
            } else {
                target = -ease_in(rate);
            }
            tilt_in_degrees = (degrees * along).max(0.);
            rising = target > 0.;
            if rising && self.hill < 0. {
                self.hill = 0.;
            }
        }
        let mut frames = HILL_ACCEL_ADD_FRAMES;
        if self.hill > target {
            let share = (target + 1.).clamp(0., 1.);
            let wanted_frames = share * (HILL_ACCEL_SUB_FRAMES - 1) as f32;
            frames = wanted_frames.ceil() as i32 + 1;
            if rising {
                let angle = rate01(
                    tilt_in_degrees,
                    HILL_ACCEL_SUB_ANGLE_MIN,
                    HILL_ACCEL_SUB_ANGLE_MAX,
                );
                target = lerp(target, self.hill, ease_out(ease_out(angle)));
            }
        }
        self.hill = converge(self.hill, target, 1. / frames as f32);
    }
    /// `PlayerActionGroundMoveControl::calcTurnTiltRate`: how far into a turn the control is,
    /// signed by the side it turns to (the joint controllers lean the body by it).
    pub fn turn_tilt_rate(&self, up: Vec3) -> f32 {
        self.turn.power_signed(up)
    }
    /// What the hill factor adds to the top speed (`calcMaxSpeed`).
    fn hill_speed(&self) -> f32 {
        if !self.hills {
            0.
        } else if self.hill > 0. {
            self.hill * HILL_ADD_SPEED
        } else {
            -(self.hill * HILL_SUB_SPEED)
        }
    }
    /// One frame of `updateNormalMove` for the run.
    pub fn update(&mut self, input: &SteerInput) -> Steer {
        let up = input.up;
        let mut ground = input.velocity;
        let measured = ground.length();
        if measured < STANDING_SPEED {
            ground = Vec3::ZERO;
        }
        let heading = unit_or_zero(ground);
        // The turn limit narrows toward the dash speed.
        let dash = 1. - rate01(measured, DASH_BORDER - DASH_RANGE, DASH_BORDER + DASH_RANGE);
        let limit = lerp(t::ROUND_LIMIT_DEG_MIN, t::ROUND_LIMIT_DEG, dash);
        self.turn.set_limit(limit);
        self.turn.update(TurnFrame {
            actor_front: input.actor_front,
            actor_up: input.actor_up,
            stick: input.stick,
            up,
        });
        let wanted = self.turn.direction;
        let stick_dir = unit_or_zero(input.stick);
        let has_input = !is_zero(stick_dir);
        let wanted_zero = is_zero(wanted);
        let along = wanted.dot(heading);
        let opposed = !wanted_zero && has_input && is_opposite(stick_dir, wanted);
        let no_input = !has_input;
        let reversing = self.turn.reverse;
        let braking_request = reversing || no_input;
        self.update_hill(up, wanted, has_input);
        // Running into a wall squarely: no bonus from the slope, and the
        // brake takes everything in one frame.
        let into_wall = input
            .wall
            .is_some_and(|(normal, _)| normal.dot(heading) < WALL_INTO_COS);

        let mut speed = measured;
        let mut target = 0.;
        if along >= 0. && !braking_request {
            // Accelerate toward what the stick's tilt allows.
            let tilt = input.stick.length().min(1.);
            let top = input.max_speed + self.hill_speed();
            target = tilt * top + (1. - tilt) * t::NORMAL_MIN;
            if into_wall {
                target = target.min(input.max_speed);
            }
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
        let brake_frames = if into_wall {
            1
        } else if !moving_on && opposed {
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
        // Running up a wall's face is cut off (`0x418aec`).
        steer.velocity = match input.wall {
            Some((normal, true)) => cut_climb(velocity, -up * GRAVITY_MOVE, normal),
            _ => velocity,
        };
        steer
    }
}

/// The function `updateNormalMove` calls last (`0x418aec`): against the face of
/// a wall, the part of the velocity that climbs along the wall, less nine tenths
/// of the pull holding the player down, is taken off.
fn cut_climb(velocity: Vec3, pull: Vec3, wall: Vec3) -> Vec3 {
    let along_wall = verticalize(wall, velocity);
    let climb = along_wall.y + WALL_PULL_SHARE * pull.y;
    if climb >= 0. || climb.abs() < NEAR_ZERO_SPEED {
        velocity - Vec3::Y * climb
    } else {
        velocity
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
                up: Vec3::Y,
                wall: None,
                actor_front: Vec3::Z,
                actor_up: Vec3::Y,
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
        let mut turn = TurnControl::default();
        turn.set_limit(8.5);
        let frame = |stick| TurnFrame {
            actor_front: Vec3::Z,
            actor_up: Vec3::Y,
            stick,
            up: Vec3::Y,
        };
        turn.update(frame(Vec3::Z));
        turn.update(frame(Vec3::NEG_Z));
        assert!(turn.reverse);
        assert_eq!(turn.direction, Vec3::Z);
    }
}
