//! `PlayerJudgeInvalidateInputFall` and the input invalidation it starts.
//!
//! A stage places `InvalidateInputFallArea` volumes (`ProjectAreaFactory`: a plain `al::AreaObj`).
//! The actor's judge (+0x430, built by `initPlayer` 0x41c098 from the area checker) asks for the
//! volume holding the actor's position, but only while the collider holds no ground
//! (`PlayerJudgeInvalidateInputFall::update` 0x459c14: `rs::isCollidedGround`, then
//! `PlayerAreaChecker::tryFindInvalidateInputFall` 0x42d670 = `al::tryFindAreaObj` by name; the
//! judge (0x459c58) answers whether it found one). When it does, the asking nerve
//! (a) writes `PlayerInput` +0x94 = 1 and +0x98 = 1 (a word and a halfword store, e.g. 0x4236e4/0x4236e8)
//! and (b) selects the actor's Fall nerve (table base 0x1d78910).
//!
//! What the two fields are, from `PlayerInput::update` 0x44aacc, which runs at the start of the
//! actor's `movement` (0x41dec8): +0x98 (the "disabled" byte) becomes `counter > 0`, then the
//! counter at +0x94 converges one step toward zero (`al::converge(counter, 0, 1)`). Nearly every
//! `PlayerInput` query (255 functions; the stick getters, every trigger and hold) starts by
//! reading +0x98 and answers zero/false when it is set (0x44c3bc, 0x44c6ec, 0x44c728 ...). So the
//! judge's write silences the input for the update that follows, i.e. the Fall nerve's first
//! step, and the input is back on the step after that. The Fall nerve itself never asks this
//! judge (`exeFall` 0x425228 has no read of +0x430 and does not call `tryChangeNerveFromAir`).
//!
//! The nerves that ask it (every read of the actor field +0x430 in `PlayerActorHakoniwa`):
//!
//! | nerve | where | position in the nerve's judges |
//! |---|---|---|
//! | `exeSlope` 0x4235b0 | 0x4236c8 | last, after the water entry |
//! | `exeRolling` 0x423708 | 0x4237dc | right after the forced slope slide, before the sand sink |
//! | `exeJump` 0x42405c | 0x424144 | first, before the air cap throw (state not ended) |
//! | `exeWallAir` 0x424570 | 0x42477c | last of the slide judges; the air phase uses the helper |
//! | `exeEndHack` 0x426a18 | 0x426a5c | first after `updateNerveState` (no capture in the port) |
//! | `exeBind` 0x426c70 | 0x426dac | after the forced slope slide, in the air-end branch |
//! | `tryChangeNerveFromAir` 0x423d94 | 0x423dec | right after the forced slope slide; asked by `exeSpinCap` 0x423a60, `exeWallAir` 0x4245cc, `exeGrabCeil` 0x424af4, `exePoleClimb` 0x424b68 and `exeLongJump` 0x425214 |
//!
//! The area query reads the actor's position and the collider's ground contact, neither of which
//! changes while a nerve runs, so [`Player::locate_invalidate_input_area`] answers it once a step
//! and every asking nerve reads that answer.
//!
//! [I] The stage switch of a `SwitchKill` link (one placement in the local exports) is not
//! simulated, like the other areas' switches (`water.rs`): the volume counts as alive.
//! [I] Inside the step that asks it, the executable sets +0x98 at once; nothing in the nerves
//! reads the input after the `setNerve` that follows, so only the next update's silence is
//! modelled. [I] `exeWallAir`'s first air step asks the slide chain (judge last) in the
//! executable; the port's wall jump asks it first through the air helper.
use super::{Course, Move, Player};
use crate::input::Orders;
use crate::water::WaterAreas;
use bevy::prelude::Vec3;

/// `PlayerInput` +0x94 / +0x98 and the judge's area answer for this step.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct InputInvalidation {
    /// `PlayerInput` +0x94: the update count the input stays silenced for.
    counter: i32,
    /// `PlayerJudgeInvalidateInputFall`'s area pointer is set: the actor's position lies in an
    /// `InvalidateInputFallArea` (the collider's ground contact is asked separately).
    in_area: bool,
}

