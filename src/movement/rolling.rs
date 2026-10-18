//! The roll (`PlayerStateRolling`) and the slope slide control that moves it.
//!
//! The state is not in the public decompilation; this is a reading of its
//! 1.0.0 machine code (`appear`, `control`, `exeStart`, `exeRolling`,
//! `exeBoostStart`, `exeUnRoll`, `exeLand`, `exeStandUp`, `restartRolling`,
//! `updateRollingAnimFrameRate`) checked against the recorded roll, with
//! `PlayerActionSlopeSlideControl::update` from the decompilation as the
//! movement. The pieces the recorded runs do not exercise (restarting, the
//! slow-roll sub-states, slopes) follow the same code but are unverified.
//!
//! The nerves are `Start`, `Rolling`, `BoostStart`, `UnRoll`, `Brake` and
//! `Land` here (`RollNerve`, mode `Move::Roll`), `StandUp` (`Move::RollEnd`)
//! and `Jump` (`Move::RollJump`, a long jump). Like every nerve they are chosen
//! in one step and run their first step in the next; what `appear` does (the
//! speed the roll starts with) happens in the step that chooses the roll. The
//! sub-nerves belong to one actor nerve, `Rolling`, so its step counter runs on
//! through all of them, the jump included. `EndSquat` is the state's last
//! nerve: the actor leaves for the squat in the step that reaches it.
use super::*;

/// How a roll begins; the original's `appear` tells them apart by the action
/// triggers the previous state left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RollStart {
    /// Crouching and pressing the cap button: hop into the roll.
    Normal,
    /// Cancelling the landing of a ground pound (trigger 0x11): a stronger
    /// start without the hop.
    Boost,
    /// Landing from a dive or a long jump while crouching fast (trigger 8):
    /// straight into the rolling loop, free to end at once.
    Landing,
}

/// The nerves of the roll state that share `Move::Roll`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum RollNerve {
    #[default]
    Start,
    Rolling,
    BoostStart,
    UnRoll,
    /// The winding down roll slides on until it is slow enough to crouch.
    Brake,
    Land,
    /// The roll's jump: its flight is `Move::RollJump`, the state's counters
    /// still run.
    Jump,
}

/// What the roll state keeps (fields `_90` to `_b0` of the original, and the
/// slope slide control's ground).
#[derive(Clone, Debug)]
pub(super) struct RollControl {
    nerve: RollNerve,
    /// Steps the nerve has run, counted when the step begins.
    steps: u32,
    /// `_98`: frames rolled, held just above the longest minimum.
    frames: i32,
    /// `_9c`: frames since the cap button was last pressed (a charged press
    /// restarts the roll stronger).
    charge: i32,
    /// `_a0`: frames since the latest restart, up to `RollingReStartForce`.
    force: i32,
    /// `_90`: restarts so far, up to two.
    restarts: i32,
    /// `_94`: frames in the air, for the landing reaction.
    air_frames: i32,
    /// The slope slide control: the ground it last saw, the way down it and
    /// the horizontal speed it left.
    ground_normal: Vec3,
    dir_slide: Vec3,
    horizontal_speed: f32,
    /// The roll's own effect (dust under the roll) is emitting, and was
    /// called this step: `exeRolling` emits it each step on the ground and
    /// deletes it in the air; `exeStart` and the stand-up delete it once.
    effect_on: bool,
    effect_called: bool,
}
impl Default for RollControl {
    fn default() -> Self {
        Self {
            nerve: RollNerve::Start,
            steps: 0,
            frames: 0,
            charge: 0,
            force: 0,
            restarts: 0,
            air_frames: 0,
            ground_normal: Vec3::Y,
            dir_slide: Vec3::ZERO,
            horizontal_speed: 0.,
            effect_on: false,
            effect_called: false,
        }
    }
}

