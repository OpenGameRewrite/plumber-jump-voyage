//! Cappy in flight: the thrown cap's nerves, step by step.
//!
//! Written from the original's `HackCap` (its throw start, `exeThrow`,
//! `exeThrowBrake`, `exeThrowStay` and `exeReturn`, with their parameter
//! block `HackCapThrowParam`) as read from the 1.0.0 executable and confirmed
//! against recorded runs (`capthrow`, `spin-throw`). The flight over open
//! ground is reproduced, and in a stage's real collision (`geometry.rs`) the
//! cap collides with the walls and slides along one it meets at an angle.
//! Cappy's other wall reactions (a head-on hit that stops it, the
//! lock-on, the attack on what it hits), its floor reactions, the stick
//! steering of a throw in progress, the spiral and rolling throws and the
//! re-throw have no counterpart yet.
//!
//! The cap is a separate actor that updates before the player in the same
//! frame. It runs its current nerve once per frame and then its collider moves
//! it by its velocity, so a nerve that sets the velocity moves the cap in that
//! very frame. A nerve change (`al::setNerve`) takes effect on the next frame;
//! the nerve name a trace row shows is the one the frame left behind.
use crate::collider::{Location, PlayerCollider};
use crate::geometry::TriangleMesh;
use crate::movement::cap_pose::{CapPose, direction_of, horizontal_length, length};
use crate::movement::{Throw, UNIT};
use bevy::prelude::*;

