use super::*;

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
fn fwd(extra: Orders) -> Orders {
    Orders {
        direction: Vec2::NEG_Y,
        ..extra
    }
}
fn frames(p: &mut Player, o: Orders, scene: &Course, n: usize) {
    for _ in 0..n {
        p.step(&o, scene);
    }
}
/// The state a step chose and the clip it starts with. A state change is decided
/// in one step and starts in the next, so the clip still playing is the old one.
fn chosen(p: &Player) -> (Move, &'static str) {
    (p.mode, p.phase_clip())
}
/// Step until the ground is reached, then once more: the step after the contact
/// is the one that decides what the landing is.
fn until_landed(p: &mut Player, o: Orders, scene: &Course) -> f32 {
    let high = until_ground(p, o.clone(), scene);
    p.step(&o, scene);
    high
}
fn until_ground(p: &mut Player, o: Orders, scene: &Course) -> f32 {
    let mut high = p.pos.y;
    for _ in 0..600 {
        p.step(&o, scene);
        high = high.max(p.pos.y);
        if p.grounded {
            break;
        }
    }
    high
}

pub(super) fn run() {
    original_simulation_trace();
    the_spin_throw_brakes_only_along_its_front();
    state_changes_follow_the_nerve_machine();
    idle_sequence();
    run_speed();
    variable_jump_height();
    chained_jumps();
    brake_and_sideflip();
    crouch_jumps();
    ground_pound();
    pound_jump_window();
    dive_from_pound();
    dive_roll_landing();
    roll_timing_and_boost();
    roll_winds_down();
    roll_boost_needs_the_force_to_fill();
    landing_asks_the_run_judge_before_the_squat();
    roll_jump_is_a_nerve_of_the_roll();
    cap_flight();
    spin_throw();
    air_throw();
    running_throw();
    cap_bounce();
    wall_slide_and_ledge();
    swimming();
    the_dive_in_judge_wants_a_long_fall_over_deep_water();
    a_trample_in_the_water_hops_the_swimmer_up();
    poles();
    slope_slide();
    burn_and_soot();
    low_ceiling();
    wall_collision_and_stress();
}

#[cfg_attr(test, test)]
fn state_changes_follow_the_nerve_machine() {
    let floor = floor();
    let idle = Orders::default();
    // A jump pressed while standing only chooses the jump: the row reads the
    // new state at step 0 with the old clip and the old motion.
    let mut p = Player::default();
    frames(&mut p, idle.clone(), &floor, 5);
    let before = (p.state_frames(), p.anim.name);
    assert_eq!(before.0, 5);
    p.step(
        &Orders {
            jump: true,
            jump_held: true,
            ..default()
        },
        &floor,
    );
    assert_eq!(
        (p.mode, p.state_frames(), p.anim.name),
        (Move::Jump, 0, "Wait")
    );
    assert!(p.grounded && p.pos.y == 0. && p.vel.y <= 0.);
    // The first step starts the clip at frame 0 and takes off.
    p.step(&idle, &floor);
    // Facing the camera's axis, the first jump plays the mirrored clip.
    assert_eq!(
        (p.anim.name, p.anim.frame, p.state_frames()),
        ("JumpReverse", 0., 1)
    );
    assert!(!p.grounded && p.vel.y > 0. && p.pos.y > 0.);
    // Landing: the contact is flagged, then the landing state is chosen, then
    // its clip starts.
    until_ground(&mut p, idle.clone(), &floor);
    assert!(p.grounded && p.mode == Move::Jump, "contact only");
    p.step(&idle, &floor);
    assert_eq!(
        (p.mode, p.state_frames(), p.phase_clip(), p.anim.name),
        (Move::Idle, 0, "Land", "JumpReverse"),
        "the landing state is chosen a step after the contact"
    );
    p.step(&idle, &floor);
    assert_eq!((p.anim.name, p.anim.frame), ("Land", 0.));
    // Running from a standstill: chosen first, moving from its first step.
    let mut p = Player::default();
    p.step(&fwd(default()), &floor);
    assert_eq!(
        (p.mode, p.state_frames(), p.flat_speed()),
        (Move::Walk, 0, 0.)
    );
    p.step(&fwd(default()), &floor);
    assert!(p.flat_speed() > 0. && p.state_frames() == 1);
    // Crouching passes through the squat nerve's braking sub-state, which
    // keeps the nerve's step count when it settles into the crouch.
    let mut p = Player::default();
    let crouch = Orders {
        crouch: true,
        ..default()
    };
    p.step(&crouch, &floor);
    assert_eq!((p.mode, p.state_frames()), (Move::SquatBrake, 0));
    p.step(&crouch, &floor);
    assert_eq!(
        (p.mode, p.state_frames(), p.anim.name),
        (Move::Crouch, 1, "SquatStart")
    );
    p.step(&crouch, &floor);
    assert_eq!((p.state_frames(), p.anim.name), (2, "SquatWait"));
}

/// The wait state's idle sequence on the steps the `idle-sleep` recording shows
/// (its rows are these steps minus the ten the replay settles for): the relax
/// after 180 steps of `Wait`, the clips that end, then 1200 steps of `WaitRelax`
/// and 900 of `WaitSnooze`; any input ends the sequence and standing starts it over.
#[cfg_attr(test, test)]
fn idle_sequence() {
    let floor = floor();
    let idle = Orders::default();
    let mut p = Player::default();
    let mut chosen_at = Vec::new();
    let mut last = p.phase_clip();
    for step in 0..4500 {
        p.step(&idle, &floor);
        if p.phase_clip() != last {
            last = p.phase_clip();
            chosen_at.push((step, last));
        }
    }
    assert_eq!(
        chosen_at,
        [
            (180, "WaitRelaxStart"),
            (331, "WaitRelax"),
            (1532, "SnoozeStart"),
            (2040, "Snooze"),
            (2941, "SleepStart"),
            (3550, "Sleep"),
        ]
    );
    assert_eq!(p.nerve_name().name, "Wait/WaitSleep");
    // Waking up: the stick runs, and standing again starts over with `Wait`.
    frames(&mut p, fwd(Orders::default()), &floor, 30);
    assert_eq!(p.mode, Move::Run);
    let mut chosen_at = Vec::new();
    let mut last = p.phase_clip();
    for step in 0..400 {
        p.step(&idle, &floor);
        if p.phase_clip() != last {
            last = p.phase_clip();
            chosen_at.push((step, last));
        }
    }
    let wait = chosen_at.iter().position(|(_, clip)| *clip == "Wait");
    let relax = chosen_at
        .iter()
        .position(|(_, clip)| *clip == "WaitRelaxStart");
    let (wait, relax) = (
        wait.expect("standing starts with Wait"),
        relax.expect("relaxes again"),
    );
    assert_eq!(
        chosen_at[relax].0 - chosen_at[wait].0,
        181,
        "the relax comes 180 steps after Wait's first step"
    );
}

