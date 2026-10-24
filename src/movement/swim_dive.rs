//! The water hip drop of the swim state: the wind-up (`SwimHipDropStart`), the dive down
//! (`SwimHipDropLoop`, `PlayerStateSwim::updateNerveDownFall`), the landing on the bottom
//! (`SwimHipDropLand`) and the jump back up (`SwimHipDropJump`), with `rs::moveDiveSwim`.
//!
//! Written from the 1.0.0 executable and checked against `swim-lake-underwater` and
//! `swim-lake-hipdrop`.
use super::swim::{SwimEnd, SwimNerve};
use super::swim_const as k;
use super::vector_lib as lib;
use super::*;

const GRAVITY: Vec3 = Vec3::NEG_Y;

impl Player {
    /// `rs::moveDiveSwim(actor, horizontal, vertical, step)`: the horizontal velocity is
    /// scaled by `horizontal`, the vertical one by `vertical` and then its speed along the
    /// gravity moves toward zero by at most `step`.
    pub(super) fn move_dive_swim(&mut self, horizontal: f32, vertical: f32, step: f32) {
        let (down, across) = lib::separate_parallel_vertical(GRAVITY, self.v);
        let across = Vec3::new(
            across.x * horizontal,
            across.y * horizontal,
            across.z * horizontal,
        );
        let down = Vec3::new(down.x * vertical, down.y * vertical, down.z * vertical);
        let speed = down.x * GRAVITY.x + down.y * GRAVITY.y + down.z * GRAVITY.z;
        let speed = converge_f32(speed, 0., step);
        self.v = Vec3::new(
            speed * GRAVITY.x + across.x,
            speed * GRAVITY.y + across.y,
            speed * GRAVITY.z + across.z,
        );
    }

    /// `PlayerStateSwim::exeSwimHipDropStart`: the wind-up of the dive, motionless at the
    /// depth the swimmer had; the dive starts when the clip ends.
    pub(super) fn exe_swim_hip_drop_start(
        &mut self,
        first: bool,
        _o: &Orders,
        scene: &Course,
    ) -> Option<SwimEnd> {
        if first {
            self.swim_anim("SwimHipDropStart");
            self.v = Vec3::ZERO;
            self.joints.swim.reset();
            self.swim.keep_depth = if self.water_finder.found {
                self.water_finder.distance
            } else {
                k::TALL
            };
        }
        self.keep_water_surface_height(scene, self.swim.keep_depth);
        self.swim.ground_normal = self.ground_normal_or_up();
        // (The head slide out of the wind-up is not ported.)
        if self.anim_done(clip::SWIM_HIP_DROP_START) {
            // `al::addVelocityToGravity`: the dive's speed along the gravity.
            self.v = Vec3::new(
                k::DIVE_START_SPEED * GRAVITY.x + self.v.x,
                k::DIVE_START_SPEED * GRAVITY.y + self.v.y,
                k::DIVE_START_SPEED * GRAVITY.z + self.v.z,
            );
            self.set_swim_nerve(SwimNerve::HipDropLoop);
        }
        None
    }

    /// `PlayerStateSwim::exeSwimHipDropLoop`.
    pub(super) fn exe_swim_hip_drop_loop(
        &mut self,
        first: bool,
        o: &Orders,
        _scene: &Course,
    ) -> Option<SwimEnd> {
        if first {
            self.swim_anim("SwimHipDrop");
        }
        self.swim_down_fall(first, o);
        None
    }

