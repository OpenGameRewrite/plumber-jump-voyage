//! GrabCeil's ordered thickness, transverse and body-clearance arrow queries.
use super::pole_find::{Probe, first_poly};
use super::pose_math::{dot, normalize, try_normalize_or_zero};
use crate::collider::{FaceFilter, Strike, arrow_strikes_filtered};
use crate::geometry::TriangleMesh;
use bevy::prelude::Vec3;

#[derive(Clone, Copy, Debug)]
pub(super) struct Fit {
    pub pos: Vec3,
    pub front: Vec3,
    pub axis: Vec3,
    pub triangle: u32,
}

fn last(mesh: &TriangleMesh, pos: Vec3, arrow: Vec3) -> Option<Strike> {
    arrow_strikes_filtered(mesh, pos, arrow, FaceFilter::Any)
        .into_iter()
        .max_by(|a, b| a.distance.total_cmp(&b.distance))
}
fn first(mesh: &TriangleMesh, pos: Vec3, arrow: Vec3) -> Option<Strike> {
    first_poly(mesh, pos, arrow, Probe::Any)
}

/// 0x5630b0: last hit toward the contact, then its perpendicular thickness.
fn thickness(mesh: &TriangleMesh, normal: Vec3, pos: Vec3) -> Option<(Strike, f32)> {
    let arrow = normal * 100.;
    let hit = last(mesh, pos - arrow, arrow)?;
    let depth = dot(hit.position - pos, hit.normal).abs();
    Some((hit, depth))
}

/// 0x5631dc: fit 50 cm of body clearance along the bar, five behind the surface.
fn fit_body(mesh: &TriangleMesh, pos: Vec3, normal: Vec3, front: Vec3) -> Vec3 {
    let side = normalize(normal.cross(front));
    let arrow = side * 50.;
    let centre = pos - normal * 5.;
    let hit = first(mesh, centre - arrow, arrow).or_else(|| first(mesh, centre + arrow, -arrow));
    let Some(hit) = hit else { return pos };
    let left = (50. - dot(centre - hit.position, side).abs()).max(0.);
    pos + side * (-dot(side, hit.normal)).signum() * left
}

/// No-wall finder: thickness first, then the two transverse directions in order.
pub(super) fn no_wall(mesh: &TriangleMesh, normal: Vec3, pos: Vec3, facing: Vec3) -> Option<Fit> {
    let (hit, depth) = thickness(mesh, normal, pos)?;
    let (surface, point) = if hit.normal.y > 0. {
        (normal, pos)
    } else {
        (hit.normal, hit.position)
    };
    let mut across = try_normalize_or_zero(normal.with_y(0.));
    if !across.1 {
        across = try_normalize_or_zero(facing - normal * dot(normal, facing));
        if !across.1 {
            across.0 = normalize(Vec3::Y - normal * dot(normal, Vec3::Y));
        }
    }
    let other = normalize(across.0.cross(surface));
    for direction in [across.0, other] {
        let length = depth + depth;
        let offset = direction * (length + 1.);
        let arrow = -direction * (length + 2.);
        let Some(a) = last(mesh, point - surface + offset, arrow) else {
            continue;
        };
        if dot(a.normal, surface).abs() >= std::f32::consts::FRAC_1_SQRT_2 {
            continue;
        }
        let Some(b) = last(mesh, point - surface - offset, -arrow) else {
            continue;
        };
        // The executable repeats the first normal's test here.
        if dot(a.normal, surface).abs() >= std::f32::consts::FRAC_1_SQRT_2 {
            continue;
        }
        let centre = point
            + (a.normal * dot(a.normal, a.position - point)
                + b.normal * dot(b.normal, b.position - point))
                * 0.5;
        let front = if dot(a.normal, facing) > dot(b.normal, facing) {
            a.normal
        } else {
            b.normal
        };
        return Some(Fit {
            pos: fit_body(mesh, centre, surface, direction),
            front,
            axis: surface,
            triangle: hit.triangle,
        });
    }
    None
}

pub(super) fn wall(mesh: &TriangleMesh, front: Vec3, pos: Vec3) -> Option<Fit> {
    let depth = thickness(mesh, -front, pos).map_or(1., |(_, depth)| depth);
    let start = (pos + front * (depth * 0.5)) + Vec3::NEG_Y * 160.;
    let hit = first(mesh, start, Vec3::Y * 161.)?;
    if mesh.codes_of(hit.triangle).floor != "GrabCeil" || hit.normal.y >= -0.01 {
        return None;
    }
    Some(Fit {
        pos: fit_body(mesh, hit.position, hit.normal, front),
        front,
        axis: Vec3::Y,
        triangle: hit.triangle,
    })
}