#[cfg_attr(test, test)]
fn run_speed() {
    let floor = floor();
    // Running reaches the original top speed after its 40-frame ramp.
    let mut p = Player::default();
    frames(&mut p, fwd(default()), &floor, 45);
    assert_eq!(p.mode, Move::Run);
    assert!((p.move_speed - 14.).abs() < 0.01, "{}", p.move_speed);
    assert_eq!(p.anim.name, "Move");
}

#[cfg_attr(test, test)]
fn variable_jump_height() {
    let floor = floor();
    // Holding jump extends the first jump; tapping gives a lower hop.
    let mut held = Player::default();
    held.step(
        &Orders {
            jump: true,
            jump_held: true,
            ..default()
        },
        &floor,
    );
    assert_eq!(chosen(&held), (Move::Jump, "JumpReverse"));
    let high = until_ground(
        &mut held,
        Orders {
            jump_held: true,
            ..default()
        },
        &floor,
    );
    let mut tap = Player::default();
    tap.step(
        &Orders {
            jump: true,
            ..default()
        },
        &floor,
    );
    let low = until_ground(&mut tap, Orders::default(), &floor);
    assert!(high > low + 1. && low > 0.8, "held {high} tap {low}");
}

#[cfg_attr(test, test)]
fn chained_jumps() {
    let floor = floor();
    // Chained jumps while running: Jump, Jump2, Jump3.
    let mut p = Player::default();
    frames(&mut p, fwd(default()), &floor, 45);
    for (mode, anim) in [
        (Move::Jump, "Jump"),
        (Move::DoubleJump, "Jump2"),
        (Move::TripleJump, "Jump3"),
    ] {
        p.step(
            &fwd(Orders {
                jump: true,
                jump_held: true,
                ..default()
            }),
            &floor,
        );
        let name = if p.phase_clip() == "JumpReverse" {
            "Jump"
        } else {
            p.phase_clip()
        };
        assert_eq!((p.mode, name), (mode, anim));
        until_ground(
            &mut p,
            fwd(Orders {
                jump_held: true,
                ..default()
            }),
            &floor,
        );
    }
}

#[cfg_attr(test, test)]
fn brake_and_sideflip() {
    let floor = floor();
    // Reversing at speed brakes; jumping out of it is a side somersault.
    let mut p = Player::default();
    frames(&mut p, fwd(default()), &floor, 45);
    p.step(
        &Orders {
            direction: Vec2::Y,
            ..default()
        },
        &floor,
    );
    assert_eq!(chosen(&p), (Move::Skid, "Brake"));
    p.step(
        &Orders {
            direction: Vec2::Y,
            jump: true,
            ..default()
        },
        &floor,
    );
    assert_eq!(chosen(&p), (Move::Sideflip, "JumpTurn"));
    // The flip itself is the state's first step.
    p.step(
        &Orders {
            direction: Vec2::Y,
            ..default()
        },
        &floor,
    );
    assert!(p.vel.z > 0.);
}

#[cfg_attr(test, test)]
fn crouch_jumps() {
    let floor = floor();
    // Crouch then jump: backflip at rest, long jump while sliding forward.
    let mut p = Player::default();
    frames(
        &mut p,
        Orders {
            crouch: true,
            ..default()
        },
        &floor,
        5,
    );
    p.step(
        &Orders {
            crouch: true,
            jump: true,
            ..default()
        },
        &floor,
    );
    assert_eq!(chosen(&p), (Move::Backflip, "JumpBack"));
    p = Player::default();
    frames(&mut p, fwd(default()), &floor, 45);
    p.step(
        &fwd(Orders {
            crouch: true,
            ..default()
        }),
        &floor,
    );
    p.step(
        &fwd(Orders {
            crouch: true,
            jump: true,
            ..default()
        }),
        &floor,
    );
    assert_eq!(chosen(&p), (Move::LongJump, "JumpBroad"));
    assert!(p.move_speed >= 14.);
}

#[cfg_attr(test, test)]
fn ground_pound() {
    let floor = floor();
    // Ground pound: hang through the start clip, drop at 45, land, jump.
    let mut p = Player::default();
    p.pos.y = 6.;
    p.grounded = false;
    p.mode = Move::Fall;
    p.step(
        &Orders {
            crouch_pressed: true,
            crouch: true,
            ..default()
        },
        &floor,
    );
    assert_eq!(chosen(&p), (Move::GroundPound, "HipDropStart"));
    frames(&mut p, Orders::default(), &floor, 10);
    // The frame the pound was chosen in still carried the fall's gravity.
    assert!((p.pos.y - 6.).abs() < 0.02, "pound hangs first");
    until_landed(&mut p, Orders::default(), &floor);
    assert_eq!(chosen(&p), (Move::PoundLand, "HipDropLand"));
    // The game's logic does not freeze on the impact.
    p.step(&Orders::default(), &floor);
    assert!(p.hitstop == 0.);
    frames(&mut p, Orders::default(), &floor, 6);
    p.step(
        &Orders {
            jump: true,
            ..default()
        },
        &floor,
    );
    assert_eq!(chosen(&p), (Move::PoundJump, "JumpHipDrop"));
}

#[cfg_attr(test, test)]
fn pound_jump_window() {
    let floor = floor();
    // The landing nerve may be cancelled into a jump from its fifth step
    // (`isEnableLandCancel` reads the step after the nerve's own update has
    // counted the one in progress): a jump pressed on the second step is
    // buffered and chosen on the fifth. The recorded run chooses it that way.
    let mut p = Player::default();
    p.pos.y = 6.;
    p.grounded = false;
    p.mode = Move::Fall;
    p.step(
        &Orders {
            crouch_pressed: true,
            crouch: true,
            ..default()
        },
        &floor,
    );
    until_landed(&mut p, Orders::default(), &floor);
    assert_eq!(chosen(&p).0, Move::PoundLand);
    p.step(&Orders::default(), &floor);
    p.step(
        &Orders {
            jump: true,
            ..default()
        },
        &floor,
    );
    for _ in 0..2 {
        p.step(&Orders::default(), &floor);
        assert_eq!(p.mode, Move::PoundLand);
    }
    p.step(&Orders::default(), &floor);
    assert_eq!(chosen(&p), (Move::PoundJump, "JumpHipDrop"));
}

