use super::*;

pub(super) fn run() {
    the_exes_ground_move_agrees_bit_for_bit();
    skate_keeps_signed_inertia_and_forces_reversal();
}

#[cfg_attr(test, test)]
fn skate_keeps_signed_inertia_and_forces_reversal() {
    let mut control = GroundMove::default();
    let mut input = SteerInput {
        skate: true,
        velocity: Vec3::ZERO,
        up: Vec3::Y,
        actor_front: Vec3::Z,
        actor_up: Vec3::Y,
        stick: Vec3::Z,
        wall: None,
        max_speed: 14.,
        accel_frames: 40,
    };
    let first = control.update(&input);
    assert_eq!(first.velocity.z.to_bits(), (14_f32 / 60.).to_bits());
    // A mature slide retains its signed side component at a 90-degree input
    // change, then a reverse uses high braking while facing turns independently.
    control.brake_counter = control.border;
    input.velocity = Vec3::new(10., 0., 8.);
    input.stick = Vec3::Z;
    let crossed = control.update(&input);
    assert_eq!(
        crossed.velocity.x.to_bits(),
        (10_f32 - 14. / 120.).to_bits()
    );
    input.velocity = Vec3::Z * 10.;
    input.stick = Vec3::NEG_Z;
    let reversed = control.update(&input);
    assert_eq!(
        reversed.velocity.z.to_bits(),
        (10_f32 - 14. / 60.).to_bits()
    );
    assert!(!reversed.brake_run && !reversed.pivot && reversed.slip_ice);
    assert!(control.turn.reverse_forced);
    assert_ne!(reversed.front, input.actor_front);
    input.velocity = first.velocity;
    input.stick = Vec3::ZERO;
    let released = control.update(&input);
    assert_eq!(
        released.velocity.z.to_bits(),
        (14_f32 / 60. - 14. / 120.).to_bits()
    );
    assert!(!released.stopped);
    control.brake_counter = control.border;
    control.resume(18., 1, 0., Vec3::ZERO, 0., true);
    assert_eq!(
        control.brake_counter, control.border,
        "fast arrival hold survives injection"
    );
    control.resume(1., 80, 0., Vec3::ZERO, 0., true);
    assert_eq!(
        control.brake_counter, control.border,
        "slow ice retains its hold"
    );
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
