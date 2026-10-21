//! `PlayerActionSlopeSlideControl`: the movement control the forced slope slide
//! (`PlayerStateSlope`) and the roll (`PlayerStateRolling`) both run, plus the floor judges
//! that read the collider the way `rs::isOnGround*` do.
//!
//! The control keeps the ground it last saw, the way down that ground and the horizontal
//! velocity it left. `update` turns the velocity onto the floor under the feet, speeds it along
//! the way down (or brakes it on flat ground), lets the stick push it sideways, lays the pull
//! into the floor on top and steers the pose toward the travel. In the air gravity pulls and
//! the velocity of the slope turns onto the horizontal plane.
//!
//! Written from the decompilation (`PlayerActionSlopeSlideControl.cpp`), whose operations
//! the 1.0.0 executable runs in the same order; the library routines it calls are in
//! `vector_lib.rs` and `pose_math.rs`.
use super::vector_lib as lib;
use super::*;

/// What the control remembers between steps (`mGroundNormal`, `mDirSlide`,
/// `mHorizontalVelocity`).
#[derive(Clone, Copy, Debug)]
pub(super) struct SlideControl {
    /// The ground normal of the step before (up while there was no ground).
    pub ground_normal: Vec3,
    /// The unit vector down the ground (zero on level ground).
    pub dir_slide: Vec3,
    /// The velocity without its part along the up axis, as the last update left it.
    pub horizontal_velocity: Vec3,
}
impl Default for SlideControl {
    fn default() -> Self {
        Self {
            ground_normal: Vec3::Y,
            dir_slide: Vec3::ZERO,
            horizontal_velocity: Vec3::ZERO,
        }
    }
}

/// The arguments of `PlayerActionSlopeSlideControl::update`.
#[derive(Clone, Copy, Debug)]
pub(super) struct SlideParams {
    pub accel: f32,
    pub brake: f32,
    pub against: f32,
    pub angle_power_max: f32,
    pub max_speed: f32,
    pub side_accel: f32,
    pub side_brake: f32,
    pub side_max_speed: f32,
    pub turn_degree: f32,
    pub gravity: f32,
    /// The roll lets the stick push the velocity on flat ground as well.
    pub rolling: bool,
}

/// Gravity points straight down in this game.
const GRAVITY: Vec3 = Vec3::NEG_Y;
/// The up axis (`-al::getGravity`).
const UP: Vec3 = Vec3::Y;

impl SlideControl {
    /// The speed the last update left along the floor (`getHorizontalVelocity().length()`).
    pub fn horizontal_speed(&self) -> f32 {
        lib::length(self.horizontal_velocity)
    }
}

impl Player {
    /// `rs::isCollidedGround`: the collider has found floor under the feet, whatever the
    /// velocity does.
    pub(super) fn is_collided_ground(&self) -> bool {
        self.grounded || self.hop_contact
    }

    /// `rs::getCollidedGroundNormal`: the normal the collider averaged for the floor it touches
    /// (up where there is none).
    pub(super) fn collided_ground_normal(&self) -> Vec3 {
        self.floor_normal().unwrap_or(UP)
    }

    /// `rs::isJustLand`: the first step the collider touches the ground in after steps without.
    pub(super) fn is_just_land(&self) -> bool {
        self.is_collided_ground() && self.ground_contact == 1
    }

    /// `rs::isOnGround`: touching the ground and not moving away from it (a velocity heading
    /// along the ground's normal ends it).
    pub(super) fn is_on_ground_by(&self, velocity: Vec3) -> bool {
        if !self.is_collided_ground() {
            return false;
        }
        let heading = pose_math::try_normalize_or_zero(velocity).0;
        let away = pose_math::dot(heading, self.collided_ground_normal());
        !(away > 0. && !lib::is_near_zero_value(away, 0.001))
    }

    /// The ground is flatter than the angle (`!` of the polygon test that the three
    /// `rs::isOnGroundSlopeSlide*` and `isOnGroundForceSlideCode` make).
    fn ground_within(&self, degrees: f32) -> bool {
        lib::is_floor_polygon_cos(
            self.collided_ground_normal(),
            GRAVITY,
            lib::cos_of_degrees(degrees),
        )
    }

