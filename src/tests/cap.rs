use super::*;

pub(super) fn run() {
    lockstep_retains_the_stay_reaction_latch();
    lockstep_rewinds_the_cap_clip_with_its_nerve();
    lockstep_spiral_entry_retains_its_launch_front();
    lockstep_keeps_the_cap_water_and_throw_parameters();
    lockstep_return_keeps_the_completed_pose_and_action();
    straight_throw_matches_the_recording();
    a_wall_at_an_angle_turns_the_cap();
    the_return_after_a_trample_leaves_along_the_cap_pose();
    the_return_writes_the_pose_from_its_first_execution();
    a_cap_flying_a_fixed_front_alternates_its_pose_by_an_ulp();
    the_catch_is_the_sensor_contact();
    the_spiral_circles_the_player();
    the_spiral_calls_the_terrain_helper_and_circles_about_its_up_axis();
    the_throw_down_rolls_along_the_floor();
    the_helper_makes_the_front_a_unit_vector_in_its_easing_only();
    the_near_zero_tests_are_strict();
    the_collision_turn_follows_the_binarys_gates();
    the_collision_turn_lays_the_front_and_velocity_in_the_new_plane();
    a_cap_stood_on_its_up_axis_faces_back_along_its_front();
    a_cap_thrown_into_a_slope_turns_onto_it_and_raises_the_reaction();
    the_returns_tail_counts_walls_and_ends_by_step();
    a_return_that_never_arrives_hides_the_cap_after_180_steps();
    a_return_pressed_into_a_wall_ends_when_the_counter_passes_45();
    the_rolling_stand_follows_the_throw_type_not_the_rolling_state();
    the_cap_thrown_from_a_slope_flies_along_the_slope();
}

#[cfg_attr(test, test)]
fn lockstep_return_keeps_the_completed_pose_and_action() {
    // exeReturn tries the existing flight clip (0x406fc8) and steers from
    // the retained model pose (0x4071f8..0x40735c), with its own speed scalar.
    let launch = Launch {
        front: Vec3::NEG_Z,
        kind: Throw::Spin,
        left: false,
        air: false,
        continuous: false,
        spiral_side: -1.,
        up: Vec3::Y,
    };
    let mut before = Cap::start(launch, Vec3::ZERO, 1);
    let world = CapWorld {
        player: Vec3::ZERO,
        hold: false,
        mesh: None,
        water: None,
        targets: &[],
    };
    for _ in 0..cap_spiral::FRAMES {
        before.update(&world);
    }
    assert_eq!(before.nerve, CapNerve::Return);
    assert_eq!(before.step, 0);
    for _ in 0..4 {
        let mut restored = Cap::start(launch, Vec3::ZERO, 1);
        restored.resume(&Moment {
            nerve: before.nerve,
            step: before.step,
            spot: before.spot,
            velocity: before.velocity,
            speed: before.speed,
            // Position deltas also contain the incoming spiral's lateral velocity.
            return_speed: before.velocity.length(),
            anchor: before.spot,
            after_lock_on: false,
        });
        restored.continue_clip(&before);
        let expected_events = before.update(&world);
        assert_eq!(restored.update(&world), expected_events);
        assert_eq!(restored.spot, before.spot);
        assert_eq!(restored.clip_frame(), before.clip_frame());
    }
}

#[cfg_attr(test, test)]
fn lockstep_keeps_the_cap_water_and_throw_parameters() {
    // The water keeper and launch parameters survive position injection;
    // movement across the surface is not a new throw (0x4024d0).
    let launch = Launch {
        front: Vec3::Z,
        kind: Throw::Forward,
        left: false,
        air: false,
        continuous: false,
        spiral_side: 0.,
        up: Vec3::Y,
    };
    let mut before = Cap::start(launch, Vec3::ZERO, 1);
    before.water.submerged = true;
    before.water.splash_found = true;
    before.water.splash_area = true;
    before.water.fly_effect = true;
    let mut restored = Cap::start(
        Launch {
            continuous: true,
            ..launch
        },
        Vec3::ZERO,
        2,
    );
    restored.continue_clip(&before);
    assert_eq!(restored.speed, before.speed);
    assert_eq!(restored.cruise_frames, before.cruise_frames);
    assert_eq!(restored.brake_step, before.brake_step);
    assert_eq!(restored.capcatch, before.capcatch);
    assert!(restored.water.submerged && restored.water.splash_found);
    assert!(restored.water.splash_area && restored.water.fly_effect);
}

#[cfg_attr(test, test)]
fn lockstep_spiral_entry_retains_its_launch_front() {
    // exeThrowSpiral copies +0x240 to +0x300 on its first execution
    // (0x403a58..0x403a78). Placement displacement is not the launch front.
    let launch = Launch {
        front: Vec3::NEG_Z,
        kind: Throw::Spin,
        left: false,
        air: false,
        continuous: false,
        spiral_side: -1.,
        up: Vec3::Y,
    };
    let mut before = Cap::start(launch, Vec3::ZERO, 1);
    let mut restored = Cap::start(
        Launch {
            front: Vec3::X,
            ..launch
        },
        Vec3::ZERO,
        2,
    );
    restored.spot = before.spot;
    restored.pos = before.pos;
    restored.continue_clip(&before);
    let world = CapWorld {
        player: Vec3::ZERO,
        hold: false,
        mesh: None,
        water: None,
        targets: &[],
    };
    before.update(&world);
    restored.update(&world);
    assert_eq!(restored.spot, before.spot);
    assert_eq!(restored.curve_front, before.curve_front);
}

