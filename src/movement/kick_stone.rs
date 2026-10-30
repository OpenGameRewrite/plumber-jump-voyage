//! KickStone's front hold and deterministic Fall prefix (SMO 1.0.0).
//! Runtime landing effects consume the shared RNG; without its independent context the
//! actor stops at that call, exposes a fidelity limit and cannot certify the later roll.
use super::actor_collider::ActorCollider;
use super::carry::hex_vec;
use super::pose_math::{self, dot, squared_length, try_normalize_or_zero};
use super::*;

const RADIUS: f32 = 60.;
const HOLD_SCALE: f32 = 0.3;
pub(super) const HOLD_OFFSET: Vec3 = Vec3::new(0., 30., 65.);

#[derive(Clone, Debug)]
pub struct Source {
    id: String,
    position: Vec3,
    joint: Quat,
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
    contact_age: u32,
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
            contact_age: 0,
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
            }
        }
        match self.nerve {
            Nerve::Wait => {} // IsConnectToCollision=false: initial Wait has offCollide.
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
            Nerve::Fall => {
                // MoveType0 parameter pointers in 0x1d0a728, exeFall 0x2ac15c.
                self.velocity.y -= 2.;
                self.velocity *= 0.99;
                self.joint = roll_quat(self.joint, self.velocity * -0.4, Vec3::Y, RADIUS);
                if self.collider.wall.is_present() {
                    // Wall rebound and its reaction have not been established by this prefix.
                    self.limit = "stone_wall_rebound_unavailable";
                    return;
                }
                self.contact_age += 1;
                if self.collider.floor.is_present() {
                    let normal = self.collider.floor.hit.normal;
                    let parallel = normal * dot(self.velocity, normal);
                    let rebound = parallel * -0.55;
                    let rebound_speed = squared_length(rebound).sqrt();
                    if rebound_speed >= 6. && (rebound_speed > 15. || self.contact_age >= 15) {
                        // 0x2ac5e8 consumes the global RNG before writing the rebound velocity.
                        self.limit = "stone_runtime_rng_unavailable";
                        return;
                    }
                    if rebound_speed < 6. {
                        self.limit = "stone_roll_unavailable";
                        return;
                    }
                    self.contact_age = 0;
                    self.velocity = (self.velocity - parallel) + rebound;
                }
                self.position +=
                    self.collider
                        .collide(mesh, self.position, self.pose[1], self.velocity);
            }
        }
        self.step += 1;
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
        };
        format!(
            ";stone={}/{};stone_velocity={};stone_quat={quat};stone_joint={joint};stone_nerve={nerve}/{};stone_radius={:08x};stone_push={};stone_limit={}",
            self.id,
            hex_vec(self.position),
            hex_vec(self.velocity),
            if self.pending.is_some() { 0 } else { self.step },
            self.collider.radius().to_bits(),
            hex_vec(self.push),
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
        stone.limit = "stone_runtime_rng_unavailable";
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
}
