//! Cappy in flight: the thrown cap's nerves, step by step.
//!
//! Written from the original's `HackCap` (its throw start, `exeThrow`,
//! `exeThrowBrake`, `exeThrowStay` and `exeReturn`, with their parameter
//! block `HackCapThrowParam`) as read from the 1.0.0 executable and confirmed
//! against recorded runs (`capthrow`, `spin-throw`). Only the flight over
//! open ground is reproduced: Cappy's wall and floor reactions, the stick
//! steering of a throw in progress, the spiral and rolling throws and the
//! re-throw have no counterpart yet.
//!
//! The cap is a separate actor that updates before the player in the same
//! frame. It runs its current nerve once per frame and then its collider moves
//! it by its velocity, so a nerve that sets the velocity moves the cap in that
//! very frame. A nerve change (`al::setNerve`) takes effect on the next frame;
//! the nerve name a trace row shows is the one the frame left behind.
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
/// The cap is caught when its centre is this close to the head point. The
/// catch is a sensor contact: the player's sensor sends the catch message to
/// the cap's "Catch" sensor, which the cap takes in its return nerve
/// (`HackCap::receiveMsg`). That sensor's radius is 100 (`HackCap::init`) and
/// the player's head sensor sits 110 up (`PlayerActorHakoniwa` sensor list).
/// Adding the player sensor's own radius (40) would catch at 140, but the
/// recordings reject that: a cap 131 from the head after its move is not
/// caught (`areas-cascade`) while one 99 away is (`capthrow`). The centre
/// distance of 100 is the reading that fits; the contact rule behind it
/// (what the sensor test really compares) is not derived.
const CATCH_DISTANCE: f32 = 100.;
/// The return only slows down this near the head: speed is limited to a
/// fraction of the distance there.
const RETURN_SLOW_DISTANCE: f32 = 200.;
const RETURN_SLOW_RATE: f32 = 0.8;
/// Speed is considered gone below this (`al::isNearZero`).
const NEAR_ZERO: f32 = 0.001;
/// Frames over which the return gives up the velocity it had (ease-in).
const RETURN_EASE_FRAMES: f32 = 10.;
/// The catch animation lasts this many frames; the cap hides when the nerve
/// has run longer than that.
pub const CATCH_FRAMES: u32 = 5;

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
    Return,
}
impl CapNerve {
    /// The nerve a recorded cap state names, when it is one of the flight's.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "Throw" => Self::Throw,
            "ThrowBrake" => Self::ThrowBrake,
            "ThrowStay" => Self::ThrowStay,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapEvent {
    /// The first step of the throw nerve: the throw's hit reaction and voice.
    Thrown,
    /// A step of the stay nerve: it starts a level sound on every step.
    Staying,
    /// The stay nerve chose the return (the reaction named after it).
    LeftStay,
    /// The cap reached the head.
    Caught,
}

/// What the cap needs to know of the player each frame.
#[derive(Clone, Copy, Debug)]
pub struct CapWorld {
    /// Player position, metres.
    pub player: Vec3,
    /// The throw button is held (`PlayerInput::isHoldSpinCap`).
    pub hold: bool,
}

#[derive(Clone, Debug)]
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
    speed: f32,
    /// Frames the throw flies at full speed before it brakes, and the speed
    /// the brake takes off each frame (`_27c` and `_280`).
    cruise_frames: u32,
    brake_step: f32,
    /// The speed the return has built up (`_288`).
    return_speed: f32,
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
            speed,
            cruise_frames: cruise_frames(speed),
            brake_step: -speed / brake_frames,
            return_speed: 0.,
        };
        cap.advance();
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
        looping && self.action_frame > 0 && self.action_frame % length == 0
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
            (CapNerve::Return, 0) => (stay(self.left), 0),
            (CapNerve::Return, step) => (flying(self.kind, self.air, self.left), step - 1),
        };
        self.action = action;
        self.action_frame = frame;
        if moment.velocity.length() > NEAR_ZERO {
            self.front = moment.velocity.normalize();
        }
    }
    /// Run one frame: the current nerve, then the collider's move. Returns
    /// what happened.
    pub fn update(&mut self, world: &CapWorld) -> Vec<CapEvent> {
        self.frames += 1;
        self.age = self.frames as f32 / crate::movement::FPS;
        self.action_frame += 1;
        let head = world.player / UNIT + Vec3::Y * HEAD_HEIGHT;
        let mut events = Vec::new();
        let next = match self.nerve {
            CapNerve::Throw => self.throw(&mut events),
            CapNerve::ThrowBrake => self.throw_brake(),
            CapNerve::ThrowStay => self.throw_stay(world.hold, &mut events),
            CapNerve::Return => self.go_back(head),
        };
        self.step += 1;
        if let Some(next) = next {
            self.nerve = next;
            self.step = 0;
            self.phase = phase_of(next);
            self.returning = next == CapNerve::Return;
        }
        self.advance();
        if self.nerve == CapNerve::Return && (self.spot - head).length() <= CATCH_DISTANCE {
            events.push(CapEvent::Caught);
        }
        events
    }
    /// The collider moves the cap by its velocity.
    fn advance(&mut self) {
        self.spot += self.velocity;
        self.pos = self.spot * UNIT;
    }
    /// `HackCap::exeThrow`: fly on at the speed the throw set, until the
    /// cruise frames are used up.
    fn throw(&mut self, events: &mut Vec<CapEvent>) -> Option<CapNerve> {
        if self.step == 0 {
            events.push(CapEvent::Thrown);
        }
        (self.step >= self.cruise_frames).then_some(CapNerve::ThrowBrake)
    }
    /// `HackCap::exeThrowBrake`: take a tenth of the speed off each frame; the
    /// cap stays where it has stopped.
    fn throw_brake(&mut self) -> Option<CapNerve> {
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
        if self.step <= param::STAY_FRAMES || hold && self.step <= param::STAY_FRAMES_MAX {
            return None;
        }
        events.push(CapEvent::LeftStay);
        Some(CapNerve::Return)
    }
    /// `HackCap::exeReturn`: fly toward the head, a little faster every frame.
    fn go_back(&mut self, head: Vec3) -> Option<CapNerve> {
        if self.step == 0 {
            self.play(flying(self.kind, self.air, self.left));
            self.return_speed = 0.;
        }
        let to_head = head - self.spot;
        let distance = to_head.length();
        let Some(direction) = to_head.try_normalize() else {
            return None;
        };
        // The cap's heading follows the head, at most the turn limit a frame.
        self.front = if self.step == 0 {
            direction
        } else {
            turn_toward(
                self.front,
                direction,
                param::TURN_LIMIT_DEGREES.to_radians(),
            )
        };
        if to_head.with_y(0.).length() < RETURN_SLOW_DISTANCE {
            self.return_speed = self
                .return_speed
                .clamp(0., (distance * RETURN_SLOW_RATE).max(0.));
        }
        self.return_speed =
            (self.return_speed + param::RETURN_STRENGTH).min(param::RETURN_SPEED_MAX);
        // What the cap still moves across its new heading fades out.
        let across = self.velocity - self.front * self.velocity.dot(self.front);
        let ease = ease_in((self.step as f32 / RETURN_EASE_FRAMES).min(1.));
        self.velocity = self.front * self.return_speed + across * (1. - ease);
        None
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
        CapNerve::ThrowStay => CapPhase::Stay,
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
    } else {
        (11, false)
    }
}
/// `al::easeIn`.
fn ease_in(x: f32) -> f32 {
    (-0.5 * x + 1.5) * x * x
}
/// Turn `from` toward `to` by at most `limit` radians (`al::turnFrontSpherical`).
fn turn_toward(from: Vec3, to: Vec3, limit: f32) -> Vec3 {
    let angle = from.angle_between(to);
    if angle <= limit || angle < 1e-6 {
        return to;
    }
    let axis = from.cross(to);
    if axis.length_squared() < 1e-12 {
        return to;
    }
    Quat::from_axis_angle(axis.normalize(), limit) * from
}

#[path = "tests/cap.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
