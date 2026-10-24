use super::*;

pub(super) fn run() {
    the_control_rate_follows_the_walls_target();
    the_spring_lags_a_moving_hip_and_turns_it_about_itself();
}

#[cfg_attr(test, test)]
fn the_control_rate_follows_the_walls_target() {
    let mut grab = GrabDynamics::default();
    assert_eq!(grab.update(), 0.);
    grab.appear();
    assert_eq!(grab.update(), 1.);
    // A move along the wall brings the target down by a fifth a step to a fifth.
    for expected in [0.8, 0.6, 0.4, 0.2, 0.2] {
        grab.ease_down();
        assert!((grab.update() - expected).abs() < 1e-6, "{expected}");
    }
    grab.restore();
    assert_eq!(grab.update(), 1.);
    grab.release();
    assert_eq!(grab.update(), 0.);
}

#[cfg_attr(test, test)]
fn the_spring_lags_a_moving_hip_and_turns_it_about_itself() {
    let hip_at = |y: f32| -> Mat34 { [[1., 0., 0., 10.], [0., 1., 0., y], [0., 0., 1., 20.]] };
    let mut spring = HipSpring::default();
    spring.set_control_rate(1.);
    // The first calculation only notes where the point is; the hip is left alone.
    let mut hip = hip_at(100.);
    spring.calc(Joint::Hip, &mut hip);
    assert_eq!(hip, hip_at(100.));
    // Other joints are left alone as well.
    let mut other = hip_at(150.);
    spring.calc(Joint::Spine1, &mut other);
    assert_eq!(other, hip_at(150.));
    // The hip moves up at once: the spring's point lags and the hip turns about its own position,
    // toward the point that stayed behind (its x axis tilts down), keeping its length.
    let mut moved = hip_at(130.);
    spring.calc(Joint::Hip, &mut moved);
    assert_eq!([moved[0][3], moved[1][3], moved[2][3]], [10., 130., 20.]);
    assert!(moved[1][0] < -1e-4, "{moved:?}");
    let length = |c: usize| {
        (moved[0][c] * moved[0][c] + moved[1][c] * moved[1][c] + moved[2][c] * moved[2][c]).sqrt()
    };
    assert!((length(0) - 1.).abs() < 1e-5 && (length(1) - 1.).abs() < 1e-5);
    // The turn never passes the limit times the control rate.
    let mut wild = HipSpring::default();
    wild.set_control_rate(0.2);
    let mut start = hip_at(0.);
    wild.calc(Joint::Hip, &mut start);
    let mut thrown = hip_at(5000.);
    wild.calc(Joint::Hip, &mut thrown);
    let angle = thrown[0][0].clamp(-1., 1.).acos().to_degrees();
    assert!(angle <= 75. * 0.2 + 1e-3, "{angle}");
    // With the control rate at nothing the spring keeps its point at the hip and does not turn it.
    let mut off = HipSpring::default();
    off.set_control_rate(1.);
    let mut first = hip_at(0.);
    off.calc(Joint::Hip, &mut first);
    off.set_control_rate(0.);
    let mut idle = hip_at(300.);
    off.calc(Joint::Hip, &mut idle);
    assert_eq!(idle, hip_at(300.));
}
