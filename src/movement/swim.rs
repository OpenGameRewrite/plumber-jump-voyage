//! Swimming: the actor's `Swim` nerve (`PlayerActorHakoniwa::exeSwim`), the state it runs
//! (`PlayerStateSwim` with its nerves `SwimMove`, `SwimSurface`, ...) and the judges around it
//! (`PlayerJudgeInWater`, `PlayerJudgeStartSwimJump`, `al::WaterSurfaceFinder`'s answers).
//!
//! Written from the 1.0.0 executable (the state has no decompilation) and checked against the
//! recordings `swim-*`. The numbers are the built-in `PlayerConst` defaults (`swim_const.rs`);
//! the arithmetic helpers are in `swim_move.rs`.
//!
//! The game runs the state like any other: the actor's nerve `Swim` starts it (`appear` runs in
//! the update that chose the nerve and picks the first inner nerve), every later update runs
//! the inner nerve (`exe*`, one step per update, a nerve set by it starts at once and executes
//! next update) and then the state's `control`.
use super::ground::TurnControl;
use super::swim_const as k;
use super::swim_move::{Stroke, limit_vector_opposite_dir};
use super::vector_lib as lib;
use super::*;

/// The up axis (`-al::getGravity`) and the gravity.
const UP: Vec3 = Vec3::Y;
const GRAVITY: Vec3 = Vec3::NEG_Y;
/// `PlayerActorHakoniwa::updateCollider` asks the surface finder for the surface this far from
/// the root.
const FINDER_DISTANCE: f32 = 200.;
/// `PlayerStateSwim::appear` sets the pre-input counter of the paddle to this (a count that is
/// never reached: nothing has been paddled).
const NEVER_PADDLED: i32 = 9999;
/// Steps after the player is placed during which a clip starts without a blend (a guess, see
/// `swim_anim`).
const PLACED_STEPS: u32 = 10;

/// The inner nerves of `PlayerStateSwim`, by the names the probe logs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) enum SwimNerve {
    /// The state's first nerve: under the surface (and anywhere nothing else applies).
    #[default]
    Move,
    /// At the surface.
    Surface,
    /// Walking on the bottom.
    Walk,
    /// Standing on the bottom after touching it.
    Land,
    /// The stroke that pushes up.
    Paddle,
    /// The wind-up of the water hip drop.
    HipDropStart,
    /// The dive down.
    HipDropLoop,
    /// Landing on the bottom after the dive.
    HipDropLand,
    /// The jump up from the bottom.
    HipDropJump,
    /// The first steps after a fast fall into the water.
    FallIn,
    /// The dive of a hip drop that fell into the water.
    Dive,
}
impl SwimNerve {
    /// The name `<actor nerve>/<state nerve>` the probe logs.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Move => "Swim/SwimMove",
            Self::Surface => "Swim/SwimSurface",
            Self::Walk => "Swim/SwimWalk",
            Self::Land => "Swim/SwimLand",
            Self::Paddle => "Swim/SwimPaddle",
            Self::HipDropStart => "Swim/SwimHipDropStart",
            Self::HipDropLoop => "Swim/SwimHipDropLoop",
            Self::HipDropLand => "Swim/SwimHipDropLand",
            Self::HipDropJump => "Swim/SwimHipDropJump",
            Self::FallIn => "Swim/SwimFallIn",
            Self::Dive => "Swim/SwimDive",
        }
    }
}

/// How the state ended, for the actor to pick what comes next (`isEndSwimJump*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SwimEnd {
    /// The jump out of the water from the surface (`EndSwimJump`).
    Jump,
    /// The jump out of the water at the end of the jump from the bottom (`EndSwimJumpPop`).
    JumpPop,
    /// The jump out of the water from a hip drop's landing near the surface
    /// (`EndSwimJumpHipDrop`).
    JumpHipDrop,
}

