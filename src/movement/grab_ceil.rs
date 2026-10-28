//! Ceiling-bar spring and fixed PoleGrabCeil source (SMO 1.0.0).
use super::*;

/// A fixed, unconnected bar. Positions are absolute stage centimetres.
#[derive(Clone, Debug)]
pub struct GrabCeilBar {
    pub id: String,
    pub position: Vec3,
    pub side: Vec3,
    pub centre: Vec3,
    pub phase: u8,
    pub step: u32,
    pub active: bool,
}
impl GrabCeilBar {
    /// `PoleGrabCeil::followPlayer`, followed by its nerve update.
    pub fn tick(&mut self, player: Vec3, attached: bool) {
        if !self.active {
            return;
        }
        self.centre = self.position
            + self.side * pose_math::dot(player - self.position, self.side).clamp(0., 380.);
        let next = match self.phase {
            0 if attached => Some(1),
            1 if !attached => Some(2),
            2 if self.step > 30 => Some(0),
            _ => None,
        };
        if let Some(next) = next {
            self.phase = next;
            self.step = 0;
        } else {
            self.step += 1;
        }
    }

    /// `attackSensor`: body projection and the original scalar/direction arithmetic.
    pub fn force(&self, body: Vec3) -> Option<Vec3> {
        if !self.active
            || self.phase != 0
            || !(0. ..=380.).contains(&pose_math::dot(body - self.position, self.side))
        {
            return None;
        }
        let difference = self.centre - body;
        let distance = vector_lib::length(difference);
        let direction = pose_math::try_normalize_or_zero(difference).0;
        Some(direction * (distance * 0.25).max(distance.min(40.)))
    }
}

/// The two angular springs and input history passed to the shared helper at 0x46cfd4.
#[derive(Clone, Debug, Default)]
pub(super) struct Springs {
    pub angle: f32,
    pub lateral: f32,
    pub speed: f32,
    pub lateral_speed: f32,
    phase_counter: i32,
    remembered_input: f32,
    reversal_counter: i32,
}

fn energy(angle: f32, speed: f32) -> f32 {
    ((angle * 0.01) * angle + speed * speed).sqrt()
}

fn lerp(a: f32, b: f32, rate: f32) -> f32 {
    rate * b + (1. - rate) * a
}

impl Springs {
    /// Angles advance before the pose and input are evaluated. Keep this separate so
    /// the input is projected against the newly transformed snap front.
    fn advance(&mut self) {
        self.angle += self.speed;
        self.lateral += self.lateral_speed;
    }