/// `al::converge` on integers.
fn converge_int(value: i32, target: i32, step: i32) -> i32 {
    if value < target {
        (value + step).min(target)
    } else {
        (value - step).max(target)
    }
}
/// The clip a nerve plays.
fn nerve_clip(nerve: RollNerve) -> &'static str {
    match nerve {
        RollNerve::Start => "RollingStart",
        RollNerve::Rolling | RollNerve::BoostStart => "Rolling",
        // The brake keeps the clip the unrolling started (it never restarts it).
        RollNerve::UnRoll | RollNerve::Brake => "RollingEnd",
        RollNerve::Land => "RollingLand",
        RollNerve::Jump => "JumpBroad",
    }
}

impl Player {
    /// The roll's dust effect is wanted (for the effect tables).
    pub fn roll_effect_on(&self) -> bool {
        matches!(self.mode, Move::Roll | Move::RollEnd | Move::RollJump) && self.rolling.effect_on
    }
    /// The roll called its effect's emit this step.
    pub fn roll_effect_called(&self) -> bool {
        self.roll_effect_on() && self.rolling.effect_called
    }
    /// The roll is in its brake nerve, which shares the unrolling clip.
    pub(super) fn is_roll_brake(&self) -> bool {
        self.mode == Move::Roll && self.rolling.nerve == RollNerve::Brake
    }
    /// The ground's unit normal (up on level ground and in the air).
    fn floor_up(&self) -> Vec3 {
        self.floor_normal().unwrap_or(Vec3::Y)
    }
    /// `rs::isOnGroundSlopeSlideEnd`: on ground flatter than the slide's end angle.
    fn on_slide_end_ground(&self) -> bool {
        self.is_on_ground()
            && self.floor_up().angle_between(Vec3::Y).to_degrees() < t::SLOPE_SLIDE_ANGLE_END
    }
    /// `rs::isCollidedGroundLessAngle`: touching ground flatter than `degrees`.
    fn ground_flatter_than(&self, degrees: f32) -> bool {
        self.grounded && self.floor_up().angle_between(Vec3::Y).to_degrees() <= degrees
    }
    /// Add speed along the facing, on the floor, until the velocity along it
    /// is `speed` (`al::addVelocityToDirection` with the missing amount).
    fn raise_speed_along_front(&mut self, speed: f32) {
        let up = self.floor_up();
        let front = (self.facing - up * self.facing.dot(up)).normalize_or_zero();
        let missing = (speed - self.v.dot(front)).max(0.);
        self.v += front * missing;
    }

    /// Start rolling (`PlayerStateRolling::appear`, then the first nerve).
    pub(super) fn start_roll(&mut self, kind: RollStart) {
        let (nerve, floor_speed, frames, charge, restarts) = match kind {
            RollStart::Normal => (RollNerve::Start, t::ROLL_START, 0, 0, 0),
            RollStart::Boost => (RollNerve::BoostStart, t::ROLL_BOOST, 0, 0, 1),
            RollStart::Landing => (
                RollNerve::Rolling,
                t::ROLL_START,
                t::ROLL_FRAMES_MIN,
                t::ROLL_RESTART_CHARGE,
                1,
            ),
        };
        // The step that chooses the roll also runs the state's `control`, so
        // its counters are one on when the first step reads them.
        self.rolling = RollControl {
            nerve,
            frames: frames + 1,
            charge: charge + 1,
            force: 1,
            restarts,
            ground_normal: self.floor_up(),
            dir_slide: self.slide_direction(),
            ..RollControl::default()
        };
        self.raise_speed_along_front(floor_speed);
        self.change(Move::Roll, nerve_clip(nerve), move |p| p.roll_enter(nerve));
    }
    /// The way down the ground under the feet (`al::calcDirSlide`): zero on
    /// level ground.
    fn slide_direction(&self) -> Vec3 {
        let up = self.floor_up();
        let down = Vec3::NEG_Y - up * Vec3::NEG_Y.dot(up);
        down.try_normalize()
            .filter(|_| down.length() > 1e-4)
            .unwrap_or(Vec3::ZERO)
    }
    /// Choose another nerve of the running state; its first step is next step.
    fn roll_to(&mut self, nerve: RollNerve) {
        self.rolling.nerve = nerve;
        self.rolling.steps = 0;
        // Unrolling keeps the rolling clip's frame (`setAnimFrame`).
        let keep =
            (nerve == RollNerve::UnRoll && self.anim.name == "Rolling").then_some(self.anim.frame);
        self.defer(Move::Roll, nerve_clip(nerve), move |p| {
            p.roll_enter(nerve);
            if let Some(frame) = keep {
                p.anim.frame = frame;
            }
        });
        if nerve == RollNerve::Brake {
            // `exeBrake` starts no clip: the unrolling clip plays on.
            self.nerve.keep_clip();
        }
    }
    /// What a nerve does on its first step besides starting its clip.
    fn roll_enter(&mut self, nerve: RollNerve) {
        match nerve {
            RollNerve::Start => {
                // The hop: the start jump power along the ground's normal.
                self.v += self.floor_up() * t::ROLL_START_JUMP;
                self.grounded = false;
                self.hop_contact = true;
                self.rolling.effect_on = false;
                self.events.push(Fx::RollStart);
            }
            RollNerve::BoostStart => self.events.push(Fx::RollBoost { perfect: true }),
            _ => {}
        }
    }

