use super::*;

pub(super) fn run() {
    a_head_on_stroke_is_shortened_to_the_minimum_speed();
    the_exes_wall_hit_scaling_agrees_bit_for_bit();
}

/// 4.5 of speed straight into the wall: three quarters is 3.375, over the minimum 3, so that is
/// what is left; 3.2 of speed is cut to the minimum 3; an oblique stroke is left alone.
#[cfg_attr(test, test)]
fn a_head_on_stroke_is_shortened_to_the_minimum_speed() {
    let wall = Vec3::new(0., 0., -1.);
    let head_on = |speed: f32| velocity_at_wall(Vec3::new(0., 1., speed), wall, 0.75, 3., 12.);
    assert_eq!(head_on(4.5), Some(Vec3::new(0., 1., 3.375)));
    assert_eq!(head_on(3.2), Some(Vec3::new(0., 1., 3.)));
    let oblique = velocity_at_wall(Vec3::new(4., 0., 1.), wall, 0.75, 3., 12.);
    assert_eq!(oblique, Some(Vec3::new(4., 0., 1.)));
    assert_eq!(
        velocity_at_wall(Vec3::new(0., 0., 4.), Vec3::Y, 0.75, 3., 12.),
        None
    );
}

/// Cases from the executable's own function in a local interpreter (the fidelity folder's
/// `wallhit_oracle.py`): `PJV_WALLHIT_CASES` names the file.
#[cfg_attr(test, test)]
fn the_exes_wall_hit_scaling_agrees_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_WALLHIT_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let cases: serde_json::Value = serde_json::from_str(&text).expect("json");
    let v3 = |v: &serde_json::Value| {
        let a = v.as_array().expect("a vector");
        Vec3::new(
            a[0].as_f64().unwrap() as f32,
            a[1].as_f64().unwrap() as f32,
            a[2].as_f64().unwrap() as f32,
        )
    };
    let num = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
    let mut checked = 0;
    for case in cases.as_array().expect("cases") {
        // The executable skips a missing wall and a `GrabCeil` one before the scaling.
        let ours = if case["wall"].as_bool().unwrap() && !case["noscale"].as_bool().unwrap() {
            velocity_at_wall(
                v3(&case["v"]),
                v3(&case["normal"]),
                num(&case["ratio"]),
                num(&case["min"]),
                num(&case["max"]),
            )
        } else {
            None
        };
        let want = case["set"].as_array().map(|_| v3(&case["set"]));
        assert_eq!(ours, want, "case {case}");
        checked += 1;
    }
    eprintln!("wall hit cases checked against the executable: {checked}");
}
