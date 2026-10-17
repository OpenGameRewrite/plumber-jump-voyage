use super::*;

pub(super) fn run() {
    walking_footsteps();
    sub_animation_and_standing_still();
    fast_fall_and_roll();
}

fn walking(phase: f32) -> SeFrame {
    SeFrame {
        stepping: true,
        rate: 1.,
        phase,
        speed: 5.,
        ..SeFrame::default()
    }
}

/// The recorded slow walk: a foot lifts on the first frame, the left foot
/// falls on the next, then a lift 21 frames after each footfall and a footfall
/// every 30 frames, alternating feet.
#[cfg_attr(test, test)]
fn walking_footsteps() {
    let mut ctrl = SeCtrl::default();
    let mut log = Vec::new();
    for row in 0..125 {
        for event in ctrl.update(&walking((row % 60) as f32)) {
            log.push((row, event));
        }
    }
    let at = |row, event| assert!(log.contains(&(row, event)), "{row} {event:?} in {log:?}");
    at(0, SeEvent::FootLift { left: false });
    at(1, SeEvent::Footfall { left: true });
    at(22, SeEvent::FootLift { left: true });
    at(31, SeEvent::Footfall { left: false });
    at(52, SeEvent::FootLift { left: false });
    at(61, SeEvent::Footfall { left: true });
    // A fast pace shortens the gap and skips the beat correction.
    let mut ctrl = SeCtrl::default();
    let fast = SeFrame {
        rate: 3.5,
        speed: 14.,
        ..walking(0.)
    };
    let steps = (0..60)
        .flat_map(|_| ctrl.update(&fast))
        .filter(|e| matches!(e, SeEvent::Footfall { .. }))
        .count();
    // 22 animation frames at rate 3.5 is 6.3 game frames: a step every 7th frame, the first on the second.
    assert_eq!(steps, 9);
}

#[cfg_attr(test, test)]
fn sub_animation_and_standing_still() {
    let mut ctrl = SeCtrl::default();
    // While another clip covers the legs the count still runs, so the lift and
    // the footfall come at once when the cycle shows again.
    let covered = SeFrame {
        stepping: false,
        ..walking(0.)
    };
    for _ in 0..9 {
        assert!(ctrl.update(&covered).is_empty());
    }
    assert_eq!(
        ctrl.update(&walking(0.)),
        [SeEvent::FootLift { left: false }]
    );
    // Standing still counts nothing.
    let mut ctrl = SeCtrl {
        moved: 0,
        ..SeCtrl::default()
    };
    let still = SeFrame {
        speed: 0.,
        ..walking(0.)
    };
    for _ in 0..100 {
        assert!(ctrl.update(&still).is_empty());
    }
}

#[cfg_attr(test, test)]
fn fast_fall_and_roll() {
    let mut ctrl = SeCtrl::default();
    let falling = |speed| SeFrame {
        vertical_speed: speed,
        ..SeFrame::default()
    };
    // The first loud frame of a new player only starts the count.
    assert!(ctrl.update(&falling(-31.)).is_empty());
    assert!(ctrl.update(&falling(-10.)).is_empty());
    assert_eq!(
        ctrl.update(&falling(-30.)),
        [SeEvent::FallWhoosh { frames: 1 }]
    );
    assert_eq!(
        ctrl.update(&falling(-45.)),
        [SeEvent::FallWhoosh { frames: 2 }]
    );
    assert!(ctrl.update(&falling(-29.)).is_empty());
    assert_eq!(
        ctrl.update(&falling(-30.)),
        [SeEvent::FallWhoosh { frames: 1 }]
    );
    let rolling = |frame| SeFrame {
        rolling: Some(frame),
        ..SeFrame::default()
    };
    let mut ctrl = SeCtrl::default();
    assert!(ctrl.update(&rolling(5.4)).is_empty());
    assert_eq!(ctrl.update(&rolling(6.4)), [SeEvent::Roll]);
    assert!(ctrl.update(&rolling(8.)).is_empty());
    // The clip loops: the next lap sounds again.
    assert!(ctrl.update(&rolling(1.)).is_empty());
    assert_eq!(ctrl.update(&rolling(7.)), [SeEvent::Roll]);
}
