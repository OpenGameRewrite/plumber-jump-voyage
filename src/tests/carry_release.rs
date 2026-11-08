//! The keeper's cancel-and-release (`PlayerCarryKeeper::startCancelAndRelease` 0x42e9b0, run by
//! the Release nerve `exeRelease` 0x42f28c) behind the actor's stand-up squat and forced slope slide.
use super::*;

pub(super) fn run() {
    the_stand_up_squat_cancels_the_hold_and_the_keeper_releases_in_the_same_frame();
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
/// and the upper-body carry animation is cleared. Until that update `isCarry` still holds.
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
    p.set_nerve_on_ground(&Orders::default(), &open);
    assert_eq!(p.mode, Move::SquatBrake);
    assert!(p.is_carry(), "the Release nerve has not run yet");
    assert!(p.carry.cancel.is_some());
    p.finish_carry_release(&open);
    assert!(!p.is_carry());
    assert!(p.carry.cancel.is_none());
    assert_eq!((p.carry.phase, p.carry.step), (0, 0));
    assert_eq!(p.carry.inhibition, 10);
    assert!(p.carry.upper.is_none());
    assert!(p.upper_cleared);
    let seed = p.carry.seed.as_ref().unwrap();
    assert!(
        seed.velocity.length() > 20.,
        "the seed flew off the hand: {:?}",
        seed.velocity
    );
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