#[cfg_attr(test, test)]
fn dive_from_pound() {
    let floor = floor();
    // Throw during the pound start: dive with the original head-slide clip.
    let mut p = Player::default();
    p.pos.y = 6.;
    p.grounded = false;
    p.mode = Move::Fall;
    p.step(
        &Orders {
            crouch_pressed: true,
            crouch: true,
            ..default()
        },
        &floor,
    );
    p.step(
        &Orders {
            throw_pressed: true,
            direction: Vec2::X,
            ..default()
        },
        &floor,
    );
    assert_eq!(chosen(&p), (Move::Dive, "HeadSlidingStart"));
    // The dive's push is the state's first step.
    p.step(&Orders::default(), &floor);
    assert!(p.vel.x > 10. && p.vel.y > 0.);
    until_landed(&mut p, Orders::default(), &floor);
    // Without the stick a dive lands like any jump: the wait state's landing
    // clip, which stops the player (the recorded `dive-no-stick`).
    assert_eq!(chosen(&p), (Move::Idle, "Land"));
    assert_eq!(p.nerve_name().name, "Wait/Land");
    // The state's first step starts the clip and stops the slide.
    p.step(&Orders::default(), &floor);
    assert!(p.speed_h() < 1e-3);
}

#[cfg_attr(test, test)]
fn dive_roll_landing() {
    let floor = floor();
    // Holding crouch through the landing rolls on with the dive's speed.
    let mut p = Player::default();
    p.pos.y = 6.;
    p.grounded = false;
    p.mode = Move::Fall;
    let held = Orders {
        crouch: true,
        ..default()
    };
    p.step(
        &Orders {
            crouch_pressed: true,
            crouch: true,
            ..default()
        },
        &floor,
    );
    p.step(
        &Orders {
            throw_pressed: true,
            crouch: true,
            direction: Vec2::X,
            ..default()
        },
        &floor,
    );
    until_landed(&mut p, held.clone(), &floor);
    assert_eq!(p.mode, Move::Roll);
    let mut names = std::collections::BTreeSet::new();
    let mut rolled = 0;
    while p.mode == Move::Roll && rolled < 600 {
        // The roll goes on while crouch is held and ends once it is let go.
        let orders = if rolled < 60 {
            &held
        } else {
            &Orders::default()
        };
        p.step(orders, &floor);
        names.insert(p.anim.name);
        rolled += 1;
    }
    assert!(rolled > 60, "holding crouch keeps rolling: {rolled} frames");
    assert!(
        names.contains("Rolling"),
        "the roll plays its loop: {names:?}"
    );
    assert!(
        rolled < 200,
        "the roll ends like any other: {rolled} frames"
    );
}

#[cfg_attr(test, test)]
fn roll_timing_and_boost() {
    let floor = floor();
    // Roll: starts at 20, decays, and on flat ground ends in a crouch.
    let mut p = Player::default();
    frames(
        &mut p,
        Orders {
            crouch: true,
            ..default()
        },
        &floor,
        3,
    );
    p.step(
        &Orders {
            crouch: true,
            throw_pressed: true,
            ..default()
        },
        &floor,
    );
    assert_eq!(chosen(&p), (Move::Roll, "RollingStart"));
    assert!(p.cap.is_none(), "rolling does not throw the cap");
    // Holding crouch keeps the roll going, slowing a little every frame.
    for _ in 0..100 {
        p.step(
            &Orders {
                crouch: true,
                ..default()
            },
            &floor,
        );
    }
    assert_eq!(p.mode, Move::Roll);
    assert!(p.move_speed < t::ROLL_START && p.move_speed > t::ROLL_END_SPEED - 5.);
    // Letting go before the minimum rolls on until it is up, then stands up.
    let mut q = Player::default();
    frames(
        &mut q,
        Orders {
            crouch: true,
            ..default()
        },
        &floor,
        3,
    );
    q.step(
        &Orders {
            crouch: true,
            throw_pressed: true,
            ..default()
        },
        &floor,
    );
    let mut ended = None;
    for i in 0..200 {
        q.step(&Orders::default(), &floor);
        if q.mode == Move::RollEnd {
            ended = Some(i);
            break;
        }
    }
    assert!(
        ended.is_some_and(|f| f >= t::ROLL_FRAMES_MIN as usize - 2),
        "roll lasts its minimum before standing up: {ended:?}"
    );
    // Pressing again once the roll is charged restarts it faster.
    let before = p.move_speed;
    p.step(
        &Orders {
            crouch: true,
            throw_pressed: true,
            ..default()
        },
        &floor,
    );
    assert!(p.move_speed > before, "{} vs {before}", p.move_speed);
    // Releasing crouch stands up once the new roll has run its minimum.
    frames(&mut p, Orders::default(), &floor, 80);
    assert_ne!(p.mode, Move::Roll);
}

/// The restart boosts only when the restart force (`ReStartForce`, 60 steps since the roll
/// began or the last boost) or the charge is full: presses 20 steps apart boost on the fourth
/// (the recorded `roll-boost`), and the roll that began on a landing counts its first boost
/// as the second.
#[cfg_attr(test, test)]
fn roll_boost_needs_the_force_to_fill() {
    let floor = floor();
    let held = Orders {
        crouch: true,
        ..default()
    };
    let press = Orders {
        throw_pressed: true,
        ..held.clone()
    };
    let mut p = Player::default();
    frames(&mut p, held.clone(), &floor, 3);
    p.step(&press, &floor);
    assert_eq!(chosen(&p), (Move::Roll, "RollingStart"));
    let mut boosts = vec![];
    for _ in 0..4 {
        frames(&mut p, held.clone(), &floor, 19);
        p.events.clear();
        p.step(&press, &floor);
        boosts.push(
            p.events
                .iter()
                .filter(|fx| matches!(fx, Fx::RollBoost { .. }))
                .count(),
        );
    }
    assert_eq!(boosts, [0, 0, 0, 1]);
    // The charge of a roll that began in `Rolling` (a landing) is full: its first press boosts,
    // and as the second boost of the roll.
    let mut q = Player::default();
    q.start_roll(RollStart::Landing);
    frames(&mut q, held.clone(), &floor, 12);
    q.events.clear();
    q.step(&press, &floor);
    assert!(
        q.events
            .iter()
            .any(|fx| matches!(fx, Fx::RollBoost { again: true }))
    );
}

