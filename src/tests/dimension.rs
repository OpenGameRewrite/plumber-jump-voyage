use super::*;

pub(super) fn run() {
    priority_ignore_and_keeper_lifecycle();
    snap_preserves_original_near_zero_and_cap();
    executable_surface_cases();
    executable_keeper_cases();
}

#[cfg_attr(test, test)]
fn executable_keeper_cases() {
    let Some(path) = std::env::var_os("PJV_DIMENSION_KEEPER_CASES") else {
        return;
    };
    let cases: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("keeper oracle cases"))
            .expect("keeper oracle JSON");
    let cases = cases.as_array().unwrap();
    assert!(!cases.is_empty());
    for (case_index, sequence) in cases.iter().enumerate() {
        let mut keeper = DimensionKeeper::default();
        for (step_index, step) in sequence.as_array().unwrap().iter().enumerate() {
            let mut areas = DimensionAreas::default();
            for (index, spec) in step["areas"].as_array().unwrap().iter().enumerate() {
                let mut a = area(
                    spec["priority"].as_i64().unwrap() as i32,
                    spec["ignore"].as_bool().unwrap(),
                );
                a.id = index.to_string();
                if spec["inside"] == false {
                    a.volume.matrix[3] = 2000.;
                }
                areas.movement.push(a);
            }
            if step["keep"] == true {
                areas.keep = WaterAreas::from_export_named(
                    &serde_json::json!([{
                        "name": "2DKeepArea", "shape": "AreaInfinite",
                        "matrix": [1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.]
                    }]),
                    "2DKeepArea",
                );
            }
            match step["op"].as_str().unwrap() {
                "invalidate" => keeper.invalidate(),
                "validate" => keeper.valid = true,
                "force_keep" => keeper.keep_2d = true,
                "end_keep" => keeper.keep_2d = false,
                "none" => (),
                _ => unreachable!(),
            }
            keeper.update(&areas, Vec3::new(0., 20., 30.));
            let flags = [
                keeper.valid,
                keeper.is_2d,
                keeper.in_area,
                keeper.keep_2d,
                keeper.change_2d,
                keeper.change_3d,
            ]
            .map(u8::from);
            let want: Vec<u8> = step["flags"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u8)
                .collect();
            assert_eq!(
                flags.as_slice(),
                want,
                "keeper case {case_index} step {step_index}"
            );
            for (name, got) in [("current", &keeper.current), ("last", &keeper.last)] {
                let want: Vec<usize> = step[name]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as usize)
                    .collect();
                assert_eq!(
                    *got, want,
                    "keeper case {case_index} step {step_index} {name}"
                );
            }
        }
    }
}

fn area(priority: i32, ignore: bool) -> MoveArea {
    MoveArea {
        id: format!("area{priority}"),
        volume: Area {
            shape: Shape::CubeBase,
            matrix: [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0.],
            scale: Vec3::ONE,
            priority,
            ignore,
        },
        surface_distance: 5.,
        gravity_offset: 0.,
        radial_gravity_outward: true,
    }
}

#[cfg_attr(test, test)]
fn priority_ignore_and_keeper_lifecycle() {
    let inside = Vec3::new(0., 20., 30.);
    let outside = Vec3::splat(2000.);
    let mut areas = DimensionAreas {
        movement: vec![area(1, false), area(2, false), area(2, false)],
        ..Default::default()
    };
    assert_eq!(areas.select(inside), [1, 2]);
    areas.movement.insert(2, area(2, true));
    assert!(areas.select(inside).is_empty());
    areas.movement.push(area(3, false));
    assert_eq!(areas.select(inside), [4]);

    let mut keeper = DimensionKeeper::default();
    keeper.invalidate();
    keeper.update(&areas, inside);
    assert!(keeper.in_area);
    assert!(!keeper.is_2d);
    assert!(keeper.current.is_empty());
    keeper.valid = true;
    keeper.update(&areas, inside);
    assert!(keeper.change_2d && keeper.is_2d);
    assert_eq!(keeper.current, [4]);
    keeper.update(&areas, inside);
    assert!(!keeper.change_2d && !keeper.change_3d);
    keeper.keep_2d = true;
    keeper.update(&areas, outside);
    assert!(keeper.is_2d && keeper.keep_2d);
    assert!(!keeper.in_area);
    assert!(keeper.current.is_empty());
    assert_eq!(keeper.last, [4]);
    keeper.keep_2d = false;
    keeper.update(&areas, outside);
    assert!(keeper.change_3d && !keeper.is_2d);
    assert_eq!(keeper.last, [4]);
    keeper.update(&areas, inside);
    keeper.invalidate();
    assert!(keeper.change_3d && !keeper.valid && !keeper.is_2d);
    keeper.update(&areas, inside);
    assert!(keeper.in_area && !keeper.change_3d);
    // Invalid update does not refresh or clear the last valid move control.
    assert_eq!(keeper.current, [4]);

    let keep_export = serde_json::json!([{
        "id": "keep", "name": "2DKeepArea", "shape": "AreaCubeBase",
        "matrix": [1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.], "scale": [1.,1.,1.]
    }]);
    let keep_only = DimensionAreas::from_export(&keep_export);
    let mut keeper = DimensionKeeper::default();
    keeper.update(&keep_only, inside);
    assert!(keeper.is_2d && keeper.keep_2d && !keeper.in_area);
    assert!(keeper.change_2d);
    keeper.update(&keep_only, outside);
    assert!(keeper.is_2d && keeper.keep_2d);
}

