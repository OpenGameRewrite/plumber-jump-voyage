//! PlayerActorHakoniwa::checkDeathArea and PlayerStateAbyss (1.0.0).
use super::*;

impl Player {
    pub(super) fn check_death_area(&mut self, scene: &Course) {
        if self.is_dead() || self.recovering() && self.mode != Move::RecoveryFall {
            return;
        }
        let death_area = match &scene.mesh {
            Some(mesh) => mesh
                .death
                .volume_at(mesh.to_game_vec(self.position_cm()))
                .is_some(),
            // The toy course's kill plane is scene geometry, not a player reset timer.
            None => self.pos.y < -18.,
        };
        if !death_area {
            return;
        }
        if self.recovery.valid() && self.damage.hp > 1 {
            self.begin_safety_recovery();
            return;
        }
        self.damage.hp = 0;
        self.recovery.safety = None;
        self.death.kind = if self.cap.is_none() {
            death::Kind::AbyssWithCapMessage
        } else {
            death::Kind::Abyss
        };
        self.death.abyss_pending = true;
    }
    pub(super) fn start_pending_abyss(&mut self) {
        if !std::mem::take(&mut self.death.abyss_pending) {
            return;
        }
        self.nerve.clear();
        self.nerve.actor = "Abyss";
        self.nerve.step = 0;
        self.mode = Move::AbyssFall;
        self.frame = 0;
    }
    pub(super) fn abyss_fall(&mut self) {
        if self.frame == 0 {
            self.sub.active = false;
            self.sub_anim = None;
            self.start_clip("DeadFall");
            self.events.push(Fx::Abyss);
        }
        self.v.y = (self.v.y - 1.5).max(-35.);
    }
    pub(super) fn move_without_collision(&mut self, scene: &Course) {
        let start = self.position_cm();
        let end = if let Some(mesh) = &scene.mesh {
            mesh.relative_vec(mesh.to_game_vec(start) + self.v)
        } else {
            start + self.v
        };
        self.pos = end * UNIT;
        self.collider_end = Some((self.pos, end));
    }
}
