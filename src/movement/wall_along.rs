//! The stick bent along a wall (`PlayerInput::updateWallAlong` and
//! `PlayerInput::snapWallAlongInput`).
//!
//! After the body has touched a wall's face the move input keeps being bent
//! along that wall: a weight rises while the contact lasts and for five frames
//! after it, then fades away by five percent a frame. The bend is what lets a
//! player who pushes diagonally into a wall run along it, so it only moves the
//! stick when the stick is within about eighty degrees of the wall's direction.

use bevy::math::{Vec2, Vec3};

/// The frames the wall stays remembered after the last contact.
const REMEMBER_FRAMES: i32 = 5;
/// How fast the weight rises (while remembered) and fades (afterwards).
const RISE_RATE: f32 = 0.1;
const FADE_RATE: f32 = 0.05;
/// Under this weight the fade ends.
const FADE_END: f32 = 0.005;
/// The stick is no input under this length.
const STICK_DEADZONE: f32 = 0.1;
/// A new wall replaces the remembered one when its normal is within 85 degrees
/// of it (cos 85), or when the stick has turned by ten degrees (cos 10).
const SAME_WALL_COS: f32 = 0.087_155_74;
const SAME_STICK_COS: f32 = 0.984_807_73;
/// `al::isNearZero` of the move input.
const NEAR_ZERO: f32 = 0.001;
/// Angles of the bend, in radians: inputs within the first one of the wall's
/// direction run along it (25 degrees away from the wall, 50 into it), the
/// bend fades out up to 80 degrees and an input between 80 and 100 degrees
/// stays across it.
const ALONG_AWAY_DEGREES: f32 = 25.;
const ALONG_INTO_DEGREES: f32 = 50.;
const FADE_OUT_END: f32 = 1.396_263_4;
const ACROSS_END: f32 = 1.745_329_3;
const HALF_PI: f32 = 1.570_796_4;
const PI: f32 = std::f32::consts::PI;

/// What the input remembers of the last wall.
#[derive(Clone, Debug, Default)]
pub struct WallAlong {
    /// Frames the wall is still remembered for (`_1c`).
    frames: i32,
    /// How much of the bend is applied (`_20`).
    weight: f32,
    /// The wall's normal (`_24`) and the stick when it was remembered (`_30`).
    wall: Vec3,
    stick: Vec2,
}

impl WallAlong {
    /// `updateWallAlong`, once a frame before the states run: `contact` is the
    /// normal of the wall face the body is pressed against, as the last step's
    /// collisions left it, and `stick` the stick as held.
    pub fn update(&mut self, contact: Option<Vec3>, stick: Vec2) {
        self.frames = (self.frames - 1).max(0);
        let Some(normal) = contact else {
            if self.frames < 1 {
                self.weight += (0. - self.weight) * FADE_RATE;
                if self.weight < FADE_END {
                    self.weight = 0.;
                }
                return;
            }
            self.rise();
            return;
        };
        let stick = if stick.length() < STICK_DEADZONE {
            Vec2::ZERO
        } else {
            stick
        };
        let remembered = self.frames != 0;
        let new_wall = normal.dot(self.wall) > SAME_WALL_COS;
        let turned = stick.dot(self.stick) < SAME_STICK_COS;
        if !remembered || new_wall || turned {
            self.wall = normal;
            self.stick = stick;
        }
        self.frames = REMEMBER_FRAMES;
        self.rise();
    }

    fn rise(&mut self) {
        self.weight += (1. - self.weight) * RISE_RATE;
    }

    /// `snapWallAlongInput`: the move input `input` on the floor with the
    /// normal `up`, bent along the remembered wall.
    pub fn snap(&self, input: Vec3, up: Vec3) -> Vec3 {
        if self.weight <= 0. || input.length_squared() < NEAR_ZERO * NEAR_ZERO {
            return input;
        }
        // The wall's direction on the floor, and across it.
        let across = (self.wall - up * self.wall.dot(up)).normalize_or_zero();
        if across == Vec3::ZERO {
            return input;
        }
        let along = across.cross(up).normalize();
        let length = input.length();
        let angle = input.dot(along).clamp(-1., 1.).acos();
        let toward_wall = input.dot(across);
        let limit = if toward_wall > 0. {
            ALONG_AWAY_DEGREES
        } else {
            ALONG_INTO_DEGREES
        };
        let limit_rad = limit * PI / 180.;
        // The angle the input is bent to, measured from the wall's direction.
        let bent = if angle <= limit_rad {
            0.
        } else if angle < FADE_OUT_END {
            let share = (angle - limit_rad) * PI * 0.5 / (FADE_OUT_END - limit_rad);
            share.clamp(0., HALF_PI)
        } else if angle <= ACROSS_END {
            HALF_PI
        } else {
            let far_end = (180. - limit) * PI / 180.;
            if angle >= far_end {
                PI
            } else {
                let share = (angle - ACROSS_END) * PI * 0.5 / (far_end - ACROSS_END);
                share.clamp(0., HALF_PI) + HALF_PI
            }
        };
        let (sin, cos) = bent.sin_cos();
        let kept = along * (cos * length);
        let turned = across * (sin * length);
        let snapped = if toward_wall < 0. {
            kept - turned
        } else {
            kept + turned
        };
        input * (1. - self.weight) + snapped * self.weight
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remembering(wall: Vec3) -> WallAlong {
        let mut along = WallAlong::default();
        for _ in 0..40 {
            along.update(Some(wall), Vec2::Y);
        }
        along
    }

    #[test]
    fn a_wall_is_remembered_for_five_frames_and_then_fades() {
        let mut along = remembering(Vec3::Z);
        let full = along.weight;
        assert!(full > 0.98);
        for _ in 0..4 {
            along.update(None, Vec2::Y);
        }
        assert!(along.weight >= full, "still rising while remembered");
        along.update(None, Vec2::Y);
        assert!(along.weight < full, "fading");
        for _ in 0..200 {
            along.update(None, Vec2::Y);
        }
        assert_eq!(along.weight, 0.);
    }

    #[test]
    fn an_input_near_the_wall_runs_along_it() {
        // A wall facing -z, an input 30 degrees off the wall's direction (x):
        // within the 50 degrees allowed into the wall, so it runs along it.
        let along = remembering(Vec3::NEG_Z);
        let input = Vec3::new(30f32.to_radians().cos(), 0., 30f32.to_radians().sin());
        let out = along.snap(input, Vec3::Y);
        assert!(out.z.abs() < 0.03, "{out}");
        assert!((out.length() - 1.).abs() < 0.03, "{out}");
    }

    #[test]
    fn an_input_across_the_wall_is_left_alone() {
        let along = remembering(Vec3::NEG_Z);
        let out = along.snap(Vec3::Z, Vec3::Y);
        assert!((out - Vec3::Z).length() < 1.0e-5, "{out}");
    }

    #[test]
    fn nothing_remembered_changes_nothing() {
        let along = WallAlong::default();
        let input = Vec3::new(0.3, 0., 0.9);
        assert_eq!(along.snap(input, Vec3::Y), input);
    }
}
