//! CapSwitchSave, the switch a thrown cap locks onto (SMO 1.0.0 `CapSwitch.cpp`).
//!
//! The switch answers the cap's lock-on protocol without capturing anything (`InitHackCap.byml`:
//! `LockOnOnly`, joint `HatTarget`):
//! * `CapStartLockOn` is accepted in OffWait and `InitCapTarget` enters OffWaitCapHold, which keeps
//!   the cap (`CapKeepLockOn`, `CapIgnoreCancelLockOn`); every nerve answers `CapCancelLockOn`;
//! * OffWaitCapHold plays `HitReaction`, then `HitReactionKeepFront` when that clip has ended, and
//!   after 30 steps asks the cap's hit reaction and enters HitReaction, which stops keeping the cap;
//! * HitReaction starts its reaction `起動` (`ActorHitReactionCtrl.byml`: it stops the scene for
//!   `StopSceneFrame` ticks) and plays `HitReactionFront`; at its end the switch turns on (the
//!   stage switch the linked door listens to, the save) and waits in OnWait.
//!
//! The cap's target matrix is the `HatTarget` joint of the switch's own model, so the target moves
//! with the clip the actor plays. The exporter embeds the model's curve file (`PJVCURV1`) and the
//! replay evaluates the joint chain from it. The linked door and its camera demo are not simulated:
//! Mario's own clock does not run through the demo.
use super::*;
use crate::clip_curves::Curves;
use crate::model_2d::compose;
use crate::skeleton_pose::IDENTITY;
use std::sync::Arc;

/// `exeOffWaitCapHold` asks for the hit reaction at step 30 (`CapSwitch.cpp:146`).
const HOLD_STEPS: i32 = 30;
/// The follow matrix's local rotation about Z while the cap is held (`CapSwitch.cpp:139`).
const FOLLOW_ROLL_DEGREES: f32 = 270.;
/// `al::isNearZero`'s default `0.001f` tolerance (`MathUtil.h`); the source tests the normalized
/// HatTarget-to-player vector, so this is only a zero-direction check, not a distance threshold.
const NEAR_ZERO: f32 = 0.001;
/// `al::HitSensorType::MapObj`, the type `InitSensor.byml` gives the Body sensor.
const MAP_OBJ_SENSOR: u32 = 12;
const HIT_REACTION: &str = "HitReaction";
const KEEP_CLIP: &str = "HitReactionKeepFront";
const FRONT_CLIP: &str = "HitReactionFront";
const OFF_WAIT_CLIP: &str = "OffWait";
const ON_WAIT_CLIP: &str = "OnWaitFront";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SwitchNerve {
    OffWait,
    OffWaitCapHold,
    HitReaction,
    OnWait,
}

impl SwitchNerve {
    fn name(self) -> &'static str {
        match self {
            Self::OffWait => "OffWait",
            Self::OffWaitCapHold => "OffWaitCapHold",
            Self::HitReaction => "HitReaction",
            Self::OnWait => "OnWait",
        }
    }
}

/// The model clip the actor plays: `al::startAction` restarts it, and the first update after the
/// start does not advance it (the frame a start shows is the frame the next sample reads).
#[derive(Clone, Debug)]
struct Action {
    clip: &'static str,
    frame: f32,
    fresh: bool,
}

#[derive(Clone)]
pub struct CapSwitch {
    pub id: String,
    /// The name the game logs for the actor (`UnitConfig.DisplayName`).
    host: String,
    /// Index of the Body sensor among the stage's look sensors, set by the stage loader.
    pub look_sensor: Option<usize>,
    /// The placement's translation, and the same position relative to the replay's start (set by
    /// the stage loader).
    pub position: Vec3,
    pub relative: Vec3,
    /// `InitSensor.byml` Body: radius and offset above the actor.
    pub body_radius: f32,
    pub body_offset: f32,
    /// `StopSceneFrame` of the `起動` hit reaction.
    stop_frames: u32,
    curves: Arc<Curves>,
    joint: usize,
    nerve: SwitchNerve,
    step: i32,
    action: Action,
    /// `CapTargetInfo`'s local rotation (degrees): `InitHackCap.byml`'s until the hold sets it.
    follow_rotate: Vec3,
    /// Mario's position when the hold started (`mPlayerPos`).
    player: Vec3,
    /// The exported INITIAL pending OffWait has not executed at the first sample.
    initial: bool,
}

impl std::fmt::Debug for CapSwitch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapSwitch")
            .field("id", &self.id)
            .field("nerve", &self.nerve)
            .field("step", &self.step)
            .finish()
    }
}

