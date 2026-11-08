//! Which child the spin-cap state starts in (`PlayerStateSpinCap::appear`, SMO 1.0.0 0x481958).
//!
//! `appear` reads two action triggers the throwing nerve set in the same step, asks
//! `rs::isOnGround` (0x568788) and chooses `exeSpinGround` or `exeSpinAir` (0x481a94 and
//! 0x481aa0; the air child also stores 1 at +0x78):
//!
//! - trigger 0x21 on: the ground child. `exeRun` sets it for a throw pressed while running on
//!   the water's surface (0x423370, behind `isRunWaterSurface`); `exeSpinCap` sets it when it
//!   throws again out of a water-surface spin (0x423acc).
//! - otherwise `rs::isOnGround(actor, collider)` and trigger 2 off: the ground child.
//!   `exeFall` sets trigger 2 (0x425340) when its throw succeeds and
//!   `rs::isLandGroundRunAngle` (0x569ac8) does not hold, trigger 1 (0x425284) when it does.
//!   With trigger 1 the ground child also carries the velocity onto the ground it landed on
//!   (0x481a1c..0x481a70: `spin_cap_fall_landing_velocity`).
//! - anything else: the air child, which hands `-gravity` to the cap (0x4829d4).
//!
//! Every throw reaches the state through the actor's `tryActionCapSpinAttackImpl` (0x4294cc) and
//! the `SpinCap` nerve, whichever actor state asked (ground, run, fall, burn jump, wall jump...):
//! none of them picks the child itself.
use super::*;

impl Player {
    /// `rs::isLandGroundRunAngle` (0x569ac8): the collider holds ground the velocity is not
    /// leaving, within the run angle for the speed across gravity.
    pub(super) fn is_land_ground_run_angle(&self) -> bool {
        self.is_on_ground_by(self.v) && self.run_angle_contact(true)
    }

    /// The velocity `PlayerStateSpinCap::appear` hands the ground child when `exeFall` set
    /// trigger 1 (the throw pressed in a fall that lands within the run angle, 0x425280): the
    /// fall's velocity carried onto the collided ground, `alongVectorNormalH(velocity, velocity,
    /// -gravity, collidedGroundNormal)` (0x481a1c..0x481a70, before the ground move control's
    /// `appear` and so before the first step's brake). The part along gravity is dropped and the
    /// horizontal part turns about the axis across the two normals into the ground's plane,
    /// keeping its length. `None` for every other way into the ground child.
    ///
    /// The port keeps a ground state's velocity as that velocity plus the floor pull along
    /// `-normal` (what `ground_following` lays down), so the pull is added back here.
    pub(super) fn spin_cap_fall_landing_velocity(&self) -> Option<Vec3> {
        (self.mode == Move::Fall && self.is_land_ground_run_angle()).then(|| {
            let normal = self.collided_ground_normal();
            pose_math::along_vector_normal_h(self.v, pose_math::NEGATED_GRAVITY, normal)
                - normal * GRAVITY_MOVE
        })
    }

    /// `PlayerStateSpinCap::appear` starts the air child (`exeSpinAir`) for the actor as it is
    /// now. Only the triggers the port's states produce are known: 0x21 from a throw on the run
    /// over the water's surface (the run nerve's judge) and 2 from a throw in the fall. The
    /// rethrow out of a water-surface spin (0x423acc) is not ported with the rethrow.
    pub(super) fn spin_cap_in_air(&self) -> bool {
        let water_surface_run =
            matches!(self.mode, Move::Run | Move::Walk) && self.water_surface_run();
        let fall_off_run_angle = self.mode == Move::Fall && !self.is_land_ground_run_angle();
        let ground_child = water_surface_run || self.is_on_ground_by(self.v) && !fall_off_run_angle;
        !ground_child
    }
}
