//! GrowFlowerSeed release, flight and support connection.
use super::*;

#[derive(Clone, Debug)]
pub struct Seed {
    pub id: String,
    pub position: Vec3,
    pub velocity: Vec3,
    pub(super) ground: Option<Vec3>,
    pub(super) settled: bool,
    pub(super) connecting: Option<usize>,
    pub(super) rebound: bool,
    pub(super) age: u32,
}

impl Player {
    /// GrowSeedUtil::throwFromPlayer and GrowFlowerSeed's release arrow check.
    pub(super) fn release_flower_seed(&mut self, scene: &Course) {
        let front = self.pose_axes().1.with_y(0.);
        let front = pose_math::try_normalize_or_zero(front).0;
        let direction = pose_math::turn_vec_to_vec_degree(front, Vec3::Y, 70.);
        let velocity =
            pose_math::try_normalize_or_zero(direction).0 * 28. + self.units_per_frame().with_y(0.);
        let player_position = self.position_cm();
        let Some(seed) = &mut self.carry.seed else {
            return;
        };
        seed.velocity = velocity;
        seed.settled = false;
        seed.connecting = None;
        seed.ground = None;
        seed.rebound = false;
        if let Some(mesh) = scene.mesh.as_deref() {
            let direction = pose_math::try_normalize_or_zero(velocity).0;
            let extent = pose_math::squared_length(seed.position + direction * 75.).sqrt();
            let arrow = direction * extent;
            if crate::collider::first_strike(
                mesh,
                mesh.relative_vec(seed.position - arrow),
                arrow,
                crate::collider::FaceFilter::Any,
            )
            .is_some()
            {
                seed.position = mesh.to_game_vec(player_position);
            }
        }
        self.carry.released = true;
    }
    /// Only placement/init state enters the replay. No carry observation file is loaded.
    pub fn init_flower_seed(&mut self, mesh: &crate::geometry::TriangleMesh) {
        self.carry.origin = mesh.to_game_vec(Vec3::ZERO);
        if let Some(source) = mesh
            .carry_sources
            .iter()
            .find(|s| s.class == "GrowFlowerSeed" && s.active)
        {
            self.carry.seed = Some(Seed {
                id: source.id.clone(),
                position: source.position,
                velocity: Vec3::new(0., -1.8 * 0.995, 0.),
                ground: None,
                settled: false,
                connecting: None,
                rebound: false,
                age: 1,
            });
        }
    }

    pub(super) fn update_flower_seed(&mut self, scene: &Course) {
        let Some(mesh) = scene.mesh.as_deref() else {
            return;
        };
        let Some(seed) = &mut self.carry.seed else {
            return;
        };
        if self.carry.held {
            if let Some(hold) = self.carry.hold {
                seed.position = look_joint::translation(&hold);
                seed.velocity = Vec3::ZERO;
            }
            return;
        }
        seed.age += 1;
        if seed.settled {
            if let Some(part) = seed.connecting {
                let frame = &mesh.parts[part];
                let local = crate::prism::transform_point(&frame.inverse, seed.position.to_array());
                seed.position = Vec3::from_array(crate::prism::transform_point(&frame.base, local));
            }
            return;
        }
        seed.position += seed.velocity;
        let centre = mesh.relative_vec(seed.position + Vec3::Y * 40.);
        let mut normal = None;
        let was_connecting = seed.connecting.is_some();
        for (triangle, hit) in crate::collider::sphere_strike_hits(mesh, centre, 40.) {
            let n = mesh.triangles[triangle as usize].normal;
            if hit.depth > 0. {
                seed.position += n * hit.depth;
            }
            if n.y > 0.5 {
                normal = Some(n);
                seed.connecting = mesh.triangles[triangle as usize]
                    .local
                    .map(|p| p.part as usize);
            }
        }
        if let Some(n) = normal {
            if seed.velocity.with_y(0.).length_squared() < 1.5 * 1.5 {
                seed.velocity = Vec3::ZERO;
                if was_connecting {
                    seed.settled = true;
                    seed.ground = Some(n);
                    return;
                }
            }
            seed.rebound = !seed.rebound;
            if seed.rebound {
                let d = pose_math::dot(n, seed.velocity);
                if d < 0. {
                    seed.velocity = (seed.velocity - n * d) * 0.7 - (n * d) * 0.7;
                }
            }
        }
        // GrowSeedStateWait's ground/air gravity and drag (0x18bafb0).
        seed.velocity.y -= 1.8;
        seed.velocity.x *= if normal.is_some() { 0.85 } else { 1. };
        seed.velocity.z *= if normal.is_some() { 0.85 } else { 1. };
        seed.velocity.y *= if normal.is_some() { 0.8 } else { 0.995 };
        seed.ground = normal;
    }
}
