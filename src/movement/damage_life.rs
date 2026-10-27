//! PlayerStateDamageLife (matching 1.0.0 decomp, 0x469750..0x469f38).
use super::vector_lib::verticalize_vec;
use super::*;

#[derive(Debug)]
pub(super) struct LifeControl {
    pub up: Vec3,
    pub no_down: bool,
    pub poison: bool,
    pub air: air_move::AirMove,
    pub turn: ground::TurnControl,
    /// Replay supplies the original getU32 draws, never a chosen clip or state.
    pub random: std::collections::VecDeque<u32>,
    rng: [u32; 4],
}
impl Default for LifeControl {
    fn default() -> Self {
        Self {
            up: Vec3::Y,
            no_down: false,
            poison: false,
            air: air_move::AirMove::default(),
            turn: ground::TurnControl::air(),
            random: Default::default(),
            // Host seed only; the source generator is sead::Random::getU32.
            rng: [123456789, 362436069, 521288629, 88675123],
        }
    }
}
impl LifeControl {
    fn random_int(&mut self, max: u32) -> u32 {
        let word = self.random.pop_front().unwrap_or_else(|| {
            let x = self.rng[0] ^ (self.rng[0] << 11);
            let next = x ^ (x >> 8) ^ self.rng[3] ^ (self.rng[3] >> 19);
            self.rng = [self.rng[1], self.rng[2], self.rng[3], next];
            next
        });
        // al::getRandom(int,int) 0x922914: construct [1,2), subtract 1,
        // multiply the exclusive interval and truncate toward zero.
        ((f32::from_bits(0x3f800000 | (word >> 9)) - 1.) * max as f32) as u32
    }
    fn dead_clip(&mut self) -> &'static str {
        if self.random_int(20) == 0 {
            "Dead04"
        } else if self.random_int(2) == 0 {
            "Dead01"
        } else {
            "Dead02"
        }
    }
}
impl Player {
    pub fn set_damage_random(&mut self, words: impl IntoIterator<Item = u32>) {
        self.life.random = words.into_iter().collect();
    }
    pub(super) fn appear_damage_life(&mut self, poison: bool, land: bool) {
        self.nerve.clear();
        self.nerve.actor = "Damage";
        self.nerve.step = 0;
        self.frame = 0;
        self.life.up = self.ground_normal_or_up();
        self.life.no_down = false;
        self.life.poison = poison;
        self.cap_jump_possible = true;
        if self.is_dead() {
            self.v = Vec3::ZERO;
            self.mode = Move::DamageDead;
        } else if land {
            self.mode = Move::DamageLand;
        } else {
            self.mode = Move::DamageAir;
            self.v = self.pose_axes().1 * -2.;
            self.damage_air_setup(30., 2., 12., 0.95);
        }
    }
    pub(super) fn damage_choose(&mut self, mode: Move) {
        self.mode = mode;
        self.frame = 0;
        self.nerve.choose_silently(false);
    }
    pub(super) fn damage_air_setup(&mut self, cap: f32, base: f32, power: f32, gravity: f32) {
        let support = self.ground_normal_or_up();
        self.v = verticalize_vec(Vec3::Y, verticalize_vec(support, self.v)) + Vec3::Y * self.v.y;
        self.life.air.setup(
            &mut self.v,
            self.facing,
            &air_move::Takeoff {
                speed_limit: cap,
                base_speed: base,
                extend_frames: 0,
                power,
                gravity,
                input_lock: 9999,
                speed_window: None,
                fall_max: 35.,
            },
        );
        self.life.turn.reset();
    }
    pub(super) fn damage_air_update(&mut self) {
        let input = self.plane_stick(Vec3::Y);
        self.life.air.update(
            &mut self.v,
            input,
            false,
            &air_move::Steering {
                front: 0.5,
                back: 1.,
                turn: 0.3,
            },
        );
        let (actor_up, actor_front) = self.pose_axes();
        self.life.turn.update(turn_control::TurnFrame {
            actor_front,
            actor_up,
            stick: if self.life.air.steered() {
                input
            } else {
                Vec3::ZERO
            },
            up: Vec3::Y,
        });
        self.set_pose(self.actor_pose().slerp_up_front(
            Vec3::Y,
            self.life.turn.direction,
            t::SLERP_QUAT_RATE,
            0.,
        ));
    }
    fn damage_on_ground(&self) -> bool {
        self.is_on_ground_by(self.v)
            && vector_lib::is_floor_polygon_cos(
                self.collided_ground_normal(),
                Vec3::NEG_Y,
                vector_lib::cos_of_degrees(t::STAND_ANGLE_MIN),
            )
    }
    fn damage_brake_land(&mut self, reference: Vec3) {
        // rs::brakeLandVelocityGroundNormal 0x55eaa4: the stored up is
        // refreshed, tangent is scaled by BrakeLandUpFactor (zero), then pull3.
        let reference = if self.is_just_land() {
            Vec3::Y
        } else {
            reference
        };
        self.life.up = self.ground_normal_or_up();
        self.v = pose_math::along_vector_normal_h(self.v, reference, self.life.up) * 0.
            - self.life.up * 3.;
    }
    pub(super) fn damage_face_camera(&mut self) {
        let up = self.pose_axes().0;
        let front = self
            .camera_cm
            .map_or(-self.forward, |c| c - self.position_cm());
        let front = pose_math::try_normalize_or_zero(verticalize_vec(up, front)).0;
        if !vector_lib::is_near_zero(front, 0.001) {
            self.set_pose(pose_math::quat_up_front(up, front));
        }
    }
    pub(super) fn damage_dead_first(&mut self, clip: Option<&'static str>, align_up: bool) {
        self.sub.active = false;
        self.sub_anim = None;
        let clip = clip.unwrap_or_else(|| self.life.dead_clip());
        self.start_clip(clip);
        self.damage_face_camera();
        if align_up {
            self.slerp_pose_up(Vec3::Y, 1., 0.);
        }
    }
    fn damage_end_on_ground(&mut self, o: &Orders) {
        if !self.grounded {
            self.go(Move::Fall, "Fall");
        } else if self.pre_jump > 0 {
            self.ground_jump(o);
        } else if self.is_move() {
            self.run_appear(true);
            self.go(Move::Run, "Move");
        } else if o.crouch {
            self.start_squat();
        } else {
            self.go(
                Move::Idle,
                if self.damage.hp == 1 {
                    "DamageWait"
                } else {
                    "Wait"
                },
            );
        }
    }
    pub(super) fn damage_life_step(&mut self, o: &Orders, scene: &Course) {
        if !self.is_dead() && self.fire_throws(o) {
            return;
        }
        let mut ended = false;
        match self.mode {
            Move::DamageAir => {
                if self.frame == 0 {
                    self.sub.active = false;
                    self.sub_anim = None;
                    self.start_clip(if self.life.no_down {
                        "NoDamageDown"
                    } else {
                        "DamageDown"
                    });
                }
                self.damage_air_update();
                if self.damage_on_ground() {
                    if self.is_just_land() {
                        self.events.push(Fx::Land { hard: false });
                    }
                    self.damage_brake_land(Vec3::Y);
                    self.damage_choose(Move::DamageLand);
                } else {
                    ended = self.frame >= 120;
                }
            }
            Move::DamageLand => {
                if self.frame == 0 {
                    self.start_clip("DamageLand");
                }
                if self.damage_on_ground() {
                    self.damage_brake_land(self.life.up);
                } else {
                    air_move::add_vector_limit(&mut self.v, Vec3::NEG_Y * 1.5, 35.);
                }
                ended = self.anim_done(78);
            }
            Move::DamageDead => {
                if self.frame == 0 {
                    self.damage_dead_first(self.life.poison.then_some("DeadPoison"), true);
                }
                if self.anim_done(190) {
                    self.damage_choose(Move::Dead);
                    self.nerve.renewed = true;
                }
                return;
            }
            _ => unreachable!(),
        }
        if ended {
            self.damage_end_on_ground(o);
        } else if self.is_in_water(scene, swim::ACTOR_JUDGE) {
            if self.mode == Move::DamageLand {
                self.start_swim(scene);
            } else {
                self.appear_damage_swim(scene, true);
                self.nerve.choose_silently(true);
            }
        } else if self.mode == Move::DamageLand
            && self.frame + u32::from(!self.nerve.deferred) >= 45
            && self.pre_jump > 0
        {
            self.ground_jump(o);
        }
    }
}
