use super::*;

pub(super) fn run() {
    the_head_follows_a_point_in_front_and_returns_without_one();
    the_exes_look_at_callback_agrees_bit_for_bit();
}

fn identity() -> Mat34 {
    [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]]
}

#[cfg_attr(test, test)]
fn the_head_follows_a_point_in_front_and_returns_without_one() {
    let mut head = HeadLookAt::default();
    let actor = identity();
    let animated = identity();
    // A point up and to the side in front of an actor facing +z (and the first call only
    // records the actor's front).
    for _ in 0..40 {
        let mut joint = animated;
        head.request(Vec3::new(100., 100., 300.));
        head.end_frame();
        head.apply(&mut joint, &actor);
    }
    let turned = head.current_turn();
    assert!(turned.w < 0.99, "the head turned: {turned:?}");
    for _ in 0..80 {
        let mut joint = animated;
        head.end_frame();
        head.apply(&mut joint, &actor);
    }
    assert!(head.current_turn().w > 0.9999, "it came back");
}

fn floats(v: &serde_json::Value) -> Vec<f32> {
    v.as_array()
        .expect("array")
        .iter()
        .map(|x| x.as_f64().unwrap() as f32)
        .collect()
}

fn matrix(v: &serde_json::Value) -> Mat34 {
    let m = floats(v);
    [
        [m[0], m[1], m[2], m[3]],
        [m[4], m[5], m[6], m[7]],
        [m[8], m[9], m[10], m[11]],
    ]
}

fn bits(m: &Mat34) -> Vec<u32> {
    m.iter().flatten().map(|x| x.to_bits()).collect()
}

/// Sequences of calls recorded from the executable's own function run in a local interpreter
/// (the fidelity folder's `gen_lookat_cases.py`): `PJV_LOOKAT_CASES` names the file; without it
/// nothing runs.
#[cfg_attr(test, test)]
fn the_exes_look_at_callback_agrees_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_LOOKAT_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let cases: serde_json::Value = serde_json::from_str(&text).expect("json");
    let mut calls = 0;
    for (n, sequence) in cases.as_array().expect("sequences").iter().enumerate() {
        let mut head = HeadLookAt::default();
        for (i, frame) in sequence["frames"].as_array().unwrap().iter().enumerate() {
            let t = floats(&frame["target"]);
            head.request(Vec3::new(t[0], t[1], t[2]));
            head.end_frame();
            head.requested = frame["requested"].as_bool().unwrap();
            head.keeps_turn = frame["mode"].as_bool().unwrap();
            head.snap = frame["snap"].as_bool().unwrap();
            let mut joint = matrix(&frame["mtx"]);
            head.apply(&mut joint, &matrix(&frame["actor"]));
            let want = matrix(&frame["out"]);
            assert_eq!(
                bits(&joint),
                bits(&want),
                "sequence {n} frame {i}: {joint:?} vs {want:?}"
            );
            let q = floats(&frame["q_cur"]);
            assert_eq!(
                [head.current.x, head.current.y, head.current.z, head.current.w].map(f32::to_bits),
                [q[0], q[1], q[2], q[3]].map(f32::to_bits),
                "sequence {n} frame {i}: current turn"
            );
            let w = floats(&frame["q_tgt"]);
            assert_eq!(
                [head.wanted.x, head.wanted.y, head.wanted.z, head.wanted.w].map(f32::to_bits),
                [w[0], w[1], w[2], w[3]].map(f32::to_bits),
                "sequence {n} frame {i}: wanted turn"
            );
            assert_eq!(head.following, frame["following"].as_i64().unwrap() != 0);
            calls += 1;
        }
    }
    assert!(calls > 0);
}
