//! Placement-bound flower objects and the player's independent carry keeper.
pub use super::grow_seed::Seed;
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
                || item["initial_phase"] != "init"
                || !item["initial_alive"].is_boolean()
                || !matches!(class, "GrowFlowerPot" | "GrowFlowerSeed")
                || p["Scale"] != serde_json::json!({"X":1.,"Y":1.,"Z":1.})
                || (class == "GrowFlowerPot"
                    && item["sensors"] != serde_json::json!([{"Name":"Body","Radius":70.,"Y":50.}]))
                || (class == "GrowFlowerSeed"
                    && item["sensors"]
                        != serde_json::json!([
                        {"Name":"Body","Radius":50.,"Type":"EnemyBody","Y":35.},
                        {"Name":"Hold","Radius":75.,"Type":"HoldObj","Y":35.}]))
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
    pub stone: Option<super::kick_stone::Stone>,
    pub held: bool,
    pub wall_keep: bool,
    pub saved_position: Vec3,
    pub lock_up: u32,
    pub phase: u8,
    pub step: u32,
    pub inhibition: u32,
    pub upper: Option<(&'static str, f32)>,
    pub hold: Option<look_joint::Mat34>,
    pub hold_matrix: Option<look_joint::Mat34>,
    pub(super) origin: Vec3,
    pub picked_up: bool,
    pub(super) held_during_sensor_phase: bool,
    pub(super) reconstruction_release_unavailable: bool,
    /// The throw reached its release step; the seed leaves the hand once the frame's state ran.
    pub release_due: bool,
    /// A `startCancelAndRelease` the keeper has not run yet (see [`Player::start_cancel_and_release`]).
    pub(super) cancel: Option<PendingCancel>,
}

/// The keeper's Release nerve, chosen by `startCancelAndRelease` (0x42e9b0). A nerve chosen in a
/// state runs in a later keeper update (`updateCarry` 0x420ed8). `getCurrentNerve`
/// (0x959780) exposes it immediately, so `isCarry` (0x42ebfc) is already false while the
/// held sensor still awaits cleanup (0x42f2ac).
#[derive(Clone, Copy, Debug)]
pub(super) struct PendingCancel;

/// The receive dispatch compares message identities; an unhandled identity falls through.
/// SensorMsgPush and SensorMsgPushToPlayer are distinct original classes.
#[derive(Clone, Copy)]
pub(super) enum Message {
    Push,
    PushToPlayer,
    #[allow(dead_code)] // The shared receive route; stone is part 2.
    CollidePush(Vec3),
    #[allow(dead_code)] // Unknown runtime identities use the default dispatch branch.
    Unhandled,
}

impl Player {
    pub fn flower_look_sensors(
        &self,
        sensors: &mut Vec<look_at::LookSensor>,
        mesh: Option<&crate::geometry::TriangleMesh>,
    ) {
        if let Some(mesh) = mesh {
            for source in &mesh.carry_sources {
                if !source.active || source.class != "GrowFlowerPot" {
                    continue;
                }
                let at = source.position + Vec3::Y * 50.;
                sensors.push(look_at::LookSensor {
                    host: "[育つ花]植木鉢".into(),
                    name: "Body".into(),
                    kind: 12,
                    radius: 70.,
                    position: mesh.relative_vec(at),
                    stage_position: at,
                });
            }
        }
        if let Some(seed) = &self.carry.seed {
            let at = seed.body_sensor(35.);
            sensors.push(look_at::LookSensor {
                host: "[育つ花]種".into(),
                name: "Body".into(),
                kind: 9,
                radius: 50.,
                position: mesh.map_or(at, |m| m.relative_vec(at)),
                stage_position: at,
            });
        }
        if let Some(stone) = &self.carry.stone {
            sensors.push(look_at::LookSensor {
                host: "キック小石".into(),
                name: "Body".into(),
                kind: 9,
                radius: 70.,
                position: mesh.map_or(stone.position, |m| m.relative_vec(stone.position)),
                stage_position: stone.position,
            });
        }
    }

