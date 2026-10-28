//! The player's head look-at: which sensor the head turns toward and when it may.
//!
//! Three original pieces work together every frame:
//!
//! * `PlayerEyeSensorHitHolder`: Mario's `Eye` sensor (a sphere of 800 cm, 80 cm above the
//!   feet) overlaps the sensors of other actors; `PlayerActorHakoniwa::attackSensor` records
//!   the nearest of those that are look-at candidates (map objects, NPCs, enemies...) in the
//!   holder, and the holder keeps it until its host dies, its sensor turns invalid or it
//!   answers `PlayerDisregard`. There is no angle or sight test.
//! * `PlayerActorHakoniwa::movement` decides from the state whether the head may look at all
//!   (standing, running, a few swim states), and hands the holder's candidate to the
//!   `PlayerJointControlKeeper` (`updateLookAtTarget`), which takes it as its target when it
//!   is within 400 cm of the head joint and nearer than the target it has.
//! * The keeper asks the head's joint controller (`look_joint.rs`) to look at the target's
//!   position every frame; for the first 40 frames of a target the request is made whatever
//!   the distance, afterwards a target farther than 400 cm is dropped.
//!
//! What is ported from the executable: the holder's selection, the keeper's target choice and
//! counter, the enable rule per state, and the request made to the joint controller. What is
//! not: hosts that answer `PlayerLookAtPosition` (their reply replaces the sensor's position
//! and widens the gate by the sensor's radius), the fixed-direction modes of the bind and demo
//! states, the entrance camera, and the dead or disregarding hosts: the sensors that replays
//! know are static placements whose hosts answer neither query.
use super::look_joint::{HeadLookAt, Mat34, translation};
use super::vector_lib::converge;
use bevy::prelude::Vec3;

/// The eye sensor: reach (cm) and height over the feet (`PlayerActorHakoniwa/InitSensor`).
const EYE_RADIUS: f32 = 800.;
const EYE_HEIGHT: f32 = 80.;
/// A target within this distance of the head joint can be taken, and a target the keeper holds
/// for longer than `UNGATED_FRAMES` is dropped past it.
const GATE_DISTANCE: f32 = 400.;
/// The counter runs from -1 (a new target) to this many frames: while it is at most 39 the
/// request does not check the distance.
const UNGATED_COUNTER: i32 = 39;
const COUNTER_LIMIT: i32 = 40;
/// Where the head joint is when nothing has said (the standing head height of the recordings).
/// A guess for builds without the model: it only decides the 400 cm gate.
const GUESSED_HEAD_HEIGHT: f32 = 92.;
/// The head joint's index in the original's skeleton (the probe's `headjoint` field).
pub const ORIGINAL_HEAD_JOINT: i32 = 14;

/// A hit sensor another actor offers, in game units (cm) in the replay's frame.
#[derive(Clone, Debug, PartialEq)]
pub struct LookSensor {
    /// The host actor's name as the game logs it.
    pub host: String,
    /// The sensor's own name and its `al::HitSensorType` number.
    pub name: String,
    pub kind: u32,
    /// In the replay's frame (the start is the origin).
    pub position: Vec3,
    /// The same point in the stage's own coordinates, as the probe logs it.
    pub stage_position: Vec3,
    pub radius: f32,
}

impl LookSensor {
    /// `host/name/type`, as the probe prints a sensor.
    pub fn label(&self) -> String {
        format!("{}/{}/{}", self.host, self.name, self.kind)
    }
}

/// Whether the head may look in this state (`PlayerActorHakoniwa::movement` with the states'
/// `isEnableLookAt`), by the state's trace label.
pub fn look_enabled(label: &str) -> bool {
    let (actor, sub) = label.split_once('/').unwrap_or((label, ""));
    match actor {
        // Standing: everything but the landing, snoozing and sleeping sub-states.
        "Wait" => !matches!(
            sub,
            "Land"
                | "LandStiffen"
                | "WaitSnoozeStart"
                | "WaitSnooze"
                | "WaitSleepStart"
                | "WaitSleep"
        ),
        // The run state's own test, for its nerve, is `Player::run_nerve_blocks_look`: the
        // label is `Run/Run3D` through the turn and the brake alike.
        "Run" => sub == "Run3D",
        "Swim" => matches!(
            sub,
            "SwimSurface"
                | "SwimMove"
                | "SwimPaddle"
                | "SwimLand"
                | "SwimWalk"
                | "SwimTrample"
                | "SwimReflectDownFall"
        ),
        _ => false,
    }
}

