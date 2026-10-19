//! Climbing a pole: `PlayerStatePoleClimb`, the judge that starts it
//! (`PlayerJudgePoleClimb`) and the input it reads, written from the 1.0.0
//! executable (none of them has decompiled source) and confirmed with the
//! recordings `pole-climb` and `pole-top`, which jump into the Cap Kingdom's
//! hexagonal pole, climb it, slide down, go round it, jump off, and climb to
//! the top, turn there and jump off again. The values are `PlayerConst`'s
//! defaults, which the game's data does not override for Mario.
//!
//! * A body in the air (or on a pole-coded floor) that touched a pole-coded
//!   triangle in the last step asks the judge after its step. The pole is
//!   only its collision triangles: `pole_find.rs` reads its face, width and
//!   axis off them, and the body is put on the middle of the face it touched.
//! * The nerves: `Start` (the catch clip, one step unless the stick already
//!   asks for something), `Wait`, `Up` (a 25-step move of 100 along the pole,
//!   17 with the throw button held, eased out), `Down` (1 step, 10 or 15),
//!   `Turn` (round the pole by 40 over 15 steps), `Jump` (the wall jump away
//!   from the pole), and at the top `TopStart`, `TopWait`, `TopTurn` (4
//!   degrees a step), `TopEnd` (back down onto the pole) and `TopJump`.
//! * The stick is read in the body's own axes, not the camera's: up the pole,
//!   down, or round it. A direction is chosen when the stick is held deep and
//!   has turned more than 10 degrees from the last one it chose.
//! * Each step the actor is put where the snap says (`followCollision`),
//!   then the collider runs. While the body hangs on the pole (`isFormPoleClimb`)
//!   it has the pole form: two spheres behind the body that only walls push,
//!   and the push along the pole's face cut (`onCutCollideAffectDir`).
//!
//! Not ported: the swing of motion controls (`PlayerJudgePreInputPoleClimbSwing`
//! and the swing speeds and the down-keep time that go with it), the joint
//! pose of the arms and legs, the tutorials, the angle offset of the slanted
//! codes, the material the pole sounds by, ground-nerve ordering of the judge
//! (it is asked where the air nerves ask it), moving poles, and the
//! trigger of a fire damage jump (`0x1f`).
use super::pole_find::{
    Actor, Fit, Touch, TurnAt, find_from_top_pos, find_move_pos, find_start_pos, find_turn_pos,
    is_climbable, is_parallel, try_normalize, verticalize,
};
use super::snap::Snap;
use super::wall_air::wall_jump_takeoff;
use super::*;
use crate::geometry::{CollisionCodes, TriangleMesh};

/// The single-precision degree factor the game multiplies by.
const DEGREES_TO_RADIANS: f32 = 0.017_453_292;

/// `PlayerConst` defaults the judge reads.
const CATCH_RANGE: f32 = 50.;
const CATCH_RANGE_MAX: f32 = 100.;
const UP_MARGIN: f32 = 40.;
/// `PlayerConst` defaults the state reads.
const CATCH_RANGE_MIN: f32 = 10.;
const INPUT_REPEAT_ANGLE: f32 = 10.;
const INPUT_DEGREE_MOVE: f32 = 50.;
const UP_SPEED: f32 = 100.;
const UP_FRAME: i32 = 25;
const UP_FRAME_FAST: i32 = 17;
const DOWN_SPEED: f32 = 10.;
const DOWN_SPEED_FAST: f32 = 15.;
const DOWN_FRAME: i32 = 1;
const TURN_DIST: f32 = 40.;
const TURN_FRAME: i32 = 15;
const TURN_STOP_FRAME: u32 = 5;
const TOP_START_FRAME: u32 = 10;
const TOP_END_FRAME: i32 = 10;
const TOP_TURN_SPEED: f32 = 4.;
const TOP_END_UNDER_OFFSET_Y: f32 = 60.;
/// `PlayerConst::getCollisionRadius`: the body is moved back this far when
/// the pole lets go of it.
const COLLISION_RADIUS: f32 = 55.;
const STICK_DEEP: f32 = 0.64;

/// Lengths of the pole clips in steps (the game's own `anim_len`).
const POLE_CATCH_CLIP: u32 = 8;
const POLE_CLIMB_CLIP: f32 = 25.;
const HAND_STAND_START_CLIP: u32 = 32;
const HAND_STAND_END_CLIP: u32 = 32;
const HAND_STAND_JUMP_CLIP: u32 = 63;
const WALL_JUMP_CLIP: u32 = 37;

