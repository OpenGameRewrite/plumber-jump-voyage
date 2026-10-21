//! The executable's vector routines the slope slide runs on, with their single-precision
//! arithmetic in the order the machine code does it (no fused operations): `al::calcDirSlide`,
//! `al::addVectorLimit`, `al::limitLength`, `al::parallelizeVec`, `al::verticalizeVec`,
//! `al::calcAngleDegree`, `al::turnVecToVecCosOnPlane` (with `al::turnVecToVecCos`),
//! `al::isNearZero` and `al::isFloorPolygonCos`. The normalising, cross and dot products are
//! the ones of `pose_math.rs`.
//!
//! The routines agree with the executable's own, run in a local interpreter, bit for bit (see
//! the test at the end of this file and the fidelity folder's `gen_slide_cases.py`).
use super::pose_math::{
    DEGREES_TO_RADIANS, NO_DIRECTION, atan2f, cosf, cross, dot, scaled_by_inverse_length,
    squared_length, try_normalize_or_zero,
};
use bevy::prelude::Vec3;

/// `sead::Mathf::rad2deg`'s factor, as the executable holds it.
const RADIANS_TO_DEGREES: f32 = 57.295_776;
/// `al::turnVecToVecCosOnPlane` hands this to `al::turnVecToVecCos` as the weight of the axis
/// it turns about when the two vectors are opposed.
const OPPOSED_TURN_WEIGHT: f32 = 0.02;
/// The cosine of 70 degrees: `al::isFloorPolygonCos` accepts no floor steeper than that.
const FLOOR_COS_LIMIT: f32 = 0.342_020_15;

/// `al::isNearZero(vec, tolerance)`: the vector is shorter than `tolerance`.
pub fn is_near_zero(v: Vec3, tolerance: f32) -> bool {
    squared_length(v) < tolerance * tolerance
}

/// `al::isNearZero(value, tolerance)` on a number.
pub fn is_near_zero_value(value: f32, tolerance: f32) -> bool {
    value.abs() < tolerance
}

/// `sead::Vector3f::length`.
pub fn length(v: Vec3) -> f32 {
    squared_length(v).sqrt()
}

/// `al::parallelizeVec(out, dir, vec)`: the part of `vec` along `dir` (a unit vector).
pub fn parallelize_vec(dir: Vec3, vec: Vec3) -> Vec3 {
    let along = dot(dir, vec);
    Vec3::new(dir.x * along, dir.y * along, dir.z * along)
}

/// `al::separateVectorParallelVertical(parallel, vertical, dir, vec)`.
pub fn separate_parallel_vertical(dir: Vec3, vec: Vec3) -> (Vec3, Vec3) {
    let parallel = parallelize_vec(dir, vec);
    (
        parallel,
        Vec3::new(vec.x - parallel.x, vec.y - parallel.y, vec.z - parallel.z),
    )
}

/// `al::verticalizeVec(out, vertical, vec)`: `vec` without its part along `vertical`.
pub fn verticalize_vec(vertical: Vec3, vec: Vec3) -> Vec3 {
    let along = dot(vertical, vec);
    Vec3::new(
        vec.x - vertical.x * along,
        vec.y - vertical.y * along,
        vec.z - vertical.z * along,
    )
}

/// `al::limitLength(out, vec, limit)`.
pub fn limit_length(vec: Vec3, limit: f32) -> Vec3 {
    let len = length(vec);
    if len > limit {
        let scale = limit / len;
        Vec3::new(scale * vec.x, scale * vec.y, scale * vec.z)
    } else {
        vec
    }
}