    /// Rebuild the roll's hidden state from a recorded row (lockstep): the
    /// nerve the clip names, counters from the steps the state has taken, and
    /// the nerve a choice in this row starts in the next step.
    pub(super) fn resume_roll(
        &mut self,
        state_steps: u32,
        next_clip: Option<&'static str>,
        brake: bool,
    ) {
        // A row on the ground reports the pull or the fall the floor already
        // took out of the velocity.
        if self.grounded {
            self.v.y = 0.;
        }
        let clip = next_clip.unwrap_or(self.anim.name);
        let nerve = match clip {
            _ if brake => RollNerve::Brake,
            "RollingStart" => RollNerve::Start,
            "RollingEnd" => RollNerve::UnRoll,
            "RollingLand" => RollNerve::Land,
            _ => RollNerve::Rolling,
        };
        let steps = state_steps as i32;
        self.rolling = RollControl {
            nerve,
            steps: state_steps,
            frames: steps.min(t::ROLL_FRAMES_MIN + 1),
            charge: steps.min(t::ROLL_RESTART_CHARGE + 1),
            force: steps.min(t::ROLL_RESTART_FORCE),
            restarts: 0,
            air_frames: 0,
            ground_normal: self.floor_up(),
            dir_slide: self.slide_direction(),
            horizontal_speed: self.v.with_y(0.).length(),
            effect_on: self.is_on_ground() && self.anim.name == "Rolling",
            effect_called: false,
        };
        if let Some(clip) = next_clip {
            if self.mode == Move::RollEnd {
                self.nerve.queue(clip, |p| p.rolling.effect_on = false);
            } else {
                self.nerve.queue(clip, move |p| p.roll_enter(nerve));
            }
        }
    }

