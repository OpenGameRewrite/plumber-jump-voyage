//! The player's joint controllers (`PlayerJointControlKeeper`): what bends the skeleton after the
//! animation, in the order the model's world calculation gives each joint to them, and what the
//! states hand the keeper every frame to steer them.
//!
//! Registered in the original, in this order: the swim pose, the ground pose, the cap throw, the
//! head's look-at, the pole climb pose and the grab pose; the wall catch's hip spring comes
//! after them. The grab pose (the arms' reach, the shoulder turns) is not ported yet.
use super::cap_throw::CapThrow;
use super::ground_pose::{CenterInput, GroundInputs, GroundPose};
use super::hip_spring::{GrabDynamics, HipSpring};
use super::joint_math::{calc_rate01, lerp_value};
use super::look_at::LookAt;
use super::look_joint::{Mat34, translation};
use super::pole_pose::PolePose;
use super::pose_math::{along_vector_normal_h, try_normalize_or_zero};
use super::swim_pose::SwimPose;
use super::vector_lib::verticalize_vec;
use super::*;
use crate::skeleton_pose::{Joint, JointCalc, Matrices};

/// `PlayerConst::getIKBlendFrameRun`, `getIKBlendRateRunMin` and `getIKBlendRateRunMax`
/// (built-in defaults: Mario has no player-constant file).
const IK_BLEND_FRAMES_RUN: i32 = 60;
const IK_RATE_RUN_MIN: f32 = 0.85;
const IK_RATE_RUN_MAX: f32 = 0.95;
/// `PlayerStateWait::getInverseKinematicsRate`: the sleep-start nerve takes the rate away over
/// this many steps.
const SLEEP_START_FADE_STEPS: f32 = 120.;

/// What the run state tells the keeper (`PlayerStateRunHakoniwa`, fields `+0xa4..+0xac`): the
/// inverse kinematics rate of the run, which fades once the run has been at its top speed for a
/// second, and the turn tilt.
#[derive(Clone, Debug, Default)]
pub struct RunPose {
    /// Steps the run has been at the top speed, up to the blend length (`+0xa4`).
    at_top_speed: i32,
    ik: f32,
    turn_tilt: f32,
}

impl RunPose {
    /// `PlayerStateRunHakoniwa::appear`: the count starts over and the rate is whole until the
    /// first `exeRun` sets it (the frame that begins the state reads it before that).
    pub fn appear(&mut self) {
        self.at_top_speed = 0;
        self.ik = 1.;
    }

    /// One step of `PlayerStateRunHakoniwa::exeRun` (the run and the run after a turn): the
    /// speed the ground move control gave and the turn tilt it measures. (The count is reset
    /// by `appear` and by the brake, the turn and the pivot, never by this nerve itself.)
    pub fn run_step(&mut self, speed: f32, turn_tilt: f32) {
        let over = speed - t::NORMAL_MAX;
        // `al::isNearZeroOrGreater`.
        let reached = over >= 0. || -over < 0.001;
        let goal = if reached { IK_BLEND_FRAMES_RUN } else { 0 };
        self.at_top_speed = converge_int(self.at_top_speed, goal, 1);
        let blend = if IK_BLEND_FRAMES_RUN >= 1 {
            (self.at_top_speed as f32 / IK_BLEND_FRAMES_RUN as f32).clamp(0., 1.)
        } else {
            1.
        };
        let rate = calc_rate01(speed, t::NORMAL_MIN, t::NORMAL_MAX);
        let strongest = lerp_value(IK_RATE_RUN_MIN, IK_RATE_RUN_MAX, (1. - rate).clamp(0., 1.));
        self.ik = strongest * (1. - blend).clamp(0., 1.);
        self.turn_tilt = turn_tilt;
    }

    /// The first step of the brake and of the turn: no rate and no tilt until the run resumes.
    pub fn stopped(&mut self) {
        self.at_top_speed = 0;
        self.ik = 0.;
        self.turn_tilt = 0.;
    }

    /// A step of the pivot: no rate (and no tilt since its first step).
    pub fn pivoting(&mut self) {
        self.at_top_speed = 0;
        self.ik = 0.;
    }
}

