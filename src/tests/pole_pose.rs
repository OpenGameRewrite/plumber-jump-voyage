use super::*;

pub(super) fn run() {
    the_angle_follows_the_width_of_the_pole();
    the_limbs_follow_the_block_by_its_rate_and_turn_mirrored();
}

#[cfg_attr(test, test)]
fn the_angle_follows_the_width_of_the_pole() {
    // The body's own width asks for no turn, wide poles for up to the maximum, narrow ones for
    // the minimum; the slant of the pole code is added to it.
    assert_eq!(joint_angle(50., 0.), 0.);
    assert_eq!(joint_angle(80., 0.), 25.);
    assert_eq!(joint_angle(100., 0.), 25.);
    assert_eq!(joint_angle(15., 0.), -25.);
    assert_eq!(joint_angle(10., 0.), -25.);
    assert!((joint_angle(32.5, 0.) + 12.5).abs() < 1e-5);
    assert!((joint_angle(65., -10.) - 2.5).abs() < 1e-5);
}

#[cfg_attr(test, test)]
fn the_limbs_follow_the_block_by_its_rate_and_turn_mirrored() {
    let upright: Mat34 = [[1., 0., 0., 5.], [0., 1., 0., 6.], [0., 0., 1., 7.]];
    let mut pose = PolePose::default();
    // Nothing asked: no joint changes.
    let mut matrix = upright;
    pose.calc(Joint::ArmL1, &mut matrix);
    assert_eq!(matrix, upright);
    // A tenth of the way each step.
    pose.ask_angle(20.);
    pose.ask_rate(0.1);
    pose.update();
    assert!((pose.arms.y - 2.).abs() < 1e-5 && (pose.legs.y - 2.).abs() < 1e-5);
    // The left limbs turn by the angle about their own y axis, the right ones the other way,
    // other joints not at all, and the position stays.
    let (mut left, mut right, mut other) = (upright, upright, upright);
    pose.calc(Joint::LegL1, &mut left);
    pose.calc(Joint::LegR1, &mut right);
    pose.calc(Joint::Hip, &mut other);
    assert_eq!(other, upright);
    let radians = 2f32.to_radians();
    assert!((left[0][2] - radians.sin()).abs() < 1e-5, "{left:?}");
    assert!((right[0][2] + radians.sin()).abs() < 1e-5, "{right:?}");
    assert_eq!([left[0][3], left[1][3], left[2][3]], [5., 6., 7.]);
    // A stretched joint is turned without shearing: its scale stays on its columns.
    let stretched: Mat34 = [[1.5, 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]];
    let mut turned = stretched;
    pose.calc(Joint::ArmL1, &mut turned);
    let length =
        |m: &Mat34, c: usize| (m[0][c] * m[0][c] + m[1][c] * m[1][c] + m[2][c] * m[2][c]).sqrt();
    assert!((length(&turned, 0) - 1.5).abs() < 1e-5 && (length(&turned, 2) - 1.).abs() < 1e-5);
    // The release clears the block and the controller follows it at the whole rate.
    pose.release();
    pose.update();
    assert!(pose.arms.length() < 2. && pose.arms.length() >= 0.);
    pose.update();
    assert_eq!(pose.arms, Vec3::ZERO);
}
