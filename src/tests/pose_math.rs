use super::*;

pub(super) fn run() {
    an_untouched_vector_has_no_axis_angle();
    the_exes_pose_routines_agree_bit_for_bit();
    the_exes_follow_of_a_collision_part_agrees_bit_for_bit();
}

#[cfg_attr(test, test)]
fn an_untouched_vector_has_no_axis_angle() {
    assert!(get_axis_angle_from_two_vec(Vec3::Y, Vec3::Y).is_none());
    let (axis, angle) = get_axis_angle_from_two_vec(Vec3::Y, Vec3::X).expect("a quarter turn");
    assert!((angle - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    assert!((axis - Vec3::new(0., 0., -1.)).length() < 1e-6);
}

fn vec3(v: &serde_json::Value) -> Vec3 {
    let a = v.as_array().expect("array");
    Vec3::new(
        a[0].as_f64().unwrap() as f32,
        a[1].as_f64().unwrap() as f32,
        a[2].as_f64().unwrap() as f32,
    )
}

fn quat(v: &serde_json::Value) -> Quat {
    let a = v.as_array().expect("array");
    Quat::from_xyzw(
        a[0].as_f64().unwrap() as f32,
        a[1].as_f64().unwrap() as f32,
        a[2].as_f64().unwrap() as f32,
        a[3].as_f64().unwrap() as f32,
    )
}

fn number(v: &serde_json::Value) -> f32 {
    v.as_f64().unwrap() as f32
}

/// The 3x4 row-major matrix of a case as its three columns.
fn axes(v: &serde_json::Value) -> Axes {
    let m: Vec<f32> = v.as_array().unwrap().iter().map(number).collect();
    [
        Vec3::new(m[0], m[4], m[8]),
        Vec3::new(m[1], m[5], m[9]),
        Vec3::new(m[2], m[6], m[10]),
    ]
}

fn same_bits(a: Vec3, b: Vec3) -> bool {
    a.to_array().map(f32::to_bits) == b.to_array().map(f32::to_bits)
}

fn same_quat_bits(a: Quat, b: Quat) -> bool {
    a.to_array().map(f32::to_bits) == b.to_array().map(f32::to_bits)
}

/// Cases from the executable's own functions, run in a local interpreter (see the fidelity
/// folder's `gen_pose_cases.py`): `PJV_POSE_CASES` names the file; without it nothing runs.
#[cfg_attr(test, test)]
fn the_exes_pose_routines_agree_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_POSE_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let cases: serde_json::Value = serde_json::from_str(&text).expect("json");
    let mut checked = 0;
    for case in cases.as_array().expect("cases") {
        let name = case["f"].as_str().unwrap();
        match name {
            "normalize" => {
                let got = normalize(vec3(&case["v"]));
                assert!(same_bits(got, vec3(&case["out"])), "{case} -> {got:?}");
            }
            "try_normalize" => {
                let (got, ok) = try_normalize_or_zero(vec3(&case["v"]));
                assert_eq!(ok, case["ok"].as_bool().unwrap(), "{case}");
                assert!(same_bits(got, vec3(&case["out"])), "{case} -> {got:?}");
            }
            "axis_angle" => {
                let got = get_axis_angle_from_two_vec(vec3(&case["a"]), vec3(&case["b"]));
                let want_ok = case["ok"].as_bool().unwrap();
                assert_eq!(got.is_some(), want_ok, "{case}");
                // (An exactly opposed pair takes the guessed axis: its angle only.)
                if let Some((axis, angle)) = got {
                    assert_eq!(angle.to_bits(), number(&case["angle"]).to_bits(), "{case}");
                    let opposed = angle == std::f32::consts::PI;
                    assert!(opposed || same_bits(axis, vec3(&case["axis"])), "{case}");
                }
            }
            "along" => {
                let got = along_vector_normal_h(
                    vec3(&case["vec"]),
                    vec3(&case["frm"]),
                    vec3(&case["to"]),
                );
                assert!(same_bits(got, vec3(&case["out"])), "{case} -> {got:?}");
            }
            "turn" => {
                let (frm, to) = (vec3(&case["frm"]), vec3(&case["to"]));
                if get_axis_angle_from_two_vec(frm, to)
                    .is_some_and(|(_, a)| a == std::f32::consts::PI)
                {
                    continue;
                }
                let got = turn_vec_to_vec_degree(frm, to, number(&case["deg"]));
                assert!(same_bits(got, vec3(&case["out"])), "{case} -> {got:?}");
            }
            "mtx_up_front" => {
                let got = make_mtx_up_front(vec3(&case["up"]), vec3(&case["front"]));
                let want = axes(&case["mtx"]);
                assert!(
                    (0..3).all(|i| same_bits(got[i], want[i])),
                    "{case} -> {got:?}"
                );
            }
            "mtx_front_up" => {
                let got = make_mtx_front_up(vec3(&case["front"]), vec3(&case["up"]));
                let want = axes(&case["mtx"]);
                assert!(
                    (0..3).all(|i| same_bits(got[i], want[i])),
                    "{case} -> {got:?}"
                );
            }
            "quat_up_front" => {
                let got = quat_up_front(vec3(&case["up"]), vec3(&case["front"]));
                assert!(same_quat_bits(got, quat(&case["out"])), "{case} -> {got:?}");
            }
            "quat_front_up" => {
                let got = quat_front_up(vec3(&case["front"]), vec3(&case["up"]));
                assert!(same_quat_bits(got, quat(&case["out"])), "{case} -> {got:?}");
            }
            "quat_to_mtx" => {
                let got = quat_to_axes(quat(&case["q"]));
                let want = axes(&case["mtx"]);
                assert!(
                    (0..3).all(|i| same_bits(got[i], want[i])),
                    "{case} -> {got:?}"
                );
            }
            "to_quat" => {
                let got = to_quat(axes(&case["mtx"]));
                assert!(same_quat_bits(got, quat(&case["out"])), "{case} -> {got:?}");
            }
            "slerp" => {
                let got = slerp(quat(&case["a"]), quat(&case["b"]), number(&case["rate"]));
                assert!(same_quat_bits(got, quat(&case["out"])), "{case} -> {got:?}");
            }
            "slerp_up_front" => {
                let pose = ActorPose::new(quat(&case["stored"]));
                let got = pose.slerp_up_front(
                    vec3(&case["up"]),
                    vec3(&case["front"]),
                    number(&case["rate"]),
                    number(&case["max"]),
                );
                assert!(same_quat_bits(got, quat(&case["out"])), "{case} -> {got:?}");
            }
            "wait_ground" => {
                let pose = ActorPose::new(quat(&case["stored"]));
                let got = pose.wait_ground(
                    vec3(&case["normal"]),
                    number(&case["rate"]),
                    number(&case["max"]),
                );
                assert!(same_quat_bits(got, quat(&case["out"])), "{case} -> {got:?}");
            }
            other => panic!("unknown case {other}"),
        }
        checked += 1;
    }
    eprintln!("pose cases checked against the executable: {checked}");
}

/// `al::followRotateFrontAxisUp` from the executable (the fidelity folder's `follow_oracle.py`):
/// `PJV_FOLLOW_CASES` names the file; without it nothing runs. Each case is a stored pose, the
/// part's base matrix and previous inverse, and the quaternion the executable stores.
#[cfg_attr(test, test)]
fn the_exes_follow_of_a_collision_part_agrees_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_FOLLOW_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let cases: serde_json::Value = serde_json::from_str(&text).expect("json");
    let matrix = |v: &serde_json::Value| -> [f32; 12] {
        let list: Vec<f32> = v.as_array().unwrap().iter().map(number).collect();
        list.try_into().unwrap()
    };
    let cases = cases.as_array().expect("cases");
    for (index, case) in cases.iter().enumerate() {
        let pose = ActorPose::new(quat(&case["stored"]));
        let got = pose.follow_part(&matrix(&case["inverse"]), &matrix(&case["base"]));
        assert!(
            same_quat_bits(got, quat(&case["out"])),
            "case {index}: {case} got {got:?}"
        );
    }
    assert!(!cases.is_empty(), "no cases");
}
