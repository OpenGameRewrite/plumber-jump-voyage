//! Placement-bound flower objects and the player's independent carry keeper.
use super::push_receiver::{PushReceiver, calc_push_trans};
use super::*;

#[derive(Clone, Debug)]
pub struct Source {
    pub id: String,
    pub class: String,
    pub position: Vec3,
    pub active: bool,
}

pub fn sources(root: &serde_json::Value) -> Vec<Source> {
    if root["stage"] != "SandWorldHomeStage" || root["scenario"] != 1 {
        return Vec::new();
    }
    root["carry_objects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let p = &item["placement"];
            let class = p["UnitConfigName"].as_str()?;
            if p["Id"] != item["id"]
                || item["parent"] != "obj6326"
                || !matches!(class, "GrowFlowerPot" | "GrowFlowerSeed")
                || p["Scale"] != serde_json::json!({"X":1.,"Y":1.,"Z":1.})
                || (class == "GrowFlowerPot"
                    && item["sensors"] != serde_json::json!([{"Name":"Body","Radius":70.,"Y":50.}]))
            {
                return None;
            }
            Some(Source {
                id: item["id"].as_str()?.into(),
                class: class.into(),
                active: item["initial_alive"] == true,
                position: Vec3::new(
                    p["Translate"]["X"].as_f64()? as f32,
                    p["Translate"]["Y"].as_f64()? as f32,
                    p["Translate"]["Z"].as_f64()? as f32,
                ),
            })
        })
        .collect()
}

#[derive(Clone, Debug, Default)]
pub struct Carry {
    pub push: PushReceiver,
    pub diagnostic: String,
    pub seed: Option<Seed>,
    pub held: bool,
    pub phase: u8,
    pub step: u32,
    pub inhibition: u32,
    pub upper: Option<(&'static str, f32)>,
    pub hold: Option<look_joint::Mat34>,
    pub picked_up: bool,
    pub unsupported_release: bool,
}

impl Player {
    /// Hit sensors update before Mario's movement; clear happens at its end (0x41f06c).
    pub(super) fn flower_sensor_phase(&mut self, scene: &Course) {
        let Some(mesh) = scene.mesh.as_deref() else {
            return;
        };
        self.carry.diagnostic.clear();
        if mesh.carry_sources.is_empty() {
            return;
        }
        let body = mesh.to_game_vec(self.position_cm()) + self.pose_axes().0 * 75.;
        // receivePushMsg 0x4293b0: Hack/Demo are absent from this controller;
        // 2D, PoleClimb, GrabCeil and recovery do not receive ordinary pushes.
        if matches!(self.mode, Move::Pole | Move::GrabCeil)
            || self.recovering()
            || self.recovery.push_invalid > 0
        {
            return;
        }
        for source in &mesh.carry_sources {
            if source.class != "GrowFlowerPot" {
                continue;
            }
            let sender = source.position + Vec3::Y * 50.;
            if pose_math::squared_length(body - sender) >= (50. + 70.) * (50. + 70.) {
                continue;
            }
            let push =
                calc_push_trans(body, 50., sender, 70., Vec3::NEG_Y, self.pose_axes().1, 20.);
            // GrowFlowerPot 0x28f040: Push is unhandled; its fallback PushToPlayer
            // reaches the receiver only for the player's Body sensor.
            self.carry.push.receive(push, false);
        }
    }

    pub(super) fn receive_sensor_push_velocity(&mut self, velocity: Vec3, at: Vec3) -> Vec3 {
        let wall = self
            .touched
            .wall_contact
            .as_ref()
            .filter(|w| w.at_face)
            .map(|w| {
                (
                    w.normal,
                    (55. - pose_math::dot(w.normal, at - w.position)).max(0.),
                )
            });
        let pushed = self.carry.push.pushed_velocity(velocity, wall);
        self.carry.diagnostic = format!(
            "push={};velocity={};result={}",
            hex_vec(self.carry.push.min + self.carry.push.max),
            hex_vec(velocity),
            hex_vec(pushed)
        );
        pushed
    }
}

fn hex_vec(v: Vec3) -> String {
    v.to_array()
        .map(|x| format!("{:08x}", x.to_bits()))
        .join("/")
}

#[derive(Clone, Debug)]
pub struct Seed {
    pub id: String,
    pub position: Vec3,
    pub velocity: Vec3,
    ground: Option<Vec3>,
    settled: bool,
    rebound: bool,
    age: u32,
}

impl Player {
    pub fn is_carry(&self) -> bool {
        self.carry.held
    }

