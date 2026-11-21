//! The spin-move throw's flight (`HackCap::exeThrowSpiral`) and where it starts.
//!
//! The cap does not fly out and back: it chases a point that circles the player. Each step the
//! point is the player's position (70 centimetres up) plus the throw direction turned about the up
//! axis by an angle that grows every step, scaled to a distance that grows too; from the tenth step
//! on a second, tighter point is blended in, and the cap's velocity is the way to the blended point
//! (at most 200 a step). The cap leaves the nerve for the return after 105 steps. Everything is the
//! executable's single-precision arithmetic in its order (`al::rotateVectorDegree`, `al::lerpVec`,
//! `al::easeOut`).
//!
//! Where the cap appears (`exeThrowStart`): 150 away from the player at 70 up, along the throw
//! direction turned a quarter of the way to the side the spiral circles: the sum of the direction
//! and the unit vector across it and the up axis, normalised, so 45 degrees round.
//!
//! Each step the nerve calls the throw nerves' terrain helper (`0x401d0c`, `cap_ground`) after the
//! reflect and wall reactions, and then turns the curve's front (`+0x300`) with the cap's up axis
//! the helper eased (`align_curve_front`); the circle's axis and the point it circles are the
//! cap's up axis (`+0x24c`), which is the world's up unless the throw began on a tilted floor.
//!
//! The nerve (entry 0x403a1c) asks `tryCollideReflectReaction` at call site 0x403ae8 before the terrain helper: a sign board
//! the cap touches sends it into its rebound.
//!
//! Not ported: the wall reaction of the spiral (`tryCollideWallReactionSpiral` 0x40416c, which
//! sends the cap into its rebound nerve when the wall's sensor accepts `CapTouchWall`; no sensor
//! in the rewrite answers it) and the five `SpiralTail` hit sensors (`validateHitSensor` on the
//! tenth step, then each follows the cap's recent positions: the sensors have no counterpart in
//! the rewrite's contact model, so what the tail hits is not produced).
use super::joint_math::ease_out;
use super::pose_math::{
    DEGREES_TO_RADIANS, along_vector_normal_h, cosf, cross, normalize, rotate_by_quat, sinf,
};
use super::vector_lib::{limit_length, verticalize_vec};
use bevy::prelude::{Quat, Vec3};

/// Steps the spiral lasts before the cap returns (`exeThrowSpiral` compares the step count + 1 with 105).
pub const FRAMES: u32 = 105;
/// The most the cap moves in a step toward its point.
const SPEED_MAX: f32 = 200.;
/// Distance from the player at which the spin-move throw appears.
const START_DISTANCE: f32 = 150.;
const UP: Vec3 = Vec3::Y;

/// Where `exeThrowStart` puts a spiral cap: the point above the player (`base`, feet plus the throw
/// height) and 150 along the throw direction `front` turned toward `side` (-1 clockwise).
pub fn start_position(base: Vec3, front: Vec3, side: f32) -> Vec3 {
    // `(front x up)`, negated for the clockwise side, added to the front and normalised.
    let across = cross(front, UP);
    let across = if side < 0. {
        Vec3::new(-across.x, -across.y, -across.z)
    } else {
        across
    };
    let diagonal = normalize(Vec3::new(
        across.x + front.x,
        across.y + front.y,
        across.z + front.z,
    ));
    Vec3::new(
        diagonal.x * START_DISTANCE + base.x,
        diagonal.y * START_DISTANCE + base.y,
        diagonal.z * START_DISTANCE + base.z,
    )
}

/// `al::rotateVectorDegree(vec, axis, degrees)`: `vec` turned about `axis` (not normalised).
fn rotate_vector_degree(vec: Vec3, axis: Vec3, degrees: f32) -> Vec3 {
    let half = (degrees * DEGREES_TO_RADIANS) * 0.5;
    let (cos, sin) = (cosf(half), sinf(half));
    rotate_by_quat(
        vec,
        Quat::from_xyzw(sin * axis.x, sin * axis.y, sin * axis.z, cos),
    )
}

/// One point of the spiral: `front` turned by `degrees` about the side's up axis, `distance` out.
fn point(pivot: Vec3, front: Vec3, axis: Vec3, degrees: f32, distance: f32) -> Vec3 {
    let turned = rotate_vector_degree(front, axis, degrees);
    Vec3::new(
        distance * turned.x + pivot.x,
        distance * turned.y + pivot.y,
        distance * turned.z + pivot.z,
    )
}

/// What one step of the spiral asks for.
pub struct Step {
    /// The velocity the cap is set to: the way to its point, at most 200.
    pub velocity: Vec3,
    /// The throw direction the cap keeps (`+0x240`): the new velocity's part across the up axis.
    pub front: Option<Vec3>,
}

/// The curve's front (`+0x300`) carried over from the cap's up axis before the terrain helper to
/// the one after it (`al::alongVectorNormalH` right after the helper's call).
pub fn align_curve_front(curve_front: Vec3, previous_up: Vec3, up: Vec3) -> Vec3 {
    along_vector_normal_h(curve_front, previous_up, up)
}

/// One step (`step` counts the steps of the nerve already run) of `exeThrowSpiral` for a cap at
/// `spot`, circling `pivot` (the player's position `up` the throw height) about the cap's `up`
/// axis, starting along `curve_front`.
pub fn step(step: u32, spot: Vec3, pivot: Vec3, curve_front: Vec3, up: Vec3, side: f32) -> Step {
    let axis = Vec3::new(side * up.x, side * up.y, side * up.z);
    let counted = step as f32 + 1.;
    let later = step.saturating_sub(9) as f32;
    let early = (step + 1).min(10) as f32;
    let outer = point(
        pivot,
        curve_front,
        axis,
        (early * 3. + later * 8.) + 330.,
        (counted * 60. + 175.).min(750.),
    );
    // The second point only exists from the tenth step; before it its weight is 0.
    let inner = if step >= 9 {
        point(
            pivot,
            curve_front,
            axis,
            later * 19. + 0.,
            (later * 6. + 50.).min(450.),
        )
    } else {
        outer
    };
    let share = ease_out((later * 0.125).clamp(0., 1.));
    let target = Vec3::new(
        outer.x + (inner.x - outer.x) * share,
        outer.y + (inner.y - outer.y) * share,
        outer.z + (inner.z - outer.z) * share,
    );
    let velocity = limit_length(
        Vec3::new(target.x - spot.x, target.y - spot.y, target.z - spot.z),
        SPEED_MAX,
    );
    let (across, found) = super::pose_math::try_normalize_or_zero(verticalize_vec(up, velocity));
    Step {
        velocity,
        front: found.then_some(across),
    }
}
