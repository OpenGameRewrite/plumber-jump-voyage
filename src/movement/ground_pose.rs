//! The ground pose of the body (`PlayerJointControlGroundPose`): how the skeleton leans into a
//! turn, stands up straight, sways when the player lands or sets off, and plants its feet.
//!
//! The keeper hands the controller a few numbers every frame (`update`): how much of the body's
//! and the legs' inverse kinematics is wanted, the turn tilt, a flag for the stiff landing of a
//! triple jump, and the centre dynamics' impulses (`PlayerJointParamCenterDynamics`). The model's
//! world calculation then gives the controller each joint's matrix as it computes it (`calc`),
//! in the order of the skeleton, and the controller bends it, joint by joint in this order of
//! its own: `calcTilt`, `calcCenterTilt`, `calcCenterBalance`, `calcIK`.
//!
//! * Tilt: the turn tilt `T` leans the root and the spine to the side (about the front axis of
//!   the root, by up to 20 and 15 degrees per unit), the head over, the legs' tops out.
//! * Centre tilt: off. It is driven by `PlayerConst::CenterTiltRateMax`, which is 0 for Mario.
//! * Inverse kinematics, on the root: works out the frame the others use. The root's up axis
//!   against the world's up gives the turn that stands the hip (by at most 25 degrees per unit
//!   of the rate) and the spine (30) upright about the root's position, and a shift to the side
//!   when the root's side axis is not level.
//! * Centre balance, on the joint root: a damped sway of the upper body. A landing or the start
//!   of a run gives it an impulse; each frame it works out where the stood-up joint root would
//!   be pulled to and turns the hip's and the spine's frames by that.
//! * Inverse kinematics on the hip and the spine moves them by the frame; on the feet it keeps
//!   them where the unbent body would have put them while they are low (`calcLegBottomMtx`),
//!   bending the legs between the hip and the foot with a two-link solution.
//!
//! The arithmetic follows the executable's single-precision order and is checked bit for bit
//! against it (`PJV_GROUND_CASES`, `tests/ground_pose.rs`).
use super::joint_math::{
    Rows, calc_rate01, converge, ease_out, lerp_vec, make_quat_rotation_limit, mul_rows,
    rotate_axes_only, rotate_matrix, rotate_own_x, rotate_own_y, rotate_own_z, rotate_vec,
    rotation_rows, transposed,
};
use super::look_joint::Mat34;
use super::pose_math::{
    DEGREES_TO_RADIANS, NEGATED_GRAVITY, NO_DIRECTION, atan2f, cosf, cross, dot, normalize,
    rotate_by_quat, scaled_by_inverse_length, sinf, slerp, squared_length, to_quat,
    try_normalize_or_zero,
};
use super::vector_lib::{calc_angle_degree, is_near_zero_value, limit_length, verticalize_vec};
use crate::skeleton_pose::{Joint, Matrices};
use bevy::prelude::{Quat, Vec3};

/// `al::isNearZero` limit of the controller's gates.
const NEAR_ZERO: f32 = 0.001;
/// The rate at which the turn tilt follows its target, and the most it moves in a frame.
const TILT_RATE: f32 = 0.15;
const TILT_STEP_LIMIT: f32 = 0.1;
/// `calcTilt`: degrees of roll per unit of tilt and the limit of the root's and spine's tilt
/// from the vertical.
const ROOT_TILT_DEGREES: f32 = 20.;
const ROOT_TILT_LIMIT: f32 = 45.;
const SPINE_TILT_DEGREES: f32 = 15.;
const SPINE_TILT_LIMIT: f32 = 50.;
/// `calcHeadMtxTilt`: degrees about the head's own x, y and z per unit of tilt (z by the
/// tilt's magnitude, backwards), and `calcLegTopMtxTilt`'s about y.
const HEAD_TILT_X: f32 = 10.;
const HEAD_TILT_Y: f32 = 25.;
const HEAD_TILT_Z: f32 = -20.;
const LEG_TOP_TILT_Y: f32 = -10.;
/// `calcRootMtx`: the most the hip and the spine are stood up, per unit of the inverse
/// kinematics, and the sideways shift.
const HIP_STAND_UP_DEGREES: f32 = 25.;
const SPINE_STAND_UP_DEGREES: f32 = 30.;
const SHIFT_LEVEL_DEGREES: f32 = -90.;
const SHIFT_FULL_DEGREES: f32 = 45.;
const SHIFT_DISTANCE: f32 = 12.5;
/// The step of the follower of the inverse kinematics' target: rising and falling.
const FOLLOW_STEPS: [f32; 2] = [0.1, 0.2];