#[cfg_attr(test, test)]
fn snap_preserves_original_near_zero_and_cap() {
    let areas = DimensionAreas {
        movement: vec![area(0, false)],
        ..Default::default()
    };
    let mut keeper = DimensionKeeper::default();
    let mut position = Vec3::new(0., 20., 30.);
    keeper.update(&areas, position);
    let mut velocity = Vec3::new(2., 3., 4.);
    keeper.snap(&areas, &mut position, &mut velocity, 2.);
    assert_eq!(position, Vec3::new(0., 20., 26.));
    assert_eq!(velocity, Vec3::new(2., 3., 0.));
    let unchanged = position;
    velocity.z = 10.;
    keeper.snap(&areas, &mut position, &mut velocity, NEAR_ZERO.next_down());
    assert_eq!(position, unchanged);
    assert_eq!(velocity.z, 0.);
    keeper.snap(&areas, &mut position, &mut velocity, NEAR_ZERO);
    assert_ne!(position, unchanged);
    // Tiny negative power is deliberately retained by OrGreater; it isn't
    // made positive or rounded to zero by an invented snap margin.
    let (dir, power) = areas.movement[0]
        .snap(Vec3::new(0., 20., 4.9995), 5.)
        .unwrap();
    assert!(power < 0. && power.abs() < NEAR_ZERO);
    assert_eq!(dir, Vec3::NEG_Z);
    assert_eq!(keeper.gravity(&areas, position, Vec3::X), Vec3::NEG_Y);
}

/// Optional local oracle: original executable functions on synthetic inputs,
/// never player/source recordings. No ROM or generated cases enter Git.
#[cfg_attr(test, test)]
fn executable_surface_cases() {
    let Some(path) = std::env::var_os("PJV_DIMENSION_CASES") else {
        return;
    };
    let cases: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("dimension oracle cases"))
            .expect("dimension oracle JSON");
    let array = cases.as_array().expect("case list");
    assert!(!array.is_empty());
    let vec3 = |v: &serde_json::Value| {
        Vec3::new(
            v[0].as_f64().unwrap() as f32,
            v[1].as_f64().unwrap() as f32,
            v[2].as_f64().unwrap() as f32,
        )
    };
    for (index, case) in array.iter().enumerate() {
        let areas = DimensionAreas::from_export(&serde_json::json!([case["area"]]));
        let area = &areas.movement[0];
        let position = vec3(&case["position"]);
        let compare = |got: Option<(Vec3, f32)>, name: &str| {
            if case[name].is_null() {
                assert!(got.is_none(), "case {index} {name}");
            } else {
                let (direction, power) = got.expect("successful surface query");
                assert_eq!(
                    direction.to_array().map(f32::to_bits),
                    vec3(&case[name]["direction"]).to_array().map(f32::to_bits),
                    "case {index} {name} direction"
                );
                assert_eq!(
                    power.to_bits(),
                    (case[name]["power"].as_f64().unwrap() as f32).to_bits(),
                    "case {index} {name} power"
                );
            }
        };
        compare(area.snap(position, area.surface_distance), "snap");
        compare(area.gravity(position), "gravity");
        let weighted = area.weighted_gravity(position);
        if !case["weighted"].is_null() {
            assert_eq!(
                weighted.unwrap().to_array().map(f32::to_bits),
                vec3(&case["weighted"]).to_array().map(f32::to_bits),
                "case {index} weight"
            );
        } else {
            assert!(weighted.is_none());
        }
        let keeper = DimensionKeeper {
            current: vec![0],
            ..Default::default()
        };
        let (lock, distance) = keeper.lock(&areas, position);
        assert_eq!(
            lock.to_array().map(f32::to_bits),
            vec3(&case["lock"]["direction"])
                .to_array()
                .map(f32::to_bits),
            "case {index} combined lock"
        );
        assert_eq!(
            distance.to_bits(),
            (case["lock"]["power"].as_f64().unwrap() as f32).to_bits(),
            "case {index} combined power"
        );
        assert_eq!(
            keeper
                .gravity(&areas, position, Vec3::new(0.4, -0.8, 0.3))
                .to_array()
                .map(f32::to_bits),
            vec3(&case["combined_gravity"]).to_array().map(f32::to_bits),
            "case {index} combined gravity"
        );
        let mut moved = position;
        let mut velocity = vec3(&case["velocity"]);
        keeper.snap(
            &areas,
            &mut moved,
            &mut velocity,
            case["length"].as_f64().unwrap() as f32,
        );
        assert_eq!(
            moved.to_array().map(f32::to_bits),
            vec3(&case["snap_position"]).to_array().map(f32::to_bits),
            "case {index} snapped position"
        );
        assert_eq!(
            velocity.to_array().map(f32::to_bits),
            vec3(&case["snap_velocity"]).to_array().map(f32::to_bits),
            "case {index} snapped velocity"
        );
        let up = vec3(&case["actor_up"]);
        let front = vec3(&case["actor_front"]);
        let quat_bits = |v: &serde_json::Value| {
            std::array::from_fn::<_, 4, _>(|i| (v[i].as_f64().unwrap() as f32).to_bits())
        };
        assert_eq!(
            keeper
                .snap_pose(&areas, position, up, front, false)
                .to_array()
                .map(f32::to_bits),
            quat_bits(&case["pose"]),
            "case {index} snapped pose"
        );
        let mut pipe_position = position;
        let mut pipe_velocity = Vec3::ZERO;
        keeper.snap(&areas, &mut pipe_position, &mut pipe_velocity, 500.);
        assert_eq!(
            pipe_position.to_array().map(f32::to_bits),
            vec3(&case["parallel_position"])
                .to_array()
                .map(f32::to_bits),
            "case {index} pipe position"
        );
        assert_eq!(
            keeper
                .snap_pose(&areas, pipe_position, up, front, true)
                .to_array()
                .map(f32::to_bits),
            quat_bits(&case["parallel_pose"]),
            "case {index} pipe pose"
        );
        assert!(!area.id.is_empty());
    }
}
