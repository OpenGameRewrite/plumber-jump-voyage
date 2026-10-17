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
    state_changes_follow_the_nerve_machine();
    run_speed();
    variable_jump_height();
    chained_jumps();
    brake_and_sideflip();
    crouch_jumps();
    ground_pound();
    dive_from_pound();
    dive_roll_landing();
    roll_timing_and_boost();
    cap_flight();
    spin_throw();
    running_throw();
    cap_bounce();
    wall_slide_and_ledge();
    swimming();
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
    assert_eq!(chosen(&p), (Move::Slide, "LandRolling"));
    // The state's first step starts the clip; it stands up once the clip is over.
    frames(
        &mut p,
        Orders::default(),
        &floor,
        clip::LAND_ROLLING as usize + 1,
    );
    assert_eq!(p.mode, Move::Idle, "the landing roll stands up");
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
    // The stick cuts it short once the cap is away (`isEnableCancelGround`),
    // but not while the cap still winds up.
    let moving = Orders {
        direction: Vec2::X,
        ..default()
    };
    frames(&mut q, moving.clone(), &floor, 5);
    assert_eq!(q.mode, Move::Throw, "the cap has not left yet");
    frames(&mut q, moving, &floor, 6);
    assert_ne!(q.mode, Move::Throw);
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
    // Cap jump: dive into a hovering cap.
    let mut p = Player::default();
    p.pos.y = 3.;
    p.grounded = false;
    p.mode = Move::Fall;
    let mut cap = Cap::start(
        crate::cap::Launch {
            front: Vec3::NEG_Z,
            kind: Throw::Forward,
            left: false,
            continuous: false,
        },
        Vec3::ZERO,
    );
    // Hovering where the player falls onto it.
    cap.resume(
        crate::cap::CapNerve::ThrowStay,
        5,
        (p.pos + Vec3::Y * 0.8) / UNIT,
        Vec3::ZERO,
        0.,
    );
    cap.frames = 30;
    p.cap = Some(cap);
    p.v.y = -5.;
    p.step(
        &Orders {
            throw_held: true,
            ..default()
        },
        &floor,
    );
    assert_eq!(chosen(&p), (Move::CapBounce, "JumpCapLeapFrog"));
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
        ..default()
    };
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
    // Ledge grab and climb.
    p = Player::default();
    p.pos = Vec3::new(0., 4.75, -1.2);
    p.grounded = false;
    p.mode = Move::Fall;
    p.v = Vec3::new(0., -2., -8.);
    p.step(&fwd(default()), &wall);
    assert_eq!(chosen(&p), (Move::Ledge, "WallCatchStart"));
    assert!(
        (p.pos.y - 6.).abs() < 1e-4 && (p.pos.z + 1.78).abs() < 0.01,
        "the hang sits on the edge: {:?}",
        p.pos
    );
    for _ in 0..200 {
        p.step(&fwd(default()), &wall);
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
    let floor = floor();
    // Water.
    let mut water = floor.clone();
    water.water = Some((Vec3::Y * 5., Vec3::splat(100.)));
    let mut p = Player::default();
    frames(
        &mut p,
        Orders {
            jump: true,
            jump_held: true,
            ..default()
        },
        &water,
        3,
    );
    assert!(matches!(p.mode, Move::Swim | Move::SwimDash));
}

#[cfg_attr(test, test)]
fn poles() {
    let floor = floor();
    // Poles.
    let mut pole = floor.clone();
    pole.poles.push(Pole {
        center: Vec3::ZERO,
        height: 8.,
    });
    let mut p = Player {
        pos: Vec3::new(0., 2., 0.3),
        grounded: false,
        mode: Move::Fall,
        ..default()
    };
    p.step(&Orders::default(), &pole);
    assert_eq!(p.mode, Move::Pole);
    assert!(
        p.pos.x.abs() < 1e-4 && p.pos.z.abs() < 1e-4,
        "held on the pole's axis"
    );
    frames(&mut p, Orders::default(), &pole, 10);
    frames(
        &mut p,
        Orders {
            direction: Vec2::X,
            ..default()
        },
        &pole,
        30,
    );
    assert_eq!(p.mode, Move::Pole);
    assert!(
        p.pos.x.abs() < 1e-4 && p.pos.z.abs() < 1e-4,
        "circling stays on the pole"
    );
    assert!(p.facing.x.abs() > 0.3, "circling turns round the pole");
    let y = p.pos.y;
    frames(&mut p, fwd(default()), &pole, 5);
    assert!(p.pos.y > y);
    // Poles block running into them.
    p = Player::default();
    p.pos = Vec3::new(0., 0., 3.);
    frames(&mut p, fwd(default()), &pole, 120);
    assert!(
        p.pos.with_y(0.).length() >= POLE_RADIUS + RADIUS - 0.01,
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
        p.pos.x > start_x + 1.5 && p.vel.x > 0.,
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
    pool.water = Some((Vec3::new(0., 2., 0.), Vec3::new(50., 4., 50.)));
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
                Some((Vec3::new(0., 2., 0.), Vec3::new(50., 4., 50.)))
            } else {
                None
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
        hash, 0x52c5cd2273e2181b,
        "simulation differs from the nerve-timing revision"
    );
    #[cfg(not(target_os = "windows"))]
    assert_ne!(hash, 0);
}