fn converge_int(value: i32, target: i32, step: i32) -> i32 {
    if value < target {
        (value + step).min(target)
    } else {
        (value - step).max(target)
    }
}

/// The joint controllers' state, kept by the player.
#[derive(Clone, Debug, Default)]
pub struct JointState {
    /// The swim pose's angles (`PlayerJointParamSwim`): the swim state steers them while it
    /// lives.
    pub swim: SwimPose,
    pub ground: GroundPose,
    /// The spine's swing of the cap throw.
    pub cap: CapThrow,
    /// The pole climb pose and the angle block the pole state writes for it.
    pub pole: PolePose,
    /// The wall catch's hip spring and the block the wall catch state steers its rate with.
    pub grab: GrabDynamics,
    pub spring: HipSpring,
    pub run: RunPose,
    /// The centre dynamics' parameter the states write to during a frame
    /// (`PlayerJointParamCenterDynamics`): the keeper reads it and clears it.
    center: CenterInput,
    /// Frames of follow-up impulses the run still gives after a landing (`PlayerStateRunHakoniwa`
    /// `+0xbc`).
    run_landing: i32,
    /// What the frame began with: the actor was in the run state, and the collider had just
    /// found the ground (`rs::isJustLand`, which the states read while they decide).
    was_running: bool,
    /// The actor was in the standing (Wait) state when the frame began.
    was_waiting: bool,
    just_landed: bool,
    /// The actor pose's up axis when the frame began. A state that starts in the frame (the
    /// jump's `appear`) reads the pose before the run's end-of-step pose update.
    pub(super) frame_start_pose_up: Vec3,
    /// The velocity the actor has when a landing chooses the run state (see
    /// [`Player::velocity_when_landing_state_is_chosen`]); the state's `appear` reads it for its
    /// impulse, ahead of `PlayerActionGroundMoveControl::appear` laying it into the floor's plane.
    run_arrival_velocity: Option<Vec3>,
    wait_platform_power: Vec3,
    wait_platform_counter: i32,
}

/// The run's impulse is this share of the velocity (`appear` and `control` pass `0.3`).
const RUN_IMPULSE_SHARE: f32 = 0.3;
/// A landing into the run gives the impulse for this many frames (`+0xbc` starts at 3).
const RUN_LANDING_FRAMES: i32 = 3;

impl Player {
    /// `PlayerActorHakoniwa::movement`'s hand-over to the keeper: the numbers the states give
    /// the ground pose this frame, then `PlayerJointControlKeeper::update`.
    pub(super) fn update_joint_controls(&mut self) {
        // The swim state clears the angles when it starts and when it ends.
        if self.actor_name() != "Swim" {
            self.joints.swim.reset();
        }
        self.joints.cap.update();
        // The pole state's `kill` leaves the block clear; the controller follows it either way.
        if self.mode != Move::Pole {
            self.joints.pole.release();
        }
        if self.is_carry() && self.carry.phase < 3 {
            // Start/Carry copy the keeper's requested angles. Front's block stays zero;
            // the flower FrontWallKeep agreement requests the existing 30-degree bend.
            self.joints
                .pole
                .ask_carry_front(if self.carry.wall_keep { 30. } else { 0. });
        }
        self.joints.pole.update();
        // `PlayerStateWallCatch::kill` leaves the target at nothing; the spring follows it.
        if self.mode != Move::Ledge || self.catch.is_killed() {
            self.joints.grab.release();
        }
        let rate = self.joints.grab.update();
        self.joints.spring.set_control_rate(rate);
        let inputs = self.ground_pose_inputs();
        let center_rate =
            if !self.is_carry() && self.actor_name() == "Run" && self.run_nerve == RunNerve::Run {
                let blend = (1. - self.cycle.weights[4]).clamp(0., 1.);
                let max = self.constants.center_tilt_rate_max;
                joint_math::lerp_value(max.min(0.1), max, blend * blend)
            } else {
                0.
            };
        self.joints.ground.update_center_tilt(
            center_rate,
            self.units_per_frame(),
            self.actor_pose().side(),
        );
        let center = std::mem::take(&mut self.joints.center);
        self.joints.ground.update(inputs, &center);
    }

