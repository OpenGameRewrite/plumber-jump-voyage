//! The touch jump off the hovering cap: `PlayerCapTouchJump`, sent by the player's body
//! sensors (`PlayerActorHakoniwa::attackSensor` 0x427f54..0x428034) and answered by the cap's
//! `Catch` sensor (`HackCap::receiveMsg` 0x407eb8..0x407f38, see `Cap::take_touch_jump`).
use super::*;
use crate::cap::{PlayerSensor, TouchJumpGate};

/// `PlayerActorHakoniwa/InitSensor.byml`, the `Type: Player` entries: height of the sensor's
/// centre above the actor and radius, for the three forms `PlayerFormSensorCollisionArranger::
/// syncForm` (0x443b54) switches between in 3D (`Body`/`Head`, `SquatBody`/`SquatHead`) and the
/// 2D model's pair (`2DBody`/`2DHead`, `2DSquatBody`/`2DSquatHead`).
const STANDING: [(f32, f32); 2] = [(75., 50.), (110., 40.)];
const SQUATTING: [(f32, f32); 2] = [(40., 40.), (50., 30.)];
const STANDING_2D: [(f32, f32); 2] = [(60., 60.), (140., 60.)];
const SQUATTING_2D: [(f32, f32); 2] = [(55., 55.), (75., 55.)];

impl Player {
    /// The sensors the actor has valid in this frame. `syncSensorAndCollision` (0x41d858) selects
    /// the squat form for the squat, slope and rolling nerves (rolling only outside its jump
    /// sub-state, `PlayerStateRolling::isRolling` 0x47aeac) and for the 2D `JumpSquat` clip
    /// (`PlayerStateJump::isFormSquat2D` 0x472504). The wall, ceiling and pole forms and the swim
    /// and hack ones have sensors the port does not place, so their callers never get here.
    pub(super) fn player_sensors(&self) -> [PlayerSensor; 2] {
        let actor = self.actor_name();
        let squat = matches!(actor, "Squat" | "Slope")
            || actor == "Rolling" && self.mode != Move::RollJump
            || actor == "Jump" && self.model_2d() && self.clip() == "JumpSquat";
        let table = match (self.model_2d(), squat) {
            (false, false) => STANDING,
            (false, true) => SQUATTING,
            (true, false) => STANDING_2D,
            (true, true) => SQUATTING_2D,
        };
        // The sensors follow the actor's pose (`al::HitSensor` offsets are in its matrix).
        let up = self.actor_pose().up();
        let base = self.pos / UNIT;
        table.map(|(height, radius)| PlayerSensor {
            center: base + up * height,
            radius,
        })
    }

    /// The player's own conditions for sending `PlayerCapTouchJump` (attackSensor 0x427f6c..
    /// 0x428034): the shared "cannot act" test 0x429c68 (dead, bound, damage, abyss, demo and
    /// hack states), no slope slide, no air spin, and a collider that can stand up when the player
    /// stands on the ground (`PlayerColliderHakoniwa::isEnableStandUp`, the virtual at slot 1).
    /// The swimmer's own condition (`PlayerStateSwim::isEnableCapTouchJump` 0x485734) and the wall,
    /// ceiling and pole forms are not ported: those states keep to the foot sensor's trample.
    pub(super) fn touch_jump_possible(&self) -> bool {
        if self.is_dead() || self.bound_to.is_some() {
            return false;
        }
        let actor = self.actor_name();
        if matches!(
            actor,
            "Damage"
                | "DamageSwim"
                | "Abyss"
                | "Dead"
                | "Slope"
                | "Swim"
                | "WallCatch"
                | "GrabCeil"
                | "PoleClimb"
        ) {
            return false;
        }
        if self.is_spin_attack_air() {
            return false;
        }
        !self.is_collided_ground() || self.ceiling.enable_stand_up
    }

    /// `PlayerStateSpinCap::isSpinAttackAir` (0x481e40): not killed, nerve SpinAir
    /// (table 0x1d7ff70 +0x20) or SpinAirSeparate (+0x18), and isLessEqualStep(22).
    /// The port implements SpinAir as Throw/in_air; separate-player nerves are unported.
    /// A pending first-step clip has not appeared yet (appear clears the killed byte at
    /// 0x481980). `frame` is the state's step, independently of the actor's `nerve.step`.
    fn is_spin_attack_air(&self) -> bool {
        self.mode == Move::Throw
            && self.throwing.in_air
            && self.nerve.pending_clip().is_none()
            && self.frame <= 22
    }

    /// The hit sensors' answer to the hovering cap's `Catch` sphere in this frame, with the
    /// player where the last frame left them. Accepted, the player jumps off the cap.
    pub(super) fn cap_touch_jump_phase(&mut self, o: &Orders) {
        if !self.touch_jump_possible() {
            return;
        }
        let sensors = self.player_sensors();
        let gate = TouchJumpGate {
            hold: o.throw_held,
            jump_possible: self.cap_jump_possible,
            wall_jump_stored: self.wall_history.jump.is_some(),
        };
        let Some(cap) = &mut self.cap else {
            return;
        };
        if !cap.take_touch_jump(&sensors, gate) {
            return;
        }
        // attackSensor 0x428038..0x428084: the cap-jump permission is spent unless the player
        // stands on ground flatter than `StandAngleMin`.
        if !self.is_on_ground_less_angle(t::STAND_ANGLE_MIN) {
            self.cap_jump_possible = false;
        }
        if self.is_collided_ground() {
            self.leap_frog_jump();
        } else {
            self.trample_jump();
        }
        // The nerve was named before this frame's update, which runs its
        // first step now: no frame passes in the chosen state.
        self.nerve.deferred = false;
        self.nerve.renewed = false;
        self.nerve.step = 0;
        self.nerve.actor = "Jump";
        self.sensor_restart = true;
    }

    /// Jump request 8 (`PlayerStateJump::appear` 0x471458, chosen at 0x4280d8 when the collider
    /// touches the ground): `CapLeapFrogJumpPowerAir` with `CapLeapFrogJumpGravity`, the
    /// `JumpCapLeapFrog` clip, no extension, and the velocity dropped when the stick is not
    /// pushed. The air control starts from the velocity left.
    fn leap_frog_jump(&mut self) {
        // `appear` decides the combo flag (+0xb4) for every request but 1, 13 and 16, before
        // the request's own take-off (0x470e58..0x470fc4): request 8 takes the combo's path.
        self.pick_jump_kind();
        let pushing = self.is_move();
        self.jump_with(
            Move::CapBounce,
            "JumpCapLeapFrog",
            self.constants.cap_leap_frog_jump_power_air,
            self.constants.cap_leap_frog_jump_gravity,
            move |p| {
                if !pushing {
                    // `al::setVelocityZero` at 0x4714fc; the power is set by the first step.
                    p.set_h(Vec3::ZERO);
                }
                p.max_air_speed = p.speed_h().clamp(t::JUMP_MOVE_MIN, t::JUMP_MOVE_MAX);
                p.jump_extend = false;
            },
        );
    }
}
