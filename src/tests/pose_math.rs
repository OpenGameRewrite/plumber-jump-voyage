use super::*;

pub(super) fn run() {
    an_untouched_vector_has_no_axis_angle();
    an_opposed_turn_takes_the_least_component_basis_axis();
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
/// `al::calcDirVerticalAny` (0x925c30) through `getAxisAngleFromTwoVec`: the basis axis of the
/// smallest absolute component, across the input; ties go to z, then y (strict comparisons).
#[cfg_attr(test, test)]
fn an_opposed_turn_takes_the_least_component_basis_axis() {
    // Straight up: x and z tie at zero and z wins the tie, so the axis is z projected across Y.
    let (axis, angle) = get_axis_angle_from_two_vec(Vec3::Y, -Vec3::Y).expect("a half turn");
    assert_eq!((axis, angle), (Vec3::Z, std::f32::consts::PI));
    // The smallest component picks its own basis axis: x when it is under both others.
    assert_eq!(calc_dir_vertical_any(Vec3::new(0., 0.6, 0.8)), Vec3::X);
    // ... y when x is not under both and y is under z.
    assert_eq!(calc_dir_vertical_any(Vec3::new(0.8, 0., 0.6)), Vec3::Y);
    // ... otherwise z, including when every component ties.
    assert_eq!(calc_dir_vertical_any(Vec3::new(0.6, 0.8, 0.)), Vec3::Z);
    let corner = Vec3::splat(1. / 3f32.sqrt());
    let axis = calc_dir_vertical_any(corner);
    assert!(
        axis.z.abs() > 0.5 && axis.dot(corner).abs() < 1e-6 && (axis.length() - 1.).abs() < 1e-6
    );
    // The turn follows the input, not glam's choice: a half turn from -Y about x-less axes.
    let (axis, _) = get_axis_angle_from_two_vec(Vec3::new(0., 0.6, 0.8), Vec3::new(0., -0.6, -0.8))
        .expect("a half turn");
    assert_eq!(axis, Vec3::X);
    // A zero vector (all components tie) still seeds the z basis, which stays unprojected.
    assert_eq!(calc_dir_vertical_any(Vec3::ZERO), Vec3::Z);
}

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
                if let Some((axis, angle)) = got {
                    assert_eq!(angle.to_bits(), number(&case["angle"]).to_bits(), "{case}");
                    assert!(same_bits(axis, vec3(&case["axis"])), "{case}");
                }
            }
            "dir_vertical" => {
                let got = calc_dir_vertical_any(vec3(&case["v"]));
                assert!(same_bits(got, vec3(&case["out"])), "{case} -> {got:?}");
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
            "rotation_rate" => {
                let got = make_quat_rotation_rate(
                    vec3(&case["a"]),
                    vec3(&case["b"]),
                    number(&case["rate"]),
                );
                assert!(same_quat_bits(got, quat(&case["out"])), "{case} -> {got:?}");
            }
            "rotation_limit" => {
                let got = make_quat_rotation_limit(
                    vec3(&case["a"]),
                    vec3(&case["b"]),
                    number(&case["limit"]),
                );
                assert!(same_quat_bits(got, quat(&case["out"])), "{case} -> {got:?}");
            }
            "turn_quat" => {
                let got = turn_quat(
                    quat(&case["q"]),
                    vec3(&case["frm"]),
                    vec3(&case["to"]),
                    number(&case["max"]),
                );
                assert!(same_quat_bits(got, quat(&case["out"])), "{case} -> {got:?}");
            }
            "turn_quat_y" => {
                let got = turn_quat_y_dir_radian(
                    quat(&case["q"]),
                    vec3(&case["dir"]),
                    number(&case["rad"]),
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
