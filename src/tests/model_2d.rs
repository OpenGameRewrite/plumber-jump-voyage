use super::*;
use crate::clip_curves::checks::file_with_bind_pose;

pub(super) fn run() {
    a_joint_follows_its_parent_and_the_actor();
    an_action_is_one_frame_shorter_than_its_samples();
    a_part_shows_only_with_the_figure_joint();
    a_held_action_keeps_its_last_sample_and_a_loop_wraps();
}

fn action(looping: bool, figure: &[bool], part: &[bool]) -> Action {
    Action {
        looping,
        tracks: BTreeMap::from([
            (FIGURE_JOINT.to_string(), figure.to_vec()),
            ("MarioDot03".to_string(), part.to_vec()),
        ]),
    }
}

#[cfg_attr(test, test)]
fn a_joint_follows_its_parent_and_the_actor() {
    let curves = Curves::parse(&file_with_bind_pose()).expect("a well formed file");
    // The actor turned a quarter about y (front z -> x) and standing at (10, 0, 0).
    let actor: Mat34 = [[0., 0., 1., 10.], [0., 1., 0., 0.], [-1., 0., 0., 0.]];
    let joints = compose(&curves, &actor, "Hold", 0.).expect("the clip is in the file");
    assert_eq!(
        joints.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        ["Root", "Part"]
    );
    // Root is at its bind translation (1, 2, 3) in the actor's frame: (10 + 3, 2, -1).
    let root = joints[0].1;
    assert_eq!([root[0][3], root[1][3], root[2][3]], [13., 2., -1.]);
    // Part is the clip's (0, 0, 7) from there: z is the actor's x, so +7 in x.
    let part = joints[1].1;
    assert_eq!([part[0][3], part[1][3], part[2][3]], [20., 2., -1.]);
    assert!(compose(&curves, &actor, "Missing", 0.).is_none());
}

#[cfg_attr(test, test)]
fn an_action_is_one_frame_shorter_than_its_samples() {
    let held = action(false, &[true; 6], &[true; 6]);
    assert_eq!(held.length(), Some((5, false)));
    let looping = action(true, &[true; 61], &[false; 61]);
    assert_eq!(looping.length(), Some((60, true)));
    assert_eq!(action(false, &[], &[]).length(), None);
}

#[cfg_attr(test, test)]
fn a_part_shows_only_with_the_figure_joint() {
    let hidden_after_two = action(false, &[true, true, false, false], &[true; 4]);
    assert_eq!(hidden_after_two.shows(1, "MarioDot03"), Some(true));
    assert_eq!(hidden_after_two.shows(2, "MarioDot03"), Some(false));
    // A part the action has no track for is unknown, not hidden.
    assert_eq!(hidden_after_two.shows(0, "MarioDot07"), None);
}

#[cfg_attr(test, test)]
fn a_held_action_keeps_its_last_sample_and_a_loop_wraps() {
    // Samples 0..=3; the loop's last sample is its first one again.
    let part = [false, true, false, false];
    let held = action(false, &[true; 4], &part);
    assert_eq!(held.shows(3, "MarioDot03"), Some(false));
    assert_eq!(held.shows(40, "MarioDot03"), Some(false));
    let looping = action(true, &[true; 4], &part);
    // The loop is three frames long: frame 3 is its first frame again.
    assert_eq!(looping.shows(3, "MarioDot03"), Some(false));
    assert_eq!(looping.shows(4, "MarioDot03"), Some(true));
}