#[cfg_attr(test, test)]
fn lockstep_retains_the_stay_reaction_latch() {
    // HackCap::exeThrowStay's +0x2a9 latch survives for the throw
    // (0x405d4c..0x405d6c), including a release and another press.
    let mut before = returning_at(0, Vec3::new(0., 70., 1000.));
    before.nerve = CapNerve::ThrowStay;
    let mut world = far_world(None);
    world.hold = true;
    assert!(before.update(&world).contains(&CapEvent::StayStarted));
    let mut restored = returning_at(0, before.spot);
    restored.nerve = CapNerve::ThrowStay;
    restored.step = before.step;
    restored.continue_clip(&before);
    world.hold = false;
    restored.update(&world);
    world.hold = true;
    assert!(!restored.update(&world).contains(&CapEvent::StayStarted));
}

#[cfg_attr(test, test)]
fn lockstep_rewinds_the_cap_clip_with_its_nerve() {
    // Re-choice injects the row before the last completed one. A continuing
    // Stay action is one update older too (exeThrowStay, 0x4055cc).
    let mut before = returning_at(0, Vec3::new(0., 70., 1000.));
    before.nerve = CapNerve::ThrowStay;
    let world = far_world(None);
    for _ in 0..8 {
        before.update(&world);
    }
    let mut restored = returning_at(0, before.spot);
    restored.nerve = CapNerve::ThrowStay;
    restored.step = before.step - 1;
    restored.action = before.action;
    restored.continue_clip(&before);
    assert_eq!(restored.clip_frame(), before.clip_frame() - 1.);
    restored.update(&world);
    assert_eq!(restored.clip_frame(), before.clip_frame());
    assert_eq!(restored.clip_serial(), before.clip_serial());
}

/// The throw down of the recorded `cap-throw-rolling-down` run, on the plane the player stood on:
/// the cap appears 150 ahead and drops onto the floor (its sphere rests 60 up), rolls at 30 for
/// 19 steps, brakes by one a step for 30 and stays at 1155, then returns straight at the head.
#[cfg_attr(test, test)]
fn the_throw_down_rolls_along_the_floor() {
    let mut cap = Cap::start(
        Launch {
            front: Vec3::Z,
            kind: Throw::Down,
            left: false,
            air: false,
            continuous: false,
            spiral_side: 0.,
            up: Vec3::Y,
        },
        Vec3::ZERO,
        1,
    );
    let world = CapWorld {
        player: Vec3::ZERO,
        hold: false,
        mesh: None,
        water: None,
        targets: &[],
    };
    let at = |cap: &Cap| cap.pos / UNIT;
    // Row 74: the start, the drop onto the floor and the clip the start began.
    assert!((at(&cap) - Vec3::new(0., 60., 150.)).length() < 1e-3);
    assert_eq!(
        (cap.state_name(), cap.clip()),
        ("ThrowRolling", "FlyingWaitR")
    );
    let mut log = vec![];
    for _ in 75..=145 {
        cap.update(&world);
        log.push((cap.state_name(), at(&cap).z, cap.clip()));
    }
    // Rows 75 to 93 roll 30 a step (row 93 is the last step of the rolling nerve, already
    // named the brake), and the first of them plays the rolling clip.
    assert_eq!(log[0], ("ThrowRolling", 180., "FlyingWaitRolling"));
    assert!((log[18].1 - 720.).abs() < 1e-3 && log[18].0 == "ThrowRollingBrake");
    // Rows 94 to 122 brake 29, 28 ... 1; row 123 has stopped at 1155 and names the stay.
    assert!((log[19].1 - 749.).abs() < 1e-3);
    assert!((log[47].1 - 1155.).abs() < 1e-3 && log[47].0 == "ThrowRollingBrake");
    assert_eq!(log[48].0, "ThrowStay");
    assert!((log[48].1 - 1155.).abs() < 1e-3);
    // The stay runs 22 steps (rows 123 to 144); row 145 chooses the return.
    assert_eq!(log[70].0, "Return");
    assert!(!cap.grounding_effect().0, "the effect ends with the return");
}

/// The spin-move throw of the recorded `cap-throw-spiral` run (rows 54 to 58, the player
/// `UNIT`-scaled to where each row's cap update saw him): the cap appears 150 away along the
/// throw direction turned 45 degrees, with no velocity, then each step goes to the point on the
/// widening circle round the player, and the nerve names itself.
#[cfg_attr(test, test)]
fn the_spiral_circles_the_player() {
    let at = |x: f32, z: f32| Vec3::new(x, 0., z) * UNIT;
    let mut cap = Cap::start(
        Launch {
            front: Vec3::new(-0.587_803, 0., -0.809_004),
            kind: Throw::Spin,
            left: false,
            air: false,
            continuous: false,
            spiral_side: -1.,
            up: Vec3::Y,
        },
        at(11.258_301, 68.216_8),
        1,
    );
    let spot = |cap: &Cap| (cap.pos / UNIT, cap.state_name(), cap.clip());
    let (start, name, clip) = spot(&cap);
    assert!((start - Vec3::new(-136.895_5, 70., 44.753_9)).length() < 0.01);
    assert_eq!((name, clip), ("ThrowSpiral", "FlyingWaitR"));
    // The rows 55 to 58: where the player stood after the row before, and the cap's recorded spot.
    let rows = [
        ((10.680_664, 69.775_39), (-198.708, -36.908)),
        ((10.076_172, 71.296_88), (-245.405, -76.197)),
        ((9.45, 72.78), (-288.29, -120.56)),
        ((8.79, 74.24), (-326.96, -169.69)),
    ];
    for ((px, pz), (x, z)) in rows {
        let world = CapWorld {
            player: at(px, pz),
            hold: false,
            mesh: None,
            water: None,
            targets: &[],
        };
        assert!(cap.update(&world).is_empty());
        let (found, name, clip) = spot(&cap);
        assert!(
            (found - Vec3::new(x, 70., z)).length() < 0.05,
            "{found} {x} {z}"
        );
        assert_eq!((name, clip), ("ThrowSpiral", "FlyingWaitSpiral"));
    }
}

