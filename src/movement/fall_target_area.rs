//! The area `PlayerStateFallHakoniwa` latches and steers toward: `appear` 0x46afb8 and
//! `updateFallTargetArea` 0x46b0bc. [V] Exactly two direct calls reach `updateFallTargetArea`:
//! `appear` (call at 0x46b088) and the child's `exeFall` (0x46b394, call at 0x46b3b4, before the
//! first-step air setup and the air control).
//!
//! The latch itself (state +0x70, cleared by `appear`, set from the state's own
//! `PlayerJudgeInvalidateInputFall` the first time it holds, never released while the state
//! lasts) and its input silence are in `invalidate_input.rs`. What follows the latch here:
//!
//! 1. `al::getAreaObjBaseMtx` of the latched area gives the area's up axis (column 1) and its
//!    translation (column 3). The offset from the actor to that translation is made vertical to
//!    the axis (`al::verticalizeVec`): `distance` is its length, `direction` its unit vector.
//! 2. The velocity splits along the axis (`al::separateVectorHV`) into a vertical part, which
//!    stays, and a horizontal part, which splits again along `direction`
//!    (`al::separateVectorParallelVertical`) into `toward` (signed speed toward the axis) and
//!    the sideways remainder.
//! 3. The sideways remainder keeps 0.75 of itself. The speed toward the axis `toward` becomes
//!    `toward` when it is not negative and `5 * toward <= distance`, else `0.75 * toward`; then
//!    0.1 is added and the result is limited to the smaller of `distance` and the ceiling
//!    stored at `appear` (state +0x78). That is the new speed along `direction`.
//! 4. With no direction (the actor stands on the axis) the horizontal part is zero.
//!    The new velocity is the new horizontal part plus the vertical part.
//!
//! The ceiling (+0x78, `appear` 0x46b03c..0x46b080) is the horizontal speed clamped between
//! `NormalMaxSpeed` (`PlayerConst` slot 0x1c8) and `JumpMoveSpeedMax` (slot 0x5f8). The
//! constants 0.75, 5 and 0.1 are literals of `updateFallTargetArea` (0x46b238, 0x46b22c,
//! 0x46b270), not `PlayerConst` values.
//!
//! Frame order [V]. When the actor picks Fall in the middle of an execution, `NerveKeeper::update`
//! (decomp `NerveKeeper.cpp`) calls `tryChangeNerve` again and the state controller runs `appear`
//! then, in the same frame, right after that `exeXxx`. `appear` is therefore the first steering.
//! The child's `exeFall` has not run yet: its first execution (the second call, 0x46b3b4) is part
//! of the actor's next Fall update, the following frame.
//!
//! Calls in the port: `appear_fall_target` from `change` (the frame that chose Fall), then
//! `update_fall_target_area` at the top of every Fall state step (`run_state`), the first one
//! being the frame after the choice. A touchdown step skips `run_state` but the actor's
//! `exeFall` still updates its child first (0x425240), before it asks SandSink (+0x450, 0x42539c)
//! or lands (0x425410): both touchdown branches of `change_from_surroundings` (the sand one and
//! `land`) therefore steer first, with `update_fall_target_at_touchdown`. The water judge
//! (+0x410, 0x4255fc) is skipped by an accepted landing; a contact that fails the run-angle
//! gate (0x4253e0..0x4253e4) continues through the judges to it. [I] The port holds the position and velocity of that point of the
//! step, which is where the executable reads them, except for the other port states that move
//! the actor between `change` and the end of the step, and the stage position cached at the top
//! of the step (the executable reads the current translation at 0x46b134).
use super::pose_math::{dot, try_normalize_or_zero};
use super::vector_lib::{length, separate_parallel_vertical, verticalize_vec};
use super::{Player, t};
use crate::water::Area;
use bevy::prelude::Vec3;

/// What the steering reads of an `InvalidateInputFallArea`: the columns of
/// `al::getAreaObjBaseMtx` (rotation and translation, without the scale), centimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TargetArea {
    /// Column 1, the area's own up axis.
    pub(super) axis: Vec3,
    /// Column 3, the area's translation.
    pub(super) center: Vec3,
}

