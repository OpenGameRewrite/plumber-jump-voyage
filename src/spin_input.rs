//! The stick-circle detector (`al::SpinInputAnalyzer`), the one input rule
//! behind the ground spin and the spin jump.
//!
//! The player's input object feeds it the raw left stick once per frame
//! (`PlayerInput::update`, before any state runs). It keeps the last 128
//! sticks and the signed area each frame's swing swept (half the cross
//! product of the stick before and after), then looks back up to 70 frames
//! for a circle: from the 30th frame back on, a spin is found when the area
//! summed so far reaches 5 and the stick back then still points within about
//! 45 degrees of where it points now. A stick back at the centre points
//! nowhere and never agrees, so a half-finished circle that ends in neutral
//! is not a spin.
//!
//! A spin lasts 30 frames: the frame it is found and the 29 after it
//! (`PlayerInput::isSpinInput`). Counter-clockwise in the plane with the stick
//! to the right and up positive is the positive direction; the game's
//! `isSpinClockwise` is a negative one. One turn of radius 1 sweeps pi, so a
//! full-radius circle needs about one and three-fifths turns, and a stick
//! pushed only part of the way round needs more.
use bevy::prelude::*;

/// Frames of stick and swept area kept.
const HISTORY: usize = 128;
/// The detector looks back at most this many frames.
const LOOK_BACK: usize = 70;
/// The test only applies from this many frames back.
const FIRST_TESTED_AGE: usize = 30;
/// Swept area (either direction) that makes a spin.
const SPIN_AREA: f32 = 5.;
/// The stick back then and now must agree at least this much (cosine of the
/// angle between them).
const SPIN_AGREEMENT: f32 = 0.7;
/// Frames the spin is reported for after it is found.
const SPIN_FRAMES: u32 = 30;
/// A stick shorter than this has no direction (`al::isNearZero`).
const NO_DIRECTION: f32 = 0.001;

/// A stick's direction, or nothing when it rests at the centre
/// (`al::tryNormalizeOrZero`).
fn direction_of(stick: Vec2) -> Vec2 {
    if stick.length_squared() < NO_DIRECTION * NO_DIRECTION {
        Vec2::ZERO
    } else {
        stick.normalize()
    }
}

#[derive(Clone, Debug)]
pub struct SpinInput {
    sticks: [Vec2; HISTORY],
    /// The area each frame's swing swept, signed.
    swept: [f32; HISTORY],
    /// Frames of history that count (reset to nothing by a detection, so the
    /// same circle cannot be found twice).
    usable: usize,
    next: usize,
    /// 1 counter-clockwise, -1 clockwise, 0 no spin.
    direction: i32,
    /// Frames the spin has left.
    left: u32,
}
impl Default for SpinInput {
    fn default() -> Self {
        Self {
            sticks: [Vec2::ZERO; HISTORY],
            swept: [0.; HISTORY],
            usable: 0,
            next: 0,
            direction: 0,
            left: 0,
        }
    }
}
impl SpinInput {
    /// One frame: the raw stick, x right and y up, as the controller reports it.
    pub fn update(&mut self, stick: Vec2) {
        let previous = (self.next + HISTORY - 1) % HISTORY;
        self.sticks[self.next] = stick;
        self.swept[self.next] = self.sticks[previous].perp_dot(stick) * 0.5;
        let now = direction_of(stick);
        let mut area: f32 = 0.;
        let mut age = 0;
        while age < LOOK_BACK && age < self.usable {
            let index = (self.next + HISTORY - age) % HISTORY;
            if age >= FIRST_TESTED_AGE
                && area.abs() >= SPIN_AREA
                && direction_of(self.sticks[index]).dot(now) > SPIN_AGREEMENT
            {
                self.usable = 0;
                self.direction = if area > 0. { 1 } else { -1 };
                // The count down below takes the first of these frames.
                self.left = SPIN_FRAMES + 1;
                break;
            }
            area += self.swept[index];
            age += 1;
        }
        if self.left >= 1 {
            self.left -= 1;
            if self.left == 0 {
                self.direction = 0;
            }
        }
        self.next = (self.next + 1) % HISTORY;
        if self.usable < HISTORY {
            self.usable += 1;
        }
    }
    /// `PlayerInput::isSpinInput`.
    pub fn is_spinning(&self) -> bool {
        self.direction != 0
    }
    /// `PlayerInput::isSpinClockwise`.
    pub fn is_clockwise(&self) -> bool {
        self.direction < 0
    }
}

#[path = "tests/spin_input.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
