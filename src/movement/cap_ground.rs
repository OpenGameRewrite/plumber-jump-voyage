//! The thrown cap's terrain helper (`HackCap`'s function at `0x401d0c`, with the ground check at
//! `0x409f20`), which the throw nerves call every step before they set the pose.
//!
//! The cap keeps an up axis (`+0x24c`) next to its front (`+0x240`): the throw starts with the
//! one the player hands it, the ground's normal when the player stood on a slope under the
//! water (the world's up otherwise). The helper has three parts, in this order.
//!
//! 1. The ground check (`0x409f20`): an arrow along the cap's own down axis. The easing is
//!    skipped while the cap is low over the ground it flies over (the arrow reaches the ground
//!    within the throw height), and while it flies down toward ground that rises to meet it.
//!    Both reset the step counter (`+0x28c`); otherwise the counter counts up to five.
//! 2. The easing (`0x401dc8..0x4020e4`): the up axis eases back toward the world's up (a share
//!    of the way that grows over the first five steps, to half), the front turns with it
//!    (`al::alongVectorNormalH`, then made a unit vector, every time this part runs) and the
//!    velocity follows: its part along the old front keeps its size along the new one, the rest
//!    becomes a part along the new up. A cap higher than the throw height over ground that is
//!    flatter than its own up axis is drawn down toward that height. With the world's up the
//!    easing changes nothing, which is why throws on flat ground never showed it.
//! 3. The collision turn (`0x4020e8..0x402440`, whether or not the easing ran): when the cap's
//!    collider met a wall face last move that is steep against the cap's up axis (its normal at
//!    least half along it and against the gravity), or else met the ground, the cap takes that
//!    surface's normal for its up axis, the front is laid in the new plane (made a unit vector
//!    again, and only here), the velocity is rebuilt the same way, the counter restarts and, for
//!    a turn of a tenth of a degree or more, the collision reaction and the sound `HitCol` are
//!    raised. Without such a touch, or when the normal points away from the up axis, nothing of
//!    this part runs.
//!
//! The caller ends the helper's work by standing the cap upright with its up axis, facing back
//! along its front (`CapPose::face_back_along`, `0x402444`).
use super::joint_math::{calc_rate01, ease_out};
use super::pose_math::{
    NEGATED_GRAVITY, along_vector_normal_h, dot, make_quat_rotation_rate, normalize, rotate_by_quat,
};
use super::vector_lib::{calc_angle_degree, converge, separate_parallel_vertical};
use crate::collider::{FaceFilter, first_strike};
use crate::geometry::TriangleMesh;
use bevy::prelude::Vec3;

/// The arrow along the cap's down axis reaches this far (`getHitPosAndNormalOnArrow` with
/// `up * -200`).
const ARROW_LENGTH: f32 = 200.;
/// The counter of steps the helper has worked (`+0x28c`) counts up to this, and the share it
/// turns the up axis by grows from nothing at `RATE_FROM` to `MAX_RATE` at `RATE_TO`.
const COUNTER_LIMIT: i32 = 5;
const RATE_FROM: f32 = 2.;
const RATE_TO: f32 = 5.;
const MAX_RATE: f32 = 0.5;
/// The limit of `al::isNearZeroOrLess(height - throw height, 0.001)` and `al::isNearZeroOrGreater`
/// (`0x187fc80`): both compare strictly against it (`< 0.001`, `> -0.001`).
const NEAR_ZERO: f32 = 0.001;
/// The cosine of 5 degrees: the hit surface counts as tilted against the cap's up axis when the
/// cosine between them is under it.
const TILT_COS: f32 = 0.996_194_7;
/// A wall face turns the cap only when its normal is at least this much along the cap's up axis
/// (`0x402160`) ...
const WALL_UP_LIMIT: f32 = 0.5;
/// ... and against the gravity by more than the sine of 5 degrees (`0x402194`).
const WALL_GRAVITY_LIMIT: f32 = -0.087_155_744;
/// The collision turn raises its reaction from this angle (degrees) on (`0x402328`).
const REACTION_DEGREES: f32 = 0.1;
/// The cap's gravity, (0, -1, 0): what the actor's `al::getGravity` holds.
const GRAVITY: Vec3 = Vec3::new(0., -1., 0.);

/// What the helper reads and writes of the cap.
pub struct Cap<'a> {
    /// Where the cap is (centimetres, replay frame).
    pub spot: &'a mut Vec3,
    /// The cap's front and up axes (`+0x240`, `+0x24c`).
    pub front: &'a mut Vec3,
    pub up: &'a mut Vec3,
    pub velocity: &'a mut Vec3,
    /// The steps the helper has been working (`+0x28c`).
    pub counter: &'a mut i32,
}

/// A surface the cap's collider touched in its last move.
#[derive(Clone, Copy, Debug)]
pub struct Surface {
    pub normal: Vec3,
}

/// What the cap's collider found in its last move: the wall it met on a face
/// (`rs::isCollidedWallFace`, `getCollidedWallNormal`) and the ground under it
/// (`rs::isCollidedGround`, `getCollidedGroundNormal`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Touch {
    pub wall_face: Option<Surface>,
    pub ground: Option<Surface>,
}

/// The ground an arrow along the cap's down axis finds.
struct Ground {
    position: Vec3,
    normal: Vec3,
}