/// `updateCenterBalance`: the impulse is the vector halved, limited to this length (10 for the
/// strong kind, 5 otherwise).
const IMPULSE_LIMITS: [f32; 2] = [5., 10.];
/// The blend the impulse starts at, and the steps it falls by: the quick one, or by the kind.
const QUICK_BLEND_STEP: f32 = 0.2;
const BLEND_STEPS: [f32; 2] = [0.033_333_335, 0.05];
/// `calcCenterBalance`: the most the sway turns the stood-up joint root in one frame (25
/// degrees), the shortest reach (60 cm), the shares of the turn the hip (a quarter), the spine
/// (the whole) and the shift take, the damping of the sway and the spring stiffness.
const SWAY_TURN_LIMIT: f32 = 0.436_332_32;
const SWAY_MIN_REACH: f32 = 60.;
const HIP_SWAY_SHARE: f32 = 0.25;
const SWAY_DAMPING: f32 = 0.9;
const SWAY_SPRING: f32 = 0.1;
const SWAY_SPRING_DAMPER: f32 = 0.1;
const SWAY_ANGLE_SPRINGS: [f32; 2] = [0.02, 0.03];
const SWAY_ANGLE_DAMPER: f32 = 0.01;
/// `calcLegBottomMtx`: a foot lower than 15 cm over the root's plane is held where the unbent
/// body would have put it, one higher than 25 is left to the animation; a leg may be stretched
/// by 2 cm to reach, except in the stiff landing.
const FOOT_HELD_BELOW: f32 = 15.;
const FOOT_FREE_ABOVE: f32 = 25.;
const LEG_STRETCH: f32 = 2.;
/// `al::calcAngleOnPlaneDegree`'s radians to degrees.
const RADIANS_TO_DEGREES: f32 = 57.295_776;

/// What the keeper hands the controller each frame (`PlayerJointControlGroundPose::update`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GroundInputs {
    /// The body's inverse kinematics wanted (0 to 1): `keeper+0x80`.
    pub body_ik: f32,
    /// The legs' (`keeper+0x84`).
    pub leg_ik: f32,
    /// The turn tilt the body leans to (`keeper+0x88`).
    pub tilt: f32,
    /// The stiff landing after a triple jump holds the legs' reach (`*(keeper+0x78)`).
    pub stiff_landing: bool,
}

/// The centre dynamics' parameter (`PlayerJointParamCenterDynamics`, the keeper's `+0x68`): what
/// the states hand the sway for one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CenterInput {
    /// A new impulse (byte 0), with its vector.
    pub impulse: bool,
    pub impulse_vector: Vec3,
    /// A follow-up of an impulse (byte 1), with its vector: it adds to the sway while the blend
    /// is still high.
    pub follow_up: bool,
    pub follow_up_vector: Vec3,
    /// The sway fades quickly (byte 2).
    pub fade_quickly: bool,
    /// The stronger kind of impulse (byte 3).
    pub strong: bool,
}

/// A turn and its inverse, for a joint to be stood upright about the root's position.
#[derive(Clone, Copy, Debug)]
struct Turn {
    forward: Rows,
    back: Rows,
}

impl Default for Turn {
    fn default() -> Self {
        let identity = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        Self {
            forward: identity,
            back: identity,
        }
    }
}

/// What the root's callback works out for the joints below it, this frame, and the centre
/// balance adjusts.
#[derive(Clone, Copy, Debug, Default)]
pub struct StandFrame {
    /// The root's position (`+0x54`) and the shift of the stood-up joints (`+0x60`).
    pub pivot: Vec3,
    pub shift: Vec3,
    /// The root's up axis (`+0x6c`).
    pub root_up: Vec3,
    hip: Turn,
    spine: Turn,
}

/// The sway of the centre balance (`+0x1f0..+0x226`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Balance {
    /// Where the stood-up joint root is held, from the pivot (`+0x1f0`).
    offset: Vec3,
    /// The last frame's pull of the stood-up joint root (`+0x1fc`) and position from the pivot
    /// (`+0x208`).
    last_pull: Vec3,
    last_relative: Vec3,
    /// The sway itself (`+0x214`), and how much of it is left (`+0x220`).
    sway: Vec3,
    blend: f32,
    /// An impulse was given this frame (`+0x224`); the fade is the quick one (`+0x225`); the
    /// impulse was the strong kind (`+0x226`).
    started: bool,
    quick_fade: bool,
    strong: bool,
}

#[derive(Clone, Debug, Default)]
pub struct GroundPose {
    /// The turn tilt (`+0x1e4`).
    tilt: f32,
    /// The follower of the body's inverse kinematics target (`+0x1d8`) and the strength the
    /// callbacks use (`+0x1dc`): the follower eased out, less the turn tilt.
    ik_follow: f32,
    ik: f32,
    /// The legs' strength follows its target at the follower's step (`+0x1e0`).
    leg_ik: f32,
    stiff_landing: bool,
    frame: StandFrame,
    balance: Balance,
}