/// `exeThrowSpiral` calls the throw nerves' terrain helper (0x401d0c) every step, as `exeThrow`
/// and `exeThrowBrake` do (its step counter counts up while the cap is not low over the ground),
/// and circles about the cap's own up axis, which the helper eases toward the world's: the front
/// the step leaves is across that axis, not across the world's.
#[cfg_attr(test, test)]
fn the_spiral_calls_the_terrain_helper_and_circles_about_its_up_axis() {
    let world = CapWorld {
        player: Vec3::ZERO,
        hold: false,
        mesh: None,
        water: None,
        targets: &[],
    };
    let tilted = Vec3::new(0.6, 0.8, 0.);
    let mut cap = Cap::start(
        Launch {
            front: Vec3::new(0., 0., 1.),
            kind: Throw::Spin,
            left: false,
            air: false,
            continuous: false,
            spiral_side: 1.,
            up: tilted,
        },
        Vec3::ZERO,
        1,
    );
    assert_eq!(cap.ground_counter, 0);
    for step in 1..=3 {
        cap.update(&world);
        assert_eq!(cap.ground_counter, step);
        // The front the spiral keeps is across the up axis of this step's end.
        assert!(cap.front.dot(cap.up).abs() < 1e-3, "{:?}", cap.front);
    }
    // The easing starts at the third step of the helper (a share of the way that grows from the
    // second): the up axis is no longer the one the throw began with.
    assert!(cap.up.dot(Vec3::Y) > tilted.dot(Vec3::Y));
}

/// The catch is the contact of the cap's `Catch` sensor (100) with the player's
/// `Body` sensor (50, centred 75 above the feet), read after the cap moved: a
/// cap returning at full speed is caught from 150 plus its step away.
#[cfg_attr(test, test)]
fn the_catch_is_the_sensor_contact() {
    let returning = |z: f32| {
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
            nerve: CapNerve::Return,
            step: 6,
            spot: Vec3::new(0., 75., z),
            velocity: Vec3::new(0., 0., -80.),
            speed: 32.,
            return_speed: 80.,
            anchor: Vec3::ZERO,
            after_lock_on: false,
        });
        cap
    };
    let world = CapWorld {
        player: Vec3::ZERO,
        hold: false,
        mesh: None,
        water: None,
        targets: &[],
    };
    // 220 away it comes to about 140 from the body in this frame: touching.
    assert!(returning(220.).update(&world).contains(&CapEvent::Caught));
    // 250 away it only comes to about 170: not yet.
    assert!(!returning(250.).update(&world).contains(&CapEvent::Caught));
    // A cap approaching from above reaches Head before Body. Both sensors
    // have type Player in InitSensor and may send PlayerCapCatch.
    let mut above = returning(0.);
    above.spot = Vec3::new(0., 250., 0.);
    above.velocity = Vec3::NEG_Y;
    above.return_speed = 0.;
    above.pose.stand_facing(Vec3::NEG_Y);
    assert!(above.update(&world).contains(&CapEvent::Caught));
}

/// The standing throw of the recorded `capthrow` run: the cap appears 150 in
/// front plus its first step, flies 32 a frame, brakes 3.2 a frame, stays
/// for 22 frames and comes back 18, 36, 54, 72, 80 ... faster.
#[cfg_attr(test, test)]
fn straight_throw_matches_the_recording() {
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
    let world = CapWorld {
        player: Vec3::ZERO,
        hold: false,
        mesh: None,
        water: None,
        targets: &[],
    };
    let z = |cap: &Cap| cap.pos.z / UNIT;
    assert!((z(&cap) - 182.).abs() < 1e-3 && (cap.pos.y / UNIT - 70.).abs() < 1e-3);
    let mut log = vec![(cap.state_name(), z(&cap))];
    let mut moments = vec![];
    for row in 1..=60 {
        for moment in cap.update(&world) {
            moments.push((row + 24, moment));
        }
        log.push((cap.state_name(), z(&cap)));
    }
    // Row 25 is the throw nerve's first step; the stay's 22 steps are rows
    // 46 to 67, the last one choosing the return, and the cap arrives in row 76.
    assert_eq!(moments[0], (25, CapEvent::Thrown));
    let stays: Vec<_> = moments
        .iter()
        .filter(|m| m.1 == CapEvent::Staying)
        .map(|m| m.0)
        .collect();
    assert_eq!(stays, (46..=67).collect::<Vec<_>>());
    assert!(moments.contains(&(67, CapEvent::LeftStay)));
    let caught = moments.iter().find(|m| m.1 == CapEvent::Caught);
    assert_eq!(caught, Some(&(76, CapEvent::Caught)));
    // The clips restart when the nerves start theirs: flying, staying, returning.
    assert_eq!(cap.clip(), "FlyingWaitR");
    assert_eq!(cap.clip_serial(), 3);
    // Row 24 is the start; rows 25..34 fly, row 35 brakes its first frame.
    assert_eq!(log[10], ("Throw", 502.));
    assert_eq!(log[11].0, "ThrowBrake");
    assert!((log[11].1 - 534.).abs() < 1e-2);
    assert!((log[12].1 - 562.8).abs() < 1e-2);
    assert!((log[20].1 - 678.).abs() < 1e-2);
    assert_eq!(log[21].0, "ThrowStay");
    assert_eq!(log[43].0, "Return");
    assert!((log[44].1 - 660.0).abs() < 0.1);
}

