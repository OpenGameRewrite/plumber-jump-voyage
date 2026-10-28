//! PlayerStateSandSink (0x480574), PlayerJudgeSandSink (matching decomp), and
//! PlayerSandSinkAffect (0x461dcc). All distances are original centimetres.
use super::vector_lib::{length, verticalize_vec};
use super::*;

pub(super) const HEIGHT: f32 = 200.;
pub(super) const DEAD_TIME: f32 = 240.;

/// Sand owns the ordinary pivot controller with GravityMove=7. The run's
/// older toy pivot has different pose integration, so do not change it here.
#[derive(Debug, Default)]
struct SandPivot {
    target: Vec3,
    steps: u32,
    has_input: bool,
    done: bool,
}
impl SandPivot {
    fn reset(&mut self, front: Vec3) {
        *self = Self {
            target: front,
            ..Self::default()
        };
    }
    fn update(&mut self, stick: Vec3, front: Vec3, velocity: Vec3, up: Vec3) -> ground::PivotStep {
        let (wanted, moving) = pose_math::try_normalize_or_zero(stick);
        self.has_input = moving;
        if moving {
            self.target = wanted;
        }
        let (target, valid) = pose_math::try_normalize_or_zero(verticalize_vec(up, self.target));
        let (front, facing) = pose_math::try_normalize_or_zero(verticalize_vec(up, front));
        if !valid || !facing {
            self.done = true;
            return ground::PivotStep { front, velocity };
        }
        self.steps = (self.steps + 1).min(10);
        // makeQuatAxisRotation: signed atan2, degrees, fraction, radians, half-angle.
        let cross = pose_math::cross(front, target);
        let angle = console_libm::atan2f(length(cross), pose_math::dot(front, target));
        let angle = if pose_math::dot(up, cross) < 0. {
            -angle
        } else {
            angle
        };
        let degrees = (angle * 57.295_776) * (self.steps as f32 / 10.);
        let half = (degrees * pose_math::DEGREES_TO_RADIANS) * 0.5;
        let sin = console_libm::sinf(half);
        let q = Quat::from_xyzw(up.x * sin, up.y * sin, up.z * sin, console_libm::cosf(half));
        let swung = pose_math::try_normalize_or_zero(pose_math::rotate_by_quat(front, q)).0;
        // makeQuatRotationLimit answers false when the remaining angle is <=5 degrees.
        let left = console_libm::atan2f(
            length(pose_math::cross(swung, target)),
            pose_math::dot(swung, target),
        );
        self.done = left <= 0.087_266_46;
        if self.done {
            let parallel = up * pose_math::dot(up, velocity);
            ground::PivotStep {
                front: target,
                velocity: parallel + target * length(velocity - parallel),
            }
        } else {
            ground::PivotStep {
                front: swung,
                velocity,
            }
        }
    }
}