/// `al::addVectorLimit(vec, add, limit)`: `add` is added along its own direction, but not past
/// the point where `vec`'s part along that direction reaches `limit`. True when anything was
/// added.
pub fn add_vector_limit(vec: &mut Vec3, add: Vec3, limit: f32) -> bool {
    let add_length = length(add);
    let sq = squared_length(add);
    let dir = if sq < NO_DIRECTION {
        Vec3::ZERO
    } else {
        let len = sq.sqrt();
        if len > 0. {
            let inverse = 1. / len;
            Vec3::new(add.x * inverse, add.y * inverse, add.z * inverse)
        } else {
            add
        }
    };
    if squared_length(dir) < NO_DIRECTION {
        return false;
    }
    let along = dot(dir, *vec);
    if along >= limit {
        return false;
    }
    let room = limit - along;
    let amount = if room < add_length { room } else { add_length };
    *vec = Vec3::new(
        vec.x + dir.x * amount,
        vec.y + dir.y * amount,
        vec.z + dir.z * amount,
    );
    true
}

/// `al::calcAngleDegree(a, b)`: the angle between two vectors, 0 to 180.
pub fn calc_angle_degree(a: Vec3, b: Vec3) -> f32 {
    let across = cross(a, b);
    // (The library squares the cross product's components in the order z, then x and y.)
    let squared = across.z * across.z + (across.x * across.x + across.y * across.y);
    atan2f(squared.sqrt(), dot(a, b)) * RADIANS_TO_DEGREES
}

/// `al::calcDirSlide(out, gravity, normal)`: the unit vector down the plane of `normal`,
/// that is the gravity without its part along the normal, then made exactly perpendicular to
/// the normal by two cross products. Zero where the normal is parallel to the gravity (a
/// level floor: no direction is left once the normal's part is removed).
pub fn calc_dir_slide(gravity: Vec3, normal: Vec3) -> Vec3 {
    let along = dot(normal, gravity);
    let down = Vec3::new(
        gravity.x - normal.x * along,
        gravity.y - normal.y * along,
        gravity.z - normal.z * along,
    );
    if squared_length(down) < NO_DIRECTION {
        return Vec3::ZERO;
    }
    let down = scaled_by_inverse_length(down, squared_length(down).sqrt());
    // Re-orthogonalised against the normal: (normal x down) x normal.
    let side = cross(normal, down);
    let exact = cross(side, normal);
    scaled_by_inverse_length(exact, squared_length(exact).sqrt())
}

/// `al::turnVecToVecCos(out, from, to, cos, axis, weight)` for unit vectors: `from` turned
/// toward `to` until the cosine of the angle between them is `cos`. The bool is the library's
/// result (true when `to` was already within that angle); `None` is a call that wrote nothing.
fn turn_vec_to_vec_cos(
    from: Vec3,
    to: Vec3,
    cos: f32,
    axis: Vec3,
    weight: f32,
) -> Option<(Vec3, bool)> {
    if squared_length(from) < NO_DIRECTION || squared_length(to) < NO_DIRECTION {
        return None;
    }
    let between = dot(from, to);
    if between > cos {
        let len = squared_length(to).sqrt();
        let out = if len > 0. {
            let inverse = 1. / len;
            Vec3::new(inverse * to.x, inverse * to.y, inverse * to.z)
        } else {
            to
        };
        return Some((out, true));
    }
    let sine = (1. - cos * cos).sqrt();
    let across = Vec3::new(
        to.x - from.x * between,
        to.y - from.y * between,
        to.z - from.z * between,
    );
    let out = if squared_length(across) >= NO_DIRECTION {
        let across = scaled_by_inverse_length(across, squared_length(across).sqrt());
        Vec3::new(
            sine * across.x + from.x * cos,
            sine * across.y + from.y * cos,
            sine * across.z + from.z * cos,
        )
    } else {
        // Opposed vectors: turn about the axis instead.
        let turn = cross(from, axis);
        let turn = scaled_by_inverse_length(turn, squared_length(turn).sqrt());
        Vec3::new(
            turn.x * weight + from.x,
            turn.y * weight + from.y,
            turn.z * weight + from.z,
        )
    };
    Some((
        scaled_by_inverse_length(out, squared_length(out).sqrt()),
        false,
    ))
}

