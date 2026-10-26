//! The rolling throw (`HackCap::exeThrowStart`'s rolling branch, `exeThrowRolling`,
//! `rollingGround` and `exeThrowRollingBrake`): the cap thrown down onto the floor at the end of a
//! ground pound (`DoubleDown`, the throw the pound's landing cancel starts) rolls away along it.
//!
//! The cap starts where any throw does, 150 ahead and 70 up, but its direction turns straight
//! down and its up axis becomes the throw's side (the throw direction crossed with the world's
//! up), so the model rolls like a wheel about it. It falls onto the floor, and from then on
//! every step `rollingGround` turns its direction along the surface it touches (the contact's
//! normal crossed with the side axis) at the speed it has; in the air it turns toward the ground
//! by a tenth of the angle instead. It rolls at `RollSpeed` for the steps the distance allows,
//! brakes at a constant rate over the brake time and stays where it stops.
//!
//! Only the ground throw is here: the up throw (`DoubleUp`) comes from a swing of the motion
//! controls, and the wall reaction of the rolling nerves (`tryCollideWallReactionRollingGround`)
//! is not ported.
use super::pose_math::{
    atan2f, cosf, cross, dot, normalize, rotate_by_quat, sinf, squared_length,
    try_normalize_or_zero,
};
use super::vector_lib::{is_near_zero, is_near_zero_value, verticalize_vec};
use bevy::prelude::{Quat, Vec3};

/// `HackCapThrowParam`'s rolling entries of `MarioCapCommonInfo`: the speed, the distance and the
/// brake time of the throw down, how closely the pose follows the surface on the ground and in
/// the air.
pub const SPEED: f32 = 30.;
const DISTANCE_DOWN: f32 = 1000.;
pub const BRAKE_FRAMES_DOWN: u32 = 30;
const POSE_TRACK_GROUND: f32 = 1.;
const POSE_TRACK_AIR: f32 = 0.1;
/// What the nerves keep of the velocity across the rolling direction each step.
const SIDE_KEEP: f32 = 0.94;
/// `scaleVelocityExceptDirection` and `isNearZero` of a velocity.
const NEAR_ZERO: f32 = 0.001;
const UP: Vec3 = Vec3::Y;
const GRAVITY: Vec3 = Vec3::NEG_Y;

/// How long the throw rolls and how hard it brakes (`exeThrowStart`'s `_27c` and `_280`).
#[derive(Clone, Copy, Debug)]
pub struct Plan {
    /// Steps of the rolling nerve (it runs it for this many plus one).
    pub rolling_steps: u32,
    /// Speed added along the direction each brake step (negative).
    pub brake_step: f32,
}

/// The plan of the throw down: the distance less the braking distance, over the speed.
pub fn plan_down() -> Plan {
    let frames = BRAKE_FRAMES_DOWN as f32;
    let per_frame = SPEED / frames;
    let braking = frames * (frames * (per_frame * -0.5));
    Plan {
        rolling_steps: ((DISTANCE_DOWN - (SPEED * frames + braking)) / SPEED) as u32,
        brake_step: -per_frame,
    }
}

/// The direction and up axis `exeThrowStart` gives a cap thrown down: straight down and the
/// throw's side, `front x up` normalised (`front` is the horizontal throw direction).
pub fn start_axes(front: Vec3) -> (Vec3, Vec3) {
    let side = normalize(cross(front, UP));
    (-UP, side)
}

/// What one step of `rollingGround` decided.
#[derive(Clone, Copy, Debug)]
pub struct Rolled {
    pub direction: Vec3,
    pub velocity: Vec3,
    /// The pose to take: the cap's front is the way back along `direction`, its up the side
    /// axis. How much of the way to it the pose goes is `pose_rate`; none: all of it.
    pub pose_rate: Option<f32>,
    /// The cap touched a surface: it is rolling on it (the grounding effect runs).
    pub grounded: bool,
}

