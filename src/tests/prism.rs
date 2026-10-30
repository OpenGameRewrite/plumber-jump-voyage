use super::*;

pub(super) fn run() {
    a_rotated_part_is_inverted_exactly_enough();
    a_sphere_over_a_floor_prism_finds_the_face();
    a_sphere_beside_an_edge_finds_the_edge();
    an_arrow_crosses_the_prism_inside_its_edges_only();
    the_corners_of_a_prism_are_where_the_edges_meet();
    the_exes_prism_tests_agree_bit_for_bit();
    the_exes_face_data_agrees_bit_for_bit();
    disks_use_their_axial_slab_and_strict_face_boundary();
    the_exes_disk_tests_agree_bit_for_bit();
}

#[cfg_attr(test, test)]
fn disks_use_their_axial_slab_and_strict_face_boundary() {
    let p = floor_prism();
    assert_eq!(
        p.disk_test([20., 50., -20.], 55., 1., 5., [0., 0., 1.]),
        Some((5., 1))
    );
    assert!(
        p.disk_test([20., 55., -20.], 55., 1., 5., [0., 0., 1.])
            .is_none()
    );
    assert!(
        p.disk_test([20., 50., -20.], 55., 1., 5., [0., 1., 0.])
            .is_none()
    );
}

#[cfg_attr(test, test)]
fn the_exes_disk_tests_agree_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_DISK_CASES") else {
        return;
    };
    let cases: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("disk oracle cases"))
            .expect("disk oracle JSON");
    let cases = cases.as_array().unwrap();
    assert!(!cases.is_empty());
    let point = |v: &serde_json::Value| std::array::from_fn(|i| v[i].as_f64().unwrap() as f32);
    for (index, case) in cases.iter().enumerate() {
        let p = &case["prism"];
        let prism = Prism {
            length: p["length"].as_f64().unwrap() as f32,
            origin: point(&p["origin"]),
            face: point(&p["face"]),
            edges: std::array::from_fn(|i| point(&p["edges"][i])),
            thickness: p["thickness"].as_f64().unwrap() as f32,
        };
        let got = prism.disk_test(
            point(&case["center"]),
            case["radius"].as_f64().unwrap() as f32,
            case["scale"].as_f64().unwrap() as f32,
            case["half_height"].as_f64().unwrap() as f32,
            point(&case["axis"]),
        );
        if case["hit"].is_null() {
            assert!(got.is_none(), "disk case {index}: {got:?}");
        } else {
            let (depth, region) =
                got.unwrap_or_else(|| panic!("disk case {index}: missing {:?}", case["hit"]));
            assert_eq!(
                region,
                case["hit"][1].as_u64().unwrap() as u8,
                "disk case {index} region"
            );
            assert_eq!(
                depth.to_bits(),
                (case["hit"][0].as_f64().unwrap() as f32).to_bits(),
                "disk case {index} depth"
            );
            let position = prism.sphere_hit_position(point(&case["center"]), region);
            let expected: [f32; 3] = point(&case["position"]);
            assert_eq!(
                position.map(f32::to_bits),
                expected.map(f32::to_bits),
                "disk case {index} position"
            );
        }
    }
}

/// A floor triangle in the plane y = 0 with corners (0,0,0), (100,0,0) and (0,0,-100), face up.
/// As in a KCL file, the edge normals point out of the triangle in its plane (edge 1 belongs to
/// the side from the first corner to the third, edge 2 to the first-second one, edge 3 to the
/// far one, whose plane lies `length` from the first corner), and the prism is 40 thick.
fn floor_prism() -> Prism {
    let s = std::f32::consts::FRAC_1_SQRT_2;
    Prism {
        length: 100. * s,
        origin: [0., 0., 0.],
        face: [0., 1., 0.],
        edges: [[-1., 0., 0.], [0., 0., 1.], [s, 0., -s]],
        thickness: 40.,
    }
}