/// `al::turnVecToVecCosOnPlane(out, from, to, axis, cos)`: `from` and `to` are flattened onto the
/// plane with the normal `axis`, then `from` is turned toward `to` until the angle between
/// them has the cosine `cos`. Returns the new content of `out` (which is `from` itself in the
/// slide control) and the library's result; `None` when it wrote nothing.
pub fn turn_vec_to_vec_cos_on_plane(
    from: Vec3,
    to: Vec3,
    axis: Vec3,
    cos: f32,
) -> Option<(Vec3, bool)> {
    let to_flat = flatten_unit(to, axis);
    let mut from_flat = flatten_unit(from, axis);
    if squared_length(from_flat) < NO_DIRECTION {
        from_flat = -to_flat;
    }
    if squared_length(to_flat) < NO_DIRECTION {
        return None;
    }
    if cos > -1. {
        turn_vec_to_vec_cos(from_flat, to_flat, cos, axis, OPPOSED_TURN_WEIGHT)
    } else {
        Some((to_flat, true))
    }
}

/// The vector without its part along `axis`, as a unit vector (zero when nothing is left).
fn flatten_unit(v: Vec3, axis: Vec3) -> Vec3 {
    let flat = verticalize_vec(axis, v);
    let (unit, _) = try_normalize_or_zero(flat);
    unit
}

/// `al::isFloorPolygonCos(normal, gravity, cos)`: the normal is a floor within the angle whose
/// cosine is `cos` of straight up, and not steeper than 70 degrees.
pub fn is_floor_polygon_cos(normal: Vec3, gravity: Vec3, cos: f32) -> bool {
    if is_near_zero(normal, 0.001) {
        return false;
    }
    let against = dot(normal, gravity);
    if -against < cos {
        return false;
    }
    let magnitude = if against > 0. { against } else { -against };
    against < 0. && magnitude >= FLOOR_COS_LIMIT
}

/// The cosine of a limit given in degrees, as the executable computes it (`cosf` of the
/// radians, in single precision).
pub fn cos_of_degrees(degrees: f32) -> f32 {
    cosf(degrees * DEGREES_TO_RADIANS)
}