const DOWN: Vec3 = Vec3::NEG_Y;

/// The nerves of `PlayerStatePoleClimb`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum PoleNerve {
    #[default]
    Start,
    Jump,
    TopJump,
    Up,
    Turn,
    TopWait,
    TopTurn,
    TopStart,
    TopEnd,
    Wait,
    Down,
}
impl PoleNerve {
    /// The name the probe logs for the sub-nerve.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Start => "PoleClimb/Start",
            Self::Jump => "PoleClimb/Jump",
            Self::TopJump => "PoleClimb/TopJump",
            Self::Up => "PoleClimb/Up",
            Self::Turn => "PoleClimb/Turn",
            Self::TopWait => "PoleClimb/TopWait",
            Self::TopTurn => "PoleClimb/TopTurn",
            Self::TopStart => "PoleClimb/TopStart",
            Self::TopEnd => "PoleClimb/TopEnd",
            Self::Wait => "PoleClimb/Wait",
            Self::Down => "PoleClimb/Down",
        }
    }
}

/// What the stick asks of the body on the pole (`0x476c18`, the function
/// `appear`, `tryStartClimbMove` and `tryTurnTopOrClimb` share).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StickAsk {
    None,
    Up,
    Down,
    /// Round the pole, toward the body's side axis.
    TurnPositive,
    TurnNegative,
}

/// The state's own bookkeeping.
#[derive(Clone, Debug, Default)]
pub(super) struct PoleControl {
    nerve: PoleNerve,
    /// Steps the nerve has executed (`al::getNerveStep`), and how many times a
    /// nerve has been chosen (to tell a step that chose one).
    steps: u32,
    chosen: u32,
    /// The state has ended (`kill`).
    killed: bool,
    snap: Snap,
    /// `updatePoleDepth`: the pole's width, within the catch range limits.
    depth: f32,
    /// What the top finder measured when the climb reached the top.
    top_width: f32,
    /// Frames of the running up move, and whether it was sped up.
    up_frames: i32,
    up_fast: bool,
    /// The stick pointed down when the pole was caught and has not been moved
    /// since: down then needs the sub-action button.
    ignore_down: bool,
    /// The stick's chosen direction (`_9c`), the raw stick it was taken from
    /// (`_a8`) and the one `ignore_down` watches (`_b8`).
    move_input: Vec2,
    raw_latch: Vec2,
    ignore_latch: Vec2,
    /// Whether the camera reverses the sideways directions is asked when a
    /// sideways input first comes after a move (`_a4`, `_a5`).
    reverse_stale: bool,
    reverse_x: bool,
}
impl PoleControl {
    pub(super) fn nerve(&self) -> PoleNerve {
        self.nerve
    }
    /// `isFormPoleClimb`: the body hangs in the pole's collision form until
    /// the jumps and the top let go of it.
    pub(super) fn is_form(&self) -> bool {
        if self.killed {
            return false;
        }
        match self.nerve {
            PoleNerve::Jump => self.steps < 1,
            PoleNerve::TopJump | PoleNerve::TopWait | PoleNerve::TopTurn => false,
            PoleNerve::TopStart => self.steps < TOP_START_FRAME,
            PoleNerve::TopEnd => self.steps >= 1,
            _ => true,
        }
    }
    /// `isPoleJump`: the body is in the air after letting go, which is when
    /// the actor's air judges apply again.
    pub(super) fn is_pole_jump(&self) -> bool {
        !self.killed
            && match self.nerve {
                PoleNerve::Jump => self.steps >= 1,
                PoleNerve::TopJump => true,
                _ => false,
            }
    }
    /// `getPoleFront`: the direction the body faces into the pole.
    pub(super) fn front(&self) -> Vec3 {
        self.snap.front
    }
}

/// `al::isNearZero(value)` for a vector.
fn is_near_zero(v: Vec3) -> bool {
    v.length_squared() < 1e-6
}

/// `PlayerInput::isSameStickMove`: has the raw stick stayed within `degrees`
/// of `reference`?
fn same_stick(reference: Vec2, stick: Vec2, degrees: f32) -> bool {
    if reference.length_squared() < 1e-6 || stick.length_squared() < 1e-6 {
        return false;
    }
    reference.normalize().dot(stick.normalize()) >= (degrees * DEGREES_TO_RADIANS).cos()
}

impl Player {
    pub(super) fn pole_nerve(&self) -> PoleNerve {
        self.pole.nerve()
    }

