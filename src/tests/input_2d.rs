use super::*;

pub(super) fn run() {
    executable_input_sequences();
}

fn vec(v: &serde_json::Value) -> Vec3 {
    Vec3::new(
        v[0].as_f64().unwrap() as f32,
        v[1].as_f64().unwrap() as f32,
        v[2].as_f64().unwrap() as f32,
    )
}

#[cfg_attr(test, test)]
fn executable_input_sequences() {
    let Some(path) = std::env::var_os("PJV_INPUT_2D_CASES") else {
        return;
    };
    let cases: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let sequences = cases.as_array().unwrap();
    assert!(!sequences.is_empty());
    for (i, sequence) in sequences.iter().enumerate() {
        let mut input = Input2D::default();
        for (j, case) in sequence.as_array().unwrap().iter().enumerate() {
            let f = &case["frame"];
            let frame = Frame {
                active: f["active"].as_bool().unwrap(),
                stick: Vec2::new(
                    f["stick"][0].as_f64().unwrap() as f32,
                    f["stick"][1].as_f64().unwrap() as f32,
                ),
                view: std::array::from_fn(|k| vec(&f["view"][k])),
                position: vec(&f["position"]),
                velocity: vec(&f["velocity"]),
                actor_side: vec(&f["actor_side"]),
                actor_up: vec(&f["actor_up"]),
                actor_front: vec(&f["actor_front"]),
                lock: vec(&f["lock"]),
                up: vec(&f["up"]),
                ground: (!f["ground"].is_null())
                    .then(|| (vec(&f["ground"][0]), vec(&f["ground"][1]))),
                just_landed: f["just_landed"].as_bool().unwrap(),
            };
            input.update(&frame);
            let mut raw = Vec::new();
            raw.extend(input.stick.to_array());
            raw.extend(input.mapped_stick.to_array());
            for v in input.axes {
                raw.extend(v.to_array());
            }
            raw.extend(input.up.to_array());
            for v in input.mapped_axes {
                raw.extend(v.to_array());
            }
            for v in [input.lock, input.ground_position, input.ground_normal] {
                raw.extend(v.to_array());
            }
            let want: Vec<u32> = case["raw"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_u64().unwrap() as u32)
                .collect();
            assert_eq!(
                raw.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                want,
                "sequence {i} frame {j} input memory"
            );
            assert_eq!(
                input.enabled,
                case["enabled"].as_bool().unwrap(),
                "{i}/{j} enabled"
            );
            assert_eq!(input.held, case["held"].as_bool().unwrap(), "{i}/{j} held");
            for out in case["outputs"].as_array().unwrap() {
                let want: Vec<u32> = out["bits"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_u64().unwrap() as u32)
                    .collect();
                assert_eq!(
                    input
                        .move_input(&frame, vec(&out["up"]))
                        .to_array()
                        .map(f32::to_bits)
                        .to_vec(),
                    want,
                    "{i}/{j} move up {:?}",
                    out["up"]
                );
            }
        }
    }
}
