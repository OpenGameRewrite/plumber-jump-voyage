//! Standing clearance follows `PlayerCeilingCheck::update` (1.0.0, 0x42f494)
//! and its stepped sphere sweep (0x84d174).
//!
//! The check decides whether the standing body fits (`PlayerJudgeEnableStandUp`, 0x459514,
//! asks it) with a sphere of the squat radius swept upward from the feet. The sweep is not
//! an exact first-contact test: the sphere is stepped along the move (at most 35 cm a step),
//! every face it overlaps at a step is looked at once, and a face's time is backed out of the
//! overlap of that step, so which region of the face the stepped centre was in decides the
//! answer (see `strike_sphere_move`).
use crate::collider::{Location, SphereHit, sphere_strike_hits};
use crate::geometry::TriangleMesh;
use bevy::prelude::Vec3;
use std::collections::HashSet;

/// `PlayerCollisionCheckSphereMove(director, 16)`: the hits one sweep keeps.
const MAX_HITS: usize = 16;
/// The longest step of the sphere along its move (`fmin(radius, 35)`).
const MAX_STEP: f32 = 35.;
/// `al::isNearZero` tolerance the sweep uses for the shift and the edge direction.
const NEAR_ZERO: f32 = 0.001;
/// `al::isCeilingPolygon`: the face normal against straight down, 70 degrees of tilt.
const CEILING_COS: f32 = 0.342_020_15;
/// `PlayerConst` defaults the unladen actor passes (`Tall`, `CollisionRadiusSquat`).
pub const TALL: f32 = 160.;
pub const COLLISION_RADIUS_SQUAT: f32 = 45.;

/// One face the sweep touches and the fraction of the move at which the sphere meets it. (The
/// original also keeps the touched point; only the ceiling check's `getPos` reads it, for the
/// safety space nothing here uses.)
#[derive(Clone, Copy, Debug)]
pub struct SphereMoveHit {
    pub t: f32,
    pub triangle: u32,
}

/// `alCollisionUtil::checkStrikeSphereMove`: sweep a sphere of `radius` from `start` along
/// `motion`, hits sorted by their time.
pub fn strike_sphere_move(
    mesh: &TriangleMesh,
    start: Vec3,
    motion: Vec3,
    radius: f32,
) -> Vec<SphereMoveHit> {
    // `SphereInterpolator::startInterp`: the step is a share of the move's length.
    let length = motion.length();
    let step_share = if length > 0. {
        MAX_STEP.min(radius) / length
    } else {
        1.
    };
    let mut hits = Vec::new();
    // The faces already looked at in this sweep (the triangle filter's list).
    let mut seen = HashSet::new();
    let mut at = 0.0_f32;
    loop {
        // `SphereInterpolator::nextStep`
        let before = at;
        at = (at + step_share).min(1.);
        if before == 1. && at == 1. {
            break;
        }
        let center = start + motion * at;
        let strikes = sphere_strike_hits(mesh, center, radius);
        for (triangle, strike) in strikes.into_iter().rev() {
            if hits.len() >= MAX_HITS {
                break;
            }
            if seen.contains(&triangle) {
                continue;
            }
            match face_hit(mesh, triangle, &strike, center, radius, motion, at) {
                FaceAnswer::Back => {
                    seen.insert(triangle);
                }
                FaceAnswer::Skip => {}
                FaceAnswer::Hit(t) => {
                    seen.insert(triangle);
                    hits.push(SphereMoveHit { t, triangle });
                }
            }
        }
        if hits.len() >= MAX_HITS {
            break;
        }
    }
    hits.sort_by(|a, b| a.t.total_cmp(&b.t));
    hits
}

enum FaceAnswer {
    /// The face looks away from the move: remembered, not reported.
    Back,
    /// Degenerate geometry: neither reported nor remembered.
    Skip,
    Hit(f32),
}