/// A cap thrown at a wall at an angle stops short of it by its radius (60),
/// turns along it at the speed it had and raises the collision reaction once.
#[cfg_attr(test, test)]
fn a_wall_at_an_angle_turns_the_cap() {
    // A wall at x = 500 facing -x, tall enough for the cap at 70.
    let mesh = TriangleMesh::from_corners(&[
        [
            Vec3::new(500., -100., -2000.),
            Vec3::new(500., 300., 2000.),
            Vec3::new(500., 300., -2000.),
        ],
        [
            Vec3::new(500., -100., -2000.),
            Vec3::new(500., -100., 2000.),
            Vec3::new(500., 300., 2000.),
        ],
    ]);
    assert!(mesh.triangles.iter().all(|t| t.normal.x < -0.99));
    let mut cap = Cap::start(
        Launch {
            front: Vec3::new(1., 0., 1.).normalize(),
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
    let world = CapWorld {
        player: Vec3::ZERO,
        hold: false,
        mesh: Some(&mesh),
        water: None,
        targets: &[],
    };
    let mut hits = 0;
    let mut deepest = f32::MIN;
    for _ in 0..40 {
        for event in cap.update(&world) {
            if matches!(event, CapEvent::WallHit(_)) {
                hits += 1;
            }
        }
        deepest = deepest.max(cap.pos.x / UNIT);
    }
    assert_eq!(hits, 1);
    // The sphere's radius keeps the cap's position 60 from the wall.
    assert!(deepest <= 440.01, "reached x = {deepest}");
    // Free flight at 45 degrees ends near z = 480; turned along the wall it goes on.
    assert!(cap.pos.z / UNIT > 485., "slid to z = {}", cap.pos.z / UNIT);
}

/// `exeReturn` writes the pose on every execution, its first one included (the lean at 0x4071f8,
/// the stand at 0x407350). Whatever pose the cap had, it leaves the upright one facing the front
/// it flew along, made from that front alone, both within the lean's 200 of the head and beyond.
#[cfg_attr(test, test)]
fn the_return_writes_the_pose_from_its_first_execution() {
    use crate::movement::pose_math::{NEGATED_GRAVITY, quat_front_up};
    let world = CapWorld {
        player: Vec3::ZERO,
        hold: false,
        mesh: None,
        water: None,
        targets: &[],
    };
    for distance in [150., 300.] {
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
            nerve: CapNerve::Return,
            step: 0,
            spot: Vec3::new(distance, 75., distance),
            velocity: Vec3::ZERO,
            speed: 32.,
            return_speed: 0.,
            anchor: Vec3::ZERO,
            after_lock_on: false,
        });
        let thrown = cap.pose.quat();
        cap.update(&world);
        assert_ne!(cap.pose.quat(), thrown, "{distance}");
        assert_eq!(
            cap.pose.quat(),
            quat_front_up(cap.front, NEGATED_GRAVITY),
            "{distance}"
        );
    }
}

/// The cap-bounce run: Cappy hovers 678 ahead, the player lands on it, and after the fifteen
/// steps of the trample the cap flies back. It does not head straight for the player: the pose it
/// has (facing back at the thrower) leans a quarter of the way toward the head and then turns
/// toward it by 40 degrees at most, so the first step goes sideways and up at 18 a frame and
/// the cap arcs. Positions are the recording's (centimetres, rows 112 to 116).
#[cfg_attr(test, test)]
fn the_return_after_a_trample_leaves_along_the_cap_pose() {
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
    let centimetres = |y: f32, z: f32| Vec3::new(0., y * UNIT, z * UNIT);
    let mut world = CapWorld {
        player: centimetres(339.343_5, 789.248_05),
        hold: true,
        mesh: None,
        water: None,
        targets: &[],
    };
    while cap.state_name() != "ThrowStay" {
        cap.update(&world);
    }
    let foot = cap.pos / UNIT + Vec3::Y * 30.;
    assert!(cap.take_trample(foot, true, true));
    while cap.state_name() != "Return" {
        cap.update(&world);
    }
    // The player's heights and distances of rows 111 to 115, which the cap flies toward in
    // rows 112 to 116 (it updates before the player does).
    let player = [
        (339.343_5, 789.248_05),
        (346.343_5, 803.248_05),
        (352.343_5, 817.248_05),
        (357.343_5, 831.248_05),
        (361.343_5, 845.248_05),
    ];
    let cap_rows = [
        Vec3::new(8.420_898, 83.242_55, 669.183_6),
        Vec3::new(17.276_855, 120.992_19, 667.742_2),
        Vec3::new(24.603_027, 177.055_73, 675.328_1),
        Vec3::new(29.098_145, 249.206_6, 694.292_97),
        Vec3::new(29.965_332, 326.909_06, 723.283_2),
    ];
    for ((y, z), want) in player.into_iter().zip(cap_rows) {
        world.player = centimetres(y, z);
        cap.update(&world);
        let got = cap.pos / UNIT;
        assert!(
            (got - want).length() < 0.01,
            "got {got:?}, recorded {want:?}"
        );
    }
}

