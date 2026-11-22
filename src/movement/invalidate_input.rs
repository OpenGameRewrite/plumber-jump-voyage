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
//! judge's write silences the input for the update that follows. Fall owns a second judge
//! (state +0x48, constructed at 0x46af20): `appear` clears its target pointer (+0x70) and
//! `updateFallTargetArea` (0x46b0bc) latches the first area found until the state ends
//! (`fall_target_area.rs` also steers the velocity toward it).
//! `exeFall` updates that state first (0x425240 -> 0x46b3b4), then renews the input silence
//! after its water judge (0x425630..0x425654). Leaving the volume does not release the latch.
//! Landing or another earlier transition bypasses this gate; the last counter write still
//! silences the following input update. The order is [V]: input update 0x41dec8, base movement
//! 0x41e020 -> LiveActor 0x90d8b0 -> actor nerve -> child state -> actor gate.
//!
//! The same test of the state's bytes +0x6c and +0x70 guards the wall judge: with either set the
//! actor's `exeFall` skips `PlayerJudgeWallKeep` (+0x4f8, 0x42556c..0x425580), so a latched
//! Fall that touches a wall stays Fall and silent (`Player::judge_wall_keep`). The gate's
//! other effect is to return before the tail of `exeFall` (0x425658..0x4256d0): the 2D hovering
//! jump (`rs::isPlayer2D`, `HackCap::isEnableSpinAttack`, the actor's
//! `HackCapJudgePreInputHoveringJump` at +0x4b8, then the Jump nerve with kind 0x12). That judge
//! has no port at all, latched or not, so there is nothing to skip.
//!
//! [I] Unmodelled: byte +0x6c, the trigger-7 silence. `appear` copies `ECollisionTrigger` 7
//! into it (0x46afe8..0x46affc), and the only writer of trigger 7 is `initAfterPlacement`
//! (0x41dd10, the scene-start branch that finds no ground below); `sendCollisionMsg` clears the
//! triggers at its top (0x4274c0), after the first movement. The replay places the actor from a
//! recorded state instead of running that scene start, so a Fall that appears on the first
//! movement after placement does not get the whole-fall silence (and the wall-judge skip) that
//! +0x6c would give it. The silence a volume gives (+0x70) is modelled.
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
use super::fall_target_area::{FallTarget, TargetArea};
use super::{Course, Move, Player};
use crate::input::Orders;
use bevy::prelude::Vec3;

/// `PlayerInput` +0x94 / +0x98 and the judge's area answer for this step.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct InputInvalidation {
    /// `PlayerInput` +0x94: the update count the input stays silenced for.
    counter: i32,
    /// `PlayerJudgeInvalidateInputFall`'s area pointer is set: the actor's position lies in an
    /// `InvalidateInputFallArea` (the collider's ground contact is asked separately).
    pub(super) in_area: Option<TargetArea>,
    /// The actor's position in the stage's frame (centimetres) that answer was given at.
    pub(super) position: Vec3,
    /// The Fall state's +0x70 and +0x78: once found, the target survives leaving the volume,
    /// until Fall ends.
    pub(super) fall: FallTarget,
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
        let located = scene.mesh.as_ref().map(|mesh| {
            let position = self.stage_position(mesh);
            // `al::tryFindAreaObj` by name: any placement holding the point.
            let area = mesh.invalidate_input_fall.volume_at(position);
            (position, area.map(TargetArea::of))
        });
        let (position, area) = located.unwrap_or_default();
        self.input_invalidation.position = position;
        self.input_invalidation.in_area = area;
    }

    /// Actor Fall's gate follows the water judge, and is bypassed by earlier transitions.
    pub(super) fn renew_fall_input_invalidation(&mut self) {
        if !self.nerve.deferred && self.fall_target_latched() {
            self.input_invalidation.counter = 1;
        }
    }

    /// `PlayerStateFallHakoniwa::appear` clears +0x70 before its first area query (0x46b084).
    pub(super) fn clear_fall_input_target(&mut self) {
        self.input_invalidation.fall.area = None;
    }

    /// `rs::updateJudgeAndResult` of the actor's `PlayerJudgeInvalidateInputFall` (+0x430) and the
    /// two writes and the `setNerve(Fall)` every asking nerve makes when it holds. True when the
    /// actor chose the Fall nerve.
    pub(super) fn judge_invalidate_input_fall(&mut self) -> bool {
        if self.nerve.deferred
            || self.is_collided_ground()
            || self.input_invalidation.in_area.is_none()
        {
            return false;
        }
        self.input_invalidation.counter = 1;
        if self.mode == Move::WallSlide {
            self.leave_wall_slide();
        }
        self.spin_fall = false;
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
    /// Fall nerve, which owns a separate judge and latches its answer.
    pub(super) fn judge_invalidate_input_fall_in_air(&mut self) -> bool {
        (self.mode != Move::Fall || self.actor_name() == "SpinCap")
            && self.judge_invalidate_input_fall()
    }
}