/// What one overlapped face contributes at the step `at` (fraction of the move).
fn face_hit(
    mesh: &TriangleMesh,
    triangle: u32,
    strike: &SphereHit,
    center: Vec3,
    radius: f32,
    motion: Vec3,
    at: f32,
) -> FaceAnswer {
    let t = &mesh.triangles[triangle as usize];
    let normal = t.normal;
    let along = normal.dot(motion);
    if along >= 0. {
        return FaceAnswer::Back;
    }
    if strike.location != Location::Face {
        // An edge or corner region: the touched point alone, a point the sphere moved from.
        let back = time_back(strike.position - center, motion, radius);
        return FaceAnswer::Hit(at - back);
    }
    // The centre is over the face: the move backs out of the overlap along the move, and the
    // touched point follows the move's part across the face.
    let back = strike.depth / along.abs();
    let shift = across(motion * -back, normal);
    let point = strike.position + shift;
    let outside = (0..3).any(|i| t.edges[i].dot(point - t.corners[i]) > 0.);
    if !outside {
        return FaceAnswer::Hit(at - back);
    }
    // The point left the face: the sphere meets the edge the shift crossed, or the corner.
    if shift.length_squared() < NEAR_ZERO * NEAR_ZERO {
        return FaceAnswer::Skip;
    }
    let corner = if t.edges[0].dot(shift) <= 0. {
        1
    } else if t.edges[1].dot(shift) <= 0. {
        2
    } else {
        0
    };
    let to_corner = t.corners[corner] - strike.position;
    let turning = normal.dot(to_corner.cross(shift));
    let edge = t.edges[(corner + usize::from(turning > 0.)) % 3];
    let slope = edge.dot(shift);
    if slope.abs() < NEAR_ZERO {
        return FaceAnswer::Skip;
    }
    let crossing = strike.position + shift * (edge.dot(to_corner) / slope);
    let back = time_back(crossing - center, motion, radius);
    FaceAnswer::Hit(at - back)
}

/// How far back along the move (as a share of it) the centre was at `radius` from the point
/// `offset` away from it now: the smaller non-negative root of |offset + k * motion| = radius,
/// 0 when the point is never that close.
fn time_back(offset: Vec3, motion: Vec3, radius: f32) -> f32 {
    let a = motion.dot(motion);
    let b = 2. * motion.dot(offset);
    let c = offset.dot(offset) - radius * radius;
    let discriminant: f32 = b * b + (-4. * a) * c;
    if discriminant < 0. {
        return 0.;
    }
    let root = discriminant.sqrt();
    // The numerically stable form: the root away from the cancellation first.
    let half = 0.5 * if b > 0. { -b - root } else { root - b };
    let first = (1. / a) * half;
    if first >= 0. { first } else { c / (a * first) }
}

/// `al::verticalizeVec`: the part of `v` across the unit vector `axis`.
fn across(v: Vec3, axis: Vec3) -> Vec3 {
    v - axis * axis.dot(v)
}

/// The answers of `PlayerCeilingCheck` the actor reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CeilingCheck {
    /// `isEnableStandUp`: the squat sphere's sweep finds the ceiling no nearer than the
    /// standing height.
    pub enable_stand_up: bool,
    pub enable_hold_up: bool,
    pub pressed: bool,
}
impl CeilingCheck {
    /// Nothing overhead: the answers of a check that found no face.
    pub const OPEN: Self = Self {
        enable_stand_up: true,
        enable_hold_up: true,
        pressed: false,
    };
}

/// `PlayerCeilingCheck::update` in the ordinary 3D mode (not the grab arrangement) for a
/// body standing at `trans` with `up` its up axis (centimetres, replay frame).
pub fn update(
    mesh: &TriangleMesh,
    trans: Vec3,
    up: Vec3,
    tall: f32,
    radius_squat: f32,
    head_clearance: f32,
    hold_height: f32,
) -> CeilingCheck {
    let start = trans + up * (radius_squat - head_clearance);
    let player_height = 2. * tall - 2. * radius_squat;
    let stand_height = tall - 2. * radius_squat;
    let check_height = player_height + head_clearance + hold_height;
    let min_t_stand = (stand_height + head_clearance) / check_height;
    let hits = strike_sphere_move(mesh, start, up * check_height, radius_squat);
    let down = -up;
    let mut min_t = 1.0_f32;
    let mut min_t_counted = 1.0_f32;
    for hit in &hits {
        let triangle = &mesh.triangles[hit.triangle as usize];
        if triangle.normal.dot(down) < CEILING_COS {
            continue;
        }
        min_t = min_t.min(hit.t);
        let codes = mesh.codes_of(hit.triangle);
        // In the ordinary mode a Press floor code does not count, and nor does NoCeilSquat.
        if codes.floor == "Press" || codes.wall == "NoCeilSquat" {
            continue;
        }
        min_t_counted = min_t_counted.min(hit.t);
    }
    let min_t_hold = 0.99 * (check_height - hold_height);
    let max_t_pressed = 0.01 * (check_height - hold_height);
    CeilingCheck {
        enable_stand_up: min_t_counted >= min_t_stand,
        enable_hold_up: min_t >= min_t_hold / check_height,
        pressed: min_t_counted < max_t_pressed / check_height,
    }
}