    /// `PlayerStateSwim::updateNerveDownFall`: the dive down. The speed holds while the
    /// button is held (up to its valid frames) and brakes after; the dive ends on the bottom
    /// or when it has all but stopped.
    fn swim_down_fall(&mut self, first: bool, o: &Orders) {
        if first {
            self.swim.dive_hold = k::DIVE_BUTTON_VALID_FRAME;
            self.swim.land_steps = 0;
            self.swim.slow_steps = 0;
        }
        self.swim.ground_normal = self.ground_normal_or_up();
        if self.swim.steps as i32 >= k::DIVE_NO_BRAKE_FRAME {
            self.swim.dive_hold = if o.crouch {
                (self.swim.dive_hold - 1).max(0)
            } else {
                0
            };
        }
        let step = if self.swim.dive_hold != 0 {
            0.
        } else {
            k::DIVE_BRAKE
        };
        self.move_dive_swim(1., 1., step);
        // (The attack triggers of a trample and the head slide are not ported.)
        if self.try_change_hip_drop_land(o) {
            return;
        }
        // The speed down (the speed along the gravity) has been slow for a while.
        if k::DIVE_END_SPEED > pose_math::dot(self.v, GRAVITY) {
            self.swim.slow_steps += 1;
            if self.swim.slow_steps as u32 >= k::DIVE_END_FRAME as u32 {
                let finder = self.water_finder;
                if finder.found && finder.distance < k::SURFACE_START_DIST {
                    self.set_swim_nerve(SwimNerve::Surface);
                } else {
                    self.set_swim_nerve(SwimNerve::Move);
                }
            }
        }
    }

    /// `PlayerStateSwim::tryChangeHipDropLand`: on the ground (and not still diving into it
    /// with the button held) the dive lands.
    fn try_change_hip_drop_land(&mut self, o: &Orders) -> bool {
        if !self.is_on_ground_by(self.v) {
            return false;
        }
        // (`ECollisionTrigger 1` is not ported; with the button held it would refuse.)
        let _ = o;
        self.swim.land_steps += 1;
        self.swim.slow_steps += 1;
        self.swim_wait_ground(k::GRAVITY, k::GRAVITY, 1., 0.);
        if self.swim.land_steps > k::DIVE_LAND_COUNT {
            self.set_swim_nerve(SwimNerve::HipDropLand);
        }
        true
    }

    /// `PlayerStateSwim::exeSwimHipDropLand`: standing on the bottom after the dive; a jump
    /// button within the permitted steps jumps up.
    pub(super) fn exe_swim_hip_drop_land(
        &mut self,
        first: bool,
        o: &Orders,
        _scene: &Course,
    ) -> Option<SwimEnd> {
        if first {
            self.swim_anim("SwimHipDropLand");
            self.v = Vec3::ZERO;
            // The hit reaction of the landing (`rs::startHitReactionHipDropLand`).
            self.events.push(Fx::SwimHipDropLand);
        }
        self.swim.ground_normal = self.ground_normal_or_up();
        self.swim_wait_ground(k::GRAVITY, k::FALL_SPEED_MAX, 0., 0.);
        let finder = self.water_finder;
        if self.swim.steps as i32 <= k::JUMP_HIP_DROP_PERMIT_END_FRAME && o.jump {
            if finder.found && finder.distance <= k::SURFACE_ENABLE_JUMP_HEIGHT {
                self.swim.surface_hold = 0;
                return Some(SwimEnd::JumpHipDrop);
            } else {
                self.set_swim_nerve(SwimNerve::HipDropJump);
            }
            return None;
        }
        if self.swim.steps as i32 >= k::DIVE_LAND_CANCEL_FRAME {
            if finder.found && finder.distance < k::SURFACE_START_DIST {
                self.set_swim_nerve(SwimNerve::Surface);
                return None;
            }
            if o.jump {
                self.set_swim_nerve(SwimNerve::Paddle);
                return None;
            }
            if self.is_move() {
                self.set_swim_nerve(SwimNerve::Move);
                return None;
            }
        }
        if !self.is_collided_ground() {
            self.set_swim_nerve(SwimNerve::Move);
            return None;
        }
        if self.anim_done(clip::SWIM_HIP_DROP_LAND) {
            self.swim_walk_velocity();
            self.set_swim_nerve(SwimNerve::Walk);
        }
        None
    }

