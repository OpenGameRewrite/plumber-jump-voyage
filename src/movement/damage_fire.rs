//! PlayerStateDamageFire (SMO 1.0.0, 0x468690..0x46962c), expressed through the existing
//! air/ground and nerve controls. Actor damage entry is immediate; child changes are deferred.
use super::ground::{GroundMove, SteerInput};
use super::vector_lib::{is_near_zero, verticalize_vec};
use super::*;

#[derive(Debug)]
pub(super) struct FireControl {
    pub frames: u32,
    pub brake: u32,
    floor: bool,
    wall: bool,
    ceiling: bool,
    air: air_move::AirMove,
    turn: ground::TurnControl,
    ground: GroundMove,
}
impl Default for FireControl {
    fn default() -> Self {
        Self {
            frames: 0,
            brake: 0,
            floor: false,
            wall: false,
            ceiling: false,
            air: air_move::AirMove::default(),
            turn: ground::TurnControl::air(),
            ground: GroundMove::fire(),
        }
    }
}
fn takeoff(power: f32, extend: u32, lock: u32, gravity: f32) -> air_move::Takeoff {
    air_move::Takeoff {
        speed_limit: 10.,
        base_speed: 10.,
        extend_frames: extend,
        power,
        gravity,
        input_lock: lock,
        speed_window: None,
        fall_max: 35.,
    }
}
impl Player {
    /// `rs::isCollidedGroundRunAngle` (0x56968c..0x56989c). First contact measures
    /// speed across gravity; subsequent contacts measure it across the floor normal.
    pub(super) fn fire_run_angle_contact(&self) -> bool {
        if !self.is_collided_ground() {
            return false;
        }
        let normal = self.collided_ground_normal();
        let reference = if self.is_just_land() {
            Vec3::NEG_Y
        } else {
            normal
        };
        let speed = vector_lib::length(verticalize_vec(reference, self.v));
        // PlayerConst: StandAngleMin/Max 60/70; HillAngleSpeedMin/Max 21/26.
        let rate = turn_control::ease_in(joint_math::calc_rate01(speed, 21., 26.));
        let angle = joint_math::lerp_value(t::STAND_ANGLE_MIN, 70., rate);
        vector_lib::is_floor_polygon_cos(normal, Vec3::NEG_Y, vector_lib::cos_of_degrees(angle))
    }

