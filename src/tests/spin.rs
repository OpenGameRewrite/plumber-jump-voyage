//! The spin family against the recordings `spin-ground-left`, `spin-jump-left`
//! and `spin-jump-drop` (frame numbers below are their rows).
use super::*;

pub(super) fn run() {
    a_stick_circle_starts_the_ground_spin();
    the_ground_spin_lasts_ninety_steps();
    the_spin_jump_leaves_with_its_own_limits();
    the_spin_drop_restarts_the_jump();
    a_held_spin_drop_lands_in_the_drop_landing();
}

fn floor() -> Course {
    Course {
        solids: vec![Solid {
            center: Vec3::new(0., -0.5, 0.),
            half: Vec3::new(100., 0.5, 100.),
            slope: 0.,
            moving: false,
            slippery: false,
        }],
        ..default()
    }
}
/// The scripted stick of the recordings: a full-radius circle of 15 frames a
/// turn that starts upward and turns counter-clockwise.
fn circling(n: usize) -> Orders {
    let angle = std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * n as f32 / 15.;
    let up = Vec2::new(angle.cos(), angle.sin());
    Orders {
        stick: Vec2::new(up.x, -up.y),
        direction: Vec2::new(-up.x, up.y),
        ..default()
    }
}
/// A player that rested for ten frames and then circled the stick for 31
/// frames (rows 10 to 40): the spin was just chosen.
fn just_spinning(scene: &Course) -> Player {
    let mut p = Player::default();
    for _ in 0..10 {
        p.step(&Orders::default(), scene);
    }
    for n in 0..31 {
        p.step(&circling(n), scene);
    }
    p
}
/// Step until the ground is reached, then once more: the step after the
/// contact decides what the landing is.
fn until_landed(p: &mut Player, o: Orders, scene: &Course) {
    for _ in 0..600 {
        p.step(&o, scene);
        if p.grounded {
            break;
        }
    }
    p.step(&o, scene);
}
fn jump_pressed() -> Orders {
    Orders {
        jump: true,
        jump_held: true,
        ..default()
    }
}

#[cfg_attr(test, test)]
fn a_stick_circle_starts_the_ground_spin() {
    let scene = floor();
    let mut p = Player::default();
    for _ in 0..10 {
        p.step(&Orders::default(), &scene);
    }
    for n in 0..30 {
        p.step(&circling(n), &scene);
        assert_ne!(p.mode, Move::GroundSpin, "frame {n}: not a spin yet");
    }
    let before = p.state_frames();
    p.step(&circling(30), &scene);
    // Row 40: the spin is chosen in the step the detector finds the circle,
    // on the run's own step count; the run's last step has just moved the
    // player, and the spin's setup took the push into the floor away.
    assert_eq!(p.mode, Move::GroundSpin);
    assert_eq!(p.nerve_name().name, "Run/GroundSpin3D");
    assert_eq!(p.state_frames(), before + 1, "the run's step count goes on");
    assert_eq!(p.v.y, 0.);
    assert_eq!(p.pull, 0., "no push into the floor in the chosen step");
    assert_ne!(
        p.anim.name, "SpinGroundL",
        "the clip starts with the first step"
    );
    assert!(p.events.iter().all(|e| !matches!(e, Fx::Land { .. })));
    // Row 41: the first step starts the left clip (counter-clockwise) and
    // faces the way the velocity points.
    p.step(&circling(31), &scene);
    assert_eq!((p.anim.name, p.anim.frame), ("SpinGroundL", 0.));
    let along = p.v.with_y(0.).normalize();
    assert!(p.facing.dot(along) > 0.9999);
    assert!(p.grounded, "the floor is regained");
    assert!(
        p.events.iter().all(|e| !matches!(e, Fx::Land { .. })),
        "regaining the floor is not a landing"
    );
}

#[cfg_attr(test, test)]
fn the_ground_spin_lasts_ninety_steps() {
    let scene = floor();
    let mut p = just_spinning(&scene);
    // The stick rests from here on. The spin executes 91 times (rows 41 to
    // 131): the last one ends it and the wait state is chosen.
    let mut speed = p.speed_h();
    for step in 0..90 {
        p.step(&Orders::default(), &scene);
        assert_eq!(p.mode, Move::GroundSpin, "step {step}");
        if step > 0 {
            // The braking is 0.95 a step with no stick.
            assert!((p.speed_h() - speed * 0.95).abs() < 1e-4, "step {step}");
        }
        speed = p.speed_h();
    }
    p.step(&Orders::default(), &scene);
    assert_eq!(p.mode, Move::Idle);
    assert_eq!(p.state_frames(), 0, "the wait nerve starts over");
    p.step(&Orders::default(), &scene);
    assert_eq!((p.anim.name, p.speed_h()), ("Wait", 0.));
}