impl GroundPose {
    /// The turn tilt, for the tests and the trace.
    #[allow(dead_code)]
    pub fn tilt(&self) -> f32 {
        self.tilt
    }

    /// `PlayerJointControlGroundPose::update` and `updateCenterBalance`.
    pub fn update(&mut self, input: GroundInputs, center: &CenterInput) {
        // The tilt follows its target by 0.15 of the way, but by no more than 0.1 a frame.
        let step = (input.tilt - self.tilt) * TILT_RATE;
        let magnitude = if step > 0. { step } else { -step };
        self.tilt = if magnitude > TILT_STEP_LIMIT {
            converge(self.tilt, input.tilt, TILT_STEP_LIMIT)
        } else {
            self.tilt + step
        };
        let follow_step = FOLLOW_STEPS[usize::from(self.ik_follow > input.body_ik)];
        self.ik_follow = converge(self.ik_follow, input.body_ik, follow_step);
        self.ik = ease_out(self.ik_follow) * (1. - self.tilt).clamp(0., 1.);
        self.leg_ik = converge(self.leg_ik, input.leg_ik, follow_step);
        self.stiff_landing = input.stiff_landing;
        self.balance.update(center);
    }

    /// The numbers the oracle compares after a frame: the tilt, the two strengths, the blend,
    /// the sway, the hold and the shift.
    #[allow(dead_code)]
    pub fn state(&self) -> Vec<f32> {
        let (b, f) = (&self.balance, &self.frame);
        let mut out = vec![
            self.tilt,
            self.ik,
            self.leg_ik,
            b.blend,
            b.sway.x,
            b.sway.y,
            b.sway.z,
            b.offset.x,
            b.offset.y,
            b.offset.z,
            f.shift.x,
            f.shift.y,
            f.shift.z,
        ];
        for turn in [&f.hip.forward, &f.hip.back, &f.spine.forward] {
            out.extend(turn.iter().flatten());
        }
        out
    }

    /// `calcGroundPoseUp`: the actor's up axis as the stood-up spine carries it (the actor's own
    /// while no stand-up is applied).
    pub fn ground_pose_up(&self, actor_up: Vec3) -> Vec3 {
        if is_near_zero_value(self.ik, NEAR_ZERO) {
            return actor_up;
        }
        normalize(rotate_vec(&self.frame.spine.back, actor_up))
    }

    /// The frame the root worked out (the tests read it).
    #[allow(dead_code)]
    pub fn stand_frame(&self) -> &StandFrame {
        &self.frame
    }

    /// `calcJointCallback`: bend `world`, the matrix of `joint` as the model has just computed
    /// it. `solved` holds the matrices of the joints before it (the legs rewrite theirs).
    pub fn calc(&mut self, joint: Joint, world: &mut Mat34, solved: &mut Matrices) {
        self.calc_tilt(joint, world, solved);
        // (`calcCenterTilt` has nothing to do: its rate is 0 for Mario.)
        self.calc_center_balance(joint, world);
        self.calc_inverse_kinematics(joint, world, solved);
    }

    fn calc_tilt(&self, joint: Joint, world: &mut Mat34, solved: &Matrices) {
        if is_near_zero_value(self.tilt, NEAR_ZERO) {
            return;
        }
        let tilt = self.tilt;
        match joint {
            Joint::AllRoot => {
                let (front, up) = (column(world, 2), column(world, 1));
                tilt_about_front(world, front, up, ROOT_TILT_DEGREES, tilt, ROOT_TILT_LIMIT);
            }
            Joint::Spine1 => {
                // The spine leans about the axes of the root as it stands now.
                let root = &solved[Joint::AllRoot as usize];
                let (front, up) = (column(root, 2), column(root, 1));
                tilt_about_front(world, front, up, SPINE_TILT_DEGREES, tilt, SPINE_TILT_LIMIT);
            }
            Joint::Head => {
                rotate_own_x(world, tilt * HEAD_TILT_X);
                rotate_own_y(world, tilt * HEAD_TILT_Y);
                let magnitude = if tilt > 0. { tilt } else { -tilt };
                rotate_own_z(world, magnitude * HEAD_TILT_Z);
            }
            Joint::LegL1 | Joint::LegR1 => rotate_own_y(world, tilt * LEG_TOP_TILT_Y),
            _ => {}
        }
    }

    fn calc_inverse_kinematics(&mut self, joint: Joint, world: &mut Mat34, solved: &mut Matrices) {
        if is_near_zero_value(self.ik, NEAR_ZERO) {
            return;
        }
        match joint {
            Joint::AllRoot => self.calc_root(world),
            Joint::Hip => self.stand_up(world, self.frame.hip.forward),
            Joint::Spine1 => self.stand_up(world, self.frame.spine.forward),
            Joint::FootL => self.calc_leg_bottom(world, solved, [Joint::LegL1, Joint::LegL2]),
            Joint::FootR => self.calc_leg_bottom(world, solved, [Joint::LegR1, Joint::LegR2]),
            _ => {}
        }
    }