    pub fn fire_sub_steps(&self) -> Option<u32> {
        matches!(
            self.mode,
            Move::BurnJump
                | Move::BurnBounce
                | Move::FireRun
                | Move::FireFall
                | Move::FireDead
                | Move::AbyssFall
                | Move::RecoveryStart
                | Move::RecoveryTravel
                | Move::RecoveryFall
                | Move::DamageAir | Move::DamageLand | Move::DamageDead
                | Move::DamageSwim | Move::DamageSurface | Move::DamageLandWater | Move::DamageSwimDead
        )
        .then_some(self.frame)
    }
    pub(super) fn burning(&self) -> bool {
        matches!(
            self.mode,
            Move::BurnJump | Move::BurnBounce | Move::FireRun | Move::FireFall
        )
    }
    /// `appear` snapshots fire codes, resets the shared Run/Fall counter, and chooses Dead
    /// from HP. The pre-movement entry executes its first step in this same update.
    pub(super) fn appear_damage_fire(&mut self, floor: bool, wall: bool, ceiling: bool) {
        self.nerve.clear();
        self.nerve.actor = "DamageFire";
        self.nerve.step = 0;
        self.frame = 0;
        self.fire = FireControl {
            floor,
            wall,
            ceiling,
            ..FireControl::default()
        };
        if self.is_dead() {
            self.v = Vec3::ZERO;
            self.mode = Move::FireDead;
        } else {
            if self.is_just_land() {
                let normal = self.mesh_ground.unwrap_or(Vec3::Y);
                self.v = pose_math::along_vector_normal_h(self.v, Vec3::Y, normal) - normal * 3.;
            }
            self.mode = Move::BurnJump;
        }
    }
    fn fire_choose(&mut self, mode: Move) {
        self.mode = mode;
        self.frame = 0;
        self.nerve.choose_silently(false);
    }
    fn fire_setup_air(&mut self, power: f32, extend: u32, lock: u32, gravity: f32) {
        let support = self
            .mesh_ground
            .filter(|_| self.is_collided_ground())
            .unwrap_or(Vec3::Y);
        let horizontal = verticalize_vec(Vec3::Y, verticalize_vec(support, self.v));
        self.v = horizontal + Vec3::Y * self.v.y;
        self.fire.air.setup(
            &mut self.v,
            self.facing,
            &takeoff(power, extend, lock, gravity),
        );
        self.fire.turn.reset();
    }
    fn fire_jump_first(&mut self, second: bool) {
        self.start_clip("FireRunStart");
        self.events.push(Fx::Burned(second));
        let support = self
            .mesh_ground
            .filter(|_| self.is_collided_ground())
            .unwrap_or(Vec3::Y);
        let wall_only = !second && self.fire.wall && !self.fire.floor && !self.fire.ceiling;
        let lock = if !second && self.fire.wall { 10 } else { 0 };
        if wall_only {
            let normal = self.collider.wall.as_ref().map_or(self.wall, |c| c.normal);
            self.v = normal * 10.;
            self.fire_setup_air(20., 0, lock, 0.9);
            self.set_pose(pose_math::quat_up_front(Vec3::Y, normal));
        } else {
            let normal = if !second && self.fire.ceiling {
                self.collider
                    .ceiling
                    .as_ref()
                    .map_or(Vec3::NEG_Y, |c| c.normal)
            } else {
                support
            };
            let parallel = normal * pose_math::dot(normal, self.v);
            let tangent = self.v - parallel;
            let direction = if !second && self.fire.wall {
                self.collider.wall.as_ref().map_or(self.wall, |c| c.normal)
            } else {
                let up = if self.fire.ceiling && !second {
                    -normal
                } else {
                    normal
                };
                pose_math::try_normalize_or_zero(self.plane_stick(up)).0
            };
            if !second || !is_near_zero(direction, 0.001) {
                let normal_part = if !second && self.fire.ceiling {
                    normal
                } else {
                    parallel
                };
                self.v = normal_part + direction * tangent.length();
            }
            let ceiling = !second && self.fire.ceiling;
            self.fire_setup_air(
                if ceiling { 0. } else { 20. },
                if !second && !ceiling { 20 } else { 0 },
                lock,
                0.9,
            );
        }
    }
    fn fire_air_update(&mut self) {
        let input = self.plane_stick(Vec3::Y);
        // exeJump forces the air controller's extend hold flag; B release is ignored.
        self.fire.air.update(
            &mut self.v,
            input,
            true,
            &air_move::Steering {
                front: 0.5,
                back: 1.,
                turn: 0.3,
            },
        );
        let (actor_up, actor_front) = self.pose_axes();
        self.fire.turn.update(turn_control::TurnFrame {
            actor_front,
            actor_up,
            stick: if self.fire.air.steered() {
                input
            } else {
                Vec3::ZERO
            },
            up: Vec3::Y,
        });
        let front = self.fire.turn.direction;
        if let Some(stored) = self.pose_quat() {
            self.set_pose(plane::ActorPose::new(stored).slerp_up_front(
                Vec3::Y,
                front,
                t::SLERP_QUAT_RATE,
                0.,
            ));
        } else {
            self.facing = front;
        }
    }
    fn fire_ground_appear(&mut self) {
        let up = self.mesh_ground.unwrap_or(Vec3::Y);
        let front = self.pose_axes().1;
        self.fire
            .ground
            .reset(verticalize_vec(up, self.v), front, up);
    }
    fn fire_run_motion(&mut self) {
        let up = self.mesh_ground.unwrap_or(Vec3::Y);
        let (actor_up, actor_front) = self.pose_axes();
        let steer = self.fire.ground.update(&SteerInput {
            velocity: verticalize_vec(up, self.v),
            up,
            actor_front,
            actor_up,
            stick: self.plane_stick(up),
            wall: self
                .touched
                .wall_contact
                .as_ref()
                .map(|c| (c.normal, c.at_face)),
            max_speed: 18.,
            accel_frames: 1,
        });
        self.v = steer.set_velocity;
        // The fire constructor enables the ground control's turn tilt (byte +0xbb).
        let pose_up = self.fire.ground.turn.tilted_up(steer.up, 20.);
        self.set_pose(self.actor_pose().slerp_up_front(
            pose_up,
            steer.front,
            t::SLERP_QUAT_RATE,
            t::HILL_POSE_DEGREE_MAX,
        ));
        air_move::add_vector_limit(
            &mut self.v,
            self.fire.ground.movement_direction() * 18.,
            18.,
        );
        if self.is_move() {
            self.fire.brake = 0;
        } else {
            self.fire.brake = (self.fire.brake + 1).min(20);
            let rate = joint_math::calc_rate01(self.fire.brake as f32, 0., 20.);
            let scale = (1. - joint_math::ease_out(rate)).clamp(0., 1.);
            let parallel = up * pose_math::dot(up, self.v);
            self.v = parallel + (self.v - parallel) * scale;
        }
    }
    pub(super) fn damage_fire_step(&mut self, o: &Orders, _scene: &Course) {
        let dead = self.is_dead();
        if dead && self.frame == 0 {
            self.soot.record_damage_fire_dead();
        }
        // The actor tries the independent cap throw before updating the child state.
        if !dead && self.fire_throws(o) {
            return;
        }
        match self.mode {
            Move::BurnJump | Move::BurnBounce => {
                let second = self.mode == Move::BurnBounce;
                if self.frame == 0 {
                    self.fire_jump_first(second);
                }
                self.fire_air_update();
                if self.grounded && self.is_on_ground_by(self.v) {
                    if second {
                        self.fire.frames = 0;
                        self.fire_ground_appear();
                        self.fire_choose(Move::FireRun);
                    } else {
                        let up = self.mesh_ground.unwrap_or(Vec3::Y);
                        let tangent = pose_math::along_vector_normal_h(self.v, Vec3::Y, up);
                        self.v = if self.is_move() { tangent } else { Vec3::ZERO } - up * 3.;
                        self.fire_choose(Move::BurnBounce);
                    }
                } else {
                    self.grounded = false;
                }
            }
            Move::FireRun => {
                if self.frame == 0 {
                    if self.anim.name != "FireRun" {
                        self.start_clip("FireRun");
                        self.anim.rate = 18. * 0.3;
                    }
                    self.ground_lost = 0;
                    self.fire_ground_appear();
                    self.fire.brake = 20;
                }
                self.fire_run_motion();
                if self.ground_lost >= 5 {
                    self.fire_choose(Move::FireFall);
                } else if self.frame >= 60 {
                    self.end_burn();
                }
            }
            Move::FireFall => {
                if self.frame == 0 {
                    self.fire_setup_air(0., 0, 0, 1.5);
                }
                self.fire_air_update();
                if self.grounded && self.is_on_ground_by(self.v) {
                    self.fire_ground_appear();
                    self.fire_choose(Move::FireRun);
                }
            }
            Move::FireDead => {
                if self.frame == 0 {
                    self.sub.active = false;
                    self.sub_anim = None;
                    self.start_clip("DeadFire");
                    self.slerp_pose_up(Vec3::Y, 1., 0.);
                    let front = self
                        .camera_cm
                        .map_or(-self.forward, |camera| camera - self.position_cm());
                    let up = self.pose_axes().0;
                    let (front, valid) =
                        pose_math::try_normalize_or_zero(verticalize_vec(up, front));
                    if valid {
                        self.set_pose(pose_math::quat_up_front(up, front));
                    }
                }
                if self.anim_done(190) {
                    self.mode = Move::Dead;
                    self.frame = 0;
                    self.nerve.choose_silently(true);
                }
            }
            _ => unreachable!(),
        }
        if dead {
            return;
        }
        self.soot.record_damage_fire();
        if matches!(self.mode, Move::FireRun | Move::FireFall) {
            if self.fire.frames >= 60 {
                self.end_burn();
            }
            self.fire.frames += 1;
        }
        // Actor cancellation follows child update: post-first-land spin/shallow material,
        // then buffered jump in Run. Immersion is handled by the existing water judge.
        if self.mode != Move::BurnJump
            && self.throw_kind(o) == Throw::Spin
            && (o.spin_throw || self.pre_throw > 0)
            && self.cap.is_none()
        {
            self.pre_throw = 0;
            self.start_throw(Throw::Spin);
        } else if self.mode != Move::BurnJump && self.grounded && self.material_puddle {
            self.events.push(Fx::Quenched);
            self.end_burn();
        } else if self.mode == Move::FireRun && self.pre_jump > 0 {
            self.ground_jump(o);
        }
    }
}