fn squared_distance(a: Vec3, b: Vec3) -> f32 {
    let (dx, dy, dz) = (a.x - b.x, a.y - b.y, a.z - b.z);
    (dx * dx + dy * dy) + dz * dz
}

/// `rs::tryCalcLookAtTargetInDistance` for a host that does not answer `PlayerLookAtPosition`:
/// the sensor's position when it is within `GATE_DISTANCE` of `origin`.
fn within_gate(sensor: &LookSensor, origin: Vec3) -> Option<Vec3> {
    let limit = (0. + GATE_DISTANCE) * (0. + GATE_DISTANCE);
    (squared_distance(sensor.position, origin) <= limit).then_some(sensor.position)
}

/// The holder, keeper and joint controller of the player's look-at.
#[derive(Clone, Debug)]
pub struct LookAt {
    /// The holder's candidate (`PlayerEyeSensorHitHolder` +0x30) as an index into the sensors.
    candidate: Option<usize>,
    /// The keeper's target (`PlayerJointControlKeeper` +0x98) and its counter (+0xb8).
    target: Option<usize>,
    counter: i32,
    /// The keeper's mode (+0x94): looking at a sensor, or off.
    active: bool,
    /// The controller the head joint obeys.
    pub joint: HeadLookAt,
    /// Where the eye sensor was after the previous step: the overlap of a frame is decided with
    /// the sensor positions of the frame before.
    previous_eye: Option<Vec3>,
    invalid_sensors: Vec<usize>,
    /// The head joint's position after the previous model calculation (cm), which the keeper
    /// reads when it chooses.
    head: Option<Vec3>,
}

/// What a recorded row says of the look-at, for a lockstep replay that starts every frame from
/// the game's previous state: where the player and the head joint were, and the keeper's counter
/// and target (the point it was asked to look at, in the stage's coordinates).
#[derive(Clone, Copy, Debug, Default)]
pub struct LookHint {
    pub feet: Vec3,
    pub head: Option<Vec3>,
    pub counter: Option<i32>,
    pub target_stage: Option<Vec3>,
}

impl Default for LookAt {
    fn default() -> Self {
        Self {
            candidate: None,
            target: None,
            counter: -1,
            active: false,
            joint: HeadLookAt::default(),
            previous_eye: None,
            invalid_sensors: Vec::new(),
            head: None,
        }
    }
}

impl LookAt {
    pub fn invalidate_source_sensor(&mut self, index: usize) {
        self.invalid_sensors.push(index);
        if self.candidate == Some(index) {
            self.candidate = None;
        }
        if self.target == Some(index) {
            self.forget_target();
        }
    }

    /// PlayerEyeSensorHitHolder::clear drops a sensor invalidated by actor clipping.
    pub fn set_invalid_sensors(&mut self, invalid: Vec<usize>) {
        if self.candidate.is_some_and(|i| invalid.contains(&i)) {
            self.candidate = None;
        }
        self.invalid_sensors = invalid;
    }

    /// The head joint's position (cm, replay frame) after the model's last calculation.
    pub fn set_head_position(&mut self, head: Vec3) {
        self.head = Some(head);
    }

    /// Start the next frame from a recorded one (lockstep): the keeper's counter and target, the
    /// head joint and the eye of the frame before. The controller's turn is not recorded and
    /// stays the replay's own.
    pub fn restore(&mut self, hint: &LookHint, sensors: &[LookSensor]) {
        self.head = hint.head;
        self.previous_eye = Some(hint.feet + Vec3::new(0., EYE_HEIGHT, 0.));
        if let Some(counter) = hint.counter {
            self.counter = counter;
        }
        self.target = hint.target_stage.and_then(|at| {
            sensors
                .iter()
                .position(|s| (s.stage_position - at).abs().max_element() < 0.01)
        });
    }

    fn head_position(&self, feet: Vec3) -> Vec3 {
        self.head
            .unwrap_or(feet + Vec3::new(0., GUESSED_HEAD_HEIGHT, 0.))
    }

    /// `PlayerJointControlKeeper::updateLookAtTarget`: take `candidate` as the target when it is
    /// in reach of the head and nearer than the target the keeper has (or that one is out of
    /// reach).
    fn take_target(&mut self, candidate: usize, sensors: &[LookSensor], head: Vec3) {
        if self.target == Some(candidate) {
            return;
        }
        if let Some(current) = self.target
            && let Some(point) = within_gate(&sensors[current], head)
            && squared_distance(point, head) <= squared_distance(sensors[candidate].position, head)
        {
            return;
        }
        if within_gate(&sensors[candidate], head).is_some() {
            self.target = Some(candidate);
            self.counter = -1;
        }
    }