impl TargetArea {
    pub(super) fn of(area: &Area) -> Self {
        let m = &area.matrix;
        Self {
            axis: Vec3::new(m[1], m[5], m[9]),
            center: Vec3::new(m[3], m[7], m[11]),
        }
    }
}

/// The Fall state's +0x70 (the latched area) and +0x78 (the speed ceiling `appear` stored).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct FallTarget {
    pub(super) area: Option<TargetArea>,
    pub(super) max_speed: f32,
}

/// The share of the sideways speed `updateFallTargetArea` keeps (the literal at 0x46b238), and the
/// share of the speed toward the axis it keeps when the actor is near the axis.
const SIDEWAYS_KEPT: f32 = 0.75;
/// A speed toward the axis is "far" when this many times it still fits in the distance (0x46b22c).
const NEAR_AXIS_RATE: f32 = 5.;
/// Added to the speed toward the axis every call (the float at 0x187fc84).
const PULL_TOWARD_AXIS: f32 = 0.1;

/// The ceiling `appear` stores (0x46b03c..0x46b080): the horizontal speed, at least
/// `NormalMaxSpeed` and at most `JumpMoveSpeedMax`.
pub(super) fn speed_ceiling(speed_h: f32) -> f32 {
    if speed_h < t::NORMAL_MAX {
        t::NORMAL_MAX
    } else if speed_h <= t::JUMP_MOVE_MAX {
        speed_h
    } else {
        t::JUMP_MOVE_MAX
    }
}

/// The velocity `updateFallTargetArea` leaves (0x46b0f8..0x46b2e4) for an actor at `position`
/// (centimetres) moving with `velocity`.
pub(super) fn steered(velocity: Vec3, position: Vec3, area: &TargetArea, max_speed: f32) -> Vec3 {
    let offset = verticalize_vec(
        area.axis,
        Vec3::new(
            area.center.x - position.x,
            area.center.y - position.y,
            area.center.z - position.z,
        ),
    );
    let distance = length(offset);
    // `al::separateVectorHV(horizontal, vertical, axis, velocity)`.
    let along_axis = dot(area.axis, velocity);
    let vertical = Vec3::new(
        area.axis.x * along_axis,
        area.axis.y * along_axis,
        area.axis.z * along_axis,
    );
    let horizontal = Vec3::new(
        velocity.x - vertical.x,
        velocity.y - vertical.y,
        velocity.z - vertical.z,
    );
    let (direction, found) = try_normalize_or_zero(offset);
    let horizontal = if found {
        let (parallel, sideways) = separate_parallel_vertical(direction, horizontal);
        let toward = dot(parallel, direction);
        let damped = toward * SIDEWAYS_KEPT;
        let kept = if toward * NEAR_AXIS_RATE <= distance {
            toward
        } else {
            damped
        };
        let speed = (if toward >= 0. { kept } else { damped }) + PULL_TOWARD_AXIS;
        let ceiling = if distance < max_speed {
            distance
        } else {
            max_speed
        };
        let speed = if speed > ceiling { ceiling } else { speed };
        Vec3::new(
            sideways.x * SIDEWAYS_KEPT + direction.x * speed,
            sideways.y * SIDEWAYS_KEPT + direction.y * speed,
            sideways.z * SIDEWAYS_KEPT + direction.z * speed,
        )
    } else {
        Vec3::ZERO
    };
    Vec3::new(
        horizontal.x + vertical.x,
        horizontal.y + vertical.y,
        horizontal.z + vertical.z,
    )
}

impl Player {
    /// `PlayerStateFallHakoniwa::appear` (0x46afb8): the ceiling from the speed the actor has,
    /// then the first `updateFallTargetArea`. The latch was cleared by `clear_fall_input_target`
    /// (0x46b084) when the state was chosen.
    pub(super) fn appear_fall_target(&mut self) {
        self.input_invalidation.fall.max_speed = speed_ceiling(self.speed_h());
        self.update_fall_target_area();
    }

    /// `updateFallTargetArea` (0x46b0bc): while no area is latched the state's own judge asks for
    /// one (no ground contact, the position inside an `InvalidateInputFallArea`) and keeps the
    /// first it finds; once one is held, the velocity is steered toward its axis every call,
    /// inside the volume or not. SpinCap's fall child is another state and has none.
    pub(super) fn update_fall_target_area(&mut self) {
        if let Some(velocity) = self.fall_target_steering(self.v) {
            self.v = velocity;
        }
    }

