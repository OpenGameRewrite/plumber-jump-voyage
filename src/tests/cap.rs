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
            air: false,
            continuous: false,
        },
        Vec3::ZERO,
        1,
    );
    let world = CapWorld {
        player: Vec3::ZERO,
        hold: false,
    };
    let z = |cap: &Cap| cap.pos.z / UNIT;
    assert!((z(&cap) - 182.).abs() < 1e-3 && (cap.pos.y / UNIT - 70.).abs() < 1e-3);
    let mut log = vec![(cap.state_name(), z(&cap))];
    let mut moments = vec![];
    for row in 1..=60 {
        for moment in cap.update(&world) {
            moments.push((row + 24, moment));
        }
        log.push((cap.state_name(), z(&cap)));
    }
    // Row 25 is the throw nerve's first step; the stay's 22 steps are rows
    // 46 to 67, the last one choosing the return, and the cap arrives in row 76.
    assert_eq!(moments[0], (25, CapEvent::Thrown));
    let stays: Vec<_> = moments
        .iter()
        .filter(|m| m.1 == CapEvent::Staying)
        .map(|m| m.0)
        .collect();
    assert_eq!(stays, (46..=67).collect::<Vec<_>>());
    assert!(moments.contains(&(67, CapEvent::LeftStay)));
    let caught = moments.iter().find(|m| m.1 == CapEvent::Caught);
    assert_eq!(caught, Some(&(76, CapEvent::Caught)));
    // The clips restart when the nerves start theirs: flying, staying, returning.
    assert_eq!(cap.clip(), "FlyingWaitR");
    assert_eq!(cap.clip_serial(), 3);
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