impl CapSwitch {
    pub fn load(source: &serde_json::Value) -> Result<Self, String> {
        let p = &source["placement"];
        let sensors = source["sensors"].as_array().ok_or("switch sensors")?;
        let link_names: Vec<_> = p["Links"]
            .as_object()
            .map_or(vec![], |l| l.keys().collect());
        let zero = serde_json::json!({"X": 0.0, "Y": 0.0, "Z": 0.0});
        let one = serde_json::json!({"X": 1.0, "Y": 1.0, "Z": 1.0});
        // The pose is translation only: the model's joint chain is evaluated under that matrix.
        if p["UnitConfigName"] != "CapSwitchSave"
            || p["IsNoReaction"] != false
            || p["IsLinkDest"] != false
            || p["IsConnectToCollision"] != false
            || link_names != ["RiseParts"]
            || p["Rotate"] != zero
            || p["Scale"] != one
            || source["initial_phase"] != "WaitPending"
            || source["hack_cap"]["JointName"] != "HatTarget"
            || source["hack_cap"]["LockOnOnly"] != true
            || sensors.len() != 2
            || sensors[0]["Name"] != "Body"
            || sensors[1]["Name"] != "Collision"
        {
            return Err("unsupported cap switch placement".into());
        }
        let reaction = source["hit_reactions"]
            .as_array()
            .and_then(|all| all.iter().find(|r| r["ReactionName"] == "起動"))
            .filter(|r| r["IsStopSceneForHitEffect"] == true)
            .ok_or("switch hit reaction")?;
        let hex = source["curves"].as_str().ok_or("switch curves")?;
        let bytes = (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "switch curves")?;
        let curves = Curves::parse(&bytes).ok_or("switch curves")?;
        let joint = curves.joint("HatTarget").ok_or("switch joint")?;
        for clip in [
            HIT_REACTION,
            KEEP_CLIP,
            FRONT_CLIP,
            OFF_WAIT_CLIP,
            ON_WAIT_CLIP,
        ] {
            curves.clip(clip).ok_or("switch clip")?;
        }
        let component = |axis: &str| {
            p["Translate"][axis]
                .as_f64()
                .map(|v| v as f32)
                .ok_or("switch position")
        };
        Ok(Self {
            id: p["Id"].as_str().ok_or("switch id")?.into(),
            host: p["UnitConfig"]["DisplayName"]
                .as_str()
                .ok_or("switch host")?
                .into(),
            look_sensor: None,
            position: Vec3::new(component("X")?, component("Y")?, component("Z")?),
            relative: Vec3::ZERO,
            body_radius: sensors[0]["Radius"].as_f64().ok_or("switch radius")? as f32,
            body_offset: sensors[0]["Y"].as_f64().ok_or("switch offset")? as f32,
            stop_frames: reaction["StopSceneFrame"].as_u64().ok_or("switch stop")? as u32,
            curves: Arc::new(curves),
            joint,
            nerve: SwitchNerve::OffWait,
            step: 0,
            follow_rotate: {
                let rotate = &source["hack_cap"]["LocalRotate"];
                let degrees = |axis: &str| rotate[axis].as_f64().map(|v| v as f32);
                Vec3::new(
                    degrees("X").ok_or("switch rotate")?,
                    degrees("Y").ok_or("switch rotate")?,
                    degrees("Z").ok_or("switch rotate")?,
                )
            },
            player: Vec3::ZERO,
            action: Action {
                clip: OFF_WAIT_CLIP,
                frame: 0.,
                fresh: false,
            },
            initial: true,
        })
    }

    /// The Body sensor as a candidate of the player's head look-at (`InitSensor.byml`: a `MapObj`
    /// sensor above the actor), in the replay's frame.
    pub fn body_look_sensor(&self, stage: Vec3, relative: Vec3) -> look_at::LookSensor {
        look_at::LookSensor {
            host: self.host.clone(),
            name: "Body".into(),
            kind: MAP_OBJ_SENSOR,
            position: relative,
            stage_position: stage,
            radius: self.body_radius,
        }
    }

    /// The Body sensor's stage position: the placement raised by the sensor's offset.
    pub fn body_stage_position(&self) -> Vec3 {
        self.position + Vec3::Y * self.body_offset
    }

    /// `exeOnWait` invalidates the actor's hit sensors on its first execution
    /// (`al::invalidateHitSensors`); the switch never turns off again here.
    pub fn sensors_valid(&self) -> bool {
        !(self.nerve == SwitchNerve::OnWait && self.step > 0)
    }

    /// `receiveMsg` CapStartLockOn: accepted in OffWait.
    pub fn waiting(&self) -> bool {
        self.nerve == SwitchNerve::OffWait
    }

    /// `receiveMsg` CapKeepLockOn: accepted in OffWaitCapHold only.
    pub fn keeps(&self) -> bool {
        self.nerve == SwitchNerve::OffWaitCapHold
    }

    /// The accepted `InitCapTarget` sets OffWaitCapHold; it executes in the same frame.
    pub fn start(&mut self) {
        self.set_nerve(SwitchNerve::OffWaitCapHold);
    }

