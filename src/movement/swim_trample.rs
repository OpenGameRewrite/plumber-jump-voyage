//! Stomping on something in the water: `SwimTrample` (the hop after the swimmer's body hit an
//! enemy from above) and `SwimReflectDownFall` (the dive that hit something on its way down).
//!
//! Written from the 1.0.0 executable; no recording reaches them (the rewrite has no enemies, so
//! nothing ever raises the attack-sensor triggers), so they are UNVERIFIED. The triggers are
//! `PlayerTrigger`'s attack-sensor flags that the player's sensors set when they touch another
//! actor (`PlayerActorHakoniwa::attackSensor`): 1 (any attack hit), 2 (trampled something) and
//! 3 (the hip drop attack hit). A host raises them with [`Player::receive_swim_attack`].
use super::swim::{SwimEnd, SwimNerve};
use super::swim_const as k;
use super::*;

const GRAVITY: Vec3 = Vec3::NEG_Y;

/// The attack-sensor triggers of the step (`EAttackSensorTrigger` 1 to 3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SwimAttack {
    /// 1: the swimmer's attack hit something.
    pub hit: bool,
    /// 2: the swimmer trampled something.
    pub trample: bool,
    /// 3: the hip drop's attack hit something.
    pub hip_drop_hit: bool,
}

impl Player {
    /// A sensor of the player touched another actor in this step: the water states read the
    /// triggers in this step's nerve (they are cleared at its end).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn receive_swim_attack(&mut self, attack: SwimAttack) {
        self.swim.attack = attack;
    }

    /// The vertical speed the attack triggers give (`0x48b154`): a trample rises with
    /// `DiveTramplePower`, another hit with `SwimTramplePower`; `None` without either.
    pub(super) fn swim_attack_rise(&self) -> Option<f32> {
        if self.swim.attack.trample {
            Some(k::DIVE_TRAMPLE_POWER)
        } else if self.swim.attack.hit {
            Some(k::TRAMPLE_POWER)
        } else {
            None
        }
    }

    /// The nerves under the surface hop into the trample when the swimmer trampled something.
    pub(super) fn swim_tramples(&mut self) -> bool {
        if self.swim.attack.trample {
            self.set_swim_nerve(SwimNerve::Trample);
            return true;
        }
        false
    }

    /// `PlayerStateSwim::exeSwimTrample`: the hop; it counts as a paddle for the stroke's rules.
    pub(super) fn exe_swim_trample(
        &mut self,
        first: bool,
        o: &Orders,
        _scene: &Course,
    ) -> Option<SwimEnd> {
        if first {
            let rate = Self::paddle_rate(self.swim.since_paddle as f32);
            self.swim.since_paddle = 0;
            self.swim.paddle_restart = false;
            self.swim.paddle_rate = rate;
            self.swim_anim("SwimTrample");
        }
        self.swim_underwater_motion();
        self.swim_turn(1.);
        self.update_swim_pose();
        let finder = self.water_finder;
        if finder.found && finder.distance < k::SURFACE_START_DIST {
            self.set_swim_nerve(SwimNerve::Surface);
            return None;
        }
        if o.jump {
            self.set_swim_nerve(SwimNerve::Paddle);
            return None;
        }
        if o.crouch_pressed {
            self.set_swim_nerve(SwimNerve::HipDropStart);
            return None;
        }
        if self.is_on_ground_by(self.v) {
            if self.is_move() {
                self.swim_walk_velocity();
                self.set_swim_nerve(SwimNerve::Walk);
            } else {
                self.swim_wait_ground(k::GRAVITY, k::FALL_SPEED_MAX, 0., 0.);
                self.set_swim_nerve(SwimNerve::Land);
            }
            return None;
        }
        // The hop ends with the clip, or when the swimmer has stopped rising.
        if self.anim_done(clip::SWIM_TRAMPLE) || self.v.y < 0. {
            self.set_swim_nerve(SwimNerve::Move);
        }
        None
    }

    /// `PlayerStateSwim::exeSwimReflectDownFall`: the dive that hit something on its way down
    /// bounces up with `DiveTramplePower`, then falls back with the swim's gravity.
    pub(super) fn exe_swim_reflect_down_fall(
        &mut self,
        first: bool,
        o: &Orders,
        _scene: &Course,
    ) -> Option<SwimEnd> {
        if first {
            self.swim_anim("SwimTrample");
            self.v = Vec3::new(
                -GRAVITY.x * k::DIVE_TRAMPLE_POWER,
                -GRAVITY.y * k::DIVE_TRAMPLE_POWER,
                -GRAVITY.z * k::DIVE_TRAMPLE_POWER,
            );
        }
        // `al::addVelocityToGravity(SwimGravity)`.
        self.v = Vec3::new(
            self.v.x + GRAVITY.x * k::GRAVITY,
            self.v.y + GRAVITY.y * k::GRAVITY,
            self.v.z + GRAVITY.z * k::GRAVITY,
        );
        self.update_swim_pose();
        let finder = self.water_finder;
        if finder.found && finder.distance < k::SURFACE_START_DIST {
            self.set_swim_nerve(SwimNerve::Surface);
            return None;
        }
        if self.swim.attack.trample {
            self.set_swim_nerve(SwimNerve::Trample);
            return None;
        }
        if self.swim.steps as i32 > k::DIVE_TRAMPLE_CANCEL_FRAME {
            if o.jump {
                self.set_swim_nerve(SwimNerve::Paddle);
                return None;
            }
            if o.crouch_pressed {
                self.set_swim_nerve(SwimNerve::HipDropStart);
                return None;
            }
            if self.is_move() {
                self.set_swim_nerve(SwimNerve::Move);
                return None;
            }
        }
        if self.anim_done(clip::SWIM_TRAMPLE) || self.v.y < 0. {
            self.set_swim_nerve(SwimNerve::Move);
        }
        None
    }
}
