use super::*;
use crate::cap::{CapNerve, Launch, Moment};

pub(super) fn run() {
    the_touch_jump_is_the_catch_sphere_against_the_body_sensors();
    the_touch_jump_keeps_the_gates_of_the_receiver();
    level_ground_does_not_spend_the_cap_jump();
    the_pop_follows_its_judge();
    the_pop_requires_the_catch_nerve_and_byte();
    the_touch_sender_checks_the_air_spin_step_window();
    the_pop_is_its_own_state();
}

fn floor() -> Course {
    Course {
        solids: vec![Solid {
            center: Vec3::new(0., -0.5, 0.),
            half: Vec3::new(100., 0.5, 100.),
            slope: 0.,
            moving: false,
            floor_code: "Ground",
        }],
        ..default()
    }
}
fn held() -> Orders {
    Orders {
        throw_held: true,
        ..default()
    }
}
/// A cap hovering at `spot` (centimetres), as the recorded `cap-bounce` run has it.
fn hovering(spot: Vec3) -> Cap {
    let mut cap = Cap::start(
        Launch {
            front: Vec3::Z,
            kind: Throw::Forward,
            left: false,
            air: false,
            continuous: false,
            spiral_side: 0.,
            up: Vec3::Y,
        },
        Vec3::ZERO,
        1,
    );
    cap.resume(&Moment {
        nerve: CapNerve::ThrowStay,
        step: 30,
        spot,
        velocity: Vec3::ZERO,
        speed: 32.,
        return_speed: 0.,
        anchor: Vec3::ZERO,
        after_lock_on: false,
    });
    cap
}
/// A player standing on the floor with the cap hovering `offset` (centimetres) from his feet.
fn standing_with_cap(offset: Vec3) -> Player {
    let scene = floor();
    let mut p = Player::default();
    for _ in 0..5 {
        p.step(&Orders::default(), &scene);
    }
    assert!(p.is_collided_ground());
    p.cap = Some(hovering(p.pos / UNIT + offset));
    p.throw_held_before = true;
    p
}
fn touches(p: &mut Player) -> bool {
    p.step(&held(), &floor());
    p.mode == Move::CapBounce
}

/// The Body sensor (radius 50, 75 up) and the cap's Catch sphere (radius 100 at the cap's
/// centre) touch when the centres are closer than 150; the Head sensor (radius 40, 110 up)
/// when they are closer than 140. A standing player is not on the foot's trample geometry at all.
#[cfg_attr(test, test)]
fn the_touch_jump_is_the_catch_sphere_against_the_body_sensors() {
    // Body: 149 cm in front at the height of the body sensor.
    let mut p = standing_with_cap(Vec3::new(0., 75., 149.));
    assert!(touches(&mut p));
    assert_eq!(p.mode, Move::CapBounce);
    // The ground request (jump type 8): `JumpCapLeapFrog` with the air power and its gravity;
    // the jump nerve's first step ran in the same frame.
    assert_eq!(p.clip(), "JumpCapLeapFrog");
    let power = p.constants.cap_leap_frog_jump_power_air - p.constants.cap_leap_frog_jump_gravity;
    assert!((p.v.y - power).abs() < 1e-3, "{}", p.v.y);
    // The cap enters its nerve when it next updates.
    p.step(&held(), &floor());
    assert_eq!(p.cap.as_ref().unwrap().state_name(), "TrampleReturn");
    // 151 cm is out of reach of both sensors.
    let mut p = standing_with_cap(Vec3::new(0., 75., 151.));
    assert!(!touches(&mut p));
    // Head: 139 cm above the head sensor, out of the body's reach (164 cm).
    let mut p = standing_with_cap(Vec3::new(0., 110. + 139., 0.));
    assert!(touches(&mut p));
    let mut p = standing_with_cap(Vec3::new(0., 110. + 141., 0.));
    assert!(!touches(&mut p));
}

/// `HackCap::receiveMsg` for the touch jump: the throw button held, a cap jump still possible, no
/// wall jump stored in the history, the cap hovering.
#[cfg_attr(test, test)]
fn the_touch_jump_keeps_the_gates_of_the_receiver() {
    let spot = Vec3::new(0., 75., 100.);
    let mut p = standing_with_cap(spot);
    p.step(&Orders::default(), &floor());
    assert_eq!(p.mode, Move::Idle, "the button must be held");
    let mut p = standing_with_cap(spot);
    p.cap_jump_possible = false;
    p.step(&held(), &floor());
    assert_eq!(p.mode, Move::Idle, "one cap jump per time on the ground");
    let mut p = standing_with_cap(spot);
    p.wall_history.jump = Some((Vec3::ZERO, Vec3::Z));
    p.step(&held(), &floor());
    assert_eq!(p.mode, Move::Idle, "a stored wall jump refuses it");
    let mut p = standing_with_cap(spot);
    let at = p.pos / UNIT + spot;
    p.cap.as_mut().unwrap().resume(&Moment {
        nerve: CapNerve::Return,
        step: 3,
        spot: at,
        velocity: Vec3::ZERO,
        speed: 32.,
        return_speed: 0.,
        anchor: Vec3::ZERO,
        after_lock_on: false,
    });
    p.step(&held(), &floor());
    assert_ne!(p.mode, Move::CapBounce, "only a hovering cap takes it");
}