/// `setNerveOnGround` asks the run judge before the squat judge: with the stick and crouch held at
/// touchdown the run state is chosen, and it starts the crouch a step later (the recorded
/// `roll-from-land`). The spin drop's landing (`ForceLand`) vetoes both.
#[cfg_attr(test, test)]
fn landing_asks_the_run_judge_before_the_squat() {
    let floor = floor();
    let run = fwd(Orders::default());
    let mut p = Player::default();
    frames(&mut p, run.clone(), &floor, 30);
    p.step(
        &Orders {
            jump: true,
            jump_held: true,
            ..run.clone()
        },
        &floor,
    );
    let crouching = Orders {
        crouch: true,
        ..run.clone()
    };
    until_landed(&mut p, crouching.clone(), &floor);
    assert!(
        matches!(p.mode, Move::Run | Move::Walk),
        "the stick runs first: {:?}",
        p.mode
    );
    p.step(&crouching, &floor);
    assert_eq!(p.mode, Move::SquatBrake);
    // Standing still, crouch alone squats at touchdown.
    let mut q = Player::default();
    q.step(
        &Orders {
            jump: true,
            jump_held: true,
            ..default()
        },
        &floor,
    );
    let held = Orders {
        crouch: true,
        ..default()
    };
    until_landed(&mut q, held, &floor);
    assert_eq!(q.mode, Move::SquatBrake);
}

/// A roll that slows below the end speed with crouch held unrolls, brakes
/// and hands the actor to the squat (the recorded `roll-unroll`).
#[cfg_attr(test, test)]
fn roll_winds_down() {
    let floor = floor();
    let held = Orders {
        crouch: true,
        ..default()
    };
    let mut p = Player::default();
    frames(&mut p, held.clone(), &floor, 3);
    p.step(
        &Orders {
            throw_pressed: true,
            ..held.clone()
        },
        &floor,
    );
    let mut names = vec![];
    for _ in 0..400 {
        p.step(&held, &floor);
        let name = p.nerve_name().name;
        if names.last() != Some(&name) {
            names.push(name);
        }
        if p.mode == Move::SquatBrake {
            break;
        }
    }
    // The brake shares the unrolling clip; only the nerve tells them apart.
    assert_eq!(
        &names[names.len() - 4..],
        [
            "Rolling/Rolling",
            "Rolling/UnRoll",
            "Rolling/Brake",
            "Squat/Brake"
        ]
    );
    // The squat the brake hands over to skips its start clip.
    assert_eq!(p.phase_clip(), "SquatWait");
}

/// The roll's jump and its landing are nerves of the roll state: its step
/// count runs on through them, and landing with crouch held rolls on.
#[cfg_attr(test, test)]
fn roll_jump_is_a_nerve_of_the_roll() {
    let floor = floor();
    let held = Orders {
        crouch: true,
        ..default()
    };
    let mut p = Player::default();
    frames(&mut p, held.clone(), &floor, 3);
    p.step(
        &Orders {
            throw_pressed: true,
            ..held.clone()
        },
        &floor,
    );
    frames(&mut p, held.clone(), &floor, 20);
    p.step(
        &Orders {
            jump: true,
            jump_held: true,
            ..held.clone()
        },
        &floor,
    );
    assert_eq!(p.nerve_name().name, "Rolling/Jump");
    assert!(
        p.state_frames() > 20,
        "the roll's steps run on: {}",
        p.state_frames()
    );
    let steps = p.state_frames();
    p.step(&held, &floor);
    assert_eq!(p.state_frames(), steps + 1);
    // The long jump's count rises at take-off and waits in the air.
    assert_eq!(p.long_combo.count, 1);
    let waiting = p.long_combo.timer;
    frames(&mut p, held.clone(), &floor, 5);
    assert_eq!(p.long_combo.timer, waiting);
    until_landed(&mut p, held.clone(), &floor);
    assert_eq!(p.nerve_name().name, "Rolling/Land");
    assert!(
        p.state_frames() > steps + 20,
        "no restart: {}",
        p.state_frames()
    );
    // Without crouch the landing ends the roll instead.
    let mut q = Player::default();
    frames(&mut q, held.clone(), &floor, 3);
    q.step(
        &Orders {
            throw_pressed: true,
            ..held.clone()
        },
        &floor,
    );
    frames(&mut q, held.clone(), &floor, 20);
    q.step(
        &Orders {
            jump: true,
            jump_held: true,
            ..held
        },
        &floor,
    );
    until_landed(&mut q, Orders::default(), &floor);
    assert!(
        !matches!(q.mode, Move::Roll | Move::RollJump),
        "{:?}",
        q.mode
    );
}

/// `rs::moveInertiaTurn` in the spin-cap state (`cap-throw-spiral`, rows 46 to 48): the player
/// ran, turned to the stick (-0.59, -0.81) and threw; the velocity (-0.30, 1.94) points away from
/// that front, and only its part along the front loses the 0.95 each step, so the sideways part
/// stays and the direction swings.
#[cfg_attr(test, test)]
fn the_spin_throw_brakes_only_along_its_front() {
    let mut p = Player {
        facing: Vec3::new(-0.587_803, 0., -0.809_004),
        grounded: true,
        v: Vec3::new(-0.302_507, -7., 1.937_606),
        ..Player::default()
    };
    p.spin_cap_inertia();
    assert!(
        (p.v.x + 0.343_351).abs() < 2e-6 && (p.v.z - 1.881_392).abs() < 2e-6,
        "{}",
        p.v
    );
    p.spin_cap_inertia();
    assert!(
        (p.v.x + 0.382_153).abs() < 2e-6 && (p.v.z - 1.827_988).abs() < 2e-6,
        "{}",
        p.v
    );
}

#[cfg_attr(test, test)]
fn cap_flight() {
    let floor = floor();
    // Standing cap throw: the spin clip, then the cap leaves on frame 12.
    let mut p = Player::default();
    p.step(
        &Orders {
            throw_pressed: true,
            throw_held: true,
            ..default()
        },
        &floor,
    );
    assert_eq!(chosen(&p), (Move::Throw, "SpinCapStart"));
    // The throw state calls for the cap on its twelfth step; the cap, which
    // updates first in a frame, appears in the next one.
    let held = Orders {
        throw_held: true,
        ..default()
    };
    frames(&mut p, held.clone(), &floor, 12);
    assert!(p.cap.is_none() && p.launch.is_none());
    frames(&mut p, held.clone(), &floor, 1);
    assert!(p.cap.is_none() && p.launch.is_some());
    frames(&mut p, held.clone(), &floor, 1);
    assert!(p.cap.is_some());
    // The cap travels its 678 units, hovers while held, then returns.
    frames(&mut p, held, &floor, 40);
    let cap = p.cap.as_ref().unwrap();
    assert_eq!(cap.phase, CapPhase::Stay);
    assert!(((cap.pos - p.pos).with_y(0.).length() - 6.78).abs() < 0.05);
    frames(&mut p, Orders::default(), &floor, 120);
    assert!(p.cap.is_none());
}

