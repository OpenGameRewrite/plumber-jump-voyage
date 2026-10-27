//! The swim pose of the body (`PlayerJointControlSwimPose`): how the swimmer pitches forward,
//! heels over and turns the spine while it swims.
//!
//! `PlayerStateSwim` holds four angles (`PlayerJointParamSwim`, in degrees) that its two
//! swimming nerves, the move and the paddle, steer every step (`update`, the executable's helper
//! at `0x486a34`): the faster the swimmer goes forward, up to `SwimLowSpeedMaxH`, the further the
//! pitch follows its limit, and the sideways heel and the spine's turn follow the body's turn
//! power as well. Starting and ending the state clears them, so do the first steps of the dive and
//! of the hip drop's wind-up, and the nerves at the surface and on the bottom ease them off
//! (`ease_off`); the other nerves leave them as they are. The model's world calculation then
//! gives the controller the root's matrix, which it turns about a point 80 cm above it (the pitch
//! about the root's own x axis, the heel about its y axis, the roll about its z axis), and the
//! first spine joint's, which it turns about its own y axis (`calc`).
//!
//! The spine's turn only happens while the heel is not near zero: the controller gates it on the
//! heel's angle (the second one), not on its own (a quirk of the executable that is kept).
//!
//! The arithmetic follows the executable's single-precision order and is checked bit for bit
//! against it (`PJV_SWIM_CASES`, `tests/swim_pose.rs`).
use super::joint_math::{calc_rate01, lerp_value, rotate_own_y};
use super::look_joint::Mat34;
use super::pose_math::{DEGREES_TO_RADIANS, cosf, sinf, to_quat};
use crate::skeleton_pose::Joint;
use bevy::prelude::Vec3;

/// `PlayerConst` defaults (Mario has no player-constant file): the forward speed at which the
/// pitch is whole, the pitch and the heel and spine limits (degrees) and how fast they follow.
const LOW_SPEED_MAX_H: f32 = 6.5;
const BENT_FORWARD_MAX: f32 = 30.;
const BENT_FORWARD_BLEND_RATE: f32 = 0.05;
const BENT_SIDE_MAX: f32 = 60.;
const BENT_SPINE_MAX: f32 = 40.;
const BENT_SIDE_BLEND_RATE: f32 = 0.05;
/// `SwimBentFrontBlendRate`: how fast the nerves that let the pose go ease the spine's turn back.
const BENT_FRONT_BLEND_RATE: f32 = 0.04;
/// How fast the dive into the water brings the pitch to the travel's angle (the literal in the
/// executable's helper at `0x488410`).
const DIVE_IN_PITCH_RATE: f32 = 0.2;
/// The point the root turns about, in its own frame (`+0x30` of the controller).
const PIVOT: Vec3 = Vec3::new(0., 80., 0.);
/// `al::isNearZero`'s limit for the angles.
const NEAR_ZERO: f32 = 0.001;

/// The four angles of `PlayerJointParamSwim`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SwimPose {
    /// The pitch about the root's x axis.
    pitch: f32,
    /// The heel about its y axis.
    heel: f32,
    /// The roll about its z axis (it only ever eases to zero).
    roll: f32,
    /// The spine's turn about its y axis.
    spine: f32,
}

impl SwimPose {
    /// `PlayerStateSwim::appear` and `kill`: no pose.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// What the nerves that are not swimming forward do every step (the surface, the bottom's
    /// standing and walking): each angle eases toward zero by its rate, the spine's by the front
    /// blend rate (`lerpValue(angle, 0, rate)` inlined in those `PlayerStateSwim` functions).
    pub fn ease_off(&mut self) {
        self.pitch = lerp_value(self.pitch, 0., BENT_FORWARD_BLEND_RATE);
        self.heel = lerp_value(self.heel, 0., BENT_SIDE_BLEND_RATE);
        self.roll = lerp_value(self.roll, 0., BENT_SIDE_BLEND_RATE);
        self.spine = lerp_value(self.spine, 0., BENT_FRONT_BLEND_RATE);
    }