/// `attackSensor` 0x428050..0x428084: the cap-jump permission is spent unless the player stands
/// on ground flatter than `StandAngleMin`.
#[cfg_attr(test, test)]
fn level_ground_does_not_spend_the_cap_jump() {
    let mut p = standing_with_cap(Vec3::new(0., 75., 100.));
    p.step(&held(), &floor());
    assert_eq!(p.mode, Move::CapBounce);
    assert!(p.cap_jump_possible);
    // In the air it is spent.
    let mut p = Player {
        pos: Vec3::new(0., 3., 0.),
        grounded: false,
        mode: Move::Fall,
        v: Vec3::new(0., -1., 0.),
        ..default()
    };
    p.cap = Some(hovering(p.pos / UNIT + Vec3::new(0., 75., 100.)));
    p.throw_held_before = true;
    p.step(&held(), &floor());
    assert_eq!(p.mode, Move::CapBounce);
    assert!(!p.cap_jump_possible);
}

fn falling_after_a_catch() -> Player {
    let mut p = Player {
        pos: Vec3::new(0., 3., 0.),
        grounded: false,
        mode: Move::Jump,
        v: Vec3::new(0., 1., 0.),
        ..default()
    };
    p.catch_window = t::CAP_CATCH_WINDOW;
    p.cap_catched = true; // A previous PlayerCapCatch set +0x2a3; the cap is now in Hide.
    p
}
fn jump_press() -> Orders {
    Orders {
        jump: true,
        jump_held: true,
        ..default()
    }
}

/// `PlayerJudgeCapCatchPop::judge`: the window, a jump press, no ground, an unspent pop, the cap
/// on the head, and not the 2D model.
#[cfg_attr(test, test)]
fn the_pop_follows_its_judge() {
    let scene = floor();
    let mut p = falling_after_a_catch();
    p.step(&jump_press(), &scene);
    assert_eq!(p.mode, Move::CapCatchPop);
    assert_eq!(p.nerve_name().name, "CapCatchPop/Pop");
    // Each gate refuses it.
    let refused = |edit: &dyn Fn(&mut Player)| {
        let mut p = falling_after_a_catch();
        edit(&mut p);
        p.step(&jump_press(), &scene);
        p.mode != Move::CapCatchPop
    };
    assert!(refused(&|p| p.catch_window = 0), "the window");
    assert!(refused(&|p| p.cap_pop_possible = false), "an unspent pop");
    assert!(
        refused(&|p| p.cap = Some(hovering(Vec3::Z))),
        "the cap on the head"
    );
    assert!(refused(&|p| p.dimension.is_2d = true), "not the 2D model");
    let mut p = falling_after_a_catch();
    p.step(&Orders::default(), &scene);
    assert_ne!(p.mode, Move::CapCatchPop, "a jump press");
}

/// isCatched (0x3fe6b8) tests the nerve AND +0x2a3, including when no flight cap exists.
#[cfg_attr(test, test)]
fn the_pop_requires_the_catch_nerve_and_byte() {
    let mut p = falling_after_a_catch();
    assert!(p.judge_cap_catch_pop(&jump_press()), "Hide after a catch");
    p.cap_catched = false;
    assert!(!p.judge_cap_catch_pop(&jump_press()), "Hide without +0x2a3");
    p.catching = Some(0);
    assert!(
        !p.judge_cap_catch_pop(&jump_press()),
        "Catch without +0x2a3"
    );
    p.cap_catched = true;
    assert!(p.judge_cap_catch_pop(&jump_press()), "Catch with +0x2a3");
    p.winding = true;
    assert!(
        !p.judge_cap_catch_pop(&jump_press()),
        "SpinAttack during Catch"
    );
    p.winding = false;
    p.launch = Some(p.throw_launch(Throw::Forward, false));
    assert!(!p.judge_cap_catch_pop(&jump_press()), "ThrowStart");
    p.launch = None;
    for name in [
        "Throw",
        "ThrowBrake",
        "ThrowRolling",
        "ThrowRollingBrake",
        "ThrowSpiral",
        "ThrowStay",
        "Trample",
        "TrampleReturn",
        "Rebound",
        "LockOn",
        "Return",
    ] {
        let mut cap = hovering(Vec3::Z);
        cap.nerve = CapNerve::from_name(name).unwrap();
        p.cap = Some(cap);
        assert!(!p.judge_cap_catch_pop(&jump_press()), "{name} with +0x2a3");
    }
    p.cap_put_on = true;
    assert!(
        p.judge_cap_catch_pop(&jump_press()),
        "forcePutOn selects Hide"
    );
    p.cap_catched = false;
    assert!(
        !p.judge_cap_catch_pop(&jump_press()),
        "Hide still needs +0x2a3"
    );

    // Exercise the real return -> catch -> pop path, including the catch's arrival frame.
    let mut p = falling_after_a_catch();
    p.cap_catched = false;
    p.catch_window = 0;
    let mut cap = hovering(p.pos / UNIT + Vec3::Y * 110.);
    cap.nerve = CapNerve::Return;
    p.cap = Some(cap);
    p.step(&jump_press(), &floor());
    assert!(p.cap_catched);
    assert_eq!(p.catching, Some(-1));
    assert_eq!(p.mode, Move::CapCatchPop);

    let mut p = falling_after_a_catch();
    p.start_throw(Throw::Forward);
    p.step(&Orders::default(), &floor());
    assert!(!p.cap_catched, "startSpinAttack clears +0x2a3");
    let mut p = falling_after_a_catch();
    p.resume_cap(&Snapshot::default());
    assert!(!p.cap_catched, "a Hide snapshot does not expose +0x2a3");
    p.resume_cap(&Snapshot {
        catching: Some(0),
        ..default()
    });
    assert!(p.cap_catched, "a Catch snapshot establishes +0x2a3");
}

