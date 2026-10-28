//! HP and invalidation needed by the actor's damage dispatch. State-specific motion stays
//! with its consumer; scene restart and recovery do not belong to this keeper.
use super::*;

#[derive(Debug)]
pub(super) struct Damage {
    pub hp: u8,
    pub invalid: u32,
    pub hidden: bool,
    blocked: bool,
    fire_message: bool,
    clipped_candles: Vec<usize>,
}

pub(super) fn self_check() {
    tests::static_needle_uses_keeper_landing_and_scene_death();
    tests::candle_spheres_use_strict_hit_and_receive_invalidation();
    tests::swim_cancel_reads_strict_post_update_steps();
    tests::poison_bypasses_invalidation_and_uses_poison_death();
}
mod tests {
    use super::*;
    #[cfg_attr(test, test)]
    pub(super) fn candle_spheres_use_strict_hit_and_receive_invalidation() {
        let fire = Vec3::ZERO;
        assert!(!candle_hits_player(Vec3::new(0., -190., 0.), fire));
        assert!(candle_hits_player(Vec3::new(0., -189., 0.), fire));
        assert!(candle_hits_player(Vec3::new(89., -75., 0.), fire));
        assert!(!candle_hits_player(Vec3::new(90., -75., 0.), fire));
        let mut p = Player::default();
        p.damage.blocked = true;
        p.request_fire_damage();
        assert!(!p.damage.fire_message);
        p.damage.blocked = false;
        p.request_fire_damage();
        p.dispatch_fire_damage(&Course::default());
        assert!(p.burning());
        assert_eq!(p.damage.hp, 2);
        assert!(!p.damage.fire_message);
    }
    #[cfg_attr(test, test)]
    pub(super) fn static_needle_uses_keeper_landing_and_scene_death() {
        let scene = Course {
            solids: vec![Solid {
                center: Vec3::NEG_Y,
                half: Vec3::new(50., 1., 50.),
                slope: 0.,
                moving: false,
                slippery: false,
            }],
            needles: vec![(Vec3::ZERO, Vec3::splat(50.))],
            ..default()
        };
        let mut p = Player::default();
        let mut entries = Vec::new();
        let mut saw_land = false;
        for f in 0..520 {
            p.step(&Orders::default(), &scene);
            if p.nerve.actor == "Damage" && p.nerve.step == 1 {
                entries.push(f);
            }
            saw_land |= p.mode == Move::DamageLand && p.v == Vec3::NEG_Y * 3.;
        }
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[1] - entries[0], 241);
        assert_eq!(entries[2] - entries[1], 241);
        assert!(saw_land && p.is_dead() && p.mode == Move::DamageDead);
        assert_eq!(p.v, Vec3::ZERO);
        assert!(p.death.ready());
    }
    #[cfg_attr(test, test)]
    pub(super) fn swim_cancel_reads_strict_post_update_steps() {
        let mut p = Player::default();
        for (mode, threshold) in [(Move::DamageSwim, 50), (Move::DamageLandWater, 40)] {
            p.mode = mode;
            p.frame = threshold - 1;
            assert!(!p.damage_swim_cancel());
            p.frame = threshold;
            assert!(p.damage_swim_cancel());
        }
        p.mode = Move::DamageSurface;
        p.frame = 120;
        assert!(!p.damage_swim_cancel());
    }
    #[cfg_attr(test, test)]
    pub(super) fn poison_bypasses_invalidation_and_uses_poison_death() {
        let mut p = Player::default();
        p.damage.fire();
        p.touched.floor = Some(CollisionCodes {
            floor: "Poison".into(),
            ..default()
        });
        p.dispatch_damage(&Course::default());
        assert!(p.is_dead());
        p.damage_life_step(&Orders::default(), &Course::default());
        assert_eq!(p.anim.name, "DeadPoison");
        assert_eq!(p.v, Vec3::ZERO);
    }
}
impl Default for Damage {
    fn default() -> Self {
        Self {
            hp: 3,
            invalid: 0,
            hidden: false,
            blocked: false,
            fire_message: false,
            clipped_candles: Vec::new(),
        }
    }
}
impl Damage {
    /// PlayerDamageKeeper::damageForce and PlayerHitPointData::damage (0x538a94).
    /// Fire collision bypasses ordinary damage invalidation (0x429748..0x4297f0).
    fn fire(&mut self) {
        self.invalid = 240;
        self.blocked = true;
        self.hp = self.hp.saturating_sub(1);
    }
    /// PlayerDamageKeeper::update: death restores visibility and stops the damage blink.
    pub(super) fn update(&mut self, dead: bool) {
        if self.invalid == 0 || dead {
            self.blocked = false;
            self.hidden = false;
            return;
        }
        self.hidden = self.invalid % 6 < 1;
        self.invalid -= 1;
    }
}
/// HitSensorDirector::executeHitCheck, with the 3D InitSensor Player/PlayerFoot
/// spheres (Foot, Body, Head). PlayerAttack and PlayerEye cannot receive this fire.
fn candle_hits_player(feet: Vec3, fire: Vec3) -> bool {
    [(40., 40.), (75., 50.), (110., 40.)]
        .into_iter()
        .any(|(y, radius)| {
            let delta = feet + Vec3::Y * y - fire;
            let distance = (delta.x * delta.x + delta.y * delta.y) + delta.z * delta.z;
            distance < (radius + 40.) * (radius + 40.)
        })
}
impl Player {
    pub fn set_clipped_candles(&mut self, clipped: Vec<usize>) {
        self.damage.clipped_candles = clipped;
    }