    /// `rs::startCenterJointControl(param, actor, collision, vector, front_free)`: an impulse of
    /// `vector` along the ground the player stands on. With `front_free` (the landing of the
    /// standing state) the part of the vector along the actor's front is dropped, so that a
    /// landing only sways the body sideways. On the frame of a landing the motion of a moving
    /// ground under the actor (`rs::calcMovePowerGround`, negated) is added; with `front_free`
    /// its part against the front is cancelled first.
    fn start_center_joint_control(&mut self, vector: Vec3, front_free: bool) {
        let up = self.ground_normal_or_up();
        let mut vector = vector;
        let mut ground_move = if self.is_collided_ground() {
            -self.ground_inertia.jump_force
        } else {
            Vec3::ZERO
        };
        if front_free {
            let (actor_up, front) = self.pose_axes();
            let (along, found) = try_normalize_or_zero(along_vector_normal_h(front, actor_up, up));
            if found {
                vector = verticalize_vec(along, vector);
                ground_move = swim_move::limit_vector_opposite_dir(
                    ground_move,
                    along,
                    vector_lib::length(ground_move),
                );
            }
        }
        if self.is_just_land() {
            vector += ground_move;
        }
        self.joints.center.impulse = true;
        self.joints.center.impulse_vector = verticalize_vec(up, vector);
    }

    /// `PlayerStateWait::appear` after a landing (not the stiff one, not the triple jump's):
    /// the velocity the player came down with, carried along the floor, sways the body.
    pub(super) fn wait_state_appears_after_landing(&mut self, landing_velocity: Vec3) {
        let up = self.ground_normal_or_up();
        let along_floor = along_vector_normal_h(landing_velocity, Vec3::Y, up);
        self.start_center_joint_control(along_floor, true);
    }

    /// `PlayerStateWait::appear` after the roll's stand-up (`EActionTrigger 0x19`, which
    /// `PlayerActorHakoniwa::exeRolling` sets when the stand-up ends): the velocity the
    /// roll still has sways the body strongly, along the ground pose's own up.
    pub(super) fn wait_state_appears_after_roll(&mut self, velocity: Vec3) {
        self.start_center_joint_control(velocity, false);
        let up = self.ground_normal_or_up();
        let (actor_up, _) = self.pose_axes();
        let pose_up = self.joints.ground.ground_pose_up(actor_up);
        let center = &mut self.joints.center;
        center.impulse_vector = along_vector_normal_h(center.impulse_vector, up, pose_up);
        center.strong = true;
    }

    /// The start of a frame.
    pub(super) fn note_frame_start_for_joints(&mut self) {
        self.joints.was_running = self.actor_name() == "Run";
        self.joints.was_waiting = self.actor_name() == "Wait";
        self.joints.frame_start_pose_up = self.pose_axes().0;
        self.joints.just_landed = self.is_just_land();
        self.joints.run_arrival_velocity = None;
        // The run state's `control` comes with the frame, before the actor's nerve runs: the
        // frame that begins the state (`appear`) does not call it. (Recordings of a landing
        // into a run agree: the impulse of `appear` reaches the sway as an impulse and the
        // follow-ups start with the next frame.)
        if self.joints.was_running {
            self.run_state_control();
        }
    }

    /// `PlayerStateWait::control` for a platform that moves under the standing player: the
    /// power is `rs::calcMovePowerGround` at the actor (zero without ground contact), the same
    /// sample the jump inertia takes before the frame's movement.
    pub(super) fn wait_state_platform_control(&mut self) {
        self.wait_platform_control(self.wait_platform_power());
    }

    /// `PlayerStateWait::appear` records the platform power it starts with (0x48cdb0) and clears
    /// the follow-up counter, so that standing up on a moving platform raises no impulse.
    pub(super) fn wait_state_appears_on_platform(&mut self) {
        self.joints.wait_platform_power =
            self.local_wait_platform_power(self.wait_platform_power());
        self.joints.wait_platform_counter = 0;
    }

    /// `rs::calcMovePowerGround` at the actor, zero without ground contact.
    fn wait_platform_power(&self) -> Vec3 {
        if self.is_collided_ground() {
            self.ground_inertia.jump_force
        } else {
            Vec3::ZERO
        }
    }

