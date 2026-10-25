use super::*;

pub(super) fn run() {
    the_exes_wall_along_agrees_bit_for_bit();
}

fn vec3(v: &serde_json::Value) -> Vec3 {
    let a = v.as_array().expect("array");
    Vec3::new(
        a[0].as_f64().unwrap() as f32,
        a[1].as_f64().unwrap() as f32,
        a[2].as_f64().unwrap() as f32,
    )
}

fn same_bits(a: Vec3, b: Vec3) -> bool {
    a.to_array().map(f32::to_bits) == b.to_array().map(f32::to_bits)
}

/// Sequences from the executable's `PlayerInput::updateWallAlong` and `snapWallAlongInput`, run in
/// a local interpreter (the fidelity folder's `wallalong_oracle.py`): `PJV_WALLALONG_CASES` names
/// the file; without it nothing runs. The remembered wall and the bent inputs are compared after
/// every frame.
#[cfg_attr(test, test)]
fn the_exes_wall_along_agrees_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_WALLALONG_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let sequences: serde_json::Value = serde_json::from_str(&text).expect("json");
    let mut frames = 0;
    for (index, sequence) in sequences.as_array().expect("sequences").iter().enumerate() {
        let mut along = WallAlong::default();
        for (step, frame) in sequence["frames"].as_array().unwrap().iter().enumerate() {
            let stick = frame["stick"].as_array().unwrap();
            let stick = Vec2::new(
                stick[0].as_f64().unwrap() as f32,
                stick[1].as_f64().unwrap() as f32,
            );
            let contact = (!frame["wall"].is_null()).then(|| vec3(&frame["wall"]));
            along.update(contact, stick);
            let at = format!("sequence {index} frame {step}: {frame}");
            let state = &frame["state"];
            assert_eq!(
                along.frames,
                state["frames"].as_i64().unwrap() as i32,
                "{at}"
            );
            assert_eq!(
                along.weight.to_bits(),
                (state["weight"].as_f64().unwrap() as f32).to_bits(),
                "weight, {at} got {}",
                along.weight
            );
            if along.frames > 0 {
                assert!(same_bits(along.wall, vec3(&state["wall"])), "wall, {at}");
            }
            for snap in frame["snaps"].as_array().unwrap() {
                let got = along.snap(vec3(&snap["input"]), vec3(&snap["up"]));
                assert!(
                    same_bits(got, vec3(&snap["out"])),
                    "snap of {} on {}: got {got:?}, {at}",
                    snap["input"],
                    snap["up"]
                );
            }
            frames += 1;
        }
    }
    assert!(frames > 0, "no cases");
}