/// A floor at y = 0 facing up, wide enough for any cap in these tests.
fn flat_floor() -> TriangleMesh {
    TriangleMesh::from_corners(&[
        [
            Vec3::new(-1000., 0., -1000.),
            Vec3::new(-1000., 0., 1000.),
            Vec3::new(1000., 0., -1000.),
        ],
        [
            Vec3::new(1000., 0., 1000.),
            Vec3::new(1000., 0., -1000.),
            Vec3::new(-1000., 0., 1000.),
        ],
    ])
}

/// What the helper (0x401d0c) leaves of a cap.
struct Helped {
    front: Vec3,
    up: Vec3,
    velocity: Vec3,
    counter: i32,
    turn: Option<f32>,
}

/// One step of the helper for a cap with (`front`, `up`, `velocity`), `counter` steps worked and
/// at `spot`.
fn help(
    (mut front, mut up, mut velocity): (Vec3, Vec3, Vec3),
    mut counter: i32,
    mut spot: Vec3,
    touch: &cap_ground::Touch,
    mesh: Option<&TriangleMesh>,
) -> Helped {
    let turn = cap_ground::follow(
        cap_ground::Cap {
            spot: &mut spot,
            front: &mut front,
            up: &mut up,
            velocity: &mut velocity,
            counter: &mut counter,
        },
        touch,
        mesh,
        // `param::THROW_HEIGHT`.
        70.,
    );
    Helped {
        front,
        up,
        velocity,
        counter,
        turn,
    }
}

fn surface(normal: Vec3) -> Option<cap_ground::Surface> {
    Some(cap_ground::Surface { normal })
}

/// The helper makes its front a unit vector in the easing (0x401e60, every step the easing runs)
/// and leaves it as it is while the ground check keeps the cap low (0x40a088 skips the easing,
/// and without a touch nothing else writes the front).
#[cfg_attr(test, test)]
fn the_helper_makes_the_front_a_unit_vector_in_its_easing_only() {
    let long_front = Vec3::new(0., 0., 1.001);
    let state = (long_front, Vec3::Y, Vec3::new(0., 0., 10.));
    let none = cap_ground::Touch::default();
    // High over nothing (no ground to find): the easing runs and normalizes.
    let eased = help(state, 0, Vec3::new(0., 200., 0.), &none, None);
    assert_eq!(eased.front, Vec3::Z);
    assert_eq!(eased.counter, 1);
    assert_eq!(eased.turn, None);
    // Low over the floor (10 up, the throw height 70): the counter resets and the front stays.
    let floor = flat_floor();
    let low = help(state, 3, Vec3::new(0., 10., 0.), &none, Some(&floor));
    assert_eq!(low.front, long_front);
    assert_eq!(low.velocity, state.2);
    assert_eq!(low.counter, 0);
    // The same cap with a floor touch: the collision turn is the part that normalizes now.
    let touch = cap_ground::Touch {
        wall_face: None,
        ground: surface(Vec3::Y),
    };
    let turned = help(state, 3, Vec3::new(0., 10., 0.), &touch, Some(&floor));
    assert_eq!(turned.front, Vec3::Z);
    assert_eq!(turned.up, Vec3::Y);
    assert_eq!(turned.counter, 0);
    // (A turn of no angle raises no reaction.)
    assert_eq!(turned.turn, None);
}

/// `al::isNearZeroOrLess(x, 0.001)` is `x < 0.001` and `al::isNearZeroOrGreater(x, 0.001)` is
/// `x > -0.001` (0x91f568, 0x91f544): a value exactly on the limit is outside it.
#[cfg_attr(test, test)]
fn the_near_zero_tests_are_strict() {
    let floor = flat_floor();
    // The ground check (0x40a038): the cap's height over the floor minus the throw height.
    let height_over = |throw_height: f32, height: f32| {
        let (mut front, mut up, mut velocity) = (Vec3::Z, Vec3::Y, Vec3::new(0., 0., 10.));
        let mut spot = Vec3::new(0., height, 0.);
        let mut counter = 3;
        cap_ground::follow(
            cap_ground::Cap {
                spot: &mut spot,
                front: &mut front,
                up: &mut up,
                velocity: &mut velocity,
                counter: &mut counter,
            },
            &cap_ground::Touch::default(),
            Some(&floor),
            throw_height,
        );
        counter
    };
    // Exactly 0.001 over the throw height is not low (the counter goes on); a hair under is.
    assert_eq!(height_over(0., 0.001), 4);
    assert_eq!(height_over(0., 0.000_999_9), 0);

    // The pull toward the throw height (0x401e94): a cap far above floor that is flatter than its
    // up axis is drawn down unless its front rises by more than -0.001 along the up axis.
    let pulled = |front_up: f32| {
        let tilt = 40_f32.to_radians();
        let up = Vec3::new(tilt.sin(), tilt.cos(), 0.);
        let (mut front, mut up, mut velocity) = (Vec3::new(0., front_up, 1.), up, Vec3::Z);
        let mut spot = Vec3::new(0., 100., 0.);
        let mut counter = 3;
        cap_ground::follow(
            cap_ground::Cap {
                spot: &mut spot,
                front: &mut front,
                up: &mut up,
                velocity: &mut velocity,
                counter: &mut counter,
            },
            &cap_ground::Touch::default(),
            Some(&floor),
            70.,
        );
        spot.y < 100.
    };
    assert!(!pulled(-0.001), "on the limit");
    assert!(pulled(-0.000_999_9), "inside the limit");
}