    /// `rs::isOnGroundSlopeSlideEnd`: on ground flatter than `SlopeSlideAngleEnd`.
    pub(super) fn on_ground_slope_slide_end(&self, velocity: Vec3) -> bool {
        self.is_on_ground_by(velocity) && self.ground_within(t::SLOPE_SLIDE_ANGLE_END)
    }

    /// `rs::isOnGroundSlopeSlideStart`: on ground steeper than `SlopeSlideAngleStart`.
    pub(super) fn on_ground_slope_slide_start(&self, velocity: Vec3) -> bool {
        self.is_on_ground_by(velocity) && !self.ground_within(t::SLOPE_SLIDE_ANGLE_START)
    }

    /// `rs::isOnGroundForceSlideCode`: on ground steeper than `SlopeSlideAngleEnd` whose
    /// floor code is `Slide`.
    pub(super) fn on_ground_force_slide_code(&self, velocity: Vec3) -> bool {
        self.is_on_ground_by(velocity)
            && self
                .touched
                .floor
                .as_ref()
                .is_some_and(|f| f.floor == "Slide")
            && !self.ground_within(t::SLOPE_SLIDE_ANGLE_END)
    }

    /// `PlayerActionSlopeSlideControl::setup`.
    pub(super) fn slide_control_setup(&self) -> SlideControl {
        let mut control = SlideControl::default();
        if self.is_collided_ground() {
            let normal = self.collided_ground_normal();
            control.dir_slide = lib::calc_dir_slide(GRAVITY, normal);
            if !self.is_just_land() {
                control.ground_normal = normal;
            }
        }
        control
    }

    /// `PlayerActionSlopeSlideControl::update`: the new velocity (the caller stores it). The
    /// pose is turned toward the travel here, as the control does.
    pub(super) fn slide_control_update(
        &mut self,
        control: &mut SlideControl,
        velocity: Vec3,
        p: &SlideParams,
    ) -> Vec3 {
        let previous_normal = control.ground_normal;
        let mut up = UP;
        let mut velocity = velocity;
        let slide_end = self.on_ground_slope_slide_end(velocity);
        let new_velocity = if self.is_on_ground_by(velocity) {
            control.ground_normal = self.collided_ground_normal();
            up = control.ground_normal;
            control.dir_slide = lib::calc_dir_slide(GRAVITY, up);
            velocity = along_vector_normal_h(velocity, previous_normal, up);
            let (vel_dir_slide, vel_side) = if slide_end {
                self.slide_on_flat_ground(velocity, up, p)
            } else {
                self.slide_on_slope(control, velocity, up, p)
            };
            let sum = Vec3::new(
                vel_dir_slide.x + vel_side.x,
                vel_dir_slide.y + vel_side.y,
                vel_dir_slide.z + vel_side.z,
            );
            let sum = lib::limit_length(sum, p.max_speed);
            let pull = GRAVITY_MOVE;
            Vec3::new(
                sum.x - pull * up.x,
                sum.y - pull * up.y,
                sum.z - pull * up.z,
            )
        } else {
            // Leaving the ground: the velocity of the slope's plane turns onto the horizontal
            // plane and the part along the old normal goes on along the new one.
            control.ground_normal = up;
            let previous_up_speed = pose_math::dot(previous_normal, velocity);
            let mut turned =
                along_vector_normal_h(velocity, previous_normal, control.ground_normal);
            turned = Vec3::new(
                previous_up_speed * control.ground_normal.x + turned.x,
                previous_up_speed * control.ground_normal.y + turned.y,
                previous_up_speed * control.ground_normal.z + turned.z,
            );
            let fall = Vec3::new(up.x * -p.gravity, up.y * -p.gravity, up.z * -p.gravity);
            lib::add_vector_limit(&mut turned, fall, t::FALL_MAX);
            turned
        };
        let horizontal = lib::verticalize_vec(up, new_velocity);
        let (heading, has_heading) = pose_math::try_normalize_or_zero(horizontal);
        if has_heading {
            self.turn_pose_toward(up, heading, p.turn_degree);
        }
        control.horizontal_velocity = horizontal;
        new_velocity
    }