/// What `PlayerStateSwim` keeps.
#[derive(Clone, Debug)]
pub(super) struct Swim {
    pub nerve: SwimNerve,
    /// Updates the inner nerve has executed (`al::getNerveStep`).
    pub steps: u32,
    /// The nerve that is executing set another (or itself again) in this update.
    nerve_set: bool,
    /// Steps since the last paddle (`this+0xa0`, counted up by `control`).
    pub since_paddle: i32,
    /// The ground normal, or the up axis in the open water, the nerves lay the velocity on
    /// (`this+0xd8`).
    pub ground_normal: Vec3,
    /// Where the surface was last step (`this+0xe4`), in the water areas' frame.
    surface_at: Vec3,
    /// The counter that fades the surface spring in and out (`this+0xf4`).
    pub damper: i32,
    /// Keeps the surface nerve from letting go (`this+0xfc`).
    pub surface_hold: i32,
    /// A jump from the surface is refused while this runs (`this+0x100`).
    jump_invalid: i32,
    /// `SwimMove`'s timer of the "moving" clip (`this+0xac`).
    move_timer: i32,
    /// The jump judge's pre-input counter (`PlayerJudgeStartSwimJump`).
    start_jump: i32,
    /// Steps without ground the walk, the landing and the paddle tolerate (`this+0xf0`).
    pub air_steps: i32,
    /// The depth the hip drop's wind-up holds (`this+0xb0`).
    pub keep_depth: f32,
    /// The dive's button frames left, steps on ground and steps of slow dive (`this+0xb4`, `0xb8`, `0xbc`).
    pub dive_hold: i32,
    pub land_steps: i32,
    pub slow_steps: i32,
    /// The paddle clip's rate and whether it is asked to restart (`this+0xa4`, `0xa8`).
    pub paddle_rate: f32,
    pub paddle_restart: bool,
    /// `PlayerJudgeOutInWater`: the player took off in the water (or was in it) and has neither
    /// left it nor started to fall since; the jump nerve's water judge waits for that.
    pub(super) out_in_inside: bool,
    /// The water was entered by a hip drop (`EActionTrigger 19`, set by the actor's hip drop
    /// nerve): `appear` picks the dive.
    entered_by_hip_drop: bool,
    /// The body's turn.
    pub turn: TurnControl,
}
impl Default for Swim {
    fn default() -> Self {
        Self {
            nerve: SwimNerve::Move,
            steps: 0,
            nerve_set: false,
            since_paddle: NEVER_PADDLED,
            ground_normal: UP,
            surface_at: Vec3::ZERO,
            damper: 0,
            surface_hold: 0,
            jump_invalid: 0,
            move_timer: 0,
            start_jump: 0,
            air_steps: 0,
            keep_depth: 0.,
            dive_hold: 0,
            land_steps: 0,
            slow_steps: 0,
            paddle_rate: 1.,
            paddle_restart: false,
            out_in_inside: true,
            entered_by_hip_drop: false,
            turn: TurnControl::swim(),
        }
    }
}

/// What `PlayerJudgeInWater` is built to ask: besides the water test, whether the ground below
/// shortens the depth that counts (`this+0x41`).
#[derive(Clone, Copy, Debug)]
pub(super) struct InWaterJudge {
    ground_check: bool,
}
/// The judge the ground, air and water-bound states of the actor ask (`0x410` in the actor).
pub(super) const ACTOR_JUDGE: InWaterJudge = InWaterJudge {
    ground_check: false,
};
/// The judge the swim nerve asks to stay (`0x428`).
pub(super) const SWIM_JUDGE: InWaterJudge = InWaterJudge { ground_check: true };

impl Player {
    // ---- Water ---------------------------------------------------------------------------

    /// `PlayerActorHakoniwa::updateCollider` ends by asking the surface finder, from the root's
    /// final position, for the surface along the up axis within 200.
    pub(super) fn update_water_finder(&mut self, scene: &Course) {
        let at = scene.in_water_frame(self.position_cm());
        self.water_finder = scene.water_areas().update_finder(at, UP, FINDER_DISTANCE);
    }

    /// `rs::isInPuddleHeight`: the water is shallower than `height` (the surface above the
    /// root plus the ground under it).
    pub(super) fn is_in_puddle_height(&self, height: f32) -> bool {
        let finder = &self.water_finder;
        if !finder.found || finder.distance < 0. {
            return false;
        }
        // `PlayerColliderHakoniwa::updateHeightCheck` reports no height at all (zero) for a
        // collider that touches ground; otherwise the distance down to the floor.
        let ground = if self.is_collided_ground() || self.ground_height <= 0. {
            0.
        } else {
            self.ground_height
        };
        finder.distance + ground < height
    }