    /// `PlayerStateSwim::exeSwimHipDropJump`: the jump up from the bottom (`SwimJumpHipDrop`).
    pub(super) fn exe_swim_hip_drop_jump(
        &mut self,
        first: bool,
        o: &Orders,
        _scene: &Course,
    ) -> Option<SwimEnd> {
        if first {
            self.swim_anim("SwimJumpHipDrop");
            self.v = Vec3::new(
                GRAVITY.x * -k::JUMP_HIP_DROP_SPEED,
                GRAVITY.y * -k::JUMP_HIP_DROP_SPEED,
                GRAVITY.z * -k::JUMP_HIP_DROP_SPEED,
            );
        }
        self.swim.ground_normal = self.ground_normal_or_up();
        let brake = if self.collider.ceiling.is_some() {
            k::JUMP_HIP_DROP_BRAKE_V_CEILING
        } else {
            k::JUMP_HIP_DROP_BRAKE_V
        };
        self.move_dive_swim(1., brake, k::JUMP_HIP_DROP_GRAVITY);
        // The stick pushes the horizontal speed a little.
        let stick = self.swim_stick();
        self.v = scale_horizontal(self.v, k::BRAKE_RATE_H, 1.);
        let push = Vec3::new(
            k::JUMP_HIP_DROP_ACCEL_H * stick.x,
            k::JUMP_HIP_DROP_ACCEL_H * stick.y,
            k::JUMP_HIP_DROP_ACCEL_H * stick.z,
        );
        self.v = Vec3::new(self.v.x + push.x, self.v.y + push.y, self.v.z + push.z);
        self.v = limit_velocity_h(self.v, k::JUMP_HIP_DROP_MOVE_SPEED_H);
        self.swim_turn(1.);

        let finder = self.water_finder;
        let speed_up = -pose_math::dot(self.v, GRAVITY);
        if finder.found && finder.distance < k::SURFACE_START_DIST {
            if speed_up >= k::JUMP_HIP_DROP_POP_SPEED {
                return Some(SwimEnd::JumpPop);
            }
            self.set_swim_nerve(SwimNerve::Surface);
            return None;
        }
        if self.is_on_ground_by(self.v) {
            if self.is_move() {
                self.swim_walk_velocity();
                self.set_swim_nerve(SwimNerve::Walk);
            } else {
                self.swim_wait_ground(k::GRAVITY, k::FALL_SPEED_MAX, 0., 0.);
                self.set_swim_nerve(SwimNerve::Land);
            }
            return None;
        }
        if speed_up < k::JUMP_HIP_DROP_CANCEL_SPEED {
            if o.jump {
                self.set_swim_nerve(SwimNerve::Paddle);
                return None;
            }
            if o.crouch_pressed {
                self.set_swim_nerve(SwimNerve::HipDropStart);
                return None;
            }
        }
        if speed_up <= 0.001 {
            self.set_swim_nerve(SwimNerve::Move);
        }
        None
    }
}

impl Player {
    /// `PlayerStateSwim::exeSwimDive`: the dive of a hip drop that fell into the water; the
    /// speed it came with is held to `SwimDiveStartSpeed`.
    pub(super) fn exe_swim_dive(
        &mut self,
        first: bool,
        o: &Orders,
        _scene: &Course,
    ) -> Option<SwimEnd> {
        if first {
            self.swim_anim("SwimDive");
            self.v = limit_velocity_dir(self.v, GRAVITY, k::DIVE_START_SPEED);
            self.joints.swim.reset();
        }
        self.swim_down_fall(first, o);
        None
    }