    /// One step of the roll nerves, then `PlayerStateRolling::control`.
    pub(super) fn roll(&mut self, o: &Orders) {
        self.rolling.steps += 1;
        self.rolling.effect_called = false;
        let pressed = o.crouch && o.throw_pressed && self.grounded;
        match self.rolling.nerve {
            RollNerve::Start => self.roll_start_step(),
            RollNerve::Rolling => self.roll_rolling_step(o, pressed),
            RollNerve::BoostStart => self.roll_boost_step(o),
            RollNerve::UnRoll => self.roll_unroll_step(o, pressed),
            RollNerve::Brake => self.roll_brake_step(o, pressed),
            RollNerve::Land => self.roll_land_step(o, pressed),
            // The jump's flight runs as its own state (`roll_jump_step`).
            RollNerve::Jump => {}
        }
        self.roll_control(pressed);
    }
    /// `exeStart`: hop and roll until the start clip ends.
    fn roll_start_step(&mut self) {
        self.roll_move();
        if self.anim_done(clip::ROLLING_START) {
            self.roll_to(RollNerve::Rolling);
        } else if self.roll_slowed() {
            self.roll_to(RollNerve::UnRoll);
        }
    }
    /// `exeBoostStart`: roll on from a pound cancel; free to jump at once,
    /// settling into the rolling loop after the boost's minimum.
    fn roll_boost_step(&mut self, o: &Orders) {
        self.roll_move();
        if self.roll_jump_wanted() {
            self.roll_jump(o);
        } else if self.rolling.steps as i32 > t::ROLL_FRAMES_MIN_BOOST {
            self.roll_to(RollNerve::Rolling);
        }
    }
    /// `exeRolling`.
    fn roll_rolling_step(&mut self, o: &Orders, pressed: bool) {
        self.roll_move();
        self.rolling.effect_on = self.is_on_ground();
        self.rolling.effect_called = self.rolling.effect_on;
        if !o.crouch && self.rolling.frames >= t::ROLL_FRAMES_MIN {
            self.roll_stand_up();
            return;
        }
        if self.roll_jump_wanted() {
            self.roll_jump(o);
            return;
        }
        if pressed && self.rolling.frames >= t::ROLL_RESTART_INTERVAL {
            self.roll_restart();
        } else if self.roll_slowed() {
            self.roll_to(RollNerve::UnRoll);
        }
    }
    /// `exeUnRoll`: the slow roll winds down for a few steps into the brake;
    /// everything that can interrupt a roll still can.
    fn roll_unroll_step(&mut self, o: &Orders, pressed: bool) {
        let rate = t::ROLL_BRAKE;
        self.inertia_slide(rate, 0., 0., 0.);
        self.roll_anim_rate();
        if pressed {
            self.roll_restart();
        } else if self.roll_jump_wanted() {
            self.roll_jump(o);
        } else if !o.crouch && self.rolling.frames >= t::ROLL_FRAMES_MIN {
            self.roll_stand_up();
        } else if self.rolling.steps > t::ROLL_UNROLL_FRAMES {
            self.roll_to(RollNerve::Brake);
        }
    }
    /// `exeBrake`: the slow roll slides on with `SlopeRollingEndBrake` until
    /// it is slow enough to crouch (`EndSquat`, which hands the actor over to
    /// the squat whether or not crouch is still held). It can be interrupted
    /// like the roll before it; leaving the ground ends the state.
    fn roll_brake_step(&mut self, o: &Orders, pressed: bool) {
        let slide = self.inertia_slide(t::ROLL_END_BRAKE, 0., 0., 0.);
        self.roll_anim_rate();
        self.rolling.effect_on = self.grounded;
        self.rolling.effect_called = self.rolling.effect_on;
        if pressed {
            self.roll_restart();
        } else if self.roll_jump_wanted() {
            self.roll_jump(o);
        } else if !self.grounded {
            self.go(Move::Fall, "Fall");
        } else if !o.crouch && self.rolling.frames >= t::ROLL_FRAMES_MIN {
            self.roll_stand_up();
        } else if slide.length() <= t::ROLL_END_BRAKE_END_SPEED {
            self.rolling.effect_on = false;
            self.start_squat();
        }
    }
    /// `exeLand`: the landing pose after the roll's jump.
    fn roll_land_step(&mut self, o: &Orders, pressed: bool) {
        self.roll_move();
        if !o.crouch && self.rolling.frames >= t::ROLL_FRAMES_MIN {
            self.roll_stand_up();
        } else if self.roll_jump_wanted() {
            self.roll_jump(o);
        } else if pressed && self.rolling.frames >= t::ROLL_RESTART_INTERVAL {
            self.roll_restart();
        } else if self.anim_done(clip::ROLLING_LAND) {
            self.roll_to(RollNerve::Rolling);
        } else if self.roll_slowed() {
            self.roll_to(RollNerve::UnRoll);
        }
    }
    /// The roll has slowed to its end speed on flat ground and has rolled long
    /// enough: it winds down (`UnRoll`).
    fn roll_slowed(&self) -> bool {
        self.on_slide_end_ground()
            && self.rolling.horizontal_speed <= t::ROLL_END_SPEED
            && self.rolling.frames >= t::ROLL_FRAMES_MIN
    }
    /// A jump was asked for and the ground is flat enough to take it.
    fn roll_jump_wanted(&self) -> bool {
        self.pre_jump > 0 && self.ground_flatter_than(t::STAND_ANGLE_MIN)
    }
    /// The roll's jump is a long jump (`exeJump`: the long jump's clip, take-off
    /// velocity and flight).
    fn roll_jump(&mut self, _o: &Orders) {
        self.rolling.nerve = RollNerve::Jump;
        self.rolling.steps = 0;
        self.long_jump_as(Move::RollJump);
        // The jump is a nerve of the roll state: its steps keep counting.
        self.nerve.keep_actor_steps();
    }
    /// One step of the roll's jump: the long jump's flight, then the state's
    /// own `control` (the frame count and the restart force run on in the air;
    /// the charge does not).
    pub(super) fn roll_jump_step(&mut self, o: &Orders) {
        self.rolling.steps += 1;
        self.rolling.effect_called = false;
        if self.rolling.steps == 1 {
            // `exeJump`'s first step: the rolling effect stops, the charge empties.
            self.rolling.effect_on = false;
            self.rolling.charge = 0;
        }
        self.long_jump_air(o);
        self.roll_control(false);
    }
    /// The roll's jump came down with crouch held on ground the roll can go
    /// on over (`rs::isOnGroundRunAngle`). Guess: only the flat-ground end of
    /// that test, whose angle limit depends on the speed, is ported.
    pub(super) fn roll_goes_on_after_jump(&self, crouch: bool) -> bool {
        crouch && self.ground_flatter_than(t::STAND_ANGLE_MIN)
    }
    /// The roll goes on in its landing nerve with the charge full and the
    /// frame count at the restart interval (`exeJump`; the landing reaction is
    /// raised there too). Called with the jump's last step already run.
    pub(super) fn roll_land_from_jump(&mut self) {
        self.roll_control(false);
        // The step that chooses the nerve also runs `control` once more.
        self.rolling.charge = t::ROLL_RESTART_CHARGE + 1;
        self.rolling.frames = t::ROLL_RESTART_INTERVAL + 1;
        self.rolling.ground_normal = self.floor_up();
        self.rolling.dir_slide = self.slide_direction();
        self.roll_to(RollNerve::Land);
    }
    /// `restartRolling`, then the start nerve again.
    fn roll_restart(&mut self) {
        let charged = self.rolling.charge >= t::ROLL_RESTART_CHARGE;
        let allowed = self.rolling.force >= t::ROLL_RESTART_FORCE || charged;
        let count = if allowed {
            self.rolling.restarts = converge_int(self.rolling.restarts, 2, 1);
            self.rolling.force = 0;
            self.events.push(Fx::RollBoost {
                perfect: self.rolling.restarts >= 2,
            });
            self.rolling.restarts as f32
        } else {
            0.
        };
        self.rolling.frames = 0;
        let speed = self.v.with_y(0.).length();
        let ceiling = (t::ROLL_START + count * t::ROLL_RESTART_MAX_ADD).max(speed);
        let target = (speed + count * t::ROLL_RESTART_ACCEL)
            .max(t::ROLL_START)
            .min(ceiling);
        self.raise_speed_along_front(target);
        self.roll_to(RollNerve::Start);
    }
    /// Release into the stand-up (`StandUp` nerve, the mode `RollEnd`).
    fn roll_stand_up(&mut self) {
        self.defer(Move::RollEnd, "RollingStandUp", |p| {
            p.rolling.effect_on = false;
        });
    }
    /// `exeStandUp`: slide on the roll's momentum while standing, then hand
    /// the actor back (`setNerveOnGround`).
    pub(super) fn roll_stand_up_step(&mut self, o: &Orders) {
        self.inertia_slide(t::ROLL_BRAKE, 0., 0., 0.);
        if self.anim_done(clip::ROLLING_STAND_UP) {
            self.end_roll(o);
        }
    }
    /// The roll is over: the actor picks what the ground allows next.
    fn end_roll(&mut self, o: &Orders) {
        if self.pre_jump > 0 {
            self.ground_jump(o);
        } else if o.crouch {
            self.start_squat();
        } else if self.is_move() {
            self.run_frames = t::BRAKE_ON_COUNTER;
            self.go(Move::Run, "Move");
        } else {
            self.change(Move::Idle, "Wait", |p| p.set_h(Vec3::ZERO));
        }
    }

