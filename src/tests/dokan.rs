use super::*;

pub(super) fn run() {
    side_bind_judge_follows_the_original_gates();
    original_mouth_and_puppet_pose_operations();
    executable_entry_cases();
}

/// `judgeMsgBindStart` (0x25b2f0) side branch: speed along the pipe's up axis above 0.4,
/// facing against that axis, and the player below the pipe's origin.
#[cfg_attr(test, test)]
fn side_bind_judge_follows_the_original_gates() {
    let source = serde_json::json!({
        "placement": {
            "Id": "synthetic", "UnitConfigName": "Dokan",
            "IsConnectToCollision": false, "IsNoSnap": false, "IsMoon": false,
            "IsExitOnly": false, "Scale": {"X": 1., "Y": 1., "Z": 1.}
        },
        "destination_id": "other",
        // Side direction: the up axis is world +X, the front -Z.
        "raw_matrix": [0., 1., 0., 0., 1., 0., 0., 0., 0., 0., -1., 0.],
        "sensors": [{"Name": "Bind", "Radius": 80., "Y": 120.}]
    });
    let pipe = Dokan::load(&source, &DimensionAreas::default()).unwrap();
    let (below, facing, going) = (Vec3::new(50., -30., 0.), Vec3::NEG_X, Vec3::NEG_X);
    assert!(pipe.accepts_side_bind(below, facing, going));
    assert!(
        !pipe.accepts_side_bind(below, facing, Vec3::ZERO),
        "standing still"
    );
    assert!(
        !pipe.accepts_side_bind(below, facing, Vec3::new(-0.3, 0., 0.)),
        "0.4 limit"
    );
    assert!(
        !pipe.accepts_side_bind(below, Vec3::X, going),
        "facing along the up axis"
    );
    assert!(
        !pipe.accepts_side_bind(Vec3::new(50., 30., 0.), facing, going),
        "above the origin"
    );
}

#[cfg_attr(test, test)]
fn executable_entry_cases() {
    let Some(path) = std::env::var_os("PJV_DOKAN_CASES") else {
        return;
    };
    let cases: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("Dokan oracle cases"))
            .expect("Dokan oracle JSON");
    let cases = cases.as_array().unwrap();
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let pipe = Dokan::load(&case["source"], &DimensionAreas::default()).unwrap();
        let target = pipe.side_entry_target().unwrap();
        let bits = |v: &serde_json::Value| {
            v.as_array()
                .unwrap()
                .iter()
                .map(|n| (n.as_f64().unwrap() as f32).to_bits())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            target.map(f32::to_bits).as_slice(),
            bits(&case["target"]),
            "Dokan entry case {index}"
        );
        assert_eq!(
            pipe.bind_center().to_array().map(f32::to_bits).as_slice(),
            bits(&case["bind_center"]),
            "Dokan sensor case {index}"
        );
    }
}

#[cfg_attr(test, test)]
fn original_mouth_and_puppet_pose_operations() {
    let areas = DimensionAreas::from_export(&serde_json::json!([{
        "id":"area", "name":"2DMoveArea", "shape":"AreaCubeBase",
        "matrix":[1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.],
        "scale":[1.,1.,1.],"args":{"SurfaceDistance":5.}
    }]));
    let mut source = serde_json::json!({
        "placement":{"Id":"side", "UnitConfigName":"Dokan", "IsConnectToCollision":false,
            "IsNoSnap":false,"IsMoon":false,"IsExitOnly":false,"Scale":{"X":1.,"Y":1.,"Z":1.}},
        "destination_id":"flat",
        "raw_matrix":[0.,1.,0.,10.,1.,0.,0.,20.,0.,0.,-1.,30.],
        "sensors":[{"Name":"Bind","Radius":80.,"Y":120.}]
    });
    let side = Dokan::load(&source, &areas).unwrap();
    assert_eq!(side.direction, Direction::Side);
    assert_eq!(side.bind_center(), Vec3::new(130., 20., 30.));
    assert_eq!(side.bind_radius, 80.);
    let target = side.side_entry_target().unwrap();
    assert_eq!([target[3], target[7], target[11]], [220., -80., 30.]);
    assert_eq!(side.up(), Vec3::X);
    assert_eq!(side.side(), Vec3::Y);
    assert!(side.upright_2d_exit().is_none());
    assert_eq!(side.id, "side");
    assert_eq!(side.destination_id, "flat");

    source["placement"]["Id"] = "flat".into();
    source["placement"]["UnitConfigName"] = "Dokan2D".into();
    source["destination_id"] = "side".into();
    source["raw_matrix"] = serde_json::json!([1., 0., 0., 0., 0., 1., 0., 20., 0., 0., 1., 30.]);
    source["sensors"] = serde_json::json!([{"Name":"Bind","Radius":100.,"Y":125.}]);
    let flat = Dokan::load(&source, &areas).unwrap();
    assert_eq!(flat.direction, Direction::Up);
    assert_eq!(flat.position(), Vec3::new(0., 20., 5.));
    assert_eq!(flat.bind_center(), Vec3::new(0., 145., 5.));
    let (exit, pose) = flat.upright_2d_exit().unwrap();
    assert_eq!(exit, Vec3::new(0., 220., 5.));
    assert_eq!(
        pose.to_array().map(f32::to_bits),
        quat_front_up(Vec3::X, Vec3::Y).to_array().map(f32::to_bits)
    );
    assert!(flat.side_entry_target().is_none());
    source["placement"]["IsNoSnap"] = true.into();
    assert!(Dokan::load(&source, &areas).is_err());
}