/// `al::tryFindAreaObj` by name: any placement holding the point (the answer is only used as
/// found / not found).
pub(super) fn holds(areas: &WaterAreas, position: Vec3) -> bool {
    areas.volume_at(position).is_some()
}

impl Player {
    /// `PlayerInput::update` 0x44aacc, the part that concerns the invalidation: the disabled byte
    /// follows the counter (`counter > 0`), then the counter converges one step toward zero.
    /// While disabled every query of the input answers zero, which is what the orders become.
    pub(super) fn update_input_invalidation(&mut self, orders: &mut Orders) {
        let counter = &mut self.input_invalidation.counter;
        let disabled = *counter > 0;
        *counter -= counter.signum();
        if disabled {
            orders.silence_player_input();
        }
    }

    /// The area half of `PlayerJudgeInvalidateInputFall::update`, answered at the position the
    /// nerves of this step run from.
    pub(super) fn locate_invalidate_input_area(&mut self, scene: &Course) {
        self.input_invalidation.in_area = scene
            .mesh
            .as_ref()
            .is_some_and(|mesh| holds(&mesh.invalidate_input_fall, self.stage_position(mesh)));
    }

    /// `rs::updateJudgeAndResult` of the actor's `PlayerJudgeInvalidateInputFall` (+0x430) and the
    /// two writes and the `setNerve(Fall)` every asking nerve makes when it holds. True when the
    /// actor chose the Fall nerve.
    pub(super) fn judge_invalidate_input_fall(&mut self) -> bool {
        if self.nerve.deferred || self.is_collided_ground() || !self.input_invalidation.in_area {
            return false;
        }
        self.input_invalidation.counter = 1;
        if self.mode == Move::WallSlide {
            self.leave_wall_slide();
        }
        self.go(Move::Fall, "Fall");
        true
    }

    /// The judge as the nerves that ask it last (`exeSlope`, the slide judges of `exeWallAir`)
    /// and the ones that ask it after a decision chain the port runs as sweeps.
    pub(super) fn judge_invalidate_input_fall_after_sweeps(&mut self) {
        if matches!(self.mode, Move::SlopeSlide | Move::WallSlide) {
            self.judge_invalidate_input_fall();
        }
    }

    /// The air chain shared by the jump, long jump, wall jump, grab-ceil, pole and spin-cap-fall
    /// nerves (`exeJump` 0x424144, `tryChangeNerveFromAir` 0x423dec): everything but the plain
    /// Fall nerve, which never asks it.
    pub(super) fn judge_invalidate_input_fall_in_air(&mut self) -> bool {
        (self.mode != Move::Fall || self.actor_name() == "SpinCap")
            && self.judge_invalidate_input_fall()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{Vec2, default};
    use std::sync::Arc;

    /// A stage with no collision and one `InvalidateInputFallArea` (`AreaCylinder`, unit scale:
    /// a cylinder of 500 cm radius standing on the origin, 500 cm tall).
    fn course_with_volume() -> Course {
        let text = r#"{"attributes":[{}],"objects":[],"triangles":[],
            "areas":[{"name":"InvalidateInputFallArea","shape":"AreaCylinder","priority":-1,
                "matrix":[1,0,0,0, 0,1,0,0, 0,0,1,0],"scale":[1,1,1],"args":{},"links":[]}]}"#;
        Course {
            mesh: Some(Arc::new(
                crate::geometry::TriangleMesh::from_json(text, Vec3::ZERO).unwrap(),
            )),
            ..default()
        }
    }

    fn airborne(mode: Move, height_m: f32) -> Player {
        Player {
            mode,
            pos: Vec3::new(0., height_m, 0.),
            grounded: false,
            hop_contact: false,
            v: Vec3::new(0., 1., 0.),
            ..default()
        }
    }

    fn pressed() -> Orders {
        Orders {
            direction: Vec2::new(0., -1.),
            stick: Vec2::new(0., -1.),
            jump: true,
            jump_held: true,
            ..default()
        }
    }