/// `HackCapThrowParam`, as stored in `MarioCapCommonInfo/ActorParam`:
/// distances in centimetres, speeds in centimetres per frame.
mod param {
    /// Height of the cap above the feet when it is thrown.
    pub const THROW_HEIGHT: f32 = 70.;
    /// Frames the cap waits at the end of the throw (`mEndpointStopTime`) and
    /// the longest wait while the button is held (`mMaxEndpointStopTime`).
    pub const STAY_FRAMES: u32 = 20;
    pub const STAY_FRAMES_MAX: u32 = 120;
    /// Flight speed (`mMaxVel`).
    pub const MAX_SPEED: f32 = 32.;
    /// Flight speed of the continuous throw (`mContinuousThrowSpeed`).
    pub const CONTINUOUS_SPEED: f32 = 64.;
    /// How hard the return accelerates and its top speed.
    pub const RETURN_STRENGTH: f32 = 18.;
    pub const RETURN_SPEED_MAX: f32 = 80.;
    /// Throw distance (`mMaxDist`) and braking time (`mBreakTime`).
    pub const RANGE: f32 = 500.;
    pub const BRAKE_FRAMES: u32 = 10;
    /// Most the return turns toward the player in one frame, in degrees.
    pub const TURN_LIMIT_DEGREES: f32 = 40.;
}
/// Where the cap appears, in front of the thrown-from point
/// (`HackCap::exeThrowStart` sets the position that far along the throw).
const START_DISTANCE: f32 = 150.;
/// Height of the player's head above the feet (`rs::getPlayerHeadPos`
/// while standing): the point the cap returns to.
const HEAD_HEIGHT: f32 = 110.;
/// The cap is caught when its `Catch` hit sensor (a sphere of 100 around the cap's centre,
/// added by `PlayerFunction::initCapModelActor`) touches the player's `Body` sensor (50, see
/// `CATCH_CENTER_HEIGHT`). The sensors' contact is read after the cap has moved in the frame
/// and before the player's movement (`HackCap::receiveMsg` takes `PlayerCapCatch` and flags the
/// cap, the player's movement then puts it on), so a cap that flew into range this frame is
/// caught in it.
const CATCH_SENSOR_RADIUS: f32 = 100.;
const BODY_SENSOR_RADIUS: f32 = 50.;
/// Height of the player's `Body` sensor above the feet (`PlayerActorHakoniwa/InitSensor`).
const CATCH_CENTER_HEIGHT: f32 = 75.;
/// The return only slows down this near the head: speed is limited to a
/// fraction of the distance there.
const RETURN_SLOW_DISTANCE: f32 = 200.;
const RETURN_SLOW_RATE: f32 = 0.8;
/// `tryCollideWallReaction` slides the cap along a wall it meets at an angle
/// whose cosine to the cap's heading is under this (30 degrees from head-on).
const SLIDE_LIMIT: f32 = 0.866_025_4;
/// Under this cosine to the wall the cap grazes it and raises no reaction
/// (sine of 5 degrees); a wall within 5 degrees of the last one counts as that
/// same wall (cosine of 5 degrees).
const GRAZE_LIMIT: f32 = 0.087_155_74;
const SAME_WALL: f32 = 0.996_194_7;
/// Frames the cap stays put after it was trampled (`exeTrample` compares its
/// step with 15).
const TRAMPLE_FRAMES: u32 = 15;
/// What `HackCap::receiveMsg` takes of a trample on its `Trample` sensor: the
/// vertical and horizontal distance between the player's foot sensor and its
/// own sensor must not exceed these.
const TRAMPLE_MAX_VERTICAL: f32 = 70.;
const TRAMPLE_MAX_HORIZONTAL: f32 = 200.;
/// The cap's `Trample` hit sensor (`PlayerFunction::initCapModelActor`): a
/// sphere of 300 around a point 20 centimetres off the cap along its up axis.
/// The offset's direction is a reading of the recordings, not of the code: the
/// cap-bounce run is first trampled in the frame the player's foot comes within
/// the vertical limit of a sensor 20 below the cap, while a sensor above the
/// cap would have been reached some frames earlier or not at all. The cap's
/// pose is not simulated, so the offset is taken downward here.
const TRAMPLE_SENSOR_RADIUS: f32 = 300.;
const TRAMPLE_SENSOR_Y: f32 = -20.;
/// The player's foot sensor (`PlayerActorHakoniwa/InitSensor`: `Foot`).
pub const FOOT_SENSOR_RADIUS: f32 = 40.;
pub const FOOT_SENSOR_Y: f32 = 40.;
/// `rs::isEnableSendTrampleMsg`: the foot must be within 70 degrees of straight
/// above the cap's sensor (cosine of 70 degrees).
const TRAMPLE_ANGLE_COS: f32 = 0.342_020_15;
/// Speed is considered gone below this (`al::isNearZero`).
const NEAR_ZERO: f32 = 0.001;
/// Frames over which the return gives up the velocity it had (ease-in).
const RETURN_EASE_FRAMES: f32 = 10.;
/// The catch animation lasts this many frames; the cap hides when the nerve
/// has run longer than that. The cap plays the clip of the way the player
/// takes it (`PlayerActorHakoniwa::attackSensor`, the cap's catch message):
/// `CatchCap` standing, `CatchCapJumpParts` in the air, `CatchCapParts` on the
/// ground in any other state. The lengths are those of the cap's own clips
/// (`MarioCapAnimation`: `CatchCap` 5, `CatchCapParts` 2, `CatchCapJumpParts` 3).
pub fn catch_frames(clip: &str) -> u32 {
    match clip {
        "CatchCapJumpParts" => 3,
        "CatchCapParts" => 2,
        _ => 5,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapPhase {
    Out,
    Stay,
    Return,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapNerve {
    Throw,
    ThrowBrake,
    ThrowStay,
    /// The player stood on the hovering cap (`HackCap::exeTrample`, which the
    /// recordings name `TrampleReturn`): it stays put for a short while, then
    /// goes back.
    Trample,
    Return,
}
impl CapNerve {
    /// The nerve a recorded cap state names, when it is one of the flight's.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "Throw" => Self::Throw,
            "ThrowBrake" => Self::ThrowBrake,
            "ThrowStay" => Self::ThrowStay,
            "TrampleReturn" => Self::Trample,
            "Return" => Self::Return,
            _ => return None,
        })
    }
    /// The nerve's name in the game's traces.
    pub fn name(self) -> &'static str {
        match self {
            Self::Throw => "Throw",
            Self::ThrowBrake => "ThrowBrake",
            Self::ThrowStay => "ThrowStay",
            Self::Trample => "TrampleReturn",
            Self::Return => "Return",
        }
    }
}

/// A cap as a recording shows it at one moment: the nerve it is in, the steps
/// it has run there, where it is, how fast it moves and the speed it was thrown at.
#[derive(Clone, Copy, Debug)]
pub struct Moment {
    pub nerve: CapNerve,
    pub step: u32,
    /// Centimetres, and centimetres per frame.
    pub spot: Vec3,
    pub velocity: Vec3,
    pub speed: f32,
    pub return_speed: f32,
}

