//! The player's own per-frame sounds: what the player tells the sound control
//! about this frame, and the events it answers with.
use super::{Course, Fx, Player};
use crate::se_ctrl::{CameraWater, SeEvent, SeFrame};

/// Clips the roll sound follows.
const ROLLING_CLIPS: [&str; 3] = ["Rolling", "RollingEnd", "RollingLand"];

impl Player {
    /// Feed the sound control the frame's result. Called once at the end of
    /// each step, after the animation and the velocity are final.
    pub(super) fn update_sounds(&mut self, scene: &Course) {
        let frame = SeFrame {
            // A whole-body sub clip (the dive's stand-up) takes the legs over.
            stepping: self.anim.name == "Move" && self.sub_anim.is_none(),
            rate: self.anim.rate,
            phase: self.cycle.shown,
            run_start: self.cycle.weights[4],
            // PlayerSeCtrl::update uses calcSpeed for its persistent footstep
            // counter, including the ground pull during a stopped pivot.
            speed: super::vector_lib::length(self.units_per_frame()),
            vertical_speed: if self.is_dead() || self.recovery_bubble() {
                0.
            } else {
                self.units_per_frame().y
            },
            in_water: scene
                .water_areas()
                .is_water(scene.in_water_frame(self.position_cm())),
            rolling: ROLLING_CLIPS
                .contains(&self.anim.name)
                .then_some(self.anim.frame),
            anim_frame: self.anim.frame,
            stroke: (self.anim.name == "SwimStand").then_some(self.anim.frame),
            camera: self.camera_water(scene),
        };
        for event in self.sounds.update(&frame) {
            self.events.push(match event {
                SeEvent::FootLift { left } => Fx::FootLift { left },
                SeEvent::Footfall { left } => Fx::Step { left },
                SeEvent::Roll => Fx::RollSpin,
                SeEvent::FallWhoosh { frames } => Fx::FallWhoosh { frames },
                SeEvent::CameraWater { entered } => Fx::CameraWater { entered },
                SeEvent::CameraWaterHold => Fx::CameraWaterHold,
                SeEvent::SwimStroke { camera_under } => Fx::SwimStrokeSound { camera_under },
            });
        }
    }

    /// Where the camera is against the water, when the host has told the player where it is.
    fn camera_water(&self, scene: &Course) -> Option<CameraWater> {
        let camera = self.camera_cm?;
        let areas = scene.water_areas();
        let at = scene.in_water_frame(camera);
        let here = scene.in_water_frame(self.position_cm());
        Some(CameraWater {
            under: areas.volume_at(at).is_some(),
            ambience: areas.is_water(at) && areas.is_water(here),
        })
    }
}