    /// The power in the actor's own axes without its part along gravity.
    fn local_wait_platform_power(&self, power: Vec3) -> Vec3 {
        let inverse = self.actor_pose().calc_quat().inverse();
        verticalize_vec(Vec3::NEG_Y, pose_math::rotate_by_quat(power, inverse))
    }

    /// PlayerStateWait::control (0x48cfc0): a sudden change in the ground's motion
    /// adds a bounded follow-up impulse to the centre dynamics.
    pub(super) fn wait_platform_control(&mut self, power: Vec3) {
        let local = self.local_wait_platform_power(power);
        let change = local - self.joints.wait_platform_power;
        self.joints.wait_platform_power = local;
        if vector_lib::length(change) < 3. {
            self.joints.wait_platform_counter = (self.joints.wait_platform_counter - 1).max(0);
            return;
        }
        let rate = joint_math::ease_out(joint_math::calc_rate01(
            self.joints.wait_platform_counter as f32,
            0.,
            10.,
        ));
        let share = joint_math::lerp_value(0.5, 0.01, rate);
        let impulse = pose_math::rotate_by_quat(change * -share, self.actor_pose().calc_quat());
        let length = vector_lib::length(impulse);
        self.joints.center.follow_up = true;
        self.joints.center.follow_up_vector = if length > 4. {
            impulse * (4. / length)
        } else {
            impulse
        };
        self.joints.wait_platform_counter = 10;
    }

    /// The end of a frame, for the run state: the actor choosing it calls its `appear` in that
    /// frame.
    pub(super) fn run_state_frame(&mut self) {
        if self.actor_name() == "Run" && !self.joints.was_running {
            self.run_state_appear();
        }
        self.wait_state_kill();
    }

    /// `PlayerStateWait::kill` (0x48cf50) writes byte 2 of the centre dynamics' parameter
    /// (`strb 1, [param + 2]`, 0x48cf68) as the actor leaves the standing state. The keeper
    /// reads it as the quick fade (`updateCenterBalance` 0x454538..0x454548: the blend then
    /// converges by 0.2 instead of the slow step), so a sway the standing state still
    /// carries ends in the frame the next state starts. An impulse of the same frame still
    /// wins (`updateCenterBalance` tests it first, 0x454384).
    /// [V] bones21 probe, `force-waterfall-airpass` row 0: the actor leaves Wait for Run,
    /// `Run::appear` raises no impulse (`rs::isJustLand` false, collider count 26), the keeper
    /// gets only byte 2 and the sway (blend 0.2) ends at zero in that frame.
    fn wait_state_kill(&mut self) {
        if self.joints.was_waiting && self.actor_name() != "Wait" {
            self.joints.center.fade_quickly = true;
        }
    }

    /// The velocity the actor has when the state a landing chooses starts (`getVelocity` in the
    /// states' `appear`). The collider does not clip the actor's velocity (the trace keeps
    /// reporting the fall speed of the touchdown), and the jump state's last step, which `land`
    /// replays for the air control, still applies its gravity before the landing state is
    /// chosen.
    pub(super) fn velocity_when_landing_state_is_chosen(
        &self,
        flown: Vec3,
        air_step_ran: bool,
    ) -> Vec3 {
        if self.touchdown_speed >= 0. {
            return flown;
        }
        let mut fall = self.touchdown_speed;
        if air_step_ran {
            fall = (fall - self.gravity).max(-t::FALL_MAX);
        }
        flown.with_y(fall)
    }

    /// The run state is chosen by a landing with this velocity.
    pub(super) fn note_velocity_at_run_choice(&mut self, velocity: Vec3) {
        self.joints.run_arrival_velocity = Some(velocity);
    }

    /// `PlayerStateRunHakoniwa::appear`: landing into a run gives the body its sway; its impulse
    /// reads the velocity before the ground move control's own `appear` lays it into the plane.
    fn run_state_appear(&mut self) {
        self.joints.run.appear();
        self.joints.run_landing = 0;
        let arrival = self.joints.run_arrival_velocity.take().unwrap_or(self.v);
        if self.joints.just_landed {
            self.joints.run_landing = RUN_LANDING_FRAMES;
            let vector = arrival * RUN_IMPULSE_SHARE;
            self.start_center_joint_control(vector, false);
        }
    }