    /// The child's `exeFall` on a touchdown step (call at 0x46b3b4, run by the actor's
    /// `exeFall` at 0x425240 before SandSink at 0x42539c and the landing at 0x425410). The
    /// collider has already stopped the fall (`terrain.rs` clears `v.y` at contact and keeps the
    /// speed that reached the ground in `touchdown_speed`), but the executable steers the full
    /// actor velocity, vertical speed included (0x46b1b0..0x46b2e4 read and write all three
    /// components). So the steering acts on the velocity with the incoming vertical speed and
    /// writes its horizontal part back. Returns the vertical speed it leaves, `None` when no area
    /// is latched (the velocity is untouched): the caller carries that speed into the air update
    /// that follows, as the executable's velocity does.
    pub(super) fn update_fall_target_at_touchdown(&mut self) -> Option<f32> {
        let incoming = self.v.with_y(self.touchdown_speed);
        let velocity = self.fall_target_steering(incoming)?;
        self.set_h(velocity);
        Some(velocity.y)
    }

    /// `updateFallTargetArea` on `velocity`: latches the area, and returns the steered velocity
    /// once one is held.
    fn fall_target_steering(&mut self, velocity: Vec3) -> Option<Vec3> {
        if self.actor_name() != "Fall" {
            self.input_invalidation.fall = FallTarget::default();
            return None;
        }
        if self.input_invalidation.fall.area.is_none() && !self.is_collided_ground() {
            self.input_invalidation.fall.area = self.input_invalidation.in_area;
        }
        let area = self.input_invalidation.fall.area?;
        Some(steered(
            velocity,
            self.input_invalidation.position,
            &area,
            self.input_invalidation.fall.max_speed,
        ))
    }

    /// A Fall restored from a recording (`Player::inject`) when the player was not in Fall: the
    /// state's `appear` ran frames earlier, with a speed the snapshot does not hold, and stored
    /// the ceiling (+0x78) the steering reads. Zero is outside what `appear` can store (14..30),
    /// so the ceiling is the one `appear` computes from the speed the restored actor has.
    /// [I] The entry speed is hidden state; the area (+0x70) starts empty because the snapshot
    /// does not say whether the original had latched one, and the next Fall update asks the
    /// judge as it does for any Fall that has not latched yet. The restored velocity already
    /// holds `appear`'s own steering, so none is applied here.
    pub(super) fn restore_fall_target(&mut self) {
        self.input_invalidation.fall = FallTarget {
            area: None,
            max_speed: speed_ceiling(self.speed_h()),
        };
    }

