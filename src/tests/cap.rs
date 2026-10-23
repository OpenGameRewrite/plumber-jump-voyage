use super::*;

pub(super) fn run() {
    straight_throw_matches_the_recording();
    a_wall_at_an_angle_turns_the_cap();
    the_return_after_a_trample_leaves_along_the_cap_pose();
    the_catch_is_the_sensor_contact();
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
        });
        cap
    };
    let world = CapWorld {
        player: Vec3::ZERO,
        hold: false,
        mesh: None,
    };
    // 220 away it comes to about 140 from the body in this frame: touching.
    assert!(returning(220.).update(&world).contains(&CapEvent::Caught));
    // 250 away it only comes to about 170: not yet.
    assert!(!returning(250.).update(&world).contains(&CapEvent::Caught));
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
        },
        Vec3::ZERO,
        1,
    );
    let world = CapWorld {
        player: Vec3::ZERO,
        hold: false,
        mesh: None,
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
        },
        Vec3::ZERO,
        1,
    );
    let world = CapWorld {
        player: Vec3::ZERO,
        hold: false,
        mesh: Some(&mesh),
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
        },
        Vec3::ZERO,
        1,
    );
    let centimetres = |y: f32, z: f32| Vec3::new(0., y * UNIT, z * UNIT);
    let mut world = CapWorld {
        player: centimetres(339.343_5, 789.248_05),
        hold: true,
        mesh: None,
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