#[cfg(test)]
mod tests {
    use super::super::wall_air::WallContact;
    use super::*;
    use bevy::prelude::{Vec2, default};
    use std::sync::Arc;

    /// The test volume as the Fall state latches it: standing up from the origin.
    fn volume() -> TargetArea {
        TargetArea {
            axis: Vec3::Y,
            center: Vec3::ZERO,
        }
    }

    /// A stage with no collision and one `InvalidateInputFallArea` (`AreaCylinder`, unit scale:
    /// a cylinder of 500 cm radius standing on the origin, 500 cm tall), and perhaps a pool
    /// (`WaterArea`, `AreaCubeTop`: its surface at 1000 cm, 2000 cm deep, 10000 cm wide).
    fn course_of(pool: bool) -> Course {
        let water = r#",{"name":"WaterArea","shape":"AreaCubeTop","priority":-1,
                "matrix":[1,0,0,0, 0,1,0,1000, 0,0,1,0],"scale":[10,2,10],"args":{},"links":[]}"#;
        let text = format!(
            r#"{{"attributes":[{{}}],"objects":[],"triangles":[],
            "areas":[{{"name":"InvalidateInputFallArea","shape":"AreaCylinder","priority":-1,
                "matrix":[1,0,0,0, 0,1,0,0, 0,0,1,0],"scale":[1,1,1],"args":{{}},"links":[]}}{}]}}"#,
            if pool { water } else { "" }
        );
        Course {
            mesh: Some(Arc::new(
                crate::geometry::TriangleMesh::from_json(&text, Vec3::ZERO).unwrap(),
            )),
            ..default()
        }
    }

    fn course_with_volume() -> Course {
        course_of(false)
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
        let holds = |position| mesh.invalidate_input_fall.volume_at(position).is_some();
        assert!(holds(Vec3::new(0., 200., 0.)));
        assert!(!holds(Vec3::new(0., 600., 0.)));
        assert!(!holds(Vec3::new(700., 200., 0.)));
    }

    /// The actor judge silences the next input update; Fall then renews it on every step.
    #[test]
    fn a_jump_in_the_volume_falls_and_the_input_stays_silent() {
        let course = course_with_volume();
        let mut p = airborne(Move::Jump, 2.);
        p.step(&Orders::default(), &course);
        assert_eq!(p.mode, Move::Fall);
        assert_eq!(p.input_invalidation.counter, 1);

        p.step(&pressed(), &course);
        assert_eq!(p.pre_jump, 0, "the jump press was not heard");
        assert_eq!(p.tilt, 0., "the stick was not heard");
        assert_eq!(p.input_invalidation.counter, 1);

        p.step(&pressed(), &course);
        assert_eq!(p.pre_jump, 0);
        assert_eq!(p.tilt, 0.);
        assert_eq!(p.input_invalidation.counter, 1);
    }

    /// The state's target is sticky (0x46b0d8); exiting the volume is not an exit from Fall.
    /// A landing bypasses the gate, leaving only the last write's single update of silence.
    #[test]
    fn a_fall_latches_the_volume_until_landing_then_releases_input() {
        let course = course_with_volume();
        let mut p = airborne(Move::Fall, 2.);
        let mut first = pressed();
        first.jump = false;
        p.step(&first, &course);
        assert!(p.tilt > 0., "first input update precedes the Fall gate");
        assert!(p.fall_target_latched());
        for _ in 0..3 {
            p.step(&pressed(), &course);
            assert_eq!(p.pre_jump, 0);
            assert_eq!(p.tilt, 0.);
        }
        p.pos.x = 7.;
        p.step(&pressed(), &course);
        assert!(p.input_invalidation.in_area.is_none());
        assert_eq!(p.pre_jump, 0);
        assert_eq!(p.input_invalidation.counter, 1);
        // Use the normal collision scene for the landing; do not manufacture a state exit.
        let floor = crate::replay::flat_floor();
        p.pos = Vec3::new(0., 0.01, 0.);
        p.v = Vec3::new(0., -10., 0.);
        for _ in 0..3 {
            p.step(&Orders::default(), &floor);
        }
        assert!(p.grounded);
        assert_ne!(p.actor_name(), "Fall");
        p.step(&pressed(), &floor);
        assert!(p.tilt > 0., "the landed actor hears the stick again");
    }

    /// `exeSpinCap` 0x423a60 calls the air helper for its Fall child. This must select the
    /// actor's Fall, not leave the player under the SpinCap/Fall name.
    #[test]
    fn the_spin_cap_fall_asks_the_actor_judge_and_enters_plain_fall() {
        let mut p = airborne(Move::Fall, 2.);
        p.spin_fall = true;
        p.input_invalidation.in_area = Some(volume());
        assert_eq!(p.actor_name(), "SpinCap");
        assert!(p.judge_invalidate_input_fall_in_air());
        assert_eq!(p.actor_name(), "Fall");
        assert_eq!(p.input_invalidation.counter, 1);
    }

    /// The gate itself tests the retained pointer, not ground contact (0x425630).
    /// Landing normally leaves Fall earlier; a contact without a transition cannot clear it.
    #[test]
    fn the_fall_gate_uses_the_latched_target_until_an_earlier_transition() {
        let mut p = airborne(Move::Fall, 2.);
        p.input_invalidation.fall.area = Some(volume());
        p.grounded = true;
        p.renew_fall_input_invalidation();
        assert_eq!(p.input_invalidation.counter, 1);
        p.go(Move::Idle, "Wait");
        p.input_invalidation.counter = 0;
        p.renew_fall_input_invalidation();
        assert_eq!(p.input_invalidation.counter, 0);
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

    /// A falling player pressed against a face he looks at: `PlayerJudgeWallKeep` holds for him
    /// (the control case), and `exeFall` asks it only while the state holds no target area.
    fn against_a_wall(latched: bool) -> Player {
        let mut p = Player {
            mode: Move::Fall,
            pos: Vec3::new(0., 300., 0.),
            v: Vec3::new(0., -5., 0.),
            facing: Vec3::NEG_Z,
            ground_height: 1000.,
            grounded: false,
            ..default()
        };
        p.touched.wall_contact = Some(WallContact {
            normal: Vec3::Z,
            position: Vec3::ZERO,
            at_face: true,
        });
        p.input_invalidation.fall.area = latched.then(volume);
        p
    }

    /// 0x42556c..0x425580: with the state's target set the wall judge is not asked, so the fall
    /// stays Fall and the gate that follows keeps the input silent (0x425630..0x425654).
    #[test]
    fn a_latched_fall_against_a_wall_keeps_fall_and_the_silence() {
        let scene = Course::default();
        let mut free = against_a_wall(false);
        free.judge_wall_keep(&scene);
        free.renew_fall_input_invalidation();
        assert_eq!(
            free.mode,
            Move::WallSlide,
            "the judge holds without a target"
        );
        assert!(free.nerve.deferred);
        assert_eq!(free.input_invalidation.counter, 0);

        let mut latched = against_a_wall(true);
        latched.judge_wall_keep(&scene);
        latched.renew_fall_input_invalidation();
        assert_eq!(latched.mode, Move::Fall);
        assert!(!latched.nerve.deferred);
        assert_eq!(latched.input_invalidation.counter, 1);
    }

    /// The skip belongs to the plain Fall nerve: the spin-cap's fall child has no such state.
    #[test]
    fn the_spin_cap_fall_still_asks_the_wall_judge() {
        let mut p = against_a_wall(true);
        p.spin_fall = true;
        assert_eq!(p.actor_name(), "SpinCap");
        p.judge_wall_keep(&Course::default());
        assert_eq!(p.mode, Move::WallSlide);
    }

    /// A ground-free Fall outside every volume latches nothing and hears the player.
    #[test]
    fn a_fall_outside_the_volume_does_not_latch() {
        let course = course_with_volume();
        let mut p = airborne(Move::Fall, 6.);
        for _ in 0..2 {
            p.step(&pressed(), &course);
            assert!(!p.fall_target_latched());
            assert_eq!(p.input_invalidation.counter, 0);
        }
        assert!(p.tilt > 0., "the stick is heard outside the volume");

        // Inside the volume but touching the ground: the state's judge refuses as well.
        let mut grounded = airborne(Move::Fall, 2.);
        grounded.grounded = true;
        grounded.locate_invalidate_input_area(&course);
        grounded.update_fall_target_area();
        assert!(!grounded.fall_target_latched());
    }

    /// `appear` (0x46b084) forgets the area of an earlier fall: the new one asks again, here at
    /// a position outside the volume.
    #[test]
    fn a_new_fall_forgets_the_old_latch() {
        let course = course_with_volume();
        let mut p = airborne(Move::Fall, 6.);
        p.input_invalidation.fall.area = Some(volume());
        p.locate_invalidate_input_area(&course);
        p.go(Move::Fall, "Fall");
        assert!(!p.fall_target_latched());
    }

    /// `appear` asks the area in the step that chose Fall, so the latch stands even when the
    /// first step of the state begins outside the volume.
    #[test]
    fn a_fall_entered_in_the_volume_latches_at_entry() {
        let course = course_with_volume();
        let mut p = airborne(Move::Jump, 2.);
        p.locate_invalidate_input_area(&course);
        p.go(Move::Fall, "Fall");
        assert!(p.fall_target_latched(), "latched by appear");
        p.pos.x = 7.;
        // The first update still hears the pad: the gate renews the silence after it.
        let mut first = pressed();
        first.jump = false;
        p.step(&first, &course);
        assert!(p.input_invalidation.in_area.is_none());
        assert!(p.fall_target_latched());
        assert_eq!(p.input_invalidation.counter, 1);
        p.step(&pressed(), &course);
        assert_eq!(p.pre_jump, 0);
        assert_eq!(p.tilt, 0.);
    }

    /// `exeFall` returns on the water branch (0x425600) before the gate (0x425630): a latched
    /// fall that enters the water in a step is not silenced for the swim's first update.
    #[test]
    fn a_latched_fall_entering_water_is_not_silenced() {
        let course = course_of(true);
        let mut p = airborne(Move::Fall, 2.);
        p.input_invalidation.fall.area = Some(volume());
        p.step(&pressed(), &course);
        assert_eq!(p.mode, Move::Swim);
        assert_eq!(p.input_invalidation.counter, 0);
    }

    /// The latched area steers every call, in the volume or not; a state without one does not.
    #[test]
    fn a_latched_fall_is_steered_toward_the_axis_outside_the_volume_too() {
        let course = course_with_volume();
        let mut p = airborne(Move::Fall, 2.);
        p.pos.x = 7.;
        p.v = Vec3::new(-4., -3., 0.);
        p.locate_invalidate_input_area(&course);
        assert!(p.input_invalidation.in_area.is_none());
        p.update_fall_target_area();
        assert_eq!(p.v, Vec3::new(-4., -3., 0.), "no area, no steering");

        p.input_invalidation.fall = FallTarget {
            area: Some(volume()),
            max_speed: 14.,
        };
        p.update_fall_target_area();
        assert_eq!(p.v, Vec3::new(-(4. + 0.1), -3., 0.));
    }

    /// `appear` stores the ceiling and steers once; the pull cannot take the speed past it.
    #[test]
    fn appear_steers_at_once_with_the_ceiling_it_stores() {
        let course = course_with_volume();
        let mut p = airborne(Move::Jump, 2.);
        p.pos.x = 1.;
        p.v = Vec3::new(-10., -3., 0.);
        p.locate_invalidate_input_area(&course);
        p.go(Move::Fall, "Fall");
        assert_eq!(p.input_invalidation.fall.max_speed, 14.);
        assert_eq!(p.v, Vec3::new(-(10. + 0.1), -3., 0.));

        // Faster than the ceiling's floor: the ceiling is the speed itself.
        let mut fast = airborne(Move::Jump, 2.);
        fast.pos.x = 1.;
        fast.v = Vec3::new(-16., -3., 0.);
        fast.locate_invalidate_input_area(&course);
        fast.go(Move::Fall, "Fall");
        assert_eq!(fast.input_invalidation.fall.max_speed, 16.);
        assert_eq!(fast.v.x, -16.);
    }

    /// The steering runs in the landing step too (the child of `exeFall` updates first, 0x425240),
    /// before the air update the landing reads.
    #[test]
    fn a_latched_fall_is_steered_on_its_landing_step() {
        let course = course_with_volume();
        let land = |latched: bool| {
            let mut p = airborne(Move::Fall, 2.);
            p.pos.x = 7.;
            p.v = Vec3::new(-4., -1., 0.);
            p.grounded = true;
            p.input_invalidation.fall = FallTarget {
                area: latched.then(volume),
                max_speed: 14.,
            };
            p.air_for = Some(Move::Fall);
            p.locate_invalidate_input_area(&course);
            p.land(&Orders::default(), &course);
            p.v
        };
        assert!(
            land(true).x < land(false).x,
            "{:?} {:?}",
            land(true),
            land(false)
        );
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
        grounded.input_invalidation.in_area = Some(volume());
        assert!(!grounded.judge_invalidate_input_fall());
        assert_eq!(grounded.input_invalidation.counter, 0);
    }

    /// The slide and the wall slide ask last, the plain Fall nerve never asks, the air chain asks
    /// for every other air nerve and for the spin-cap's fall.
    #[test]
    fn the_nerves_that_ask_and_the_one_that_does_not() {
        for mode in [Move::SlopeSlide, Move::WallSlide] {
            let mut p = airborne(mode, 2.);
            p.input_invalidation.in_area = Some(volume());
            p.judge_invalidate_input_fall_after_sweeps();
            assert_eq!(p.mode, Move::Fall, "{mode:?}");
        }
        let mut idle = airborne(Move::Idle, 2.);
        idle.input_invalidation.in_area = Some(volume());
        idle.judge_invalidate_input_fall_after_sweeps();
        assert_eq!(idle.mode, Move::Idle);

        let mut fall = airborne(Move::Fall, 2.);
        fall.input_invalidation.in_area = Some(volume());
        assert!(!fall.judge_invalidate_input_fall_in_air());
        assert_eq!(fall.input_invalidation.counter, 0);

        for mode in [
            Move::LongJump,
            Move::Backflip,
            Move::WallJump,
            Move::SpinJump,
        ] {
            let mut p = airborne(mode, 2.);
            p.input_invalidation.in_area = Some(volume());
            assert!(p.judge_invalidate_input_fall_in_air(), "{mode:?}");
            assert_eq!(p.mode, Move::Fall);
        }
    }
}