#[cfg_attr(test, test)]
fn a_rotated_part_is_inverted_exactly_enough() {
    // Turned 45 degrees about the up axis and moved: a point goes there and back.
    let s = std::f32::consts::FRAC_1_SQRT_2;
    let base: Matrix34 = [s, 0., s, 6293.25, 0., 1., 0., 0., -s, 0., s, -5303.301];
    let frame = PartFrame::new(base).expect("a rotation has an inverse");
    let local = [2165.332, 5335.404, -1021.4045];
    let world = transform_point(&frame.base, local);
    let back = transform_point(&frame.inverse, world);
    for k in 0..3 {
        assert!((back[k] - local[k]).abs() < 2e-3, "{back:?} {local:?}");
    }
    assert!((frame.inverse_scale - 1.).abs() < 1e-6);
    assert!(PartFrame::new([0.; 12]).is_none());
}

#[cfg_attr(test, test)]
fn a_sphere_over_a_floor_prism_finds_the_face() {
    let prism = floor_prism();
    // 50 above the floor with radius 55: 5 deep, on the face.
    let (depth, region) = prism
        .sphere_test([20., 50., -20.], 55., 1.)
        .expect("touching");
    assert_eq!(region, 1);
    assert!((depth - 5.).abs() < 1e-4, "{depth}");
    // Too high, or beyond the prism's thickness below the face: nothing.
    assert!(prism.sphere_test([20., 60., -20.], 55., 1.).is_none());
    assert!(prism.sphere_test([20., -41., -20.], 55., 1.).is_none());
}

#[cfg_attr(test, test)]
fn a_sphere_beside_an_edge_finds_the_edge() {
    let prism = floor_prism();
    // Outside the x = 0 side by 10 and 30 above the face: the edge.
    let (depth, region) = prism
        .sphere_test([-10., 30., -20.], 55., 1.)
        .expect("touching the edge");
    assert_eq!(region, 2);
    // How far the centre must rise along the face normal for the sphere to just touch the edge.
    let rise = (55f32 * 55. - 10. * 10.).sqrt() - 30.;
    assert!((depth - rise).abs() < 1e-3, "{depth}");
    let hit = prism.sphere_hit_position([-10., 30., -20.], region);
    assert!(hit[0].abs() < 1e-4 && hit[1].abs() < 1e-4 && (hit[2] + 20.).abs() < 1e-4);
}

#[cfg_attr(test, test)]
fn an_arrow_crosses_the_prism_inside_its_edges_only() {
    let prism = floor_prism();
    let t = prism
        .arrow_test([20., 40., -20.], [0., -60., 0.])
        .expect("crosses");
    assert!((t - 40. / 60.).abs() < 1e-6);
    // Starting below the face, ending above it, or crossing outside the edges: no hit.
    assert!(prism.arrow_test([20., -1., -20.], [0., -60., 0.]).is_none());
    assert!(prism.arrow_test([20., 40., -20.], [0., 10., 0.]).is_none());
    assert!(prism.arrow_test([-5., 40., -20.], [0., -60., 0.]).is_none());
    assert!(prism.arrow_test([80., 40., -80.], [0., -60., 0.]).is_none());
}

#[cfg_attr(test, test)]
fn the_corners_of_a_prism_are_where_the_edges_meet() {
    let prism = floor_prism();
    assert_eq!(prism.corner(0), [0., 0., 0.]);
    let second = prism.corner(1);
    let third = prism.corner(2);
    assert!((second[0] - 100.).abs() < 1e-3 && second[1].abs() < 1e-4 && second[2].abs() < 1e-3);
    assert!(third[0].abs() < 1e-3 && third[1].abs() < 1e-4 && (third[2] + 100.).abs() < 1e-3);
}

