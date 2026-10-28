use super::*;

pub(super) fn run() {
    the_exes_ground_move_agrees_bit_for_bit();
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

/// Sequences from the executable's `PlayerActionGroundMoveControl::updateNormalMove`, run in a
/// local interpreter (the fidelity folder's `move_oracle.py`): `PJV_MOVE_CASES` names the file;
/// without it nothing runs. The velocity it sets, the speed it returns, the pose it asks for and
/// the control's own state are compared after every frame.
#[cfg_attr(test, test)]
fn the_exes_ground_move_agrees_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_MOVE_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let sequences: serde_json::Value = serde_json::from_str(&text).expect("json");
    let mut frames = 0;
    for (index, sequence) in sequences.as_array().expect("sequences").iter().enumerate() {
        let cfg = &sequence["cfg"];
        let mut control = if cfg["kind"].as_str() == Some("run") {
            GroundMove::default()
        } else {
            GroundMove::plain()
        };
        let max_speed = number(&cfg["max"]);
        let accel_frames = cfg["frames"].as_i64().unwrap() as i32;
        for (step, frame) in sequence["frames"].as_array().unwrap().iter().enumerate() {
            let wall = frame["wall"].as_bool().unwrap();
            let steer = control.update(&SteerInput {
                skate: false,
                velocity: vec3(&frame["velocity"]),
                up: vec3(&frame["normal"]),
                actor_front: vec3(&frame["front"]),
                actor_up: vec3(&frame["actor_up"]),
                stick: vec3(&frame["stick"]),
                wall: wall.then(|| {
                    (
                        vec3(&frame["wall_normal"]),
                        frame["face"].as_bool().unwrap(),
                    )
                }),
                max_speed,
                accel_frames,
            });
            let at = format!("sequence {index} ({}) frame {step}: {frame}", cfg["kind"]);
            let state = &frame["state"];
            assert_eq!(
                steer.speed.to_bits(),
                number(&frame["out_speed"]).to_bits(),
                "speed, {at} got {}",
                steer.speed
            );
            assert!(
                same_bits(steer.set_velocity, vec3(&frame["set_velocity"])),
                "velocity set, {at} got {:?}",
                steer.set_velocity
            );
            let pose = &frame["pose"];
            assert_eq!(
                steer.speed.to_bits(),
                number(&pose["speed"]).to_bits(),
                "pose speed, {at}"
            );
            assert!(same_bits(steer.up, vec3(&pose["up"])), "pose up, {at}");
            assert!(
                same_bits(steer.front, vec3(&pose["front"])),
                "pose front, {at} got {:?}",
                steer.front
            );
            let flags = state["flags"].as_array().unwrap();
            assert_eq!(
                steer.brake_run,
                flags[0].as_u64() == Some(1),
                "brake run, {at}"
            );
            assert_eq!(steer.stopped, flags[1].as_u64() == Some(1), "stopped, {at}");
            assert_eq!(steer.pivot, flags[2].as_u64() == Some(1), "pivot, {at}");
            assert_eq!(
                control.brake_counter,
                state["counter"].as_i64().unwrap() as i32,
                "brake counter, {at}"
            );
            assert_eq!(
                control.hill.to_bits(),
                number(&state["hill"]).to_bits(),
                "hill, {at} got {}",
                control.hill
            );
            if cfg["kind"].as_str() == Some("run") {
                assert_eq!(
                    control.turn_power.to_bits(),
                    number(&state["power"]).to_bits(),
                    "turn power, {at} got {}",
                    control.turn_power
                );
            }
            let turn = &state["turn"];
            assert_eq!(
                control.turn.rate.to_bits(),
                number(&turn["rate"]).to_bits(),
                "turn rate, {at}"
            );
            assert!(
                same_bits(control.turn.direction, vec3(&turn["out"])),
                "turn direction, {at}"
            );
            frames += 1;
        }
    }
    assert!(frames > 0, "no cases");
}