#[cfg_attr(test, test)]
fn the_spin_jump_leaves_with_its_own_limits() {
    let scene = floor();
    let mut p = just_spinning(&scene);
    for _ in 0..5 {
        p.step(&Orders::default(), &scene);
    }
    // Row 46: the jump press during the spin chooses the spin jump.
    p.step(&jump_pressed(), &scene);
    assert_eq!(p.mode, Move::SpinJump);
    assert_eq!(p.nerve_name().name, "Jump/JumpSpinGround");
    assert_eq!(p.phase_clip(), "StartSpinJumpL");
    let speed = p.speed_h();
    // Row 47: the power of the spin jump, one step of its gravity, the speed
    // the spin had.
    p.step(&Orders::default(), &scene);
    assert_eq!(p.anim.name, "StartSpinJumpL");
    assert!((p.v.y - 19.6).abs() < 1e-5);
    assert!((p.speed_h() - speed).abs() < 1e-5);
    p.step(&Orders::default(), &scene);
    assert!((p.v.y - 19.2).abs() < 1e-5);
    // The air control caps the speed at SpinJumpMoveSpeedMax however long the
    // stick pushes.
    let push = Orders {
        direction: p.facing.xz() * Vec2::new(1., 1.),
        ..default()
    };
    for _ in 0..60 {
        p.step(&push, &scene);
        assert!(p.speed_h() <= t::SPIN_JUMP_MOVE_MAX + 1e-4);
    }
    // The clip repeats every 8 frames.
    let mut frames = Vec::new();
    for _ in 0..18 {
        p.step(&Orders::default(), &scene);
        frames.push(p.anim.frame);
    }
    assert!(frames.iter().all(|f| (0.0..8.).contains(f)));
    assert!(frames.windows(9).all(|w| w[0] == w[8]), "{frames:?}");
}

/// A spin jump 20 steps in (row 66, the step the recorded drop was pressed).
fn spin_jump_rising(scene: &Course) -> Player {
    let mut p = just_spinning(scene);
    for _ in 0..5 {
        p.step(&Orders::default(), scene);
    }
    p.step(&jump_pressed(), scene);
    for _ in 0..19 {
        p.step(&Orders::default(), scene);
    }
    assert_eq!(p.mode, Move::SpinJump);
    p
}

#[cfg_attr(test, test)]
fn the_spin_drop_restarts_the_jump() {
    let scene = floor();
    let mut p = spin_jump_rising(&scene);
    let steps = p.state_frames();
    // The crouch trigger chooses the drop on the jump nerve's own steps.
    p.step(
        &Orders {
            crouch: true,
            crouch_pressed: true,
            ..default()
        },
        &scene,
    );
    assert_eq!(p.mode, Move::SpinDrop);
    assert_eq!(p.nerve_name().name, "Jump/JumpSpinGroundDownFall");
    assert_eq!(p.state_frames(), steps + 1);
    // Rows 67 to 71: straight down from the initial speed, then 1.5 more a
    // step; no horizontal speed is left.
    for (n, expected) in [-36.5, -38., -39.5, -41., -42.5].into_iter().enumerate() {
        p.step(&Orders::default(), &scene);
        assert!((p.v.y - expected).abs() < 1e-4, "step {n}: {}", p.v.y);
        assert_eq!(p.speed_h(), 0.);
    }
    assert_eq!(p.anim.name, "SpinJumpDownFallL");
    // Row 72: five steps over and the button not held: the jump nerve is
    // chosen again; its first step (row 73) starts the restart clip falling
    // at the fall limit.
    p.step(&Orders::default(), &scene);
    assert_eq!(p.mode, Move::SpinJump);
    assert!((p.v.y + 44.).abs() < 1e-4);
    p.step(&Orders::default(), &scene);
    assert_eq!(p.anim.name, "RestartSpinJumpL");
    assert!((p.v.y + 35.).abs() < 1e-4);
    // The rest of the fall is the plain landing of a jump.
    until_landed(&mut p, Orders::default(), &scene);
    assert_eq!(p.phase_clip(), "Land");
}

#[cfg_attr(test, test)]
fn a_held_spin_drop_lands_in_the_drop_landing() {
    let scene = floor();
    let mut p = spin_jump_rising(&scene);
    let held = Orders {
        crouch: true,
        ..default()
    };
    p.step(
        &Orders {
            crouch_pressed: true,
            ..held.clone()
        },
        &scene,
    );
    // Held through the fall, the drop never hands back to the jump (the fall
    // speed stops at the drop's maximum) and ends on the ground.
    let mut deepest = 0f32;
    for _ in 0..200 {
        p.step(&held, &scene);
        deepest = deepest.min(p.v.y);
        assert_ne!(p.mode, Move::SpinJump);
        if p.mode != Move::SpinDrop {
            break;
        }
    }
    assert!(deepest >= -t::SPIN_DOWN_FALL_MAX);
    assert_eq!(p.phase_clip(), "LandDownFall");
}