    /// `PlayerStateRolling::control`, after the nerve has run.
    fn roll_control(&mut self, pressed: bool) {
        let grounded = self.is_on_ground();
        let r = &mut self.rolling;
        if r.frames <= t::ROLL_FRAMES_MIN.max(t::ROLL_RESTART_INTERVAL) {
            r.frames += 1;
        }
        // The charge fills outside the jump.
        if r.charge <= t::ROLL_RESTART_CHARGE && r.nerve != RollNerve::Jump {
            r.charge += 1;
        }
        if pressed {
            r.charge = 0;
        }
        r.force = converge_int(r.force, t::ROLL_RESTART_FORCE, 1);
        if matches!(
            r.nerve,
            RollNerve::Start | RollNerve::Rolling | RollNerve::BoostStart
        ) {
            if grounded {
                // Touching down after a long enough flight raises the landing
                // reaction (the original's roll-landing hit reaction).
                let landed = r.air_frames >= t::ROLL_LAND_AIR_FRAMES;
                r.air_frames = 0;
                if landed {
                    self.events.push(Fx::RollLand);
                }
            } else {
                r.air_frames = converge_int(r.air_frames, t::ROLL_LAND_AIR_FRAMES, 1);
            }
        }
    }

    /// One frame of the nerves' movement: the slope slide control, then the
    /// clip's rate follows the speed.
    fn roll_move(&mut self) {
        self.slide_update();
        self.roll_anim_rate();
    }
    /// `updateRollingAnimFrameRate`: the rolling clip plays faster the faster
    /// the roll goes. The original sets the rate before the clip moves on, so
    /// the step's advance is redone with it.
    fn roll_anim_rate(&mut self) {
        if !matches!(self.anim.name, "Rolling" | "RollingEnd") {
            return;
        }
        let up = self.floor_up();
        let horizontal = self.v - up * self.v.dot(up);
        let front = (self.facing - up * self.facing.dot(up)).normalize_or_zero();
        let along = front.dot(horizontal).abs();
        let share = ground::rate01(along, t::ROLL_ANIM_SPEED_MIN, t::ROLL_ANIM_SPEED_MAX);
        let rate = t::ROLL_ANIM_RATE_MIN + (t::ROLL_ANIM_RATE_MAX - t::ROLL_ANIM_RATE_MIN) * share;
        let old = self.anim.rate;
        self.anim.rate = rate;
        // A clip that began this step has not advanced yet.
        if self.anim.frame > 0. {
            let length = clip::loop_length(self.anim.name).unwrap_or(f32::MAX);
            let mut before = self.anim.frame - old;
            if before < 0. {
                before += length;
            }
            self.anim.frame = before + rate;
            self.clip_wrapped = self.anim.frame >= length;
            if self.clip_wrapped {
                self.anim.frame -= length;
            }
        }
    }

