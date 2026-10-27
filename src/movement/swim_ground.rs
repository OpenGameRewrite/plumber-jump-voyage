//! The swim state's dealings with the ground below the water: standing and walking on the
//! bottom (`SwimLand`, `SwimWalk`), the paddle (`SwimPaddle`), and the library routines they
//! share (`rs::waitGround`, `rs::reboundVelocityFromCollision`, `rs::calcSnapGroundNormalOrGravityDir`).
//!
//! Written from the 1.0.0 executable and checked against `swim-lake-underwater` and
//! `swim-lake-hipdrop`.
use super::ground::rate01;
use super::swim::{SwimEnd, SwimNerve};
use super::swim_const as k;
use super::swim_move::Stroke;
use super::vector_lib as lib;
use super::*;
use crate::collider::{FaceFilter, arrow_strikes_filtered};

const UP: Vec3 = Vec3::Y;
const GRAVITY: Vec3 = Vec3::NEG_Y;

/// `al::lerpValue`.
fn lerp_value(a: f32, b: f32, t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    a * (1. - t) + t * b
}

impl Player {
    // ---- Library routines ------------------------------------------------------------------

    /// `rs::calcGroundNormalOrGravityDir` / `rs::calcGroundNormalOrUpDir`: the normal of the
    /// ground touched, or the up axis.
    pub(super) fn ground_normal_or_up(&self) -> Vec3 {
        if self.is_collided_ground() {
            self.collided_ground_normal()
        } else {
            UP
        }
    }

    /// The position the game holds for the actor (`al::getTrans`), in the stage's coordinates.
    fn trans_in_stage(&self, scene: &Course) -> Vec3 {
        scene.in_water_frame(self.position_cm())
    }

    /// `al::setTrans(actor, trans + delta)` with the single-precision addition done at the
    /// stage's coordinates, as the game does.
    pub(super) fn move_trans(&mut self, scene: &Course, delta: Vec3) {
        let relative = self.position_cm();
        // The collider stood where the actor was; it follows the hand-made move first.
        self.trans_carry.get_or_insert(relative);
        let moved = match &scene.mesh {
            Some(mesh) => {
                let at = mesh.to_game_vec(relative);
                mesh.relative_vec(Vec3::new(at.x + delta.x, at.y + delta.y, at.z + delta.z))
            }
            None => relative + delta,
        };
        self.pos = moved * UNIT;
        self.collider_end = Some((self.pos, moved));
    }

    /// `al::keepWaterSurfaceHeight`: the root is moved down until the surface is at least
    /// `height` above it.
    pub(super) fn keep_water_surface_height(&mut self, scene: &Course, height: f32) {
        let finder = self.water_finder;
        if !finder.found {
            return;
        }
        let down = (height - finder.distance).max(0.);
        let delta = Vec3::new(down * GRAVITY.x, down * GRAVITY.y, down * GRAVITY.z);
        self.move_trans(scene, delta);
    }

    /// `rs::reboundVelocityFromCollision(actor, collision, a, b, c)` against one surface
    /// normal: a velocity into the surface faster than `b` loses the part into it, is scaled
    /// by `c` and rebounds with `a`; a slower one only loses the part into the surface.
    fn rebound_against(&mut self, normal: Vec3, a: f32, b: f32, c: f32) -> bool {
        let into = pose_math::dot(normal, self.v);
        if into < -b {
            let mut v = Vec3::new(
                self.v.x - into * normal.x,
                self.v.y - into * normal.y,
                self.v.z - into * normal.z,
            );
            v = Vec3::new(v.x * c, v.y * c, v.z * c);
            let back = Vec3::new(
                into * normal.x * a,
                into * normal.y * a,
                into * normal.z * a,
            );
            self.v = Vec3::new(v.x - back.x, v.y - back.y, v.z - back.z);
            true
        } else {
            if into < 0. {
                self.v = Vec3::new(
                    self.v.x - into * normal.x,
                    self.v.y - into * normal.y,
                    self.v.z - into * normal.z,
                );
            }
            false
        }
    }