    /// The body's own axes (`calcFrontDir`, `calcUpDir`, `calcSideDir`).
    fn pole_actor(&self) -> Actor {
        let front = self.facing;
        let up = Vec3::Y;
        Actor {
            front,
            up,
            side: up.cross(front),
        }
    }

    // ---- The judge -----------------------------------------------------------
    /// Does the actor ask the pole judge in this state? The air nerves, the
    /// ground nerves and the wall states do; the ground pound, swimming, the
    /// slope slide and the burns do not.
    fn pole_judge_applies(&self) -> bool {
        match self.mode {
            Move::Pole => self.pole.is_pole_jump(),
            Move::GroundPound
            | Move::PoundLand
            | Move::Swim
            | Move::SwimDash
            | Move::SlopeSlide
            | Move::BurnJump
            | Move::BurnBounce
            | Move::FireRun => false,
            _ => true,
        }
    }

    /// The actor's pole judge, asked after the state's own step (before the
    /// ledge and the wall judges): a body that touched a pole it can climb
    /// takes hold of it.
    pub(super) fn judge_pole_climb(&mut self, o: &Orders, scene: &Course) {
        if self.nerve.deferred || !self.pole_judge_applies() {
            return;
        }
        let Some(mesh) = scene.catch_mesh() else {
            return;
        };
        let Some(touch) = self.pole_touch() else {
            return;
        };
        let Some(fit) = find_start_pos(&mesh, &touch, CATCH_RANGE, CATCH_RANGE_MAX, UP_MARGIN)
        else {
            return;
        };
        self.start_pole(fit, o);
    }

    /// `PlayerJudgePoleClimb::update` up to the finder: which surface the body
    /// touched, and whether it is moving toward it.
    fn pole_touch(&self) -> Option<Touch> {
        let climbable = |codes: &Option<CollisionCodes>| codes.as_ref().is_some_and(is_climbable);
        let wall = self
            .collider
            .wall
            .as_ref()
            .filter(|_| climbable(&self.touched.wall));
        let ceiling = self
            .collider
            .ceiling
            .as_ref()
            .filter(|_| climbable(&self.touched.ceiling));
        let ground = self
            .collider
            .floor
            .as_ref()
            .filter(|_| climbable(&self.touched.floor));
        if wall.is_none() && ceiling.is_none() && ground.is_none() {
            return None;
        }
        // The direction the body wants to go: where it faces, or while the
        // back jump flies (`EActionTrigger_val30`) where the stick pushes.
        let toward = if self.mode == Move::Backflip {
            if self.raw_stick.length_squared() <= STICK_DEEP {
                return None;
            }
            self.push().normalize_or_zero()
        } else {
            self.facing
        };
        let up = Vec3::Y;
        let standing_elsewhere = (self.grounded || self.collider.floor.is_some()) && ground.is_none();
        let (normal, pos, up) = if let Some(wall) = wall {
            if standing_elsewhere || toward.dot(wall.normal) >= 0. {
                return None;
            }
            (wall.normal, wall.position, up)
        } else if let Some(ceiling) = ceiling {
            if standing_elsewhere {
                return None;
            }
            let across = try_normalize(verticalize(up, ceiling.normal))?;
            if toward.dot(across) >= 0. {
                return None;
            }
            let slide = try_normalize(verticalize(ceiling.normal, DOWN)).unwrap_or(Vec3::ZERO);
            (ceiling.normal, ceiling.position, -slide)
        } else {
            let ground = ground?;
            if is_parallel(toward, ground.normal) || is_parallel(ground.normal, DOWN) {
                return None;
            }
            let slide = try_normalize(verticalize(ground.normal, DOWN)).unwrap_or(Vec3::ZERO);
            (ground.normal, ground.position, -slide)
        };
        // It must be moving into the surface.
        let heading = try_normalize(self.v)?;
        if heading.dot(normal) >= 0. {
            return None;
        }
        Some(Touch { normal, pos, up })
    }

