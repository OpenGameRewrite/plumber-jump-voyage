//! The ground move control (`PlayerActionGroundMoveControl`): the run's speed, the velocity it
//! sets, the braking decisions and the pose it asks for, one frame of `updateNormalMove` with
//! its helpers `calcAccelRate`, `updateHillAffect`, `calcInitBrakeOnCounter` and the wall rule
//! (`0x418aec`).
//!
//! The arithmetic is the executable's own, in its single-precision order (`console_libm.rs`
//! for the library functions, `pose_math.rs` and `vector_lib.rs` for the vector routines): the
//! last bit of the speed and of the direction decides on which side of a comparison the turn
//! limit falls, and the velocity it sets is what the collision moves the body by. The maths is in
//! the plane of the floor under the player (`mGroundNormal`, which is straight up on level
//! ground): velocities, the facing and the stick are vectors of that plane.
//!
//! RocketFlower supplies the forced-run input (`_9c`) and the enabled surface
//! judge. The 2D area's hold remains outside this controller's recorded coverage.
//!
//! Checked bit for bit against the executable's routine run in a local interpreter
//! (`PJV_MOVE_CASES`, see `tests/ground_move.rs`).
use super::console_libm::acosf;
use super::ground::GRAVITY_MOVE;
use super::joint_math::{calc_rate01, converge, ease_out, lerp_value};
use super::pose_math::{dot, squared_length, try_normalize_or_zero};
use super::t;
use super::turn_control::{TurnControl, TurnFrame, brake, ease_in, is_opposite_dir};
use super::vector_lib::{is_near_zero, is_near_zero_value, verticalize_vec};
use bevy::prelude::*;

/// Radians to degrees, with the original's constant.
const DEGREES: f32 = 57.295_776;
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
/// The speed ramp's average rate relative to a straight line (`mRunAccelAverageScale`).
const RUN_ACCEL_AVERAGE_SCALE: f32 = 0.5;
/// The dash border of the run-cycle blend, and its half range; the turn limit
/// narrows around them.
const DASH_BORDER: f32 = 15.;
const DASH_RANGE: f32 = 1.;
/// `al::isNearZero(x, 0.001)` on a speed.
const NEAR_ZERO_SPEED: f32 = 0.001;
/// Horizontal velocity below which the player counts as standing still.
const STANDING_SPEED: f32 = 0.1;
/// How far from one the heading's length may be for the drift (`al::isNormalize`).
const NORMALIZED_TOLERANCE: f32 = 0.001;
/// The share the turn power's smoothing keeps and the drift's sideways share at the two ends of
/// the smoothed turn.
const TURN_POWER_FOLLOW: f32 = 0.15;
const DRIFT_KEEP_LEAST: f32 = 0.3;
const DRIFT_KEEP_MOST: f32 = 0.9;
/// The speed a stick held to its end is eased toward when it asks for little, and the least
/// forward speed of the drift (`mMinSpeed`).
const NORMAL_MIN: f32 = t::NORMAL_MIN;

