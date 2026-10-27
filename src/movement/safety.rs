//! Static collision consumers of PlayerRecoverySafetyPoint. Actor replies are explicit data:
//! a solid's floor code alone cannot answer AskSafetyPoint for its host.
use super::recovery_dead::SafetyPoint;
use super::vector_lib::{length, verticalize_vec};
use super::*;

fn safe_code(codes: &CollisionCodes) -> bool {
    codes.floor == "Ground"
        || !matches!(codes.floor.as_str(), "Poison" | "Poison2D" | "DamageFire" | "DamageFire2D" | "Needle" | "Pole" | "Pole10" | "Pole20" | "Pole30Plus" | "GrabCeil" | "Slide" | "SandSink")
            && codes.wall != "NoClimbPole"
}
fn safe_hit(mesh: &TriangleMesh, hit: &crate::collider::Strike) -> bool {
    let triangle = &mesh.triangles[hit.triangle as usize];
    mesh.safety_hosts.get(triangle.object as usize).copied().unwrap_or(false)
        && pose_math::acosf(pose_math::dot(hit.normal, Vec3::Y).clamp(-1., 1.)) / pose_math::DEGREES_TO_RADIANS < 60.
        && safe_code(mesh.codes_of(hit.triangle))
}
fn first(mesh: &TriangleMesh, from: Vec3, direction: Vec3) -> Option<crate::collider::Strike> {
    crate::collider::arrow_strikes(mesh, from, direction, false)
        .into_iter().min_by(|a,b| a.distance.total_cmp(&b.distance))
}
impl Player {
    pub(super) fn update_recovery_point(&mut self, scene: &Course) {
        if self.recovering() || self.is_dead() { return; }
        let at = self.position_cm();
        if let Some(mesh) = &scene.mesh {
            let absolute = mesh.to_game_vec(at);
            if mesh.recovery.volume_at(absolute).is_some() {
                self.recovery.area_active = true;
            }
            if mesh.recovery_ban.volume_at(absolute).is_some() { return; }
            // noticeRequestSafetyPoint: +10 -> -100, with a ground-only filter.
            let hit = first(mesh, at + Vec3::Y * 10., Vec3::NEG_Y * 110.);
            if self.grounded && hit.as_ref().is_some_and(|h| safe_hit(mesh, h)) {
                let hit = hit.unwrap();
                self.set_safety_point(hit.position, hit.normal, Vec3::NEG_Y);
            } else {
                self.slide_safety_point(mesh, at, false);
            }
            if self.recovery.valid() && self.recovery.safety.is_some_and(|s| mesh.recovery_invalidate.volume_at(mesh.to_game_vec(s.position)).is_some()) {
                self.recovery.safety = None;
                self.recovery.area_active = false;
            }
        } else if self.grounded && !scene.hot(self.pos) {
            // The toy's own static solids answer AskSafetyPoint; moving boxes do not.
            if scene.solids.iter().any(|s| !s.moving && (s.center.x-self.pos.x).abs() <= s.half.x && (s.center.z-self.pos.z).abs() <= s.half.z && self.pos.y == s.top(self.pos.x)) {
                self.set_safety_point(at, self.floor_normal().unwrap_or(Vec3::Y), Vec3::NEG_Y);
            }
        }
    }
    fn slide_safety_point(&mut self, mesh: &TriangleMesh, dangerous: Vec3, skip_if_valid: bool) {
        let Some(safety) = self.recovery.safety else { return };
        let direction = verticalize_vec(safety.normal, safety.position - dangerous);
        let distance = length(direction);
        let (direction, normalized) = pose_math::try_normalize_or_zero(direction);
        if distance > 100. || !normalized { return; }
        let gravity = safety.gravity;
        let offset = gravity * 50.;
        let ray = offset * 2.;
        if skip_if_valid && first(mesh, safety.position - direction * distance - offset, ray).is_some_and(|h| safe_hit(mesh, &h)) { return; }
        let from = safety.position + direction * (100. - distance).max(0.) - offset;
        if let Some(hit) = first(mesh, from, ray).filter(|h| safe_hit(mesh, h))
            && first(mesh, safety.position - gravity * 5., hit.position - safety.position).is_none() {
                self.recovery.safety = Some(SafetyPoint { position: hit.position, normal: hit.normal, ..safety });
            }
    }
}