    /// `PlayerStatePoleClimb::setup`, `appear` and the actor's `setNerve`: the
    /// body goes to the pole's face, faces the pole and starts the catch clip,
    /// all in the step that found the pole.
    fn start_pole(&mut self, fit: Fit, o: &Orders) {
        let mut pole = PoleControl::default();
        pole.depth = fit.width.clamp(CATCH_RANGE_MIN, CATCH_RANGE_MAX);
        pole.snap.setup(fit.pos, fit.front, fit.axis);
        // `PlayerActionCollisionSnap::start`: the actor goes to the point and
        // turns to face, standing still, with the collision forgotten.
        pole.snap.previous = fit.pos;
        pole.snap.held = fit.pos;
        pole.reverse_stale = true;
        self.pos = fit.pos * UNIT;
        self.facing = fit.front.with_y(0.).normalize_or(self.facing);
        self.v = Vec3::ZERO;
        self.grounded = false;
        self.pole = pole;
        self.reset_collision();
        self.air_for = None;
        self.jump_extend = false;
        self.dive_possible = true;
        self.air_throw_used = false;
        self.wall_history = WallHistory::default();
        // A stick that already points down when the pole is caught is ignored
        // until it is moved (`_b4`).
        let deep = self.raw_stick.length_squared() > STICK_DEEP;
        let input = if deep {
            Vec2::new(-self.raw_stick.x, self.raw_stick.y)
        } else {
            Vec2::ZERO
        };
        if deep {
            self.pole.ignore_latch = self.raw_stick;
        }
        let actor = self.pole_actor();
        if self.input_kind(input, &actor, false, o) == StickAsk::Down {
            self.pole.ignore_down = true;
        }
        self.change(Move::Pole, "PoleCatch", |p| p.events.push(Fx::Pole));
        // The state's `appear` starts its clip itself.
        self.nerve.keep_clip();
        self.start_clip("PoleCatch");
    }

    /// `rs::resetCollision`: the contacts of the last step are forgotten and
    /// the collider's own pose becomes the actor's.
    fn reset_collision(&mut self) {
        self.collider.floor = None;
        self.collider.wall = None;
        self.collider.ceiling = None;
        self.touched.wall_contact = None;
        self.catch_collider_at = None;
    }

    // ---- The stick -----------------------------------------------------------
    /// The function `appear`, `tryStartClimbMove` and `tryTurnTopOrClimb` share:
    /// the stick (as `calcPoleMoveInput` turned it: x mirrored) in the body's
    /// axes. Within 50 degrees of up it asks up, within 50 of down down (which
    /// needs the sub-action button while the catch's down is still held), else
    /// it asks to go round.
    fn input_kind(&self, input: Vec2, actor: &Actor, ignore_down: bool, o: &Orders) -> StickAsk {
        let wish = actor.side * input.x + actor.up * input.y;
        if is_near_zero(wish) {
            return if o.crouch {
                StickAsk::Down
            } else {
                StickAsk::None
            };
        }
        let limit = (INPUT_DEGREE_MOVE * DEGREES_TO_RADIANS).cos();
        if actor.up.dot(wish) > limit {
            return StickAsk::Up;
        }
        if -actor.up.dot(wish) > limit {
            return if ignore_down && !o.crouch {
                StickAsk::None
            } else {
                StickAsk::Down
            };
        }
        if actor.side.dot(wish) > 0. {
            StickAsk::TurnPositive
        } else {
            StickAsk::TurnNegative
        }
    }

    /// `PlayerInput::isPoleMoveInputReverseX`: the body faces toward the
    /// camera, so the sides swap.
    fn pole_input_reversed(&self, actor: &Actor) -> bool {
        // The camera looks along its yaw; its back is the opposite.
        let back = -Vec3::new(self.camera_yaw.sin(), 0., self.camera_yaw.cos());
        let along_front = actor.front.dot(back);
        let along_side = actor.side.dot(back);
        along_front > 0. && along_front.abs() > along_side.abs()
    }

    /// The first part of `PlayerStatePoleClimb::update`: the stick's chosen
    /// direction is refreshed when it turned more than 10 degrees.
    fn update_pole_input(&mut self) {
        let raw = self.raw_stick;
        let deep = raw.length_squared() > STICK_DEEP;
        if deep {
            if !same_stick(self.pole.raw_latch, raw, INPUT_REPEAT_ANGLE) {
                self.pole.move_input = Vec2::new(-raw.x, raw.y);
                self.pole.raw_latch = raw;
            }
        } else {
            self.pole.move_input = Vec2::ZERO;
            self.pole.raw_latch = Vec2::ZERO;
        }
        if self.pole.ignore_down {
            if !deep {
                self.pole.ignore_latch = Vec2::ZERO;
                self.pole.ignore_down = false;
            } else if !same_stick(self.pole.ignore_latch, raw, INPUT_REPEAT_ANGLE) {
                self.pole.ignore_latch = raw;
                self.pole.ignore_down = false;
            }
        }
    }

