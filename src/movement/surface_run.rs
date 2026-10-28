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

pub(super) fn self_check() {
    checks::run();
}

mod checks {
    use super::*;

    pub(super) fn run() {
        the_start_judge_needs_a_forced_run_a_surface_and_a_fall();
        the_sustain_judge_is_enabled_by_the_forced_run_and_then_by_speed();
    }

    #[cfg(test)]
    #[test]
    fn port_checks() {
        run();
    }

    fn above_water(distance: f32) -> Player {
        let mut player = Player::default();
        player.water_finder.found = true;
        player.water_finder.distance = distance;
        player
    }

    fn the_start_judge_needs_a_forced_run_a_surface_and_a_fall() {
        let mut player = above_water(3.);
        player.v.y = -1.;
        assert!(!player.start_surface_run(), "no forced run");
        player.force_run.setup(2, 38.);
        assert!(player.start_surface_run());
        player.v.y = 1.;
        assert!(!player.start_surface_run(), "rising");
        player.v.y = -1.;
        player.water_finder.distance = -0.5;
        assert!(
            !player.start_surface_run(),
            "the surface is below the player"
        );
        player.water_finder.distance = -0.0005;
        assert!(player.start_surface_run(), "the near-zero tolerance");
        player.water_finder.found = false;
        assert!(!player.start_surface_run(), "no surface");
    }

    fn the_sustain_judge_is_enabled_by_the_forced_run_and_then_by_speed() {
        let mut player = above_water(1.);
        player.v = Vec3::new(30., 0., 0.);
        player.update_surface_run();
        assert!(
            !player.surface_run.result,
            "a fast run alone never enabled it"
        );
        player.force_run.setup(2, 38.);
        player.update_surface_run();
        assert!(player.surface_run.enabled && player.surface_run.result);
        // The enable stays; the forced run may end while the speed keeps the judge true.
        player.force_run.update();
        player.force_run.update();
        player.update_surface_run();
        assert!(player.surface_run.result, "speed above the dash border");
        player.v = Vec3::new(20., 0., 0.);
        player.update_surface_run();
        assert!(!player.surface_run.result, "the comparison is strict");
    }
}
