//! PlayerExternalVelocity's saved-ground contribution (1.0.0 0x4411b0).
use super::*;

#[derive(Clone, Debug, Default)]
pub(super) struct GroundInertia {
    pub request: u32,
    pub saved: Vec3,
    pub last: Vec3,
    /// calcJumpInertia samples the actor, independently of the saved contact.
    pub jump_force: Vec3,
}
/// What a teleport keeps of `PlayerExternalVelocity` (see `Player::inertia_kept_by_teleport`):
/// the saved ground force, the external and the area velocity and the wind forces not yet applied.
pub struct KeptInertia {
    ground: GroundInertia,
    external: Vec3,
    area: Vec3,
    keeper: external_velocity::ForceKeeper,
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
        self.ground_inertia.jump_force = scene.mesh.as_ref().map_or(Vec3::ZERO, |mesh| {
            let at = self
                .stage_translation
                .filter(|(p, _)| *p == self.pos)
                .map_or_else(|| mesh.to_game_vec(self.position_cm()), |(_, at)| at);
            self.collider.moving_ground_displacement_at(mesh, at)
        });
        let force = scene
            .mesh
            .as_ref()
            .map_or(Vec3::ZERO, |m| self.collider.moving_ground_force(m));
        self.ground_inertia
            .update(self.is_on_ground_by(self.units_per_frame()), force);
    }
    /// The probe's placement preserves the `PlayerExternalVelocity` accumulators and pending
    /// forces. It resets collision and joint dynamics (0x565ce8 and 0x56b680). The old ground
    /// result is invalidated at 0x42fe8c. See `Player::start_in_air`.
    pub fn inertia_kept_by_teleport(&self) -> KeptInertia {
        KeptInertia {
            ground: self.ground_inertia.clone(),
            external: self.external_velocity,
            area: self.area_velocity,
            keeper: self.external_keeper.clone(),
        }
    }
    pub fn keep_inertia_through_teleport(&mut self, kept: KeptInertia) {
        self.ground_inertia = kept.ground;
        self.external_velocity = kept.external;
        self.area_velocity = kept.area;
        self.external_keeper = kept.keeper;
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
    a_jump_turn_resets_collision_after_takeoff_setup();
    choosing_pivot_does_not_enable_a_turn_jump();
    a_rotating_wait_snapshot_does_not_turn_normal_pull_into_travel();
}
#[cfg_attr(test, test)]
fn a_rotating_wait_snapshot_does_not_turn_normal_pull_into_travel() {
    let mut player = Player {
        rotating_ground: true,
        ground_lost: 3,
        ..Player::default()
    };
    let snapshot = Snapshot {
        mode: Some(Move::Idle),
        grounded: true,
        v: Vec3::new(0., -2.4, -1.8),
        anim: Some(("Wait", 5.)),
        counters: vec![("ground_lost".into(), 0)],
        ..Snapshot::default()
    };
    player.inject(&snapshot);
    assert_eq!(player.v, Vec3::ZERO);
    assert_eq!(player.units_per_frame(), snapshot.v);
    assert_eq!(player.ground_lost, 3);
}
#[cfg_attr(test, test)]
fn choosing_pivot_does_not_enable_a_turn_jump() {
    let mut player = Player {
        mode: Move::Turn,
        grounded: true,
        ..Player::default()
    };
    player.nerve.actor = "Run";
    player.nerve.queue("TurnPoint", |_| {});
    player.ground_jump(&Orders::default());
    assert_eq!(player.mode, Move::Jump);
}
#[cfg_attr(test, test)]
fn a_jump_turn_resets_collision_after_takeoff_setup() {
    let mut player = Player::default();
    player.set_pose(Quat::from_rotation_x(30_f32.to_radians()));
    player.collider.floor = Some(crate::collider::Contact {
        normal: Vec3::Y,
        position: Vec3::ZERO,
        triangle: 0,
        location: crate::collider::Location::Face,
    });
    player.turn_pose_up_for_jump(Vec3::Y);
    assert!(player.collider.grounded());
    assert!(player.collision_expand_pending);
    player.collide_mesh(&crate::geometry::TriangleMesh::from_corners(&[]), false);
    assert!(!player.collider.grounded());
    assert!(!player.collision_expand_pending);
}
#[cfg_attr(test, test)]
fn fall_uses_the_current_ground_velocity() {
    for rotating in [false, true] {
        let mut player = Player {
            grounded: true,
            mesh_ground: Some(Vec3::new(0., 0.6, 0.8)),
            ground_move: Some((Vec2::ZERO, t::GRAVITY, Vec3::NEG_Y * t::GRAVITY)),
            rotating_ground: rotating,
            ground_lost: GROUND_LOST_FRAMES + 1,
            ..Player::default()
        };
        player.v = Vec3::NEG_Y * t::GRAVITY;
        player.judge_fall();
        assert_eq!(player.mode, Move::Fall);
        let expected = if rotating {
            Vec3::new(0., -0.6 * t::GRAVITY, -0.8 * t::GRAVITY)
        } else {
            Vec3::NEG_Y * t::GRAVITY
        };
        assert_eq!(player.v, expected);
    }
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