    /// The material code the actor ends its movement with: the water prefix
    /// (`PlayerAreaChecker::isInWater` at the root while the water is deeper than the swim's
    /// center offset).
    pub(super) fn update_material_code(&mut self, scene: &Course) {
        let in_area = scene
            .water_areas()
            .is_water(scene.in_water_frame(self.position_cm()));
        self.material_water = in_area && !self.is_in_puddle_height(k::CENTER_OFFSET);
    }

    /// `PlayerJudgeInWater::judge`: the root, or the point `SwimCenterOffset` above it, is in a
    /// water area and the water is deeper than a puddle.
    pub(super) fn is_in_water(&self, scene: &Course, judge: InWaterJudge) -> bool {
        let at = scene.in_water_frame(self.position_cm());
        let areas = scene.water_areas();
        let high = Vec3::new(
            at.x - GRAVITY.x * k::CENTER_OFFSET,
            at.y - GRAVITY.y * k::CENTER_OFFSET,
            at.z - GRAVITY.z * k::CENTER_OFFSET,
        );
        if !(areas.is_water(at) || areas.is_water(high)) {
            return false;
        }
        // No surface within reach above the root: deep water.
        if !self.water_finder.found {
            return true;
        }
        let mut depth = k::CENTER_OFFSET;
        if judge.ground_check && self.is_collided_ground() {
            let normal = self.collided_ground_normal();
            if !pose_math::is_parallel_direction(UP, normal, 0.01)
                && pose_math::dot(UP, normal) > 0.
            {
                // The ground is tilted: measure the depth along the slope's own up axis.
                let rise = Quat::from_rotation_arc(UP, normal)
                    * (self.position_cm() - self.collider.ground_position());
                depth -= pose_math::dot(GRAVITY, rise).max(0.) + 5.;
            } else {
                depth -= 5.;
            }
            depth = depth.max(0.);
        } else if judge.ground_check {
            depth = (depth - 5.).max(0.);
        }
        !self.is_in_puddle_height(depth)
    }

    /// The actor's water judge, asked after a state's own step: the player enters the water.
    pub(super) fn judge_in_water(&mut self, scene: &Course) {
        if matches!(self.mode, Move::Swim | Move::Pole) || self.nerve.deferred {
            return;
        }
        if self.actor_name() == "Jump" && !self.out_in_water_judged(scene) {
            return;
        }
        if self.actor_name() != "Jump" && !self.is_in_water(scene, ACTOR_JUDGE) {
            return;
        }
        if self.burning() {
            self.events.push(Fx::Quenched);
        }
        // `PlayerStateHipDrop::isEnableInWater`: from its fall on (not the wind-up) the hip drop
        // sets the dive trigger.
        self.swim.entered_by_hip_drop = matches!(self.mode, Move::PoundLand)
            || self.mode == Move::GroundPound && self.clip() != "HipDropStart";
        self.start_swim(scene);
    }

    /// `rs::updateJudgeAndResult(PlayerJudgeOutInWater)`, which the jump nerve asks: while the
    /// player rises in the water he took off in, nothing is judged; once he has left the
    /// water or turned to fall, the ordinary judge decides.
    fn out_in_water_judged(&mut self, scene: &Course) -> bool {
        if self.swim.out_in_inside {
            self.swim.out_in_inside =
                self.is_in_water(scene, SWIM_JUDGE) && pose_math::dot(self.v, GRAVITY) < 0.;
        }
        !self.swim.out_in_inside && self.is_in_water(scene, ACTOR_JUDGE)
    }

    // ---- The actor's Swim nerve ------------------------------------------------------------

    /// `setNerve(Swim)`: the nerve machine starts the state in the same update that chose it
    /// (`PlayerStateSwim::appear` takes the velocity the old state left and the step's collision
    /// moves the body by what it sets); the first inner nerve executes with the next update.
    fn start_swim(&mut self, scene: &Course) {
        self.mode = Move::Swim;
        self.frame = 0;
        self.nerve.choose_silently(true);
        self.swim_appear(scene);
    }