/// The collision turn's gates (0x402104..0x4021e8, 0x402218): a wall face counts only when its
/// normal is at least half along the cap's up axis and leans against the gravity; otherwise the
/// ground does; a surface facing away from the up axis turns nothing.
#[cfg_attr(test, test)]
fn the_collision_turn_follows_the_binarys_gates() {
    let state = |up: Vec3| (Vec3::Z, up, Vec3::new(0., 0., 10.));
    let low = Vec3::new(0., 10., 0.);
    let floor = flat_floor();
    let ground_normal = Vec3::new(0., 0.8, -0.6);
    let side_wall = Vec3::new(-1., 0., 0.);
    // A vertical wall is not steep against the up axis: the ground turns the cap.
    let both = cap_ground::Touch {
        wall_face: surface(side_wall),
        ground: surface(ground_normal),
    };
    assert_eq!(
        help(state(Vec3::Y), 0, low, &both, Some(&floor)).up,
        ground_normal
    );
    // A wall face along the cap's own up axis (here x) and leaning against the gravity takes
    // over ...
    let leaning = Vec3::new(0.8, 0.6, 0.);
    let tilted_ground = Vec3::new(0.6, 0.8, 0.);
    let along_x = cap_ground::Touch {
        wall_face: surface(leaning),
        ground: surface(tilted_ground),
    };
    assert_eq!(help(state(Vec3::X), 0, low, &along_x, None).up, leaning);
    // ... but not when it leans with the gravity (its normal pointing down).
    let hanging = Vec3::new(0.8, -0.6, 0.);
    let down_x = cap_ground::Touch {
        wall_face: surface(hanging),
        ground: surface(tilted_ground),
    };
    assert_eq!(
        help(state(Vec3::X), 0, low, &down_x, None).up,
        tilted_ground
    );
    // A wall face alone that fails the gates turns nothing, and neither does a normal facing
    // away from the up axis.
    let wall_only = cap_ground::Touch {
        wall_face: surface(side_wall),
        ground: None,
    };
    let kept = help(state(Vec3::Y), 2, low, &wall_only, Some(&floor));
    assert_eq!((kept.up, kept.counter, kept.turn), (Vec3::Y, 0, None));
    let away = cap_ground::Touch {
        wall_face: None,
        ground: surface(Vec3::NEG_Y),
    };
    let kept = help(state(Vec3::Y), 2, low, &away, Some(&floor));
    assert_eq!((kept.front, kept.up, kept.turn), (Vec3::Z, Vec3::Y, None));
}

/// The collision turn lays the front in the surface's plane (`alongVectorNormalH`, a unit
/// vector), rebuilds the velocity (the part across the old front along the new up, the part
/// along it along the new front, sizes kept) and answers the angle it turned by.
#[cfg_attr(test, test)]
fn the_collision_turn_lays_the_front_and_velocity_in_the_new_plane() {
    let normal = Vec3::new(0.6, 0.8, 0.);
    let touch = cap_ground::Touch {
        wall_face: None,
        ground: surface(normal),
    };
    let floor = flat_floor();
    let state = (Vec3::X, Vec3::Y, Vec3::new(10., 3., 0.));
    let turned = help(state, 4, Vec3::new(0., 10., 0.), &touch, Some(&floor));
    assert_eq!(turned.up, normal);
    assert_eq!(turned.counter, 0);
    assert!((turned.front.length() - 1.).abs() < 1e-6);
    assert!(turned.front.dot(normal).abs() < 1e-6, "{:?}", turned.front);
    // The angle between the surfaces is the angle between the fronts.
    let expected = normal.dot(Vec3::Y).acos().to_degrees();
    let turn = turned.turn.expect("a turn wide enough for the reaction");
    assert!((turn - expected).abs() < 1e-3, "{turn} {expected}");
    // 10 ran along the old front, 3 across it: they are now along the new front and up.
    let rebuilt = turned.front * 10. + normal * 3.;
    assert!((turned.velocity - rebuilt).length() < 1e-4);
}

/// The helper ends by standing the cap on its own up axis, facing back along its front
/// (0x40247c): the pose's up is the cap's up, not the world's.
#[cfg_attr(test, test)]
fn a_cap_stood_on_its_up_axis_faces_back_along_its_front() {
    let mut upright = CapPose::thrown_along(Vec3::Z);
    upright.face_back_along(Vec3::Z, Vec3::Y);
    assert!((upright.world_axis(Vec3::Y) - Vec3::Y).length() < 1e-5);
    let up = Vec3::new(0.6, 0.8, 0.);
    let mut leaning = CapPose::thrown_along(Vec3::Z);
    leaning.face_back_along(Vec3::Z, up);
    assert!((leaning.world_axis(Vec3::Y) - up).length() < 1e-5);
    assert!((leaning.world_axis(Vec3::Z) + Vec3::Z).length() < 1e-5);
}