    /// The first step of the dive into the water: the pitch takes the spine's angle and the other
    /// three are cleared (`PlayerStateSwim::exeSwimHeadIn` / `exeSwimDiveIn`).
    pub fn start_dive_in(&mut self, pitch: f32) {
        *self = Self {
            pitch,
            ..Self::default()
        };
    }

    /// The cap throw's first step: only the pitch is set (the angle the model's root leans).
    pub fn set_pitch(&mut self, pitch: f32) {
        self.pitch = pitch;
    }

    /// The pitch follows the angle of the travel against the body (the helper at `0x488410`
    /// that the two dive-in nerves end with): a fifth of the way each step.
    pub fn follow_travel(&mut self, angle: f32) {
        self.pitch = lerp_value(self.pitch, angle, DIVE_IN_PITCH_RATE);
    }

    /// The angles in the executable's order, for the tests.
    #[allow(dead_code)]
    pub fn angles(&self) -> [f32; 4] {
        [self.pitch, self.heel, self.roll, self.spine]
    }

    /// The helper `PlayerStateSwim` calls after the move and the turn of the swimming nerves:
    /// `front` and `velocity` are the actor's, `turn_power` the body's signed turn power against
    /// the up axis (`PlayerActionTurnControl::calcTurnPowerRate`).
    pub fn update(&mut self, front: Vec3, velocity: Vec3, turn_power: f32) {
        let forward = (front.x * velocity.x + front.y * velocity.y) + front.z * velocity.z;
        let rate = calc_rate01(forward, 0., LOW_SPEED_MAX_H);
        self.pitch = lerp_value(self.pitch, rate * BENT_FORWARD_MAX, BENT_FORWARD_BLEND_RATE);
        let turned = rate * turn_power;
        self.heel = lerp_value(self.heel, turned * BENT_SIDE_MAX, BENT_SIDE_BLEND_RATE);
        self.spine = lerp_value(self.spine, turned * BENT_SPINE_MAX, BENT_SIDE_BLEND_RATE);
        self.roll = lerp_value(self.roll, 0., BENT_SIDE_BLEND_RATE);
    }

    /// `calcJointCallback`: bend `world`, the matrix of `joint` as the model has just computed it.
    pub fn calc(&self, joint: Joint, world: &mut Mat34) {
        match joint {
            Joint::AllRoot => {
                // Each turn is about the axis the matrix has after the one before.
                if !is_near_zero(self.pitch) {
                    rotate_about_center(world, 0, self.pitch);
                }
                if !is_near_zero(self.heel) {
                    rotate_about_center(world, 1, self.heel);
                }
                if !is_near_zero(self.roll) {
                    rotate_about_center(world, 2, self.roll);
                }
            }
            Joint::Spine1 => {
                if is_near_zero(self.heel) {
                    return;
                }
                rotate_own_y(world, self.spine);
            }
            _ => {}
        }
    }
}

fn is_near_zero(value: f32) -> bool {
    value.abs() <= NEAR_ZERO
}