    /// Set the inner nerve: it is current at once, its first step is the next update's.
    pub(super) fn set_swim_nerve(&mut self, nerve: SwimNerve) {
        self.swim.nerve = nerve;
        self.swim.steps = 0;
        self.swim.nerve_set = true;
    }

    /// `PlayerStateSwim::appear`.
    fn swim_appear(&mut self, scene: &Course) {
        self.swim.start_jump = 0;
        self.swim.since_paddle = NEVER_PADDLED;
        self.swim.jump_invalid = 0;
        self.swim.move_timer = 0;
        self.swim.turn.reset();
        self.swim.ground_normal = if !self.is_just_land() && self.is_collided_ground() {
            self.collided_ground_normal()
        } else {
            UP
        };
        // (The damage swim, trigger 20, skips the reaction; it is not ported.)
        self.try_reaction_water_in(scene);
        // The velocity is laid on the plane of the ground normal: the stroke keeps the speed
        // the player came with (at most `SwimSurfaceSpeedMaxH` less a quarter on the ground),
        // and the vertical part is held between the fall and the rise limits.
        if std::mem::take(&mut self.swim.entered_by_hip_drop) {
            self.set_swim_nerve(SwimNerve::Dive);
            return;
        }
        // A fast fall into water near the surface (not the dive or the head slide, which
        // are not ported) is the fall-in: only the vertical speed is kept without a stick.
        let finder = self.water_finder;
        if !self.is_on_ground_by(self.v)
            && finder.found
            && finder.distance < k::SURFACE_START_DIST
            && pose_math::dot(self.v, GRAVITY) > k::FALL_IN_SPEED
        {
            if !self.is_move() {
                self.v = lib::parallelize_vec(GRAVITY, self.v);
            }
            self.set_swim_nerve(SwimNerve::FallIn);
            return;
        }
        let (vertical, mut horizontal) =
            lib::separate_parallel_vertical(self.swim.ground_normal, self.v);
        if self.is_move() {
            if self.is_on_ground_by(self.v) {
                let mut speed = lib::length(horizontal);
                if speed > k::SURFACE_SPEED_MAX_H {
                    speed *= 0.75;
                    if speed < k::SURFACE_SPEED_MAX_H {
                        speed = k::SURFACE_SPEED_MAX_H;
                    }
                }
                let length = lib::length(horizontal);
                if length > 0. {
                    let scale = speed / length;
                    horizontal = Vec3::new(
                        scale * horizontal.x,
                        scale * horizontal.y,
                        scale * horizontal.z,
                    );
                }
            }
        } else {
            horizontal = Vec3::ZERO;
        }
        let up_speed = -(GRAVITY.y * vertical.y) - GRAVITY.x * vertical.x - GRAVITY.z * vertical.z;
        let up_speed = up_speed.clamp(-k::FALL_SPEED_MAX, k::RISE_SPEED_MAX);
        let lift = Vec3::new(
            GRAVITY.x * -up_speed,
            GRAVITY.y * -up_speed,
            GRAVITY.z * -up_speed,
        );
        self.v = Vec3::new(
            horizontal.x + lift.x,
            horizontal.y + lift.y,
            horizontal.z + lift.z,
        );
        let finder = self.water_finder;
        if finder.found && finder.distance < k::SURFACE_START_DIST {
            if !self.is_on_ground_by(self.v) {
                self.swim.jump_invalid = k::FALL_IN_INVALID_JUMP_FRAME;
            }
            self.set_swim_nerve(SwimNerve::Surface);
        } else {
            self.set_swim_nerve(SwimNerve::Move);
        }
    }

    /// `rs::tryCalcEffectMtxDoubleCheckWaterIn/Out`: whether there is a surface to splash on.
    /// The surface the finder holds counts unless the body stands on the ground; with none in
    /// reach the segment the body moved along since last step is tried for a crossing, from the
    /// swim's center offset above the root and from the root itself.
    fn surface_to_splash(&self, scene: &Course) -> bool {
        if self.water_finder.found {
            return !self.is_collided_ground();
        }
        let here = self.position_cm();
        let (vertical, horizontal) = lib::separate_parallel_vertical(GRAVITY, self.v);
        [k::CENTER_OFFSET, 0.].into_iter().any(|offset| {
            let start = here + UP * offset;
            [vertical, horizontal].into_iter().any(|part| {
                let back = start - part * 1.5;
                let areas = scene.water_areas();
                areas.crosses_surface(scene.in_water_frame(back), scene.in_water_frame(start))
            })
        })
    }

