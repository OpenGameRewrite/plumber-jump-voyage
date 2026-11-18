//! KickStone's front hold, release, and motion on static ground (SMO 1.0.0).
//! The shared random draw in the landing reaction affects its presentation matrix.
use super::actor_collider::ActorCollider;
use super::carry::hex_vec;
use super::pose_math::{self, dot, squared_length, try_normalize_or_zero};
use super::*;

const RADIUS: f32 = 60.;
const HOLD_SCALE: f32 = 0.3;
pub(super) const HOLD_OFFSET: Vec3 = Vec3::new(0., 30., 65.);

#[derive(Clone, Debug)]
pub struct Source {
    pub(super) id: String,
    pub(super) position: Vec3,
    pub(super) joint: Quat,
}

pub fn sources(root: &serde_json::Value) -> Vec<Source> {
    if root["stage"] != "ForestWorldHomeStage" || root["scenario"] != 1 {
        return Vec::new();
    }
    root["carry_objects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let p = &s["placement"];
            if s["id"] != "obj1837"
                || p["Id"] != s["id"]
                || s["parent"] != ""
                || p["UnitConfigName"] != "KickStone"
                || p["MoveType"] != 0
                || p["IsRandomKickCount"] != false
                || p["KickCountMax"] != 3
                || p["IsConnectToCollision"] != false
                || p["ItemType"] != "None"
                || p["Rotate"] != serde_json::json!({"X":0.,"Y":0.,"Z":0.})
                || p["Scale"] != serde_json::json!({"X":1.,"Y":1.,"Z":1.})
                || s["initial_phase"] != "init"
                || s["initial_alive"] != true
                || s["initial_kick_count"] != 3
                || s["sensors"]
                    != serde_json::json!([{"Name":"Body","Radius":70.,"Type":"EnemyBody"}])
                || s["collider"] != serde_json::json!({"Radius":60.,"Y":5.})
            {
                return None;
            }
            let q = s["initial_joint_quat"].as_array()?;
            if q.len() != 4 {
                return None;
            }
            Some(Source {
                id: s["id"].as_str()?.into(),
                position: Vec3::new(
                    p["Translate"]["X"].as_f64()? as f32,
                    p["Translate"]["Y"].as_f64()? as f32,
                    p["Translate"]["Z"].as_f64()? as f32,
                ),
                joint: Quat::from_xyzw(
                    q[0].as_f64()? as f32,
                    q[1].as_f64()? as f32,
                    q[2].as_f64()? as f32,
                    q[3].as_f64()? as f32,
                ),
            })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Nerve {
    Wait,
    Hold,
    Fall,
    Roll,
}

#[derive(Clone, Debug)]
pub struct Stone {
    pub id: String,
    pub position: Vec3,
    pub velocity: Vec3,
    joint: Quat,
    pose: [Vec3; 3],
    hold_rotation: joint_math::Rows,
    collider: ActorCollider,
    nerve: Nerve,
    pending: Option<Nerve>,
    step: u32,
    hold_updates: u32,
    pub push: Vec3,
    pub limit: &'static str,
    rolling_forward: bool,
    roll_ground_grace: u32,
}

impl Stone {
    pub(super) fn new(source: &Source) -> Self {
        Self {
            id: source.id.clone(),
            position: source.position,
            velocity: Vec3::ZERO,
            joint: source.joint,
            pose: [Vec3::X, Vec3::Y, Vec3::Z],
            hold_rotation: [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
            collider: ActorCollider::new(RADIUS, 5., source.position),
            nerve: Nerve::Wait,
            pending: None,
            step: 0,
            hold_updates: 0,
            push: Vec3::ZERO,
            limit: "",
            rolling_forward: false,
            roll_ground_grace: 0,
        }
    }

    pub(super) fn can_pick_up(&self) -> bool {
        self.nerve == Nerve::Wait && self.pending.is_none() && self.limit.is_empty()
    }

    /// receiveMsg 0x2ab494 only calls tryStartCarryFront, never FrontWallKeep.
    pub(super) fn pick_up(&mut self) {
        self.hold_rotation = rotation_from_joint(self.joint);
        self.pending = Some(Nerve::Hold);
        self.velocity = Vec3::ZERO;
        self.hold_updates = 0;
        self.push = Vec3::ZERO;
    }

    pub(super) fn update(
        &mut self,
        mesh: &crate::geometry::TriangleMesh,
        hold: Option<&look_joint::Mat34>,
        player_grounded: bool,
        player_position: Vec3,
        player_front: Vec3,
    ) {
        if !self.limit.is_empty() {
            return;
        }
        if mesh.water.is_water(self.position) {
            self.limit = "stone_water_unavailable";
            return;
        }
        if mesh.death.volume_at(self.position).is_some() {
            self.limit = "stone_restart_unavailable";
            return;
        }
        if let Some(nerve) = self.pending.take() {
            self.nerve = nerve;
            self.step = 0;
            if nerve == Nerve::Hold {
                // The actor enters its independent hold state on its next movement.
                self.collider.set_radius((RADIUS * HOLD_SCALE).min(40.));
                self.collider.invalidate(self.position, self.pose[1]);
            } else if nerve == Nerve::Roll {
                self.roll_ground_grace = 5;
            } else if nerve == Nerve::Wait {
                // The static connector's resetPosition at 0x2abf98 clears contact history.
                self.collider.invalidate(self.position, self.pose[1]);
            }
        }
        match self.nerve {
            // 0x2abf4c: offCollide on initial Wait, stationary connector after Roll.
            Nerve::Wait => {}
            Nerve::Hold => {
                let Some(hold) = hold else {
                    return;
                };
                self.position = look_joint::translation(hold);
                let rows: joint_math::Rows = std::array::from_fn(|r| {
                    std::array::from_fn(|c| {
                        hold[r][2].mul_add(
                            self.hold_rotation[2][c],
                            hold[r][1].mul_add(
                                self.hold_rotation[1][c],
                                hold[r][0] * self.hold_rotation[0][c],
                            ),
                        )
                    })
                });
                self.pose = std::array::from_fn(|c| Vec3::new(rows[0][c], rows[1][c], rows[2][c]));
                self.joint = pose_math::to_quat(self.pose);
                // updateCollider 0x2b6f90: radius ramp, zero-velocity sweep, then feedback.
                let ramp = self.hold_updates.saturating_sub(3) as f32 * 10. + 40.;
                self.collider.set_radius((RADIUS * HOLD_SCALE).min(ramp));
                let mut push = self
                    .collider
                    .collide(mesh, self.position, self.pose[1], Vec3::ZERO);
                if self.collider.floor.is_present() {
                    if player_grounded {
                        push = vector_lib::verticalize_vec(Vec3::NEG_Y, push);
                    } else {
                        let delta = player_position - self.collider.floor.hit.position;
                        let (direction, valid) =
                            try_normalize_or_zero(vector_lib::verticalize_vec(Vec3::NEG_Y, delta));
                        let direction = if valid { direction } else { -player_front };
                        push += direction * RADIUS * HOLD_SCALE * 0.5;
                    }
                }
                self.push = push;
                self.hold_updates = (self.hold_updates + 1).min(1000);
            }
            Nerve::Fall | Nerve::Roll => {
                if matches!(self.floor_code(mesh), "DamageFire" | "Needle" | "Poison") {
                    // 0x2ac020 can destroy the stone before the movement state runs.
                    self.limit = "stone_hazard_unavailable";
                    return;
                }
                if self.nerve == Nerve::Fall {
                    self.exe_fall(mesh);
                } else {
                    self.exe_roll(mesh);
                }
                if !self.limit.is_empty() || self.pending == Some(Nerve::Wait) {
                    return;
                }
                self.position +=
                    self.collider
                        .collide(mesh, self.position, self.pose[1], self.velocity);
            }
        }
        self.step += 1;
    }

    fn floor_code<'a>(&self, mesh: &'a crate::geometry::TriangleMesh) -> &'a str {
        if !self.collider.floor.is_present() {
            return "";
        }
        let triangle = &mesh.triangles[self.collider.floor.hit.face as usize];
        &mesh.codes[triangle.attribute as usize].floor
    }

    /// 0x2ac15c with the MoveType0 parameters at 0x1d0a728.
    fn exe_fall(&mut self, mesh: &crate::geometry::TriangleMesh) {
        self.velocity.y -= 2.;
        self.velocity *= 0.99;
        let roll_scale = if self.rolling_forward { 0.7 } else { -0.4 };
        self.joint = roll_quat(self.joint, self.velocity * roll_scale, Vec3::Y, RADIUS);
        if self.collider.wall.is_present() {
            self.limit = "stone_wall_rebound_unavailable";
            return;
        }
        let normal = self.collider.floor.hit.normal;
        // isOnGround(0), 0x8d8074, also checks the direction of travel.
        if !self.collider.floor.is_present() || dot(self.velocity, normal) > 0. {
            return;
        }
        self.rolling_forward = true; // 0x2ac290 selects 0.7 in the next Fall step.
        let (parallel, tangent) = vector_lib::separate_parallel_vertical(normal, self.velocity);
        let rebound = if self.floor_code(mesh) == "Slide" {
            // 0x2ac4b8–0x2ac54c: full rebound, at least 15 along the floor normal.
            if vector_lib::length(parallel) < 15. {
                normal * 15.
            } else {
                -parallel
            }
        } else {
            parallel * -0.55
        };
        if vector_lib::length(rebound) < 6. {
            self.velocity = tangent;
            self.pending = Some(Nerve::Roll);
        } else {
            // 0x2ac5e8 only feeds the hit reaction matrix. 0x2ac738 sets velocity from
            // the tangent and rebound computed before the reaction call.
            self.velocity = tangent + rebound;
        }
    }

    /// 0x2acb10: contact rebound, ground-directed gravity, drag, roll, and rest.
    fn exe_roll(&mut self, mesh: &crate::geometry::TriangleMesh) {
        let mut normals = Vec3::ZERO;
        for contact in [
            self.collider.floor,
            self.collider.wall,
            self.collider.ceiling,
        ] {
            if contact.is_present() {
                normals += contact.hit.normal;
            }
        }
        // calcCollidedNormalSum 0x8d8a00 and reboundVelocityFromCollision 0x8e5f98.
        normals *= 1. / 3.;
        if !vector_lib::is_near_zero(normals, 0.001) {
            let normal = pose_math::normalize(normals);
            let speed = dot(normal, self.velocity);
            if speed < 0. {
                let into = normal * speed;
                self.velocity -= into;
                if speed < -5. {
                    self.velocity -= into * 0.8;
                }
            }
        }

        let ground = self.collider.floor;
        let down = if ground.is_present() {
            -ground.hit.normal
        } else {
            Vec3::NEG_Y
        };
        // addVelocityToDirection 0x8e3914 normalizes the supplied direction.
        self.velocity += try_normalize_or_zero(down).0 * 2.;
        if ground.is_present() && ground.hit.normal.y < 0.898_794_05 {
            // 0x2acc28: project the normal off world up on slopes steeper than 26°.
            self.velocity += vector_lib::verticalize_vec(Vec3::Y, ground.hit.normal) * 2.;
        }
        self.velocity *= 0.925;
        self.joint = roll_quat(self.joint, self.velocity * 0.7, -down, RADIUS);

        if ground.is_present() {
            self.roll_ground_grace = 5;
            let tangent = vector_lib::verticalize_vec(down, self.velocity);
            if vector_lib::length(tangent) < 0.1 {
                // 0x2acda4–0x2ace14: settle on the ground's collision part.
                let triangle = &mesh.triangles[ground.hit.face as usize];
                if let Some(local) = triangle.local {
                    let part = local.part as usize;
                    if mesh.key_move_parts.iter().any(|s| s.part == part)
                        || mesh.rotate_parts.iter().any(|s| s.part == part)
                        || mesh.geysers.iter().any(|s| s.part == part)
                    {
                        self.limit = "stone_moving_rest_unavailable";
                        return;
                    }
                }
                self.velocity = Vec3::ZERO;
                self.pending = Some(Nerve::Wait);
            }
        } else {
            self.roll_ground_grace -= 1;
            if self.roll_ground_grace == 0 {
                self.pending = Some(Nerve::Fall);
            }
        }
    }

    pub(super) fn release(&mut self, mesh: &crate::geometry::TriangleMesh, front: Vec3) {
        let front = try_normalize_or_zero(vector_lib::verticalize_vec(Vec3::NEG_Y, front)).0;
        let quat = pose_math::quat_front_up(front, pose_math::NEGATED_GRAVITY);
        // receiveMsgRelease changes the stored quaternion before the model matrix is updated.
        self.collider.set_radius((RADIUS * HOLD_SCALE).min(40.));
        self.collider.invalidate(self.position, self.pose[1]);
        self.collider.set_radius(RADIUS);
        self.position += self
            .collider
            .collide(mesh, self.position, self.pose[1], Vec3::ZERO);
        self.pose = pose_math::quat_to_axes(quat);
        self.velocity = self.pose[2] * 22.5 + self.pose[1] * 32.5;
        self.rolling_forward = false; // 0x2abcac selects -0.4 until the first ground response.
        self.pending = Some(Nerve::Fall);
    }

    pub(super) fn diagnostic(&self) -> String {
        let quat = pose_math::to_quat(self.pose)
            .to_array()
            .map(|x| format!("{:08x}", x.to_bits()))
            .join("/");
        let joint = self
            .joint
            .to_array()
            .map(|x| format!("{:08x}", x.to_bits()))
            .join("/");
        let nerve = match self.pending.unwrap_or(self.nerve) {
            Nerve::Wait => "Wait",
            Nerve::Hold => "Hold",
            Nerve::Fall => "Fall",
            Nerve::Roll => "Roll",
        };
        format!(
            ";stone={}/{};stone_velocity={};stone_quat={quat};stone_joint={joint};stone_nerve={nerve}/{};stone_radius={:08x};stone_push={};stone_ground={};stone_limit={}",
            self.id,
            hex_vec(self.position),
            hex_vec(self.velocity),
            if self.pending.is_some() { 0 } else { self.step },
            self.collider.radius().to_bits(),
            hex_vec(self.push),
            u8::from(self.collider.floor.is_present()),
            self.limit
        )
    }
}

/// calcQuatRotateDegree/calcRPY followed by MapObjStatePlayerHold's Euler matrix.
fn rotation_from_joint(q: Quat) -> joint_math::Rows {
    let opposite_pitch = (q.x * q.z - q.w * q.y) * 2.;
    let angles = if 1. - opposite_pitch.abs() < 1.192_092_9e-6 {
        Vec3::new(
            0.,
            (opposite_pitch / opposite_pitch.abs()) * -std::f32::consts::FRAC_PI_2,
            pose_math::atan2f(
                -((q.x * q.y - q.w * q.z) * 2.),
                -opposite_pitch * ((q.w * q.y + q.x * q.z) * 2.),
            ),
        )
    } else {
        Vec3::new(
            pose_math::atan2f(
                (q.w * q.x + q.y * q.z) * 2.,
                ((q.w * q.w - q.x * q.x) - q.y * q.y) + q.z * q.z,
            ),
            pose_math::asinf(-opposite_pitch),
            pose_math::atan2f(
                (q.x * q.y + q.w * q.z) * 2.,
                ((q.w * q.w + q.x * q.x) - q.y * q.y) - q.z * q.z,
            ),
        )
    } * 57.295_776
        * pose_math::DEGREES_TO_RADIANS;
    let (sx, sy, sz) = (
        pose_math::sinf(angles.x),
        pose_math::sinf(angles.y),
        pose_math::sinf(angles.z),
    );
    let (cx, cy, cz) = (
        pose_math::cosf(angles.x),
        pose_math::cosf(angles.y),
        pose_math::cosf(angles.z),
    );
    [
        [cy * cz, (sx * sy) * cz - sz * cx, sx * sz + (sy * cx) * cz],
        [cy * sz, (sx * sy) * sz + cx * cz, (sy * cx) * sz - sx * cz],
        [-sy, sx * cy, cx * cy],
    ]
}

/// rotateQuatRollBall -> rotateQuatMoment, including the final quaternion normalization.
fn roll_quat(q: Quat, velocity: Vec3, up: Vec3, radius: f32) -> Quat {
    let up = try_normalize_or_zero(up).0;
    let moment = pose_math::cross(up, velocity) * (1. / radius);
    let half = squared_length(moment).sqrt() * 0.5;
    let axis = try_normalize_or_zero(moment).0;
    let sin = pose_math::sinf(half);
    let (x, y, z, w) = (
        axis.x * sin,
        axis.y * sin,
        axis.z * sin,
        pose_math::cosf(half),
    );
    let a = ((w * q.x + q.w * x) + y * q.z) - z * q.y;
    let b = ((w * q.y - x * q.z) + q.w * y) + z * q.x;
    let c = ((x * q.y + w * q.z) - y * q.x) + q.w * z;
    let d = ((w * q.w - x * q.x) - y * q.y) - z * q.z;
    let inverse = 1. / (c * c + ((d * d + a * a) + b * b)).sqrt();
    Quat::from_xyzw(a * inverse, b * inverse, c * inverse, d * inverse)
}

impl Player {
    pub(super) fn update_kick_stone(&mut self, scene: &Course) {
        let Some(mesh) = scene.mesh.as_deref() else {
            return;
        };
        let position = mesh.to_game_vec(self.position_cm());
        let front = self.pose_axes().1;
        if let Some(stone) = &mut self.carry.stone {
            stone.update(
                mesh,
                self.carry.hold.as_ref(),
                self.grounded,
                position,
                front,
            );
            if self.carry.held {
                let push = stone.push;
                self.receive_object_message(
                    super::carry::Message::CollidePush(push),
                    false,
                    Vec3::ZERO,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> Source {
        Source {
            id: "test".into(),
            position: Vec3::new(0., 100., 0.),
            joint: Quat::IDENTITY,
        }
    }

    /// A stone answers `HoldCancel` by disappearing and restarting from its placement (nerve
    /// RestartPrepare, 0x2acf7c; `MapObjStatePlayerHold::receiveMsgCancel` 0x2b6ddc), which the
    /// port does not model: the limit is exposed and the hold stays, as for the collision lock-up.
    #[test]
    fn a_stone_hold_cancel_is_exposed_and_keeps_the_hold() {
        let mut p = Player::default();
        p.carry.stone = Some(Stone::new(&source()));
        p.carry.held = true;
        p.start_cancel_and_release(None);
        assert!(p.carry.cancel.is_none());
        assert!(p.is_carry());
        assert_eq!(p.fidelity_limits(), "stone_hold_cancel_unavailable");
    }

    #[test]
    fn hold_shrinks_on_actor_entry_and_release_restores_the_collider() {
        let mesh = crate::geometry::TriangleMesh::from_corners(&[]);
        let mut stone = Stone::new(&source());
        stone.pick_up();
        assert!(!stone.can_pick_up());
        assert_eq!(stone.collider.radius(), 60.);
        let hold = [[1., 0., 0., 0.], [0., 1., 0., 100.], [0., 0., 1., 0.]];
        stone.update(&mesh, Some(&hold), true, Vec3::ZERO, Vec3::Z);
        assert_eq!(stone.collider.radius(), 18.);
        assert_eq!(stone.push, Vec3::ZERO);
        assert_eq!((stone.nerve, stone.step), (Nerve::Hold, 1));
        stone.release(&mesh, Vec3::Z);
        assert_eq!(stone.collider.radius(), 60.);
        assert_eq!(stone.velocity, Vec3::new(0., 32.5, 22.5));
        assert_eq!(stone.pending, Some(Nerve::Fall));
        stone.update(&mesh, None, false, Vec3::ZERO, Vec3::Z);
        assert!(stone.velocity.y < 32.5);
        assert!(stone.position.z > 0.);
        stone.limit = "stone_wall_rebound_unavailable";
        let position = stone.position;
        let velocity = stone.velocity;
        stone.update(&mesh, None, false, Vec3::ZERO, Vec3::Z);
        assert_eq!(stone.position, position);
        assert_eq!(stone.velocity, velocity);
    }

    #[test]
    fn held_collider_computes_obstruction_feedback_without_moving_the_attachment() {
        let mesh = crate::geometry::TriangleMesh::from_corners(&[[
            Vec3::new(-1000., 0., -1000.),
            Vec3::new(0., 0., 1000.),
            Vec3::new(1000., 0., -1000.),
        ]]);
        let mut stone = Stone::new(&source());
        stone.pick_up();
        let hold = [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]];
        stone.update(&mesh, Some(&hold), false, Vec3::new(0., 50., -50.), Vec3::Z);
        assert_eq!(stone.position, Vec3::ZERO);
        assert!(stone.push.length_squared() > 0.);
        assert!(stone.collider.floor.is_present());
    }

    /// Oracle: from the recorded state at the release, the port's flight, rebounds, roll and rest
    /// match the recording bit for bit (`PJV_STONE_CASES`: a local export of the actor samples that
    /// only supplies the starting state and the expected results, never the replay's inputs).
    #[test]
    fn flight_roll_and_rest_follow_the_recorded_stone_from_the_release_state() {
        let Ok(path) = std::env::var("PJV_STONE_CASES") else {
            return;
        };
        let cases: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let origin = cases["origin"].as_array().unwrap();
        let origin = Vec3::new(
            origin[0].as_f64().unwrap() as f32,
            origin[1].as_f64().unwrap() as f32,
            origin[2].as_f64().unwrap() as f32,
        );
        let text = std::fs::read_to_string(cases["geometry"].as_str().unwrap()).unwrap();
        let mesh = crate::geometry::TriangleMesh::from_json(&text, origin).unwrap();
        let frames = cases["frames"].as_array().unwrap();
        let at = |f: u64| frames.iter().find(|c| c["f"] == f).unwrap();
        let bits = |c: &serde_json::Value, key: &str| -> Vec<f32> {
            c[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| f32::from_bits(x.as_u64().unwrap() as u32))
                .collect()
        };
        let vec3 = |c: &serde_json::Value, key: &str| {
            let v = bits(c, key);
            Vec3::new(v[0], v[1], v[2])
        };
        let quat = |c: &serde_json::Value, key: &str| {
            let v = bits(c, key);
            Quat::from_xyzw(v[0], v[1], v[2], v[3])
        };
        let held = at(251);
        let mut stone = Stone::new(&Source {
            id: "obj1837".into(),
            position: vec3(held, "pos"),
            joint: quat(held, "joint"),
        });
        stone.pose = pose_math::quat_to_axes(quat(held, "quat"));
        stone.nerve = Nerve::Hold;
        stone.release(&mesh, Vec3::Z);
        let released = at(252);
        let mut report = Vec::new();
        if stone.position != vec3(released, "pos") {
            report.push(format!("release position {:?} vs {:?}", stone.position, vec3(released, "pos")));
        }
        stone.position = vec3(released, "pos");
        stone.velocity = vec3(released, "vel");
        stone.pose = pose_math::quat_to_axes(quat(released, "quat"));
        stone.joint = quat(released, "joint");
        for f in 253..=447u64 {
            stone.update(&mesh, None, false, Vec3::ZERO, Vec3::Z);
            let sample = at(f);
            let nerve = match stone.pending.unwrap_or(stone.nerve) {
                Nerve::Wait => "Wait",
                Nerve::Hold => "Hold",
                Nerve::Fall => "Fall",
                Nerve::Roll => "Roll",
            };
            let mut diffs = Vec::new();
            if stone.position != vec3(sample, "pos") {
                diffs.push(format!("pos {:?} vs {:?}", stone.position, vec3(sample, "pos")));
            }
            if stone.velocity != vec3(sample, "vel") {
                diffs.push(format!("vel {:?} vs {:?}", stone.velocity, vec3(sample, "vel")));
            }
            if nerve != sample["nerve"] {
                diffs.push(format!("nerve {nerve} vs {}", sample["nerve"]));
            }
            for (name, contact, key) in [
                ("ground", &stone.collider.floor, "ground"),
                ("wall", &stone.collider.wall, "wall"),
                ("ceiling", &stone.collider.ceiling, "ceiling"),
            ] {
                if contact.is_present() != (sample[key] == "1") {
                    diffs.push(format!("{name} {} vs {}", contact.is_present(), sample[key]));
                }
            }
            if !diffs.is_empty() {
                report.push(format!("f{f}: {}", diffs.join("; ")));
            }
            // Later frames start from the port's own state; the first mismatch is the finding.
            if report.len() > 12 {
                break;
            }
        }
        assert!(stone.limit.is_empty(), "{}", stone.limit);
        assert!(report.is_empty(), "{}", report.join("
"));
    }

    #[test]
    fn floor_response_rebounds_before_roll_and_settles_on_static_ground() {
        let mesh = crate::geometry::TriangleMesh::from_corners(&[[
            Vec3::new(-1000., 0., -1000.),
            Vec3::new(0., 0., 1000.),
            Vec3::new(1000., 0., -1000.),
        ]]);
        let mut stone = Stone::new(&source());
        stone.position = Vec3::new(0., 70., 0.);
        stone.collider.invalidate(stone.position, Vec3::Y);
        stone.nerve = Nerve::Fall;
        stone.velocity = Vec3::new(0., -30., 3.);
        stone.update(&mesh, None, false, Vec3::ZERO, Vec3::Z);
        assert!(stone.collider.floor.is_present());
        stone.update(&mesh, None, false, Vec3::ZERO, Vec3::Z);
        assert!(stone.velocity.y > 15.);
        assert!(stone.limit.is_empty());
        assert!(stone.rolling_forward);

        for _ in 0..240 {
            stone.update(&mesh, None, false, Vec3::ZERO, Vec3::Z);
            if stone.nerve == Nerve::Wait {
                break;
            }
        }
        assert_eq!(stone.nerve, Nerve::Wait);
        assert_eq!(stone.velocity, Vec3::ZERO);
        assert!(stone.can_pick_up());
    }
}