    /// `PlayerStateSwim::exeSwimFallIn`: the first steps in the water after a fast fall. The
    /// fall speed is held to `SwimFallInSpeed`, then braked until it is slow enough for the
    /// swim's own rules to take over.
    pub(super) fn exe_swim_fall_in(
        &mut self,
        first: bool,
        o: &Orders,
        _scene: &Course,
    ) -> Option<SwimEnd> {
        if first {
            self.swim_anim("SwimFallIn");
            self.v = limit_velocity_dir(self.v, GRAVITY, k::FALL_IN_SPEED);
        }
        self.swim.ground_normal = self.ground_normal_or_up();
        let falling = -(-pose_math::dot(self.v, GRAVITY));
        let slow = if k::FALL_SPEED_MAX > falling {
            self.move_dive_swim(k::FALL_IN_BRAKE_H, 1., 0.);
            lib::add_vector_limit(
                &mut self.v,
                Vec3::new(
                    GRAVITY.x * k::GRAVITY,
                    GRAVITY.y * k::GRAVITY,
                    GRAVITY.z * k::GRAVITY,
                ),
                k::FALL_SPEED_MAX,
            );
            true
        } else {
            self.move_dive_swim(k::FALL_IN_BRAKE_H, k::FALL_IN_BRAKE_V, 0.);
            k::FALL_SPEED_MAX > pose_math::dot(self.v, GRAVITY)
        };
        self.swim_turn(1.);
        if !slow {
            return None;
        }
        let finder = self.water_finder;
        if self.is_move() && finder.found {
            self.swim.surface_hold = k::FALL_IN_FORCE_SURFACE_FRAME;
            self.set_swim_nerve(SwimNerve::Surface);
            return None;
        }
        if self.is_on_ground_by(self.v) {
            if self.is_move() {
                self.swim_walk_velocity();
                self.set_swim_nerve(SwimNerve::Walk);
            } else {
                self.swim_wait_ground(k::GRAVITY, k::FALL_SPEED_MAX, 0., 0.);
                self.set_swim_nerve(SwimNerve::Land);
            }
            return None;
        }
        if finder.found && finder.distance < k::SURFACE_START_DIST {
            if self.start_swim_jump_judged(o) {
                self.swim.surface_hold = 0;
                return Some(SwimEnd::Jump);
            }
            return None;
        }
        if o.jump {
            self.set_swim_nerve(SwimNerve::Paddle);
            return None;
        }
        // The fall's last step raises the hit reaction of falling in (its bubbles).
        self.events.push(Fx::WaterDive);
        self.set_swim_nerve(SwimNerve::Move);
        None
    }
}

/// `al::limitVelocityDir`: the part of the velocity along `dir` is held to `limit`.
fn limit_velocity_dir(velocity: Vec3, dir: Vec3, limit: f32) -> Vec3 {
    let (mut along, across) = lib::separate_parallel_vertical(dir, velocity);
    if pose_math::squared_length(along) > limit * limit {
        let length = lib::length(along);
        if length > 0. {
            let scale = limit / length;
            along = Vec3::new(along.x * scale, along.y * scale, along.z * scale);
        }
    }
    Vec3::new(along.x + across.x, along.y + across.y, along.z + across.z)
}

/// `al::converge(f32 value, target, step)`.
fn converge_f32(value: f32, target: f32, step: f32) -> f32 {
    if value < target {
        (value + step).min(target)
    } else {
        (value - step).max(target)
    }
}

/// `al::scaleVelocityHV`: the part of the velocity along the gravity is scaled by `vertical`,
/// the rest by `horizontal`.
fn scale_horizontal(velocity: Vec3, horizontal: f32, vertical: f32) -> Vec3 {
    let (down, across) = lib::separate_parallel_vertical(GRAVITY, velocity);
    Vec3::new(
        across.x * horizontal + down.x * vertical,
        across.y * horizontal + down.y * vertical,
        across.z * horizontal + down.z * vertical,
    )
}

/// `al::limitVelocityH`: the part of the velocity across the gravity is held to `limit`.
fn limit_velocity_h(velocity: Vec3, limit: f32) -> Vec3 {
    let (down, across) = lib::separate_parallel_vertical(GRAVITY, velocity);
    let mut across = across;
    if pose_math::squared_length(across) > limit * limit {
        let length = lib::length(across);
        if length > 0. {
            let scale = limit / length;
            across = Vec3::new(across.x * scale, across.y * scale, across.z * scale);
        }
    }
    Vec3::new(down.x + across.x, down.y + across.y, down.z + across.z)
}
