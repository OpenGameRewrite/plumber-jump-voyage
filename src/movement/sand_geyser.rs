//! SandGeyser's timed source and PlayerStateSandGeyser (1.0.0 0x303d50, 0x543e84).
use super::*;

#[derive(Clone, Debug)]
pub struct SandGeyserSource {
    pub id: String,
    pub position: Vec3,
    pub target_y: f32,
    pub up_start: u32,
    pub up_time: u32,
    pub end_wait: u32,
    pub phase: u8,
    pub step: u32,
    pub top: f32,
    pub radius: f32,
    pub centre: Vec3,
    pub valid: bool,
    pub part: usize,
    pub animation: u8,
    pub animation_frame: u32,
    pub animation_started: bool,
}
impl SandGeyserSource {
    pub fn tick(&mut self) {
        if self.animation_started {
            let length = match self.animation { 1 => 15, 2 => 20, _ => 30 };
            self.animation_frame += 1;
            if self.animation != 4 { self.animation_frame %= length; }
        }
        if self.step == 0 && self.phase != 3 {
            self.animation = if self.phase == 5 { 0 } else { self.phase };
            self.animation_frame = 0;
            self.animation_started = true;
        }
        let next = match self.phase {
            0 if self.step == self.up_start => Some(1),
            1 => {
                if self.step == 0 { self.top = 0.; }
                (self.step == 180).then_some(2)
            }
            2 => {
                self.valid = true;
                self.top = (self.target_y - self.position.y) * (self.step as f32 / self.up_time as f32);
                if self.step <= self.up_time {
                    self.radius = (self.top * 0.5 + 200.).max(175.);
                    self.centre.y = (self.position.y - 200.) + self.radius;
                }
                (self.step > self.up_time).then_some(3)
            }
            3 => (self.step == 300).then_some(4),
            4 => {
                self.valid = false;
                // UpEndDay/Night reaches animation end at executed step 30.
                (self.step == 30).then_some(5)
            }
            5 => (self.step == self.end_wait).then_some(1),
            _ => None,
        };
        if let Some(next) = next { self.phase = next; self.step = 0; }
        else { self.step += 1; }
    }
    pub fn overlaps(&self, feet: Vec3, attack: bool) -> bool {
        if !self.valid { return false; }
        [(40., 40.), (75., 50.), (110., 40.)].into_iter().any(|(y,r)| {
            let p = feet + Vec3::Y * y;
            let d = p - self.centre;
            let broad = pose_math::dot(d,d) < (r+self.radius)*(r+self.radius);
            let horizontal = d.x*d.x + d.z*d.z;
            broad && (!attack || horizontal <= (175.+r)*(175.+r)
                && p.y + r >= self.position.y - 200.
                && p.y - r <= self.position.y + self.top)
        })
    }
}

#[derive(Debug, Default)]
pub(super) struct GeyserControl {
    sender: Option<(Vec3, f32, f32)>,
    next_sender: Option<(Vec3, f32, f32)>,
    pub sources: String,
}
impl Player {
    pub(super) fn follow_geyser_ground(&mut self, scene: &Course) {
        if !self.grounded || self.mode == Move::SandGeyser { return; }
        let (Some(mesh), Some(part), Some((_, inverse))) = (&scene.mesh, self.ground_part, self.ground_frame) else { return };
        if !mesh.geysers.iter().any(|s| s.part == part) { return; }
        let local = crate::prism::transform_point(&inverse, mesh.to_game(self.position_cm()));
        let end = mesh.relative_point(crate::prism::transform_point(&mesh.parts[part].base, local));
        self.pos = end * UNIT;
        self.collider_end = Some((self.pos, end));
    }

    pub fn geyser_sources(&self) -> &str { &self.geyser.sources }
    pub(super) fn receive_sand_geyser(&mut self) {
        self.geyser.sender = self.geyser.next_sender.take();
        if self.is_dead() || self.mode == Move::AbyssFall || self.recovering() { self.geyser.sender = None; return; }
        if self.geyser.sender.is_some() && self.mode != Move::SandGeyser {
            self.go(Move::SandGeyser, "Fall");
            self.nerve.deferred = false;
            self.nerve.renewed = false;
            self.nerve.step = 0;
            self.nerve.actor = "SandGeyser";
            self.frame = 0;
            self.sensor_restart = true;
            self.grounded = false;
            self.hop_contact = false;
            self.touched = terrain::Touched { active: true, ..default() };
            self.mesh_ground = None;
            self.collider.invalidate(self.facing.x.atan2(self.facing.z));
        }
    }
    pub(super) fn queue_sand_geyser(&mut self, scene: &Course) {
        let Some(mesh) = &scene.mesh else { return };
        self.geyser.sources = mesh.geysers.iter().map(|s| format!("{}/{}/{}/{:08x}/{:08x}/{}",s.id,s.phase,s.step,s.top.to_bits(),s.radius.to_bits(),u8::from(s.valid))).collect::<Vec<_>>().join("|");
        self.geyser.next_sender = None;
        if self.is_dead() { return; }
        let feet = mesh.to_game_vec(self.position_cm());
        for source in &mesh.geysers {
            if source.overlaps(feet, true) {
                self.geyser.next_sender = Some((source.position, source.target_y,
                    (source.target_y-source.position.y)/source.up_time as f32));
            }
        }
    }
    pub(super) fn sand_geyser_step(&mut self, scene: &Course) {
        let absolute = scene.in_water_frame(self.position_cm());
        if let Some((host, target, speed)) = self.geyser.sender.take()
            && (target-absolute.y >= 0. || (target-absolute.y).abs() < 0.001)
        {
            let d = (host-absolute).with_y(0.);
            let n = pose_math::try_normalize_or_zero(d).0;
            let along = pose_math::dot(n,self.v);
            let attraction = if along <= 0. || along.abs() < 0.001 { 0.2 } else { along+0.2 };
            self.v = n*attraction.min(vector_lib::length(d)) + Vec3::Y*speed;
        } else {
            self.v = Vec3::Y * 30.;
            self.go(Move::Fall,"Fall");
        }
    }
}
