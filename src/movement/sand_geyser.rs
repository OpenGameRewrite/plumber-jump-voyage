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
            let length = match self.animation {
                1 => 15,
                2 => 20,
                _ => 30,
            };
            self.animation_frame += 1;
            if self.animation != 4 {
                self.animation_frame %= length;
            }
        }
        if self.step == 0 && self.phase != 3 {
            self.animation = if self.phase == 5 { 0 } else { self.phase };
            self.animation_frame = 0;
            self.animation_started = true;
        }
        let next = match self.phase {
            0 if self.step == self.up_start => Some(1),
            1 => {
                if self.step == 0 {
                    self.top = 0.;
                }
                (self.step == 180).then_some(2)
            }
            2 => {
                self.valid = true;
                self.top =
                    (self.target_y - self.position.y) * (self.step as f32 / self.up_time as f32);
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
        if let Some(next) = next {
            self.phase = next;
            self.step = 0;
        } else {
            self.step += 1;
        }
    }
    pub fn overlaps(&self, feet: Vec3, attack: bool) -> bool {
        if !self.valid {
            return false;
        }
        [(40., 40.), (75., 50.), (110., 40.)]
            .into_iter()
            .any(|(y, r)| {
                let p = feet + Vec3::Y * y;
                let d = p - self.centre;
                let broad = pose_math::dot(d, d) < (r + self.radius) * (r + self.radius);
                let horizontal = d.x * d.x + d.z * d.z;
                broad
                    && (!attack
                        || horizontal <= (175. + r) * (175. + r)
                            && p.y + r >= self.position.y - 200.
                            && p.y - r <= (self.position.y - 200.) + (self.top + 200.))
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
        if !self.grounded || self.mode == Move::SandGeyser {
            return;
        }
        let (Some(mesh), Some(part), Some((_, inverse))) =
            (&scene.mesh, self.ground_part, self.ground_frame)
        else {
            return;
        };
        let Some(source) = mesh.geysers.iter().find(|s| s.part == part) else {
            return;
        };
        // UpLoop validates/resets the Top collision: its previous matrix becomes its current
        // matrix, so the first update must not carry the UpStart animation back to rest.
        if source.phase == 2 && source.step == 1 {
            return;
        }
        let local = crate::prism::transform_point(&inverse, mesh.to_game(self.position_cm()));
        let end = mesh.relative_point(crate::prism::transform_point(&mesh.parts[part].base, local));
        if self.mode == Move::Idle {
            self.wait_platform_control(end - self.position_cm());
        }
        self.pos = end * UNIT;
        self.collider_end = Some((self.pos, end));
    }

    pub fn geyser_sources(&self) -> &str {
        &self.geyser.sources
    }
    pub(super) fn receive_sand_geyser(&mut self) {
        self.geyser.sender = self.geyser.next_sender.take();
        if self.is_dead() || self.mode == Move::AbyssFall || self.recovering() {
            self.geyser.sender = None;
            return;
        }
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
            self.touched = terrain::Touched {
                active: true,
                ..default()
            };
            self.mesh_ground = None;
            self.collider.invalidate(self.facing.x.atan2(self.facing.z));
        }
    }
    pub(super) fn queue_sand_geyser(&mut self, scene: &Course) {
        // LiveActor::movement attacks before its source nerve update. The source runs
        // after Mario, using this pose and the source state sampled on the same row;
        // executePreMovementNerveChange consumes the resulting latch next frame.
        let Some(mesh) = &scene.mesh else { return };
        self.geyser.sources = mesh
            .geysers
            .iter()
            .map(|s| {
                format!(
                    "{}/{}/{}/{:08x}/{:08x}/{}",
                    s.id,
                    s.phase,
                    s.step,
                    s.top.to_bits(),
                    s.radius.to_bits(),
                    u8::from(s.valid)
                )
            })
            .collect::<Vec<_>>()
            .join("|");
        self.geyser.next_sender = None;
        if self.is_dead() {
            return;
        }
        let feet = mesh.to_game_vec(self.position_cm());
        for source in &mesh.geysers {
            if source.overlaps(feet, true) {
                self.geyser.next_sender = Some((
                    source.position,
                    source.target_y,
                    (source.target_y - source.position.y) / source.up_time as f32,
                ));
            }
        }
    }
    pub(super) fn sand_geyser_step(&mut self, scene: &Course) {
        let absolute = scene.in_water_frame(self.position_cm());
        if let Some((host, target, speed)) = self.geyser.sender.take()
            && (target - absolute.y >= 0. || (target - absolute.y).abs() < 0.001)
        {
            let d = (host - absolute).with_y(0.);
            let n = pose_math::try_normalize_or_zero(d).0;
            let along = pose_math::dot(n, self.v);
            let attraction = if along <= 0. || along.abs() < 0.001 {
                0.2
            } else {
                along + 0.2
            };
            self.v = n * attraction.min(vector_lib::length(d)) + Vec3::Y * speed;
        } else {
            // kill calls al::onCollide (the ordinary collider reset); PlayerCollider's
            // saved pre-integration pose is retained. Its carry sweep corrects against
            // the moving Top first, then its own sweep integrates this exit impulse.
            self.v = Vec3::Y * 30.;
            self.go(Move::Fall, "Fall");
        }
    }
}

pub(super) fn self_check() {
    checks::source_cycle_and_raise_boundaries();
    checks::launch_latch_and_exit_sweep();
}

mod checks {
    use super::*;

    fn source() -> SandGeyserSource {
        SandGeyserSource {
            id: "fixture".into(),
            position: Vec3::new(0., -1200., 0.),
            target_y: 100.,
            up_start: 240,
            up_time: 60,
            end_wait: 120,
            phase: 0,
            step: 0,
            top: 0.,
            radius: 450.,
            centre: Vec3::new(0., -1200., 0.),
            valid: true,
            part: 0,
            animation: 0,
            animation_frame: 0,
            animation_started: false,
        }
    }

    #[cfg_attr(test, test)]
    pub(super) fn source_cycle_and_raise_boundaries() {
        let mut s = source();
        let mut transitions = Vec::new();
        for frame in -10..1200 {
            if s.step == 0 {
                transitions.push((frame, s.phase));
            }
            if frame == 474 {
                assert_eq!(s.top, 1300. * (61_f32 / 60.));
                assert_eq!((s.radius, s.centre.y), (850., -550.));
            }
            if frame == 776 {
                assert!(!s.valid);
            }
            if frame == 928 {
                assert_eq!(s.top, 0.);
                assert!(!s.valid);
            }
            if frame == 1109 {
                assert!(s.valid);
                assert_eq!(s.radius, 200.);
            }
            s.tick();
        }
        assert_eq!(
            transitions,
            [
                (-10, 0),
                (231, 1),
                (412, 2),
                (474, 3),
                (775, 4),
                (806, 5),
                (927, 1),
                (1108, 2),
                (1170, 3)
            ]
        );
        let s = source();
        assert!(s.overlaps(Vec3::new(0., -1200., 0.), true));
        assert!(!s.overlaps(Vec3::new(225., -1200., 0.), true));
        assert!(!s.overlaps(Vec3::new(0., -1100., 0.), true));
        assert!(!SandGeyserSource { valid: false, ..s }.overlaps(Vec3::new(0., -1200., 0.), true));
    }

    #[cfg_attr(test, test)]
    pub(super) fn launch_latch_and_exit_sweep() {
        let scene = Course::default();
        let mut p = Player {
            pos: Vec3::ZERO,
            ..default()
        };
        p.geyser.next_sender = Some((Vec3::ZERO, 100., 20.));
        p.receive_sand_geyser();
        assert_eq!(p.mode, Move::SandGeyser);
        assert!(p.geyser.next_sender.is_none());
        p.sand_geyser_step(&scene);
        assert_eq!(p.v, Vec3::Y * 20.);
        assert!(p.geyser.sender.is_none());
        p.sand_geyser_step(&scene);
        assert_eq!((p.mode, p.v), (Move::Fall, Vec3::Y * 30.));

        // A plane at an arbitrary height: no measured game correction is an input.
        // The carry starts below the floor and reaches the actor before the impulse.
        let mesh = TriangleMesh::from_corners(&[
            [
                Vec3::new(-100., 20., -100.),
                Vec3::new(-100., 20., 100.),
                Vec3::new(100., 20., -100.),
            ],
            [
                Vec3::new(100., 20., 100.),
                Vec3::new(100., 20., -100.),
                Vec3::new(-100., 20., 100.),
            ],
        ]);
        let mut collider = crate::collider::PlayerCollider::default();
        collider.carry_from(Vec3::ZERO);
        let outcome = collider.collide(&mesh, Vec3::Y * 15., Vec3::Y * 30., 0.);
        assert_eq!(outcome.pos, Vec3::Y * 50.);
        assert!(collider.grounded());
    }
}
