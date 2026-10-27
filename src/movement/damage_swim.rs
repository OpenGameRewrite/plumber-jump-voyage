//! PlayerStateDamageSwim, 1.0.0 0x46a054..0x46abb4.
use super::swim::{ACTOR_JUDGE, SWIM_JUDGE};
use super::*;

impl Player {
    pub(super) fn appear_damage_swim(&mut self, scene: &Course, entering: bool) {
        self.nerve.clear();
        self.nerve.actor = "DamageSwim";
        self.nerve.step = 0;
        self.frame = 0;
        self.life.no_down = false;
        self.cap_jump_possible = true;
        if self.is_dead() {
            self.v = Vec3::ZERO;
            self.mode = Move::DamageSwimDead;
        } else if entering {
            self.try_reaction_water_in(scene);
            if self.v.y > 0. {
                self.v -= Vec3::NEG_Y * (2. * pose_math::dot(self.v, Vec3::NEG_Y));
            }
            self.mode = Move::DamageLandWater;
        } else if self.water_finder.found && self.water_finder.distance < 120. {
            self.try_reaction_water_out(scene);
            self.mode = Move::DamageSurface;
        } else {
            self.mode = Move::DamageSwim;
        }
    }
    /// isGreaterStep, not isGreaterEqualStep (0x46a42c).
    pub(super) fn damage_swim_cancel(&self) -> bool {
        // The actor asks after NerveKeeper::update has counted this execution.
        let step = self.frame + u32::from(!self.nerve.deferred);
        self.mode == Move::DamageSwim && step > 50
            || self.mode == Move::DamageLandWater && step > 40
    }
    pub(super) fn damage_swim_step(&mut self, o: &Orders, scene: &Course) {
        self.wet.record_in_water();
        if !self.is_dead() && self.fire_throws(o) {
            return;
        }
        let mut ended = false;
        let mut end_ground = false;
        let mut end_in_water = true;
        match self.mode {
            Move::DamageSwim => {
                if self.frame == 0 {
                    self.start_clip(if self.life.no_down {
                        "NoDamageDownSwim"
                    } else {
                        "DamageDownSwim"
                    });
                    self.v = self.pose_axes().1 * -3.;
                }
                let (vertical, horizontal) =
                    vector_lib::separate_parallel_vertical(Vec3::NEG_Y, self.v);
                let mut speed = -pose_math::dot(vertical, Vec3::NEG_Y);
                if self.is_on_ground_by(self.v) && speed < 0. {
                    speed = 0.;
                }
                speed = (speed - 0.02).max(-6.5);
                self.v = horizontal * 0.975 + Vec3::Y * speed;
                end_in_water = self.is_in_water(scene, SWIM_JUDGE);
                ended = !end_in_water || self.anim_done(140);
            }
            Move::DamageSurface => {
                if self.frame == 0 {
                    self.start_clip(if self.life.no_down {
                        "NoDamageDownSwimSurface"
                    } else {
                        "DamageDownSwimSurface"
                    });
                    let front = self.pose_axes().1;
                    self.v = vector_lib::verticalize_vec(front, self.v) - front * 3.;
                    self.damage_air_setup(30., 4., 20., 0.95);
                }
                self.damage_air_update();
                if self.is_on_ground_by(self.v) {
                    ended = true;
                    end_ground = true;
                } else if self.is_in_water(scene, ACTOR_JUDGE) {
                    self.try_reaction_water_in(scene);
                    self.damage_choose(Move::DamageLandWater);
                }
            }
            Move::DamageLandWater => {
                if self.frame == 0 {
                    self.start_clip("DamageSwimSurfaceLand");
                    // limitVelocityDir separates/rejoins even below its limit.
                    air_move::limit_velocity_dir(&mut self.v, Vec3::NEG_Y, 5.);
                }
                self.move_dive_swim(0.975, 1., 0.05);
                if self.is_on_ground_by(self.v) {
                    self.v = vector_lib::verticalize_vec(Vec3::NEG_Y, self.v);
                }
                end_in_water = self.is_in_water(scene, SWIM_JUDGE);
                if !end_in_water {
                    self.try_reaction_water_out(scene);
                }
                ended = !end_in_water || self.anim_done(145);
            }
            Move::DamageSwimDead => {
                if self.frame == 0 {
                    let clip = self.is_in_water(scene, SWIM_JUDGE).then_some("DeadWater");
                    self.damage_dead_first(clip, false);
                }
                if self.anim_done(if self.anim.name == "DeadWater" {
                    264
                } else {
                    190
                }) {
                    self.damage_choose(Move::Dead);
                    self.nerve.renewed = true;
                }
                return;
            }
            _ => unreachable!(),
        }
        if ended && end_ground {
            self.appear_damage_life(false, true);
            self.nerve.choose_silently(true);
        } else if ended || self.damage_swim_cancel() && o.jump {
            if end_in_water {
                self.start_swim_after_damage(scene, o.jump && self.damage_swim_cancel());
            } else {
                self.leave_swim(o);
            }
        }
    }
}