    /// `PlayerStateRunHakoniwa::control`: for a few frames after the landing the run keeps
    /// pushing the sway, as a follow-up of the first impulse; leaving the run's own nerve ends
    /// it with a quick fade.
    fn run_state_control(&mut self) {
        if self.joints.run_landing < 1 {
            return;
        }
        if self.run_nerve == RunNerve::Run {
            self.joints.run_landing -= 1;
            let vector = self.v * RUN_IMPULSE_SHARE;
            self.start_center_joint_control(vector, false);
            let center = &mut self.joints.center;
            center.impulse = false;
            center.follow_up = true;
            center.follow_up_vector = center.impulse_vector;
            center.impulse_vector = Vec3::ZERO;
        } else {
            self.joints.center.fade_quickly = true;
            self.joints.run_landing = 0;
        }
    }

    /// The inverse kinematics rates and the turn tilt of the state the actor is in (the nerve
    /// chain at `0x41e56c..0x41f8f0` of `movement`, and the states' getters).
    fn ground_pose_inputs(&self) -> GroundInputs {
        let label = self.nerve_name().name;
        let (actor, sub) = label.split_once('/').unwrap_or((label, ""));
        let (body_ik, leg_ik, tilt) = match (actor, sub) {
            ("Wait", _) => {
                let ik = self.wait_inverse_kinematics_rate(sub);
                (ik, ik, 0.)
            }
            ("Run", "Run3D") => (
                self.joints.run.ik,
                0.,
                if self.is_carry() {
                    0.
                } else {
                    self.joints.run.turn_tilt
                },
            ),
            ("Rolling", "StandUp") => (1., 0., 0.),
            ("Squat", _) | ("HipDrop", "Land") => (1., 1., 0.),
            // `PlayerStateSpinCap`'s flag, set near the end of the ground spin's clip.
            ("SpinCap", _) => {
                let rate = if self.spin_ik { 1. } else { 0. };
                (rate, rate, 0.)
            }
            // The movement block gives the wall catch's body and legs the whole rate while
            // `PlayerStateWallCatch::isEnableIK` holds.
            ("WallCatch", _) => {
                let rate = if self.catch.is_enable_ik() { 1. } else { 0. };
                (rate, rate, 0.)
            }
            _ => (0., 0., 0.),
        };
        GroundInputs {
            body_ik,
            leg_ik,
            tilt,
            stiff_landing: actor == "Wait" && sub == "Land" && self.anim.name == "LandJump3",
        }
    }

    /// The helper the swimming nerves (the move and the paddle) call after they turn the body:
    /// the swim pose follows the speed along the front and the turn.
    pub(super) fn update_swim_pose(&mut self) {
        let (_, front) = self.pose_axes();
        // (The up axis the turn is measured against is the gravity's, not the pose's.)
        let power = self.swim.turn.power_signed(Vec3::Y);
        self.joints.swim.update(front, self.v, power);
    }

    /// `PlayerStateWait::getInverseKinematicsRate`: whole while standing, gone while asleep, and
    /// fading over the sleep start.
    fn wait_inverse_kinematics_rate(&self, sub: &str) -> f32 {
        match sub {
            "WaitSleep" => 0.,
            "WaitSleepStart" => {
                let steps = self.wait.steps as f32;
                1. - (steps / SLEEP_START_FADE_STEPS).clamp(0., 1.)
            }
            _ => 1.,
        }
    }
}

/// The controllers bending one pose: borrowed from the player for the calculation of one frame.
pub struct Controls<'a> {
    pub swim: &'a SwimPose,
    pub ground: &'a mut GroundPose,
    pub look: &'a mut LookAt,
    pub cap: &'a CapThrow,
    pub pole: &'a PolePose,
    pub spring: &'a mut HipSpring,
    /// The actor's pose matrix.
    pub actor: Mat34,
}

impl JointCalc for Controls<'_> {
    fn calc(&mut self, joint: Joint, world: &mut Mat34, solved: &mut Matrices) {
        // (The keeper registers the swim pose first, then the ground pose.)
        self.swim.calc(joint, world);
        self.ground.calc(joint, world, solved);
        self.cap.calc(joint, world);
        if joint == Joint::Head {
            // The keeper reads this frame's head position at the next step.
            self.look.set_head_position(translation(world));
            self.look.turn_head(world, &self.actor);
        }
        self.pole.calc(joint, world);
        self.spring.calc(joint, world);
    }
}

