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
}
impl Default for Damage {
    fn default() -> Self {
        Self {
            hp: 3,
            invalid: 0,
            hidden: false,
            blocked: false,
            fire_message: false,
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
impl Player {
    pub(super) fn in_damage_state(&self) -> bool {
        matches!(self.mode, Move::DamageAir | Move::DamageLand | Move::DamageDead
            | Move::DamageSwim | Move::DamageSurface | Move::DamageLandWater | Move::DamageSwimDead)
    }

    /// checkDamageFromCollision / executePreMovementNerveChange. Press needs the
    /// separate pressed-collision flags and opposing geometry; a floor code alone
    /// never supplies that cause. Its eventual dispatch belongs before ordinary damage.
    pub(super) fn dispatch_damage(&mut self, scene: &Course) {
        if self.is_dead() || self.recovering() { return; }
        let code = |c: &Option<CollisionCodes>, name: &str| c.as_ref().is_some_and(|c| c.floor == name);
        let on_ground = self.is_on_ground_by(self.v);
        let poison = on_ground && code(&self.touched.floor, "Poison") || code(&self.touched.wall, "Poison");
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
        if self.burning() || self.is_dead() { return; }
        let needle = on_ground && code(&self.touched.floor, "Needle")
            || code(&self.touched.wall, "Needle") || code(&self.touched.ceiling, "Needle");
        if !needle || self.damage.blocked { return; }
        self.damage.fire();
        if self.mode == Move::Swim {
            self.appear_damage_swim(scene, false);
        } else {
            self.appear_damage_life(false, false);
        }
    }
    /// A sensor's fire attack requests damage independently of the collision-code query.
    pub fn request_fire_damage(&mut self) {
        self.damage.fire_message = true;
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