/// `al::converge(value, target, step)` on integers.
pub fn converge(value: i32, target: i32, step: i32) -> i32 {
    if value < target {
        (value + step).min(target)
    } else {
        (value - step).max(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn same(a: Vec3, b: Vec3) -> bool {
        a.to_array().map(f32::to_bits) == b.to_array().map(f32::to_bits)
    }

    /// Cases from the executable's own routines, run in a local interpreter (see the fidelity
    /// folder's `gen_slide_cases.py`): `PJV_SLIDE_CASES` names the file; without it nothing runs.
    #[test]
    fn the_exes_vector_routines_agree_bit_for_bit() {
        let Some(path) = std::env::var_os("PJV_SLIDE_CASES") else {
            return;
        };
        let text = std::fs::read_to_string(path).expect("the cases file");
        let cases: serde_json::Value = serde_json::from_str(&text).expect("json");
        for case in cases.as_array().expect("cases") {
            let name = case["f"].as_str().unwrap();
            match name {
                "calc_dir_slide" => {
                    let got = calc_dir_slide(vec3(&case["gravity"]), vec3(&case["normal"]));
                    assert!(same(got, vec3(&case["out"])), "{case} -> {got:?}");
                }
                "add_vector_limit" => {
                    let mut vec = vec3(&case["vec"]);
                    let ok = add_vector_limit(&mut vec, vec3(&case["add"]), number(&case["limit"]));
                    assert_eq!(ok, case["ok"].as_bool().unwrap(), "{case}");
                    assert!(same(vec, vec3(&case["out"])), "{case} -> {vec:?}");
                }
                "limit_length" => {
                    let got = limit_length(vec3(&case["vec"]), number(&case["limit"]));
                    assert!(same(got, vec3(&case["out"])), "{case} -> {got:?}");
                }
                "parallelize_vec" => {
                    let got = parallelize_vec(vec3(&case["dir"]), vec3(&case["vec"]));
                    assert!(same(got, vec3(&case["out"])), "{case} -> {got:?}");
                }
                "verticalize_vec" => {
                    let got = verticalize_vec(vec3(&case["vertical"]), vec3(&case["vec"]));
                    assert!(same(got, vec3(&case["out"])), "{case} -> {got:?}");
                }
                "separate" => {
                    let (parallel, vertical) =
                        separate_parallel_vertical(vec3(&case["dir"]), vec3(&case["vec"]));
                    assert!(
                        same(parallel, vec3(&case["parallel"])),
                        "{case} -> {parallel:?}"
                    );
                    assert!(
                        same(vertical, vec3(&case["vertical"])),
                        "{case} -> {vertical:?}"
                    );
                }
                "angle_degree" => {
                    let got = calc_angle_degree(vec3(&case["a"]), vec3(&case["b"]));
                    assert_eq!(
                        got.to_bits(),
                        number(&case["out"]).to_bits(),
                        "{case} -> {got}"
                    );
                }
                "turn_on_plane" => {
                    let from = vec3(&case["from"]);
                    let turned = turn_vec_to_vec_cos_on_plane(
                        from,
                        vec3(&case["to"]),
                        vec3(&case["axis"]),
                        number(&case["cos"]),
                    );
                    let (got, reached) = turned.unwrap_or((from, false));
                    assert_eq!(reached, case["ok"].as_bool().unwrap(), "{case}");
                    assert!(same(got, vec3(&case["out"])), "{case} -> {got:?}");
                }
                "floor_polygon_cos" => {
                    let got = is_floor_polygon_cos(
                        vec3(&case["normal"]),
                        vec3(&case["gravity"]),
                        number(&case["cos"]),
                    );
                    assert_eq!(got, case["out"].as_bool().unwrap(), "{case}");
                }
                other => panic!("unknown case {other}"),
            }
        }
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1.0e-5
    }

    #[test]
    fn the_way_down_a_ramp_lies_in_its_plane() {
        let normal = Vec3::new(0.7, 0.7, 0.).normalize();
        let down = calc_dir_slide(Vec3::NEG_Y, normal);
        assert!(down.dot(normal).abs() < 1.0e-6);
        assert!(close(down, Vec3::new(0.7, -0.7, 0.).normalize()));
    }

    #[test]
    fn a_level_floor_has_no_way_down() {
        assert_eq!(calc_dir_slide(Vec3::NEG_Y, Vec3::Y), Vec3::ZERO);
    }

    #[test]
    fn adding_along_a_direction_stops_at_the_limit() {
        let mut v = Vec3::new(9., 0., 0.);
        assert!(add_vector_limit(&mut v, Vec3::new(3., 0., 0.), 10.));
        assert!((v.x - 10.).abs() < 1.0e-6);
        assert!(!add_vector_limit(&mut v, Vec3::new(1., 0., 0.), 10.));
    }

    #[test]
    fn a_turn_by_cosine_stops_short_of_the_target() {
        let (out, reached) =
            turn_vec_to_vec_cos_on_plane(Vec3::Z, Vec3::X, Vec3::Y, 15.0f32.to_radians().cos())
                .unwrap();
        assert!(!reached);
        assert!((calc_angle_degree(out, Vec3::Z) - 15.).abs() < 1.0e-3);
        let (done, reached) =
            turn_vec_to_vec_cos_on_plane(Vec3::Z, Vec3::new(0.1, 0., 1.), Vec3::Y, 0.9).unwrap();
        assert!(reached);
        assert!(close(done, Vec3::new(0.1, 0., 1.).normalize()));
    }

    #[test]
    fn floors_are_told_by_their_angle() {
        let flat = 10.0f32.to_radians().cos();
        assert!(is_floor_polygon_cos(Vec3::Y, Vec3::NEG_Y, flat));
        let steep = Vec3::new(0.7, 0.7, 0.).normalize();
        assert!(!is_floor_polygon_cos(steep, Vec3::NEG_Y, flat));
        assert!(!is_floor_polygon_cos(
            steep,
            Vec3::NEG_Y,
            26.0f32.to_radians().cos()
        ));
    }
}