    // ---- The nerves ----------------------------------------------------------
    /// One step of the state (`PlayerStatePoleClimb::update`, which runs the
    /// nerve), then the nerve's step count moves on (a nerve that was chosen in
    /// the step starts its own count at the next one).
    pub(super) fn pole_climb(&mut self, o: &Orders, scene: &Course) {
        let mesh = scene.catch_mesh();
        let mesh = mesh.as_deref();
        if self.nerve.step == 0 {
            // `exePoleClimb`'s first step.
            self.wall_history = WallHistory::default();
        }
        self.update_pole_input();
        let chosen = self.pole.chosen;
        match self.pole.nerve {
            PoleNerve::Start => self.exe_pole_start(o, mesh),
            PoleNerve::Wait => self.exe_pole_wait(o, mesh),
            PoleNerve::Up => self.exe_pole_up(o, mesh),
            PoleNerve::Down => self.exe_pole_down(o, mesh),
            PoleNerve::Turn => self.exe_pole_turn(o, mesh),
            PoleNerve::Jump | PoleNerve::TopJump => self.exe_pole_jump(o),
            PoleNerve::TopStart => self.exe_top_start(o, mesh),
            PoleNerve::TopWait => self.exe_top_wait(o, mesh),
            PoleNerve::TopTurn => self.exe_top_turn(o, mesh),
            PoleNerve::TopEnd => self.exe_top_end(mesh),
        }
        if self.pole.chosen == chosen {
            self.pole.steps += 1;
        }
    }

    /// The nerve changes to `nerve` at the end of the step (`al::setNerve`):
    /// its first step is the next one, and it starts `clip` then unless that
    /// is the clip already playing (`keep`).
    fn set_pole_nerve(&mut self, nerve: PoleNerve, clip: &'static str, keep: bool) {
        self.pole.nerve = nerve;
        self.pole.steps = 0;
        self.pole.chosen += 1;
        self.sub_restarted = true;
        let playing = keep && self.clip() == clip;
        self.defer(Move::Pole, clip, |_| {});
        if playing {
            self.nerve.keep_clip();
        }
    }

    /// `PlayerActionCollisionSnap::followCollision`: the actor is put on the
    /// snap's pose. False when the snap's parts are gone (never, for static
    /// collision).
    fn pole_follow(&mut self) -> bool {
        let fix = self.collider.fix_reaction();
        let pose = self.pole.snap.follow(self.pos / UNIT, fix);
        self.pos = pose.pos * UNIT;
        self.facing = pose.front();
        true
    }

    /// The jump pressed in time (`judgeAndResetReturnTrue`).
    fn pole_jump_pressed(&mut self) -> bool {
        std::mem::take(&mut self.pre_jump) > 0
    }

    /// The nerves' common step: the snap's move and the follow, then the pose
    /// holds. Returns false when the state ended.
    fn pole_hold(&mut self) -> bool {
        self.pole.snap.update_move();
        self.pole_follow()
    }

    /// `PlayerStatePoleClimb::kill`: the state ends and the actor's ground
    /// nerve takes over (`setNerveOnGround`). While the body hangs on the pole
    /// it is moved out of it first, the pole's radius and 10 back.
    fn kill_pole(&mut self) {
        if self.pole.is_form() {
            self.pos -= self.facing * ((COLLISION_RADIUS + 10.) * UNIT);
            self.reset_collision();
        }
        self.pole.killed = true;
        self.hold_to_ground();
    }

    fn exe_pole_start(&mut self, o: &Orders, mesh: Option<&TriangleMesh>) {
        self.pole_follow();
        if self.try_start_climb_move(o, mesh, StickAsk::None).0 {
            return;
        }
        if !self.anim_done(POLE_CATCH_CLIP) {
            self.set_pole_nerve(PoleNerve::Wait, "PoleWait", false);
        }
    }

    fn exe_pole_wait(&mut self, o: &Orders, mesh: Option<&TriangleMesh>) {
        self.pole_follow();
        if self.pole_jump_pressed() {
            self.set_pole_nerve(PoleNerve::Jump, "WallJump", false);
        } else {
            self.try_start_climb_move(o, mesh, StickAsk::None);
        }
    }

