//! PlayerStatePress's ground-crush path (1.0.0 0x478df4 / 0x479464).
use super::*;

#[derive(Debug, Default)]
pub(super) struct Press {
    pub start: Vec3,
    pub target: Vec3,
    pub frozen_frame: f32,
    pub scale: Vec3,
}
impl Player {
    pub(super) fn dispatch_press(&mut self, scene: &Course) -> bool {
        if self.is_dead()
            || self.recovering()
            || !(self.collider.pressed_ground || self.collider.pressed_wall)
        {
            return false;
        }
        // Only the ground squeeze has a reproduced state pose so far. Wall
        // squeeze state posing still needs its own original-code port and capture.
        if !self.collider.pressed_ground {
            return false;
        }
        self.damage.hp = 0;
        self.death.kind = death::Kind::Press;
        self.nerve.clear();
        self.nerve.actor = "Press";
        self.nerve.step = 0;
        self.frame = 0;
        self.mode = Move::Press;
        self.sub.active = false;
        self.sub_anim = None;
        self.v = Vec3::ZERO;
        let start = self.position_cm();
        let normal = self.collider.press_normal;
        let mut target = start;
        if let Some(mesh) = &scene.mesh
            && let Some(hit) = crate::collider::first_strike(
                mesh,
                start - normal * 20.,
                normal * 220.,
                crate::collider::FaceFilter::Any,
            )
        {
            target = hit.position;
        }
        self.press = Press {
            start,
            target,
            frozen_frame: self.anim.frame,
            scale: Vec3::ONE,
        };
        self.slerp_pose_up(self.ground_normal_or_up(), 1., 180.);
        self.events.push(Fx::Crushed);
        true
    }
    pub(super) fn press_step(&mut self) {
        self.anim.rate = 0.;
        let rate = (self.frame as f32 / 10.).clamp(0., 1.);
        self.press.scale = Vec3::ONE + (Vec3::new(1., 0.1, 1.) - Vec3::ONE) * rate;
        let position = self.press.start + (self.press.target - self.press.start) * rate;
        self.pos = position * UNIT;
        self.collider_end = Some((self.pos, position));
        if self.frame > 60 {
            self.mode = Move::Dead;
            self.frame = 0;
            self.nerve.choose_silently(true);
        }
    }
    pub fn press_scale(&self) -> Vec3 {
        if self.death.kind == death::Kind::Press {
            self.press.scale
        } else {
            Vec3::ONE
        }
    }
    pub fn pressed_flags(&self) -> (bool, bool) {
        (self.collider.pressed_ground, self.collider.pressed_wall)
    }
}