    /// State +0x70 of the Fall state holds an area: the gate of the actor's `exeFall` reads it.
    pub(super) fn fall_target_latched(&self) -> bool {
        self.actor_name() == "Fall" && self.input_invalidation.fall.area.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis_y() -> TargetArea {
        TargetArea {
            axis: Vec3::Y,
            center: Vec3::ZERO,
        }
    }

    /// 100 cm from the axis, moving toward it at 4: five times 4 is below the distance, so the
    /// speed is kept and 0.1 is added.
    #[test]
    fn a_far_actor_speeds_up_toward_the_axis() {
        let v = steered(
            Vec3::new(-4., -7., 0.),
            Vec3::new(100., 50., 0.),
            &axis_y(),
            14.,
        );
        assert_eq!(v, Vec3::new(-(4. + 0.1), -7., 0.));
    }

    /// Five times the speed that exactly fits the distance still counts as far (`<=`, 0x46b250).
    #[test]
    fn the_far_test_includes_the_exact_fit() {
        let v = steered(
            Vec3::new(-4., 0., 0.),
            Vec3::new(20., 0., 0.),
            &axis_y(),
            14.,
        );
        assert_eq!(v, Vec3::new(-(4. + 0.1), 0., 0.));
    }

    /// Five times the speed does not fit in 18 cm: the speed is damped although four times fits.
    #[test]
    fn the_far_test_uses_five_times_the_speed() {
        let v = steered(
            Vec3::new(-4., 0., 0.),
            Vec3::new(18., 0., 0.),
            &axis_y(),
            14.,
        );
        assert_eq!(v, Vec3::new(-(4. * 0.75 + 0.1), 0., 0.));
    }

    /// 10 cm from the axis the same speed is damped to three quarters before the 0.1.
    #[test]
    fn a_near_actor_is_damped() {
        let v = steered(
            Vec3::new(-4., 0., 0.),
            Vec3::new(10., 0., 0.),
            &axis_y(),
            14.,
        );
        assert_eq!(v, Vec3::new(-(4. * 0.75 + 0.1), 0., 0.));
    }

    /// The new speed never exceeds the distance to the axis (here 2) or the ceiling.
    #[test]
    fn the_new_speed_is_limited_by_the_distance_and_the_ceiling() {
        let near = steered(
            Vec3::new(-4., 0., 0.),
            Vec3::new(2., 0., 0.),
            &axis_y(),
            14.,
        );
        assert_eq!(near, Vec3::new(-2., 0., 0.));
        let capped = steered(
            Vec3::new(-20., 0., 0.),
            Vec3::new(500., 0., 0.),
            &axis_y(),
            14.,
        );
        assert_eq!(capped, Vec3::new(-14., 0., 0.));
    }

    /// Moving away from the axis the speed is damped, then pulled 0.1 back: it still points away.
    #[test]
    fn moving_away_is_damped_and_pulled_back() {
        let v = steered(
            Vec3::new(4., 0., 0.),
            Vec3::new(100., 0., 0.),
            &axis_y(),
            14.,
        );
        assert_eq!(v, Vec3::new(-(4. * -0.75 + 0.1), 0., 0.));
    }

    /// The sideways part keeps three quarters, the axis speed gets its 0.1.
    #[test]
    fn the_sideways_part_keeps_three_quarters() {
        let v = steered(
            Vec3::new(0., 0., 8.),
            Vec3::new(100., 0., 0.),
            &axis_y(),
            14.,
        );
        assert_eq!(v, Vec3::new(-0.1, 0., 8. * 0.75));
    }

    /// On the axis there is no direction: the horizontal speed is lost, the vertical stays.
    #[test]
    fn on_the_axis_only_the_vertical_speed_stays() {
        let v = steered(
            Vec3::new(3., -7., 2.),
            Vec3::new(0., 40., 0.),
            &axis_y(),
            14.,
        );
        assert_eq!(v, Vec3::new(0., -7., 0.));
    }

    /// A tilted axis: the part of the velocity along it is the one that stays.
    #[test]
    fn the_vertical_part_follows_the_areas_axis() {
        let area = TargetArea {
            axis: Vec3::X,
            center: Vec3::ZERO,
        };
        let v = steered(Vec3::new(5., -4., 0.), Vec3::new(0., 100., 0.), &area, 14.);
        assert_eq!(v, Vec3::new(5., -(4. + 0.1), 0.));
    }

    /// The axis is the matrix's second column, the centre its translation column: a quarter turn
    /// about Z stands the area's up axis along -X.
    #[test]
    fn the_area_matrix_gives_its_up_axis_and_its_centre() {
        let area = Area {
            shape: crate::water::Shape::CylinderBase,
            matrix: [0., -1., 0., 5., 1., 0., 0., 6., 0., 0., 1., 7.],
            scale: Vec3::ONE,
            priority: -1,
            ignore: false,
        };
        let target = TargetArea::of(&area);
        assert_eq!(target.axis, Vec3::NEG_X);
        assert_eq!(target.center, Vec3::new(5., 6., 7.));
    }

    /// `appear`'s ceiling: `NormalMaxSpeed` below it, `JumpMoveSpeedMax` above, else the speed.
    #[test]
    fn the_ceiling_is_the_speed_between_the_two_limits() {
        assert_eq!(speed_ceiling(0.), t::NORMAL_MAX);
        assert_eq!(speed_ceiling(t::NORMAL_MAX), t::NORMAL_MAX);
        assert_eq!(speed_ceiling(20.), 20.);
        assert_eq!(speed_ceiling(t::JUMP_MOVE_MAX), t::JUMP_MOVE_MAX);
        assert_eq!(speed_ceiling(45.), t::JUMP_MOVE_MAX);
    }
}