    fn exe_pole_up(&mut self, o: &Orders, mesh: Option<&TriangleMesh>) {
        if self.pole.steps == 0 {
            // The move's length tells how it was asked for; the clip runs
            // once over it.
            let frames = self.pole.snap.frames;
            self.pole.up_frames = frames;
            self.pole.up_fast = frames == UP_FRAME_FAST;
            self.anim.rate = POLE_CLIMB_CLIP / (frames - 1) as f32 * 0.999;
            self.pole.reverse_stale = true;
        }
        self.pole_hold();
        if self.pole_jump_pressed() {
            self.set_pole_nerve(PoleNerve::Jump, "WallJump", false);
            return;
        }
        if self.pole.snap.frames == self.pole.snap.at {
            if !self.try_start_climb_move(o, mesh, StickAsk::None).0 {
                self.set_pole_nerve(PoleNerve::Wait, "PoleWait", false);
            }
            return;
        }
        if self.try_start_climb_move(o, mesh, StickAsk::Up).0 {
            return;
        }
        // The throw button held speeds the move up while it runs.
        if !self.pole.up_fast && o.throw_held {
            self.pole.up_fast = true;
            let elapsed = self.pole.steps as i32 + 1;
            self.change_up_speed(elapsed, UP_FRAME_FAST);
        }
    }

    /// `changeUpMoveSpeed`: the running move goes on over `frames` for the part
    /// that is left of it, and its clip with it.
    fn change_up_speed(&mut self, elapsed: i32, frames: i32) {
        let done = elapsed as f32 / self.pole.up_frames as f32;
        let left = 1. - done;
        let remaining = frames as f32 * left;
        let whole = remaining as i32;
        let carry = i32::from(remaining >= 0. && whole as f32 != remaining);
        let steps = carry + whole;
        self.pole.snap.restart_move(steps);
        self.anim.frame = done * POLE_CLIMB_CLIP;
        self.anim.rate = left * POLE_CLIMB_CLIP / steps as f32;
        self.pole.up_frames = steps + elapsed;
    }

    fn exe_pole_down(&mut self, o: &Orders, mesh: Option<&TriangleMesh>) {
        if self.pole.steps == 0 {
            self.pole.reverse_stale = true;
        }
        self.pole_hold();
        if self.pole_jump_pressed() {
            self.set_pole_nerve(PoleNerve::Jump, "WallJump", false);
            return;
        }
        if (self.pole.steps as i32) < self.pole.snap.frames - 1 {
            self.try_start_climb_move(o, mesh, StickAsk::Down);
            return;
        }
        let (moved, ask) = self.try_start_climb_move(o, mesh, StickAsk::None);
        if !moved {
            self.set_pole_nerve(PoleNerve::Wait, "PoleWait", false);
        } else if ask == StickAsk::Down
            && self.collider.floor.as_ref().is_some_and(|floor| {
                // The pole stands on ground that is level enough: the slide is over.
                floor.normal.dot(Vec3::Y) >= std::f32::consts::FRAC_1_SQRT_2
            })
        {
            self.start_clip("Fall");
            self.interpolation_cleared = true;
            self.kill_pole();
        }
    }

    fn exe_pole_turn(&mut self, o: &Orders, mesh: Option<&TriangleMesh>) {
        self.pole_hold();
        if self.pole_jump_pressed() {
            // Jumping off during the turn: the move ends at the end that faces
            // more nearly along the way the body leaves.
            let leave = if self.ground_stick() != Vec3::ZERO {
                self.stick
            } else {
                -self.facing
            };
            self.pole_end_move(leave);
            self.set_pole_nerve(PoleNerve::Jump, "WallJump", false);
            return;
        }
        if self.pole.steps >= (TURN_FRAME as u32 + TURN_STOP_FRAME - 1)
            && !self.try_start_climb_move(o, mesh, StickAsk::None).0
        {
            self.set_pole_nerve(PoleNerve::Wait, "PoleWait", false);
        }
    }

    /// `forceMoveEndNearestLeaveDir`: the snap's move ends at once, and the
    /// actor is put on the pose it ends on.
    fn pole_end_move(&mut self, leave: Vec3) {
        if self.pole.snap.frames == 0 {
            self.pole_follow();
            return;
        }
        self.pole.snap.end_move_nearest(leave);
        let pose = self.pole.snap.current;
        self.pos = pose.pos * UNIT;
        self.facing = pose.front();
        self.pole_follow();
        self.reset_collision();
    }

    // ---- The top -------------------------------------------------------------
    fn exe_top_start(&mut self, _o: &Orders, _mesh: Option<&TriangleMesh>) {
        self.pole_hold();
        self.reset_collision();
        if self.pole_jump_pressed() {
            self.set_pole_nerve(PoleNerve::TopJump, "PoleHandStandJump", false);
        } else if self.anim_done(HAND_STAND_START_CLIP) {
            self.set_pole_nerve(PoleNerve::TopWait, "PoleHandStandWait", false);
        }
    }