    /// The body turns toward the way it travels by at most `turn_degree` and its pose follows
    /// the floor (`al::turnVecToVecCosOnPlane`, then `rs::slerpUpFront`).
    fn turn_pose_toward(&mut self, up: Vec3, heading: Vec3, turn_degree: f32) {
        let pose = self.actor_pose();
        let flat_front = lib::verticalize_vec(up, pose.front());
        let mut front = pose_math::try_normalize_or_zero(flat_front).0;
        if let Some((turned, _)) =
            lib::turn_vec_to_vec_cos_on_plane(front, heading, up, lib::cos_of_degrees(turn_degree))
        {
            front = turned;
        }
        let stored = pose.slerp_up_front(up, front, t::SLERP_QUAT_RATE, t::HILL_POSE_DEGREE_MAX);
        self.set_pose(stored);
    }

    /// On ground flatter than the end angle (no slide left): the velocity in the floor's
    /// plane loses `brake`, the part along the normal loses `side_brake`, and the roll's
    /// stick pushes the plane velocity sideways. Returns (in-plane, along-normal).
    fn slide_on_flat_ground(&self, velocity: Vec3, up: Vec3, p: &SlideParams) -> (Vec3, Vec3) {
        let (along_up, in_plane) = lib::separate_parallel_vertical(up, velocity);
        let mut in_plane = scale(in_plane, p.brake);
        let along_up = scale(along_up, p.side_brake);
        if p.rolling {
            let move_input = self.plane_stick(up);
            let direction = pose_math::try_normalize_or_zero(in_plane).0;
            let side = pose_math::try_normalize_or_zero(pose_math::cross(up, direction)).0;
            let side_input = pose_math::dot(side, move_input);
            let push = scale(scale(side, side_input), p.side_accel);
            lib::add_vector_limit(&mut in_plane, push, p.max_speed);
        }
        (in_plane, along_up)
    }

    /// On a slope: the way down gains `accel` (scaled by how steep the slope is and by how
    /// the stick leans against it), the sideways part loses `side_brake` and the stick pushes
    /// it. Returns (down the slope, sideways).
    fn slide_on_slope(
        &self,
        control: &SlideControl,
        velocity: Vec3,
        up: Vec3,
        p: &SlideParams,
    ) -> (Vec3, Vec3) {
        let dir_slide = control.dir_slide;
        let (mut vel_dir_slide, mut vel_side) =
            lib::separate_parallel_vertical(dir_slide, velocity);
        let move_input = self.plane_stick(up);
        let speed_dir_slide = pose_math::dot(vel_dir_slide, dir_slide);
        if speed_dir_slide < 0. {
            vel_dir_slide = scale(vel_dir_slide, p.brake);
        }
        let input_against = (-pose_math::dot(dir_slide, move_input)).clamp(0., 1.);
        let min_speed_norm = (1. - input_against * p.against).clamp(0., 1.);
        let speed_norm = (speed_dir_slide + 1.).clamp(min_speed_norm, 1.);
        let angle_power = (90. - lib::calc_angle_degree(dir_slide, GRAVITY)).clamp(0., 90.);
        let angle_power_norm = if p.angle_power_max > 0. {
            (angle_power / p.angle_power_max).min(1.)
        } else {
            1.
        };
        let push = scale(
            scale(scale(dir_slide, p.accel), angle_power_norm),
            speed_norm,
        );
        lib::add_vector_limit(&mut vel_dir_slide, push, p.max_speed);
        vel_side = scale(vel_side, p.side_brake);
        let (input_dir, has_input) = pose_math::try_normalize_or_zero(move_input);
        if has_input {
            let side_dir = pose_math::cross(up, dir_slide);
            let side_push = scale(
                scale(
                    scale(side_dir, pose_math::dot(side_dir, input_dir)),
                    lib::length(move_input),
                ),
                p.side_accel,
            );
            lib::add_vector_limit(&mut vel_side, side_push, p.side_max_speed);
        }
        (vel_dir_slide, vel_side)
    }
}

/// `sead::Vector3f * f32`.
fn scale(v: Vec3, factor: f32) -> Vec3 {
    Vec3::new(v.x * factor, v.y * factor, v.z * factor)
}