/// What the player hands the cap when it throws (`HackCap::startThrow`).
#[derive(Clone, Copy, Debug)]
pub struct Launch {
    /// The way the throw goes, horizontal and unit length.
    pub front: Vec3,
    pub kind: Throw,
    /// Thrown from the left hand (the clip names carry the side).
    pub left: bool,
    /// Thrown in the air (the down throw only rolls along the ground).
    pub air: bool,
    /// The third throw in a row, which flies faster.
    pub continuous: bool,
}

/// Moments of the flight the player and the presentation react to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CapEvent {
    /// The first step of the throw nerve: the throw's hit reaction and voice.
    Thrown,
    /// A step of the stay nerve: it starts a level sound on every step.
    Staying,
    /// The stay nerve chose the return (the reaction named after it).
    LeftStay,
    /// The first step of the stay nerve while the button is held: the
    /// reaction `Stay状態開始`.
    StayStarted,
    /// The first step of the trample nerve: the reaction `通常Trample`.
    Trampled,
    /// The cap reached the head.
    Caught,
    /// The cap met a wall at an angle and turned along it: the collision
    /// reaction, its effect and the sound `HitCol`, which is louder the
    /// squarer the hit (the cosine of the angle).
    WallHit(f32),
}

/// What the cap needs to know of the player and the stage each frame.
#[derive(Clone, Copy, Debug)]
pub struct CapWorld<'a> {
    /// Player position, metres.
    pub player: Vec3,
    /// The throw button is held (`PlayerInput::isHoldSpinCap`).
    pub hold: bool,
    /// The stage's triangles, when the replay has them.
    pub mesh: Option<&'a TriangleMesh>,
}