#[derive(Debug)]
pub(super) struct SandControl {
    /// Affect +0x28: accumulated arrow offset; +0x2c: surface height correction.
    pub depth: f32,
    surface_offset: f32,
    safe: bool,
    ground: GroundMove,
    normal: Vec3,
    pivot: SandPivot,
    air: AirMove,
    turn: ground::TurnControl,
    pub effect: Option<&'static str>,
    pub called: bool,
    pub weight: f32,
}
impl Default for SandControl {
    fn default() -> Self {
        Self {
            depth: 0.,
            surface_offset: 0.,
            safe: false,
            ground: GroundMove::sand(),
            normal: Vec3::Y,
            pivot: SandPivot::default(),
            air: AirMove::default(),
            turn: ground::TurnControl::air(),
            effect: None,
            called: false,
            weight: 0.,
        }
    }
}
impl Player {
    /// The toy course's sand under the body: the surface height in metres (the box colliders
    /// stand for the stage's `SandSink` triangles; a stage's own collision never asks).
    fn toy_sand_surface(&self, scene: &Course) -> Option<f32> {
        Self::toy_sand_surface_at(scene, self.pos)
    }
    pub(super) fn toy_sand_surface_at(scene: &Course, at: Vec3) -> Option<f32> {
        scene
            .sands
            .iter()
            .find(|(centre, half)| {
                (at.x - centre.x).abs() <= half.x && (at.z - centre.z).abs() <= half.z
            })
            .map(|(centre, _)| centre.y)
    }
    /// How far the toy course's solid under the sand lets the body sink: its sunk depth in
    /// metres, for a solid whose top is the sand's surface.
    pub(super) fn toy_sand_sink(&self, scene: &Course, solid_top: f32) -> f32 {
        match self.toy_sand_surface(scene) {
            Some(surface) if (surface - solid_top).abs() < 1e-3 => self.sand.depth * UNIT,
            _ => 0.,
        }
    }
    /// The toy course's collision answers `SandSink` for the body standing on its sand, like
    /// the loaded triangles of a stage do.
    pub(super) fn mark_toy_sand_floor(&mut self, scene: &Course) {
        if self.grounded
            && self
                .toy_sand_surface(scene)
                .is_some_and(|y| self.pos.y <= y + 1e-4)
        {
            self.touched.active = true;
            self.touched.floor = Some(CollisionCodes {
                floor: "SandSink".into(),
                material: "SandDesert".into(),
                ..default()
            });
        }
    }
    pub(super) fn in_sand_state(&self) -> bool {
        matches!(
            self.mode,
            Move::SandWait | Move::SandRun | Move::SandPivot | Move::SandJump | Move::SandDead
        )
    }
    fn sand_floor(&self) -> bool {
        self.collided_ground_normal().y >= 0.
            && self
                .touched
                .floor
                .as_ref()
                .is_some_and(|c| c.floor == "SandSink")
    }
    pub(super) fn judge_sand_sink(&self) -> bool {
        self.sand.depth > 0. || self.is_collided_ground() && self.sand_floor()
    }
    pub(super) fn judge_sand_entry(&mut self) {
        if !self.is_dead()
            && !self.recovering()
            && !self.in_sand_state()
            && !self.nerve.deferred
            && self.judge_sand_sink()
            && (self.mode != Move::SlopeSlide || self.sand_slope_cancel())
        {
            self.appear_sand_sink();
        }
    }
    pub(super) fn appear_sand_sink(&mut self) {
        self.nerve.touchdown = false;
        if self.is_just_land() {
            self.events.push(Fx::Land { hard: false });
        }
        self.sand.ground = GroundMove::sand();
        self.sand_ground_appear();
        self.slerp_pose_up(Vec3::Y, 1., 0.);
        self.sub.active = false;
        self.sub_anim = None;
        self.sand_choose(if self.is_move() {
            Move::SandRun
        } else {
            Move::SandWait
        });
        self.nerve.renewed = true;
        self.cap_jump_possible = true;
    }
    fn sand_ground_appear(&mut self) {
        let up = self.collided_ground_normal();
        let from = if self.is_just_land() { Vec3::Y } else { up };
        self.v = pose_math::along_vector_normal_h(self.v, from, up) - up * GRAVITY_MOVE;
        self.sand.normal = up;
        self.sand
            .ground
            .reset(verticalize_vec(up, self.v), self.pose_axes().1, up);
    }
    fn sand_choose(&mut self, mode: Move) {
        self.mode = mode;
        self.frame = 0;
        self.nerve.choose_silently(false);
    }
    pub(super) fn dispatch_sand_death(&mut self) {
        if !self.in_sand_state()
            || self.is_dead()
            || self.sand.depth + self.sand.surface_offset < HEIGHT
        {
            return;
        }
        if self.recovery.valid() && self.damage.hp > 1 {
            self.begin_safety_recovery();
            return;
        }
        self.damage.hp = 0;
        self.death.kind = death::Kind::Sand;
        self.sand_choose(Move::SandDead);
        // Pre-movement setSandSinkDead executes Dead's first step immediately.
        self.nerve.clear();
    }
    fn sand_animation(&mut self, death_rate: f32) {
        self.sand.weight = joint_math::ease_out(joint_math::calc_rate01(death_rate, 0., 0.5));
        let speed = joint_math::ease_out(joint_math::calc_rate01(death_rate, 0., 0.5));
        let rate = joint_math::lerp_value(4., 0.5, speed);
        if !self.anim_skip {
            self.anim_rate_change += rate - self.anim.rate;
        }
        self.anim.rate = rate;
    }
    pub(super) fn sand_step(&mut self, o: &Orders) {
        let rate = joint_math::calc_rate01(self.sand.depth + self.sand.surface_offset, 0., HEIGHT);
        match self.mode {
            Move::SandDead => {
                if self.frame == 0 {
                    self.sub.active = false;
                    self.sub_anim = None;
                    self.start_clip("DeadSand");
                    let front = self
                        .camera_cm
                        .map_or(-self.forward, |c| c - self.position_cm());
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
                return;
            }
            Move::SandJump => self.sand_jump(o),
            Move::SandWait => {
                if self.frame == 0 {
                    self.start_clip("SandWait");
                }
                let up = self.collided_ground_normal();
                self.v = pose_math::along_vector_normal_h(self.v, self.sand.normal, up);
                self.sand.normal = up;
                // reboundVelocityFromCollision with zero restitution, then Gravity.
                if self.is_collided_ground() {
                    self.v = Vec3::ZERO;
                }
                self.v -= up * t::GRAVITY;
                self.slerp_pose_up(Vec3::Y, 1., 0.);
                self.sand_animation(rate);
                if self.pre_input_jump_judges() {
                    self.pre_jump = 0;
                    self.sand_choose(Move::SandJump);
                } else if self.is_move() {
                    self.sand_choose(Move::SandRun);
                }
            }
            Move::SandRun => {
                let up = self.collided_ground_normal();
                if self.frame == 0 {
                    self.sand
                        .ground
                        .reset(verticalize_vec(up, self.v), self.pose_axes().1, up);
                    self.start_clip("SandWalk");
                }
                let v = pose_math::along_vector_normal_h(self.v, self.sand.normal, up);
                self.sand.normal = up;
                let (actor_up, actor_front) = self.pose_axes();
                let input = SteerInput {
                    skate: false,
                    velocity: v,
                    up,
                    actor_front,
                    actor_up,
                    stick: self.plane_stick(up),
                    wall: self
                        .touched
                        .wall_contact
                        .as_ref()
                        .map(|c| (c.normal, c.at_face)),
                    max_speed: t::NORMAL_MAX,
                    accel_frames: t::ACCEL_FRAMES as i32,
                };
                let steer = self.sand.ground.update(&input);
                self.v = steer.set_velocity;
                self.set_pose(self.actor_pose().slerp_up_front(
                    steer.up,
                    steer.front,
                    t::SLERP_QUAT_RATE,
                    t::HILL_POSE_DEGREE_MAX,
                ));
                self.slerp_pose_up(Vec3::Y, 1., 0.);
                self.sand_animation(rate);
                if self.pre_input_jump_judges() {
                    self.pre_jump = 0;
                    self.sand_choose(Move::SandJump);
                } else if steer.pivot {
                    self.sand_choose(Move::SandPivot);
                } else if steer.stopped || steer.brake_run {
                    self.sand_choose(Move::SandWait);
                }
            }
            Move::SandPivot => {
                if self.frame == 0 {
                    self.sand.pivot.reset(self.pose_axes().1);
                }
                let up = self.collided_ground_normal();
                let v = pose_math::along_vector_normal_h(self.v, self.sand.normal, up)
                    - up * GRAVITY_MOVE;
                self.sand.normal = up;
                let step = self
                    .sand
                    .pivot
                    .update(self.ground_stick(), self.pose_axes().1, v, up);
                self.v = step.velocity;
                self.set_pose(self.actor_pose().slerp_up_front(
                    self.sand.normal,
                    step.front,
                    t::SLERP_QUAT_RATE,
                    t::HILL_POSE_DEGREE_MAX,
                ));
                self.slerp_pose_up(Vec3::Y, 1., 0.);
                self.sand_animation(rate);
                if self.pre_input_jump_judges() {
                    self.pre_jump = 0;
                    self.sand_choose(Move::SandJump);
                } else if self.sand.pivot.done {
                    self.sand_choose(if self.sand.pivot.has_input {
                        Move::SandRun
                    } else {
                        Move::SandWait
                    });
                }
            }
            _ => unreachable!(),
        }
        // PlayerActorHakoniwa::exeSandSink asks after the child's step.
        if self.mode == Move::SandJump {
            if self.grounded && self.is_on_ground_by(self.v) && !self.judge_sand_sink() {
                self.sand_end_ground();
            }
        } else if !self.is_on_ground_by(self.v) {
            self.go(Move::Fall, "Fall");
        } else if !self.judge_sand_sink() && !self.try_force_slope_slide() {
            self.sand_end_ground();
        }
    }
    fn sand_end_ground(&mut self) {
        if self.is_just_land() {
            self.events.push(Fx::Land { hard: false });
        }
        if self.is_move() {
            self.go(Move::Walk, "Move");
            self.v =
                pose_math::along_vector_normal_h(self.v, Vec3::Y, self.collided_ground_normal())
                    - self.collided_ground_normal() * GRAVITY_MOVE;
        } else {
            self.go(Move::Idle, "Wait");
        }
    }
    fn sand_jump(&mut self, o: &Orders) {
        if self.frame == 0 {
            self.start_clip("Jump");
            let up = self.collided_ground_normal();
            let h = verticalize_vec(Vec3::Y, verticalize_vec(up, self.v));
            let power = air_move::jump_power(
                length(h),
                t::NORMAL_MIN,
                t::NORMAL_MAX,
                t::JUMP_POWER_MIN,
                t::JUMP_POWER_MAX,
            );
            self.v = h + Vec3::Y * self.v.y;
            self.sand.air.setup(
                &mut self.v,
                self.facing,
                &air_move::Takeoff {
                    speed_limit: t::JUMP_MOVE_MAX,
                    base_speed: t::NORMAL_MAX,
                    extend_frames: t::EXTEND_FRAMES,
                    power,
                    gravity: t::JUMP_GRAVITY,
                    input_lock: 0,
                    speed_window: None,
                    fall_max: t::FALL_MAX,
                },
            );
            self.sand.turn.reset();
        }
        if self.touched.ceiling.is_some() {
            self.v.y = self.v.y.min(0.);
            self.sand.air.cancel_extend();
        }
        let input = self.plane_stick(Vec3::Y);
        self.sand.air.update(
            &mut self.v,
            input,
            o.jump_held,
            &air_move::Steering {
                front: t::JUMP_ACCEL_FRONT,
                back: t::JUMP_ACCEL_BACK,
                turn: t::JUMP_ACCEL_TURN,
            },
        );
        let (actor_up, actor_front) = self.pose_axes();
        self.sand.turn.update(ground::TurnFrame {
            actor_up,
            actor_front,
            stick: if self.sand.air.steered() {
                input
            } else {
                Vec3::ZERO
            },
            up: Vec3::Y,
        });
        self.set_pose(self.actor_pose().slerp_up_front(
            Vec3::Y,
            self.sand.turn.direction,
            t::SLERP_QUAT_RATE,
            0.,
        ));
        if self.is_on_ground_by(self.v) {
            if self.is_move() {
                self.sand_ground_appear();
                self.sand_choose(Move::SandRun);
            } else {
                self.sand_choose(Move::SandWait);
            }
        }
    }
    /// syncSensorAndCollision: limit with the old depth, update depth, shift arrows.
    pub(super) fn update_sand_affect(&mut self, scene: &Course) {
        self.sand.called = false;
        if self.is_dead() {
            return;
        }
        if self.sand.depth > 0. {
            let h =
                (1. - joint_math::ease_out((self.sand.depth / 100.).clamp(0., 1.))).clamp(0.1, 0.6);
            let v = (1. - ground::ease_in((self.sand.depth / 60.).clamp(0., 1.))).clamp(0.1, 1.);
            let normal = self.collided_ground_normal();
            let parallel = normal * pose_math::dot(normal, self.v);
            let mut tangent = self.v - parallel;
            let mut parallel = parallel;
            let max_v = t::JUMP_POWER_MAX
                * if pose_math::dot(normal, self.v) < 0. {
                    1.
                } else {
                    v
                };
            let n = length(parallel);
            if n > max_v && n > 0. {
                parallel *= max_v / n;
            }
            let max_h = h * t::NORMAL_MAX;
            let n = length(tangent);
            if n > max_h && n > 0. {
                tangent *= max_h / n;
            }
            self.v = parallel + tangent;
        }
        let mut surface = false;
        if self.is_collided_ground() && self.sand_floor() {
            surface = true;
            self.sand.safe = false;
            let pos = self.position_cm();
            let ground = match (&scene.mesh, self.toy_sand_surface(scene)) {
                (Some(mesh), _) => mesh.relative_vec(self.collider.ground_position()),
                (None, Some(surface)) => Vec3::new(pos.x, surface / UNIT, pos.z),
                (None, None) => self.collider.ground_position(),
            };
            let normal = self.collided_ground_normal();
            let height = pose_math::dot(ground - pos, normal) / normal.y;
            let intersection = pos + Vec3::Y * height;
            let correction = pose_math::dot(intersection - ground, Vec3::NEG_Y);
            let measured = (correction + pose_math::dot(pos - intersection, Vec3::NEG_Y))
                .max(0.)
                .min(self.sand.depth);
            let sink = if self.in_sand_state() && self.mode != Move::SandJump || self.v.y <= 0. {
                HEIGHT / DEAD_TIME
            } else {
                -self.v.y
            };
            self.sand.depth = (measured + sink).max(0.);
            self.sand.surface_offset = -correction;
        } else if !self.sand.safe && self.sand.depth > 0. {
            // The surface the downward probe finds: a stage's `SandSink` triangle, or the toy
            // course's sand.
            let probed = match &scene.mesh {
                Some(mesh) => crate::collider::first_strike(
                    mesh,
                    self.position_cm() + Vec3::Y * self.sand.depth,
                    Vec3::NEG_Y * self.sand.depth,
                    crate::collider::FaceFilter::Any,
                )
                .filter(|hit| mesh.codes_of(hit.triangle).floor == "SandSink")
                .map(|hit| hit.position.y),
                None => self.toy_sand_surface(scene).map(|y| y / UNIT).filter(|y| {
                    *y >= self.position_cm().y && *y <= self.position_cm().y + self.sand.depth
                }),
            };
            if let Some(surface_y) = probed {
                surface = true;
                self.sand.depth = (surface_y - self.position_cm().y - self.sand.surface_offset)
                    .max(0.)
                    .min(self.sand.depth);
            } else {
                self.sand.safe = self.sand.depth > 0.;
                self.sand.depth = 0.;
            }
        } else {
            self.sand.safe = self.sand.depth > 0.;
            self.sand.depth = 0.;
        }
        if self.sand.depth.abs() <= 0.001 {
            self.sand.safe = true;
        }
        self.sand.effect = if surface && self.sand.depth + self.sand.surface_offset < HEIGHT {
            Some(if self.is_move() {
                "SandWalk"
            } else {
                "SandWait"
            })
        } else {
            None
        };
        self.sand.called = self.sand.effect.is_some();
    }
    /// Lockstep: the depth the row shows (zero when no sand is under the body), and for the
    /// sand states the ground control, the pivot and the jump's air control as the row implies.
    pub(super) fn resume_sand(&mut self, s: &Snapshot) {
        self.sand.depth = s.sand.map_or(0., |hint| hint.depth);
        self.sand.surface_offset = 0.;
        self.sand.safe = self.sand.depth <= 0.;
        if !self.in_sand_state() {
            return;
        }
        let hint = s.sand.unwrap_or_default();
        self.sand.normal = self.collided_ground_normal();
        let steps = s.state_frames.unwrap_or(0);
        self.sand.ground.resume(
            self.speed_h(),
            steps,
            hint.turn_rate,
            hint.last_input,
            hint.turn_power,
        );
        match self.mode {
            Move::SandPivot => {
                self.sand.pivot.reset(self.pose_axes().1);
                self.sand.pivot.steps = s.sub_steps.unwrap_or(0).min(10);
                self.sand.pivot.target = hint.last_input.normalize_or(self.pose_axes().1);
            }
            Move::SandJump if !self.grounded => {
                let takeoff = air_move::Takeoff {
                    speed_limit: t::JUMP_MOVE_MAX,
                    base_speed: t::NORMAL_MAX,
                    extend_frames: t::EXTEND_FRAMES,
                    power: self.v.y,
                    gravity: t::JUMP_GRAVITY,
                    input_lock: 0,
                    speed_window: None,
                    fall_max: t::FALL_MAX,
                };
                // The jump nerve's own steps, not the whole actor nerve's (which counted the wait).
                let jump_steps = s.sub_steps.unwrap_or(0);
                let held = self.jump_held_frames > jump_steps;
                self.sand.air.resume(
                    self.v,
                    self.facing,
                    &takeoff,
                    jump_steps,
                    held && self.v.y > 0.,
                );
            }
            _ => {}
        }
    }
    pub fn sand_effect(&self) -> (Option<&'static str>, bool) {
        (self.sand.effect, self.sand.called)
    }
    pub fn sand_blend(&self) -> Option<f32> {
        matches!(self.anim.name, "SandWait" | "SandWalk").then_some(self.sand.weight)
    }
    pub(super) fn sand_cap_enabled(&self) -> bool {
        self.sand.depth <= 0. || self.sand.depth < 30.
    }
}