    /// `calcRootMtx`: the frame the hip and the spine are stood up in.
    fn calc_root(&mut self, root: &Mat34) {
        let up = NEGATED_GRAVITY;
        let root_up = normalize(column(root, 1));
        let hip = make_quat_rotation_limit(
            root_up,
            up,
            (self.ik * HIP_STAND_UP_DEGREES) * DEGREES_TO_RADIANS,
        );
        let spine = make_quat_rotation_limit(
            root_up,
            up,
            (self.ik * SPINE_STAND_UP_DEGREES) * DEGREES_TO_RADIANS,
        );
        let turn = |q| {
            let forward = rotation_rows(q);
            Turn {
                forward,
                back: transposed(&forward),
            }
        };
        let side = column(root, 0);
        // How far the root's side axis is from level: 0 when it lies across the up, 1 at 45
        // degrees (or more) either way, signed.
        let from_level = calc_angle_degree(side, up) + SHIFT_LEVEL_DEGREES;
        let magnitude = if from_level > 0. {
            from_level
        } else {
            -from_level
        };
        let share = (magnitude / SHIFT_FULL_DEGREES).clamp(0., 1.);
        let signed = if from_level < 0. { -share } else { share };
        let reach = self.ik * (signed * SHIFT_DISTANCE);
        self.frame = StandFrame {
            pivot: Vec3::new(root[0][3], root[1][3], root[2][3]),
            shift: Vec3::new(side.x * reach, side.y * reach, side.z * reach),
            root_up,
            hip: turn(hip),
            // (The exe stores the spine's "back" matrix as a copy of its forward one, not the
            // transposed: `calcGroundPoseUp` stands the up axis with it.)
            spine: Turn {
                back: rotation_rows(spine),
                ..turn(spine)
            },
        };
    }

    /// `calcHipMtx` / `calcSpineMtx`: turn the joint about the root's position and shift it.
    fn stand_up(&self, world: &mut Mat34, turn: Rows) {
        let StandFrame { pivot, shift, .. } = self.frame;
        let from_pivot = Vec3::new(
            world[0][3] - pivot.x,
            world[1][3] - pivot.y,
            world[2][3] - pivot.z,
        );
        let moved = rotate_vec(&turn, from_pivot);
        *world = rotate_matrix(&turn, world);
        world[0][3] = (moved.x + pivot.x) + shift.x;
        world[1][3] = (moved.y + pivot.y) + shift.y;
        world[2][3] = (moved.z + pivot.z) + shift.z;
    }

