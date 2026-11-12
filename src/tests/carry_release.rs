//! The keeper's cancel-and-release (`PlayerCarryKeeper::startCancelAndRelease` 0x42e9b0, run by
//! the Release nerve `exeRelease` 0x42f28c) behind the actor's stand-up squat and forced slope slide.
use super::*;

pub(super) fn run() {
    the_stand_up_squat_cancels_the_hold_and_the_keeper_releases_in_the_same_frame();
    pending_release_skips_the_previous_carry_or_throw_update();
    a_forced_slope_slide_cancels_a_carry_before_the_slide_starts();
}

fn slab(center_y: f32, floor_code: &'static str, slope: f32) -> Solid {
    Solid {
        center: Vec3::new(0., center_y, 0.),
        half: Vec3::new(100., 0.5, 100.),
        slope,
        moving: false,
        floor_code,
    }
}

/// A seed lying beside the player's feet, old enough to be picked up.
fn seed_beside(p: &Player, course: &Course) -> carry::Seed {
    let feet = course
        .mesh
        .as_ref()
        .map_or(p.position_cm(), |mesh| mesh.to_game_vec(p.position_cm()));
    let mut seed = carry::Seed::new("seed".into(), feet + Vec3::X * 20.);
    seed.age = grow_seed::PICKUP_AGE;
    seed
}

/// A seed held under a ceiling too low to stand: `setNerveOnGround` (0x4219b0) sees the stand-up
/// judge fail, calls `startCancelAndRelease` (0x4219ec) and chooses the squat. The keeper's
/// Release nerve then runs in the same frame's update: the seed leaves with the throw of a
/// release (speeds 28 and 70, 0x292828), the hold is over, the pickup is inhibited for ten frames
/// and the upper-body carry animation is cleared. The pending Release already makes `isCarry`
/// false (getCurrentNerve 0x959780), and the seed responds at the send (0x42e9c4).
#[cfg_attr(test, test)]
fn the_stand_up_squat_cancels_the_hold_and_the_keeper_releases_in_the_same_frame() {
    let open = Course {
        solids: vec![slab(-0.5, "Ground", 0.)],
        ..default()
    };
    let mut p = Player::default();
    for _ in 0..3 {
        p.step(&Orders::default(), &open);
    }
    assert!(p.grounded);
    p.carry.seed = Some(seed_beside(&p, &open));
    p.carry.held = true;
    p.carry.phase = 2;
    p.carry.upper = Some(("CarryFront", 0.));
    p.v = Vec3::new(3., 0., 0.);
    // The ceiling check the stand-up judge reads (`PlayerJudgeEnableStandUp`, 0x459514).
    p.ceiling.enable_stand_up = false;
    assert!(!p.judge_enable_stand_up(), "the ceiling forbids standing");
    let expected = cancel_velocity(&p);
    p.set_nerve_on_ground(&Orders::default(), &open);
    assert_eq!(p.mode, Move::SquatBrake);
    assert!(!p.is_carry(), "the pending Release is already observable");
    assert!(p.carry.held, "the held sensor awaits exeRelease");
    assert!(p.carry.cancel.is_some());
    assert_eq!(p.carry.seed.as_ref().unwrap().velocity, expected);
    assert!(
        p.carry
            .seed
            .as_ref()
            .unwrap()
            .throw_diagnostic()
            .contains("seed_nerve=Throw/0")
    );
    assert_eq!(p.carry.upper, Some(("CarryFront", 0.)));
    assert_eq!(p.carry.inhibition, 0);
    assert!(!p.upper_cleared);
    p.finish_carry_release(&open);
    assert!(!p.is_carry());
    assert!(p.carry.cancel.is_none());
    assert_eq!((p.carry.phase, p.carry.step), (0, 0));
    assert_eq!(p.carry.inhibition, 10);
    assert!(p.carry.upper.is_none());
    assert!(p.upper_cleared);
    assert_eq!(p.carry.seed.as_ref().unwrap().velocity, expected);
}

/// Non-swing release: angle 70 (0x292850), speed 28 (0x292838), double normalization
/// (0x29a71c, setVelocityToDirection at 0x29a730), then horizontal inheritance (0x29a734..0x29a758).
fn cancel_velocity(p: &Player) -> Vec3 {
    let front = pose_math::try_normalize_or_zero(p.pose_axes().1.with_y(0.)).0;
    let direction = pose_math::turn_vec_to_vec_degree(front, Vec3::Y, 70.);
    let aimed = pose_math::try_normalize_or_zero(pose_math::try_normalize_or_zero(direction).0).0;
    aimed * 28. + p.units_per_frame().with_y(0.)
}

#[cfg_attr(test, test)]
fn pending_release_skips_the_previous_carry_or_throw_update() {
    let course = Course::default();
    for phase in [1, 2, 3] {
        let mut p = Player::default();
        p.carry.seed = Some(seed_beside(&p, &course));
        p.carry.held = true;
        p.carry.phase = phase;
        p.carry.upper = Some(("CarryFront", 7.));
        p.carry.release_due = phase == 3;
        p.carry.inhibition = 4;
        p.v = Vec3::new(3., 0., 2.);
        let expected = cancel_velocity(&p);
        p.start_cancel_and_release(None);
        assert!(!p.is_carry());
        assert_eq!(p.carry.seed.as_ref().unwrap().velocity, expected);
        p.v = Vec3::X * 12.;
        p.start_cancel_and_release(None);
        p.update_carry(
            &Orders {
                throw_pressed: true,
                ..default()
            },
            true,
        );
        assert_eq!((p.carry.phase, p.carry.step), (phase, 0));
        assert_eq!(p.carry.upper, Some(("CarryFront", 7.)));
        assert!(!p.sub.active, "the old Throw first step must not execute");
        assert_eq!(p.carry.inhibition, 3);
        p.finish_carry_release(&course);
        assert!(!p.carry.held);
        assert!(!p.carry.release_due);
        assert_eq!(p.carry.inhibition, 10);
        assert_eq!(p.carry.seed.as_ref().unwrap().velocity, expected);
        p.start_cancel_and_release(None);
        assert!(p.carry.cancel.is_none(), "no hold means no second cancel");
    }
}

/// The forced slope slide (+0x3f8) cancels and releases the carried object in every actor nerve
/// that asks it (the wait 0x4225c4, the run 0x423108, the squat 0x422b28 ...) before the slide is
/// chosen.
#[cfg_attr(test, test)]
fn a_forced_slope_slide_cancels_a_carry_before_the_slide_starts() {
    let open = Course {
        solids: vec![slab(-0.5, "Ground", 0.)],
        ..default()
    };
    let slide = Course {
        solids: vec![slab(-0.5, "Slide", 0.3)],
        ..default()
    };
    let mut p = Player::default();
    for _ in 0..3 {
        p.step(&Orders::default(), &open);
    }
    p.carry.seed = Some(seed_beside(&p, &open));
    p.carry.held = true;
    p.carry.phase = 2;
    p.carry.upper = Some(("CarryFront", 0.));
    for _ in 0..3 {
        p.step(&Orders::default(), &slide);
        if p.mode == Move::SlopeSlide {
            break;
        }
    }
    assert_eq!(p.mode, Move::SlopeSlide);
    assert!(!p.is_carry(), "the hold was cancelled in the same frame");
    assert_eq!(p.carry.inhibition, 10);
}