/// What the ground move control needs of the player each frame.
pub struct SteerInput {
    /// The contacted FloorCode is Skate and the velocity is not leaving it.
    pub skate: bool,
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
    pub slip_ice: bool,
    /// The velocity in the plane of the floor, before the pull into the floor.
    pub velocity: Vec3,
    /// The velocity the control sets on the actor (`al::setVelocity`): `velocity` less the pull
    /// along the floor's normal.
    pub set_velocity: Vec3,
    /// The pose the control asks for (`updatePoseUpFront(speed, up, front)`).
    pub up: Vec3,
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
/// Every state builds its own control object. The run's constructor switches on the speed ramp
/// of `calcAccelRate` and the sideways drift of `updateNormalMove`; the spin throw's control
/// (`PlayerStateSpinCap`) keeps the plain defaults, a straight `max / frames` acceleration and a
/// velocity along the facing, but both let the slope speed them up and slow them down.
#[derive(Clone, Debug)]
pub struct GroundMove {
    pub force_speed: Option<f32>,
    pub surface_run: bool,
    ramp: bool,
    drift: bool,
    pub turn: TurnControl,
    /// Frames the speed has been above the brake speed, up to the border:
    /// only a run that built up speed brakes into a skid.
    pub brake_counter: i32,
    /// The turn power smoothed over frames (`_c0`).
    turn_power: f32,
    /// The factor the slope gives the speed (`_7c`): above zero downhill, below it uphill.
    hill: f32,
    /// Frames of speed the brake counter needs before a release brakes over the brake frames
    /// (`mCounterBorder`, the last argument of `setup`).
    border: i32,
    min_speed: f32,
    brake_speed: f32,
    brake_frames: i32,
    stick_brake_frames: i32,
    movement: Vec3,
}
impl Default for GroundMove {
    /// The run's control.
    fn default() -> Self {
        Self {
            force_speed: None,
            surface_run: false,
            ramp: true,
            drift: true,
            turn: TurnControl::default(),
            brake_counter: 0,
            turn_power: 0.,
            hill: 0.,
            border: t::BRAKE_ON_COUNTER as i32,
            min_speed: NORMAL_MIN,
            brake_speed: t::NORMAL_MAX * t::BRAKE_ON_SPEED_RATE,
            brake_frames: t::BRAKE_FRAMES as i32,
            stick_brake_frames: STICK_ON_BRAKE_FRAMES,
            movement: Vec3::ZERO,
        }
    }
}
impl GroundMove {
    /// Sand's setup: ordinary speed/brake constants, no run ramp or drift.
    pub fn sand() -> Self {
        Self {
            ramp: false,
            drift: false,
            ..Self::default()
        }
    }
    /// DamageFire's own setup (0x4686b0..0x468714). The constructor forces the
    /// turn control and keeps plain acceleration/no run-only drift.
    pub fn fire() -> Self {
        Self {
            ramp: false,
            drift: false,
            turn: TurnControl::swim(),
            border: 10000,
            min_speed: 18.,
            brake_speed: 10000.,
            brake_frames: 9999,
            stick_brake_frames: 9999,
            ..Self::default()
        }
    }
    /// The direction stored at +0x90, distinct from the support normal.
    pub fn movement_direction(&self) -> Vec3 {
        self.movement
    }