/// isSpinAttackAir (0x481e40) asks SpinAir/SpinAirSeparate's step <=22, not all air throws.
#[cfg_attr(test, test)]
fn the_touch_sender_checks_the_air_spin_step_window() {
    for (step, allowed) in [(0, false), (21, false), (22, false), (23, true), (40, true)] {
        let mut p = standing_with_cap(Vec3::new(0., 75., 100.));
        p.mode = Move::Throw;
        p.throwing.in_air = true;
        p.frame = step;
        p.nerve.step = 100; // The actor clock must not replace the SpinCap state clock.
        assert_eq!(p.touch_jump_possible(), allowed, "SpinAir step {step}");
        p.cap_touch_jump_phase(&held());
        assert_eq!(
            p.mode == Move::CapBounce,
            allowed,
            "sensor phase step {step}"
        );
    }
    let mut p = standing_with_cap(Vec3::new(0., 75., 100.));
    p.mode = Move::Throw;
    p.throwing.in_air = false;
    assert!(p.touch_jump_possible(), "SpinGround");
    p.throwing.in_air = true;
    p.mode = Move::Fall;
    p.spin_fall = true;
    assert!(p.touch_jump_possible(), "SpinCap/Fall");
    p.mode = Move::Jump;
    assert!(p.touch_jump_possible(), "the SpinCap state was killed");
    p.mode = Move::Throw;
    p.nerve.queue_clip("SpinCapAirStart");
    assert!(p.touch_jump_possible(), "SpinCap has not appeared yet");
}

/// `exePop`: its first step ends the sub animation, starts `JumpCapCatchCommon`, sets the air
/// control up with `CapCatchPopPower` and `CapCatchPopGravity`, restarts the cap's catch with
/// `JumpCapCatch`, and spends the pop; it ends with the clip and the 30th step.
#[cfg_attr(test, test)]
fn the_pop_is_its_own_state() {
    let scene = floor();
    let mut p = falling_after_a_catch();
    p.pos.y = 50.;
    p.step(&jump_press(), &scene);
    assert_eq!(p.mode, Move::CapCatchPop);
    assert!(p.cap_pop_possible, "the nerve's first step has not run");
    p.step(&Orders::default(), &scene);
    assert!(!p.cap_pop_possible);
    assert_eq!(p.anim.name, "JumpCapCatchCommon");
    let gravity = p.constants.cap_catch_pop_gravity;
    assert!(
        (p.v.y - (t::CAP_CATCH_POP_POWER - gravity)).abs() < 1e-3,
        "{}",
        p.v.y
    );
    assert_eq!(p.cap_row().map(|row| (row.state, row.clip)), None);
    p.step(&Orders::default(), &scene);
    assert_eq!(
        p.cap_row().map(|row| (row.state, row.clip)),
        Some(("Catch", "JumpCapCatch"))
    );
    // The state ends when the clip has ended and 30 steps have run; high enough to still be in
    // the air then, it hands over to the fall.
    p.pos.y = 50.;
    p.v.y = 0.;
    for _ in 0..40 {
        p.anim.frame = clip::JUMP_CAP_CATCH_COMMON as f32;
        p.step(&Orders::default(), &scene);
        if p.mode != Move::CapCatchPop {
            break;
        }
    }
    assert_eq!(p.mode, Move::Fall);
    assert!(p.state_frames() <= 1);
}
