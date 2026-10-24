use super::*;

pub(super) fn run() {
    the_pose_follows_the_speed_and_the_turn_and_resets();
    the_exes_swim_pose_agrees_bit_for_bit();
}

#[cfg_attr(test, test)]
fn the_pose_follows_the_speed_and_the_turn_and_resets() {
    let mut pose = SwimPose::default();
    // Swimming forward at the top of the range with the turn whole: every angle eases toward
    // its limit by a twentieth of the way, the roll stays at zero.
    pose.update(Vec3::Z, Vec3::new(0., 0., LOW_SPEED_MAX_H), 1.);
    let [pitch, heel, roll, spine] = pose.angles();
    assert!((pitch - BENT_FORWARD_MAX * 0.05).abs() < 1e-5, "{pitch}");
    assert!((heel - BENT_SIDE_MAX * 0.05).abs() < 1e-5, "{heel}");
    assert!((spine - BENT_SPINE_MAX * 0.05).abs() < 1e-5, "{spine}");
    assert_eq!(roll, 0.);
    // Standing still lets the pose ease back; the state's end clears it at once.
    pose.update(Vec3::Z, Vec3::ZERO, 1.);
    assert!(pose.angles()[0] < pitch);
    pose.reset();
    assert_eq!(pose.angles(), [0.; 4]);
    // The surface and the bottom's nerves ease every angle off by its own share, the spine's
    // by the front blend rate.
    let mut eased = SwimPose {
        pitch: 20.,
        heel: 10.,
        roll: 5.,
        spine: 8.,
    };
    eased.ease_off();
    let [pitch, heel, roll, spine] = eased.angles();
    assert!((pitch - 20. * 0.95).abs() < 1e-4, "{pitch}");
    assert!((heel - 10. * 0.95).abs() < 1e-4 && (roll - 5. * 0.95).abs() < 1e-4);
    assert!((spine - 8. * 0.96).abs() < 1e-4, "{spine}");
    // The spine is gated by the heel: with no heel it is left alone, whatever its own angle.
    let upright: Mat34 = [[1., 0., 0., 5.], [0., 1., 0., 6.], [0., 0., 1., 7.]];
    let mut matrix = upright;
    SwimPose {
        spine: 30.,
        ..SwimPose::default()
    }
    .calc(Joint::Spine1, &mut matrix);
    assert_eq!(matrix, upright);
    SwimPose {
        heel: 10.,
        spine: 30.,
        ..SwimPose::default()
    }
    .calc(Joint::Spine1, &mut matrix);
    assert_ne!(matrix, upright);
    // The root turns about the point above it: the pivot itself stays where it was.
    let mut root: Mat34 = upright;
    SwimPose {
        pitch: 25.,
        ..SwimPose::default()
    }
    .calc(Joint::AllRoot, &mut root);
    let pivot = Vec3::new(
        upright[0][3] + PIVOT.x,
        upright[1][3] + PIVOT.y,
        upright[2][3] + PIVOT.z,
    );
    let turned = Vec3::new(
        root[0][1] * PIVOT.y + root[0][3],
        root[1][1] * PIVOT.y + root[1][3],
        root[2][1] * PIVOT.y + root[2][3],
    );
    assert!((turned - pivot).length() < 1e-3, "{turned:?} vs {pivot:?}");
}

fn number(v: &serde_json::Value) -> f32 {
    v.as_f64().unwrap() as f32
}

fn numbers(v: &serde_json::Value) -> Vec<f32> {
    v.as_array().unwrap().iter().map(number).collect()
}

fn mat34(v: &serde_json::Value) -> Mat34 {
    let m = numbers(v);
    [
        [m[0], m[1], m[2], m[3]],
        [m[4], m[5], m[6], m[7]],
        [m[8], m[9], m[10], m[11]],
    ]
}

fn same(a: &Mat34, b: &Mat34) -> bool {
    a.iter()
        .flatten()
        .zip(b.iter().flatten())
        .all(|(x, y)| x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan()))
}

/// Calls of the executable's own `calcJointCallback` and of the helper the swimming nerves call,
/// run in a local interpreter (the fidelity folder's `swim_oracle.py`). `PJV_SWIM_CASES` names
/// the file; without it nothing runs.
#[cfg_attr(test, test)]
fn the_exes_swim_pose_agrees_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_SWIM_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let cases: serde_json::Value = serde_json::from_str(&text).expect("json");
    for (n, case) in cases["callbacks"]
        .as_array()
        .expect("callbacks")
        .iter()
        .enumerate()
    {
        let a = numbers(&case["angles"]);
        let pose = SwimPose {
            pitch: a[0],
            heel: a[1],
            roll: a[2],
            spine: a[3],
        };
        let joint = Joint::ALL[case["joint"].as_u64().unwrap() as usize];
        let mut world = mat34(&case["in"]);
        pose.calc(joint, &mut world);
        let want = mat34(&case["out"]);
        assert!(
            same(&world, &want),
            "callback {n} {joint:?} angles {a:?}: {world:?} want {want:?}"
        );
    }
    for (n, case) in cases["helpers"]
        .as_array()
        .expect("helpers")
        .iter()
        .enumerate()
    {
        let p = numbers(&case["p_in"]);
        let mut pose = SwimPose {
            pitch: p[0],
            heel: p[1],
            roll: p[2],
            spine: p[3],
        };
        let front = numbers(&case["front"]);
        let velocity = numbers(&case["velocity"]);
        pose.update(
            Vec3::new(front[0], front[1], front[2]),
            Vec3::new(velocity[0], velocity[1], velocity[2]),
            number(&case["turn"]),
        );
        let want = numbers(&case["p_out"]);
        let got = pose.angles();
        assert!(
            got.iter()
                .zip(&want)
                .all(|(a, b)| a.to_bits() == b.to_bits()),
            "helper {n}: {got:?} want {want:?} from {p:?}"
        );
    }
}
