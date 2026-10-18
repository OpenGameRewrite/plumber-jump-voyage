use super::*;

pub(super) fn run() {
    a_circle_is_found_on_the_thirty_first_frame();
    the_direction_follows_the_turn();
    a_spin_lasts_thirty_frames();
    short_or_unfinished_turns_are_not_spins();
}

/// The stick of frame `n` of a full-radius circle that takes `per_turn`
/// frames a turn, starting upward, counter-clockwise when `turn` is positive.
fn circle(n: usize, per_turn: f32, turn: f32) -> Vec2 {
    let angle = std::f32::consts::FRAC_PI_2 + turn * std::f32::consts::TAU * n as f32 / per_turn;
    Vec2::new(angle.cos(), angle.sin())
}

/// A detector that has seen the stick rest for ten frames, as in the
/// recordings (the player updates it from the first frame of the scene).
fn resting() -> SpinInput {
    let mut input = SpinInput::default();
    for _ in 0..10 {
        input.update(Vec2::ZERO);
    }
    input
}

#[cfg_attr(test, test)]
fn a_circle_is_found_on_the_thirty_first_frame() {
    // The recorded stick circle: 15 frames a turn.
    let mut input = resting();
    for n in 0..30 {
        input.update(circle(n, 15., 1.));
        assert!(!input.is_spinning(), "frame {n}");
    }
    input.update(circle(30, 15., 1.));
    assert!(input.is_spinning(), "the 31st frame of the circle");
}

#[cfg_attr(test, test)]
fn the_direction_follows_the_turn() {
    let mut counter = resting();
    let mut clockwise = resting();
    for n in 0..31 {
        counter.update(circle(n, 15., 1.));
        clockwise.update(circle(n, 15., -1.));
    }
    assert!(counter.is_spinning() && !counter.is_clockwise());
    assert!(clockwise.is_spinning() && clockwise.is_clockwise());
}

#[cfg_attr(test, test)]
fn a_spin_lasts_thirty_frames() {
    let mut input = resting();
    for n in 0..31 {
        input.update(circle(n, 15., 1.));
    }
    // Found on this frame; the stick then rests and the spin runs out.
    for frame in 1..=29 {
        input.update(Vec2::ZERO);
        assert!(input.is_spinning(), "{frame} frames after");
    }
    input.update(Vec2::ZERO);
    assert!(!input.is_spinning(), "30 frames after");
}

#[cfg_attr(test, test)]
fn short_or_unfinished_turns_are_not_spins() {
    // One turn in 16 frames (an octagon is even shorter: area 2.83 a turn).
    let mut input = resting();
    for n in 0..16 {
        input.update(circle(n, 16., 1.));
    }
    for _ in 0..120 {
        input.update(Vec2::ZERO);
        assert!(!input.is_spinning());
    }
    // Two turns that end with the stick at the centre never agree with the
    // direction they started in.
    let mut input = resting();
    for n in 0..30 {
        input.update(circle(n, 15., 1.));
    }
    for _ in 0..10 {
        input.update(Vec2::ZERO);
        assert!(!input.is_spinning());
    }
}
