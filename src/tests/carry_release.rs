//! The keeper's cancel-and-release (`PlayerCarryKeeper::startCancelAndRelease` 0x42e9b0, run by
//! the Release nerve `exeRelease` 0x42f28c) behind the actor's stand-up squat and forced slope slide.
use super::*;

pub(super) fn run() {
    the_stand_up_squat_cancels_the_hold_and_the_keeper_releases_in_the_same_frame();
    pending_release_skips_the_previous_carry_or_throw_update();
    a_new_low_ceiling_cancels_a_carrying_wait();
    a_new_low_ceiling_cancels_a_carrying_run();
    a_new_low_ceiling_cancels_a_carrying_skid();
    a_new_low_ceiling_cancels_a_carrying_turn();
    a_new_low_ceiling_preempts_a_simultaneous_carry_throw();
    a_new_low_ceiling_preempts_an_old_throw_first_step();
    a_new_low_ceiling_preempts_an_old_throw_release_step();
    an_ordinary_throw_reads_the_post_state_pose_and_velocity();
    a_fresh_jump_slide_landing_releases_after_one_final_air_update();
    a_fresh_fall_slide_landing_releases_after_one_final_air_update();
    a_fresh_jump_mesh_slide_landing_uses_air_velocity_without_ground_pull();
    a_fresh_fall_mesh_slide_landing_uses_air_velocity_without_ground_pull();
    a_fresh_jump_mesh_slide_without_carry_uses_the_air_child_appear_input();
    a_fresh_fall_mesh_slide_without_carry_uses_the_air_child_appear_input();
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
    seed_velocity(p.pose_axes().1, p.units_per_frame())
}

