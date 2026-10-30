//! KCHitDisk (1.0.0 0x854dc4) in a collision part's local frame.
use super::{Point, Prism, dot, length};
use crate::movement::console_libm::{acosf, sinf};

// Original predicates at 0x855118, 0x855f3c and 0x92bbc8 load
// 0x187fc80. The segment routine compares SQUARED length to this value.
const NEAR_ZERO: f32 = 0.001;

fn sub(a: Point, b: Point) -> Point {
    std::array::from_fn(|i| a[i] - b[i])
}
fn at(a: Point, delta: Point, t: f32) -> Point {
    std::array::from_fn(|i| delta[i] * t + a[i])
}
fn vertical(axis: Point, v: Point) -> Point {
    let d = dot(axis, v);
    std::array::from_fn(|i| v[i] - axis[i] * d)
}
fn magnitude(v: f32) -> f32 {
    if v > 0. { v } else { -v }
}

/// Degenerate second segment of calcSquaredDistanceSegmentToSegment
/// (0x92bb44): the closest point of a segment to the origin.
fn closest_to_origin(a: Point, b: Point) -> (f32, Point) {
    let delta = sub(b, a);
    let squared = dot(delta, delta);
    let point = if squared.abs() < NEAR_ZERO {
        a
    } else {
        let t = (-dot(delta, a) / squared).clamp(0., 1.);
        at(a, delta, t)
    };
    (dot(point, point), point)
}

/// The support length the original obtains through sin(acos(abs(dot))).
fn support(normal: Point, axis: Point, radius: f32, half_height: f32) -> (f32, f32, f32) {
    let cosine = magnitude(dot(normal, axis));
    let radial = sinf(acosf(cosine.clamp(-1., 1.))) * radius;
    (cosine * half_height + radial, cosine, radial)
}

/// Disk/edge depth adjustment, 0x855e90. Clip the edge to the disk's
/// axial slab, then find its closest point in the disk plane.
#[allow(clippy::too_many_arguments)] // Original helper's independent geometric inputs.
fn edge_depth(
    a: Point,
    b: Point,
    center: Point,
    axis: Point,
    normal: Point,
    heights: [f32; 2],
    half_height: f32,
    radial: f32,
    depth: f32,
) -> Option<f32> {
    let edge = sub(b, a);
    let (unit, _) =
        crate::movement::pose_math::try_normalize_or_zero(bevy::prelude::Vec3::from(edge));
    let (start, end) = if dot(unit.to_array(), axis).abs() < NEAR_ZERO {
        (0., 1.)
    } else {
        let along = magnitude(dot(axis, edge));
        let beyond = |h: f32| magnitude(h - h.clamp(-half_height, half_height));
        (
            beyond(heights[0]) / along,
            (along - beyond(heights[1])) / along,
        )
    };
    let mut first = at(a, edge, start);
    let last = at(a, edge, end);
    let projected_first = vertical(axis, sub(first, center));
    let projected_last = vertical(axis, sub(last, center));
    let (squared, closest) = closest_to_origin(projected_first, projected_last);
    let radius_squared = radial * radial;
    if squared > radius_squared {
        return None;
    }
    let projected_length = length(sub(projected_last, projected_first));
    if projected_length.abs() >= NEAR_ZERO {
        let t = length(sub(closest, projected_first)) / projected_length;
        first = at(first, sub(last, first), t);
    }
    let offset = vertical(axis, sub(first, center));
    let normal_distance =
        ((-offset[1] * normal[1]) - offset[0] * normal[0]) - offset[2] * normal[2];
    let radial_room = (radius_squared - dot(offset, offset)).max(0.);
    let lift = ((radial_room + normal_distance * normal_distance).sqrt() - normal_distance).max(0.);
    Some(depth + ((normal_distance + lift) - radial))
}

impl Prism {
    /// Finite disk: `radius` in its plane and `half_height` on either side
    /// along unit `axis`. Returns push depth along the face and region byte
    /// (1 face, 2..=4 edges). `scale` multiplies the raw KCL thickness.
    pub fn disk_test(
        &self,
        center: Point,
        radius: f32,
        scale: f32,
        half_height: f32,
        axis: Point,
    ) -> Option<(f32, u8)> {
        let corners = [self.origin, self.corner(1), self.corner(2)];
        let relative = corners.map(|p| sub(p, center));
        let axial = relative.map(|v| dot(v, axis));
        if axial.iter().all(|&h| h < -half_height) || axial.iter().all(|&h| h > half_height) {
            return None;
        }
        let offset = sub(center, self.origin);
        let height = dot(offset, self.face);
        let (extent, cosine, radial) = support(self.face, axis, radius, half_height);
        if height >= extent || height <= -extent {
            return None;
        }
        let ratio = if extent.abs() < NEAR_ZERO {
            1.
        } else {
            height / extent
        };
        let max_axial = magnitude(axial[0])
            .max(magnitude(axial[1]))
            .max(magnitude(axial[2]));
        let correction = (max_axial - half_height).min(0.);
        let depth = (extent + cosine * correction) - height;
        if !(depth >= 0. || depth.abs() < NEAR_ZERO) {
            return None;
        }
        let planes = [
            dot(offset, self.edges[0]),
            dot(offset, self.edges[1]),
            dot(offset, self.edges[2]) - self.length,
        ];
        let projected = std::array::from_fn::<_, 3, _>(|i| vertical(axis, relative[i]));
        let edge_pairs = [(0, 2), (0, 1), (1, 2)];
        let radius_squared = radius * radius;
        if edge_pairs
            .iter()
            .all(|&(a, b)| closest_to_origin(projected[a], projected[b]).0 >= radius_squared)
            && planes.iter().any(|&d| d > 0.)
        {
            return None;
        }
        let lead = if planes[0] > planes[1] {
            if planes[0] > planes[2] { 0 } else { 2 }
        } else if planes[1] > planes[2] {
            1
        } else {
            2
        };
        let max_depth = self.thickness * scale;
        if planes[lead] <= 0. {
            return (max_depth >= depth).then_some((depth, 1));
        }
        let (edge_extent, _, _) = support(self.edges[lead], axis, radius, half_height);
        let edge_ratio = if edge_extent.abs() < NEAR_ZERO {
            1.
        } else {
            planes[lead] / edge_extent
        };
        if edge_ratio > ratio {
            return None;
        }
        let (a, b) = edge_pairs[lead];
        let depth = edge_depth(
            corners[a],
            corners[b],
            center,
            axis,
            self.face,
            [axial[a], axial[b]],
            half_height,
            radial,
            depth,
        )?;
        (max_depth >= depth).then_some((depth, 2 + lead as u8))
    }
}