    /// `calcCenterBalance`: on the joint root, the sway of the upper body.
    fn calc_center_balance(&mut self, joint: Joint, world: &Mat34) {
        if joint != Joint::JointRoot || is_near_zero_value(self.balance.blend, NEAR_ZERO) {
            return;
        }
        let pivot = self.frame.pivot;
        let here = Vec3::new(world[0][3], world[1][3], world[2][3]);
        let relative = here - pivot;
        let reach_now = squared_length(relative).sqrt();
        // Where the joint root stands once the spine's turn and shift have stood it up.
        let stood = rotate_vec(&self.frame.spine.forward, relative) + self.frame.shift;
        let stood_direction = normalize(stood);
        let stood_at = stood + pivot;
        let pull = stood_at - here;
        let balance = &mut self.balance;
        if balance.started {
            balance.offset = stood_at - pivot;
            balance.last_relative = relative;
            balance.last_pull = pull;
            balance.blend = 1.;
            return;
        }
        // The joint root's own movement since the last frame, and how the sway is used up by
        // it: the part of the sway along each movement is taken off, no more than the movement.
        let moved = relative - balance.last_relative;
        let (moved_length, moved_direction) = separate_scalar_and_direction(moved);
        let mut sway = balance.sway;
        let consumed_by_moving = consume(&mut sway, moved_length, moved_direction);
        let pulled = pull - balance.last_pull;
        let (pulled_length, pulled_direction) = separate_scalar_and_direction(pulled);
        let consumed_by_pull = consume(&mut sway, pulled_length, pulled_direction);
        // Where the stood-up joint root is held now, and the direction it is pulled to.
        let held = pivot + balance.offset;
        let wanted = (held + ((sway + moved) + pulled)) - pivot;
        let wanted_direction = match try_normalize_or_zero(wanted) {
            (direction, true) => direction,
            _ => stood_direction,
        };
        let turn = make_quat_rotation_limit(stood_direction, wanted_direction, SWAY_TURN_LIMIT);
        let to = {
            let rotated = rotate_by_quat(stood_direction, turn);
            try_normalize_or_zero(rotated).0
        };
        let plane_normal = Vec3::new(-stood_direction.x, -stood_direction.y, -stood_direction.z);
        let across = calc_distance_vec_to_plane(to, pivot, plane_normal, stood_at);
        let longest = SWAY_MIN_REACH.max(reach_now);
        let reach_limit = if across > longest { longest } else { across };
        let wanted_length = squared_length(wanted).sqrt();
        let reach = if wanted_length < 0. {
            0.
        } else if wanted_length > reach_limit {
            reach_limit
        } else {
            wanted_length
        };
        let along = Vec3::new(reach * to.x, reach * to.y, reach * to.z);
        let sway_direction = try_normalize_or_zero(verticalize_vec(to, sway)).0;
        let beyond = (wanted_length - reach).max(0.);
        let beyond_vec = Vec3::new(
            beyond * sway_direction.x,
            beyond * sway_direction.y,
            beyond * sway_direction.z,
        );
        let new_held = along + beyond_vec;
        let target = new_held + pivot;
        // What the sway keeps: the target less what the movements and the old hold gave.
        let moved_left = moved - moved_direction * consumed_by_moving;
        let pulled_left = pulled - pulled_direction * consumed_by_pull;
        let accounted = (held + moved_left) + pulled_left;
        let new_sway = target - accounted;
        balance.sway = new_sway;
        balance.offset = new_held;
        balance.last_pull = pull;
        balance.last_relative = relative;
        // The turn takes the hip a quarter of the way, the spine all of it, and the shift a
        // quarter of the way to the target.
        let strength = balance.blend * self.ik;
        let turn_rows = rotation_rows(turn);
        let hip_target = mul_rows(&turn_rows, &self.frame.hip.forward);
        let spine_target = mul_rows(&turn_rows, &self.frame.spine.forward);
        self.frame.hip.back = transposed(&slerp_rows(
            &self.frame.hip.forward,
            &hip_target,
            strength * 0.,
        ));
        self.frame.hip.forward = slerp_rows(
            &self.frame.hip.forward,
            &hip_target,
            strength * HIP_SWAY_SHARE,
        );
        self.frame.spine.forward = slerp_rows(&self.frame.spine.forward, &spine_target, strength);
        let share = strength * HIP_SWAY_SHARE;
        let towards = target - stood_at;
        self.frame.shift = Vec3::new(
            share * towards.x + self.frame.shift.x,
            share * towards.y + self.frame.shift.y,
            share * towards.z + self.frame.shift.z,
        );
        // The sway damps, then the springs push it: along the direction to the stood-up
        // position by the distance gained, and across by the angle it is turned.
        let balance = &mut self.balance;
        balance.sway *= SWAY_DAMPING;
        let sway_now = new_sway;
        let (_, along_sway) = separate_vector_hv(to, sway_now);
        let gained = reach - reach_now;
        let along_speed = dot(to, along_sway);
        let push = spring_damper_force(gained, along_speed, SWAY_SPRING, SWAY_SPRING_DAMPER);
        balance.sway = Vec3::new(
            push * stood_direction.x + balance.sway.x,
            push * stood_direction.y + balance.sway.y,
            push * stood_direction.z + balance.sway.z,
        );
        let (across_sway, _) = separate_vector_hv(to, sway_now);
        let axis = match try_normalize_or_zero(cross(stood_direction, to)) {
            (axis, true) => axis,
            _ => match try_normalize_or_zero(cross(stood_direction, across_sway)) {
                (axis, true) => axis,
                _ => return,
            },
        };
        let side = normalize(cross(axis, stood_direction));
        let angle = calc_angle_on_plane_degree(stood_direction, to, axis);
        let side_speed = dot(side, across_sway);
        let spring = SWAY_ANGLE_SPRINGS[usize::from(balance.strong)];
        let twist = spring_damper_force(angle, side_speed, spring, SWAY_ANGLE_DAMPER);
        balance.sway = Vec3::new(
            twist * side.x + balance.sway.x,
            twist * side.y + balance.sway.y,
            twist * side.z + balance.sway.z,
        );
    }

