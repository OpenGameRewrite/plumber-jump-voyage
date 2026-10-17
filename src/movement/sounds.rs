//! The player's own per-frame sounds: what the player tells the sound control
//! about this frame, and the events it answers with.
use super::{Fx, Move, Player};
use crate::se_ctrl::{SeEvent, SeFrame};

/// Clips the roll sound follows.
const ROLLING_CLIPS: [&str; 3] = ["Rolling", "RollingEnd", "RollingLand"];

impl Player {
    /// Feed the sound control the frame's result. Called once at the end of
    /// each step, after the animation and the velocity are final.
    pub(super) fn update_sounds(&mut self) {
        let frame = SeFrame {
            // A whole-body sub clip (the dive's stand-up) takes the legs over.
            stepping: self.anim.name == "Move" && self.sub_anim.is_none(),
            rate: self.anim.rate,
            phase: self.cycle.shown,
            run_start: self.cycle.weights[4],
            speed: self.move_speed,
            vertical_speed: self.units_per_frame().y,
            swimming: matches!(self.mode, Move::Swim | Move::SwimDash),
            rolling: ROLLING_CLIPS
                .contains(&self.anim.name)
                .then_some(self.anim.frame),
        };
        for event in self.sounds.update(&frame) {
            self.events.push(match event {
                SeEvent::FootLift { left } => Fx::FootLift { left },
                SeEvent::Footfall { left } => Fx::Step { left },
                SeEvent::Roll => Fx::RollSpin,
                SeEvent::FallWhoosh { frames } => Fx::FallWhoosh { frames },
            });
        }
    }
}