    /// `rs::reboundVelocityFromCollision`: against the sum of the contacts' normals (or, when
    /// they cancel out, each one), then the wall's and the ceiling's face.
    pub(super) fn rebound_velocity_from_collision(&mut self, a: f32, b: f32, c: f32) -> bool {
        let ground = self.collider.floor.is_some();
        let wall = self.collider.wall.as_ref().map(|w| w.normal);
        let ceiling = self.collider.ceiling.as_ref().map(|w| w.normal);
        if !ground && wall.is_none() && ceiling.is_none() {
            return false;
        }
        let ground_normal = self.collided_ground_normal();
        let mut sum = Vec3::ZERO;
        if ground {
            sum = Vec3::new(
                sum.x + ground_normal.x,
                sum.y + ground_normal.y,
                sum.z + ground_normal.z,
            );
        }
        if let Some(n) = wall {
            sum = Vec3::new(sum.x + n.x, sum.y + n.y, sum.z + n.z);
        }
        if let Some(n) = ceiling {
            sum = Vec3::new(sum.x + n.x, sum.y + n.y, sum.z + n.z);
        }
        if !lib::is_near_zero(sum, 0.001) {
            return self.rebound_against(pose_math::normalize(sum), a, b, c);
        }
        let mut hit = false;
        if ground {
            hit = self.rebound_against(ground_normal, a, b, c);
        }
        if let Some(n) = wall {
            hit |= self.rebound_against(n, a, b, c);
        }
        if let Some(n) = ceiling {
            hit |= self.rebound_against(n, a, b, c);
        }
        hit
    }

    /// `rs::waitGround`: the actor stands on the ground it touches. Its velocity into the
    /// contacts is cancelled, the pull `gravity` along the normal is added up to `limit`, and
    /// the pose turns toward the ground (by `rate` of the way, at most `degrees` from upright).
    pub(super) fn swim_wait_ground(&mut self, gravity: f32, limit: f32, rate: f32, degrees: f32) {
        self.follow_ground_part();
        let normal = self.ground_normal_or_up();
        self.rebound_velocity_from_collision(0., 0., 0.);
        if self.collider.floor.is_some()
            && let Some(ceiling) = self.collider.ceiling.as_ref()
            && pose_math::dot(normal, ceiling.normal) < 0.
            && pose_math::is_parallel_direction(normal, ceiling.normal, 0.01)
        {
            // Squeezed between ground and ceiling: the velocity along them goes too.
            self.v = lib::verticalize_vec(normal, self.v);
        }
        let pull = Vec3::new(
            -(normal.x * gravity),
            -(normal.y * gravity),
            -(normal.z * gravity),
        );
        lib::add_vector_limit(&mut self.v, pull, limit);
        let pose = self.actor_pose().wait_ground(normal, rate, degrees);
        self.set_pose(pose);
    }

    /// `rs::calcSnapGroundNormalOrGravityDir`: the ground touched, or a floor within
    /// `distance` below the root that is at most 45 degrees from the normal the swimmer had.
    /// Returns the normal, the point to snap to (in the stage's coordinates) and whether
    /// there is one.
    fn snap_ground_normal(
        &self,
        scene: &Course,
        distance: f32,
        previous: Vec3,
    ) -> (Vec3, Vec3, bool) {
        let here = self.trans_in_stage(scene);
        if self.is_collided_ground() {
            return (self.collided_ground_normal(), here, true);
        }
        if let Some(mesh) = &scene.mesh {
            let reach = Vec3::new(
                GRAVITY.x * distance,
                GRAVITY.y * distance,
                GRAVITY.z * distance,
            );
            let strikes =
                arrow_strikes_filtered(mesh, self.position_cm(), reach, FaceFilter::FloorsOnly);
            let nearest = strikes
                .iter()
                .min_by(|a, b| a.distance.total_cmp(&b.distance));
            if let Some(hit) = nearest
                && pose_math::dot(previous, hit.normal) >= 0.707_106_77
            {
                return (hit.normal, mesh.to_game_vec(hit.position), true);
            }
        }
        (UP, here, false)
    }

    /// The velocity of a swimmer that starts walking: its part along the ground goes, the pull
    /// of `SwimGravityWalk` along the normal stays.
    pub(super) fn swim_walk_velocity(&mut self) {
        let normal = self.ground_normal_or_up();
        let along = lib::verticalize_vec(normal, self.v);
        let pull = k::GRAVITY_WALK;
        self.v = Vec3::new(
            along.x - pull * normal.x,
            along.y - pull * normal.y,
            along.z - pull * normal.z,
        );
    }

