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
    flight_step: Option<u32>,
    support: Option<(u32, Vec3)>,
    contact: Option<Vec3>,
    offset: Vec3,
    revival: Option<u32>,
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
        seed.flight_step = Some(0);
        seed.support = None;
        seed.contact = None;
        seed.offset = self.carry.hold.map_or(Vec3::Y * 40., |m| Vec3::new(m[0][1],m[1][1],m[2][1]) * 40.);
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
                flight_step: None,
                support: None,
                contact: None,
                offset: Vec3::Y * 40.,
                revival: None,
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
        if self.carry.released {
            seed.update_released(mesh);
            return;
        }
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

impl Seed {
    fn update_released(&mut self, mesh: &crate::geometry::TriangleMesh) {
        if let Some(step) = &mut self.revival {
            // GrowSeedStateRevival uses actor steps, not time since pickup. The
            // 300-step random reappearance requires an independently seeded RNG.
            *step += 1;
            self.velocity = Vec3::ZERO;
            return;
        }
        let step = self.flight_step.unwrap_or(0);
        self.flight_step = Some(step + 1);
        if self.settled {
            if let Some(part) = self.connecting {
                let frame = &mesh.parts[part];
                let local = crate::prism::transform_point(&frame.inverse, self.position.to_array());
                self.position = Vec3::from_array(crate::prism::transform_point(&frame.base, local));
            }
            return;
        }
        if step > 0 {
            let ground = self.support.filter(|(_, n)| n.y > 0.5);
            if ground.is_some() && pose_math::squared_length(self.velocity.with_y(0.)) < 1.5 * 1.5 {
                self.velocity = Vec3::ZERO;
                self.connecting = ground.and_then(|(t,_)| mesh.triangles[t as usize].local.map(|p|p.part as usize));
                if self.ground.is_some() {
                    self.settled = true;
                    return;
                }
            }
            if let Some(normal) = ground.map(|(_, n)| n).or(self.contact) {
                self.rebound = !self.rebound;
                if self.rebound {
                    let dot = pose_math::dot(normal,self.velocity);
                    if dot < 0. {
                        self.velocity = (self.velocity - normal * dot) * 0.7 - (normal * dot) * 0.7;
                    }
                }
            }
            self.velocity.y -= 1.8;
            self.velocity.x *= if ground.is_some() {0.85} else {1.};
            self.velocity.z *= if ground.is_some() {0.85} else {1.};
            self.velocity.y *= if ground.is_some() {0.8} else {0.995};
            self.ground = ground.map(|(_, n)| n);
        }
        // exeThrow retains the held actor matrix. exeWait restores its upright
        // matrix before the next collision; joint rotation belongs to the model.
        let offset = if step == 0 { self.offset } else { Vec3::Y * 40. };
        let mut centre = self.position + offset;
        self.support = None;
        self.contact = None;
        let mut remaining = self.velocity;
        // al::Collider::collide, static sphere path: includes the initial sample,
        // componentwise correction extrema, then slides the remaining motion.
        let mut sweep = SphereSweep::new(centre, remaining, 40.);
        let mut seen = Vec::new();
        loop {
            let hits = crate::collider::sphere_strike_hits(mesh, mesh.relative_vec(sweep.position()),40.);
            if hits.is_empty() {
                if sweep.done() { centre = sweep.position(); break; }
                sweep.advance();
                continue;
            }
            let mut min = Vec3::ZERO;
            let mut max = Vec3::ZERO;
            let mut normal = Vec3::ZERO;
            for (triangle, hit) in hits {
                let n = mesh.triangles[triangle as usize].normal;
                if n.y >= 0.34202015 {
                    self.support = Some((triangle,n));
                } else if n.y.abs() < 0.34202015 {
                    self.contact = Some(n);
                }
                let fix = n * hit.depth;
                min = min.min(fix); max = max.max(fix);
                normal += n;
            }
            let fix = min + max;
            let at = sweep.position() + fix;
            remaining = sweep.remaining();
            let axis = if pose_math::squared_length(fix) < 0.001 * 0.001 { normal } else { fix };
            let axis = pose_math::try_normalize_or_zero(axis).0;
            let d = pose_math::dot(axis,remaining);
            if d < 0. { remaining -= axis * d; }
            if pose_math::dot(self.velocity,remaining) < 0. || sweep.done() || seen.contains(&at) {
                centre = at; break;
            }
            seen.push(at);
            sweep = SphereSweep::new(at,remaining,40.);
            sweep.advance();
        }
        // LiveActor adds the collider's displacement to the original actor
        // translation instead of subtracting a possibly rotated offset again.
        self.position += centre - (self.position + offset);
        if mesh.death.volume_at(self.position).is_some() {
            self.revival = Some(0);
        }
    }
}

struct SphereSweep { start: Vec3, motion: Vec3, t: f32, step: f32 }
impl SphereSweep {
    fn new(start: Vec3, motion: Vec3, radius: f32) -> Self {
        let length = pose_math::squared_length(motion).sqrt();
        Self { start, motion, t: 0., step: if length > 0. { (radius * 0.9).min(35.) / length } else {1.} }
    }
    fn position(&self) -> Vec3 { self.start + self.motion * self.t }
    fn remaining(&self) -> Vec3 { self.motion * (1. - self.t) }
    fn done(&self) -> bool { self.t >= 1. }
    fn advance(&mut self) { self.t = (self.t + self.step).min(1.); }
}
