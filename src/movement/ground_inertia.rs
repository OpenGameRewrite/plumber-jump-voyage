//! PlayerExternalVelocity's saved-ground contribution (1.0.0 0x4411b0).
use super::*;

#[derive(Clone, Debug, Default)]
pub(super) struct GroundInertia {
    pub request: u32,
    pub saved: Vec3,
    pub last: Vec3,
}
impl GroundInertia {
    fn update(&mut self, grounded: bool, force: Vec3) {
        self.last = Vec3::ZERO;
        if grounded {
            self.saved = force;
            return;
        }
        if self.request == 0 {
            self.saved = Vec3::ZERO;
        } else {
            self.last = self.saved;
        }
        self.request = self.request.saturating_sub(1);
    }
    /// cancelAndFeedbackLastGroundInertia (0x440ca0), JumpInertiaRate=0.7.
    fn feedback(&mut self, front: Vec3, front_only: bool) -> Vec3 {
        let inertia = vector_lib::verticalize_vec(Vec3::NEG_Y, self.last);
        self.last = Vec3::ZERO;
        self.request = 0;
        let direction = pose_math::try_normalize_or_zero(inertia).0;
        if direction == Vec3::ZERO {
            return Vec3::ZERO;
        }
        let mut strength = 0.7;
        if front_only {
            let front = pose_math::try_normalize_or_zero(front.with_y(0.)).0;
            if front != Vec3::ZERO {
                strength *= pose_math::dot(front, direction).max(0.);
            }
        }
        inertia * strength
    }
}
impl Player {
    pub(super) fn update_ground_inertia(&mut self, scene: &Course) {
        let force = scene
            .mesh
            .as_ref()
            .map_or(Vec3::ZERO, |m| self.collider.moving_ground_force(m));
        self.ground_inertia
            .update(self.is_on_ground_by(self.units_per_frame()), force);
    }
    pub(super) fn feedback_ground_inertia(&mut self, front_only: bool) {
        let (_, front) = self.pose_axes();
        let feedback = self.ground_inertia.feedback(front, front_only);
        if feedback != Vec3::ZERO {
            self.v += feedback;
        }
    }
}

pub(super) fn self_check() {
    saved_force_lasts_one_airborne_update();
    fall_uses_the_current_ground_velocity();
}
#[cfg_attr(test, test)]
fn fall_uses_the_current_ground_velocity() {
    let mut player = Player {
        grounded: true,
        mesh_ground: Some(Vec3::new(0., 0.6, 0.8)),
        ground_move: Some((Vec2::ZERO, t::GRAVITY, Vec3::NEG_Y * t::GRAVITY)),
        ground_lost: GROUND_LOST_FRAMES + 1,
        ..Player::default()
    };
    player.v = Vec3::NEG_Y * t::GRAVITY;
    player.judge_fall();
    assert_eq!(player.mode, Move::Fall);
    assert_eq!(
        player.v,
        Vec3::new(0., -0.6 * t::GRAVITY, -0.8 * t::GRAVITY)
    );
}
#[cfg_attr(test, test)]
fn saved_force_lasts_one_airborne_update() {
    let mut state = GroundInertia::default();
    state.update(true, Vec3::new(10., -5., 0.));
    state.request = 1;
    state.update(false, Vec3::ZERO);
    assert_eq!(state.last, Vec3::new(10., -5., 0.));
    assert_eq!(state.request, 0);
    let mut feedback = state.clone();
    assert_eq!(feedback.feedback(Vec3::X, true), Vec3::X * 7.);
    assert_eq!(feedback.last, Vec3::ZERO);
    assert_eq!(feedback.request, 0);
    state.update(false, Vec3::ZERO);
    assert_eq!(state.last, Vec3::ZERO);
    assert_eq!(state.saved, Vec3::ZERO);
}
