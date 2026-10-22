use super::*;

pub(super) fn run() {
    the_standing_head_is_over_the_shoulders_in_the_installed_model();
    the_blend_ignores_idle_layers_and_the_scale_of_the_weights();
}

/// The model is the player's own installation; without it there is nothing to check.
fn model() -> Option<&'static HeadPose> {
    HeadPose::installed()
}

fn layer(clip: &str, frame: f32, weight: f32) -> PoseLayer<'_> {
    PoseLayer {
        clip,
        frame,
        weight,
    }
}

#[cfg_attr(test, test)]
fn the_standing_head_is_over_the_shoulders_in_the_installed_model() {
    let Some(model) = model() else {
        return;
    };
    let head = model
        .head_in_actor_frame(&[layer("Wait", 10., 1.)])
        .expect("the Wait clip is in the model");
    // About 92 cm over the feet and level.
    assert!((85. ..100.).contains(&head[1][3]), "{head:?}");
    assert!(head[1][1].abs() > 0.9 || head[1][0].abs() > 0.9, "{head:?}");
    // A clip the model does not have gives no pose.
    assert!(
        model
            .head_in_actor_frame(&[layer("NoSuchClip", 0., 1.)])
            .is_none()
    );
}

#[cfg_attr(test, test)]
fn the_blend_ignores_idle_layers_and_the_scale_of_the_weights() {
    let Some(model) = model() else {
        return;
    };
    let alone = model
        .head_in_actor_frame(&[layer("Wait", 10., 1.)])
        .unwrap();
    let with_idle = model
        .head_in_actor_frame(&[layer("Wait", 10., 2.), layer("Dash", 3., 0.)])
        .unwrap();
    assert_eq!(alone, with_idle);
    // The share of each layer is its weight over the total.
    let halves = model
        .head_in_actor_frame(&[layer("Wait", 10., 0.25), layer("Dash", 3., 0.25)])
        .unwrap();
    let whole = model
        .head_in_actor_frame(&[layer("Wait", 10., 1.), layer("Dash", 3., 1.)])
        .unwrap();
    for (a, b) in halves.iter().flatten().zip(whole.iter().flatten()) {
        assert!((a - b).abs() < 1e-4);
    }
}