/// The helper in a flight: the cap's sphere meets a slope, and a later step's helper takes the
/// slope's normal for the cap's up axis and raises the collision reaction (`TerrainTurn`).
#[cfg_attr(test, test)]
fn a_cap_thrown_into_a_slope_turns_onto_it_and_raises_the_reaction() {
    // A slope rising along +z at 30 degrees, from the floor at z = 400.
    let (rise, run) = (30_f32.to_radians().sin(), 30_f32.to_radians().cos());
    let top = Vec3::new(0., 1000. * rise, 400. + 1000. * run);
    let mesh = TriangleMesh::from_corners(&[
        [
            Vec3::new(-1000., 0., 400.),
            Vec3::new(-1000., top.y, top.z),
            Vec3::new(1000., 0., 400.),
        ],
        [
            Vec3::new(1000., 0., 400.),
            Vec3::new(-1000., top.y, top.z),
            Vec3::new(1000., top.y, top.z),
        ],
    ]);
    let normal = mesh.triangles[0].normal;
    assert!(normal.y > 0.8 && normal.z < 0., "{normal:?}");
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
    let world = CapWorld {
        player: Vec3::ZERO,
        hold: false,
        mesh: Some(&mesh),
        water: None,
        targets: &[],
    };
    let mut turns = Vec::new();
    for _ in 0..40 {
        for event in cap.update(&world) {
            if let CapEvent::TerrainTurn(degrees) = event {
                turns.push(degrees);
            }
        }
    }
    assert!(!turns.is_empty(), "the cap never met the slope");
    assert!(turns.iter().all(|degrees| *degrees >= 0.1));
    assert!(cap.up.dot(normal) > 0.999, "{:?} {normal:?}", cap.up);
}

/// The helper ends in `slerpQuat(pose, target, 1.0)` (0x402470, `sead::QuatCalcCommon::slerpTo`
/// 0xf17ac), and a whole slerp does not return the target: the weight is
/// `(1 / sin(theta)) * sin(theta * t)` in single precision (0xf1860..0xf1884), and a target whose
/// own dot product is 0.99999994 (not exactly unit) has a non-zero theta even against itself, so
/// the weight is 0.99999994 and the pose lands an ulp off. From there theta is larger, the weight
/// is 1.0 and the pose is the target again. A cap flying a fixed front therefore alternates its
/// pose by an ulp every step, which is the pose the stay and the return inherit when the brake's
/// last step lands on the odd phase.
#[cfg_attr(test, test)]
fn a_cap_flying_a_fixed_front_alternates_its_pose_by_an_ulp() {
    use crate::movement::cap_pose::CapPose;
    use crate::movement::pose_math::{NEGATED_GRAVITY, quat_front_up};
    let front = Vec3::new(-0.139_765_98, 0., 0.990_184_6);
    let target = quat_front_up(-front, NEGATED_GRAVITY);
    let mut pose = CapPose::thrown_along(front);
    assert_eq!(pose.quat(), target);
    let mut poses = Vec::new();
    for _ in 0..6 {
        pose.face_back_along(front, NEGATED_GRAVITY);
        poses.push(pose.quat());
    }
    for pair in poses.windows(2) {
        assert_ne!(pair[0], pair[1]);
        // (One ulp apart in the y component, 0.997 and up.)
        assert_eq!(pair[0].y.to_bits().abs_diff(pair[1].y.to_bits()), 1);
    }
    for triple in poses.windows(3) {
        assert_eq!(triple[0], triple[2]);
    }
    // The odd steps are the target again.
    assert_eq!(poses[1], target);
}

/// A cap in its return, `step` executions old, at `spot` (centimetres), resting there.
fn returning_at(step: u32, spot: Vec3) -> Cap {
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
        nerve: CapNerve::Return,
        step,
        spot,
        velocity: Vec3::ZERO,
        speed: 32.,
        return_speed: 0.,
        anchor: Vec3::ZERO,
        after_lock_on: false,
    });
    cap
}

/// A player this far from the origin (metres) is out of any return's reach in 181 steps.
fn far_world(mesh: Option<&TriangleMesh>) -> CapWorld<'_> {
    CapWorld {
        player: Vec3::new(0., 0., 100_000.),
        hold: false,
        mesh,
        water: None,
        targets: &[],
    }
}

/// The tail of `exeReturn` (0x407434..0x40764c): the counter rises when the touched surface
/// faces against the way to the head by more than sin 5 degrees (0x1880754), strictly, and any
/// other answer, no surface and no direction included, clears it; the return ends when its step
/// is above 180 (0x4074c8, `isGreaterStep`) or the counter above 45 (0x4074e0), by `hide`.
#[cfg_attr(test, test)]
fn the_returns_tail_counts_walls_and_ends_by_step() {
    use cap_return::{AGAINST_SURFACE, ends, wall_counter};
    let toward = Vec3::Z;
    // Facing straight against the way to the head: counted. A wall seen from behind, a graze
    // (exactly -sin 5 degrees is not past it), no surface and no direction: cleared.
    assert_eq!(wall_counter(7, -Vec3::Z, toward), 8);
    assert_eq!(wall_counter(7, Vec3::Z, toward), 0);
    assert_eq!(
        wall_counter(7, Vec3::new(0., 0., AGAINST_SURFACE), toward),
        0
    );
    assert_eq!(
        wall_counter(7, Vec3::new(0., 0., AGAINST_SURFACE - 1e-3), toward),
        8
    );
    assert_eq!(wall_counter(7, Vec3::ZERO, toward), 0);
    assert_eq!(wall_counter(7, -Vec3::Z, Vec3::ZERO), 0);
    // The step: 180 still flies, 181 ends. The counter: 45 still flies, 46 ends.
    assert!(!ends(180, 0));
    assert!(ends(181, 0));
    assert!(!ends(0, 45));
    assert!(ends(0, 46));
}

