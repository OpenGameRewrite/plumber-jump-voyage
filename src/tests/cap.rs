use super::*;

pub(super) fn run() {
    straight_throw_matches_the_recording();
}

/// The standing throw of the recorded `capthrow` run: the cap appears 150 in
/// front plus its first step, flies 32 a frame, brakes 3.2 a frame, stays
/// for 22 frames and comes back 18, 36, 54, 72, 80 ... faster.
#[cfg_attr(test, test)]
fn straight_throw_matches_the_recording() {
    let mut cap = Cap::start(
        Launch {
            front: Vec3::Z,
            kind: Throw::Forward,
            left: false,
            continuous: false,
        },
        Vec3::ZERO,
    );
    let world = CapWorld {
        player: Vec3::ZERO,
        hold: false,
    };
    let z = |cap: &Cap| cap.pos.z / UNIT;
    assert!((z(&cap) - 182.).abs() < 1e-3 && (cap.pos.y / UNIT - 70.).abs() < 1e-3);
    let mut log = vec![(cap.state_name(), z(&cap))];
    for _ in 0..60 {
        cap.update(&world);
        log.push((cap.state_name(), z(&cap)));
    }
    // Row 24 is the start; rows 25..34 fly, row 35 brakes its first frame.
    assert_eq!(log[10], ("Throw", 502.));
    assert_eq!(log[11].0, "ThrowBrake");
    assert!((log[11].1 - 534.).abs() < 1e-2);
    assert!((log[12].1 - 562.8).abs() < 1e-2);
    assert!((log[20].1 - 678.).abs() < 1e-2);
    assert_eq!(log[21].0, "ThrowStay");
    assert_eq!(log[43].0, "Return");
    assert!((log[44].1 - 660.0).abs() < 0.1);
}
