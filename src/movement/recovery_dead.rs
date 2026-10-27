//! PlayerRecoverySafetyPoint and the 3D PlayerStateRecoveryDead nerves.
//! Recovery's scalar arch is decoded from 0x479a1c..0x479b60; no fitted parabola.
use super::vector_lib::{length, verticalize_vec};
use super::*;

#[derive(Clone, Copy, Debug)]
pub struct SafetyPoint {
    pub position: Vec3,
    pub normal: Vec3,
    pub gravity: Vec3,
}
#[derive(Debug, Default)]
pub(super) struct Recovery {
    pub safety: Option<SafetyPoint>,
    pub assist: bool,
    pub rescue: bool,
    pub area_active: bool,
    pub push_invalid: u32,
    start: Vec3,
    up: Vec3,
    horizontal: Vec3,
    quadratic: f32,
    linear: f32,
    duration: u32,
    rotate_end: u32,
    rotation: f32,
    pose: Quat,
    axis: Vec3,
    root: Vec3,
}
impl Recovery {
    pub fn valid(&self) -> bool {
        self.safety.is_some() && (self.assist || self.rescue || self.area_active)
    }
}
fn rate(step: u32, begin: u32, end: u32) -> f32 {
    joint_math::calc_rate01(step as f32, begin as f32, end as f32)
}
fn ease_in_out(x: f32) -> f32 {
    // al::easeInOut = x*x*(3-2*x), kept in the original operation order.
    ((x * -2. + 3.) * x) * x
}
impl Player {
    pub(super) fn recovering(&self) -> bool {
        matches!(self.mode, Move::RecoveryStart | Move::RecoveryTravel | Move::RecoveryFall)
    }
    pub fn recovery_bubble(&self) -> bool {
        matches!(self.mode, Move::RecoveryStart | Move::RecoveryTravel)
    }
    pub fn enable_assist_recovery(&mut self, enabled: bool) {
        self.recovery.assist = enabled;
    }
    /// Explicit requests (recovery-area links and safe hosts) retain their own normal/gravity.
    pub fn set_safety_point(&mut self, position: Vec3, normal: Vec3, gravity: Vec3) {
        self.recovery.safety = Some(SafetyPoint { position, normal, gravity });
    }
    pub(super) fn begin_safety_recovery(&mut self) {
        self.damage.hp = self.damage.hp.saturating_sub(1);
        self.damage.invalid = 20;
        self.events.push(Fx::RecoveryDamage);
        self.nerve.clear();
        self.nerve.actor = "Abyss";
        self.nerve.step = 0;
        self.mode = Move::RecoveryStart;
        self.frame = 0;
    }
    fn recovery_choose(&mut self, next: Move) {
        self.mode = next;
        self.frame = 0;
        self.nerve.choose_silently(false);
    }
    fn recovery_travel_setup(&mut self) {
        let safety = self.recovery.safety.expect("valid recovery point");
        let start = self.position_cm();
        let up = Vec3::Y;
        let delta = safety.position - safety.gravity * 100. - start;
        let horizontal = verticalize_vec(up, delta);
        let vertical = delta - horizontal;
        let height = pose_math::dot(up, vertical);
        let distance = length(horizontal);
        let positive_height = height.max(0.);
        let duration = (distance.max(positive_height / 0.75) / 7.5).ceil().clamp(60., 120.) as u32;
        let arch = (distance * 0.75 - positive_height).clamp(300., 1500.);
        let peak = positive_height + if pose_math::dot(up, safety.gravity) > 0. { 0. } else { arch };
        let (quadratic, linear) = if peak.abs() < 0.001 {
            (0., height)
        } else {
            let root = (peak * peak - height * peak).sqrt();
            let t = if height.abs() < 0.001 { 0.5 } else { (peak - root) / height };
            let coefficient = peak / (t * t);
            (-coefficient, height + coefficient)
        };
        let pose = self.actor_pose();
        let rotate_end = (duration as f32 * 0.9 - 10.).max(0.).floor() as u32;
        // U04: the 3D parallel-gravity path is decoded. Arbitrary gravity and 2D need
        // their dimension controller; this controller currently supports world-down only.
        self.recovery.start = start;
        self.recovery.up = up;
        self.recovery.horizontal = horizontal;
        self.recovery.quadratic = quadratic;
        self.recovery.linear = linear;
        self.recovery.duration = duration;
        self.recovery.rotate_end = rotate_end;
        self.recovery.rotation = (rotate_end / 30) as f32 * 360.;
        self.recovery.pose = pose.calc_quat();
        self.recovery.axis = -pose.side();
        self.recovery.root = pose.up() * 80.;
        self.collider = PlayerCollider::default();
    }
    pub(super) fn recovery_step(&mut self, o: &Orders, _scene: &Course) {
        match self.mode {
            Move::RecoveryStart => {
                if self.frame == 0 {
                    self.recovery.start = self.position_cm();
                    self.sub.active = false;
                    self.sub_anim = None;
                    self.start_clip("TractorBubbleStart");
                    self.v = Vec3::ZERO;
                    self.events.push(Fx::RecoveryBegin);
                }
                let position = self.recovery.start + Vec3::Y * (joint_math::ease_out(rate(self.frame, 0, 30)) * 80.);
                self.pos = position * UNIT;
                self.collider_end = Some((self.pos, position));
                if self.frame >= 45 {
                    self.recovery_choose(Move::RecoveryTravel);
                }
            }
            Move::RecoveryTravel => {
                if self.frame == 0 {
                    self.start_clip("TractorBubbleWait");
                    self.recovery_travel_setup();
                }
                let r = &self.recovery;
                let progress = ease_in_out(ease_in_out(rate(self.frame, 0, r.duration)));
                let rotation = joint_math::ease_out(rate(self.frame, 10, r.rotate_end)) * r.rotation;
                let angle = rotation * 0.017_453_292;
                let half = angle * 0.5;
                let axis = r.axis * console_libm::sinf(half);
                let spin = Quat::from_xyzw(axis.x, axis.y, axis.z, console_libm::cosf(half));
                let pose = rotate_pose(r.pose, r.axis, angle);
                let offset = pose_math::rotate_by_quat(-r.root, spin) + r.root;
                let gravity = self.recovery.safety.expect("valid recovery").gravity;
                let scale = 1. - turn_control::ease_in(rate(self.frame, 0, r.rotate_end));
                let normal = gravity * pose_math::dot(gravity, offset);
                let offset = offset - normal + normal * scale;
                let height = r.quadratic * (progress * progress) + r.linear * progress;
                let position = r.start + r.horizontal * progress + r.up * height + offset;
                let ended = self.frame >= r.duration;
                self.set_pose(pose);
                self.v = position - self.position_cm();
                if ended {
                    self.pos = position * UNIT;
                    self.collider_end = Some((self.pos, position));
                    self.v = Vec3::ZERO;
                    self.collider.reset_expand_check(self.facing.x.atan2(self.facing.z));
                    self.recovery_choose(Move::RecoveryFall);
                }
            }
            Move::RecoveryFall => {
                if self.frame == 0 {
                    self.start_clip("TractorBubbleEnd");
                }
                let gravity = self.recovery.safety.expect("valid recovery").gravity;
                self.v += gravity * 3.;
                if let Some(wall) = self.touched.wall_contact.as_ref().filter(|w| w.at_face) {
                    let normal = wall.normal;
                    self.set_pose(self.actor_pose().slerp_up_front(-gravity, -normal, 0.4, 180.));
                    self.pos -= normal * (10. * UNIT);
                }
                if self.grounded && self.is_on_ground_by(self.v) {
                    self.recovery.area_active = false;
                    self.damage.invalid = self.damage.invalid.max(120);
                    self.recovery.push_invalid = 120;
                    if self.pre_jump > 0 {
                        self.ground_jump(o);
                    } else if self.is_move() {
                        self.run_appear(true);
                        self.go(Move::Run, "Move");
                    } else if o.crouch {
                        self.start_squat();
                    } else {
                        self.go(Move::Idle, "Wait");
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

/// al::rotateQuatRadian (0x92783c), including its normalization and scalar order.
fn rotate_pose(q: Quat, axis: Vec3, angle: f32) -> Quat {
    let half = angle * 0.5;
    let c = console_libm::cosf(half);
    let s = console_libm::sinf(half);
    let a = axis * s;
    let w = ((c * q.w - a.x * q.x) - a.y * q.y) - a.z * q.z;
    let x = ((a.x * q.w + c * q.x) + a.y * q.z) - a.z * q.y;
    let y = ((c * q.y - a.x * q.z) + a.y * q.w) + a.z * q.x;
    let z = ((a.x * q.y + c * q.z) - a.y * q.x) + q.w * a.z;
    let norm = (((w * w + x * x) + y * y) + z * z).sqrt();
    if norm > 0. {
        let inverse = 1. / norm;
        Quat::from_xyzw(inverse * x, inverse * y, inverse * z, inverse * w)
    } else {
        Quat::from_xyzw(x, y, z, w)
    }
}

pub(super) fn self_check() {
    checks::safety_enablement_and_recovery_lifecycle();
    checks::death_judge_and_scene_restart();
    checks::rotation_oracle();
}
mod checks {
    use super::*;

    #[cfg_attr(test, test)]
    pub(super) fn safety_enablement_and_recovery_lifecycle() {
        let scene = crate::replay::flat_floor();
        let mut p = Player::spawn_facing(Vec3::new(0., -25., 0.), 0.);
        p.start_in_air();
        p.set_safety_point(Vec3::ZERO, Vec3::Y, Vec3::NEG_Y);
        assert!(!p.recovery.valid(), "a saved point alone cannot enable recovery");
        p.enable_assist_recovery(true);
        assert!(p.recovery.valid());
        p.begin_safety_recovery();
        assert_eq!(p.damage.hp, 2);
        let mut visited_travel = false;
        let mut visited_fall = false;
        for _ in 0..220 {
            p.step(&Orders::default(), &scene);
            visited_travel |= p.mode == Move::RecoveryTravel;
            visited_fall |= p.mode == Move::RecoveryFall;
            if !p.recovering() {
                assert!(visited_travel && visited_fall);
                assert_eq!(p.damage.invalid, 119);
                assert_eq!(p.recovery.push_invalid, 119);
                assert!(!p.recovery.area_active);
                return;
            }
        }
        panic!("recovery never landed: {:?}", p.mode);
    }

    #[cfg_attr(test, test)]
    pub(super) fn death_judge_and_scene_restart() {
        let mut d = death::Death::default();
        for _ in 0..30 {
            d.update_judge(false);
        }
        assert_eq!(d.counter, -1);
        for _ in 0..30 {
            d.update_judge(true);
        }
        assert!(!d.ready());
        d.update_judge(true);
        assert!(d.ready());
        let scene = Course::default();
        let mut p = Player::spawn_at(Vec3::new(0., -20., 0.), Vec3::NEG_Z);
        p.death.restart_pose = Some((SPAWN, 180.));
        for _ in 0..300 {
            p.step(&Orders::default(), &scene);
            if p.death.generation > 0 {
                assert!(!p.is_dead());
                assert_eq!(p.damage.hp, 3);
                assert_eq!(p.soot, Soot::default());
                return;
            }
        }
        panic!("dead toy player never restarted");
    }

    #[cfg_attr(test, test)]
    pub(super) fn rotation_oracle() {
        let Ok(path) = std::env::var("PJV_RECOVERY_CASES") else { return };
        let cases: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for case in cases.as_array().unwrap() {
            let values = |key: &str| -> Vec<f32> {
                case[key].as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect()
            };
            let q = values("q");
            let axis = values("axis");
            let got = rotate_pose(Quat::from_xyzw(q[0],q[1],q[2],q[3]), Vec3::new(axis[0],axis[1],axis[2]), case["angle"].as_f64().unwrap() as f32);
            let expected = values("out");
            for (actual, expected) in got.to_array().into_iter().zip(expected) {
                assert_eq!(actual.to_bits(), expected.to_bits(), "{case}");
            }
        }
    }
}