/// `al::rotateMtxCenterPos{X,Y,Z}DirDegree(m, m, PIVOT, degrees)`: the matrix turned by
/// `degrees` about its own axis `axis` (a column of its rotation), about `PIVOT` as the matrix
/// places it. The turn is made as a quaternion (`al::makeQuatRotateDegree`) that
/// `al::rotateMtxCenterPosQuat` multiplies with the matrix's own, which drops the matrix's scale.
fn rotate_about_center(m: &mut Mat34, axis: usize, degrees: f32) {
    let column = |c: usize| Vec3::new(m[0][c], m[1][c], m[2][c]);
    let about = column(axis);
    let half = (degrees * DEGREES_TO_RADIANS) * 0.5;
    let (cos, sin) = (cosf(half), sinf(half));
    let (qx, qy, qz, qw) = (sin * about.x, sin * about.y, sin * about.z, cos);
    // The matrix's own orientation.
    let own = to_quat([column(0), column(1), column(2)]);
    let (ox, oy, oz, ow) = (own.x, own.y, own.z, own.w);
    // The product of the two, in the executable's order of operations.
    let w = ((qw * ow - qx * ox) - qy * oy) - qz * oz;
    let x = ((ow * qx + qw * ox) + qy * oz) - oy * qz;
    let y = (ox * qz) + ((ow * qy) + ((qw * oy) - (qx * oz)));
    let z = (ow * qz) + (((qx * oy) + (qw * oz)) - (ox * qy));
    let squared = (((w * w) + (x * x)) + (y * y)) + (z * z);
    let length = squared.sqrt();
    let (w, x, y, z) = if length > 0. {
        let inverse = 1. / length;
        (inverse * w, inverse * x, inverse * y, inverse * z)
    } else {
        (w, x, y, z)
    };
    let (cx, cy, cz) = (PIVOT.x, PIVOT.y, PIVOT.z);
    let a = *m;
    let two_y = y + y;
    let two_x = x + x;
    let two_w = w + w;
    let (t2, t17, t19) = (cy * a[0][1], cy * a[1][1], cy * a[2][1]);
    let (t3, t16, t18) = (cx * a[0][0], cx * a[1][0], cx * a[2][0]);
    let s20 = y * cz;
    let s21 = z * cy;
    let s24 = x * cy;
    let s27 = y * cx;
    let s28 = -(x * cx);
    let s12 = y * cy;
    let s23 = x * two_w;
    let s22 = z * cx;
    let s30 = x * cz;
    let s24 = s24 - s27;
    let s2 = t3 + t2;
    let s3 = y * two_x;
    let s16 = t16 + t17;
    let s17 = two_x * z;
    let s18 = t18 + t19;
    let s19 = two_y * z;
    let s20 = s20 - s21;
    let s21 = z * two_w;
    let s6 = y * two_w;
    let s27 = s28 - s12;
    let s28 = s3 - s21;
    let s29 = s17 + s6;
    let s3 = s3 + s21;
    let s21 = s19 - s23;
    let s6 = s17 - s6;
    let s17 = s19 + s23;
    let s25 = cz * a[0][2];
    let s22 = s22 - s30;
    let s30 = cz * a[1][2];
    let s2 = s2 + s25;
    let s0 = w * cx;
    let s0 = s0 + s20;
    let s20 = w * cz;
    let s19 = cz * a[2][2];
    let s7 = z * cz;
    let s18 = s18 + s19;
    let s1 = w * cy;
    let s19 = s20 + s24;
    let s20 = z + z;
    let s4 = y * two_y;
    let s5 = x * two_x;
    let s16 = s16 + s30;
    let s1 = s1 + s22;
    let s5 = 1. - s5;
    let s30 = 1. - s4;
    let out20 = s6;
    let out21 = s17;
    let s6 = a[1][3] + s16;
    let s16 = a[2][3] + s18;
    let s20 = z * s20;
    let s17 = w * s0;
    let s18 = z * s1;
    let s30 = s30 - s20;
    let s20 = s5 - s20;
    let s7 = s27 - s7;
    let s2 = a[0][3] + s2;
    let s22 = y * s19;
    let s24 = w * s1;
    let s1 = x * s1;
    let s26 = z * s0;
    let s0 = y * s0;
    let s23 = x * s7;
    let s25 = x * s19;
    let s19 = w * s19;
    let s0 = s1 - s0;
    let s27 = y * s7;
    let s8 = z * s7;
    let out10 = s3;
    let out11 = s20;
    let s3 = s17 - s18;
    let s17 = s26 + s24;
    let s1 = s22 + s3;
    let s3 = s17 - s25;
    let s0 = s19 + s0;
    let s0 = s0 - s8;
    let s1 = s1 - s23;
    let s3 = s3 - s27;
    let out23 = s16 - s0;
    let out00 = s30;
    let out01 = s28;
    let out03 = s2 - s1;
    let out13 = s6 - s3;
    let out02 = s29;
    let out22 = s5 - s4;
    let out12 = s21;
    *m = [
        [out00, out01, out02, out03],
        [out10, out11, out12, out13],
        [out20, out21, out22, out23],
    ];
}

#[path = "../tests/swim_pose.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
