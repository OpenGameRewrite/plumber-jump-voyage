//! The pole-climbing pose of the limbs (`PlayerJointControlPoleClimbPose`): while the body hangs
//! on a pole the arms and the legs are turned about their own vertical (y) axes, the further the
//! wider the pole is, so the hands and feet follow its curve.
//!
//! `PlayerStatePoleClimb` hands the keeper a block (`PlayerJointParamHandLegAngle`): the angles
//! the arms and the legs should have, and how fast to follow them. `updatePoleDepth` writes the
//! angles from the width of the pole and the slant its collision code adds; the nerves write
//! the rate (slow while a move runs, whole otherwise) and clear the angles where the limbs
//! are free. The keeper then steps the controller toward the block once a frame (`update`), and
//! the model's world calculation gives it the four joints (`calc`): the left limbs turn by the
//! angle, the right ones by its opposite, on top of whatever the ground pose made of them.
use super::joint_math::{
    calc_rate01, lerp_value, lerp_vec, rotate_own_x, rotate_own_y, rotate_own_z, turn_scale_free,
};
use super::look_joint::Mat34;
use super::vector_lib::is_near_zero;
use crate::skeleton_pose::Joint;
use bevy::prelude::Vec3;

/// `PlayerConst` defaults (Mario has no player-constant file): the width the pole is the
/// body's own (`PoleClimbCatchRange`), the limit of the turn either way
/// (`PoleClimbJointAngleMin` and `Max`, degrees) and the widths where it is whole
/// (`PoleClimbJointRangeMin` and `Max`).
const CATCH_RANGE: f32 = 50.;
const JOINT_ANGLE_MIN: f32 = -25.;
const JOINT_ANGLE_MAX: f32 = 25.;
const JOINT_RANGE_MIN: f32 = 15.;
const JOINT_RANGE_MAX: f32 = 80.;
/// `al::isNearZero`'s limit for the angle vectors.
const NEAR_ZERO: f32 = 0.001;
/// The width limit `updatePoleDepth` compares with the catch range.
const SAME_WIDTH: f32 = 0.001;

/// The angle block the pole state writes and the keeper reads.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Block {
    arms: Vec3,
    legs: Vec3,
    rate: f32,
}

impl Block {
    /// The keeper's block when no pole state has written it: no angles, and a whole rate.
    const FREE: Block = Block {
        arms: Vec3::ZERO,
        legs: Vec3::ZERO,
        rate: 1.,
    };
}

/// The pole pose controller and the block the pole state steers it with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PolePose {
    /// The controller's angles now (`+0x28` for the arms, `+0x34` for the legs), degrees.
    arms: Vec3,
    legs: Vec3,
    block: Block,
}

impl Default for PolePose {
    fn default() -> Self {
        Self {
            arms: Vec3::ZERO,
            legs: Vec3::ZERO,
            block: Block::FREE,
        }
    }
}

/// The turn `updatePoleDepth` asks of the limbs for a pole `depth` wide on a face with the
/// code's `angle_offset` (all in cm and degrees): none at the catch range, `JointAngleMax`
/// for wider poles (whole from `JointRangeMax`), and `JointAngleMin` for narrower ones (whole
/// below `JointRangeMin`).
pub fn joint_angle(depth: f32, angle_offset: f32) -> f32 {
    let turn = if (depth - CATCH_RANGE).abs() < SAME_WIDTH {
        0.
    } else if depth > CATCH_RANGE {
        let rate = calc_rate01(depth, CATCH_RANGE, JOINT_RANGE_MAX);
        lerp_value(0., JOINT_ANGLE_MAX, rate)
    } else {
        let rate = calc_rate01(depth, JOINT_RANGE_MIN, CATCH_RANGE);
        lerp_value(JOINT_ANGLE_MIN, 0., rate)
    };
    turn + angle_offset
}

impl PolePose {
    /// `updatePoleDepth`: both the arms and the legs are asked to turn about y by the angle.
    pub fn ask_angle(&mut self, angle: f32) {
        // (The original multiplies the constant axis (0, 1, 0) by the angle.)
        let asked = Vec3::new(0. * angle, 1. * angle, 0. * angle);
        self.block.arms = asked;
        self.block.legs = asked;
    }

    /// How fast the controller follows the block: the climbing moves ask for a tenth, the
    /// states where the limbs hang still for the whole.
    pub fn ask_rate(&mut self, rate: f32) {
        self.block.rate = rate;
    }

    /// The legs are free of the pose (the pole top's start).
    pub fn free_legs(&mut self) {
        self.block.legs = Vec3::ZERO;
    }

    /// The jumps and the end of the state: no angles, a whole rate.
    pub fn release(&mut self) {
        self.block = Block::FREE;
    }

    /// `PlayerJointControlKeeper::updatePoleClimb` (`PoleClimbPose::update`): the angles go
    /// toward the block's by its rate, whatever state the actor is in.
    pub fn update(&mut self) {
        self.arms = lerp_vec(self.arms, self.block.arms, self.block.rate);
        self.legs = lerp_vec(self.legs, self.block.legs, self.block.rate);
    }

    /// `calcJointCallback`: bend `world`, the matrix of `joint` as the model has just computed it.
    pub fn calc(&self, joint: Joint, world: &mut Mat34) {
        if is_near_zero(self.legs, NEAR_ZERO) && is_near_zero(self.arms, NEAR_ZERO) {
            return;
        }
        // The right limbs turn the opposite way; each turn is about the axis the matrix has
        // after the one before (x, y, then z).
        let (angles, sign) = match joint {
            Joint::ArmL1 => (self.arms, 1.),
            Joint::ArmR1 => (self.arms, -1.),
            Joint::LegL1 => (self.legs, 1.),
            Joint::LegR1 => (self.legs, -1.),
            _ => return,
        };
        turn_scale_free(world, |m| {
            rotate_own_x(m, sign * angles.x);
            rotate_own_y(m, sign * angles.y);
            rotate_own_z(m, sign * angles.z);
        });
    }
}

#[path = "../tests/pole_pose.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
