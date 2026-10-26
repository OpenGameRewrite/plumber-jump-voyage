use super::*;

pub(super) fn run() {
    the_centre_input_fades_and_ends_the_sway();
    the_exes_ground_pose_agrees_bit_for_bit();
}

#[cfg_attr(test, test)]
fn the_centre_input_fades_and_ends_the_sway() {
    let mut pose = GroundPose::default();
    let impulse = CenterInput {
        impulse: true,
        impulse_vector: Vec3::new(4., 0., 2.),
        ..CenterInput::default()
    };
    pose.update(GroundInputs::default(), &impulse);
    assert_eq!(pose.balance.blend, 1.);
    assert!(pose.balance.started);
    let quiet = CenterInput::default();
    for _ in 0..100 {
        pose.update(GroundInputs::default(), &quiet);
    }
    // The blend runs out in 30 frames at the slow step, and the sway is gone with it.
    assert!(pose.balance.blend.abs() < 0.001);
    assert_eq!(pose.balance.sway, Vec3::ZERO);
}

fn number(v: &serde_json::Value) -> f32 {
    v.as_f64().unwrap() as f32
}

fn vec3(v: &serde_json::Value) -> Vec3 {
    let a = v.as_array().expect("array");
    Vec3::new(number(&a[0]), number(&a[1]), number(&a[2]))
}

fn mat34(v: &serde_json::Value) -> Mat34 {
    let m: Vec<f32> = v.as_array().unwrap().iter().map(number).collect();
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

/// Sequences from the executable's own `update`, `updateCenterBalance` and `calcJointCallback`
/// run in a local interpreter (the fidelity folder's `ground_oracle.py`): every joint's matrix,
/// the two leg matrices the feet rewrite, and the controller's state after each frame.
/// `PJV_GROUND_CASES` names the file; without it nothing runs.
#[cfg_attr(test, test)]
fn the_exes_ground_pose_agrees_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_GROUND_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let cases: serde_json::Value = serde_json::from_str(&text).expect("json");
    let mut pose = GroundPose::default();
    for (n, case) in cases.as_array().expect("sequences").iter().enumerate() {
        pose = GroundPose::default();
        for (f, frame) in case["frames"].as_array().unwrap().iter().enumerate() {
            let i = &frame["inputs"];
            let c = &frame["center"];
            pose.update(
                GroundInputs {
                    body_ik: number(&i["a"]),
                    leg_ik: number(&i["b"]),
                    tilt: number(&i["c"]),
                    stiff_landing: i["flag"].as_bool().unwrap(),
                },
                &CenterInput {
                    impulse: c["b0"].as_bool().unwrap(),
                    impulse_vector: vec3(&c["vec_a"]),
                    follow_up: c["b1"].as_bool().unwrap(),
                    follow_up_vector: vec3(&c["vec_b"]),
                    fade_quickly: c["b2"].as_bool().unwrap(),
                    strong: c["b3"].as_bool().unwrap(),
                },
            );
            // (`d`, the head tilt target, only feeds the eye animation.)
            let mut solved = [crate::skeleton_pose::IDENTITY; crate::skeleton_pose::JOINT_COUNT];
            for (j, joint) in Joint::ALL.into_iter().enumerate() {
                let entry = &frame["joints"][j];
                let mut world = mat34(&entry["in"]);
                pose.calc(joint, &mut world, &mut solved);
                let want = mat34(&entry["out"]);
                assert!(
                    same(&world, &want),
                    "sequence {n} frame {f} joint {joint:?}: {world:?} want {want:?}"
                );
                if let Some(legs) = entry.get("legs") {
                    for (k, slot) in [joint as usize - 2, joint as usize - 1]
                        .into_iter()
                        .enumerate()
                    {
                        let want = mat34(&legs[k]);
                        assert!(
                            same(&solved[slot], &want),
                            "sequence {n} frame {f} leg {slot}: {:?} want {want:?}",
                            solved[slot]
                        );
                    }
                }
                if joint == Joint::JointRoot {
                    // (Nothing after the joint root changes what the state shows.)
                    let state = pose.state();
                    let want: Vec<f32> = frame["state"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(number)
                        .collect();
                    let first = state
                        .iter()
                        .zip(&want)
                        .position(|(a, b)| a.to_bits() != b.to_bits());
                    assert!(
                        first.is_none(),
                        "sequence {n} frame {f}: state field {first:?} {state:?} want {want:?}"
                    );
                }
                solved[joint as usize] = want;
                if let Some(legs) = entry.get("legs") {
                    solved[joint as usize - 2] = mat34(&legs[0]);
                    solved[joint as usize - 1] = mat34(&legs[1]);
                }
            }
            let state = pose.state();
            let want: Vec<f32> = frame["state"]
                .as_array()
                .unwrap()
                .iter()
                .map(number)
                .collect();
            assert!(
                state
                    .iter()
                    .zip(&want)
                    .all(|(a, b)| a.to_bits() == b.to_bits()),
                "sequence {n} frame {f}: state {state:?} want {want:?}"
            );
            // `calcGroundPoseUp` stands the actor's up axis with the spine's matrix (a copy of
            // its forward one, not the inverse).
            let up = &frame["ground_up"];
            let got = pose.ground_pose_up(vec3(&up["actor_up"]));
            let want = vec3(&up["out"]);
            assert!(
                got.to_array().map(f32::to_bits) == want.to_array().map(f32::to_bits),
                "sequence {n} frame {f}: ground pose up {got:?} want {want:?}"
            );
        }
    }
    let _ = pose;
}