fn seed_velocity(front: Vec3, inherited: Vec3) -> Vec3 {
    let front = pose_math::try_normalize_or_zero(front.with_y(0.)).0;
    let direction = pose_math::turn_vec_to_vec_degree(front, Vec3::Y, 70.);
    let aimed = pose_math::try_normalize_or_zero(pose_math::try_normalize_or_zero(direction).0).0;
    aimed * 28. + inherited.with_y(0.)
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

/// EnableStandUp failure in continuing exeWait (0x422634..0x4226b0) and exeRun
/// (0x423164..0x4231e0), including Run's Brake/Turn children. Bring the ceiling in only after
/// an open frame has executed the carrying child. No setNerveOnGround call drives the test.
fn low_ceiling_cancels(mode: Move, throw_phase: Option<u32>, throw_pressed: bool) {
    let open = Course {
        solids: vec![slab(-0.5, "Ground", 0.)],
        ..default()
    };
    let low = Course {
        solids: vec![slab(-0.5, "Ground", 0.), slab(1.9, "Ground", 0.)],
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
    p.v = p.facing * 12.;
    let mut order = Orders::default();
    match mode {
        Move::Run => {
            p.go(Move::Run, "Move");
            p.enter_nerve(RunNerve::Run);
            order.direction = Vec2::new(p.facing.x, p.facing.z);
        }
        Move::Skid => {
            p.go(Move::Skid, "Brake");
            p.enter_nerve(RunNerve::Brake);
        }
        Move::Turn => {
            p.go(Move::Turn, "Turn");
            p.enter_nerve(RunNerve::Turn);
        }
        _ => {}
    }
    p.step(&order, &open);
    assert_eq!(p.mode, mode);
    assert!(p.is_carry());
    assert!(p.judge_enable_stand_up());
    // Keep momentum in Wait too, so a mistaken setNerveOnGround velocity cut is observable.
    if mode == Move::Idle {
        p.v = p.facing * 3.;
    }
    if let Some(step) = throw_phase {
        p.carry.phase = 3;
        p.carry.step = step;
        p.carry.release_due = step > 0;
    }
    let expected = cancel_velocity(&p);
    order.jump = true;
    order.throw_pressed = throw_pressed;
    p.step(&order, &low);
    assert!(!p.ceiling.enable_stand_up);
    assert_eq!(p.mode, Move::SquatBrake, "{mode:?}: ceiling precedes jump");
    assert!(!p.is_carry());
    assert!(!p.carry.held);
    assert_eq!(p.carry.inhibition, 10);
    assert!(p.carry.upper.is_none());
    assert!(p.upper_cleared);
    assert!(!p.events.contains(&Fx::CarryThrow));
    assert!(
        p.model_sub().is_none(),
        "the old keeper must not start a throw"
    );
    assert_eq!((p.carry.phase, p.carry.step), (0, 0));
    assert!(!p.carry.release_due);
    assert!(p.carry.cancel.is_none());
    if mode == Move::Idle {
        assert_eq!(p.carry.seed.as_ref().unwrap().velocity, expected);
    }
    assert!(
        p.v.with_y(0.).length_squared() > 0.,
        "chain squat keeps tangent velocity"
    );
    assert!(
        p.carry
            .seed
            .as_ref()
            .unwrap()
            .throw_diagnostic()
            .contains("seed_nerve=Throw/0")
    );
}

#[cfg_attr(test, test)]
fn a_new_low_ceiling_cancels_a_carrying_wait() {
    low_ceiling_cancels(Move::Idle, None, false);
}

#[cfg_attr(test, test)]
fn a_new_low_ceiling_cancels_a_carrying_run() {
    low_ceiling_cancels(Move::Run, None, false);
}

#[cfg_attr(test, test)]
fn a_new_low_ceiling_cancels_a_carrying_skid() {
    low_ceiling_cancels(Move::Skid, None, false);
}

#[cfg_attr(test, test)]
fn a_new_low_ceiling_cancels_a_carrying_turn() {
    low_ceiling_cancels(Move::Turn, None, false);
}

#[cfg_attr(test, test)]
fn a_new_low_ceiling_preempts_a_simultaneous_carry_throw() {
    low_ceiling_cancels(Move::Idle, None, true);
}

#[cfg_attr(test, test)]
fn a_new_low_ceiling_preempts_an_old_throw_first_step() {
    low_ceiling_cancels(Move::Idle, Some(0), true);
}

#[cfg_attr(test, test)]
fn a_new_low_ceiling_preempts_an_old_throw_release_step() {
    low_ceiling_cancels(Move::Idle, Some(5), true);
}

/// updateCarry (0x420e78..0x420ed8) uses the actor's completed movement and pose;
/// the seed inherits that velocity at 0x29a734..0x29a758.
/// exeThrow starts the sub at 0x42f194 with skeleton frame 0/rate 1 (0x8001f4..0x800210);
/// the one-shot policy only clamps (0xb1737c..0xb1738c). Release remains keeper step 5
/// (0x42f1b4..0x42f1c8), independently of the sub's lifetime.
#[cfg_attr(test, test)]
fn an_ordinary_throw_reads_the_post_state_pose_and_velocity() {
    let course = Course {
        solids: vec![slab(-0.5, "Ground", 0.)],
        ..default()
    };
    let mut p = Player::default();
    for _ in 0..3 {
        p.step(&Orders::default(), &course);
    }
    p.carry.seed = Some(seed_beside(&p, &course));
    p.carry.held = true;
    p.carry.phase = 2;
    p.v = Vec3::X * 12.;
    let order = Orders {
        direction: Vec2::Y,
        throw_pressed: true,
        ..default()
    };
    p.step(&order, &course);
    assert!(p.events.contains(&Fx::CarryThrow));
    assert_eq!(p.carry.phase, 3);
    assert_eq!(p.pose_axes().1, Vec3::Z);
    assert_eq!(p.model_sub(), Some(("CarryFrontThrow", 0.)));
    let continuing = Orders {
        throw_pressed: false,
        ..order
    };
    for frame in 1..5 {
        p.step(&continuing, &course);
        assert_eq!(p.model_sub(), Some(("CarryFrontThrow", frame as f32)));
        assert_eq!(p.carry.step, frame + 1);
        assert!(p.is_carry());
    }
    let before = cancel_velocity(&p);
    assert_eq!(p.carry.step, 5);
    p.step(&continuing, &course);
    assert_eq!(p.model_sub(), Some(("CarryFrontThrow", 5.)));
    assert!(!p.is_carry());
    assert_eq!(p.carry.seed.as_ref().unwrap().velocity, cancel_velocity(&p));
    assert_ne!(p.carry.seed.as_ref().unwrap().velocity, before);
    for frame in 6..=24 {
        p.step(&continuing, &course);
        assert_eq!(p.model_sub(), Some(("CarryFrontThrow", frame as f32)));
        assert!(p.ended_model_sub().is_none());
        assert!(p.look.keeper_cell().starts_with("mode=0;"));
        assert!(
            !p.events
                .iter()
                .any(|event| matches!(event, Fx::FootLift { .. } | Fx::Step { .. }))
        );
    }
    assert_eq!((p.carry.phase, p.carry.step), (0, 0));
    p.step(&continuing, &course);
    assert!(p.model_sub().is_none());
    assert_eq!(p.ended_model_sub(), Some(("CarryFrontThrow", 24.)));
    assert!(p.look.keeper_cell().starts_with("mode=1;"));
    assert!(
        p.events
            .iter()
            .any(|event| matches!(event, Fx::FootLift { .. } | Fx::Step { .. }))
    );
    p.step(&continuing, &course);
    assert!(p.model_sub().is_none());
    assert!(p.ended_model_sub().is_none());
}

/// exeJump updates its child at 0x4240a4 before cancel 0x4241c4; exeFall at 0x425240
/// before 0x42532c. Air updates 0x4729c4 / 0x46b4a0, Jump landing limit 0x472a1c,
/// and seed horizontal inheritance 0x29a734..0x29a758 determine the exact expected velocity.
fn fresh_slide_landing(mode: Move, real_mesh: bool, held: bool) {
    let mut course = Course {
        solids: vec![slab(-0.5, "Slide", 0.3)],
        ..default()
    };
    if real_mesh {
        // Synthetic plane y = -0.3x, through the actual triangle collider.
        let a = Vec3::new(-1000., 300., -1000.);
        let b = Vec3::new(-1000., 300., 1000.);
        let c = Vec3::new(1000., -300., -1000.);
        let d = Vec3::new(1000., -300., 1000.);
        course.mesh = Some(Arc::new(crate::geometry::TriangleMesh::from_coded_corners(
            &[([a, b, c], 0), ([c, b, d], 0)],
            vec![crate::geometry::CollisionCodes {
                floor: "Slide".into(),
                ..default()
            }],
        )));
        course.solids.clear();
    }
    let mut p = Player::default();
    p.go(mode, if mode == Move::Jump { "Jump" } else { "Fall" });
    p.pos = Vec3::Y * 0.2;
    p.grounded = false;
    p.ground_contact = 0;
    p.facing = Vec3::X;
    p.v = Vec3::new(24., -30., 0.);
    let takeoff = if mode == Move::Jump {
        jump_takeoff(0, p.v.y, p.constants.jump_gravity)
    } else {
        fall_takeoff(p.constants, p.v.y)
    };
    p.air.setup(&mut p.v, p.facing, &takeoff);
    p.air_for = Some(mode);
    if held {
        p.carry.seed = Some(seed_beside(&p, &course));
        p.carry.held = true;
        p.carry.phase = 2;
        p.carry.upper = Some(("CarryFront", 0.));
    }
    // The first frame collides; the next executes the fresh-touchdown child and actor chain.
    p.step(&Orders::default(), &course);
    assert_eq!(p.mode, mode);
    assert_eq!(p.is_carry(), held);
    assert!(p.is_just_land());
    assert!(p.nerve.touchdown);
    if real_mesh {
        assert_ne!(p.mesh_ground.unwrap().with_y(0.), Vec3::ZERO);
        assert!(p.ground_move.is_none());
        assert!(p.run_up.is_none());
    } else {
        assert!(p.mesh_ground.is_none());
    }

    let order = Orders {
        direction: Vec2::Y,
        ..default()
    };
    let push = Vec3::Z;
    let steer = air_move::Steering {
        front: t::JUMP_ACCEL_FRONT,
        back: t::JUMP_ACCEL_BACK,
        turn: t::JUMP_ACCEL_TURN,
    };
    let before = p.v;
    let mut air = p.air.clone();
    let mut velocity = before.with_y(p.touchdown_speed);
    air.update(&mut velocity, push, false, &steer);
    let before_limit = velocity;
    if mode == Move::Jump {
        air.limit_landing_speed(&mut velocity);
        assert_ne!(
            velocity.with_y(0.),
            before_limit.with_y(0.),
            "the Jump fixture exercises its landing limit"
        );
    }
    let front = air.turn_facing(p.facing, push);
    let expected = seed_velocity(front, velocity);
    let mut appear = Player {
        mode: Move::SlopeSlide,
        grounded: p.grounded,
        ground_contact: p.ground_contact,
        mesh_ground: p.mesh_ground,
        slope: p.slope,
        touched: p.touched.clone(),
        v: velocity,
        ..default()
    };
    appear.start_slope_slide();
    let expected_slide = appear.v;
    if real_mesh {
        let normal = p.mesh_ground.unwrap();
        let mut along = velocity.with_y(0.);
        along.y = -along.dot(normal) / normal.y;
        let pulled = along - normal * p.pull;
        assert_ne!(expected.with_y(0.), seed_velocity(front, pulled).with_y(0.));
    }
    assert_ne!(
        velocity.with_y(0.),
        before.with_y(0.),
        "steering changes horizontal velocity"
    );
    let mut twice = velocity;
    air.update(&mut twice, push, false, &steer);
    if mode == Move::Jump {
        air.limit_landing_speed(&mut twice);
    }
    assert_ne!(
        twice.with_y(0.),
        velocity.with_y(0.),
        "a duplicate child update is detectable"
    );

    p.step(&order, &course);
    assert_eq!(p.mode, Move::SlopeSlide);
    assert!(!p.is_carry());
    assert_eq!(p.v, expected_slide);
    if held {
        assert_eq!(p.carry.inhibition, 10);
        assert_eq!(p.carry.seed.as_ref().unwrap().velocity, expected);
    } else {
        assert!(p.carry.seed.is_none());
    }
    assert_ne!(
        p.units_per_frame().with_y(0.),
        velocity.with_y(0.),
        "slide appear has its own velocity, after the message"
    );
    assert_eq!(
        p.slide.steps, 0,
        "exeSlide must not run on the choosing step"
    );
}

#[cfg_attr(test, test)]
fn a_fresh_jump_slide_landing_releases_after_one_final_air_update() {
    fresh_slide_landing(Move::Jump, false, true);
}

#[cfg_attr(test, test)]
fn a_fresh_fall_slide_landing_releases_after_one_final_air_update() {
    fresh_slide_landing(Move::Fall, false, true);
}

#[cfg_attr(test, test)]
fn a_fresh_jump_mesh_slide_landing_uses_air_velocity_without_ground_pull() {
    fresh_slide_landing(Move::Jump, true, true);
}

#[cfg_attr(test, test)]
fn a_fresh_fall_mesh_slide_landing_uses_air_velocity_without_ground_pull() {
    fresh_slide_landing(Move::Fall, true, true);
}

#[cfg_attr(test, test)]
fn a_fresh_jump_mesh_slide_without_carry_uses_the_air_child_appear_input() {
    fresh_slide_landing(Move::Jump, true, false);
}

#[cfg_attr(test, test)]
fn a_fresh_fall_mesh_slide_without_carry_uses_the_air_child_appear_input() {
    fresh_slide_landing(Move::Fall, true, false);
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