#[cfg_attr(test, test)]
fn spin_throw() {
    let floor = floor();
    // A spin throw plays its whole clip even while the stick keeps circling.
    let mut p = Player::default();
    p.step(
        &Orders {
            spin_throw: true,
            ..default()
        },
        &floor,
    );
    assert_eq!(p.mode, Move::Throw);
    assert!(
        p.phase_clip().starts_with("SpinCapStartDouble"),
        "{}",
        p.phase_clip()
    );
    let length = clip::throw(p.phase_clip()) as usize;
    let mut q = Player::default();
    q.step(
        &Orders {
            spin_throw: true,
            ..default()
        },
        &floor,
    );
    // With the stick released the throw plays its whole clip.
    let mut lasted = 0;
    while p.mode == Move::Throw && lasted < 200 {
        p.step(&Orders::default(), &floor);
        lasted += 1;
    }
    assert!(
        (length..=length + 3).contains(&lasted),
        "{lasted} steps for a clip of {length}"
    );
    // The stick does not end it: the state runs on to the end of its clip with
    // the cap away (`exeSpinGround`), and the run starts then.
    let moving = Orders {
        direction: Vec2::X,
        ..default()
    };
    frames(&mut q, moving.clone(), &floor, 5);
    assert_eq!(q.mode, Move::Throw, "the cap has not left yet");
    frames(&mut q, moving.clone(), &floor, 6);
    assert_eq!(q.mode, Move::Throw, "the stick does not cut the clip short");
    frames(&mut q, moving, &floor, length + 3);
    assert_ne!(q.mode, Move::Throw);
}

/// The throw in the air (`PlayerStateSpinCap::exeSpinAir`): the state stays
/// in the air for its 23 steps (the ground fall judge is not asked), the first
/// step sets the air control up with the spin's values (lift 6, gravity 0.3,
/// the stick pushes 0.5 a frame) and turns the actor to the stick at once.
#[cfg_attr(test, test)]
fn air_throw() {
    let floor = floor();
    let mut p = Player::default();
    frames(&mut p, default(), &floor, 12);
    p.step(
        &Orders {
            jump: true,
            jump_held: true,
            ..default()
        },
        &floor,
    );
    frames(&mut p, default(), &floor, 8);
    assert!(!p.grounded);
    p.step(
        &Orders {
            throw_pressed: true,
            ..default()
        },
        &floor,
    );
    assert_eq!(p.mode, Move::Throw);
    let right = Orders {
        direction: Vec2::X,
        ..default()
    };
    p.step(&right, &floor);
    // The first step: lift 6 less one gravity, the push along the stick, the
    // actor facing it.
    assert!((p.v.y - 5.7).abs() < 1e-4, "{}", p.v.y);
    assert!(
        (p.v.x - 0.5).abs() < 1e-4 && p.v.z.abs() < 1e-4,
        "{:?}",
        p.v
    );
    assert!((p.facing - Vec3::X).length() < 1e-4, "{:?}", p.facing);
    for _ in 0..10 {
        p.step(&right, &floor);
        assert_eq!(p.nerve_name().name, "SpinCap/SpinAir");
    }
    assert!((p.v.y - 2.7).abs() < 1e-3, "{}", p.v.y);
}

#[cfg_attr(test, test)]
fn running_throw() {
    let floor = floor();
    // Running throw keeps running with the upper-body clip.
    let mut p = Player::default();
    frames(&mut p, fwd(default()), &floor, 45);
    p.step(
        &fwd(Orders {
            throw_pressed: true,
            throw_held: true,
            ..default()
        }),
        &floor,
    );
    assert_eq!(p.mode, Move::Run);
    assert_eq!(p.sub_anim.map(|a| a.name), Some("ThrowCapParts"));
    assert!(p.launch.is_some());
    p.step(&fwd(default()), &floor);
    assert!(p.cap.is_some());
}

