//! Fire stain requests and accumulation (`PlayerStainControl`, 0x465a84..0x46690c).
//! Fire type 2 and poison type 1 share accumulation, hold, fade and washing.
//! The imported material remains a rendering approximation.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Soot {
    pub body: f32,
    pub cap: f32,
    pub smoking: bool,
    pub smoke_called: bool,
    pub poison: bool,
    pub(super) hold: u32,
    decoration: f32,
    active: bool,
    pending: Option<(bool, f32)>,
}
impl Soot {
    /// Requests replace the pending increment; repeated requests are not accumulated here.
    pub fn record_damage_fire(&mut self) {
        self.pending = Some((false, 0.005));
    }
    pub fn record_damage_fire_dead(&mut self) {
        self.pending = Some((false, 1.));
    }
    pub fn record_black_smoke(&mut self) {
        self.pending = Some((false, 0.02));
    }
    /// PlayerStainControl::recordPoison (0x465ac8): type1, increment1.
    pub fn record_poison(&mut self) {
        self.pending = Some((true, 1.));
    }

    fn update(&mut self, cap_on: bool, in_water: bool, wet: bool) {
        self.smoke_called = false;
        if in_water || wet {
            // recordInWater / recordInWet: cancel the non-ice request and hold. Only the
            // water path multiplies levels; lingering wetness is not a second water timer.
            self.pending = None;
            self.smoking = false;
            self.hold = 0;
            if in_water {
                self.body *= 0.98;
                self.cap *= 0.98;
                self.decoration *= 0.98;
            }
        }
        if let Some((poison, increment)) = self.pending.take() {
            if self.poison != poison {
                self.body = 0.;
                self.cap = 0.;
                self.decoration = 0.;
                self.smoking = false;
            }
            self.poison = poison;
            self.active = true;
            self.body = (self.body + increment).min(self.body.max(1.));
            self.cap = if cap_on {
                (self.cap + increment).min(self.cap.max(1.))
            } else {
                0.
            };
            // Guess (U07): the classic model leaves the decoration gate open. Keep its level
            // independently, rather than silently treating it as the body's material target.
            self.decoration = (self.decoration + increment).min(self.decoration.max(1.));
            self.hold = 300;
            if self.body >= 0.75 {
                self.smoking = true;
                self.smoke_called = true;
            }
        } else if self.active {
            if self.hold > 0 {
                self.hold -= 1;
            } else {
                self.body = (self.body - 1. / 300.).max(0.);
                self.cap = (self.cap - 1. / 300.).max(0.);
            }
            if !cap_on {
                self.cap = 0.;
            }
            // The third channel is retained on the ordinary fade path (0x46666c..0x466740).
            if self.body.abs() < 0.001 && self.cap.abs() < 0.001 && self.decoration.abs() < 0.001 {
                self.active = false;
            }
        }
        if self.body <= 0.25 {
            self.smoking = false;
        }
    }
}
impl Player {
    pub(super) fn update_soot(&mut self, scene: &Course) {
        let in_water = self.mode == Move::Swim || scene.wet(self.pos + Vec3::Y * 0.8);
        self.soot
            .update(self.cap.is_none(), in_water, self.wet.is_wet());
    }
}

pub(super) fn self_check() {
    tests::request_threshold_death_hold_and_water_are_distinct();
}
mod tests {
    use super::*;
    #[cfg_attr(test, test)]
    pub(super) fn request_threshold_death_hold_and_water_are_distinct() {
        let mut s = Soot::default();
        for _ in 0..150 {
            s.record_damage_fire();
            s.update(true, false, false);
        }
        assert!(!s.smoking); // f32 accumulation is still below 0.75.
        s.record_damage_fire();
        s.update(true, false, false);
        assert!(s.smoking && s.smoke_called);
        s.record_damage_fire_dead();
        s.update(true, false, false);
        assert_eq!((s.body, s.cap, s.hold), (1., 1., 300));
        s.update(false, false, false);
        assert_eq!((s.cap, s.hold), (0., 299));
        s.update(true, true, false);
        assert_eq!(s.body, 0.98 - 1. / 300.);
        assert_eq!(s.hold, 0);
        let before = s.body;
        s.record_black_smoke();
        s.update(true, false, true);
        assert_eq!(s.body, before - 1. / 300.);
        s.record_black_smoke();
        s.update(true, false, false);
        assert_eq!(s.body, (before - 1. / 300.) + 0.02);
    }
}