/// One step of `HackCap::rollingGround`. `contact` is the normal of the surface the collider
/// touched (the floor's, else a wall's, else the ceiling's), `velocity` the cap's velocity.
/// `None` when the step changes nothing (a contact that gives no direction, or no speed).
pub fn roll(direction: Vec3, up: Vec3, velocity: Vec3, contact: Option<Vec3>) -> Option<Rolled> {
    match contact.filter(|normal| !is_near_zero(*normal, NEAR_ZERO)) {
        None => Some(roll_in_air(direction, up, velocity)),
        Some(normal) => roll_on_surface(direction, up, velocity, normal),
    }
}

/// No surface: the direction turns toward the ground by a tenth of the angle between them, about
/// their common perpendicular, and the speed stays.
fn roll_in_air(direction: Vec3, up: Vec3, velocity: Vec3) -> Rolled {
    let axis = cross(direction, GRAVITY);
    let cosine = dot(GRAVITY, direction);
    let mut angle = atan2f(squared_length(axis).sqrt(), cosine);
    let (axis, found) = try_normalize_or_zero(axis);
    let axis = if found {
        axis
    } else {
        // Along or against the gravity: any axis across will do; the original takes the cap's
        // own up reversed, and a full turn when the two are opposed.
        angle = if is_near_zero_value(angle, NEAR_ZERO) {
            0.
        } else {
            std::f32::consts::PI
        };
        -up
    };
    let half = (angle * POSE_TRACK_AIR) * 0.5;
    let (cos, sin) = (cosf(half), sinf(half));
    let turned = normalize(rotate_by_quat(
        direction,
        Quat::from_xyzw(sin * axis.x, sin * axis.y, sin * axis.z, cos),
    ));
    let speed = squared_length(velocity).sqrt();
    Rolled {
        direction: turned,
        velocity: Vec3::new(speed * turned.x, speed * turned.y, speed * turned.z),
        pose_rate: None,
        grounded: false,
    }
}

/// A surface: the direction becomes the one along it across the cap's up axis, on the side the
/// old direction pointed to, and the velocity follows at its speed.
fn roll_on_surface(direction: Vec3, up: Vec3, velocity: Vec3, normal: Vec3) -> Option<Rolled> {
    let (across, found) = try_normalize_or_zero(verticalize_vec(up, normal));
    if !found {
        return None;
    }
    let speed = squared_length(velocity).sqrt();
    if squared_length(velocity) < super::pose_math::NO_DIRECTION {
        return None;
    }
    let mut along = normalize(cross(across, up));
    let facing = dot(along, direction);
    if facing < 0. {
        let back = Vec3::new(
            direction.x * facing + direction.x * facing,
            direction.y * facing + direction.y * facing,
            direction.z * facing + direction.z * facing,
        );
        along = normalize(Vec3::new(
            along.x - back.x,
            along.y - back.y,
            along.z - back.z,
        ));
    }
    Some(Rolled {
        direction: along,
        velocity: Vec3::new(speed * along.x, speed * along.y, speed * along.z),
        pose_rate: Some(POSE_TRACK_GROUND),
        grounded: true,
    })
}

/// `al::scaleVelocityExceptDirection(actor, direction, rate)`: the part of the velocity along the
/// direction stays, the rest is scaled.
pub fn keep_along(velocity: Vec3, direction: Vec3) -> Vec3 {
    let along = dot(direction, velocity);
    let kept = Vec3::new(
        direction.x * along,
        direction.y * along,
        direction.z * along,
    );
    Vec3::new(
        kept.x + (velocity.x - kept.x) * SIDE_KEEP,
        kept.y + (velocity.y - kept.y) * SIDE_KEEP,
        kept.z + (velocity.z - kept.z) * SIDE_KEEP,
    )
}

/// The brake nerve ends when the cap has stopped or turned back against its direction.
pub fn has_stopped(velocity: Vec3, direction: Vec3) -> bool {
    is_near_zero(velocity, NEAR_ZERO)
        || (direction.x * velocity.x + direction.y * velocity.y + direction.z * velocity.z) < 0.
}