    fn exe_top_wait(&mut self, o: &Orders, mesh: Option<&TriangleMesh>) {
        self.pole_hold();
        self.reset_collision();
        if self.pole_jump_pressed() {
            self.set_pole_nerve(PoleNerve::TopJump, "PoleHandStandJump", false);
        } else {
            self.try_turn_top_or_climb(o, mesh);
        }
    }

    fn exe_top_turn(&mut self, o: &Orders, mesh: Option<&TriangleMesh>) {
        self.pole_hold();
        self.reset_collision();
        if self.pole_jump_pressed() {
            self.set_pole_nerve(PoleNerve::TopJump, "PoleHandStandJump", false);
        } else if !self.try_turn_top_or_climb(o, mesh) {
            self.set_pole_nerve(PoleNerve::TopWait, "PoleHandStandWait", false);
        }
    }

    fn exe_top_end(&mut self, _mesh: Option<&TriangleMesh>) {
        if self.pole.steps == 0 {
            self.pole.ignore_down = false;
        }
        self.pole_hold();
        self.reset_collision();
        if self.anim_done(HAND_STAND_END_CLIP) {
            self.set_pole_nerve(PoleNerve::Wait, "PoleWait", false);
        }
    }

    /// `tryTurnTopOrClimb`: at the top the stick turns the body on the spot
    /// (left or right) or, held down, takes it back down onto the pole.
    fn try_turn_top_or_climb(&mut self, o: &Orders, mesh: Option<&TriangleMesh>) -> bool {
        let actor = self.pole_actor();
        let ask = self.input_kind(self.pole.move_input, &actor, false, o);
        match ask {
            StickAsk::None | StickAsk::Up => false,
            StickAsk::Down => {
                // Back onto the pole's side, under the top.
                let Some(mesh) = mesh else {
                    return false;
                };
                let trans = self.pos / UNIT;
                let target = trans - actor.front * (self.pole.top_width * 0.5)
                    - actor.up * TOP_END_UNDER_OFFSET_Y;
                let Some(fit) =
                    find_from_top_pos(mesh, &actor, target, self.pole.depth, CATCH_RANGE_MAX)
                else {
                    return false;
                };
                self.pole.depth = fit.width.clamp(CATCH_RANGE_MIN, CATCH_RANGE_MAX);
                let yaw = self.facing.x.atan2(self.facing.z);
                self.pole
                    .snap
                    .move_to(yaw, fit.pos, fit.front, fit.axis, TOP_END_FRAME);
                self.set_pole_nerve(PoleNerve::TopEnd, "PoleHandStandEnd", false);
                true
            }
            StickAsk::TurnPositive | StickAsk::TurnNegative => {
                let speed = if ask == StickAsk::TurnPositive {
                    TOP_TURN_SPEED
                } else {
                    -TOP_TURN_SPEED
                };
                self.pole.snap.turn_front(speed);
                if self.pole.nerve != PoleNerve::TopTurn {
                    self.set_pole_nerve(PoleNerve::TopTurn, "PoleHandStandTurn", false);
                }
                true
            }
        }
    }