    /// `PlayerStateSwim::tryReactionWaterIn`: the splash and the entry sound; the dive and the
    /// hip drop (action triggers 9, 15, 19 and 29) enter harder.
    fn try_reaction_water_in(&mut self, scene: &Course) {
        if self.surface_to_splash(scene) {
            let strong = self.swim.entered_by_hip_drop;
            self.events.push(Fx::WaterIn { strong });
        }
    }

    /// `PlayerStateSwim::tryReactionWaterOut`: the hit reaction of leaving the water and its
    /// splash.
    fn try_reaction_water_out(&mut self, scene: &Course) {
        if self.surface_to_splash(scene) {
            self.events.push(Fx::WaterOut);
        }
    }

    /// `PlayerStateSwim::isSurface`: the surface is in reach and the swimmer is at it.
    fn swim_at_surface(&self) -> bool {
        let finder = self.water_finder;
        finder.found
            && (self.swim.nerve == SwimNerve::Surface || finder.distance <= k::SURFACE_START_DIST)
    }

    /// `PlayerActorHakoniwa::exeSwim`: the state's update, then what the actor decides.
    pub(super) fn swim(&mut self, o: &Orders, scene: &Course) {
        let ended = self.swim_update(o, scene);
        if ended.is_some() {
            // Every way out of the water raises the reaction first.
            self.try_reaction_water_out(scene);
        }
        if let Some(SwimEnd::Jump) = ended {
            // `EndSwimJump`: the actor leaves the water into a jump. (The swim has its own
            // jump judge: the buffered press the generic judge counted stays until the jump's
            // first step.)
            let buffered = self.pre_jump;
            self.start_standard_jump();
            self.pre_jump = buffered;
            return;
        }
        if let Some(SwimEnd::JumpPop) = ended {
            self.start_swim_pop_jump();
            return;
        }
        if let Some(SwimEnd::JumpHipDrop) = ended {
            self.start_pound_jump();
            return;
        }
        // Still swimming: the wetness is recorded, and the actor asks whether the water is still
        // deep enough.
        if self.swim_at_surface() {
            self.wet.record_water_surface();
        } else {
            self.wet.record_in_water();
        }
        if !self.is_in_water(scene, SWIM_JUDGE) {
            self.try_reaction_water_out(scene);
            self.leave_swim(o);
        }
    }

    /// `EndSwimJumpPop`: the actor leaves the water into a jump with the power of the swim
    /// up (its vertical speed, at least nothing, and `SwimJumpHipDropPopJumpAdd` more, at most
    /// `JumpHipDropPower`); the jump keeps the clip playing and has no extension.
    fn start_swim_pop_jump(&mut self) {
        let speed_up = (-pose_math::dot(self.v, GRAVITY)).max(0.) + k::JUMP_HIP_DROP_POP_JUMP_ADD;
        let power = if speed_up > k::JUMP_HIP_DROP_POWER {
            k::JUMP_HIP_DROP_POWER
        } else {
            speed_up
        };
        self.mode = Move::Jump;
        self.frame = 0;
        self.nerve.choose_keeping_clip(true, move |p| {
            p.prior_v = p.v;
            p.leave_ground(Move::Jump, power, t::JUMP_GRAVITY);
            p.jump_extend = false;
            p.arm_air_control(Move::Jump, power, t::JUMP_GRAVITY);
        });
    }

    /// The player is out of the water (`setNerveOnGround`): GUESS, no recording ends a swim
    /// outside of a jump; the ground states the controller has are chosen the way the other
    /// landings choose them.
    fn leave_swim(&mut self, _o: &Orders) {
        if self.is_collided_ground() {
            self.change(Move::Idle, "Wait", |p| p.set_h(Vec3::ZERO));
        } else {
            self.go(Move::Fall, "Fall");
        }
    }

    // ---- PlayerStateSwim -------------------------------------------------------------------

