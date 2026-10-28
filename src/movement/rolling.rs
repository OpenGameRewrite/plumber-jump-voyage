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
use super::slide_control::{SlideControl, SlideParams};
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
    /// Action trigger 8 selected by the Rolling floor.
    Forced,
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
    /// The slope slide control (`PlayerActionSlopeSlideControl`).
    control: SlideControl,
    /// The roll's own effect (dust under the roll) is emitting, and was
    /// called this step: `exeRolling` emits it each step on the ground and
    /// deletes it in the air; `exeStart` and the stand-up delete it once.
    effect_on: bool,
    effect_called: bool,
    /// The unrolling clip began this step at the rolling clip's frame; the
    /// step's rate update must not advance it again.
    frame_kept: bool,
    /// ForceRolling judge cached by appear (+0xb0).
    forced: bool,
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
            control: SlideControl::default(),
            effect_on: false,
            effect_called: false,
            frame_kept: false,
            forced: false,
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
    /// A new step: the emit call of the last one is over.
    pub(super) fn roll_forget_effect_call(&mut self) {
        self.rolling.effect_called = false;
    }
    /// The roll called its effect's emit this step, also when the step ended
    /// the state (the brake emits before it hands the actor over).
    pub fn roll_effect_called(&self) -> bool {
        self.rolling.effect_called
    }
    /// The roll is in its brake nerve, which shares the unrolling clip.
    pub(super) fn is_roll_brake(&self) -> bool {
        self.mode == Move::Roll && self.rolling.nerve == RollNerve::Brake
    }
    /// The ground's unit normal (up on level ground and in the air).
    fn floor_up(&self) -> Vec3 {
        self.floor_normal().unwrap_or(Vec3::Y)
    }
    /// `rs::isCollidedGroundLessAngle`: touching ground flatter than `degrees`.
    pub(super) fn ground_flatter_than(&self, degrees: f32) -> bool {
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
        // appear calls startHitReactionLandIfLanding(actor, collision, false).
        if self.is_just_land() {
            self.events.push(Fx::Land { hard: false });
        }
        let (nerve, floor_speed, frames, charge, restarts) = match kind {
            RollStart::Normal => (RollNerve::Start, t::ROLL_START, 0, 0, 0),
            RollStart::Boost => (RollNerve::BoostStart, t::ROLL_BOOST, 0, 0, 1),
            RollStart::Landing | RollStart::Forced => (
                RollNerve::Rolling,
                t::ROLL_START,
                t::ROLL_FRAMES_MIN,
                t::ROLL_RESTART_CHARGE,
                1,
            ),
        };
        // `appear` sets the counters in the step that chooses the roll; the
        // state's `control` first runs with the nerve's first step, after it.
        self.rolling = RollControl {
            nerve,
            frames,
            charge,
            force: 0,
            restarts,
            control: self.slide_control_setup(),
            forced: self.on_ground_force_rolling(),
            ..RollControl::default()
        };
        // Trigger 8: appear (0x47aa44..0x47aab8) projects the actor front
        // against the ground normal, except on the first landing frame where
        // calcGroundNormalExceptJustLandOrGravityDir returns -gravity.
        if kind == RollStart::Forced {
            self.v = self.units_per_frame();
            let up = if self.is_just_land() {
                Vec3::Y
            } else {
                self.floor_up()
            };
            let front = pose_math::try_normalize_or_zero(vector_lib::verticalize_vec(
                up,
                self.actor_pose().front(),
            ))
            .0;
            let missing = (floor_speed - pose_math::dot(self.v, front)).max(0.);
            self.v += front * missing;
        } else {
            self.raise_speed_along_front(floor_speed);
        }
        self.change(Move::Roll, nerve_clip(nerve), move |p| p.roll_enter(nerve));
    }
    /// Choose another nerve of the running state; its first step is next step.
    fn roll_to(&mut self, nerve: RollNerve) {
        self.rolling.nerve = nerve;
        self.rolling.steps = 0;
        // Unrolling keeps the rolling clip's frame (`setAnimFrame`), taken
        // after that clip's advance of the step the unrolling starts in. The
        // animator's own counter starts over, which the row shows.
        let keep = (nerve == RollNerve::UnRoll && self.anim.name == "Rolling")
            .then_some((self.anim.frame, self.anim.rate));
        self.defer(Move::Roll, nerve_clip(nerve), move |p| {
            p.roll_enter(nerve);
            if let Some((frame, rate)) = keep {
                let length = clip::loop_length("Rolling").unwrap_or(f32::MAX);
                p.anim.frame = (frame + rate) % length;
                p.rolling.frame_kept = true;
                p.counter_restarted = true;
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
            RollNerve::BoostStart => self.events.push(Fx::RollBoost { again: true }),
            // These nerves stop the rolling effect as their first step.
            RollNerve::UnRoll | RollNerve::Land => self.rolling.effect_on = false,
            _ => {}
        }
    }

    /// Rebuild the roll's hidden state from a recorded row (lockstep): the
    /// nerve the clip names, counters from the steps the state has taken, and
    /// the nerve a choice in this row starts in the next step.
    pub(super) fn resume_roll(
        &mut self,
        state_steps: u32,
        nerve_steps: u32,
        next_clip: Option<&'static str>,
        brake: bool,
    ) {
        // The slope control owns the complete velocity, including its normal
        // component. Collision does not remove it in the original.
        let clip = next_clip.unwrap_or(self.anim.name);
        let nerve = match clip {
            _ if brake => RollNerve::Brake,
            "RollingStart" => RollNerve::Start,
            "RollingEnd" => RollNerve::UnRoll,
            "RollingLand" => RollNerve::Land,
            _ => RollNerve::Rolling,
        };
        // The counters the row does not show, from the steps it does: the actor's state steps
        // (since the roll was entered) and the nerve's (since the latest restart, or since the
        // roll began on a landing, which starts in `Rolling`).
        let steps = state_steps as i32;
        let nerve_rows = nerve_steps as i32;
        let landed_into_rolling = nerve == RollNerve::Rolling && steps == nerve_rows;
        let (frames, charge, restarts) = match nerve {
            _ if landed_into_rolling => {
                // `appear` preset frames, charge and the restart counter (action trigger 8 or
                // 0x11); `control` ran once for each step since.
                (
                    t::ROLL_FRAMES_MIN + nerve_rows,
                    t::ROLL_RESTART_CHARGE + nerve_rows,
                    1,
                )
            }
            RollNerve::Start | RollNerve::Rolling => {
                // `restartRolling` empties the frame count in the step that chooses `Start`
                // and that step's `control` counts it: a restart is one ahead of a start
                // that `appear` chose.
                let rows = nerve_rows
                    + if nerve == RollNerve::Rolling {
                        clip::ROLLING_START as i32 + 1
                    } else {
                        0
                    };
                (
                    rows + i32::from(steps > rows),
                    rows + i32::from(steps > rows),
                    0,
                )
            }
            _ => (steps, steps, 0),
        };
        self.rolling = RollControl {
            nerve,
            steps: nerve_steps,
            frames: frames.min(t::ROLL_FRAMES_MIN + 1),
            charge: charge.min(t::ROLL_RESTART_CHARGE + 1),
            // Boosts since the roll began are not in the row: the force is counted from the start.
            force: if landed_into_rolling {
                nerve_rows
            } else {
                steps
            }
            .min(t::ROLL_RESTART_FORCE),
            restarts,
            air_frames: 0,
            control: SlideControl {
                horizontal_velocity: self.v.with_y(0.),
                ..self.slide_control_setup()
            },
            effect_on: self.is_on_ground() && self.anim.name == "Rolling",
            effect_called: false,
            frame_kept: false,
            // Cache belongs to appear, not the floor under each injected row.
            forced: self.rolling.forced,
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
        // exeRolling (0x47ba50) skips both stand-up and jump judges while
        // appear's ForceRolling cache is set; 0x47bb98 also blocks UnRoll.
        if !o.crouch && !self.rolling.forced && self.rolling.frames >= t::ROLL_FRAMES_MIN {
            self.roll_stand_up();
            return;
        }
        if !self.rolling.forced && self.roll_jump_wanted() {
            self.roll_jump(o);
            return;
        }
        if pressed && self.rolling.frames >= t::ROLL_RESTART_INTERVAL {
            self.roll_restart();
        } else if !self.rolling.forced && self.roll_slowed() {
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
        self.on_ground_slope_slide_end(self.v)
            && self.rolling.control.horizontal_speed() <= t::ROLL_END_SPEED
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
    /// own `control` (only the frame count runs on in the air).
    pub(super) fn roll_jump_step(&mut self, o: &Orders) {
        self.rolling.steps += 1;
        if self.rolling.steps == 1 {
            // `exeJump`'s first step: the rolling effect stops.
            self.rolling.effect_on = false;
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
        self.rolling.control = self.slide_control_setup();
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
                again: self.rolling.restarts >= 2,
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
        } else if self.is_move() || self.auto_run_skate() {
            self.run_frames = t::BRAKE_ON_COUNTER;
            self.pull = GRAVITY_MOVE;
            // GroundMove::appear replaces the stand-up's old vertical pull.
            self.v.y = 0.;
            self.go(Move::Run, "Move");
        } else {
            self.wait_state_appears_after_roll(self.v);
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
        // The jump nerve empties the charge and the restart force on every step and
        // leaves `control` there (no charge, judge or force update, no landing check).
        if r.nerve == RollNerve::Jump {
            r.charge = 0;
            r.force = 0;
            return;
        }
        if r.charge <= t::ROLL_RESTART_CHARGE {
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
        // A clip that began this step has not advanced yet; the unrolling clip
        // took the rolling clip's advance with it.
        if std::mem::take(&mut self.rolling.frame_kept) {
            return;
        }
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
    /// horizontal velocity loses `SlopeRollingBrake` (down a slope it gains speed
    /// instead) and the stick steers it; in the air gravity pulls. The facing
    /// follows the travel.
    fn slide_update(&mut self) {
        let skate = self.on_ground_skate();
        let params = SlideParams {
            accel: if skate { 0.9 } else { t::ROLL_SLOPE_ACCEL },
            brake: if skate { 0.999 } else { t::ROLL_BRAKE },
            against: t::ROLL_AGAINST,
            angle_power_max: t::ROLL_ANGLE_POWER_MAX,
            max_speed: t::ROLL_MAX,
            side_accel: if skate { 0.6 } else { t::ROLL_SIDE_ACCEL },
            side_brake: t::ROLL_SIDE_BRAKE,
            side_max_speed: t::ROLL_SIDE_MAX,
            turn_degree: t::SLOPE_TURN_DEG,
            gravity: t::GRAVITY_AIR,
            rolling: true,
        };
        let mut control = self.rolling.control;
        self.v = self.slide_control_update(&mut control, self.v, &params);
        self.rolling.control = control;
    }
}
