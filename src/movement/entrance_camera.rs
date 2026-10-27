//! The scene's entrance camera (`al::CameraPoserEntrance`) as the player's look-at sees it.
//!
//! A scene that has just been built starts its entrance camera, and
//! `PlayerActorHakoniwa::movement` keeps the head's look-at off while
//! `al::isPlayingEntranceCamera` holds (the current camera has priority 2 or 7). The poser
//! ends itself from its `Wait` nerve, which each camera update runs after the actors moved:
//! the target (the player) is remembered on the nerve's first step and the camera ends once the
//! target is more than 10 cm from there (the poser's keep-while-moving flag is not set by the
//! scenes recorded here), or when the camera is reset by the player. The end is a request: the
//! pose updater switches away from the poser in the next update, so the player's `movement`
//! sees the ordinary camera two frames after the one that moved the target out of range.
//!
//! The poser's other ways of ending (a time limit, the right stick's power above 0.3 and the
//! keep-in-air variant) come from camera parameters of the stage or from the camera stick. The
//! recorded scenes have no time limit; the windowed game's camera stick is a pointer delta, not
//! the stick power, so it is not mapped.
use super::*;

/// How far the target may wander from its first position before the poser ends (cm).
const END_DISTANCE: f32 = 10.;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) enum EntranceCamera {
    /// No entrance camera: a player placed by a probe or a harness, or after it ended.
    #[default]
    Inactive,
    /// Started with the scene; the target's first position is taken on the next update.
    Starting,
    Playing {
        anchor: Vec3,
    },
    /// The poser asked to end; the pose updater switches away in the next update.
    Ending,
}

impl EntranceCamera {
    pub(super) fn playing(self) -> bool {
        !matches!(self, Self::Inactive)
    }
    /// One camera update, after the actors moved. `target` is the player's position (cm) and
    /// `reset` the player's camera reset trigger.
    pub(super) fn update(&mut self, target: Vec3, reset: bool) {
        *self = match *self {
            Self::Inactive => Self::Inactive,
            Self::Starting => Self::Playing { anchor: target },
            Self::Playing { anchor } => {
                if reset || (anchor - target).length() > END_DISTANCE {
                    Self::Ending
                } else {
                    Self::Playing { anchor }
                }
            }
            Self::Ending => Self::Inactive,
        };
    }
}