/// Cases from the executable's own functions, run in a local interpreter (see the fidelity
/// folder's `gen_prism_cases.py`): `PJV_PRISM_CASES` names the file; without it nothing runs.
#[cfg_attr(test, test)]
fn the_exes_prism_tests_agree_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_PRISM_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let cases: serde_json::Value = serde_json::from_str(&text).expect("json");
    let f3 = |v: &serde_json::Value| -> [f32; 3] {
        let a = v.as_array().expect("array");
        [0, 1, 2].map(|i| a[i].as_f64().expect("number") as f32)
    };
    let mut checked = 0;
    for case in cases.as_array().expect("cases") {
        let row = case["prism"].as_array().expect("prism");
        let n = |i: usize| row[i].as_f64().expect("number") as f32;
        let prism = Prism {
            length: n(0),
            origin: [n(1), n(2), n(3)],
            face: [n(4), n(5), n(6)],
            edges: [
                [n(7), n(8), n(9)],
                [n(10), n(11), n(12)],
                [n(13), n(14), n(15)],
            ],
            thickness: case["thickness"].as_f64().unwrap() as f32,
        };
        for (i, expected) in case["corners"].as_array().unwrap().iter().enumerate() {
            assert_eq!(prism.corner(i), f3(expected), "corner {i} of {case}");
        }
        let centre = f3(&case["centre"]);
        let radius = case["radius"].as_f64().unwrap() as f32;
        let mul = case["mul"].as_f64().unwrap() as f32;
        let ours = prism.sphere_test(centre, radius, mul);
        match (&case["sphere"], ours) {
            (serde_json::Value::Null, None) => {}
            (expected, Some((depth, region))) if !expected.is_null() => {
                let want = expected["depth"].as_f64().unwrap() as f32;
                assert_eq!(depth.to_bits(), want.to_bits(), "depth of {case}");
                assert_eq!(u64::from(region), expected["kind"].as_u64().unwrap());
                let at = prism.sphere_hit_position(centre, region);
                assert_eq!(at, f3(&expected["pos"]), "hit position of {case}");
            }
            (expected, ours) => panic!("sphere differs: {expected} vs {ours:?} for {case}"),
        }
        let start = f3(&case["arrow_in"]["start"]);
        let vector = f3(&case["arrow_in"]["vector"]);
        let ours = prism.arrow_test(start, vector);
        match (&case["arrow"], ours) {
            (serde_json::Value::Null, None) => {}
            (expected, Some(t)) if !expected.is_null() => {
                let want = expected.as_f64().unwrap() as f32;
                assert_eq!(t.to_bits(), want.to_bits(), "arrow t of {case}");
            }
            (expected, ours) => panic!("arrow differs: {expected} vs {ours:?} for {case}"),
        }
        checked += 1;
    }
    eprintln!("prism cases checked against the executable: {checked}");
}

/// The world normals of a prism under its part's matrix (`Triangle::fillData`) from the
/// executable's own function in a local interpreter (the fidelity folder's `fill_oracle.py`, fed
/// with the prisms and part matrices of a stage export): `PJV_FILL_CASES` names the file.
#[cfg_attr(test, test)]
fn the_exes_face_data_agrees_bit_for_bit() {
    let Some(path) = std::env::var_os("PJV_FILL_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let cases: serde_json::Value = serde_json::from_str(&text).expect("json");
    let f3 = |v: &serde_json::Value| -> [f32; 3] {
        let a = v.as_array().expect("array");
        [0, 1, 2].map(|i| a[i].as_f64().expect("number") as f32)
    };
    let mut checked = 0;
    for case in cases.as_array().expect("cases") {
        let row = case["prism"].as_array().expect("prism");
        let n = |i: usize| row[i].as_f64().expect("number") as f32;
        let prism = Prism {
            length: n(0),
            origin: [n(1), n(2), n(3)],
            face: [n(4), n(5), n(6)],
            edges: [
                [n(7), n(8), n(9)],
                [n(10), n(11), n(12)],
                [n(13), n(14), n(15)],
            ],
            thickness: case["thickness"].as_f64().unwrap() as f32,
        };
        let m = case["matrix"].as_array().expect("matrix");
        let base: Matrix34 = std::array::from_fn(|i| m[i].as_f64().expect("number") as f32);
        let frame = PartFrame::new(base).expect("a rotation has an inverse");
        let ours = prism.world_triangle(&frame);
        assert_eq!(ours.normal, f3(&case["face"]), "face normal of {case}");
        for (k, expected) in case["edges"].as_array().unwrap().iter().enumerate() {
            assert_eq!(ours.edges[k], f3(expected), "edge normal {k} of {case}");
        }
        checked += 1;
    }
    eprintln!("face data cases checked against the executable: {checked}");
}