    /// The control of the spin throw's ground state: no ramp, no drift, and a counter border of 0
    /// (`PlayerStateSpinCap` passes 0 to `setup`), so a released stick always brakes over the
    /// brake frames, whatever speed the control has built up (the run's border is 10 frames).
    pub fn plain() -> Self {
        Self {
            ramp: false,
            drift: false,
            border: 0,
            ..Self::default()
        }
    }
    /// `calcInitBrakeOnCounter` and the turn controller's reset, at the start of a run state.
    /// `velocity` is the velocity in the floor's plane, `actor_front` the actor's front, `up`
    /// the floor's normal.
    pub fn reset(&mut self, velocity: Vec3, actor_front: Vec3, up: Vec3) {
        self.turn.reset();
        self.brake_counter = 0;
        self.hill = 0.;
        let speed = squared_length(velocity).sqrt();
        if speed >= self.brake_speed {
            let (heading, moving) = try_normalize_or_zero(velocity);
            let (front, facing) = try_normalize_or_zero(verticalize_vec(up, actor_front));
            if moving && facing && !is_opposite_dir(front, heading) {
                self.brake_counter = self.border;
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
        skate: bool,
    ) {
        let border = self.border;
        self.brake_counter = if skate {
            // Keep the controller's hidden hold: appear may already fill it
            // when a fast roll/landing chooses Run, regardless of clip age.
            self.brake_counter.max((steps as i32).min(border))
        } else if speed > self.brake_speed {
            (steps as i32).min(border)
        } else {
            0
        };
        if !skate {
            self.turn
                .resume(turn_rate, try_normalize_or_zero(last_input).0);
            self.turn_power = turn_power;
        } else {
            // Its turn rate/history are hidden state, retained just like the
            // hold. An angle inferred from the printed actor front can cross
            // the strict rate/limit comparison even when the turn was exact.
            self.turn_power = 0.;
        }
    }
    /// `calcAccelRate`: the speed gained this frame. The ramp is a straight
    /// start plus a growing push, so it reaches the top speed in `frames`.
    pub(super) fn accel_rate(&self, max: f32, frames: i32, speed: f32) -> f32 {
        if !(self.ramp && max > speed) {
            return max / frames as f32;
        }
        let n = frames as f32;
        let start = max / n * RUN_ACCEL_AVERAGE_SCALE;
        let left = max - start * n;
        let push = (left + left) / (frames.wrapping_mul(frames) as f32);
        let reach = ((start * start + (push + push) * speed).sqrt() - start) / push + 1.;
        start + push * reach
    }
    /// `updateHillAffect`: the hill factor eases toward what the slope asks
    /// of this frame: downhill along the way the player wants to go it
    /// grows with the slope and how directly it runs down it, in any other
    /// case it is a penalty that grows with the slope; it leaves its old
    /// value slowly when it has to fall and quickly when it has to rise.
    fn update_hill(&mut self, up: Vec3, wanted: Vec3, has_input: bool) {
        // The gravity is straight down: the slope is the angle between the normal and up.
        let cosine = (up.y - 0.0 * up.x) - 0.0 * up.z;
        let slope = acosf(cosine.clamp(-1., 1.));
        // The way down in the floor's plane: gravity with its part along the normal taken off
        // (`al::verticalizeVec(out, normal, gravity)`).
        let (downhill, has_downhill) =
            try_normalize_or_zero(verticalize_vec(up, Vec3::new(0., -1., 0.)));
        let (mut target, mut tilt_in_degrees) = (0., 0.);
        let mut rising = false;
        if has_downhill {
            let degrees = slope * DEGREES;
            let rate = (degrees / STAND_ANGLE_MIN).clamp(0., 1.);
            let along = if has_input { dot(downhill, wanted) } else { 0. };
            if has_input && along > 0. {
                target = along * ease_out(rate) + 0.;
            } else {
                target = 0. - ease_in(rate);
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
            // (`ceil`, written as the executable does it.)
            let whole = wanted_frames as i32;
            let up_one = i32::from(wanted_frames >= 0. && whole as f32 != wanted_frames);
            frames = whole + up_one + 1;
            if rising {
                let angle = calc_rate01(
                    tilt_in_degrees,
                    HILL_ACCEL_SUB_ANGLE_MIN,
                    HILL_ACCEL_SUB_ANGLE_MAX,
                );
                target = lerp_value(target, self.hill, ease_out(ease_out(angle)));
            }
        }
        self.hill = converge(self.hill, target, 1. / frames as f32);
    }
    /// What the hill factor adds to the top speed.
    fn hill_speed(&self) -> f32 {
        if self.hill > 0. {
            self.hill * HILL_ADD_SPEED
        } else {
            -(self.hill * HILL_SUB_SPEED)
        }
    }
    /// `PlayerActionGroundMoveControl::calcTurnTiltRate`: how far into a turn the control is,
    /// signed by the side it turns to (the joint controllers lean the body by it).
    pub fn turn_tilt_rate(&self, up: Vec3) -> f32 {
        self.turn.power_signed(up)
    }
    /// One frame of `updateNormalMove` for the run.
    pub fn update(&mut self, input: &SteerInput) -> Steer {
        if input.skate {
            return self.update_skate(input);
        }
        self.turn.setup_ground(self.border == 10000);
        let up = input.up;
        let mut ground = input.velocity;
        let measured = squared_length(ground).sqrt();
        if measured < STANDING_SPEED {
            ground = Vec3::ZERO;
        }
        let heading = try_normalize_or_zero(ground).0;
        // The turn limit narrows toward the dash speed.
        let dash = (1. - calc_rate01(measured, DASH_BORDER - DASH_RANGE, DASH_BORDER + DASH_RANGE))
            .clamp(0., 1.);
        self.turn
            .set_limit(lerp_value(t::ROUND_LIMIT_DEG_MIN, t::ROUND_LIMIT_DEG, dash));
        if self.force_speed.is_some() {
            self.turn.setup_force_run();
        }
        self.turn.update(TurnFrame {
            actor_front: input.actor_front,
            actor_up: input.actor_up,
            stick: input.stick,
            up,
        });
        let (mut stick_dir, mut has_input) = try_normalize_or_zero(input.stick);
        // The turn control decides the facing, which is also the way the velocity points.
        let wanted = self.turn.direction;
        let mut stick = input.stick;
        if self.force_speed.is_some() {
            if !has_input {
                stick_dir = wanted;
            }
            stick = stick_dir;
            has_input = true;
        }
        let wall = input.wall.map_or(Vec3::ZERO, |(normal, _)| normal);
        let wanted_zero = is_near_zero(wanted, NEAR_ZERO_SPEED);
        let along = dot(wanted, heading);
        let opposed = self.force_speed.is_none()
            && has_input
            && !wanted_zero
            && is_opposite_dir(stick_dir, wanted);
        let no_input = !has_input;
        let braking_request = self.turn.reverse || no_input;
        let into_wall = dot(wall, heading) < WALL_INTO_COS;
        self.update_hill(up, wanted, has_input);

        let mut speed = measured;
        let mut target = 0.;
        if along >= 0. && !braking_request {
            // Accelerate toward what the stick's tilt allows.
            let tilt = squared_length(stick).sqrt();
            let top = self.hill_speed()
                + if self.surface_run && input.max_speed * 0.99 - measured <= 0.001 {
                    0.
                } else {
                    input.max_speed
                };
            target = tilt * top + (1. - tilt) * self.min_speed;
            if into_wall && target > input.max_speed {
                target = input.max_speed;
            }
            let rate = self.accel_rate(input.max_speed, input.accel_frames, speed);
            speed = accel(speed, target, rate);
        }
        // Frames that speed above the brake speed have lasted.
        let border = self.border;
        if speed > self.brake_speed {
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
            self.brake_frames
        } else if !(no_input || moving_on) {
            1
        } else if !(opposed || no_input) {
            self.stick_brake_frames
        } else if self.brake_counter >= border {
            self.brake_frames
        } else {
            1
        };
        if speed > target {
            let braked = brake(speed, brake_frames, input.max_speed);
            speed = if braked < target { target } else { braked };
        }
        if let Some(forced) = self.force_speed {
            speed = speed.max(forced);
        }

        let pull = |velocity: Vec3| {
            Vec3::new(
                velocity.x - GRAVITY_MOVE * up.x,
                velocity.y - GRAVITY_MOVE * up.y,
                velocity.z - GRAVITY_MOVE * up.z,
            )
        };
        let mut steer = Steer {
            front: wanted,
            up,
            speed,
            ..Steer::default()
        };
        // A reversal or release at speed: run on along the facing and let the
        // brake state take over.
        if braking_request && !(wanted_zero || self.brake_counter < border) {
            steer.velocity = Vec3::new(speed * wanted.x, speed * wanted.y, speed * wanted.z);
            steer.set_velocity = pull(steer.velocity);
            steer.brake_run = true;
            self.movement = wanted;
            return steer;
        }
        // Turning the other way at a crawl: pivot on the spot.
        if opposed && !(wanted_zero || (along >= 0. && speed > self.min_speed)) {
            steer.velocity = Vec3::new(speed * heading.x, speed * heading.y, speed * heading.z);
            steer.set_velocity = pull(steer.velocity);
            steer.pivot = true;
            self.movement = wanted;
            return steer;
        }
        steer.stopped = is_near_zero_value(speed, NEAR_ZERO_SPEED) && !has_input;
        // The velocity swings toward the facing only partly: a run keeps its
        // sideways drift, more of it the less the turn is under way.
        let mut velocity = Vec3::new(speed * wanted.x, speed * wanted.y, speed * wanted.z);
        if self.drift && (1. - squared_length(heading).sqrt()).abs() <= NORMALIZED_TOLERANCE {
            let power = self.turn.power_signed(up).abs();
            let smooth = (power - self.turn_power).clamp(0., 1.);
            self.turn_power = lerp_value(self.turn_power, power, TURN_POWER_FOLLOW);
            let (side, has_side) = try_normalize_or_zero(verticalize_vec(wanted, heading));
            if has_side {
                let keep = lerp_value(DRIFT_KEEP_LEAST, DRIFT_KEEP_MOST, smooth);
                let forward = (speed * dot(wanted, heading)).max(self.min_speed);
                let along_side = dot(heading, side);
                let drift = Vec3::new(
                    keep * (speed * (side.x * along_side)),
                    keep * (speed * (side.y * along_side)),
                    keep * (speed * (side.z * along_side)),
                );
                let mix = Vec3::new(
                    wanted.x * forward + drift.x,
                    wanted.y * forward + drift.y,
                    wanted.z * forward + drift.z,
                );
                let length = squared_length(mix).sqrt();
                velocity = if length > 0. {
                    let scale = speed / length;
                    Vec3::new(scale * mix.x, scale * mix.y, scale * mix.z)
                } else {
                    mix
                };
            }
        } else {
            self.turn_power = 0.;
        }
        // Running up a wall's face is cut off (`0x418aec`).
        if let Some((normal, true)) = input.wall {
            cut_climb(&mut velocity, -up * GRAVITY_MOVE, normal);
        }
        self.movement = wanted;
        steer.velocity = velocity;
        steer.set_velocity = pull(velocity);
        steer
    }

    /// `updateSkateMove` (0x416d10): independent signed components, not scalar
    /// friction. Slope drive and wall braking follow the executable but have
    /// no coverage in the seven flat IceWalker recordings. ForceRun/dash and
    /// optional squat-hold overrides are not implemented.
    fn update_skate(&mut self, input: &SteerInput) -> Steer {
        use super::pose_math::{cross, normalize};
        use super::vector_lib::{calc_angle_degree, calc_dir_slide, parallelize_vec};
        let up = input.up;
        self.turn.setup_ice();
        self.turn_power = 0.;
        self.turn.update(TurnFrame {
            actor_front: input.actor_front,
            actor_up: input.actor_up,
            stick: input.stick,
            up,
        });
        let (stick_dir, has_input) = try_normalize_or_zero(input.stick);
        let wanted = if has_input {
            stick_dir
        } else {
            self.turn.direction
        };
        self.update_hill(up, wanted, has_input);
        let downhill = calc_dir_slide(Vec3::NEG_Y, up);
        let flat = is_near_zero(downhill, NEAR_ZERO_SPEED);
        let slope = if flat {
            0.
        } else {
            ((90. - calc_angle_degree(Vec3::NEG_Y, downhill)) / 30.).clamp(0., 1.)
        };
        self.brake_counter = (self.brake_counter + 1).min(self.border);
        let mut across = wanted;
        let mut across_input = has_input;
        if !flat {
            let (projected, ok) = try_normalize_or_zero(verticalize_vec(downhill, wanted));
            across = projected;
            if !ok {
                across = normalize(cross(up, wanted));
                across_input = false;
            }
        }
        let wall = input.wall.map_or(Vec3::ZERO, |(n, _)| n);
        let mut side_speed = dot(across, input.velocity);
        let side_reverse = side_speed < -self.min_speed;
        let side_wall = dot(wall, across * side_speed) < 0.;
        let mut side_target = 0.;
        if !side_reverse && across_input {
            let drive = dot(across, input.stick).clamp(0., 1.);
            let top = accel(
                self.hill_speed() + input.max_speed,
                input.max_speed,
                input.max_speed / input.accel_frames as f32,
            );
            side_target = drive * top + (1. - drive) * self.min_speed;
            side_speed = accel(side_speed, side_target, input.max_speed / 60.);
        }
        let frames = if side_wall {
            15
        } else if across_input && side_reverse {
            60
        } else if self.brake_counter >= self.border || has_input {
            120
        } else {
            1
        };
        if side_speed.abs() > side_target {
            side_speed = brake(side_speed, frames, input.max_speed);
        }
        let (mut along, mut along_input) = if !flat {
            let (axis, ok) = try_normalize_or_zero(parallelize_vec(downhill, wanted));
            (axis, ok && has_input)
        } else {
            (Vec3::ZERO, false)
        };
        if flat || is_near_zero(along, NEAR_ZERO_SPEED) {
            along = normalize(cross(up, wanted));
            along_input = false;
        }
        let mut along_speed = dot(along, input.velocity);
        let along_wall = dot(wall, along * along_speed) < 0.;
        let along_reverse = has_input && along_speed < -self.min_speed;
        let mut along_target = 0.;
        if !along_reverse && along_input {
            let slope_drive = slope
                * if dot(along, downhill) < 0. {
                    -0.5
                } else {
                    0.75
                };
            let drive = (dot(along, input.stick) + slope_drive).clamp(0., 1.);
            let top = accel(
                self.hill_speed() + input.max_speed,
                input.max_speed,
                input.max_speed / input.accel_frames as f32,
            );
            along_target = drive * top;
            along_speed = accel(
                along_speed,
                along_target,
                (1. + slope) * (input.max_speed / 60.),
            );
        }
        let frames = if along_wall {
            15
        } else if along_reverse {
            60
        } else if self.brake_counter >= self.border || has_input {
            120
        } else {
            1
        };
        if along_speed.abs() > along_target {
            along_speed = brake(along_speed, frames, input.max_speed);
        }
        let velocity = Vec3::new(
            along_speed * along.x + side_speed * across.x,
            along_speed * along.y + side_speed * across.y,
            along_speed * along.z + side_speed * across.z,
        );
        let speed = (side_speed * side_speed + along_speed * along_speed).sqrt();
        self.movement = self.turn.direction;
        Steer {
            velocity,
            set_velocity: Vec3::new(
                velocity.x - GRAVITY_MOVE * up.x,
                velocity.y - GRAVITY_MOVE * up.y,
                velocity.z - GRAVITY_MOVE * up.z,
            ),
            up,
            front: self.turn.direction,
            speed,
            stopped: is_near_zero_value(side_speed.abs() + along_speed.abs(), NEAR_ZERO_SPEED)
                && !has_input,
            slip_ice: speed > 3. && dot(velocity, wanted) < 0.,
            ..Steer::default()
        }
    }
}

/// `PlayerActionFunction::accel`: a value below its target gains `step` up to
/// it; a value above is left alone.
fn accel(value: f32, target: f32, step: f32) -> f32 {
    if value > target {
        return value;
    }
    let raised = value + step;
    if raised > target { target } else { raised }
}

/// The function `updateNormalMove` calls last (`0x418aec`): against the face of
/// a wall, the part of the velocity that climbs along the wall, less nine tenths
/// of the pull holding the player down, is taken off.
fn cut_climb(velocity: &mut Vec3, pull: Vec3, wall: Vec3) {
    let along_wall = verticalize_vec(wall, *velocity);
    // (Gravity is straight down: the executable's dots with the unit up vector. It takes the
    // pull's part along the wall's normal off into a vector it then does not read, and uses the
    // pull itself.)
    let climb = (0. * along_wall.x + 1. * along_wall.y)
        + 0. * along_wall.z
        + ((0. * pull.x + 1. * pull.y) + 0. * pull.z) * WALL_PULL_SHARE;
    if climb >= 0. || -climb < NEAR_ZERO_SPEED {
        velocity.x -= 0. * climb;
        velocity.y -= 1. * climb;
        velocity.z -= 0. * climb;
    }
}

#[path = "../tests/ground_move.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