    pub(super) fn receive_object_message(
        &mut self,
        message: Message,
        attack_sensor: bool,
        push: Vec3,
    ) -> bool {
        if attack_sensor {
            return false;
        }
        if matches!(self.mode, Move::Pole | Move::GrabCeil)
            || self.recovering()
            || self.recovery.push_invalid > 0
        {
            return false;
        }
        match message {
            Message::PushToPlayer => {
                self.carry.push.receive(push, false);
                true
            }
            Message::CollidePush(force) => {
                self.carry.push.receive(force, true);
                true
            }
            _ => false,
        }
    }

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
            if !source.active || source.class != "GrowFlowerPot" {
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
            if !self.receive_object_message(Message::Push, false, push) {
                self.receive_object_message(Message::PushToPlayer, false, push);
            }
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

pub(super) fn hex_vec(v: Vec3) -> String {
    v.to_array()
        .map(|x| format!("{:08x}", x.to_bits()))
        .join("/")
}

impl Player {
    /// PlayerCarryKeeper::updateCollideLockUp, called after updateCollider (0x4215b8).
    /// A cancellation is exposed as unsupported before changing the held ownership.
    pub(super) fn update_stone_collide_lock_up(&mut self, scene: &Course) {
        if self.carry.stone.is_none() {
            return;
        }
        let position = scene
            .mesh
            .as_ref()
            .map_or(self.position_cm(), |m| m.to_game_vec(self.position_cm()));
        let push = self.carry.push.collide_min + self.carry.push.collide_max;
        let constrained = self.carry.held
            && !self.is_collided_ground()
            && !vector_lib::is_near_zero(push, 0.001)
            && pose_math::squared_length(position - self.carry.saved_position) <= 900.
            && pose_math::dot(push, Vec3::NEG_Y) <= 0.
            && pose_math::dot(self.units_per_frame(), Vec3::NEG_Y) >= 0.;
        if constrained {
            self.carry.lock_up = (self.carry.lock_up + 1).min(10);
            if self.carry.lock_up == 10 {
                // PlayerCarryKeeper::updateCollideLockUp answers true on the tenth frame and the
                // actor cancels and releases the hold (updateCollider 0x4215c4).
                self.start_cancel_and_release(scene.mesh.as_deref());
            }
        } else {
            self.carry.lock_up = 0;
            self.carry.saved_position = position;
        }
    }

    pub fn is_carry(&self) -> bool {
        self.carry.held && self.carry.cancel.is_none()
    }

    /// What the actor's state allows `attackSensor` (0x427818) to start a carry in. Both pickup
    /// paths (the `Carry` sensor on the press, 0x4279c8, and the held `Body` one, 0x42825c) ask
    /// the same gates: the refusals of 0x429c68 and 0x429b44 (dead, hack, damage, the squat,
    /// rolling, grab ceiling, wall catch, dive and so on), and, while the actor nerve is SpinCap
    /// (0x1d78940), `PlayerStateSpinCap::isEnableCancelCarry` (0x481f10).
    ///
    /// The port starts a carry from the idle, walk, run, skid, turn, jump and fall modes and from
    /// the ground spin before its first child step or once the cap has left. The binary lets the
    /// other states it does not refuse pick an object up too (the other jump kinds, the hip drop
    /// family, the slope slide, the
    /// long jump, the sand states, the fire damage, the rise); their carry handling (animation,
    /// cancels) is not ported, and no recording picks up there.
    fn pickup_state_allows(&self) -> bool {
        match self.mode {
            Move::Idle | Move::Walk | Move::Run | Move::Skid | Move::Turn => true,
            Move::Jump | Move::Fall => true,
            Move::Throw => self.spin_cap_enable_cancel_carry(),
            _ => false,
        }
    }

    /// `PlayerStateSpinCap::isEnableCancelCarry` (0x481f10) for the ground child (`SpinGround`,
    /// 0x1d7ff80): true when the cap is not the spin attack (`PlayerSpinCapAttack::
    /// isCapSpinAttack`, 0x465420), and while the state is not dead (the byte at +0x10, set by
    /// `kill` on the child's end step). The cap starts its wind on the first child step
    /// (0x4820ac), leaving an earlier pickup window. During the wind it asks `isEnableCancelAir`
    /// (0x481da4), which is false for the ground nerves. (`SpinGroundSeparate` after step 10 is the
    /// second player's throw and never runs in single play.) The air child answers through
    /// `isEnableCancelAir` alone: false for the first 23 steps, and from the fall nerve on, which
    /// the port runs as the fall mode.
    pub(super) fn spin_cap_enable_cancel_carry(&self) -> bool {
        self.mode == Move::Throw
            && !self.throwing.in_air
            && !self.winding
            && !self.throwing.child_ended
    }

    pub(super) fn carry_sensor_phase(&mut self, o: &Orders, scene: &Course) {
        self.carry.held_during_sensor_phase = self.carry.held;
        self.carry.picked_up = false;
        if self.carry.held
            || self.carry.inhibition > 0
            || !o.throw_held
            || !self.pickup_state_allows()
        {
            return;
        }
        let Some(mesh) = scene.mesh.as_deref() else {
            return;
        };
        let (target, target_radius, wall_keep) = if let Some(seed) = &self.carry.seed {
            if !seed.can_pick_up() {
                return;
            }
            (
                seed.position + Vec3::Y * 35.,
                if o.throw_pressed { 75. } else { 50. },
                true,
            )
        } else if let Some(stone) = &self.carry.stone {
            if !stone.can_pick_up() {
                return;
            }
            (stone.position, 70., false)
        } else {
            return;
        };
        let feet = mesh.to_game_vec(self.position_cm());
        // Carry (Action edge) and Body (held Action) are distinct attack paths.
        let (height, radius) = if o.throw_pressed {
            (100., 130.)
        } else {
            (75., 50.)
        };
        let centre = feet + self.pose_axes().0 * height;
        if pose_math::squared_length(centre - target)
            >= (radius + target_radius) * (radius + target_radius)
        {
            return;
        }
        if self.carry.seed.is_some()
            && crate::collider::first_strike(
                mesh,
                mesh.relative_vec(target),
                feet - target,
                crate::collider::FaceFilter::Any,
            )
            .is_some()
        {
            return;
        }
        // GrowFlowerSeed accepts FrontWallKeep; KickStone only accepts Front.
        if let Some(stone) = &mut self.carry.stone {
            stone.pick_up();
        }
        self.carry.saved_position = feet;
        self.carry.held = true;
        self.carry.wall_keep = wall_keep;
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
    pub(super) fn update_carry(&mut self, o: &Orders, advance_sources: bool) {
        if self.carry.cancel.is_some() {
            // update decrements +0x48 before executing the pending Release (0x42e664..0x42e684).
            // The previous Carry/Throw nerve must not execute (getCurrentNerve 0x959780).
            self.carry.inhibition = self.carry.inhibition.saturating_sub(1);
            return;
        }
        if self.carry.held {
            self.pre_throw = 0;
        }
        if self.carry.held && o.throw_pressed && !self.carry.picked_up && self.carry.phase < 3 {
            if advance_sources {
                self.carry.phase = 3;
                self.carry.step = 0;
                self.carry.inhibition = 10;
                self.events.push(Fx::CarryThrow);
            } else {
                // A chosen-nerve reconstruction repeats the pickup button with a
                // keeper that already holds the object. Preserve the existing
                // snapshot limit; this is not a second source-side action.
                self.carry.reconstruction_release_unavailable = true;
            }
        }
        self.carry.inhibition = self.carry.inhibition.saturating_sub(1);
        if self.carry.phase == 3 {
            if self.carry.step == 0 {
                self.carry.upper = None;
                self.upper_cleared = true;
                self.start_sub("CarryFrontThrow", 24., -1.);
            }
            if self.carry.held && self.carry.step >= 5 {
                // The keeper releases in the actor's control, after the standing state of the
                // frame ran: `finish_carry_release` does it there.
                self.carry.release_due = true;
            }
            if !self.carry.held && (!self.sub.active || self.sub.frame >= self.sub.length) {
                self.carry.phase = 0;
                self.carry.step = 0;
                self.carry.wall_keep = false;
            } else {
                self.carry.step += 1;
            }
            return;
        }
        if !self.carry.held {
            return;
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

    /// The release of a throw (`PlayerRelease` to the held object) happens in `updateCarry`,
    /// which the actor's control runs after the state of the frame: Mario's pose and velocity
    /// are those the state left.
    pub(super) fn finish_carry_release(&mut self, scene: &Course) {
        if self.carry.cancel.take().is_some() {
            self.run_release_nerve();
        }
        if self.carry.phase == 3 && self.carry.step == 1 && self.tilt >= STICK_DEADZONE {
            // updateCarry 0x420e78..0x420ed0 runs after the state: its facing must
            // survive that state's ground pose update, and must not steer its velocity.
            let up = self
                .mesh_ground
                .filter(|_| self.grounded)
                .unwrap_or(Vec3::Y);
            let (direction, _) = pose_math::try_normalize_or_zero(self.plane_stick(up));
            self.set_pose(plane::quat_up_front(up, direction));
        }
        if std::mem::take(&mut self.carry.release_due) {
            let front = self.pose_axes().1;
            if let (Some(mesh), Some(stone)) = (scene.mesh.as_deref(), &mut self.carry.stone) {
                stone.release(mesh, front);
            }
            self.release_flower_seed(scene);
            self.carry.held = false;
        }
    }

    /// `PlayerCarryKeeper::startCancelAndRelease` (0x42e9b0), asked by the actor's states behind
    /// `isCarry` (31 direct sites; updateCollider checks through updateCollideLockUp): it sends
    /// `SensorMsgHoldCancel` from the carry sensor to the held sensor, zeroes the hand and leg
    /// joint angles the keeper hands
    /// the joint controls and the vector at +0x50 (presentation, not modelled), and chooses the
    /// Release nerve.
    ///
    /// The held object answers the message where it arrives:
    /// - `GrowFlowerSeed::receiveMsg` (0x292564), in its hold nerve, takes `PlayerRelease` and
    ///   `HoldCancel` through one branch (0x292828) that is the ordinary throw with the speeds of a
    ///   release not by swing (28 and 70, helper 0x291934), aimed at the player's velocity and front
    ///   as they are now: the seed leaves the hand.
    /// - `KickStone::receiveMsg` (0x2ab494) takes it through
    ///   `MapObjStatePlayerHold::receiveMsgCancel` (0x2b6ddc): the stone is set down, hidden, its
    ///   sensors invalidated and it restarts from its placement (nerve RestartPrepare, 0x2acf7c).
    ///   The stone's restart is not modelled, so the keeper keeps holding it and the limit is
    ///   exposed, as for the collision lock-up cancel.
    ///
    /// HoldCancel is synchronous (0x42e9c4); Release is immediately observable through
    /// getCurrentNerve (0x959780), before its keeper-update cleanup ([`Self::run_release_nerve`]).
    pub(super) fn start_cancel_and_release(
        &mut self,
        mesh: Option<&crate::geometry::TriangleMesh>,
    ) {
        if !self.is_carry() || self.carry.cancel.is_some() {
            return;
        }
        if let Some(stone) = &mut self.carry.stone {
            stone.limit = "stone_hold_cancel_unavailable";
            return;
        }
        if self.carry.seed.is_some() {
            let throw = self.aim_flower_seed_throw();
            self.throw_flower_seed(throw, mesh);
        }
        self.carry.cancel = Some(PendingCancel);
    }

    /// `PlayerCarryKeeper::exeRelease` (0x42f28c) after `updateNerve` switched to the Release
    /// nerve: the held sensor is forgotten (0x42f2ac); unless the model is the 2D one
    /// the upper body animation is cleared (`clearUpperBodyAnim`, as the throw's start does),
    /// the pickup is inhibited for ten frames (+0x48, counted down by every update) and the
    /// carry-above and wall-keep flags are cleared; then the keeper waits.
    fn run_release_nerve(&mut self) {
        self.carry.held = false;
        self.carry.release_due = false;
        self.carry.phase = 0;
        self.carry.step = 0;
        if !self.model_2d() {
            self.carry.upper = None;
            self.upper_cleared = true;
            self.carry.inhibition = 10;
            self.carry.wall_keep = false;
        }
    }

    pub(super) fn record_hold_pose(&mut self, joints: &crate::skeleton_pose::Matrices) {
        if !self.carry.held {
            return;
        }
        use crate::skeleton_pose::Joint;
        let spine = joints[Joint::Spine2 as usize];
        let left = look_joint::translation(&joints[Joint::HandL as usize]) + self.carry.origin;
        let right = look_joint::translation(&joints[Joint::HandR as usize]) + self.carry.origin;
        let position =
            if self.carry.phase == 3 && self.sub.active && self.sub.name == "CarryFrontThrow" {
                let hand = joints[Joint::HandR as usize];
                right + Vec3::new(hand[0][0], hand[1][0], hand[2][0]) * 30.
            } else {
                (left + right) * 0.5
            };
        let rotation = hold_matrix(spine, position);
        self.carry.hold_matrix = Some(rotation);
        let offset = if self.carry.stone.is_some() {
            super::kick_stone::HOLD_OFFSET
        } else {
            Vec3::new(0., -30., 40.)
        };
        let mut pose = rotation;
        for row in &mut pose {
            row[3] += row[2].mul_add(offset.z, row[1].mul_add(offset.y, row[0] * offset.x));
        }
        self.carry.hold = Some(pose);
    }

    pub fn carry_diagnostic(&self) -> String {
        if self.carry.seed.is_none()
            && self.carry.stone.is_none()
            && self.carry.diagnostic.is_empty()
        {
            return String::new();
        }
        let mut out = self.carry.diagnostic.clone();
        out.push_str(&format!(
            ";keeper={}/{}/{}",
            self.carry.phase,
            self.carry.step,
            u8::from(self.carry.held)
        ));
        if self.carry.held {
            out.push_str(&format!(
                ";saved={};front_wall_keep={};inhibition={}",
                hex_vec(self.carry.saved_position),
                u8::from(self.carry.wall_keep),
                self.carry.inhibition
            ));
        }
        if let Some(seed) = &self.carry.seed {
            out.push_str(&format!(";seed={}/{}", seed.id, hex_vec(seed.position)));
            out.push_str(&seed.throw_diagnostic());
        }
        if let Some(stone) = &self.carry.stone {
            out.push_str(&stone.diagnostic());
            out.push_str(&format!(";lockup={}", self.carry.lock_up));
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
    let multiply = |a: &joint_math::Rows, b: &joint_math::Rows| {
        std::array::from_fn(|r| {
            std::array::from_fn(|c| {
                a[r][2].mul_add(b[2][c], a[r][1].mul_add(b[1][c], a[r][0] * b[0][c]))
            })
        })
    };
    rows = multiply(&multiply(&rows, &z), &y);
    std::array::from_fn(|i| [rows[i][0], rows[i][1], rows[i][2], position[i]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pickup_layer_lifecycle_and_dispatch_are_independent_of_locomotion() {
        let mut p = Player::default();
        assert!(!p.receive_object_message(Message::Unhandled, false, Vec3::ONE));
        assert!(!p.receive_object_message(Message::Push, false, Vec3::ONE));
        assert!(!p.receive_object_message(Message::PushToPlayer, true, Vec3::ONE));
        assert!(p.receive_object_message(Message::CollidePush(Vec3::X), false, Vec3::ZERO));
        assert_eq!(p.carry.push.collide_max, Vec3::X);
        p.carry.held = true;
        p.carry.wall_keep = true;
        p.carry.phase = 1;
        p.carry.upper = Some(("CarryFrontStart", 0.));
        p.carry.picked_up = true;
        let orders = Orders::default();
        p.update_carry(&orders, true);
        p.carry.picked_up = false;
        for _ in 0..8 {
            p.update_carry(&orders, true);
        }
        assert_eq!(
            (p.carry.phase, p.carry.step, p.carry.upper),
            (1, 9, Some(("CarryFrontStart", 8.)))
        );
        p.update_carry(&orders, true);
        assert_eq!((p.carry.phase, p.carry.step), (2, 0));
        p.update_carry(&orders, true);
        assert_eq!(p.carry.upper, Some(("CarryFront", 0.)));
        assert!(!p.ground_actions(&Orders {
            crouch: true,
            ..default()
        }));
        p.carry.inhibition = 10;
        p.update_carry(&orders, true);
        assert_eq!(p.carry.inhibition, 9);
    }

    #[test]
    fn hold_matrix_uses_spine_orientation_and_hand_midpoint() {
        let m = hold_matrix(
            [[1., 0., 0., 99.], [0., 1., 0., 98.], [0., 0., 1., 97.]],
            Vec3::new(1., 2., 3.),
        );
        assert_eq!(look_joint::translation(&m), Vec3::new(1., 2., 3.));
        assert_eq!(m[0][1].to_bits(), 0x3f7fffff);
        assert_eq!(m[2][0].to_bits(), 0x3f7fffff);
        assert_eq!(m[1][2].to_bits(), 0x3f7ffffe);
    }

    #[test]
    fn front_throw_releases_after_the_fifth_state_step() {
        let mut p = Player::default();
        p.carry.held = true;
        p.carry.phase = 2;
        p.carry.upper = Some(("CarryFront", 0.));
        p.update_carry(
            &Orders {
                throw_pressed: true,
                ..default()
            },
            true,
        );
        assert_eq!((p.carry.phase, p.carry.step, p.carry.inhibition), (3, 1, 9));
        assert!(p.carry.upper.is_none());
        assert!(p.upper_cleared);
        assert_eq!(p.model_sub(), Some(("CarryFrontThrow", -1.)));
        for _ in 1..5 {
            p.update_carry(&Orders::default(), true);
            p.finish_carry_release(&Course::default());
            assert!(p.carry.held);
        }
        p.update_carry(&Orders::default(), true);
        assert!(p.carry.held);
        assert!(p.carry.release_due);
        p.finish_carry_release(&Course::default());
        assert!(!p.carry.held);
        assert!(!p.carry.release_due);
    }

    #[test]
    fn a_reconstructed_pickup_is_not_a_second_object_action() {
        let mut p = Player::default();
        p.carry.held = true;
        p.carry.phase = 1;
        p.carry.upper = Some(("CarryFrontStart", 0.));
        p.update_carry(
            &Orders {
                throw_pressed: true,
                ..default()
            },
            false,
        );
        assert_eq!((p.carry.phase, p.carry.step), (1, 1));
        assert!(p.model_sub().is_none());
        assert_eq!(p.fidelity_limits(), "carry_release_unavailable");
    }
}
