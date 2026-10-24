use super::*;

pub(super) fn run() {
    the_kinds_follow_the_throws_count();
    the_swing_rises_holds_and_falls_by_its_kind();
    a_swing_of_no_angle_turns_nothing();
}

#[cfg_attr(test, test)]
fn the_kinds_follow_the_throws_count() {
    assert_eq!(Swing::on_ground(0), Some(Swing::Long));
    assert_eq!(Swing::on_ground(1), Some(Swing::Held));
    assert_eq!(Swing::on_ground(2), Some(Swing::Quick));
    assert_eq!(Swing::on_ground(3), None);
    assert_eq!(Swing::in_air(1), Some(Swing::Wide));
}

fn shares(swing: Swing) -> Vec<f32> {
    let mut cap = CapThrow::default();
    cap.ask(swing, 20.);
    let mut out = Vec::new();
    loop {
        cap.update();
        if cap.count == 0 {
            break;
        }
        out.push(cap.share);
    }
    out
}

#[cfg_attr(test, test)]
fn the_swing_rises_holds_and_falls_by_its_kind() {
    // A held swing is whole at once and falls over its last twenty steps of thirty.
    let held = shares(Swing::Held);
    assert_eq!(held.len(), 29);
    assert!(held[..10].iter().all(|share| *share == 1.), "{held:?}");
    assert!(held.last().unwrap().abs() < 0.1);
    // The long one rises over its first 25 steps, holds, and falls over the last 10.
    let long = shares(Swing::Long);
    assert_eq!(long.len(), 44);
    assert!(long[0] < long[10] && long[10] < long[24]);
    assert!(long[25..34].iter().all(|share| *share == 1.), "{long:?}");
    assert!(long[34] > long[43]);
    // The angle is reversed and limited: the wide swing turns by at most 20 degrees.
    let mut cap = CapThrow::default();
    cap.ask(Swing::Wide, 55.);
    cap.update();
    assert_eq!(cap.yaw, -20.);
    let upright: Mat34 = [[1., 0., 0., 5.], [0., 1., 0., 6.], [0., 0., 1., 7.]];
    let mut matrix = upright;
    cap.calc(Joint::Hip, &mut matrix);
    assert_eq!(matrix, upright);
    cap.calc(Joint::Spine1, &mut matrix);
    assert_ne!(matrix, upright);
    assert_eq!([matrix[0][3], matrix[1][3], matrix[2][3]], [5., 6., 7.]);
}

#[cfg_attr(test, test)]
fn a_swing_of_no_angle_turns_nothing() {
    let mut cap = CapThrow::default();
    cap.ask(Swing::Long, 0.);
    let upright: Mat34 = [[1., 0., 0., 5.], [0., 1., 0., 6.], [0., 0., 1., 7.]];
    for _ in 0..50 {
        cap.update();
        let mut matrix = upright;
        cap.calc(Joint::Spine1, &mut matrix);
        for (a, b) in matrix.iter().flatten().zip(upright.iter().flatten()) {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
