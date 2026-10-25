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
pub use super::ground_move::{GroundMove, SteerInput};
use super::t;
pub use super::turn_control::{
    ROUND_MIN_DEGREES, TurnControl, TurnFrame, TurnSetup, brake, ease_in,
};
use bevy::prelude::*;

/// Radians to degrees and back, with the original's constants.
const DEGREES: f32 = 57.295776;
const RADIANS: f32 = 0.017453292;
/// The pull the ground move control keeps on the player, in units per frame
/// (`mGravityMove`): what the trace reports as `vy` while running.
pub const GRAVITY_MOVE: f32 = 7.;
/// Speed kept after turning round, relative to the speed the brake began at.
pub const RUN_AFTER_TURN_SCALE: f32 = 0.5;
/// The stick deadzone of the original's move input.
pub const STICK_DEADZONE: f32 = 0.1;

/// `al::calcRate01`: where `value` lies between `from` and `to`, 0 to 1.
pub fn rate01(value: f32, from: f32, to: f32) -> f32 {
    let span = to - from;
    if span.abs() < 0.001 {
        return 1.;
    }
    ((value - from) / span).clamp(0., 1.)
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
fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0., v.z)
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
            speed = (speed + ground.accel_rate(14., 40, speed)).min(14.);
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
        control.reset(Vec3::ZERO, Vec3::Z, Vec3::Y);
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
