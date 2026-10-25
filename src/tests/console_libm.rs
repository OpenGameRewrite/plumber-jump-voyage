use super::*;

pub(super) fn run() {
    the_exes_library_agrees_bit_for_bit();
}

/// Cases from the console's library run in the local interpreter (the fidelity folder's
/// `gen_libm_cases.py`): `PJV_LIBM_CASES` names the file; without it nothing runs.
#[cfg_attr(test, test)]
fn the_exes_library_agrees_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_LIBM_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let cases: serde_json::Value = serde_json::from_str(&text).expect("json");
    let mut checked = 0;
    for case in cases.as_array().expect("cases") {
        let name = case["f"].as_str().unwrap();
        let args: Vec<f32> = case["in"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| f32::from_bits(v.as_u64().unwrap() as u32))
            .collect();
        let want = case["out"].as_u64().unwrap() as u32;
        let got = match name {
            "sinf" => sinf(args[0]),
            "cosf" => cosf(args[0]),
            "acosf" => acosf(args[0]),
            "asinf" => asinf(args[0]),
            "atanf" => atanf(args[0]),
            "atan2f" => atan2f(args[0], args[1]),
            other => panic!("unknown function {other}"),
        };
        assert!(
            got.to_bits() == want || (got.is_nan() && f32::from_bits(want).is_nan()),
            "{name}{args:?}: got {got:e} ({:#x}), the library {:#x}",
            got.to_bits(),
            want
        );
        checked += 1;
    }
    assert!(checked > 0, "no cases");
}