    /// The matrix the cap follows: the `HatTarget` joint of the clip pose the previous update left,
    /// relative to the replay's start (centimetres).
    pub fn target(&self) -> Vec3 {
        let m = self.joint_world();
        self.relative + Vec3::new(m[0][3], m[1][3], m[2][3])
    }

    /// The rotation of the target matrix: the joint's, turned by the local rotation (degrees,
    /// about X, then Y, then Z).
    pub fn target_rotation(&self) -> Quat {
        let m = self.joint_world();
        let joint = Mat3::from_cols(
            Vec3::new(m[0][0], m[1][0], m[2][0]),
            Vec3::new(m[0][1], m[1][1], m[2][1]),
            Vec3::new(m[0][2], m[1][2], m[2][2]),
        );
        let local = Quat::from_euler(
            EulerRot::ZYX,
            self.follow_rotate.z.to_radians(),
            self.follow_rotate.y.to_radians(),
            self.follow_rotate.x.to_radians(),
        );
        Quat::from_mat3(&joint).normalize() * local
    }

    /// The `HatTarget` joint's world matrix in the clip pose the model was last updated to.
    fn joint_world(&self) -> look_joint::Mat34 {
        compose(&self.curves, &IDENTITY, self.action.clip, self.action.frame)
            .and_then(|joints| joints.get(self.joint).map(|(_, world)| *world))
            .unwrap_or(IDENTITY)
    }

    /// One execution of the actor's nerve and its model update. Returns the number of ticks the
    /// scene stops for when this execution starts the activation reaction.
    pub fn tick(&mut self, player: Vec3) -> Option<u32> {
        if std::mem::take(&mut self.initial) {
            return None;
        }
        let mut stop = None;
        match self.nerve {
            SwitchNerve::OffWait => {
                if self.step == 0 {
                    self.start_action(OFF_WAIT_CLIP);
                }
                self.step += 1;
            }
            SwitchNerve::OffWaitCapHold => {
                // The joint is read from the pose the model was last updated to: a clip started
                // in this execution has not posed it yet.
                let hit = self.target();
                if self.step == 0 {
                    self.start_action(HIT_REACTION);
                    self.player = player;
                }
                self.follow_rotate = self.hold_rotation(hit);
                if self.action.clip == HIT_REACTION && self.action_end() {
                    self.start_action(KEEP_CLIP);
                }
                if self.step >= HOLD_STEPS {
                    self.set_nerve(SwitchNerve::HitReaction);
                } else {
                    self.step += 1;
                }
            }
            SwitchNerve::HitReaction => {
                if self.step == 0 {
                    stop = Some(self.stop_frames);
                    self.start_action(FRONT_CLIP);
                }
                if self.action_end() {
                    self.set_nerve(SwitchNerve::OnWait);
                } else {
                    self.step += 1;
                }
            }
            SwitchNerve::OnWait => {
                if self.step == 0 {
                    self.start_action(ON_WAIT_CLIP);
                }
                self.step += 1;
            }
        }
        if !std::mem::take(&mut self.action.fresh) {
            self.action.frame += 1.;
        }
        stop
    }

    /// The local rotation `exeOffWaitCapHold` sets each execution: the angle between the actor's
    /// front and the way from Mario to the joint, turned about Y to the side Mario is on.
    fn hold_rotation(&self, hit: Vec3) -> Vec3 {
        let way = hit - self.player;
        let angle =
            if crate::movement::vector_lib::is_near_zero(pose_math::normalize(way), NEAR_ZERO) {
                0.
            } else {
                Vec3::Z
                    .dot(way.normalize())
                    .clamp(-1., 1.)
                    .acos()
                    .to_degrees()
            };
        // `al::isLeftTarget`: the actor's side axis (X) against the way from the actor to Mario.
        let left = Vec3::X.dot(self.player - self.relative) > 0.;
        Vec3::new(0., if left { -angle } else { angle }, FOLLOW_ROLL_DEGREES)
    }

    /// `al::isActionEnd`: the clip's next update reaches its last frame (measured on the recorded
    /// ends of HitReaction, 16 frames, and HitReactionFront, 25: the exit comes on the execution
    /// that reads frame count - 1).
    fn action_end(&self) -> bool {
        self.curves
            .clip(self.action.clip)
            .is_some_and(|clip| self.action.frame + 1. >= clip.frames())
    }

    fn start_action(&mut self, clip: &'static str) {
        self.action = Action {
            clip,
            frame: 0.,
            fresh: true,
        };
    }

    /// `al::setNerve` inside an execution: the new nerve shows with step 0 and first executes on
    /// the next frame.
    fn set_nerve(&mut self, nerve: SwitchNerve) {
        self.nerve = nerve;
        self.step = 0;
    }

    pub fn diagnostic(&self) -> String {
        format!("{}/{}/{}", self.id, self.nerve.name(), self.step)
    }
}
