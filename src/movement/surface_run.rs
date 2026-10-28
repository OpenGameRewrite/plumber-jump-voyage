//! Run's enabled water-surface judge and the original approach helper.
use super::*;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SurfaceRun {
    pub enabled: bool,
    pub result: bool,
}
impl Player {
    pub fn water_surface_run(&self) -> bool {
        self.surface_run.result
    }
    pub(super) fn start_surface_run(&self) -> bool {
        self.force_run.is_active() && self.surface_available() && self.v.y < 0.
    }
    fn surface_available(&self) -> bool {
        self.water_finder.found
            && (self.water_finder.distance >= 0. || self.water_finder.distance.abs() < 0.001)
    }
    pub(super) fn update_surface_run(&mut self) {
        self.surface_run.enabled |= self.force_run.is_active();
        self.surface_run.result = self.surface_run.enabled
            && self.surface_available()
            && (self.force_run.is_active() || self.speed_h() > 20.);
    }
    pub(super) fn respond_surface_run(&mut self, scene: &Course) {
        if !self.surface_run.result {
            return;
        }
        let mut rate = 0.5;
        if !self.force_run.is_active() {
            use joint_math::{calc_rate01, ease_out, lerp_value};
            let speed = self.speed_h();
            let brake = ease_out(calc_rate01(speed, 20., 35.));
            let approach = ease_out(calc_rate01(speed, 30., 38.));
            let scale = lerp_value(0.995, 1., brake);
            self.v.x *= scale;
            self.v.z *= scale;
            rate = lerp_value(0.06, 0.5, approach);
        }
        self.v.y = (self.v.y + (self.water_finder.distance - 0.) * rate).clamp(-5., 5.);
        // The area-only finder has zero +0x28 displacement (no fluid fallback).
        self.move_trans(scene, Vec3::ZERO);
    }
}