    /// `NerveStateBase::update`: the inner nerve's step, then `control` unless the state ended.
    fn swim_update(&mut self, o: &Orders, scene: &Course) -> Option<SwimEnd> {
        let first = self.swim.steps == 0;
        self.swim.nerve_set = false;
        let end = match self.swim.nerve {
            SwimNerve::Move => self.exe_swim_move(first, o, scene),
            SwimNerve::Surface => self.exe_swim_surface(first, o, scene),
            SwimNerve::Walk => self.exe_swim_walk(first, o, scene),
            SwimNerve::Land => self.exe_swim_land(first, o, scene),
            SwimNerve::Paddle => self.exe_swim_paddle(first, o, scene),
            SwimNerve::HipDropStart => self.exe_swim_hip_drop_start(first, o, scene),
            SwimNerve::HipDropLoop => self.exe_swim_hip_drop_loop(first, o, scene),
            SwimNerve::HipDropLand => self.exe_swim_hip_drop_land(first, o, scene),
            SwimNerve::HipDropJump => self.exe_swim_hip_drop_jump(first, o, scene),
            SwimNerve::FallIn => self.exe_swim_fall_in(first, o, scene),
            SwimNerve::Dive => self.exe_swim_dive(first, o, scene),
        };
        if end.is_some() {
            return end;
        }
        // The nerve counts a step unless it chose another: that one starts at 0.
        if !self.swim.nerve_set {
            self.swim.steps += 1;
        }
        self.swim_control(o);
        None
    }

    /// `PlayerStateSwim::control`.
    fn swim_control(&mut self, o: &Orders) {
        self.swim.since_paddle = (self.swim.since_paddle + 1).min(9999);
        self.swim.jump_invalid = lib::converge(self.swim.jump_invalid, 0, 1);
        // `PlayerJudgeStartSwimJump::update`: the jump button pre-inputs for a few steps.
        if o.jump {
            self.swim.start_jump = k::SURFACE_PRE_INPUT_JUMP_FRAME;
        }
        self.swim.start_jump = lib::converge(self.swim.start_jump, 0, 1);
    }

    /// `PlayerJudgeStartSwimJump::judge`: a jump button just pressed or pre-input, with the
    /// surface close enough above the root.
    pub(super) fn start_swim_jump_judged(&self, o: &Orders) -> bool {
        let pressed = o.jump || self.swim.start_jump >= 1;
        pressed
            && self.water_finder.found
            && self.water_finder.distance <= k::SURFACE_ENABLE_JUMP_HEIGHT
    }

    /// Start a clip of the swim nerves now (`PlayerAnimator::startAnim`, which leaves a clip
    /// that is already playing alone).
    pub(super) fn swim_anim(&mut self, name: &'static str) {
        if self.anim.name != name {
            self.start_clip(name);
            // GUESS: the probe's teleport clears the animator's blend, and a swimmer placed in
            // the water starts its first clips within the steps right after (the recordings'
            // first rows show no blend).
            if self.age <= PLACED_STEPS {
                self.interpolation_cleared = true;
            }
        }
    }
    pub(super) fn is_swim_anim(&self, name: &str) -> bool {
        self.anim.name == name
    }

    /// `PlayerStateSwim::exeSwimMove`.
    fn exe_swim_move(&mut self, first: bool, o: &Orders, _scene: &Course) -> Option<SwimEnd> {
        if first {
            self.swim.move_timer = 0;
        }
        self.swim_underwater_motion();
        self.swim_turn(1.);
        let finder = self.water_finder;
        if finder.found && finder.distance < k::SURFACE_START_DIST {
            self.set_swim_nerve(SwimNerve::Surface);
            return None;
        }
        // (The trample trigger is not ported.)
        if o.jump {
            self.set_swim_nerve(SwimNerve::Paddle);
            return None;
        }
        if o.crouch_pressed {
            self.set_swim_nerve(SwimNerve::HipDropStart);
            return None;
        }
        let on_ground = self.is_on_ground_by(self.v);
        let moving = self.is_move();
        if on_ground {
            if moving {
                self.swim_walk_velocity();
                self.set_swim_nerve(SwimNerve::Walk);
            } else {
                self.swim_wait_ground(k::GRAVITY, k::FALL_SPEED_MAX, 0., 0.);
                self.set_swim_nerve(SwimNerve::Land);
            }
            return None;
        }
        if moving {
            self.swim.move_timer = 15;
            if !self.is_swim_anim("SwimStandMove") {
                self.swim_anim("SwimStandMove");
            }
        } else if self.is_swim_anim("SwimStand") {
            if self.anim_done(clip::SWIM_STAND) {
                self.swim_anim("SwimStandWait");
            }
        } else if self.swim.move_timer <= 0 {
            self.swim_anim("SwimStandWait");
        }
        // The timer that keeps the moving clip counts down.
        self.swim.move_timer = (self.swim.move_timer - 1).max(0);
        None
    }

