use super::*;

pub(super) fn run() {
    the_exes_turn_control_agrees_bit_for_bit();
    ceiling_turn_uses_the_requested_direction();
}

#[cfg_attr(test, test)]
fn ceiling_turn_uses_the_requested_direction() {
    let mut turn = TurnControl::air();
    let frame = TurnFrame {
        actor_front: Vec3::NEG_Z,
        actor_up: Vec3::NEG_Y,
        stick: Vec3::X,
        up: Vec3::Y,
    };
    assert!(turn.update_sky(frame));
    assert_eq!(turn.direction, Vec3::X);
    assert_eq!(turn.rate, turn.fast_limit);
    assert!(!turn.update_sky(TurnFrame {
        actor_up: Vec3::Y,
        ..frame
    }));
}

fn vec3(v: &serde_json::Value) -> Vec3 {
    let a = v.as_array().expect("array");
    Vec3::new(
        a[0].as_f64().unwrap() as f32,
        a[1].as_f64().unwrap() as f32,
        a[2].as_f64().unwrap() as f32,
    )
}

fn number(v: &serde_json::Value) -> f32 {
    v.as_f64().unwrap() as f32
}

fn same_bits(a: Vec3, b: Vec3) -> bool {
    a.to_array().map(f32::to_bits) == b.to_array().map(f32::to_bits)
}

/// Sequences from the executable's `PlayerActionTurnControl::update`, run in a local interpreter
/// (the fidelity folder's `turn_oracle.py`): `PJV_TURN_CASES` names the file; without it nothing
/// runs. Every field the routine writes is compared after every frame.
#[cfg_attr(test, test)]
fn the_exes_turn_control_agrees_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_TURN_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let sequences: serde_json::Value = serde_json::from_str(&text).expect("json");
    let mut frames = 0;
    for (index, sequence) in sequences.as_array().expect("sequences").iter().enumerate() {
        let setup = sequence["setup"].as_array().unwrap();
        let mut control = if sequence["forced"].as_bool().unwrap() {
            TurnControl::swim()
        } else {
            TurnControl::default()
        };
        control.setup(TurnSetup {
            min_degrees: number(&setup[0]),
            fast_degrees: number(&setup[1]),
            limit: number(&setup[2]),
            fast_limit: number(&setup[3]),
            accel_frames: setup[4].as_i64().unwrap() as i32,
            accel_frames_fast: setup[5].as_i64().unwrap() as i32,
            brake_frames: setup[6].as_i64().unwrap() as i32,
        });
        for (step, frame) in sequence["frames"].as_array().unwrap().iter().enumerate() {
            control.update(TurnFrame {
                actor_front: vec3(&frame["front"]),
                actor_up: vec3(&frame["actor_up"]),
                stick: vec3(&frame["stick"]),
                up: vec3(&frame["up"]),
            });
            let state = &frame["state"];
            let at = format!("sequence {index} frame {step}: {frame}");
            assert_eq!(
                control.rate.to_bits(),
                number(&state["rate"]).to_bits(),
                "rate, {at} got {}",
                control.rate
            );
            assert!(
                same_bits(control.last_direction, vec3(&state["last_dir"])),
                "last direction, {at}"
            );
            assert!(
                same_bits(control.last_axis, vec3(&state["last_axis"])),
                "last axis, {at} got {:?}",
                control.last_axis
            );
            assert!(
                same_bits(control.last_up, vec3(&state["last_up"])),
                "last up, {at}"
            );
            assert_eq!(
                control.stick_turn.to_bits(),
                number(&state["avg"]).to_bits(),
                "stick turn, {at}"
            );
            // The ring, oldest first.
            let ring = state["ring"].as_array().unwrap();
            let head = state["head"].as_u64().unwrap() as usize;
            let count = state["count"].as_u64().unwrap() as usize;
            let logical: Vec<u32> = (0..count)
                .map(|i| number(&ring[(head + i) % 5]).to_bits())
                .collect();
            let ours: Vec<u32> = control.recent.iter().map(|v| v.to_bits()).collect();
            assert_eq!(ours, logical, "the stick's turns, {at}");
            assert!(
                same_bits(control.direction, vec3(&state["out"])),
                "direction, {at} got {:?}",
                control.direction
            );
            let flags = state["flags"].as_array().unwrap();
            assert_eq!(
                control.reverse,
                flags[0].as_u64() == Some(1),
                "reverse, {at}"
            );
            assert_eq!(
                control.reverse_forced,
                flags[1].as_u64() == Some(1),
                "forced reverse, {at}"
            );
            assert_eq!(
                control.power_signed(vec3(&frame["up"])).to_bits(),
                number(&state["power"]).to_bits(),
                "turn power, {at}"
            );
            assert_eq!(
                control.power_signed(Vec3::Y).to_bits(),
                number(&state["power_y"]).to_bits(),
                "turn power about the vertical, {at}"
            );
            frames += 1;
        }
    }
    assert!(frames > 0, "no cases");
}