    /// `calcLegBottomMtx`, on a foot: the foot is held where the unbent body would have put it
    /// while it is low, and the leg is bent between the hip and the foot.
    fn calc_leg_bottom(&self, foot: &mut Mat34, solved: &mut Matrices, legs: [Joint; 2]) {
        let StandFrame {
            pivot,
            shift,
            root_up,
            hip,
            ..
        } = self.frame;
        let thigh = solved[legs[0] as usize];
        let shin = solved[legs[1] as usize];
        let foot_at = Vec3::new(foot[0][3], foot[1][3], foot[2][3]);
        // The foot's place before the hip was turned and shifted.
        let from_pivot =
            foot_at - Vec3::new(pivot.x + shift.x, pivot.y + shift.y, pivot.z + shift.z);
        let held_at = rotate_vec(&hip.back, from_pivot) + pivot;
        let turned_back = rotate_matrix(&hip.back, foot);
        // Only a low foot is held: by how far it stands over the root's plane.
        let height = {
            let d = held_at - pivot;
            (d.x * root_up.x + d.y * root_up.y) + d.z * root_up.z
        };
        let low = 1. - calc_rate01(height.max(0.), FOOT_HELD_BELOW, FOOT_FREE_ABOVE);
        let weight = ease_out(low.clamp(0., 1.)) * self.leg_ik;
        let target = lerp_vec(foot_at, held_at, weight);
        let animated = *foot;
        *foot = blend_mtx(&animated, &turned_back, weight);
        foot[0][3] = target.x;
        foot[1][3] = target.y;
        foot[2][3] = target.z;
        // The two links: thigh and shin lengths from the animation, stretched when the foot is
        // out of reach.
        let thigh_at = Vec3::new(thigh[0][3], thigh[1][3], thigh[2][3]);
        let shin_at = Vec3::new(shin[0][3], shin[1][3], shin[2][3]);
        let thigh_up = normalize(column(&thigh, 1));
        let mut thigh_length = squared_length(shin_at - thigh_at).sqrt();
        let mut shin_length = squared_length(shin_at - foot_at).sqrt();
        let to_foot = squared_length(thigh_at - target).sqrt();
        let reach = thigh_length + shin_length;
        let margin = if self.stiff_landing { 0. } else { LEG_STRETCH };
        let wanted = to_foot + margin;
        if wanted > reach {
            thigh_length = (thigh_length / reach) * wanted;
            shin_length = (shin_length / reach) * wanted;
        }
        let knee = calc_between_two_link_pos(thigh_at, target, thigh_length, shin_length, thigh_up);
        let thigh_dir = normalize(knee - thigh_at);
        solved[legs[0] as usize] = make_mtx_side_up_pos(thigh_dir, thigh_up, thigh_at);
        let shin_dir = normalize(target - knee);
        let shin_up = normalize(column(&shin, 1));
        solved[legs[1] as usize] = make_mtx_side_up_pos(shin_dir, shin_up, knee);
    }
}

impl Balance {
    /// `updateCenterBalance`: the impulses of this frame, then the fade.
    fn update(&mut self, input: &CenterInput) {
        if input.impulse {
            if is_near_vector_zero(input.impulse_vector) {
                return;
            }
            let half = Vec3::new(
                input.impulse_vector.x * 0.5,
                input.impulse_vector.y * 0.5,
                input.impulse_vector.z * 0.5,
            );
            self.sway = limit_length(half, IMPULSE_LIMITS[usize::from(input.strong)]);
            self.blend = 1.;
            self.quick_fade = false;
            self.started = true;
            self.strong = input.strong;
            return;
        }
        if input.follow_up {
            if is_near_zero_value(self.blend, NEAR_ZERO) {
                let half = Vec3::new(
                    input.follow_up_vector.x * 0.5,
                    input.follow_up_vector.y * 0.5,
                    input.follow_up_vector.z * 0.5,
                );
                self.sway = limit_length(half, IMPULSE_LIMITS[0]);
                self.blend = 1.;
                self.started = true;
                self.quick_fade = false;
                self.strong = false;
                return;
            }
            let half = Vec3::new(
                input.follow_up_vector.x * 0.5,
                input.follow_up_vector.y * 0.5,
                input.follow_up_vector.z * 0.5,
            );
            let (length, direction) = separate_scalar_and_direction(half);
            // (A vector too short to have a direction, or one the sway already runs ahead of,
            // adds nothing and falls through to the fade.)
            if squared_length(half) >= NO_DIRECTION {
                let ahead = length - dot(direction, self.sway);
                let room = if length < IMPULSE_LIMITS[0] {
                    length
                } else {
                    IMPULSE_LIMITS[0]
                };
                let add = if ahead > room { room } else { ahead };
                if ahead >= 0. && add > 0. {
                    let left = (1. - self.blend).max(0.);
                    let add = add * left;
                    self.sway = Vec3::new(
                        self.sway.x + direction.x * add,
                        self.sway.y + direction.y * add,
                        self.sway.z + direction.z * add,
                    );
                    self.started = false;
                    self.quick_fade = false;
                    self.strong = false;
                    return;
                }
            }
        }
        self.started = false;
        if input.fade_quickly {
            self.quick_fade = true;
        }
        if is_near_zero_value(self.blend, NEAR_ZERO) {
            return;
        }
        let step = if self.quick_fade {
            QUICK_BLEND_STEP
        } else {
            BLEND_STEPS[usize::from(self.strong)]
        };
        self.blend = converge(self.blend, 0., step);
        if is_near_zero_value(self.blend, NEAR_ZERO) {
            self.sway = Vec3::ZERO;
        }
    }
}

