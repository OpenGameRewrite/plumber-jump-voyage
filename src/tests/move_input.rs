use super::*;

pub(super) fn run() {
    the_exes_move_input_agrees_bit_for_bit();
    the_locked_view_is_the_probes();
}

fn floats<const N: usize>(v: &serde_json::Value) -> [f32; N] {
    let a = v.as_array().expect("array");
    std::array::from_fn(|i| a[i].as_f64().unwrap() as f32)
}

/// Cases from the executable's `PlayerInput::calcMoveInput` (ordinary 3D path, no wall bend) run
/// in a local interpreter (the fidelity folder's `move_input_oracle.py`): `PJV_MOVE_INPUT_CASES`
/// names the file; without it nothing runs.
#[cfg_attr(test, test)]
fn the_exes_move_input_agrees_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_MOVE_INPUT_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let cases: serde_json::Value = serde_json::from_str(&text).expect("json");
    let cases = cases.as_array().expect("cases");
    for (index, case) in cases.iter().enumerate() {
        let stick = floats::<2>(&case["stick"]);
        let m = floats::<12>(&case["view"]);
        let view: ViewRows = [
            Vec3::new(m[0], m[1], m[2]),
            Vec3::new(m[4], m[5], m[6]),
            Vec3::new(m[8], m[9], m[10]),
        ];
        let up = Vec3::from(floats::<3>(&case["up"]));
        let want = floats::<3>(&case["out"]);
        let got = move_input(Vec2::from(stick), &view, up);
        assert_eq!(
            got.to_array().map(f32::to_bits),
            want.map(f32::to_bits),
            "case {index}: stick {stick:?} view {m:?} up {up:?}: got {got:?}"
        );
    }
    assert!(!cases.is_empty(), "no cases");
}

/// The locked camera's matrix as the probe wrote it (recorded `bits_cam_view` of a yaw-90 and a
/// yaw-0 run; the double precision cosine of a quarter turn is not zero).
#[cfg_attr(test, test)]
fn the_locked_view_is_the_probes() {
    let yaw_90 = locked_view(90.);
    assert_eq!(yaw_90[0].x.to_bits(), 0xa48d3132);
    assert_eq!(yaw_90[0].z.to_bits(), 0x3f800000);
    assert_eq!(yaw_90[2].x.to_bits(), 0xbf800000);
    assert_eq!(yaw_90[2].z.to_bits(), 0xa48d3132);
    let yaw_0 = locked_view(0.);
    assert_eq!(yaw_0[0].x.to_bits(), 0xbf800000);
    assert_eq!(yaw_0[2].x.to_bits(), 0x80000000);
    assert_eq!(yaw_0[2].z.to_bits(), 0xbf800000);
}