/// A return that does not reach the player runs its nerve 181 times and ends in the 182nd,
/// whose step is 181 (`isGreaterStep(180)`): the cap hides (`HackCap::hide(false)`, 0x407584) with
/// no velocity and no catch, and does not run any more.
#[cfg_attr(test, test)]
fn a_return_that_never_arrives_hides_the_cap_after_180_steps() {
    let world = far_world(None);
    let mut cap = returning_at(0, Vec3::new(0., 75., 0.));
    let mut hidden_at = None;
    for update in 1..=190 {
        let events = cap.update(&world);
        assert!(!events.contains(&CapEvent::Caught), "update {update}");
        if events.contains(&CapEvent::Hidden) {
            assert_eq!(hidden_at.replace(update), None, "hidden twice");
        }
    }
    assert_eq!(hidden_at, Some(182));
    assert_eq!(cap.velocity, Vec3::ZERO);
    // The cap stays where it ended: the later updates did nothing.
    let spot = cap.spot;
    cap.update(&world);
    assert_eq!(cap.spot, spot);
}

/// A return flown into a wall that stands between the cap and the head ends when the wall counter
/// passes 45, long before the 180 steps: each execution that finds the cap touching the wall
/// (its normal against the way to the head) counts, and the first one only finds the contact the
/// previous move made. Without the wall the same flight goes on past as many executions.
#[cfg_attr(test, test)]
fn a_return_pressed_into_a_wall_ends_when_the_counter_passes_45() {
    // A wall at x = 500 facing -x, tall enough for the cap at 75; the head is beyond it.
    let mesh = TriangleMesh::from_corners(&[
        [
            Vec3::new(500., -100., -2000.),
            Vec3::new(500., 300., 2000.),
            Vec3::new(500., 300., -2000.),
        ],
        [
            Vec3::new(500., -100., -2000.),
            Vec3::new(500., -100., 2000.),
            Vec3::new(500., 300., 2000.),
        ],
    ]);
    let world = CapWorld {
        player: Vec3::new(100_000., 0., 0.),
        hold: false,
        mesh: Some(&mesh),
        water: None,
        targets: &[],
    };
    let mut cap = returning_at(0, Vec3::new(300., 75., 0.));
    let mut ended = None;
    for update in 1..=100 {
        let events = cap.update(&world);
        if events.contains(&CapEvent::Hidden) {
            ended = Some((update, cap.wall_hits));
            break;
        }
    }
    let (update, counter) = ended.expect("the return never ended");
    assert_eq!(counter, 46, "ended by the counter");
    assert!(update < 100, "ended at update {update}");
    // The cap stopped at the wall, 60 short of it.
    assert!(cap.spot.x < 440.01, "{:?}", cap.spot);
    // The same flight over open ground is still going after as many executions.
    let open = CapWorld {
        player: Vec3::new(100_000., 0., 0.),
        ..far_world(None)
    };
    let mut free = returning_at(0, Vec3::new(300., 75., 0.));
    for _ in 0..update {
        assert!(!free.update(&open).contains(&CapEvent::Hidden));
    }
}

/// The stand's rolling gate is the throw type the start stored (`HackCap::isThrowTypeRolling`,
/// 0x400250: `+0x268` near zero and `+0x26c` not), which nothing clears: a down throw whose
/// rolling state is gone still stands across its front, and a forward throw does not.
#[cfg_attr(test, test)]
fn the_rolling_stand_follows_the_throw_type_not_the_rolling_state() {
    let thrown = |kind| {
        Cap::start(
            Launch {
                front: Vec3::Z,
                kind,
                left: false,
                air: false,
                continuous: false,
                spiral_side: 0.,
                up: Vec3::Y,
            },
            Vec3::ZERO,
            1,
        )
    };
    let world = far_world(None);
    let moment = Moment {
        nerve: CapNerve::Return,
        step: 0,
        spot: Vec3::new(0., 75., 0.),
        velocity: Vec3::ZERO,
        speed: 30.,
        return_speed: 0.,
        anchor: Vec3::ZERO,
        after_lock_on: false,
    };
    let mut kept = thrown(Throw::Down);
    kept.resume(&moment);
    let mut cleared = thrown(Throw::Down);
    cleared.resume(&moment);
    cleared.rolling = None;
    let mut upright = thrown(Throw::Forward);
    upright.pose = kept.pose;
    upright.resume(&moment);
    for cap in [&mut kept, &mut cleared, &mut upright] {
        cap.update(&world);
    }
    assert_eq!(cleared.pose.quat(), kept.pose.quat());
    assert_ne!(cleared.pose.quat(), upright.pose.quat());
}

/// `HackCap::startThrow` keeps the up axis and the front it is given (the launch's up from
/// `rs::calcGroundNormalOrGravityDir`, the front across it): the cap keeps that up axis and
/// front and leaves along the slope.
#[cfg_attr(test, test)]
fn the_cap_thrown_from_a_slope_flies_along_the_slope() {
    let normal = Vec3::new(0., 0.8, -0.6);
    let front = Vec3::new(0., 0.6, 0.8);
    let cap = Cap::start(
        Launch {
            front,
            kind: Throw::Forward,
            left: false,
            air: false,
            continuous: false,
            spiral_side: 0.,
            up: normal,
        },
        Vec3::ZERO,
        1,
    );
    assert_eq!(cap.up, normal);
    assert!((cap.front - front).length() < 1e-6);
    assert!(cap.velocity.dot(normal).abs() < 1e-3, "{:?}", cap.velocity);
    assert!(cap.velocity.normalize().dot(front) > 0.9999);
    // Level ground flies along the horizontal whatever height the direction has.
    let level = Cap::start(
        Launch {
            front: Vec3::new(0., 0.5, 1.),
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
    assert_eq!(level.front, Vec3::Z);
}