    /// `PlayerActionSlopeSlideControl::update` for the roll: on the ground the
    /// horizontal velocity loses `RollingBrake` (down a slope it gains speed
    /// instead) and the stick steers it; in the air gravity pulls. The facing
    /// follows the travel.
    fn slide_update(&mut self) {
        let on_ground = self.is_on_ground();
        let slide_end = self.on_slide_end_ground();
        let mut up = Vec3::Y;
        let stick = self.ground_stick();
        if on_ground {
            up = self.floor_up();
            self.rolling.ground_normal = up;
            self.rolling.dir_slide = self.slide_direction();
            let velocity = self.v;
            let (along, side) = if slide_end {
                // Flat enough: the velocity in the floor's plane brakes, the
                // part along the normal brakes with the side brake.
                let vertical = up * velocity.dot(up);
                let mut plane = velocity - vertical;
                plane *= t::ROLL_BRAKE;
                let side_dir = up.cross(plane.normalize_or_zero()).normalize_or_zero();
                let push = side_dir * (side_dir.dot(stick) * t::ROLL_SIDE_ACCEL);
                air_move::add_vector_limit(&mut plane, push, t::ROLL_MAX);
                (plane, vertical * t::ROLL_SIDE_BRAKE)
            } else {
                let slide = self.rolling.dir_slide;
                let mut down = slide * velocity.dot(slide);
                let mut side = velocity - down;
                let speed_down = down.dot(slide);
                if speed_down < 0. {
                    down *= t::ROLL_BRAKE;
                }
                let against = (-slide.dot(stick)).clamp(0., 1.);
                let least = (1. - against * t::ROLL_AGAINST).clamp(0., 1.);
                let norm = (speed_down + 1.).clamp(least, 1.);
                let angle = slide.angle_between(Vec3::NEG_Y).to_degrees();
                let power = (90. - angle).clamp(0., 90.);
                let power_norm = (power / t::ROLL_ANGLE_POWER_MAX).min(1.);
                air_move::add_vector_limit(
                    &mut down,
                    slide * (t::ROLL_SLOPE_ACCEL * power_norm * norm),
                    t::ROLL_MAX,
                );
                side *= t::ROLL_SIDE_BRAKE;
                if let Some(input) = stick.try_normalize() {
                    let across = up.cross(slide);
                    air_move::add_vector_limit(
                        &mut side,
                        across * (across.dot(input) * stick.length() * t::ROLL_SIDE_ACCEL),
                        t::ROLL_SIDE_MAX,
                    );
                }
                (down, side)
            };
            let velocity = (along + side).clamp_length_max(t::ROLL_MAX);
            self.v = velocity - up * GRAVITY_MOVE;
        } else {
            // Leaving the ground's plane keeps the velocity; gravity pulls.
            self.rolling.ground_normal = up;
            air_move::add_vector_limit(&mut self.v, Vec3::NEG_Y * t::GRAVITY_AIR, t::FALL_MAX);
        }
        let horizontal = self.v - up * self.v.dot(up);
        self.rolling.horizontal_speed = horizontal.length();
        if let Some(heading) = horizontal.try_normalize() {
            // The body turns toward the travel by at most `SlopeTurnDegree`,
            // then swings a share of the way there.
            let front = (self.facing - up * self.facing.dot(up)).normalize_or_zero();
            let angle = front.angle_between(heading);
            let limited = if angle > t::SLOPE_TURN_DEG.to_radians() {
                let turn = t::SLOPE_TURN_DEG.to_radians();
                let sign = if front.cross(heading).y < 0. { -1. } else { 1. };
                Quat::from_rotation_y(sign * turn) * front
            } else {
                heading
            };
            self.swing_front(limited, SLERP_QUAT_RATE);
        }
    }
}