/// `al::isNearZero(Vector3f, 0.001)`.
fn is_near_vector_zero(v: Vec3) -> bool {
    squared_length(v) < NEAR_ZERO * NEAR_ZERO
}

/// The part of `sway` along `direction`, up to `length`, is taken off it; returns the amount.
fn consume(sway: &mut Vec3, length: f32, direction: Vec3) -> f32 {
    if is_near_zero_value(length, NEAR_ZERO) {
        return 0.;
    }
    let along = dot(direction, *sway);
    let amount = if along < 0. {
        0.
    } else if along > length {
        length
    } else {
        along
    };
    *sway = Vec3::new(
        sway.x - amount * direction.x,
        sway.y - amount * direction.y,
        sway.z - amount * direction.z,
    );
    amount
}

fn column(m: &Mat34, index: usize) -> Vec3 {
    Vec3::new(m[0][index], m[1][index], m[2][index])
}

/// The tilt routine (`0x4546e4`): the matrix leans about its `front` axis so that its `up` axis
/// is `degrees * tilt` further over than it stands from the vertical, as far as `limit` from
/// it, the rotation put on the left of the matrix (about the axes of the space it lies in).
fn tilt_about_front(world: &mut Mat34, front: Vec3, up: Vec3, degrees: f32, tilt: f32, limit: f32) {
    let angle = calc_angle_degree(Vec3::Y, up);
    let across = Vec3::new(
        Vec3::Y.y * up.z - Vec3::Y.z * up.y,
        Vec3::Y.z * up.x - Vec3::Y.x * up.z,
        Vec3::Y.x * up.y - Vec3::Y.y * up.x,
    );
    let side = (across.x * front.x + across.y * front.y) + across.z * front.z;
    let signed = if side < 0. { -angle } else { angle };
    let wanted = degrees * tilt + signed;
    let limited = if wanted < -limit {
        -limit
    } else if wanted > limit {
        limit
    } else {
        wanted
    };
    let half = ((limited - signed) * DEGREES_TO_RADIANS) * 0.5;
    let (sine, cosine) = (sinf(half), cosf(half));
    let q = Quat::from_xyzw(sine * front.x, sine * front.y, sine * front.z, cosine);
    rotate_axes_only(&rotation_rows(q), world);
}

/// `al::separateScalarAndDirection`: the length and the unit direction (zero for a vector
/// shorter than 0.001).
fn separate_scalar_and_direction(v: Vec3) -> (f32, Vec3) {
    let length = squared_length(v).sqrt();
    if squared_length(v) < NO_DIRECTION {
        return (length, Vec3::ZERO);
    }
    let direction = scaled_by_inverse_length(v, squared_length(v).sqrt());
    (length, direction)
}

/// `al::separateVectorHV(across, along, direction, vec)`: `vec` as the part across the unit
/// `direction` and the part along it.
fn separate_vector_hv(direction: Vec3, vec: Vec3) -> (Vec3, Vec3) {
    let amount = (direction.x * vec.x + direction.y * vec.y) + direction.z * vec.z;
    let along = Vec3::new(
        direction.x * amount,
        direction.y * amount,
        direction.z * amount,
    );
    (vec - along, along)
}

/// `al::calcSpringDumperForce(position, velocity, stiffness, damping)`.
fn spring_damper_force(position: f32, velocity: f32, stiffness: f32, damping: f32) -> f32 {
    -(position * stiffness + velocity * damping)
}

/// `al::calcDistanceVecToPlane(direction, point, normal, on_plane)`: how far along `direction`
/// (not a unit vector: its length scales the answer) `point` is from the plane.
fn calc_distance_vec_to_plane(direction: Vec3, point: Vec3, normal: Vec3, on_plane: Vec3) -> f32 {
    let off = point - on_plane;
    let height = (off.x * normal.x + off.y * normal.y) + off.z * normal.z;
    let slope = -((normal.x * direction.x + normal.y * direction.y) + normal.z * direction.z);
    let length = squared_length(direction).sqrt();
    length * (height / slope)
}