    // ---- Moving along the pole -------------------------------------------------
    /// `tryStartClimbMove`: act on what the stick asks. `same` is the kind of
    /// move already running (asking for it again does nothing). Returns
    /// whether a nerve was chosen, and what the stick asked.
    fn try_start_climb_move(
        &mut self,
        o: &Orders,
        mesh: Option<&TriangleMesh>,
        same: StickAsk,
    ) -> (bool, StickAsk) {
        let actor = self.pole_actor();
        let mut ask = self.input_kind(self.pole.move_input, &actor, self.pole.ignore_down, o);
        if ask == StickAsk::Down {
            self.pole.ignore_down = false;
        }
        if ask == StickAsk::None || (same != StickAsk::None && ask == same) {
            return (false, ask);
        }
        if matches!(ask, StickAsk::TurnPositive | StickAsk::TurnNegative) {
            if std::mem::take(&mut self.pole.reverse_stale) {
                self.pole.reverse_x = self.pole_input_reversed(&actor);
            }
            if self.pole.reverse_x {
                ask = if ask == StickAsk::TurnPositive {
                    StickAsk::TurnNegative
                } else {
                    StickAsk::TurnPositive
                };
            }
        }
        let Some(mesh) = mesh else {
            return (false, ask);
        };
        let held = self.pole.snap.held;
        let depth = self.pole.depth;
        let moved = match ask {
            StickAsk::TurnPositive | StickAsk::TurnNegative => {
                let side = if ask == StickAsk::TurnPositive {
                    actor.side
                } else {
                    -actor.side
                };
                let turn = TurnAt {
                    held,
                    target: held + side * TURN_DIST,
                    direction: -side,
                };
                let slack = ((depth - TURN_DIST) * 0.5).max(0.);
                find_turn_pos(mesh, &actor, &turn, depth, slack, CATCH_RANGE_MAX)
                    .map(|fit| (fit, TURN_FRAME, PoleNerve::Turn, false))
            }
            StickAsk::Up | StickAsk::Down => {
                let (speed, frames, nerve) = if ask == StickAsk::Up {
                    let frames = if o.throw_held { UP_FRAME_FAST } else { UP_FRAME };
                    (UP_SPEED, frames, PoleNerve::Up)
                } else {
                    let speed = if o.throw_held || o.crouch {
                        DOWN_SPEED_FAST
                    } else {
                        DOWN_SPEED
                    };
                    (-speed, DOWN_FRAME, PoleNerve::Down)
                };
                find_move_pos(
                    mesh,
                    &actor,
                    held,
                    speed,
                    UP_MARGIN,
                    depth,
                    CATCH_RANGE_MAX,
                )
                .map(|moved| {
                    if moved.top {
                        // The pole ends: the body goes up onto its top, facing
                        // as it did unless that runs along the pole.
                        let mut fit = moved.fit;
                        fit.front = if is_parallel(actor.front, fit.axis) {
                            actor.up
                        } else {
                            actor.front
                        };
                        self.pole.top_width = fit.width;
                        fit.width = depth;
                        (fit, TOP_START_FRAME as i32, PoleNerve::TopStart, false)
                    } else {
                        (moved.fit, frames, nerve, true)
                    }
                })
            }
            StickAsk::None => None,
        };
        let Some((fit, frames, nerve, measured)) = moved else {
            return (false, ask);
        };
        if measured || nerve == PoleNerve::Turn {
            self.pole.depth = fit.width.clamp(CATCH_RANGE_MIN, CATCH_RANGE_MAX);
        }
        let yaw = self.facing.x.atan2(self.facing.z);
        self.pole
            .snap
            .move_to(yaw, fit.pos, fit.front, fit.axis, frames);
        let clip = match nerve {
            PoleNerve::Up => "PoleClimb",
            PoleNerve::Down => "PoleFall",
            PoleNerve::Turn => "PoleTurn",
            _ => "PoleHandStandStart",
        };
        // Up restarts its clip every time, the others keep the one playing.
        self.set_pole_nerve(nerve, clip, nerve != PoleNerve::Up);
        (true, ask)
    }

    // ---- Jumping off -----------------------------------------------------------
    /// The jump nerves: `PlayerStateNormalWallJump` as a child state. The jump
    /// away from the pole (`Jump`) first moves the body back by the collision
    /// radius; the top jump goes the way the body faces.
    fn exe_pole_jump(&mut self, o: &Orders) {
        let top = self.pole.nerve == PoleNerve::TopJump;
        if self.pole.steps == 0 {
            if !top && self.pole.is_form() {
                // `updateLeavePoleTrans`.
                self.pos -= self.facing * (COLLISION_RADIUS * UNIT);
            }
            // `PlayerStateNormalWallJump::exeJump`'s first step: the speed
            // goes along the front (away from the wall for the jump), the body
            // turns that way and the shared air control is set up.
            let away = if top { self.facing } else { -self.facing };
            self.facing = away;
            self.v = away * t::WALL_JUMP_H;
            self.dive_possible = true;
            self.air_throw_used = false;
            self.jump_extend = false;
            self.gravity = t::WALL_JUMP_GRAVITY;
            self.grounded = false;
            if !top {
                self.events.push(Fx::Jump(Move::WallJump));
            }
            self.air.setup(&mut self.v, self.facing, &wall_jump_takeoff());
            self.air_for = Some(self.mode);
            self.jump_power = t::WALL_JUMP_POWER;
        }
        let steer = air_move::Steering {
            front: t::JUMP_ACCEL_FRONT,
            back: t::JUMP_ACCEL_BACK,
            turn: t::JUMP_ACCEL_TURN,
        };
        let push = self.push();
        self.air.update(&mut self.v, push, o.jump_held, &steer);
        self.facing = self.air.turn_facing(self.facing, push);
        let (name, length) = if top {
            ("PoleHandStandJump", HAND_STAND_JUMP_CLIP)
        } else {
            ("WallJump", WALL_JUMP_CLIP)
        };
        if self.anim.name == name && self.anim_done(length) {
            self.start_clip("Fall");
        }
        self.air_actions(o);
    }
}