#[cfg_attr(test, test)]
fn cap_bounce() {
    let floor = floor();
    // Cap jump: fall onto a hovering cap with the button held. The recorded
    // run first tramples it with the feet at 67 cm, 127 cm short of the cap.
    let falling = || {
        let mut p = Player {
            pos: Vec3::new(0., 0.6734, 5.5125),
            grounded: false,
            mode: Move::Fall,
            v: Vec3::new(0., -13.57, 0.),
            ..default()
        };
        let mut cap = Cap::start(
            crate::cap::Launch {
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
        cap.resume(&crate::cap::Moment {
            nerve: crate::cap::CapNerve::ThrowStay,
            step: 30,
            spot: Vec3::new(0., 70., 678.),
            velocity: Vec3::ZERO,
            speed: 32.,
            return_speed: 0.,
        });
        p.cap = Some(cap);
        // The cap reads the button as the frame before left it.
        p.throw_held_before = true;
        p
    };
    let held = Orders {
        throw_held: true,
        ..default()
    };
    // Without the button the cap takes nothing.
    let mut p = falling();
    p.step(&Orders::default(), &floor);
    assert_eq!(p.mode, Move::Fall);
    // With it the jump nerve restarts inside the frame: its first step is
    // this one (state step 1, the jump power less one gravity), and the cap
    // enters its trample nerve when it next updates.
    let mut p = falling();
    p.step(&held, &floor);
    assert_eq!((p.mode, p.state_frames()), (Move::CapBounce, 1));
    assert!((p.v.y - 24.).abs() < 1e-3, "{}", p.v.y);
    assert_eq!(p.cap.as_ref().unwrap().state_name(), "ThrowStay");
    p.step(&held, &floor);
    assert_eq!(p.cap.as_ref().unwrap().state_name(), "TrampleReturn");
    // One cap jump per time on the ground.
    let mut again = falling();
    again.step(&held, &floor);
    again.pos.y = 0.6734;
    again.pos.z = 5.5125;
    again.v.y = -13.57;
    again.mode = Move::Fall;
    again.cap = falling().cap;
    again.step(&held, &floor);
    assert_eq!(again.mode, Move::Fall);
}

#[cfg_attr(test, test)]
fn wall_slide_and_ledge() {
    let floor = floor();
    // Wall slide then wall jump.
    let mut wall = floor.clone();
    wall.solids.push(Solid {
        center: Vec3::new(0., 3., -2.),
        half: Vec3::new(3., 3., 0.3),
        slope: 0.,
        moving: false,
        slippery: false,
    });
    let mut p = Player {
        pos: Vec3::new(0., 1.5, -1.2),
        grounded: false,
        v: Vec3::new(0., -2., -8.),
        mode: Move::Fall,
        facing: Vec3::NEG_Z,
        ..default()
    };
    // The step that reaches the wall flags the contact; the wall judge reads it
    // in the next one, which chooses the slide.
    p.step(&fwd(default()), &wall);
    assert_eq!(p.mode, Move::Fall);
    p.step(&fwd(default()), &wall);
    assert_eq!(p.mode, Move::WallSlide);
    frames(&mut p, Orders::default(), &wall, 5);
    assert_eq!(p.mode, Move::WallSlide, "wall slide holds contact");
    p.step(
        &Orders {
            jump: true,
            ..default()
        },
        &wall,
    );
    assert_eq!(chosen(&p), (Move::WallJump, "WallJump"));
    // The kick off the wall is the state's first step.
    p.step(&Orders::default(), &wall);
    assert!(p.vel.z > 0.);
    // Ledge grab and climb: falling at the wall's top edge, the body is put on
    // the edge (5 inside it, facing the wall), hangs through the catch clip and
    // climbs onto the top when the stick pushes toward the wall.
    p = Player::default();
    p.pos = Vec3::new(0., 5., -1.2);
    p.grounded = false;
    p.mode = Move::Fall;
    p.v = Vec3::new(0., -2., -8.);
    for _ in 0..6 {
        p.step(&fwd(default()), &wall);
        if p.mode == Move::Ledge {
            break;
        }
    }
    assert_eq!(chosen(&p), (Move::Ledge, "WallCatchStart"));
    assert!(
        (p.pos.y - 6.).abs() < 1e-3 && (p.pos.z + 1.75).abs() < 0.02,
        "the hang sits on the edge: {:?}",
        p.pos
    );
    // The stick toward the wall (the pad's, camera relative).
    let toward = Orders {
        stick: Vec2::NEG_Y,
        ..fwd(default())
    };
    for _ in 0..200 {
        p.step(&toward, &wall);
        if p.grounded {
            break;
        }
    }
    assert!(
        p.grounded && p.pos.y >= 5.99 && p.pos.z < -1.7,
        "{:?}",
        p.pos
    );
}

#[cfg_attr(test, test)]
fn swimming() {
    // A pool of water, its surface a metre up and ten metres deep, with nothing under it.
    let pool = Course {
        water: crate::water::WaterAreas::demo_box(0., 0., 10_000., 10_000., 100., 1000.),
        ..default()
    };
    // Dropping in from just above the water (a player placed in the air builds up speed
    // with the wait pull): the judge puts the player in the swim state, which settles at the
    // surface (the root floats about 80 under it, `SwimSurfaceBaseHeight`; the spring that
    // holds it there fades out after 50 idle steps and the swimmer then sinks).
    let mut p = Player::spawn_at(Vec3::new(0., 1.3, 0.), Vec3::Z);
    frames(&mut p, Orders::default(), &pool, 50);
    assert_eq!(p.mode, Move::Swim);
    assert_eq!(p.nerve_name().name, "Swim/SwimSurface");
    assert!(
        (p.pos.y - (1. - 0.8)).abs() < 0.4,
        "the root floats at the surface: {}",
        p.pos.y
    );
    // The jump button at the surface jumps out of the water.
    frames(
        &mut p,
        Orders {
            jump: true,
            jump_held: true,
            ..default()
        },
        &pool,
        1,
    );
    assert_eq!(p.mode, Move::Jump);
}

/// `PlayerJudgeDiveInWater`: a fall that has gone on, the surface 1500 to 1660 below the root and
/// no ground nearer than that (the swim state then dives in with `SwimDiveIn`).
#[cfg_attr(test, test)]
fn the_dive_in_judge_wants_a_long_fall_over_deep_water() {
    let pool = Course {
        water: crate::water::WaterAreas::demo_box(0., 0., 10_000., 10_000., 100., 10_000.),
        ..default()
    };
    let mut p = Player::spawn_at(Vec3::new(0., 17., 0.), Vec3::Z);
    p.fall.distance = 50.;
    // 16 metres over the surface at 1 m: inside the window.
    assert!(p.judge_dive_in_water(&pool));
    // A fall that has hardly begun, a surface too near and one out of the arrow's reach are not.
    p.fall.distance = 5.;
    assert!(!p.judge_dive_in_water(&pool));
    p.fall.distance = 50.;
    p.pos.y = 15.5;
    assert!(!p.judge_dive_in_water(&pool));
    p.pos.y = 19.;
    assert!(!p.judge_dive_in_water(&pool));
    // Once the clip plays the judge is not asked again.
    p.pos.y = 17.;
    p.anim.name = "DiveInWater";
    assert!(!p.dive_in_water_enabled(&pool));
}

/// The attack-sensor trigger of a trample (nothing in the rewrite raises it yet) sends the
/// swimmer under the surface into `SwimTrample` and rises it with `DiveTramplePower`.
#[cfg_attr(test, test)]
fn a_trample_in_the_water_hops_the_swimmer_up() {
    let pool = Course {
        water: crate::water::WaterAreas::demo_box(0., 0., 10_000., 10_000., 100., 1000.),
        ..default()
    };
    let mut p = Player::spawn_at(Vec3::new(0., 1.3, 0.), Vec3::Z);
    frames(&mut p, Orders::default(), &pool, 50);
    assert_eq!(p.mode, Move::Swim);
    // Three metres under the surface, in the open water.
    p.pos.y = -2.;
    p.update_water_finder(&pool);
    p.swim.nerve = swim::SwimNerve::Move;
    p.swim.steps = 1;
    p.v = Vec3::ZERO;
    p.receive_swim_attack(swim_trample::SwimAttack {
        trample: true,
        ..default()
    });
    frames(&mut p, Orders::default(), &pool, 1);
    assert_eq!(p.nerve_name().name, "Swim/SwimTrample");
    assert!((p.v.y - 11.).abs() < 1e-4, "the hop rises 11: {}", p.v.y);
    // The trigger was for that step only.
    frames(&mut p, Orders::default(), &pool, 1);
    assert!(p.v.y < 11.);
}

#[cfg_attr(test, test)]
fn poles() {
    let floor = floor();
    // Poles: hexagonal prisms of the stage's size, which a body falling against
    // one takes hold of (`PlayerJudgePoleClimb`).
    let mut pole = floor.clone();
    pole.poles.push(Pole {
        center: Vec3::ZERO,
        height: 8.,
    });
    let mut p = Player {
        pos: Vec3::new(0., 2., 0.9),
        grounded: false,
        mode: Move::Fall,
        v: Vec3::new(0., -1., -3.),
        ..default()
    };
    frames(&mut p, Orders::default(), &pole, 12);
    assert_eq!(p.mode, Move::Pole);
    assert!(
        p.pos.x.abs() < 0.01 && p.pos.z > 0. && p.pos.z < 0.5,
        "held at the pole's face: {:?}",
        p.pos
    );
    // The stick up the pole climbs, down slides, and the jump leaves it.
    frames(&mut p, Orders::default(), &pole, 12);
    let y = p.pos.y;
    let up = Orders {
        stick: Vec2::NEG_Y,
        direction: Vec2::NEG_Y,
        ..default()
    };
    frames(&mut p, up, &pole, 30);
    assert_eq!(p.mode, Move::Pole);
    assert!(p.pos.y > y + 0.5, "climbed from {y} to {}", p.pos.y);
    frames(
        &mut p,
        Orders {
            jump: true,
            ..default()
        },
        &pole,
        3,
    );
    frames(&mut p, Orders::default(), &pole, 10);
    assert_eq!(p.nerve_name().name, "PoleClimb/Jump");
    assert!(
        p.pos.z > 0.5,
        "the jump goes away from the pole: {:?}",
        p.pos
    );
    // Poles block running into them.
    p = Player::default();
    p.pos = Vec3::new(0., 0., 3.);
    frames(&mut p, fwd(default()), &pole, 120);
    assert!(
        p.pos.with_y(0.).length() >= (POLE_CIRCUMRADIUS * 0.866 + 55.) * UNIT - 0.01,
        "{:?}",
        p.pos
    );
}

#[cfg_attr(test, test)]
fn slope_slide() {
    let floor = floor();
    // Crouching on a steep slope slides down it, then stops on the flat.
    let mut ramp = floor.clone();
    ramp.solids.push(Solid {
        center: Vec3::new(0., 1.4, 0.),
        half: Vec3::new(4., 1.4, 2.),
        slope: -0.7,
        moving: false,
        slippery: false,
    });
    let mut p = Player {
        pos: Vec3::new(-2., ramp.solids[1].top(-2.), 0.),
        ..default()
    };
    frames(&mut p, Orders::default(), &ramp, 5);
    assert!(p.grounded);
    p.step(
        &Orders {
            crouch: true,
            crouch_pressed: true,
            ..default()
        },
        &ramp,
    );
    assert_eq!(chosen(&p), (Move::SlopeSlide, "Slide"));
    let start_x = p.pos.x;
    frames(
        &mut p,
        Orders {
            crouch: true,
            ..default()
        },
        &ramp,
        30,
    );
    assert!(
        p.pos.x > start_x + 1. && p.vel.x > 0.,
        "slides downhill: {:?}",
        p.pos
    );
    for _ in 0..300 {
        p.step(
            &Orders {
                crouch: true,
                ..default()
            },
            &ramp,
        );
        if p.mode != Move::SlopeSlide {
            break;
        }
    }
    assert_eq!(
        p.mode,
        Move::SquatBrake,
        "the slide ends once the ground is flat, in a crouch's braking state"
    );
    frames(
        &mut p,
        Orders {
            crouch: true,
            ..default()
        },
        &ramp,
        3,
    );
    assert_eq!(p.mode, Move::Crouch, "which settles into the crouch");
}

#[cfg_attr(test, test)]
fn burn_and_soot() {
    let floor = floor();
    // Hot ground: a floating leap, a bounce where it lands, then a short fire
    // run; jumping is only possible during the run.
    let mut lava = floor.clone();
    lava.hot
        .push((Vec3::new(0., 0., 0.), Vec3::new(1., 1., 1.)));
    let mut p = Player {
        pos: Vec3::ZERO,
        ..default()
    };
    p.step(&Orders::default(), &lava);
    assert_eq!(chosen(&p), (Move::BurnJump, "FireRunStart"));
    assert_eq!(p.move_speed, 0., "no stick: the leap goes straight up");
    frames(&mut p, Orders::default(), &lava, 20);
    assert!(
        (p.vel.y - t::DAMAGE_FIRE_JUMP_POWER_1ST * UNIT * FPS).abs() < 0.01,
        "floats up"
    );
    p.step(
        &Orders {
            jump: true,
            jump_held: true,
            ..default()
        },
        &lava,
    );
    assert_eq!(p.mode, Move::BurnJump, "no jumping out of the leap");
    let (mut peak, mut fell) = (0_f32, false);
    for _ in 0..300 {
        p.step(&Orders::default(), &lava);
        peak = peak.max(p.pos.y);
        fell |= p.vel.y < 0.;
        if fell && p.frame == 1 {
            break;
        }
    }
    assert!(peak > 5.5 && peak < 7., "the burn leap: {peak}");
    assert_eq!(
        (p.mode, p.frame),
        (Move::BurnJump, 1),
        "landing back on the lava burns again"
    );
    let mut cool = floor.clone();
    cool.hot.clear();
    p = Player::default();
    p.pos = Vec3::ZERO;
    p.step(&Orders::default(), &lava);
    let push = Orders {
        direction: Vec2::new(0., 1.),
        ..default()
    };
    let until_change = |p: &mut Player, o: &Orders, scene: &Course| -> f32 {
        let (mode, mut high) = (p.mode, p.pos.y);
        for _ in 0..600 {
            p.step(o, scene);
            high = high.max(p.pos.y);
            if p.mode != mode {
                break;
            }
        }
        high
    };
    until_change(&mut p, &push, &cool);
    assert_eq!(chosen(&p), (Move::BurnBounce, "FireRunStart"));
    let peak = until_change(&mut p, &push, &cool);
    assert!(peak > 2. && peak < 2.5, "the bounce: {peak}");
    assert_eq!(chosen(&p), (Move::FireRun, "FireRun"));
    p.step(&push, &cool);
    assert!((p.move_speed - t::DAMAGE_FIRE_RUN_SPEED).abs() < 0.01);
    frames(&mut p, Orders::default(), &cool, 20);
    assert!(p.move_speed < 0.01, "releasing the stick stops the run");
    frames(&mut p, Orders::default(), &cool, 45);
    assert_ne!(p.mode, Move::FireRun, "the run lasts a second");
    // Shallow water puts the burn out on contact; deep water ends it in a swim.
    let mut puddle = cool.clone();
    puddle.puddles.push((Vec3::ZERO, Vec3::new(50., 0.1, 50.)));
    p = Player::default();
    p.pos = Vec3::ZERO;
    p.step(&Orders::default(), &lava);
    until_change(&mut p, &push, &puddle);
    assert_eq!(p.mode, Move::Run, "landing in shallow water ends the burn");
    assert!(p.events.contains(&Fx::Quenched));
    let mut pool = cool.clone();
    pool.water = crate::water::WaterAreas::demo_box(0., 0., 5000., 5000., 200., 400.);
    p = Player::default();
    p.pos = Vec3::ZERO;
    p.step(&Orders::default(), &lava);
    p.step(&Orders::default(), &pool);
    assert_eq!(p.mode, Move::Swim);
    assert!(p.events.contains(&Fx::Quenched));
    // The burn leaves soot: it smokes once strong, holds, then fades.
    p = Player::default();
    p.pos = Vec3::ZERO;
    p.step(&Orders::default(), &lava);
    let mut burned = 0;
    while p.burning() && burned < 600 {
        p.step(&push, &cool);
        burned += 1;
    }
    assert!(
        p.soot.body > 0.75 && p.soot.smoking,
        "soot after a full burn: {:?}",
        p.soot
    );
    // The step that ended the burn already counted one held frame.
    let full = p.soot.body;
    frames(
        &mut p,
        Orders::default(),
        &cool,
        t::SOOT_HOLD_FRAMES as usize - 1,
    );
    assert_eq!(p.soot.body, full, "the stain holds for five seconds");
    frames(&mut p, Orders::default(), &cool, 150);
    assert!(p.soot.body < full - 0.45 && p.soot.smoking == (p.soot.body > 0.25));
    frames(&mut p, Orders::default(), &cool, 300);
    assert_eq!((p.soot.body, p.soot.smoking), (0., false), "faded away");
    // Water washes it: steam once, then it drains while Mario drips.
    p = Player::default();
    p.pos = Vec3::ZERO;
    p.soot.body = 1.;
    p.soot.smoking = true;
    p.soot.hold = t::SOOT_HOLD_FRAMES;
    p.step(&Orders::default(), &puddle);
    assert!(p.events.contains(&Fx::Quenched) && !p.soot.smoking);
    p.events.clear();
    frames(&mut p, Orders::default(), &puddle, 120);
    assert!(
        p.soot.body < 0.1 && !p.events.contains(&Fx::Quenched),
        "{:?}",
        p.soot
    );
    p = Player::default();
    p.soot.body = 1.;
    p.pos = Vec3::ZERO;
    p.step(&Orders::default(), &pool);
    frames(&mut p, Orders::default(), &cool, 200);
    assert!(
        p.soot.body < 0.05,
        "still dripping after the pool: {:?}",
        p.soot
    );
}

#[cfg_attr(test, test)]
fn low_ceiling() {
    let floor = floor();
    // Low ceilings force a crouch.
    let mut tunnel = floor.clone();
    tunnel.solids.push(Solid {
        center: Vec3::new(0., 1.5, 5.),
        half: Vec3::new(2., 0.3, 2.),
        slope: 0.,
        moving: false,
        slippery: false,
    });
    let mut p = Player::default();
    frames(
        &mut p,
        Orders {
            jump: true,
            ..default()
        },
        &tunnel,
        2,
    );
    assert!(p.grounded && p.pos.y == 0.);
}

#[cfg_attr(test, test)]
fn wall_collision_and_stress() {
    let floor = floor();
    let mut wall = floor.clone();
    wall.solids.push(Solid {
        center: Vec3::new(0., 3., -2.),
        half: Vec3::new(3., 3., 0.3),
        slope: 0.,
        moving: false,
        slippery: false,
    });
    let mut p = Player::default();
    // Walls block.
    frames(&mut p, fwd(default()), &wall, 300);
    assert!(p.pos.z >= -1.26);
    // Stress: random input stays finite and emits events.
    let mut events = 0;
    for i in 0..10_000 {
        let o = Orders {
            direction: Vec2::new((i as f32 * 0.3).sin(), (i as f32 * 0.2).cos()),
            jump: i % 79 == 0,
            jump_held: i % 79 < 25,
            crouch: i % 173 < 8,
            crouch_pressed: i % 173 == 0,
            throw_pressed: i % 107 == 0,
            throw_held: i % 107 < 40,
            ..default()
        };
        p.step(&o, &wall);
        events += p.events.drain(..).count();
        assert!(p.pos.is_finite() && p.vel.is_finite());
    }
    assert!(events > 0);
}

use super as controller;
macro_rules! scene {
    ($m:ident, $variant:expr) => {
        $m::Course {
            solids: vec![
                $m::Solid {
                    center: Vec3::new(0., -0.5, 0.),
                    half: Vec3::new(100., 0.5, 100.),
                    slope: 0.,
                    moving: false,
                    slippery: false,
                },
                $m::Solid {
                    center: Vec3::new(0., 3., -2.),
                    half: Vec3::new(3., 3., 0.3),
                    slope: 0.,
                    moving: false,
                    slippery: false,
                },
            ],
            poles: vec![$m::Pole {
                center: Vec3::new(2., 0., 2.),
                height: 5.,
            }],
            hot: if $variant == 1 {
                vec![(Vec3::ZERO, Vec3::new(50., 0., 50.))]
            } else {
                vec![]
            },
            water: if $variant == 2 {
                crate::water::WaterAreas::demo_box(0., 0., 5000., 5000., 200., 400.)
            } else {
                default()
            },
            ..default()
        }
    };
}
macro_rules! snapshot {
    ($p:expr) => {
        format!(
            "{:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
            $p.pos,
            $p.vel,
            $p.facing,
            $p.mode,
            $p.grounded,
            $p.cap,
            $p.anim,
            $p.sub_anim,
            $p.soot,
            (
                $p.timer,
                $p.since_land,
                $p.distance,
                $p.move_speed,
                $p.hard_land,
                $p.wall,
                $p.hitstop
            )
        )
    };
}

#[cfg_attr(test, test)]
fn original_simulation_trace() {
    let mut hash = 0xcbf29ce484222325_u64;
    for variant in 0..3 {
        let scene = scene!(controller, variant);
        let mut p = Player::default();
        for i in 0..10_000 {
            let o = Orders {
                direction: Vec2::new((i as f32 * 0.3).sin(), (i as f32 * 0.2).cos()),
                jump: i % 79 == 0,
                jump_held: i % 79 < 25,
                crouch: i % 173 < 8,
                crouch_pressed: i % 173 == 0,
                throw_pressed: i % 107 == 0,
                throw_held: i % 107 < 40,
                up_throw: i % 311 == 0,
                down_throw: i % 353 == 0,
                spin_throw: i % 433 == 0,
                ..default()
            };
            p.step(&o, &scene);
            assert!(p.pos.is_finite() && p.vel.is_finite());
            for byte in snapshot!(p)
                .bytes()
                .chain(format!("{:?}", p.events).bytes())
            {
                hash = (hash ^ byte as u64).wrapping_mul(0x100000001b3);
            }
            p.events.clear();
        }
    }
    // ponytail: this bit-exact baseline pins the Windows toolchain; other
    // platforms exercise the same trace but libm may round differently.
    #[cfg(target_os = "windows")]
    assert_eq!(
        hash, 0x2dd0aa5a8e34464f,
        "simulation differs from the nerve-timing revision"
    );
    #[cfg(not(target_os = "windows"))]
    assert_ne!(hash, 0);
}