#[derive(Clone)]
pub struct Cap {
    /// Metres.
    pub pos: Vec3,
    /// Seconds since the throw, for presentation.
    pub age: f32,
    pub returning: bool,
    pub kind: Throw,
    air: bool,
    pub phase: CapPhase,
    /// The nerve the next frame runs and the frames it has run so far.
    pub nerve: CapNerve,
    step: u32,
    /// Frames since the cap appeared.
    pub frames: u32,
    /// The clip the cap plays (`FlyingWaitR`, `StayR`, ...), as the trace names it,
    /// how many frames it has played and which start of a clip it is (a later
    /// start of the same name restarts its sounds and effects).
    action: &'static str,
    action_frame: u32,
    action_serial: u32,
    left: bool,
    /// Centimetres, and centimetres per frame.
    spot: Vec3,
    velocity: Vec3,
    /// Direction of the throw, and the speed the braking started from.
    front: Vec3,
    /// The actor's pose (`HackCap`'s quaternion): facing back at the thrower while it
    /// flies and stays, then turned by the trample and the return.
    pose: CapPose,
    speed: f32,
    /// Frames the throw flies at full speed before it brakes, and the speed
    /// the brake takes off each frame (`_27c` and `_280`).
    cruise_frames: u32,
    brake_step: f32,
    /// The speed the return has built up (`_288`).
    return_speed: f32,
    /// The player trampled the cap in this frame's sensor phase: the trample
    /// nerve is entered when the cap next updates (`al::setNerve`).
    trampled: bool,
    /// `Stay状態開始` was raised (once per throw, while the button is held).
    stay_reaction_done: bool,
    /// Cappy's collider (`PlayerColliderHackCap`), against a triangle mesh.
    collider: PlayerCollider,
    /// The normal of the wall the last reaction turned along (`+0x514`) and
    /// whether the collider found a wall in the previous frame.
    wall_memory: Vec3,
    wall_last_frame: bool,
}
impl Cap {
    /// Place a cap as `exeThrowStart` does on its one step: in front of the
    /// thrown-from point, flying at `speed`. The cap moves by that speed in
    /// the same frame.
    /// `serial` numbers the cap's first clip; later clips count on from it.
    pub fn start(launch: Launch, player: Vec3, serial: u32) -> Self {
        let front = launch.front.with_y(0.).normalize_or_zero();
        let base = player / UNIT + Vec3::Y * param::THROW_HEIGHT;
        let spot = base + front * START_DISTANCE;
        let speed = if launch.continuous {
            param::CONTINUOUS_SPEED
        } else {
            param::MAX_SPEED
        };
        let brake_frames = param::BRAKE_FRAMES as f32;
        let mut cap = Self {
            pos: spot * UNIT,
            age: 0.,
            returning: false,
            kind: launch.kind,
            air: launch.air,
            phase: CapPhase::Out,
            nerve: CapNerve::Throw,
            step: 0,
            frames: 0,
            action: flying(launch.kind, launch.air, launch.left),
            action_frame: 0,
            action_serial: serial,
            left: launch.left,
            spot,
            velocity: front * speed,
            front,
            pose: CapPose::thrown_along(front),
            speed,
            cruise_frames: cruise_frames(speed),
            brake_step: -speed / brake_frames,
            return_speed: 0.,
            trampled: false,
            stay_reaction_done: false,
            collider: PlayerCollider::for_cap(),
            wall_memory: Vec3::ZERO,
            wall_last_frame: false,
        };
        cap.advance(None);
        cap
    }
    /// Send the cap on another leg toward `direction` (the flick throw, which the
    /// original runs as `ThrowAppend`; here a fresh short throw).
    pub fn redirect(&mut self, direction: Vec3) {
        let Some(direction) = direction.try_normalize() else {
            return;
        };
        self.front = direction;
        self.velocity = direction * self.speed;
        self.nerve = CapNerve::Throw;
        self.step = 0;
        self.cruise_frames = 6;
        self.phase = CapPhase::Out;
        self.returning = false;
        self.play(flying(self.kind, self.air, self.left));
    }
    /// `HackCap::receiveMsg` for a trample on the `Trample` sensor
    /// (`rs::sendMsgPlayerCapTrample`): the player's foot sensor is at `foot`
    /// (centimetres). The cap takes it while it hovers (`ThrowStay`) with the
    /// button held, when the foot is within 70 degrees of straight above its
    /// sensor, the sensors touch, and the distance between them is at most 70
    /// vertically and 200 horizontally; the cap then stands still and returns
    /// (the nerve is entered when it next updates). `jump_possible` is
    /// `PlayerCapActionHistory`'s flag: one cap jump per time on the ground.
    pub fn take_trample(&mut self, foot: Vec3, hold: bool, jump_possible: bool) -> bool {
        if self.nerve != CapNerve::ThrowStay || self.trampled || !hold || !jump_possible {
            return false;
        }
        let sensor = self.spot + Vec3::Y * TRAMPLE_SENSOR_Y;
        let between = foot - sensor;
        let reach = (TRAMPLE_SENSOR_RADIUS + FOOT_SENSOR_RADIUS).powi(2);
        let above = between.try_normalize().map_or(0., |dir| dir.y);
        if between.length_squared() >= reach
            || above <= TRAMPLE_ANGLE_COS
            || between.y.abs() > TRAMPLE_MAX_VERTICAL
            || between.with_y(0.).length() > TRAMPLE_MAX_HORIZONTAL
        {
            return false;
        }
        self.trampled = true;
        true
    }
    /// Start a clip over.
    fn play(&mut self, action: &'static str) {
        self.action = action;
        self.action_frame = 0;
        self.action_serial += 1;
    }
    /// The nerve the trace names.
    pub fn state_name(&self) -> &'static str {
        self.nerve.name()
    }
    /// The clip the cap plays.
    pub fn clip(&self) -> &'static str {
        self.action
    }
    /// The frame the clip shows (0 on the frame it starts): a clip that is not
    /// a loop holds its last frame.
    pub fn clip_frame(&self) -> f32 {
        let (length, looping) = clip_shape(self.action);
        if looping {
            (self.action_frame % length) as f32
        } else {
            self.action_frame.min(length) as f32
        }
    }
    /// A looping clip came back to its first frame this frame.
    pub fn clip_wrapped(&self) -> bool {
        let (length, looping) = clip_shape(self.action);
        looping && self.action_frame > 0 && self.action_frame.is_multiple_of(length)
    }
    /// Which start of a clip the cap's clip is.
    pub fn clip_serial(&self) -> u32 {
        self.action_serial
    }
    /// A cap put back into place each frame (lockstep) keeps the numbering of
    /// the clip it is in the middle of.
    pub fn continue_clip(&mut self, before: &Cap) {
        self.action_serial = before.action_serial + u32::from(before.action != self.action);
        if before.action == self.action {
            self.action_frame = before.action_frame;
        }
    }
    /// Put the cap into a recorded moment (lockstep).
    pub fn resume(&mut self, moment: &Moment) {
        self.nerve = moment.nerve;
        self.step = moment.step;
        self.spot = moment.spot;
        self.pos = moment.spot * UNIT;
        self.velocity = moment.velocity;
        self.return_speed = moment.return_speed;
        self.phase = phase_of(moment.nerve);
        self.returning = moment.nerve == CapNerve::Return;
        // The throw's speed fixes how long it cruises and how hard it brakes.
        self.speed = moment.speed;
        let brake_frames = param::BRAKE_FRAMES as f32;
        self.cruise_frames = cruise_frames(moment.speed);
        self.brake_step = -moment.speed / brake_frames;
        // The stay and the return start their clips on their first step, so
        // their first row still shows the clip before.
        let (action, frame) = match (moment.nerve, moment.step) {
            (CapNerve::Throw, step) => (flying(self.kind, self.air, self.left), step),
            (CapNerve::ThrowBrake, step) => (
                flying(self.kind, self.air, self.left),
                self.cruise_frames + 1 + step,
            ),
            (CapNerve::ThrowStay, 0) => (flying(self.kind, self.air, self.left), 0),
            (CapNerve::ThrowStay, step) => (stay(self.left), step - 1),
            (CapNerve::Trample, 0) => (stay(self.left), 0),
            (CapNerve::Trample, step) => ("Trample", step - 1),
            (CapNerve::Return, 0) => (stay(self.left), 0),
            (CapNerve::Return, step) => (flying(self.kind, self.air, self.left), step - 1),
        };
        self.action = action;
        self.action_frame = frame;
        if moment.velocity.length() > NEAR_ZERO {
            self.front = moment.velocity.normalize();
            // (A cap put back in the middle of its return has no pose history: it
            // faces the way it was moving.)
            if moment.nerve == CapNerve::Return && moment.step > 0 {
                self.pose.stand_facing(self.front);
            }
        }
    }
    /// Run one frame: the current nerve, then the collider's move. Returns
    /// what happened.
    pub fn update(&mut self, world: &CapWorld) -> Vec<CapEvent> {
        self.frames += 1;
        self.age = self.frames as f32 / crate::movement::FPS;
        self.action_frame += 1;
        let head = world.player / UNIT + Vec3::Y * HEAD_HEIGHT;
        let body = world.player / UNIT + Vec3::Y * CATCH_CENTER_HEIGHT;
        let mut events = Vec::new();
        if std::mem::take(&mut self.trampled) {
            self.nerve = CapNerve::Trample;
            self.step = 0;
            self.phase = CapPhase::Stay;
        }
        let next = match self.nerve {
            CapNerve::Throw => self.throw(&mut events),
            CapNerve::ThrowBrake => self.throw_brake(&mut events),
            CapNerve::ThrowStay => self.throw_stay(world.hold, &mut events),
            CapNerve::Trample => self.trample(&mut events),
            CapNerve::Return => self.go_back(head),
        };
        self.step += 1;
        if let Some(next) = next {
            self.nerve = next;
            self.step = 0;
            self.phase = phase_of(next);
            self.returning = next == CapNerve::Return;
        }
        self.advance(world.mesh);
        // The sensors touch where the cap has come to.
        if self.nerve == CapNerve::Return
            && (self.spot - body).length() < CATCH_SENSOR_RADIUS + BODY_SENSOR_RADIUS
        {
            events.push(CapEvent::Caught);
        }
        events
    }
    /// The collider moves the cap by its velocity: in a stage's collision it
    /// stops at the surfaces it meets, otherwise the move is free.
    fn advance(&mut self, mesh: Option<&TriangleMesh>) {
        match mesh {
            Some(mesh) => {
                let out = self.collider.collide(mesh, self.spot, self.velocity, 0.);
                self.wall_last_frame = self.collider.wall.is_some();
                if out.collided {
                    self.spot = out.pos;
                } else {
                    self.spot += self.velocity;
                }
            }
            None => self.spot += self.velocity,
        }
        self.pos = self.spot * UNIT;
    }
    /// `HackCap::tryCollideWallReaction` for a wall the cap met on a face
    /// last frame, at the start of the throw and brake nerves: a wall at an
    /// angle (`0x409624`) turns the cap along it at the speed it had and moves
    /// it one centimetre into the wall (the collider pushes it back out); a
    /// head-on wall stops the cap in the original (`stayWallHit`), which is
    /// not reproduced here.
    fn wall_reaction(&mut self, events: &mut Vec<CapEvent>) {
        let Some(wall) = &self.collider.wall else {
            return;
        };
        if wall.location != Location::Face {
            return;
        }
        let Some(normal) = wall.normal.with_y(0.).try_normalize() else {
            return;
        };
        // The same wall as the last reaction's, met again right away, is no
        // new hit. The original decides this with a trigger word of the cap
        // that is not read here; "a wall last frame" is its likely meaning.
        let first = !self.wall_last_frame
            || self.wall_memory.length() < NEAR_ZERO
            || self.wall_memory.dot(normal) <= SAME_WALL;
        let alignment = self.front.dot(normal).abs();
        if alignment >= SLIDE_LIMIT {
            return;
        }
        let speed = self.velocity.length();
        let along = self.velocity - normal * self.velocity.dot(normal);
        let Some(front) = along.try_normalize() else {
            return;
        };
        self.spot -= normal;
        if alignment > GRAZE_LIMIT && first {
            events.push(CapEvent::WallHit(alignment));
        }
        self.front = front;
        self.velocity = front * speed;
        if first {
            self.wall_memory = normal;
        }
    }
    /// `HackCap::exeThrow`: fly on at the speed the throw set, until the
    /// cruise frames are used up.
    fn throw(&mut self, events: &mut Vec<CapEvent>) -> Option<CapNerve> {
        if self.step == 0 {
            events.push(CapEvent::Thrown);
        }
        self.wall_reaction(events);
        (self.step >= self.cruise_frames).then_some(CapNerve::ThrowBrake)
    }
    /// `HackCap::exeThrowBrake`: take a tenth of the speed off each frame; the
    /// cap stays where it has stopped.
    fn throw_brake(&mut self, events: &mut Vec<CapEvent>) -> Option<CapNerve> {
        self.wall_reaction(events);
        self.velocity += self.front * self.brake_step;
        let stopped = self.velocity.length() < NEAR_ZERO;
        let reversed = self.velocity.dot(self.front) < 0.;
        (stopped || reversed || self.step >= param::BRAKE_FRAMES).then_some(CapNerve::ThrowStay)
    }
    /// `HackCap::exeThrowStay`: hover for the stay time, longer while the
    /// button is held, then go back.
    fn throw_stay(&mut self, hold: bool, events: &mut Vec<CapEvent>) -> Option<CapNerve> {
        if self.step == 0 {
            self.play(stay(self.left));
        }
        events.push(CapEvent::Staying);
        if hold && !self.stay_reaction_done {
            self.stay_reaction_done = true;
            events.push(CapEvent::StayStarted);
        }
        if self.step <= param::STAY_FRAMES || hold && self.step <= param::STAY_FRAMES_MAX {
            return None;
        }
        events.push(CapEvent::LeftStay);
        Some(CapNerve::Return)
    }
    /// `HackCap::exeTrample`: face the way the joint does, start the trample
    /// clip, stand still for fifteen steps, then return.
    fn trample(&mut self, events: &mut Vec<CapEvent>) -> Option<CapNerve> {
        if self.step == 0 {
            self.face_joint_front();
            self.play("Trample");
            self.velocity = Vec3::ZERO;
            events.push(CapEvent::Trampled);
        }
        (self.step >= TRAMPLE_FRAMES).then_some(CapNerve::Return)
    }
    /// The first step of `exeTrample`: the actor turns to the front of its model's `AllRoot` joint
    /// (`al::calcJointFrontDir`, then `al::faceToDirection`). The joint matrix is the one the
    /// model last updated, which is the pose and the clip frame of the frame before.
    fn face_joint_front(&mut self) {
        let (length, looping) = clip_shape(self.action);
        let before = self.action_frame.saturating_sub(1);
        let frame = if looping {
            before % length
        } else {
            before.min(length)
        };
        let local = crate::cap_joint::all_root_front(self.action, frame as f32);
        let (front, found) = direction_of(self.pose.world_axis(local));
        if found {
            self.pose.face_to_direction(front);
        }
    }
    /// `HackCap::exeReturn`: fly toward the head, a little faster every frame. The pose first
    /// leans a quarter of the way toward the head (only within `RETURN_SLOW_DISTANCE` of it, as
    /// the speed limit), then its front turns toward the head by at most the turn limit, and the
    /// cap flies along that front. The cap's front is the one it had when the return began: facing
    /// the player after a throw, the joint's front after a trample.
    fn go_back(&mut self, head: Vec3) -> Option<CapNerve> {
        if self.step == 0 {
            self.play(flying(self.kind, self.air, self.left));
            self.return_speed = 0.;
        }
        let to_head = head - self.spot;
        let (direction, found) = direction_of(to_head);
        if !found {
            return None;
        }
        if horizontal_length(to_head) < RETURN_SLOW_DISTANCE {
            self.return_speed = self
                .return_speed
                .clamp(0., (length(to_head) * RETURN_SLOW_RATE).max(0.));
            self.pose.lean_toward(direction);
        }
        // (A rolling throw takes another up axis here, which is not ported; `front_turned_toward`
        // writes nothing only for a direction without a plane, which does not happen here.)
        let front = self
            .pose
            .front_turned_toward(direction, param::TURN_LIMIT_DEGREES)
            .unwrap_or(Vec3::ZERO);
        self.pose.stand_facing(front);
        self.front = front;
        self.return_speed =
            (self.return_speed + param::RETURN_STRENGTH).min(param::RETURN_SPEED_MAX);
        // What the cap still moves across its new heading fades out.
        let across = self.velocity - self.front * self.velocity.dot(self.front);
        let ease = ease_in((self.step as f32 / RETURN_EASE_FRAMES).min(1.));
        self.velocity = self.front * self.return_speed + across * (1. - ease);
        None
    }
}
/// The cap's flight state, without the collider's bookkeeping: the simulation
/// baseline (`original_simulation_trace`) hashes this text, and the collider
/// only acts against a stage mesh.
impl std::fmt::Debug for Cap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cap")
            .field("pos", &self.pos)
            .field("age", &self.age)
            .field("returning", &self.returning)
            .field("kind", &self.kind)
            .field("air", &self.air)
            .field("phase", &self.phase)
            .field("nerve", &self.nerve)
            .field("step", &self.step)
            .field("frames", &self.frames)
            .field("action", &self.action)
            .field("action_frame", &self.action_frame)
            .field("action_serial", &self.action_serial)
            .field("left", &self.left)
            .field("spot", &self.spot)
            .field("velocity", &self.velocity)
            .field("front", &self.front)
            .field("pose", &self.pose.quat())
            .field("speed", &self.speed)
            .field("cruise_frames", &self.cruise_frames)
            .field("brake_step", &self.brake_step)
            .field("return_speed", &self.return_speed)
            .finish()
    }
}
/// A throw this fast is the continuous one (the third in a row).
pub fn is_continuous_speed(speed: f32) -> bool {
    speed > param::MAX_SPEED + 0.5
}
/// Frames the throw cruises before it brakes: until the braking distance (a
/// triangle, half the speed over the braking time) is all that is left of the
/// range.
fn cruise_frames(speed: f32) -> u32 {
    let brake_frames = param::BRAKE_FRAMES as f32;
    ((param::RANGE - speed * brake_frames / 2.) / speed)
        .floor()
        .max(0.) as u32
}
fn phase_of(nerve: CapNerve) -> CapPhase {
    match nerve {
        CapNerve::Throw | CapNerve::ThrowBrake => CapPhase::Out,
        CapNerve::ThrowStay | CapNerve::Trample => CapPhase::Stay,
        CapNerve::Return => CapPhase::Return,
    }
}
/// The clip of a cap in flight: the spiral and rolling throws have their own,
/// the others tell the hands apart.
fn flying(kind: Throw, air: bool, left: bool) -> &'static str {
    match (kind, air) {
        (Throw::Spin, _) => "FlyingWaitSpiral",
        (Throw::Down, false) => "FlyingWaitRolling",
        _ if left => "FlyingWaitL",
        _ => "FlyingWaitR",
    }
}
fn stay(left: bool) -> &'static str {
    if left { "StayL" } else { "StayR" }
}
/// Frames of a clip of the cap's animation archive and whether it loops
/// (`MarioCapAnimation`: the flying clips run 11 frames, the stay's loop 9).
fn clip_shape(name: &str) -> (u32, bool) {
    if name.starts_with("Stay") {
        (9, true)
    } else if name == "Trample" {
        (TRAMPLE_FRAMES, false)
    } else {
        (11, false)
    }
}
/// `al::easeIn`.
fn ease_in(x: f32) -> f32 {
    (-0.5 * x + 1.5) * x * x
}
#[path = "tests/cap.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