    /// Shared spring update, after advance/pose. The return is the original drive flag.
    fn drive(&mut self, mut input: f32, swing: bool, initial: bool, wait: bool) -> bool {
        let equivalent = energy(self.angle, self.speed);
        let outward = snap::near_zero_or_greater(self.angle * self.speed, 0.001);
        self.phase_counter = if outward {
            45
        } else {
            (self.phase_counter - 1).max(0)
        };
        if initial && input != 0. && input * self.speed < 0. {
            self.speed = -self.speed;
        }
        let old_input = self.remembered_input;
        if input.abs() >= 0.001 && self.remembered_input.abs() < 0.001 {
            self.remembered_input = input;
        } else if input.abs() >= 0.001 && input * self.remembered_input < 0. {
            self.remembered_input = input;
            self.reversal_counter = 45;
        } else {
            self.reversal_counter = (self.reversal_counter - 1).max(0);
        }
        if self.reversal_counter >= 1 {
            let magnitude = input.abs();
            let remembered = self.remembered_input.abs();
            if magnitude > remembered {
                self.remembered_input = self.remembered_input.signum() * magnitude;
            } else if magnitude < 0.001 {
                input = self.remembered_input;
            } else {
                input = input.signum() * remembered;
            }
        } else if self.reversal_counter == 0 && old_input.abs() >= 0.001 {
            self.remembered_input = input;
        }
        let opposed = input * self.speed < 0.;
        let braking = opposed && self.reversal_counter == 0 && (equivalent >= 6. || swing);
        // The braking branch selects this value, then passes through the common
        // damping interpolation; retain that executable ordering.
        let endpoint = if braking && (outward || self.phase_counter == 0) {
            0.06
        } else {
            0.005
        };
        let damping = if wait {
            0.
        } else {
            lerp(0.06, endpoint, input.abs().clamp(0., 1.))
        };
        let force = -(self.angle * 0.01 + self.speed * damping);
        let mut drive = 0.;
        if !braking && self.angle.abs() < 90. {
            let rate = if swing {
                1.
            } else {
                let t = (equivalent / 6.).clamp(0., 1.);
                // al::easeIn.
                turn_control::ease_in(t)
            };
            drive = input
                * if initial {
                    lerp(3.5, 1.2, rate)
                } else {
                    lerp(0.5, 0.08, rate)
                };
        }
        if opposed {
            drive = -drive;
        }
        self.speed += force + drive;
        self.lateral_speed += -(self.lateral * 0.01 + self.lateral_speed * 0.05);
        drive.abs() > 0. && (!opposed || self.reversal_counter != 0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum GrabNerve {
    #[default]
    Start,
    Wait,
    Swing,
    Jump,
    Fall,
}
impl GrabNerve {
    fn name(self) -> &'static str {
        match self {
            Self::Start => "GrabCeil/Start",
            Self::Wait => "GrabCeil/Wait",
            Self::Swing => "GrabCeil/Swing",
            Self::Jump => "GrabCeil/Jump",
            Self::Fall => "GrabCeil/Fall",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct GrabCeilControl {
    nerve: GrabNerve,
    steps: u32,
    chosen: u32,
    spring: Springs,
    snap: snap::Snap,
    side: Vec3,
    original_front: Vec3,
    initial: bool,
    wait: bool,
    wait_counter: i32,
    buffered_jump: bool,
    gravity: f32,
    invalid: u32,
    pub bar: Option<String>,
    pub root_offset: Vec3,
    pub snap_force: Option<Vec3>,
}
impl GrabCeilControl {
    pub(super) fn is_form(&self) -> bool {
        !matches!(self.nerve, GrabNerve::Jump | GrabNerve::Fall) || self.steps == 0
    }
    pub(super) fn front(&self) -> Vec3 {
        self.original_front
    }
}

fn rotate(v: Vec3, axis: Vec3, degrees: f32) -> Vec3 {
    let half = (degrees * pose_math::DEGREES_TO_RADIANS) * 0.5;
    let sine = console_libm::sinf(half);
    pose_math::rotate_by_quat(
        v,
        Quat::from_xyzw(
            axis.x * sine,
            axis.y * sine,
            axis.z * sine,
            console_libm::cosf(half),
        ),
    )
}

impl Player {
    pub(super) fn receive_grab_ceil_force(&mut self, scene: &Course) {
        self.grab_ceil.snap_force = None;
        if self.grounded || self.is_dead() {
            return;
        }
        if self.mode == Move::GrabCeil
            && (self.grab_ceil.is_form()
                || self.grab_ceil.steps
                    <= if self.grab_ceil.nerve == GrabNerve::Jump {
                        10
                    } else {
                        30
                    })
        {
            return;
        }
        let Some(mesh) = &scene.mesh else { return };
        let feet = mesh.to_game_vec(self.position_cm());
        for bar in &mesh.grab_ceil_bars {
            let overlap =
                [(40., 40.), (75., 50.), (110., 40.)]
                    .into_iter()
                    .any(|(height, radius)| {
                        let d = feet + Vec3::Y * height - bar.centre;
                        pose_math::dot(d, d) < (radius + 150.) * (radius + 150.)
                    });
            if overlap && let Some(force) = bar.force(feet + Vec3::Y * 75.) {
                self.grab_ceil.snap_force = Some(force);
            }
        }
    }
    pub fn attached_grab_ceil_bar(&self) -> Option<&str> {
        (self.mode == Move::GrabCeil)
            .then_some(self.grab_ceil.bar.as_deref())
            .flatten()
    }
    pub(super) fn grab_ceil_name(&self) -> &'static str {
        self.grab_ceil.nerve.name()
    }

    pub(super) fn judge_grab_ceil(&mut self, scene: &Course) {
        if self.nerve.deferred
            || self.is_dead()
            || self.recovering()
            || self.mode == Move::Swim
            || self.mode == Move::Ledge
            || self.mode == Move::Pole
            || self.mode == Move::GrabCeil
                && (self.grab_ceil.is_form() || self.grab_ceil.steps <= 10)
        {
            return;
        }
        let Some(mesh) = scene.catch_mesh() else {
            return;
        };
        let grab = |codes: &Option<crate::geometry::CollisionCodes>| {
            codes.as_ref().is_some_and(|c| c.floor == "GrabCeil")
        };
        let fit = if let Some(wall) = self
            .collider
            .wall
            .as_ref()
            .filter(|_| grab(&self.touched.wall))
        {
            let velocity = pose_math::try_normalize_or_zero(self.v.with_y(0.)).0;
            if pose_math::dot(velocity, wall.normal) > 0. {
                return;
            }
            grab_ceil_find::wall(&mesh, -wall.normal, wall.position)
        } else if let Some(ceiling) = self
            .collider
            .ceiling
            .as_ref()
            .filter(|_| grab(&self.touched.ceiling))
        {
            grab_ceil_find::no_wall(&mesh, ceiling.normal, ceiling.position, self.facing)
        } else if let Some(ground) = self
            .collider
            .floor
            .as_ref()
            .filter(|_| grab(&self.touched.floor))
        {
            if pose_math::dot(self.v, ground.normal) > 0. {
                return;
            }
            grab_ceil_find::no_wall(&mesh, ground.normal, ground.position, self.facing)
        } else {
            None
        };
        let Some(mut fit) = fit else { return };
        let moving = pose_math::dot(self.v, fit.front);
        let orientation = if moving.abs() > 3. {
            moving
        } else {
            pose_math::dot(self.facing, fit.front)
        };
        if orientation < 0. {
            fit.front = -fit.front;
        }
        let mut control = GrabCeilControl {
            original_front: fit.front,
            side: pose_math::normalize(fit.front.cross(Vec3::NEG_Y)),
            ..default()
        };
        control.snap.setup(fit.pos, fit.front, fit.axis);
        let incoming =
            swim_move::limit_vector_opposite_dir(self.v, Vec3::NEG_Y, vector_lib::length(self.v))
                * 0.75;
        let length = vector_lib::length(incoming);
        let incoming = if length > 20. {
            incoming * (20. / length)
        } else {
            incoming
        };
        let projection = -pose_math::dot(incoming, fit.front);
        let direction = if projection == 0. {
            0.
        } else {
            projection.signum()
        };
        let equivalent = direction
            * projection
                .abs()
                .max(pose_math::dot(incoming, Vec3::NEG_Y).max(0.));
        let lateral = pose_math::dot(incoming, control.side);
        let delta = self.position_cm() - fit.pos;
        if pose_math::try_normalize_or_zero(delta).1 {
            let sign = if direction.abs() < 0.001 {
                -1.
            } else {
                direction
            };
            control.spring.angle = -sign
                * ground_pose::calc_angle_on_plane_degree(Vec3::NEG_Y, delta, control.side)
                    .abs()
                    .min(90.);
            let sign = if lateral.abs() < 0.001 {
                -1.
            } else {
                lateral.signum()
            };
            control.spring.lateral = -sign
                * ground_pose::calc_angle_on_plane_degree(Vec3::NEG_Y, delta, fit.front)
                    .abs()
                    .min(20.);
        }
        let angle = control.spring.angle;
        control.spring.speed = if direction.abs() < 0.001 {
            -7.5
        } else {
            direction
                * ((equivalent * equivalent - (angle * angle) * 0.01)
                    .max(0.)
                    .sqrt()
                    + (7.5 - energy(angle, equivalent)).max(0.))
        };
        let angle = control.spring.lateral;
        control.spring.lateral_speed = if lateral.abs() < 0.001 {
            -0.
        } else {
            lateral.signum()
                * ((lateral * lateral - (angle * angle) * 0.01).max(0.).sqrt()
                    + (0. - energy(angle, lateral)).max(0.))
        };
        control.bar = mesh.triangles[fit.triangle as usize]
            .local
            .and_then(|part| {
                mesh.grab_ceil_bars
                    .iter()
                    .find(|b| {
                        let at = mesh.to_game_vec(fit.pos);
                        let d = at - b.position;
                        pose_math::dot(d, b.side) >= 0.
                            && pose_math::dot(d, b.side) <= 440.
                            && (d - b.side * pose_math::dot(d, b.side)).length() <= 20.
                            && (part.part as usize) < mesh.parts.len()
                    })
                    .map(|b| b.id.clone())
            });
        control.snap.previous = fit.pos;
        control.snap.held = fit.pos;
        self.grab_ceil = control;
        self.pos = fit.pos * UNIT;
        self.collider_end = Some((self.pos, fit.pos));
        self.v = Vec3::ZERO;
        self.grounded = false;
        self.air_for = None;
        self.jump_extend = false;
        self.wall_history = WallHistory::default();
        self.collider.invalidate(self.facing.x.atan2(self.facing.z));
        self.change(Move::GrabCeil, "GrabCeilStart", |_| {});
        self.nerve.keep_clip();
        self.start_clip("GrabCeilStart");
        self.sub_anim = None;
        self.upper_catch = None;
        self.grab_ceil_follow();
    }

    fn grab_ceil_follow(&mut self) {
        let c = &mut self.grab_ceil;
        let front = pose_math::normalize(rotate(c.original_front, c.side, c.spring.angle));
        let up = pose_math::normalize(rotate(
            rotate(
                c.original_front.cross(c.side),
                c.original_front,
                c.spring.lateral,
            ),
            c.side,
            c.spring.angle,
        ));
        let point = c.snap.pos;
        self.pos = point * UNIT;
        self.collider_end = Some((self.pos, point));
        self.set_pose(snap::Snap::orientation(front, up));
    }

    fn set_grab_ceil_nerve(&mut self, nerve: GrabNerve, clip: &'static str) {
        self.grab_ceil.nerve = nerve;
        self.grab_ceil.steps = 0;
        self.grab_ceil.chosen += 1;
        self.sub_restarted = true;
        self.defer(Move::GrabCeil, clip, |_| {});
    }

    pub(super) fn grab_ceil_step(&mut self, o: &Orders) {
        let nerve = self.grab_ceil.nerve;
        let chosen = self.grab_ceil.chosen;
        let first = self.grab_ceil.steps == 0;
        if matches!(nerve, GrabNerve::Jump | GrabNerve::Fall) {
            if first {
                let current_vertical = self.v.y;
                self.air.setup(
                    &mut self.v,
                    self.facing,
                    &air_move::Takeoff {
                        speed_limit: 30.,
                        base_speed: 11.,
                        extend_frames: 0,
                        power: current_vertical,
                        gravity: self.grab_ceil.gravity,
                        input_lock: self.grab_ceil.invalid,
                        speed_window: None,
                        fall_max: t::FALL_MAX,
                    },
                );
            }
            let push = self.push();
            self.air.update(
                &mut self.v,
                push,
                o.jump_held,
                &air_move::Steering {
                    front: t::JUMP_ACCEL_FRONT,
                    back: t::JUMP_ACCEL_BACK,
                    turn: t::JUMP_ACCEL_TURN,
                },
            );
            self.facing = self.air.turn_facing(self.facing, push);
            self.pose_tilt = None;
            if self.grounded {
                self.grab_ceil.bar = None;
                self.hold_to_ground();
            } else {
                self.air_actions(o);
            }
        } else {
            if first && nerve == GrabNerve::Start {
                self.grab_ceil.initial = true;
                self.grab_ceil.wait =
                    energy(self.grab_ceil.spring.angle, self.grab_ceil.spring.speed) < 6.5;
            }
            if self.grab_ceil.initial {
                self.grab_ceil.initial =
                    self.grab_ceil.spring.angle * self.grab_ceil.spring.speed <= 0.;
            }
            let jump = std::mem::take(&mut self.pre_jump) > 0;
            if nerve == GrabNerve::Start && self.grab_ceil.steps < 10 {
                self.grab_ceil.buffered_jump |= jump;
            } else if jump || std::mem::take(&mut self.grab_ceil.buffered_jump) {
                self.leave_grab_ceil(nerve != GrabNerve::Wait);
                return;
            }
            let old_angle = self.grab_ceil.spring.angle;
            self.grab_ceil.spring.advance();
            self.grab_ceil_follow();
            let dot = pose_math::dot(self.stick, self.grab_ceil.original_front);
            let input = if dot.abs() < 0.173_648_18 {
                0.
            } else {
                -dot.signum()
                    * if nerve != GrabNerve::Swing && self.stick.length_squared() > 0. {
                        1.
                    } else {
                        self.stick.length()
                    }
            };
            let c = &mut self.grab_ceil;
            let drive = c
                .spring
                .drive(input, nerve == GrabNerve::Swing, c.initial, c.wait);
            let e = energy(c.spring.angle, c.spring.speed);
            if c.wait {
                if drive || c.wait_counter > 0 {
                    c.wait_counter = (c.wait_counter + 1).min(3);
                    if c.wait_counter >= 3 {
                        c.wait = false;
                    }
                }
            } else if self.stick.length_squared() == 0. && e < 6.5 {
                let c = &mut self.grab_ceil;
                c.wait = c.spring.reversal_counter < 1;
                if c.spring.reversal_counter <= 0 {
                    c.wait_counter = 0;
                }
            }
            if nerve == GrabNerve::Swing {
                let c = &self.grab_ceil;
                let amplitude = (c.spring.angle * c.spring.angle
                    + (c.spring.speed * c.spring.speed) / 0.01)
                    .sqrt();
                let rate = joint_math::calc_rate01(c.spring.angle, -amplitude, amplitude);
                self.anim.frame = lerp(
                    0.5,
                    if c.spring.angle - old_angle > 0. {
                        1.
                    } else {
                        0.
                    },
                    rate,
                ) * 120.;
            }
            match nerve {
                GrabNerve::Start
                    if self.stick.length_squared() > 0. && self.grab_ceil.steps >= 10
                        || self.stick.length_squared() == 0. && self.anim_done(51) =>
                {
                    if self.stick.length_squared() > 0. && e >= 6.5 {
                        self.set_grab_ceil_nerve(GrabNerve::Swing, "GrabCeilSwing");
                    } else {
                        self.set_grab_ceil_nerve(GrabNerve::Wait, "GrabCeilWait");
                    }
                }
                GrabNerve::Wait if drive && e - 1. >= 6.5 => {
                    self.set_grab_ceil_nerve(GrabNerve::Swing, "GrabCeilSwing")
                }
                GrabNerve::Swing if e < 6.5 => {
                    self.set_grab_ceil_nerve(GrabNerve::Wait, "GrabCeilWait")
                }
                _ => {}
            }
        }
        if chosen == self.grab_ceil.chosen {
            self.grab_ceil.steps += 1;
        }
    }

    fn leave_grab_ceil(&mut self, jump_enabled: bool) {
        self.grab_ceil_follow();
        let c = &self.grab_ceil;
        let jump = jump_enabled && energy(c.spring.angle, c.spring.speed) >= 6.5;
        let direction = if c.initial {
            1.
        } else if c.spring.angle.abs() > 10. {
            c.spring.angle.signum()
        } else {
            -c.spring.speed.signum()
        };
        let front = c.original_front;
        self.start_clip(if jump { "GrabCeilJump" } else { "Fall" });
        self.interpolation_cleared = true;
        let root = self.grab_ceil.root_offset;
        self.pos += root * UNIT;
        self.collider_end = None;
        self.v = Vec3::Y * if jump { 20. } else { 3. };
        self.v += front * if jump { direction * 15. } else { -1. };
        self.grab_ceil.gravity = if jump { 1. } else { 1.2 };
        self.grab_ceil.invalid = if jump { 10 } else { 0 };
        self.grab_ceil.bar = None;
        self.collider.invalidate(self.facing.x.atan2(self.facing.z));
        self.set_grab_ceil_nerve(
            if jump {
                GrabNerve::Jump
            } else {
                GrabNerve::Fall
            },
            self.anim.name,
        );
        self.nerve.keep_clip();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_bar_follow_force_and_strict_grab_end() {
        let mut bar = GrabCeilBar {
            id: "fixture".into(),
            position: Vec3::ZERO,
            side: Vec3::X,
            centre: Vec3::ZERO,
            phase: 0,
            step: 0,
            active: true,
        };
        bar.tick(Vec3::new(200., -200., 0.), false);
        assert_eq!(bar.centre, Vec3::X * 200.);
        assert_eq!(bar.force(Vec3::new(200., -200., 0.)), Some(Vec3::Y * 50.));
        assert!(bar.force(Vec3::new(-1., -200., 0.)).is_none());
        bar.tick(Vec3::ZERO, true);
        assert_eq!((bar.phase, bar.step), (1, 0));
        assert!(bar.force(Vec3::ZERO).is_none());
        bar.tick(Vec3::ZERO, false);
        for _ in 0..31 {
            bar.tick(Vec3::ZERO, false);
        }
        assert_eq!((bar.phase, bar.step), (2, 31));
        bar.tick(Vec3::ZERO, false);
        assert_eq!((bar.phase, bar.step), (0, 0));
    }

    #[test]
    fn spring_pose_advances_before_force_and_lateral_has_no_drive() {
        let mut spring = Springs {
            angle: 20.,
            speed: -2.,
            lateral: 10.,
            lateral_speed: 1.,
            ..default()
        };
        spring.advance();
        assert_eq!((spring.angle, spring.lateral), (18., 11.));
        assert!(!spring.drive(0., false, false, false));
        assert_eq!(spring.speed, -2. + -(18. * 0.01 + -2. * 0.06));
        assert_eq!(spring.lateral_speed, 1. + -(11. * 0.01 + 0.05));
    }
}