    /// `PlayerJointControlKeeper::updateLookAt`: the request made to the joint controller.
    fn ask_joint(&mut self, sensors: &[LookSensor], head: Vec3) {
        if !self.active {
            self.joint.set_keeps_turn(false);
            self.forget_target();
            return;
        }
        let Some(target) = self.target else {
            // Nothing to look at: no request, and the counter stays.
            self.joint.end_frame();
            return;
        };
        let sensor = &sensors[target];
        // The first frames of a target are requested whatever the distance, and the head keeps
        // the turn it has when the point leaves its range; afterwards the gate applies.
        let ungated = self.counter <= UNGATED_COUNTER;
        self.joint.set_keeps_turn(ungated);
        let point = if ungated {
            Some(sensor.position)
        } else {
            within_gate(sensor, head)
        };
        match point {
            Some(point) => {
                self.joint.request(point);
                self.joint.end_frame();
                self.counter = converge(self.counter, COUNTER_LIMIT, 1);
            }
            None => self.forget_target(),
        }
    }

    /// The end of a keeper frame without a request: the target and its counter are forgotten.
    fn forget_target(&mut self) {
        self.joint.end_frame();
        self.target = None;
        self.counter = -1;
    }

    /// One frame of the holder, keeper and controller request, after the state's own step.
    /// `actor` is the player's pose matrix (cm, its last column the position), `enabled` what
    /// `look_enabled` says of the state.
    pub fn step(&mut self, actor: &Mat34, enabled: bool, sensors: &[LookSensor]) {
        let feet = translation(actor);
        let head = self.head_position(feet);
        // The holder (`attackSensor`) runs in the sensor phase, before the player's own movement:
        // the eye sensor still sits where the previous frame left it, both for which sensors it
        // overlaps (the sensor director sorts them at the end of a frame) and for the distances
        // `tryRecordLookAtTarget` compares against the other sensors' positions of this frame.
        // The nearest candidate wins, ties keeping the one already held. (A flower sensor
        // trailing a running player shows the order: rocket-ground-start.)
        let eye = feet + Vec3::new(0., EYE_HEIGHT, 0.);
        let overlap_from = self.previous_eye.unwrap_or(eye);
        self.previous_eye = Some(eye);
        for (index, sensor) in sensors.iter().enumerate() {
            if self.invalid_sensors.contains(&index) {
                continue;
            }
            let reach = EYE_RADIUS + sensor.radius;
            if squared_distance(sensor.position, overlap_from) >= reach * reach {
                continue;
            }
            let nearer = self.candidate.is_none_or(|held| {
                squared_distance(sensors[held].position, overlap_from)
                    > squared_distance(sensor.position, overlap_from)
            });
            if nearer {
                self.candidate = Some(index);
            }
        }
        // The keeper then reads the candidate the sensor phase of this frame left.
        self.active = enabled;
        if enabled && let Some(candidate) = self.candidate {
            self.take_target(candidate, sensors, head);
        }
        self.ask_joint(sensors, head);
    }

    /// The probe's `look_keeper` cell.
    pub fn keeper_cell(&self) -> String {
        format!(
            "mode={};count={};suppress=0;headjoint={};requested={};ctrlmode={};accept=1",
            u8::from(self.active),
            self.counter,
            ORIGINAL_HEAD_JOINT,
            u8::from(self.joint.requested()),
            u8::from(self.joint.keeps_turn()),
        )
    }

    /// The sensor the controller was asked to look at this frame (the probe's `look_target`).
    pub fn requested_sensor(&self) -> Option<usize> {
        self.target.filter(|_| self.joint.requested())
    }

    /// The probe's `look_sensors` cell.
    pub fn sensors_cell(&self, sensors: &[LookSensor]) -> String {
        let mut parts = Vec::new();
        if let Some(sensor) = self.candidate.and_then(|i| sensors.get(i)) {
            parts.push(format!("candidate={}", sensor.label()));
        }
        if let Some(sensor) = self.target.and_then(|i| sensors.get(i)) {
            parts.push(format!("target={}", sensor.label()));
        }
        parts.join(";")
    }

    /// The head joint, turned. `animated` is the head's world matrix as the animation leaves it
    /// and `actor` the player's pose matrix.
    pub fn turn_head(&mut self, animated: &mut Mat34, actor: &Mat34) {
        self.joint.apply(animated, actor);
    }
}

#[path = "../tests/look_at.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
