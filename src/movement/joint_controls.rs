//! The player's joint controllers (`PlayerJointControlKeeper`): what bends the skeleton after the
//! animation, in the order the model's world calculation gives each joint to them, and what the
//! states hand the keeper every frame to steer them.
//!
//! Registered in the original, in this order: the swim pose, the ground pose, the cap throw, the
//! head's look-at, the pole climb pose and the grab pose. The cap throw, the pole climb and the
//! grab pose are not ported (the cap throw and the grab pose belong to states whose joint poses
//! this rewrite does not show yet).
use super::ground_pose::{CenterInput, GroundInputs, GroundPose};
use super::joint_math::{calc_rate01, lerp_value};
use super::look_at::LookAt;
use super::look_joint::{Mat34, translation};
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
    /// One step of `PlayerStateRunHakoniwa::exeRun` (the run and the run after a turn): the
    /// speed the ground move control gave and the turn tilt it measures.
    pub fn run_step(&mut self, first_step: bool, speed: f32, turn_tilt: f32) {
        if first_step {
            // Every other nerve of the state resets the count when it starts.
            self.at_top_speed = 0;
        }
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
    pub ground: GroundPose,
    pub run: RunPose,
    /// The centre dynamics' parameter the states write to during a frame
    /// (`PlayerJointParamCenterDynamics`): the keeper reads it and clears it.
    center: CenterInput,
    /// Frames of follow-up impulses the run still gives after a landing (`PlayerStateRunHakoniwa`
    /// `+0xbc`).
    run_landing: i32,
    run_active: bool,
    /// What the frame began with: the actor was in the run state, and the collider had just
    /// found the ground (`rs::isJustLand`, which the states read while they decide).
    was_running: bool,
    just_landed: bool,
}

/// The run's impulse is this share of the velocity (`appear` and `control` pass `0.3`).
const RUN_IMPULSE_SHARE: f32 = 0.3;
/// A landing into the run gives the impulse for this many frames (`+0xbc` starts at 3).
const RUN_LANDING_FRAMES: i32 = 3;

impl Player {
    /// `PlayerActorHakoniwa::movement`'s hand-over to the keeper: the numbers the states give
    /// the ground pose this frame, then `PlayerJointControlKeeper::update`.
    pub(super) fn update_joint_controls(&mut self) {
        let inputs = self.ground_pose_inputs();
        let center = std::mem::take(&mut self.joints.center);
        if std::env::var("PJV_DEBUG_IK").is_ok() {
            eprintln!("{} {:?} {:?} {:?} ground={:?} land={} active={} landing={} contact={} was={} jl={}", self.nerve_name().name, inputs, self.joints.run, self.joints.center, self.joints.ground, self.is_just_land(), self.joints.run_active, self.joints.run_landing, self.ground_contact, self.joints.was_running, self.joints.just_landed);
        }
        self.joints.ground.update(inputs, &center);
    }

    /// `rs::startCenterJointControl(param, actor, collision, vector, false)`: an impulse of
    /// `vector` along the ground the player stands on.
    fn start_center_joint_control(&mut self, vector: Vec3) {
        let up = if self.is_collided_ground() {
            self.collided_ground_normal()
        } else {
            Vec3::Y
        };
        // (Ground that moves under the player adds its motion on the landing frame; the
        // collision this rewrite replays is static.)
        self.joints.center.impulse = true;
        self.joints.center.impulse_vector = super::vector_lib::verticalize_vec(up, vector);
    }

    /// The start of a frame.
    pub(super) fn note_frame_start_for_joints(&mut self) {
        self.joints.was_running = self.actor_name() == "Run";
        self.joints.just_landed = self.is_just_land();
    }

    /// The end of a frame, for the run state: the actor choosing it calls its `appear` in that
    /// frame, and an active run state's `control` follows.
    pub(super) fn run_state_frame(&mut self) {
        let running = self.actor_name() == "Run";
        if running && !self.joints.was_running {
            self.run_state_appear();
        }
        self.joints.run_active = running;
        self.run_state_control();
    }

    /// `PlayerStateRunHakoniwa::appear`: landing into a run gives the body its sway.
    fn run_state_appear(&mut self) {
        self.joints.run_landing = 0;
        if self.joints.just_landed {
            self.joints.run_landing = RUN_LANDING_FRAMES;
            let vector = self.v * RUN_IMPULSE_SHARE;
            self.start_center_joint_control(vector);
        }
    }

    /// `PlayerStateRunHakoniwa::control`: for a few frames after the landing the run keeps
    /// pushing the sway, as a follow-up of the first impulse; leaving the run's own nerve ends
    /// it with a quick fade.
    pub(super) fn run_state_control(&mut self) {
        if !self.joints.run_active || self.joints.run_landing < 1 {
            return;
        }
        if self.run_nerve == RunNerve::Run {
            self.joints.run_landing -= 1;
            let vector = self.v * RUN_IMPULSE_SHARE;
            self.start_center_joint_control(vector);
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
            ("Run", "Run3D") => (self.joints.run.ik, 0., self.joints.run.turn_tilt),
            ("Rolling", "StandUp") => (1., 0., 0.),
            ("Squat", _) | ("HipDrop", "Land") => (1., 1., 0.),
            // (`PlayerStateWallCatch::isEnableIK` is on for the climb after its first 40 steps
            // and the fast one after 16; the rewrite does not tell the two apart. Guess: off.)
            _ => (0., 0., 0.),
        };
        GroundInputs {
            body_ik,
            leg_ik,
            tilt,
            stiff_landing: actor == "Wait" && sub == "Land" && self.anim.name == "LandJump3",
        }
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
    pub ground: &'a mut GroundPose,
    pub look: &'a mut LookAt,
    /// The actor's pose matrix.
    pub actor: Mat34,
}

impl JointCalc for Controls<'_> {
    fn calc(&mut self, joint: Joint, world: &mut Mat34, solved: &mut Matrices) {
        self.ground.calc(joint, world, solved);
        if joint == Joint::Head {
            // The keeper reads this frame's head position at the next step.
            self.look.set_head_position(translation(world));
            self.look.turn_head(world, &self.actor);
        }
    }
}