/// `al::calcAngleOnPlaneDegree(from, to, normal)`: the signed angle between the two vectors
/// flattened onto the plane across `normal`.
fn calc_angle_on_plane_degree(from: Vec3, to: Vec3, normal: Vec3) -> f32 {
    let flatten = |v: Vec3| {
        let along = (normal.x * v.x + normal.y * v.y) + normal.z * v.z;
        Vec3::new(
            v.x - normal.x * along,
            v.y - normal.y * along,
            v.z - normal.z * along,
        )
    };
    let (a, b) = (flatten(from), flatten(to));
    let dot_ab = (a.x * b.x + a.y * b.y) + a.z * b.z;
    let across = Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    );
    let magnitude = (across.x * across.x + across.y * across.y + across.z * across.z).sqrt();
    let angle = atan2f(magnitude, dot_ab);
    let side = (normal.x * across.x + normal.y * across.y) + normal.z * across.z;
    let angle = if side < 0. { -angle } else { angle };
    angle * RADIANS_TO_DEGREES
}

/// `sead::Matrix33::slerpTo` over rotations: both as quaternions, blended by `t`, back to a
/// rotation (not normalised).
fn slerp_rows(from: &Rows, to: &Rows, t: f32) -> Rows {
    let quat = |m: &Rows| {
        to_quat([
            Vec3::new(m[0][0], m[1][0], m[2][0]),
            Vec3::new(m[0][1], m[1][1], m[2][1]),
            Vec3::new(m[0][2], m[1][2], m[2][2]),
        ])
    };
    rotation_rows(slerp(quat(from), quat(to), t))
}

/// `al::blendMtx`: the translation blends linearly, the rotation by a spherical blend.
fn blend_mtx(from: &Mat34, to: &Mat34, t: f32) -> Mat34 {
    let rows = |m: &Mat34| -> Rows {
        [
            [m[0][0], m[0][1], m[0][2]],
            [m[1][0], m[1][1], m[1][2]],
            [m[2][0], m[2][1], m[2][2]],
        ]
    };
    let rotation = slerp_rows(&rows(from), &rows(to), t);
    let mut out = [[0.; 4]; 3];
    for r in 0..3 {
        for c in 0..3 {
            out[r][c] = rotation[r][c];
        }
        out[r][3] = (1. - t) * from[r][3] + to[r][3] * t;
    }
    out
}

/// `al::makeMtxSideUpPos`: the matrix whose x axis is `side`, whose y axis is `up` made
/// perpendicular to it through the z axis, at `position`.
fn make_mtx_side_up_pos(side: Vec3, up: Vec3, position: Vec3) -> Mat34 {
    let x = normalize(side);
    let z = normalize(cross(x, up));
    let y = cross(z, x);
    [
        [x.x, y.x, z.x, position.x],
        [x.y, y.y, z.y, position.y],
        [x.z, y.z, z.z, position.z],
    ]
}

/// `al::calcBetweenTwoLinkPos`: where the joint between two links of the given lengths stands
/// when the chain runs from `from` to `to`, bent toward `hint`. A chain that cannot be bent
/// (out of reach, or `hint` along it) is straight.
fn calc_between_two_link_pos(from: Vec3, to: Vec3, first: f32, second: f32, hint: Vec3) -> Vec3 {
    let straight = |d: Vec3| {
        let share = second / (first + second);
        Vec3::new(share * d.x + to.x, share * d.y + to.y, share * d.z + to.z)
    };
    let d = from - to;
    let squared = squared_length(d);
    if squared < NO_DIRECTION {
        return straight(d);
    }
    let distance = squared.sqrt();
    let direction = if distance <= 0. {
        d
    } else {
        let inverse = 1. / distance;
        Vec3::new(d.x * inverse, d.y * inverse, d.z * inverse)
    };
    let along = ((second * second - first * first) + squared) / (squared + squared);
    let middle = Vec3::new(d.x * along + to.x, d.y * along + to.y, d.z * along + to.z);
    let off = middle - to;
    let height_squared = second * second - squared_length(off);
    if height_squared <= 0. {
        return straight(d);
    }
    let bent = Vec3::new(
        direction.z * hint.y - direction.y * hint.z,
        direction.x * hint.z - direction.z * hint.x,
        direction.y * hint.x - direction.x * hint.y,
    );
    let across = Vec3::new(
        direction.y * bent.z - direction.z * bent.y,
        direction.z * bent.x - direction.x * bent.z,
        direction.x * bent.y - direction.y * bent.x,
    );
    let across_squared = across.z * across.z + (across.x * across.x + across.y * across.y);
    if across_squared < NO_DIRECTION {
        return straight(d);
    }
    let length = across_squared.sqrt();
    let unit = if length <= 0. {
        across
    } else {
        let inverse = 1. / length;
        Vec3::new(across.x * inverse, across.y * inverse, across.z * inverse)
    };
    let height = height_squared.sqrt();
    Vec3::new(
        middle.x + unit.x * height,
        middle.y + unit.y * height,
        middle.z + unit.z * height,
    )
}

#[path = "../tests/ground_pose.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