    /// Only placement/init state enters the replay. No carry observation file is loaded.
    pub fn init_flower_seed(&mut self, mesh: &crate::geometry::TriangleMesh) {
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
                seed.position = mesh.to_game_vec(look_joint::translation(&hold));
                seed.velocity = Vec3::ZERO;
            }
            return;
        }
        seed.age += 1;
        if seed.settled {
            return;
        }
        seed.position += seed.velocity;
        let centre = mesh.relative_vec(seed.position + Vec3::Y * 40.);
        let mut normal = None;
        for (triangle, hit) in crate::collider::sphere_strike_hits(mesh, centre, 40.) {
            let n = mesh.triangles[triangle as usize].normal;
            if hit.depth > 0. {
                seed.position += n * hit.depth;
            }
            if n.y > 0.5 {
                normal = Some(n);
            }
        }
        if let Some(n) = normal {
            if seed.velocity.with_y(0.).length_squared() < 1.5 * 1.5 {
                seed.velocity = Vec3::ZERO;
                seed.settled = true;
                seed.ground = Some(n);
                return;
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

    pub(super) fn carry_sensor_phase(&mut self, o: &Orders, scene: &Course) {
        self.carry.picked_up = false;
        if self.carry.held
            || self.carry.inhibition > 0
            || !o.throw_held
            || !matches!(
                self.mode,
                Move::Idle | Move::Walk | Move::Run | Move::Jump | Move::Fall
            )
        {
            return;
        }
        let (Some(mesh), Some(seed)) = (scene.mesh.as_deref(), self.carry.seed.as_ref()) else {
            return;
        };
        if seed.age < 31 {
            return;
        }
        let feet = mesh.to_game_vec(self.position_cm());
        // Carry (Action edge) and Body (held Action) are distinct attack paths.
        let (height, radius, target_radius) = if o.throw_pressed {
            (75., 100., 75.)
        } else {
            (75., 50., 50.)
        };
        let centre = feet + self.pose_axes().0 * height;
        let target = seed.position + Vec3::Y * 35.;
        if pose_math::squared_length(centre - target)
            >= (radius + target_radius) * (radius + target_radius)
        {
            return;
        }
        if crate::collider::first_strike(
            mesh,
            mesh.relative_vec(target),
            feet - target,
            crate::collider::FaceFilter::Any,
        )
        .is_some()
        {
            return;
        }
        // GrowFlowerSeed accepts FrontWallKeep in Wait after its pickup timer.
        self.carry.held = true;
        self.carry.phase = 1;
        self.carry.step = 0;
        self.carry.upper = Some(("CarryFrontStart", 0.));
        self.carry.picked_up = true;
        self.pre_throw = 0;
        self.events.push(Fx::CarryStart);
        if self.mode == Move::Idle && self.clip() != "Wait" {
            self.play("Wait");
        }
    }

    /// Keeper update 0x42e654 / Start 0x42ef20 / Carry 0x42f028.
    pub(super) fn update_carry(&mut self, o: &Orders) {
        self.carry.inhibition = self.carry.inhibition.saturating_sub(1);
        if !self.carry.held {
            return;
        }
        self.pre_throw = 0;
        if o.throw_pressed && !self.carry.picked_up {
            // Part 2 owns throw/release; expose the unsupported request.
            self.carry.unsupported_release = true;
        }
        if !self.carry.picked_up {
            match self.carry.upper {
                Some(("CarryFrontStart", frame)) if frame >= 8. => {
                    if self.carry.phase == 1 {
                        self.carry.phase = 2;
                        self.carry.step = 0;
                    } else {
                        self.carry.upper = Some(("CarryFront", 0.));
                    }
                }
                Some((name, frame)) => {
                    self.carry.upper = Some((
                        name,
                        if name == "CarryFront" {
                            (frame + 1.) % 60.
                        } else {
                            frame + 1.
                        },
                    ))
                }
                None => {}
            }
        }
        self.carry.step += u32::from(
            self.carry.phase != 2 || self.carry.upper.is_some_and(|(n, _)| n == "CarryFront"),
        );
    }

    pub(super) fn record_hold_pose(&mut self, joints: &crate::skeleton_pose::Matrices) {
        if !self.carry.held {
            return;
        }
        use crate::skeleton_pose::Joint;
        let spine = joints[Joint::Spine2 as usize];
        let left = look_joint::translation(&joints[Joint::HandL as usize]);
        let right = look_joint::translation(&joints[Joint::HandR as usize]);
        let position = (left + right) * 0.5;
        let rotation = hold_matrix(spine, position);
        let offset = Vec3::new(0., -30., 40.);
        let mut pose = rotation;
        for row in &mut pose {
            row[3] = row[2].mul_add(offset.z, row[1].mul_add(offset.y, row[0] * offset.x)) + row[3];
        }
        self.carry.hold = Some(pose);
    }

    pub fn carry_diagnostic(&self) -> String {
        if self.carry.seed.is_none() && self.carry.diagnostic.is_empty() {
            return String::new();
        }
        let mut out = self.carry.diagnostic.clone();
        out.push_str(&format!(
            ";keeper={}/{}/{}",
            self.carry.phase,
            self.carry.step,
            u8::from(self.carry.held)
        ));
        if let Some(seed) = &self.carry.seed {
            out.push_str(&format!(";seed={}/{}", seed.id, hex_vec(seed.position)));
        }
        if let Some(m) = self.carry.hold {
            out.push_str(&format!(
                ";held={}",
                m.iter()
                    .flatten()
                    .map(|x| format!("{:08x}", x.to_bits()))
                    .collect::<Vec<_>>()
                    .join("/")
            ));
        }
        out
    }
}

/// calcPlayerHoldMtx 0x5727e0: Spine2, local -90 Z then -90 Y, hand midpoint.
fn hold_matrix(spine: look_joint::Mat34, position: Vec3) -> look_joint::Mat34 {
    let s = -std::f32::consts::FRAC_1_SQRT_2;
    let z = joint_math::rotation_rows(Quat::from_xyzw(0., 0., s, -s));
    let y = joint_math::rotation_rows(Quat::from_xyzw(0., s, 0., -s));
    let mut rows = spine.map(|r| [r[0], r[1], r[2]]);
    rows = joint_math::mul_rows(&joint_math::mul_rows(&rows, &z), &y);
    std::array::from_fn(|i| [rows[i][0], rows[i][1], rows[i][2], position[i]])
}
