use super::*;

pub(super) fn run() {
    the_exes_wall_slide_agrees_bit_for_bit();
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

fn same_bits(a: Vec3, b: Vec3) -> bool {
    a.to_array().map(f32::to_bits) == b.to_array().map(f32::to_bits)
}

fn same_quat(a: Quat, b: Quat) -> bool {
    a.to_array().map(f32::to_bits) == b.to_array().map(f32::to_bits)
}

/// Cases from the executable's `PlayerStateNormalWallSlide::appear`, `followNormal` and slide
/// step, run in a local interpreter (the fidelity folder's `wslide_oracle.py`):
/// `PJV_WSLIDE_CASES` names the file; without it nothing runs.
#[cfg_attr(test, test)]
fn the_exes_wall_slide_agrees_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_WSLIDE_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let cases: serde_json::Value = serde_json::from_str(&text).expect("json");
    let mut checked = 0;
    for case in cases.as_array().expect("cases") {
        let out = &case["out"];
        let at = format!("{case}");
        match case["f"].as_str().unwrap() {
            "appear" => {
                let grab = appear(
                    vec3(&case["normal"]),
                    vec3(&case["pos"]),
                    case["face"].as_bool().unwrap(),
                    vec3(&case["trans"]),
                    55.,
                );
                assert!(same_quat(grab.pose, quat(&out["pose"])), "pose, {at}");
                assert!(
                    same_bits(grab.velocity, vec3(&out["vel"])),
                    "velocity, {at} got {:?}",
                    grab.velocity
                );
                assert!(
                    same_bits(grab.trans, vec3(&out["trans"])),
                    "trans, {at} got {:?}",
                    grab.trans
                );
                assert!(
                    same_bits(grab.slide.normal_start, vec3(&out["start"])),
                    "{at}"
                );
                assert!(same_bits(grab.slide.normal_end, vec3(&out["end"])), "{at}");
            }
            "follow" => {
                let slide = WallSlide {
                    normal_start: vec3(&case["start"]),
                    normal_end: vec3(&case["end"]),
                    side_speed: number(&case["side"]),
                };
                let wall = (case["wall"].as_bool().unwrap() && !case["no_keep"].as_bool().unwrap())
                    .then(|| vec3(&case["normal"]));
                let follow = slide.follow_normal(
                    wall,
                    quat(&case["pose_in"]),
                    vec3(&case["vel_in"]),
                    case["on_ground"].as_bool().unwrap(),
                    (FOLLOW_ANGLE_H, FOLLOW_ANGLE_V),
                );
                assert_eq!(
                    follow.holds,
                    case["ret"].as_u64() == Some(1),
                    "result, {at}"
                );
                assert!(
                    same_bits(follow.slide.normal_start, vec3(&out["start"])),
                    "start, {at} got {:?}",
                    follow.slide.normal_start
                );
                assert!(
                    same_bits(follow.slide.normal_end, vec3(&out["end"])),
                    "end, {at} got {:?}",
                    follow.slide.normal_end
                );
                assert!(
                    same_quat(follow.pose, quat(&out["pose"])),
                    "pose, {at} got {:?}",
                    follow.pose
                );
                assert!(
                    same_bits(follow.velocity, vec3(&out["vel"])),
                    "velocity, {at} got {:?}",
                    follow.velocity
                );
            }
            "slide" => {
                let end = vec3(&case["end"]);
                let mut slide = WallSlide {
                    normal_start: end,
                    normal_end: end,
                    side_speed: number(&case["side_in"]),
                };
                let velocity = slide.slide(
                    vec3(&case["vel_in"]),
                    vec3(&case["stick"]),
                    number(&case["gravity"]),
                );
                assert!(
                    same_bits(velocity, vec3(&out["vel"])),
                    "velocity, {at} got {velocity:?}"
                );
                assert_eq!(
                    slide.side_speed.to_bits(),
                    number(&out["side"]).to_bits(),
                    "side speed, {at}"
                );
            }
            other => panic!("unknown function {other}"),
        }
        checked += 1;
    }
    assert!(checked > 0, "no cases");
}