pub(super) fn self_check() {
    lockstep_rechoice_keeps_the_completed_joint_update();
    platform_power_sways_the_body_only_when_it_changes();
    landing_on_moving_ground_adds_its_motion_to_the_impulse();
    leaving_the_standing_state_asks_the_keeper_for_the_quick_fade();
}

#[cfg_attr(test, test)]
fn lockstep_rechoice_keeps_the_completed_joint_update() {
    // GroundPose::update follows IK once per actor update (0x4541f8..0x454268).
    // HipDrop/Land -> Jump0 already lowered the completed row's leg rate.
    let scene = crate::replay::flat_floor();
    let mut player = standing_on_ground();
    player.joints.ground = GroundPose::settled_wait();
    let snapshot = Snapshot {
        mode: Some(Move::PoundLand),
        grounded: true,
        pos: player.pos,
        state_frames: Some(33),
        sub_steps: Some(4),
        anim: Some(("HipDropLand", 3.)),
        ..Default::default()
    };
    player.inject(&snapshot);
    let orders = Orders {
        jump: true,
        jump_held: true,
        ..Default::default()
    };
    player.step(&orders, &scene);
    assert_eq!(player.mode, Move::PoundJump);
    let completed = player.joints.ground.state();
    player.inject(&snapshot);
    player.repeat_step_for_lockstep(&orders, &scene);
    assert_eq!(player.joints.ground.state(), completed);
}

fn standing_on_ground() -> Player {
    let mut player = Player::default();
    player.collider.floor = Some(crate::collider::Contact {
        normal: Vec3::Y,
        position: Vec3::ZERO,
        triangle: 0,
        location: crate::collider::Location::Face,
    });
    player
}

#[cfg_attr(test, test)]
fn platform_power_sways_the_body_only_when_it_changes() {
    let mut player = standing_on_ground();
    player.ground_inertia.jump_force = Vec3::new(0., 0., 10.);
    // `PlayerStateWait::appear` takes the platform's power as the starting point.
    player.wait_state_appears_on_platform();
    player.wait_state_platform_control();
    assert!(!player.joints.center.follow_up);
    // A change of at least 3 gives the follow-up impulse, bounded to 4.
    player.ground_inertia.jump_force = Vec3::new(0., 0., 20.);
    player.wait_state_platform_control();
    assert!(player.joints.center.follow_up);
    assert!((player.joints.center.follow_up_vector - Vec3::new(0., 0., -4.)).length() < 1e-5);
}

#[cfg_attr(test, test)]
fn landing_on_moving_ground_adds_its_motion_to_the_impulse() {
    let mut player = standing_on_ground();
    player.ground_contact = 1;
    player.ground_inertia.jump_force = Vec3::new(0., 0., 10.);
    player.start_center_joint_control(Vec3::ZERO, false);
    assert_eq!(player.joints.center.impulse_vector, Vec3::new(0., 0., -10.));
    // Without the landing frame the ground's motion is not added.
    player.ground_contact = 2;
    player.start_center_joint_control(Vec3::ZERO, false);
    assert_eq!(player.joints.center.impulse_vector, Vec3::ZERO);
}

#[cfg_attr(test, test)]
fn leaving_the_standing_state_asks_the_keeper_for_the_quick_fade() {
    let mut player = standing_on_ground();
    player.go(Move::Idle, "Wait");
    assert_eq!(player.actor_name(), "Wait");
    player.note_frame_start_for_joints();
    // Still in the standing state at the end of the frame: no request.
    player.run_state_frame();
    assert!(!player.joints.center.fade_quickly);
    // `PlayerStateWait::kill` (0x48cf50) writes byte 2 when the actor leaves it.
    player.note_frame_start_for_joints();
    player.go(Move::Jump, "Jump");
    player.run_state_frame();
    assert!(player.joints.center.fade_quickly);
    // The request is a one-frame flag: the keeper reads it and clears it.
    player.joints.center = CenterInput::default();
    player.note_frame_start_for_joints();
    player.run_state_frame();
    assert!(!player.joints.center.fade_quickly);
}
