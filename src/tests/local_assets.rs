use super::*;

pub(super) fn run() {
    outfit_validation_and_expressions();
    expression_sampling();
}

#[cfg_attr(test, test)]
fn outfit_validation_and_expressions() {
    #[cfg(feature = "local-assets")]
    imported::self_check();
    let mut o = Outfit {
        name: "Local outfit".into(),
        file: "local/outfit.glb".into(),
        scale: 1.,
        yaw: 0.,
        animations: default(),
        cap_file: None,
        animation_file: None,
        expressions: default(),
    };
    assert!(validate(&o));
    for path in [
        "../file.glb",
        "local/../../x.glb",
        "C:/file.glb",
        "local/x.nsp",
    ] {
        o.file = path.into();
        assert!(!validate(&o));
    }
    o.file = "local/outfit.glb".into();
    let w = Wardrobe {
        outfits: vec![o.clone(), o],
        status: String::new(),
    };
    assert_eq!(w.slot(0), 3, "original outfits are listed first");
    assert_eq!(w.slot(2), 0);
    assert_eq!(w.row(0), 2);
    let empty = Wardrobe {
        outfits: vec![],
        status: String::new(),
    };
    assert_eq!(empty.slot(1), 1);
    // Missing clips fall back along a chain that ends in the idle loop.
    for name in [
        "DashFast",
        "SpinCapStart3Left",
        "JumpBroad3",
        "LandJump3",
        "Anything",
    ] {
        let mut n = name;
        for _ in 0..4 {
            n = fallback(n);
        }
        assert_eq!(fallback(n), "Wait", "{name}");
    }
    assert_eq!(interp_frames("Jump", "Move"), 3.);
    assert_eq!(interp_frames("Jump", "Wait"), 0.);
    assert!(looping("Wait") && !looping("Jump"));
    assert!(
        looping("SpinGroundL") && looping("SpinGroundR"),
        "the stick spin cycles"
    );
}

#[cfg_attr(test, test)]
fn expression_sampling() {
    let mut o = outfit();
    std::sync::Arc::make_mut(&mut o.expressions).insert(
        "Jump".into(),
        Expression {
            looping: false,
            tracks: [("EyelidLeft".into(), vec![true, false, true])].into(),
            uv: [("EyePupilLeftMT".into(), vec![[0., 0., 0.], [0., 0.2, 0.4]])].into(),
        },
    );
    std::sync::Arc::make_mut(&mut o.expressions).insert(
        "Wait".into(),
        Expression {
            looping: true,
            tracks: [("EyelidLeft".into(), vec![false, true, false])].into(),
            uv: default(),
        },
    );
    assert_eq!(
        o.part_visibility("Jump", 1. / 60., "EyelidLeft"),
        Some(false)
    );
    assert_eq!(o.part_visibility("Jump", 20., "EyelidLeft"), Some(true));
    assert_eq!(
        o.part_visibility("Missing", 3. / 60., "EyelidLeft"),
        Some(true)
    );
    assert_eq!(
        o.part_visibility("Wait", 2. / 60., "EyelidLeft"),
        Some(false)
    );
    assert_eq!(o.part_visibility("Jump", 0., "Unknown"), None);
    let uv = o.pupil_uv("Jump", 0.5 / 60., "EyePupilLeftMT").unwrap();
    assert!(
        uv.transform_point2(Vec2::splat(0.5))
            .distance(Vec2::new(0.4, 0.7))
            < 1e-5
    );
}

fn outfit() -> Outfit {
    Outfit {
        name: "Local outfit".into(),
        file: "local/outfit.glb".into(),
        scale: 1.,
        yaw: 0.,
        animations: default(),
        cap_file: None,
        animation_file: None,
        expressions: default(),
    }
}