    /// Sensors use the previous actor pose. Receive precedes this movement's
    /// executePreMovementNerveChange, which consumes the request below.
    pub(super) fn receive_candle_fire(&mut self, scene: &Course) {
        if let Some(mesh) = &scene.mesh {
            let feet = mesh.to_game_vec(self.position_cm());
            if mesh.candle_fire.iter().enumerate().any(|(i, &fire)| {
                !self.damage.clipped_candles.contains(&i) && candle_hits_player(feet, fire)
            }) {
                self.request_fire_damage();
            }
        }
    }

    pub(super) fn in_damage_state(&self) -> bool {
        matches!(
            self.mode,
            Move::DamageAir
                | Move::DamageLand
                | Move::DamageDead
                | Move::DamageSwim
                | Move::DamageSurface
                | Move::DamageLandWater
                | Move::DamageSwimDead
        )
    }

    /// checkDamageFromCollision / executePreMovementNerveChange. Press needs the
    /// separate pressed-collision flags and opposing geometry; a floor code alone
    /// never supplies that cause. Its eventual dispatch belongs before ordinary damage.
    pub(super) fn dispatch_damage(&mut self, scene: &Course) {
        if self.is_dead() || self.recovering() {
            return;
        }
        let code =
            |c: &Option<CollisionCodes>, name: &str| c.as_ref().is_some_and(|c| c.floor == name);
        let on_ground = self.is_on_ground_by(self.v);
        let poison =
            on_ground && code(&self.touched.floor, "Poison") || code(&self.touched.wall, "Poison");
        if poison {
            if self.recovery.valid() && self.damage.hp > 1 {
                self.begin_safety_recovery();
                return;
            }
            self.damage.hp = 0;
            self.soot.record_poison();
            self.recovery.safety = None;
            self.appear_damage_life(true, false);
            return;
        }
        self.dispatch_fire_damage(scene);
        if self.burning() || self.is_dead() {
            return;
        }
        let needle = on_ground && code(&self.touched.floor, "Needle")
            || code(&self.touched.wall, "Needle")
            || code(&self.touched.ceiling, "Needle");
        if !needle || self.damage.blocked {
            return;
        }
        self.damage.fire();
        if self.mode == Move::Swim {
            self.appear_damage_swim(scene, false);
        } else {
            self.appear_damage_life(false, false);
        }
    }
    /// A sensor's fire attack requests damage independently of the collision-code query.
    pub fn request_fire_damage(&mut self) {
        // PlayerActorHakoniwa::receiveMsg checks PlayerDamageKeeper's prevent byte.
        // Static fire floors bypass it; an EnemyAttackFire message does not.
        if !self.damage.blocked {
            self.damage.fire_message = true;
        }
    }
    pub fn is_dead(&self) -> bool {
        self.damage.hp == 0
    }
    pub fn damage_hidden(&self) -> bool {
        self.damage.hidden
    }

    /// Collision causes are consumed before the nerve runs, not during collision resolution.
    /// The floor/wall/ceiling code queries are distinct from a sensor's fire request.
    pub(super) fn dispatch_fire_damage(&mut self, scene: &Course) {
        if self.is_dead() || self.recovering() {
            return;
        }
        let fire = |codes: &Option<CollisionCodes>| {
            codes.as_ref().is_some_and(|c| c.floor == "DamageFire")
        };
        let floor = fire(&self.touched.floor) || self.grounded && scene.hot(self.pos);
        let wall = fire(&self.touched.wall);
        let ceiling = fire(&self.touched.ceiling);
        // rs::isTouchDamageFireCode3D (0x55e584): a floor must support the player;
        // a wall/ceiling must oppose the velocity. Fire-volume causes will feed this same
        // dispatch when the stage loader supports them; do not infer them from a material.
        let on_floor = floor && self.is_on_ground_by(self.v);
        let against_wall = wall
            && self
                .collider
                .wall
                .as_ref()
                .is_some_and(|c| self.v.dot(c.normal) < 0.);
        let against_ceiling = ceiling
            && self
                .collider
                .ceiling
                .as_ref()
                .is_some_and(|c| self.v.dot(c.normal) < 0.);
        let message = std::mem::take(&mut self.damage.fire_message);
        if !(message || on_floor || against_wall || against_ceiling) {
            return;
        }
        if self.recovery.valid() && self.damage.hp > 1 {
            self.begin_safety_recovery();
            return;
        }
        self.recovery.safety = None;
        self.damage.fire();
        self.appear_damage_fire(floor, wall, ceiling);
    }
}
