//! The thrown cap's terrain helper (`HackCap`'s function at `0x401d0c`, with the ground check at
//! `0x409f20`), which the throw nerves call every step before they set the pose.
//!
//! The cap keeps an up axis (`+0x24c`) next to its front (`+0x240`): the throw starts with the
//! one the player hands it, the ground's normal when the player stood on a slope under the
//! water (the world's up otherwise). Every step the up axis eases back toward the world's up
//! (a share of the way that grows over the first five steps, to half), the front turns with it
//! (`al::alongVectorNormalH`) and the velocity follows: its part along the old front keeps its
//! size along the new one, the rest becomes a part along the new up. With the world's up the
//! helper changes nothing, which is why throws on flat ground never showed it.
//!
//! The helper leaves things alone while the cap is low over the ground it flies over (an arrow
//! along the cap's own down axis reaches the ground within the throw height), and when it flies
//! down toward ground that rises to meet it. A cap higher than the throw height over ground
//! that is flatter than its own up axis is drawn down toward that height (`lift`).
use super::joint_math::{calc_rate01, ease_out};
use super::pose_math::{
    along_vector_normal_h, dot, make_quat_rotation_rate, normalize, rotate_by_quat,
};
use super::vector_lib::separate_parallel_vertical;
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
/// `al::isNearZeroOrLess(height - throw height, 0.001)`.
const NEAR_ZERO: f32 = 0.001;
/// The cosine of 5 degrees: the hit surface counts as tilted against the cap's up axis when the
/// cosine between them is under it.
const TILT_COS: f32 = 0.996_194_7;

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

/// The ground an arrow along the cap's down axis finds.
struct Ground {
    position: Vec3,
    normal: Vec3,
}

/// One step of the helper for a cap thrown from a height of `throw_height` above the ground.
pub fn follow(cap: Cap, mesh: Option<&TriangleMesh>, throw_height: f32) {
    let world_up = Vec3::Y;
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
    if let Some(ground) = &ground {
        let height = dot(*cap.spot - ground.position, *cap.up);
        let low = height - throw_height <= NEAR_ZERO;
        let diving = dot(*cap.front, world_up) < 0. && dot(*cap.front, ground.normal) > 0.;
        if low || diving {
            *cap.counter = 0;
            return;
        }
    }
    *cap.counter = (*cap.counter + 1).min(COUNTER_LIMIT);

    let rate = calc_rate01(*cap.counter as f32, RATE_FROM, RATE_TO);
    // The up axis eases toward the world's up (or its opposite for an axis that points down).
    let target = if dot(*cap.up, world_up) < 0. {
        -world_up
    } else {
        world_up
    };
    let turned = rotate_by_quat(
        *cap.up,
        make_quat_rotation_rate(*cap.up, target, rate * MAX_RATE),
    );
    let new_up = normalize(turned);
    let new_front = normalize(along_vector_normal_h(*cap.front, *cap.up, new_up));

    // Ground that is flatter than the cap's up axis draws a cap that is too high down to the
    // throw height (only while the cap does not point downward).
    if let Some(ground) = &ground {
        let level = dot(*cap.front, world_up) >= -NEAR_ZERO;
        let across = dot(new_up, ground.normal).abs();
        if level
            && across < TILT_COS
            && dot(new_up, world_up).abs() < dot(ground.normal, world_up)
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

    // The velocity: what ran along the old front now runs along the new one, the rest along the
    // new up axis.
    let (along, rest) = separate_parallel_vertical(*cap.front, *cap.velocity);
    let along = along.length();
    let rest = rest.length();
    *cap.velocity = Vec3::new(
        rest * new_up.x + along * new_front.x,
        rest * new_up.y + along * new_front.y,
        rest * new_up.z + along * new_front.z,
    );
    *cap.front = new_front;
    *cap.up = new_up;
}
