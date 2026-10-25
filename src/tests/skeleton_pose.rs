use super::*;

pub(super) fn run() {
    the_standing_head_is_over_the_shoulders_in_the_installed_model();
    the_blend_ignores_idle_layers_and_the_scale_of_the_weights();
    the_joint_order_puts_every_parent_first();
    a_stretched_upper_arm_moves_the_elbow_but_does_not_shear_the_forearm();
    a_parents_scale_moves_the_offset_but_not_the_frame_and_roots_inherit();
}

/// The model is the player's own installation; without it there is nothing to check.
fn model() -> Option<&'static SkeletonPose> {
    SkeletonPose::installed()
}

fn layer(clip: &str, frame: f32, weight: f32) -> PoseLayer<'_> {
    PoseLayer {
        clip,
        frame,
        weight,
        region: Region::Whole,
    }
}

fn head_of(model: &SkeletonPose, layers: &[PoseLayer]) -> Option<Mat34> {
    let locals = model.locals(layers)?;
    Some(world_pose(&IDENTITY, &locals, &mut Unbent)[Joint::Head as usize])
}

#[cfg_attr(test, test)]
fn the_joint_order_puts_every_parent_first() {
    for joint in Joint::ALL {
        if let Some(parent) = joint.parent() {
            assert!((parent as usize) < (joint as usize), "{joint:?}");
        }
        assert_eq!(Joint::ALL[joint as usize], joint);
    }
}

#[cfg_attr(test, test)]
fn the_standing_head_is_over_the_shoulders_in_the_installed_model() {
    let Some(model) = model() else {
        return;
    };
    let head = head_of(model, &[layer("Wait", 10., 1.)]).expect("the Wait clip is in the model");
    // About 92 cm over the feet and level.
    assert!((85. ..100.).contains(&head[1][3]), "{head:?}");
    assert!(head[1][1].abs() > 0.9 || head[1][0].abs() > 0.9, "{head:?}");
    // A clip the model does not have gives no pose.
    assert!(head_of(model, &[layer("NoSuchClip", 0., 1.)]).is_none());
}

#[cfg_attr(test, test)]
fn the_blend_ignores_idle_layers_and_the_scale_of_the_weights() {
    let Some(model) = model() else {
        return;
    };
    let alone = head_of(model, &[layer("Wait", 10., 1.)]).unwrap();
    let with_idle = head_of(model, &[layer("Wait", 10., 2.), layer("Dash", 3., 0.)]).unwrap();
    assert_eq!(alone, with_idle);
    // The share of each layer is its weight over the total.
    let halves = head_of(model, &[layer("Wait", 10., 0.25), layer("Dash", 3., 0.25)]).unwrap();
    let whole = head_of(model, &[layer("Wait", 10., 1.), layer("Dash", 3., 1.)]).unwrap();
    for (a, b) in halves.iter().flatten().zip(whole.iter().flatten()) {
        assert!((a - b).abs() < 1e-4);
    }
}

#[cfg_attr(test, test)]
fn a_stretched_upper_arm_moves_the_elbow_but_does_not_shear_the_forearm() {
    // The upper arm is stretched along its x axis (the bone's); the forearm's own rotation is a
    // quarter turn about y. The elbow lies farther out by the stretch; the hand, along the
    // forearm's x, is carried by the unstretched frame: still as far as the hand's offset times
    // the stretch (the forearm's scale carries the upper arm's), but not along z.
    let mut locals = [IDENTITY; JOINT_COUNT];
    locals[Joint::ArmL1 as usize] = [[2., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]];
    locals[Joint::ArmL2 as usize] = [[0., 0., 0.5, 10.], [0., 0.5, 0., 0.], [-0.5, 0., 0., 0.]];
    locals[Joint::HandL as usize] = [[1., 0., 0., 4.], [0., 1., 0., 0.], [0., 0., 1., 0.]];
    let worlds = world_pose(&IDENTITY, &locals, &mut Unbent);
    let elbow = worlds[Joint::ArmL2 as usize];
    assert!((elbow[0][3] - 20.).abs() < 1e-4, "{elbow:?}");
    let hand = worlds[Joint::HandL as usize];
    // The forearm's x column (before its scale) points along -z; its scale is 0.5 * 2 = 1
    // along that column, so the hand is 4 away along it and stays level.
    assert!(
        (hand[0][3] - 20.).abs() < 1e-4 && hand[1][3].abs() < 1e-4,
        "{hand:?}"
    );
    assert!((hand[2][3] + 4.).abs() < 1e-4, "{hand:?}");
}

#[cfg_attr(test, test)]
fn a_parents_scale_moves_the_offset_but_not_the_frame_and_roots_inherit() {
    // The hip is squashed to twice its size; the file stores the thigh's scale divided by it
    // (0.5), so the thigh truly has scale 1 (the rig compensates the hip's segment scale): its
    // offset is carried out to 20 by the hip's scale, its own frame stays unit.
    let mut locals = [IDENTITY; JOINT_COUNT];
    locals[Joint::Hip as usize] = [[2., 0., 0., 0.], [0., 2., 0., 0.], [0., 0., 2., 0.]];
    locals[Joint::LegL1 as usize] = [[0.5, 0., 0., 10.], [0., 0.5, 0., 0.], [0., 0., 0.5, 0.]];
    let worlds = world_pose(&IDENTITY, &locals, &mut Unbent);
    let thigh = worlds[Joint::LegL1 as usize];
    assert!((thigh[0][3] - 20.).abs() < 1e-4, "{thigh:?}");
    for c in 0..3 {
        let length = (thigh[0][c].powi(2) + thigh[1][c].powi(2) + thigh[2][c].powi(2)).sqrt();
        assert!((length - 1.).abs() < 1e-4, "column {c}: {thigh:?}");
    }
    // The roots have no compensation: the scale of the actor's root reaches every joint, the
    // joint root's own is not divided out of its file scale either.
    let mut locals = [IDENTITY; JOINT_COUNT];
    locals[Joint::AllRoot as usize] = [[3., 0., 0., 0.], [0., 3., 0., 0.], [0., 0., 3., 0.]];
    locals[Joint::JointRoot as usize] = [[1., 0., 0., 0.], [0., 1., 0., 5.], [0., 0., 1., 0.]];
    let worlds = world_pose(&IDENTITY, &locals, &mut Unbent);
    assert!((worlds[Joint::JointRoot as usize][1][3] - 15.).abs() < 1e-4);
    assert!((worlds[Joint::Hip as usize][0][0] - 3.).abs() < 1e-4);
    assert!((worlds[Joint::Head as usize][0][0] - 3.).abs() < 1e-4);
}