/// One step of the helper for a cap thrown from a height of `throw_height` above the ground.
/// Returns the angle (degrees) of a collision turn wide enough to raise the reaction and sound.
pub fn follow(
    mut cap: Cap,
    touch: &Touch,
    mesh: Option<&TriangleMesh>,
    throw_height: f32,
) -> Option<f32> {
    // The arrow along -up (the cap's own, not the world's).
    let ground = mesh.and_then(|mesh| {
        let arrow = Vec3::new(
            cap.up.x * -ARROW_LENGTH,
            cap.up.y * -ARROW_LENGTH,
            cap.up.z * -ARROW_LENGTH,
        );
        first_strike(mesh, *cap.spot, arrow, FaceFilter::Any).map(|strike| Ground {
            position: strike.position,
            normal: strike.normal,
        })
    });
    if keeps_low(&cap, ground.as_ref(), throw_height) {
        *cap.counter = 0;
    } else {
        *cap.counter = converge(*cap.counter, COUNTER_LIMIT, 1);
        ease_toward_world_up(&mut cap, ground.as_ref(), throw_height);
    }
    turn_along_touch(&mut cap, touch)
}

/// The ground check's answer (`0x409f20`): the cap is within the throw height over the ground, or
/// flies down toward ground that rises to meet it.
fn keeps_low(cap: &Cap, ground: Option<&Ground>, throw_height: f32) -> bool {
    let Some(ground) = ground else {
        return false;
    };
    let height = dot(*cap.spot - ground.position, *cap.up);
    let low = height - throw_height < NEAR_ZERO;
    let diving = dot(*cap.front, NEGATED_GRAVITY) < 0. && dot(*cap.front, ground.normal) > 0.;
    low || diving
}

/// The easing part (`0x401dc8..0x4020e4`).
fn ease_toward_world_up(cap: &mut Cap, ground: Option<&Ground>, throw_height: f32) {
    let rate = calc_rate01(*cap.counter as f32, RATE_FROM, RATE_TO);
    // The up axis eases toward the world's up (or its opposite for an axis that points down).
    let target = if dot(*cap.up, NEGATED_GRAVITY) < 0. {
        -NEGATED_GRAVITY
    } else {
        NEGATED_GRAVITY
    };
    let turned = rotate_by_quat(
        *cap.up,
        make_quat_rotation_rate(*cap.up, target, rate * MAX_RATE),
    );
    let new_up = normalize(turned);
    let new_front = normalize(along_vector_normal_h(*cap.front, *cap.up, new_up));

    // Ground that is flatter than the cap's up axis draws a cap that is too high down to the
    // throw height (only while the cap does not point downward).
    if let Some(ground) = ground {
        let level = dot(*cap.front, NEGATED_GRAVITY) > -NEAR_ZERO;
        let across = dot(new_up, ground.normal).abs();
        if level
            && across < TILT_COS
            && dot(new_up, NEGATED_GRAVITY).abs() < dot(ground.normal, NEGATED_GRAVITY)
        {
            let pull = ease_out(calc_rate01(*cap.counter as f32, 0., RATE_TO));
            let height = dot(*cap.spot - ground.position, ground.normal);
            let amount = pull * (throw_height - height).min(0.);
            *cap.spot = Vec3::new(
                cap.spot.x + ground.normal.x * amount,
                cap.spot.y + ground.normal.y * amount,
                cap.spot.z + ground.normal.z * amount,
            );
        }
    }

    rebuild_velocity(cap, new_front, new_up);
    *cap.front = new_front;
    *cap.up = new_up;
}

/// The collision turn (`0x4020e8..0x402440`).
fn turn_along_touch(cap: &mut Cap, touch: &Touch) -> Option<f32> {
    let surface = touched_surface(touch, *cap.up)?;
    // A surface facing away from the up axis leaves the cap as it is.
    if dot(surface.normal, *cap.up) <= 0. {
        return None;
    }
    let new_front = normalize(along_vector_normal_h(*cap.front, *cap.up, surface.normal));
    rebuild_velocity(cap, new_front, surface.normal);
    let turn = calc_angle_degree(*cap.front, new_front);
    *cap.front = new_front;
    *cap.up = surface.normal;
    *cap.counter = 0;
    (turn >= REACTION_DEGREES).then_some(turn)
}

/// The surface that turns the cap: the wall face when it is steep against the cap's up axis and
/// leans against the gravity, otherwise the ground.
fn touched_surface(touch: &Touch, up: Vec3) -> Option<Surface> {
    if let Some(wall) = touch.wall_face
        && dot(wall.normal, up) >= WALL_UP_LIMIT
        && dot(wall.normal, GRAVITY) < WALL_GRAVITY_LIMIT
    {
        return Some(wall);
    }
    touch.ground
}

/// The velocity: what ran along the old front now runs along the new one, the rest along the
/// new up axis.
fn rebuild_velocity(cap: &mut Cap, new_front: Vec3, new_up: Vec3) {
    let (along, rest) = separate_parallel_vertical(*cap.front, *cap.velocity);
    let along = along.length();
    let rest = rest.length();
    *cap.velocity = Vec3::new(
        rest * new_up.x + along * new_front.x,
        rest * new_up.y + along * new_front.y,
        rest * new_up.z + along * new_front.z,
    );
}