    /// `PlayerAnimControlSwimWalk::update`: the clip's rate follows the speed.
    fn swim_walk_anim_rate(&mut self, speed: f32) {
        let t = rate01(speed, k::WALK_ANIM_SPEED_MIN, k::WALK_ANIM_SPEED_MAX);
        self.set_swim_anim_rate(lerp_value(
            k::WALK_ANIM_FRAME_RATE_MIN,
            k::WALK_ANIM_FRAME_RATE_MAX,
            t,
        ));
    }

    /// `PlayerAnimator::setAnimRate`. A clip that is already running takes the change over
    /// once more at its next update (see `anim_rate_change`); one started in this step does not.
    fn set_swim_anim_rate(&mut self, rate: f32) {
        if !self.anim_skip {
            self.anim_rate_change += rate - self.anim.rate;
        }
        self.anim.rate = rate;
    }

    // ---- SwimWalk --------------------------------------------------------------------------

    /// `PlayerStateSwim::exeSwimWalk`: walking on the bottom.
    pub(super) fn exe_swim_walk(
        &mut self,
        first: bool,
        o: &Orders,
        scene: &Course,
    ) -> Option<SwimEnd> {
        if first {
            self.swim_anim("SwimWalk");
            self.swim.air_steps = 0;
        }
        let old_normal = self.swim.ground_normal;
        let (vertical, horizontal) = lib::separate_parallel_vertical(old_normal, self.v);
        let (normal, snap_at, snapped) = self.snap_ground_normal(scene, 15., old_normal);
        self.swim.ground_normal = normal;
        let horizontal = along_vector_normal_h(horizontal, old_normal, normal);
        let mut along = pose_math::dot(old_normal, vertical);
        let gravity = if snapped {
            along = along.min(0.);
            // The root is held 7 above the ground the arrow found.
            let here = self.trans_in_stage(scene);
            let delta = Vec3::new(snap_at.x - here.x, snap_at.y - here.y, snap_at.z - here.z);
            let rise = delta.x * normal.x + delta.y * normal.y + delta.z * normal.z + -7.;
            self.move_trans(
                scene,
                Vec3::new(normal.x * rise, normal.y * rise, normal.z * rise),
            );
            k::GRAVITY_WALK
        } else {
            k::GRAVITY
        };
        let vertical = self.swim_vertical(
            Vec3::new(normal.x * along, normal.y * along, normal.z * along),
            normal,
            gravity,
        );
        let high = (k::HIGH_ACCEL_PERMIT_FRAME as u32) > self.swim.since_paddle as u32;
        let horizontal = self.swim_horizontal(
            horizontal,
            Stroke {
                high,
                ..Stroke::default()
            },
        );
        self.v = Vec3::new(
            horizontal.x + vertical.x,
            horizontal.y + vertical.y,
            horizontal.z + vertical.z,
        );
        let speed = lib::length(horizontal);
        self.swim_turn(0.75);
        self.joints.swim.ease_off();

        let finder = self.water_finder;
        if finder.found && finder.distance < k::SURFACE_START_DIST {
            self.set_swim_nerve(SwimNerve::Surface);
            return None;
        }
        if o.jump {
            self.set_swim_nerve(SwimNerve::Paddle);
            return None;
        }
        if self.swim_tramples() {
            return None;
        }
        if self.swim.attack.hit {
            self.set_swim_nerve(SwimNerve::Paddle);
            return None;
        }
        if self.is_collided_ground() {
            self.swim.air_steps = 0;
        } else {
            let airborne = self.swim.air_steps;
            self.swim.air_steps += 1;
            if airborne >= 2 {
                self.set_swim_nerve(SwimNerve::Move);
                return None;
            }
        }
        if self.is_move() {
            if !self.is_swim_anim("SwimWalk") {
                self.swim_anim("SwimWalk");
            }
            self.swim_walk_anim_rate(speed);
        } else {
            let slow = lib::length(lib::verticalize_vec(normal, self.v)) < 2.;
            if slow {
                if !self.is_swim_anim("SwimStandWait") {
                    self.swim_anim("SwimStandWait");
                }
            } else if self.is_swim_anim("SwimWalk") {
                self.swim_walk_anim_rate(speed);
            }
        }
        None
    }