    /// The judge's volume is read from the export's `InvalidateInputFallArea` entries.
    #[test]
    fn the_export_loads_the_volume() {
        let mesh = course_with_volume().mesh.unwrap();
        assert!(holds(&mesh.invalidate_input_fall, Vec3::new(0., 200., 0.)));
        assert!(!holds(&mesh.invalidate_input_fall, Vec3::new(0., 600., 0.)));
        assert!(!holds(
            &mesh.invalidate_input_fall,
            Vec3::new(700., 200., 0.)
        ));
    }

    /// A jump without ground in the volume selects Fall (0x424144..0x4242a4) and silences the
    /// input for one update (`PlayerInput::update`: disabled while the counter is positive);
    /// the update after it hears the pad again.
    #[test]
    fn a_jump_in_the_volume_falls_and_the_input_is_silent_for_one_update() {
        let course = course_with_volume();
        let mut p = airborne(Move::Jump, 2.);
        p.step(&Orders::default(), &course);
        assert_eq!(p.mode, Move::Fall);
        assert_eq!(p.input_invalidation.counter, 1);

        p.step(&pressed(), &course);
        assert_eq!(p.pre_jump, 0, "the jump press was not heard");
        assert_eq!(p.tilt, 0., "the stick was not heard");
        assert_eq!(p.input_invalidation.counter, 0);

        p.step(&pressed(), &course);
        assert!(p.pre_jump > 0, "the input is back after one update");
        assert!(p.tilt > 0.);
    }

    /// The slide asks last, after its own step and the sweeps, and the roll right after its step.
    #[test]
    fn the_slide_and_the_roll_fall_from_the_volume_in_their_own_steps() {
        let course = course_with_volume();
        for mode in [Move::SlopeSlide, Move::Roll] {
            let mut p = airborne(mode, 2.);
            p.step(&Orders::default(), &course);
            assert_eq!(p.mode, Move::Fall, "{mode:?}");
            assert_eq!(p.input_invalidation.counter, 1, "{mode:?}");
        }
    }

    /// Outside the volume, or standing on ground, nothing happens.
    #[test]
    fn the_judge_needs_the_volume_and_no_ground() {
        let course = course_with_volume();
        let mut outside = airborne(Move::Jump, 6.);
        outside.step(&Orders::default(), &course);
        assert_eq!(outside.mode, Move::Jump);
        assert_eq!(outside.input_invalidation.counter, 0);

        let mut grounded = airborne(Move::Jump, 2.);
        grounded.grounded = true;
        grounded.input_invalidation.in_area = true;
        assert!(!grounded.judge_invalidate_input_fall());
        assert_eq!(grounded.input_invalidation.counter, 0);
    }

    /// The slide and the wall slide ask last, the plain Fall nerve never asks, the air chain asks
    /// for every other air nerve and for the spin-cap's fall.
    #[test]
    fn the_nerves_that_ask_and_the_one_that_does_not() {
        for mode in [Move::SlopeSlide, Move::WallSlide] {
            let mut p = airborne(mode, 2.);
            p.input_invalidation.in_area = true;
            p.judge_invalidate_input_fall_after_sweeps();
            assert_eq!(p.mode, Move::Fall, "{mode:?}");
        }
        let mut idle = airborne(Move::Idle, 2.);
        idle.input_invalidation.in_area = true;
        idle.judge_invalidate_input_fall_after_sweeps();
        assert_eq!(idle.mode, Move::Idle);

        let mut fall = airborne(Move::Fall, 2.);
        fall.input_invalidation.in_area = true;
        assert!(!fall.judge_invalidate_input_fall_in_air());
        assert_eq!(fall.input_invalidation.counter, 0);

        for mode in [
            Move::LongJump,
            Move::Backflip,
            Move::WallJump,
            Move::SpinJump,
        ] {
            let mut p = airborne(mode, 2.);
            p.input_invalidation.in_area = true;
            assert!(p.judge_invalidate_input_fall_in_air(), "{mode:?}");
            assert_eq!(p.mode, Move::Fall);
        }
    }
}