    /// `PlayerStateSwim::exeSwimSurface`.
    fn exe_swim_surface(&mut self, first: bool, o: &Orders, scene: &Course) -> Option<SwimEnd> {
        let finder = self.water_finder;
        let here = scene.in_water_frame(self.position_cm());
        if first {
            self.swim_anim("SwimStandSurface");
            self.swim.damper = 0;
            self.swim.surface_at = if finder.found { finder.position } else { here };
        }
        // The nerve lets go when the surface is out of reach and the hold has run out.
        let let_go =
            (!finder.found || finder.distance >= k::SURFACE_END_DIST) && self.swim.surface_hold < 1;
        self.swim_surface_motion(Stroke {
            surface: true,
            ..Stroke::default()
        });
        if finder.found && self.is_move() {
            // The surface dropping since last step pulls the body down with it.
            let delta = Vec3::new(
                finder.position.x - self.swim.surface_at.x,
                finder.position.y - self.swim.surface_at.y,
                finder.position.z - self.swim.surface_at.z,
            );
            let drop = pose_math::dot(delta, UP).min(0.) * 0.1;
            let (unit, _) = pose_math::try_normalize_or_zero(UP);
            self.v = Vec3::new(
                drop * unit.x + self.v.x,
                drop * unit.y + self.v.y,
                drop * unit.z + self.v.z,
            );
        }
        self.swim.surface_at = if finder.found { finder.position } else { here };
        self.swim_turn(1.);

        let mut waiting = false;
        if self.is_move() {
            self.swim.damper = k::SURFACE_DAMPER_START;
            if !self.is_swim_anim("SwimSurfaceNormal") {
                self.swim_anim("SwimSurfaceNormal");
            }
        } else {
            if !self.is_swim_anim("SwimStandSurface") && !self.is_swim_anim("SwimStandWait") {
                self.swim_anim("SwimStandSurface");
            }
            let hold = k::SURFACE_DAMPER_START + k::SURFACE_DAMPER_FRAME;
            if self.is_swim_anim("SwimStandSurface") {
                // On the bottom of shallow water the idle pose stands.
                if self.is_collided_ground()
                    && (!finder.found || finder.distance >= k::SURFACE_START_DIST + 5.)
                {
                    self.swim_anim("SwimStandWait");
                }
            } else if self.is_swim_anim("SwimStandWait")
                && finder.found
                && finder.distance < k::SURFACE_START_DIST
            {
                self.swim_anim("SwimStandSurface");
            }
            waiting = self.is_swim_anim("SwimStandWait") && self.swim.jump_invalid == 0;
            self.swim.damper = lib::converge(self.swim.damper, hold, 1);
        }

        if self.swim.jump_invalid == 0 && self.start_swim_jump_judged(o) {
            self.swim.surface_hold = 0;
            return Some(SwimEnd::Jump);
        }
        if o.crouch_pressed {
            self.swim.surface_hold = 0;
            self.set_swim_nerve(SwimNerve::HipDropStart);
            return None;
        }
        if let_go {
            if self.is_swim_anim("SwimStandWait") && self.is_collided_ground() {
                // Standing on the bottom: the swimmer walks (and the rest of the step still runs).
                self.set_swim_nerve(SwimNerve::Walk);
            } else {
                self.set_swim_nerve(SwimNerve::Move);
                return None;
            }
        }
        if waiting && o.jump {
            self.swim.since_paddle = 0;
            self.v = limit_vector_opposite_dir(self.v, UP, lib::length(self.v));
        }
        if finder.found && finder.distance < k::SURFACE_START_DIST {
            self.swim.surface_hold = 0;
        } else {
            self.swim.surface_hold = lib::converge(self.swim.surface_hold, 0, 1);
        }
        None
    }
}