    // ---- SwimLand --------------------------------------------------------------------------

    /// `PlayerStateSwim::exeSwimLand`: the swimmer touched the bottom and stands.
    pub(super) fn exe_swim_land(
        &mut self,
        first: bool,
        o: &Orders,
        _scene: &Course,
    ) -> Option<SwimEnd> {
        if first {
            self.swim_anim("SwimLand");
            self.swim.air_steps = 0;
        }
        self.swim.ground_normal = self.ground_normal_or_up();
        self.swim_wait_ground(k::GRAVITY, k::FALL_SPEED_MAX, 0., 0.);
        self.swim_turn(0.75);
        self.joints.swim.ease_off();
        let finder = self.water_finder;
        if finder.found && finder.distance < k::SURFACE_START_DIST {
            self.set_swim_nerve(SwimNerve::Surface);
            return None;
        }
        if self.swim_tramples() {
            return None;
        }
        if o.jump || self.swim.attack.hit {
            self.set_swim_nerve(SwimNerve::Paddle);
            return None;
        }
        if self.is_collided_ground() {
            self.swim.air_steps = 0;
        } else {
            let airborne = self.swim.air_steps;
            self.swim.air_steps += 1;
            if airborne >= 2 {
                self.set_swim_nerve(SwimNerve::Move);
                return None;
            }
        }
        if self.is_move() || self.anim_done(clip::SWIM_LAND) {
            self.swim_walk_velocity();
            self.set_swim_nerve(SwimNerve::Walk);
        }
        None
    }

    // ---- SwimPaddle ------------------------------------------------------------------------

    /// The paddle clip's rate: faster the sooner the last paddle was (`SwimPaddleAnimRateInterval*`).
    pub(super) fn paddle_rate(since_paddle: f32) -> f32 {
        let t = rate01(
            since_paddle,
            k::PADDLE_ANIM_RATE_INTERVAL_MIN,
            k::PADDLE_ANIM_RATE_INTERVAL_MAX,
        );
        let weight = 1. - t;
        let weight = weight.clamp(0., 1.);
        lerp_value(1., k::PADDLE_ANIM_MAX_RATE, weight)
    }

    /// `PlayerStateSwim::exeSwimPaddle`: the stroke that pushes the swimmer up.
    pub(super) fn exe_swim_paddle(
        &mut self,
        first: bool,
        o: &Orders,
        _scene: &Course,
    ) -> Option<SwimEnd> {
        if first {
            let rate = Self::paddle_rate(self.swim.since_paddle as f32);
            self.swim.since_paddle = 0;
            self.swim.paddle_restart = false;
            self.swim.paddle_rate = rate;
            self.swim_anim("SwimStand");
            self.set_swim_anim_rate(rate);
        }
        self.swim_underwater_motion();
        self.swim_turn(1.);
        self.update_swim_pose();
        let finder = self.water_finder;
        if finder.found && finder.distance < k::SURFACE_START_DIST {
            self.set_swim_nerve(SwimNerve::Surface);
            return None;
        }
        if self.swim_tramples() {
            return None;
        }
        if o.jump {
            // Another paddle: the rise starts over and the clip is asked to restart.
            let rate = Self::paddle_rate((self.swim.since_paddle + 1) as f32);
            self.swim.since_paddle = -1;
            self.swim.paddle_rate = rate;
            self.swim.paddle_restart = true;
            self.set_swim_anim_rate(rate);
            return None;
        }
        if o.crouch_pressed {
            self.set_swim_nerve(SwimNerve::HipDropStart);
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
        if self.is_swim_anim("SwimStand") {
            if self.anim.frame < k::PADDLE_ANIM_INTERVAL as f32 {
                return None;
            }
            if self.swim.paddle_restart {
                self.start_clip("SwimStand");
                self.set_swim_anim_rate(self.swim.paddle_rate);
                self.swim.paddle_restart = false;
                return None;
            }
        }
        self.set_swim_nerve(SwimNerve::Move);
        None
    }
}
