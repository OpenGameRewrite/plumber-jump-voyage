//! Static 2DOnly collision through the ordinary multi-shape player collider.
use super::*;

/// Original 0x4367b8: local Z offsets +40, 0, -40; start40 up,
/// vector60 down and margin20. Z is the sprite's horizontal direction.
pub(super) fn line_arrow_start(index: usize, offset: Vec3) -> Vec3 {
    let base = Vec3::ZERO - Vec3::Y * -40.;
    let side = Vec3::Z * 40.;
    let start = match index {
        0 => side + base,
        1 => base,
        2 => base - side,
        _ => unreachable!("the keeper has three arrows"),
    };
    start + offset
}

/// rs::createCollisionPartsFilter2DOnly (0x5419c8), original predicate
/// CollisionPartsFilterOnlySpecialPurpose::isInvalidParts (0x8579ac).
pub(super) fn is_2d_part(mesh: &TriangleMesh, triangle: &MeshTriangle) -> bool {
    mesh.collision_2d_only.get(triangle.object as usize) == Some(&true)
}

/// CollisionParts::checkStrikeDisk (0x84952c), static-part path only.
/// A disk requires the raw prism and exact part frame; no world-triangle approximation.
pub(super) fn disk_hit(
    mesh: &TriangleMesh,
    triangle: &MeshTriangle,
    center: Vec3,
    radius: f32,
    half_height: f32,
    axis: Vec3,
) -> Option<SphereHit> {
    let local = triangle.local?;
    // The moving-part callback is a separate source path, still unported for disks.
    if mesh
        .key_move_parts
        .iter()
        .any(|s| s.part == local.part as usize && s.motion != Vec3::ZERO)
        || mesh
            .rotate_parts
            .iter()
            .any(|s| s.part == local.part as usize && s.executions > 0)
    {
        return None;
    }
    let frame = &mesh.parts[local.part as usize];
    let center_local = transform_point(&frame.inverse, center.to_array());
    let (axis_local, _) = try_normalize_or_zero(Vec3::from_array(rotate_vector(
        &frame.inverse,
        axis.to_array(),
    )));
    let (depth, region) = local.prism.disk_test(
        center_local,
        radius * frame.inverse_scale,
        frame.inverse_scale,
        half_height * frame.inverse_scale,
        axis_local.to_array(),
    )?;
    // Original calcDiskHitPos (0x851b84) uses the same face/edge projection
    // as calcSphereHitPos for these four regions, verified against both originals.
    let position = local.prism.sphere_hit_position(center_local, region);
    Some(SphereHit {
        motion: None,
        depth: frame.world_per_local() * depth,
        location: if region == 1 {
            Location::Face
        } else {
            Location::Edge(region - 1)
        },
        position: Vec3::from_array(transform_point(&frame.base, position)),
        center,
    })
}
