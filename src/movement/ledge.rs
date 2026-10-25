//! Hanging from a ledge: `PlayerStateWallCatch`, the judge that starts it
//! (`PlayerJudgeWallCatch`) and the
//! snap that holds the body to the ledge (`PlayerActionCollisionSnap`).
//!
//! Written from the 1.0.0 executable (neither the state nor the `rs::` finders
//! have decompiled source; the judges do) and confirmed with the recordings
//! `ledge-grab`, `ledge-jump` and `ledge-drop`, which jump at the edge of the
//! raised platform in the Cap Kingdom and hang from it. The values are
//! `PlayerConst`'s defaults, which the game's data does not override for Mario.
//!
//! * A falling body that touched a wall in the last step asks the judge
//!   (`PlayerJudgeWallCatch`) after its step. The judge looks from the touched
//!   point on the wall for the floor on top of it (`findWallCatchPos`); the
//!   catch point is where that floor meets the wall, 5 inside it. The body is
//!   put there at once, facing the wall.
//! * `Start` (the catch clip), `Wait`, `MoveLeft`/`MoveRight` (the stick slides
//!   the catch point along the ledge in 10-step moves), `Climb` (stick toward
//!   the wall), `ClimbFast` (jump pressed), `Jump` (jump pressed early) and
//!   `EndFall` (stick away from the wall: the body lets go).
//! * Each step the actor is put where the snap says (`connectPoseMtx`), then
//!   the collider runs with the hanging body shapes: the sphere that hangs in
//!   front of the wall is pushed out of it, but only across the wall's
//!   direction (`onCutCollideAffectDir`), which is what slides it along a
//!   ledge that turns.
//!
//! Not ported: moving collision (the snap's follow of a moving platform and
//! the wall-hit loop of `followCollision`), a top face that is not level (the
//! pose keeps only the facing), the ceiling-check object, the collider's
//! memory of the last push and the `WallCatchNoEntryArea` and
//! `WallClimbBan` areas.
use super::snap::Snap;
use super::*;
use crate::collider::{
    FaceFilter, Strike, arrow_strikes, arrow_strikes_filtered, border_behind, first_strike,
};
use crate::geometry::{CollisionCodes, TriangleMesh};

/// The single-precision degree factor the game multiplies by.
const DEGREES_TO_RADIANS: f32 = 0.017_453_292;

/// `PlayerConst` defaults the judge reads.
const WALL_KEEP_DEGREE: f32 = 60.;
const WALL_CATCH_DEGREE: f32 = 43.;
const WALL_CATCH_HEIGHT_EDGE_TOP: f32 = 120.;
const WALL_CATCH_HEIGHT_BOTTOM: f32 = 150.;
const COLLISION_RADIUS: f32 = 55.;
const COLLISION_RADIUS_STAND: f32 = 45.;

/// `PlayerConst` defaults the state reads.
const MOVE_DEGREE: f32 = 40.;
const CLIMB_DEGREE: f32 = 40.;
const INPUT_REPEAT_ANGLE: f32 = 30.;
const MOVE_SPEED: f32 = 70.;
const MOVE_HEIGHT_RANGE: f32 = 70.;
const MOVE_FRAMES: i32 = 10;
const CLIMB_JUMP_START_FRAME: u32 = 5;
const CLIMB_JUMP_END_FRAME: i32 = 30;
const CLIMB_START_FRAME: u32 = 20;
const CLIMB_GRAVITY: f32 = 15.;
const FALL_JUMP_SPEED: f32 = 12.;
const FALL_SPEED_MAX: f32 = 35.;
const TALL: f32 = 160.;
const CLIMB_JUMP_SPEED_V: f32 = 20.;
const CLIMB_JUMP_SPEED_H: f32 = 3.;
const CLIMB_JUMP_GRAVITY: f32 = 1.5;
const CLIMB_JUMP_INVALID_FRAME: u32 = 12;
const JUMP_MOVE_SPEED_MAX: f32 = 30.;
/// Steps the climbs keep the body on the ledge's hanging form: after them the
/// body is on the ledge and the ordinary collision form comes back.
const CLIMB_HANG_STEPS: u32 = 25;
const CLIMB_FAST_HANG_STEPS: u32 = 15;
/// `isEnableIK`: the steps of the climb and of the fast climb after which the ground pose's
/// inverse kinematics are on.
const IK_AFTER_CLIMB_STEPS: u32 = 40;
const IK_AFTER_CLIMB_FAST_STEPS: u32 = 16;
/// Lengths of the catch clips in steps (the clip is over when its counter
/// reaches the frame).
const START_CLIP: u32 = 55;
const CLIMB_CLIP: u32 = 46;
const CLIMB_FAST_CLIP: u32 = 34;
const CLIMB_JUMP_CLIP: u32 = 30;
/// The frames of the two side-move clips (the rate recorded, 1.0516, is
/// this over 19 steps times 0.999).
const MOVE_CLIP_FRAMES: f32 = 20.;
/// Where the body is when it lets go: the offset from the ledge point
/// (`CollisionRadius` + 10 out, `Tall` down).
const FALL_OUT: f32 = COLLISION_RADIUS + 10.;

/// Steps the fall after a let-go ignores the stick (the literal
/// `PlayerStateFallHakoniwa::appear` stores when the let-go trigger is on).
pub(super) const LET_GO_INPUT_LOCK: u32 = 8;
/// Steps after a let-go the ledge cannot be caught again unless the body has
/// turned from the way it faced (45 degrees).
const RECATCH_LOCK_STEPS: u32 = 15;

const DOWN: Vec3 = Vec3::NEG_Y;

/// What the judge found: the catch point on the ledge and the wall's normal
/// (the ledge top's normal is checked for being level; the pose keeps only
/// the facing).
#[derive(Clone, Copy, Debug)]
pub(super) struct CatchTarget {
    pos: Vec3,
    wall_normal: Vec3,
}

/// The nerves of `PlayerStateWallCatch`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum CatchNerve {
    #[default]
    Start,
    Wait,
    MoveLeft,
    MoveRight,
    Climb,
    ClimbFast,
    Jump,
    EndFall,
}
impl CatchNerve {
    /// The name the probe logs for the sub-nerve.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Start => "WallCatch/Start",
            Self::Wait => "WallCatch/Wait",
            Self::MoveLeft => "WallCatch/MoveLeft",
            Self::MoveRight => "WallCatch/MoveRight",
            Self::Climb => "WallCatch/Climb",
            Self::ClimbFast => "WallCatch/ClimbFast",
            Self::Jump => "WallCatch/Jump",
            Self::EndFall => "WallCatch/EndFall",
        }
    }
}

/// What the stick asks of the hanging body (`calcInputKind`, the function
/// `tryStartClimbFallMove` and `initIgnoreFallInput` share).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StickAsk {
    None,
    Climb,
    LetGo,
    Left,
    Right,
}

/// The state's own bookkeeping.
#[derive(Clone, Debug, Default)]
pub(super) struct WallCatchControl {
    nerve: CatchNerve,
    /// Steps the nerve has executed (`al::getNerveStep`), and how many times a
    /// nerve has been chosen (to tell a step that chose one).
    steps: u32,
    chosen: u32,
    /// Steps spent in `Start` and `Wait` (`_8c`): early jump presses climb
    /// with the climb jump.
    counter: i32,
    /// The stick was held away from the wall when the catch began and has not
    /// been released since: letting go ignores it (`_ad`).
    ignore_let_go: bool,
    /// The stick's direction in the body's axes (side, front) and the raw
    /// stick it was taken from (`_90`, `_98`).
    axes: Vec2,
    raw: Vec2,
    /// The state has ended (`kill`).
    killed: bool,
    /// The snap was moving when the nerve's step began (`_ac`).
    moving: bool,
    snap: Snap,
}
impl WallCatchControl {
    pub(super) fn nerve(&self) -> CatchNerve {
        self.nerve
    }
    /// The state has ended (`kill`).
    pub(super) fn is_killed(&self) -> bool {
        self.killed
    }
    /// `PlayerStateWallCatch::isEnableIK`: the climbs let the ground pose's inverse kinematics
    /// work once they have been going for a while (40 steps of the climb, 16 of the fast one).
    pub(super) fn is_enable_ik(&self) -> bool {
        !self.killed
            && match self.nerve {
                CatchNerve::Climb => self.steps >= IK_AFTER_CLIMB_STEPS,
                CatchNerve::ClimbFast => self.steps >= IK_AFTER_CLIMB_FAST_STEPS,
                _ => false,
            }
    }
    /// `isWallCatchForm`: the body hangs in the ledge's collision form until
    /// the climbs put it on the ledge.
    pub(super) fn is_hanging(&self) -> bool {
        if self.killed {
            return false;
        }
        match self.nerve {
            CatchNerve::Jump => self.steps == 0,
            CatchNerve::Climb => self.steps <= CLIMB_HANG_STEPS,
            CatchNerve::ClimbFast => self.steps <= CLIMB_FAST_HANG_STEPS,
            _ => true,
        }
    }
    /// `getWallCatchFront`: the direction the body faces into the wall.
    pub(super) fn front(&self) -> Vec3 {
        self.snap.front
    }
}

/// Is the wall (or floor) the arrow struck one the codes forbid grabbing
/// (`rs::isActionCodeNoWallGrab`)?
fn no_wall_grab(codes: &CollisionCodes) -> bool {
    matches!(
        codes.wall.as_str(),
        "NoAction" | "NoWallGrab" | "ReflectStickNoWallGrab" | "OnlyWallHitDown"
    ) || matches!(
        codes.floor.as_str(),
        "GrabCeil" | "Pole" | "Needle" | "DamageFire" | "Poison"
    )
}

/// `al::isWallPolygon` with gravity straight down: a normal within 20 degrees
/// of level.
fn is_wall_polygon(normal: Vec3) -> bool {
    normal.length() > 0.001 && normal.y.abs() < 0.342_020_15
}

/// `al::isNearZeroOrGreater(value, 0.001)`.
fn near_zero_or_greater(value: f32) -> bool {
    value >= -0.001
}

/// The limits one search reads (`PlayerConst` values, or others for the move).
#[derive(Clone, Copy, Debug)]
struct Reach {
    keep_degree: f32,
    catch_degree: f32,
    /// How high above the wall point the top may lie, and extra search height.
    edge_top: f32,
    extra: f32,
    bottom: f32,
    radius: f32,
    radius_stand: f32,
}
/// Flags of the common search (`rs::findWallCatchPos` and its relatives pass
/// them as the integer and the two bytes): `stand` picks the clearance above
/// the ledge, `relative` limits the top's height above the body.
#[derive(Clone, Copy, Debug)]
struct Search {
    stand: bool,
    relative: bool,
}

/// `rs::calcExistCollisionBorderWallCatch`: is the ledge top a thin plate, or
/// is there a wall right in front of it that turns the other way?
fn border_exists(mesh: &TriangleMesh, pos: Vec3, top: Vec3, wall: Vec3) -> bool {
    if border_behind(mesh, pos, top) {
        return true;
    }
    arrow_strikes(mesh, pos - wall * 5., wall * 65., false)
        .into_iter()
        .min_by(|a, b| a.distance.total_cmp(&b.distance))
        .is_some_and(|hit| hit.normal.dot(wall) < -0.707_106_77)
}

/// The common search for the ledge top behind a wall face (the function the
/// `rs::findWallCatch*` finders end in): from the face found at `wall_pos`
/// with normal `wall`, find the floor above it, check how it lies and that
/// the body has room, and return its point and normal.
fn find_top(
    mesh: &TriangleMesh,
    trans: Vec3,
    facing: Vec3,
    wall_pos: Vec3,
    wall: Vec3,
    reach: Reach,
    search: Search,
) -> Option<Vec3> {
    // The body must face the wall squarely enough, and the face be a wall.
    let facing_cos = -facing.dot(wall);
    if facing_cos < (reach.keep_degree * DEGREES_TO_RADIANS).cos() || !is_wall_polygon(wall) {
        return None;
    }
    let edge_top = reach.edge_top.max(3.);
    let rise = edge_top + reach.extra.max(3.);
    let down_by_rise = DOWN * rise;
    // The floor above the face: down from 120 above it (inside the wall), then
    // from the full rise, then, for a wall that leans back, along the wall.
    let start = wall_pos - DOWN * edge_top - wall * 5.;
    let mut top = first_strike(mesh, start, down_by_rise, FaceFilter::FloorsOnly);
    if top.is_none() {
        let start = wall_pos - down_by_rise - wall * 2.5;
        top = first_strike(mesh, start, down_by_rise, FaceFilter::FloorsOnly);
    }
    if top.is_none() {
        if near_zero_or_greater(wall.dot(DOWN)) {
            return None;
        }
        let along = (DOWN - wall * DOWN.dot(wall)).try_normalize()?;
        let start = wall_pos - along * rise - wall * 5.;
        top = first_strike(mesh, start, along * rise, FaceFilter::FloorsOnly);
    }
    let Strike {
        position: top_pos,
        normal: top_normal,
        triangle,
        ..
    } = top?;
    if no_wall_grab(mesh.codes_of(triangle)) || border_exists(mesh, top_pos, top_normal, wall) {
        return None;
    }
    // The top must be level enough and not too far above the body.
    let level = -DOWN.dot(top_normal);
    if !near_zero_or_greater(level - (reach.catch_degree * DEGREES_TO_RADIANS).cos()) {
        return None;
    }
    if search.relative && -((top_pos - trans).dot(DOWN)) > reach.edge_top {
        return None;
    }
    // Room above the ledge for the body: three short arrows along and out of
    // the edge, that only walls and ceilings stop.
    let edge = top_normal.cross(wall).try_normalize()?;
    let above = top_pos + top_normal * 5.;
    let clearance_stand = if search.stand {
        reach.radius_stand + 10.
    } else {
        15.
    };
    let along_edge = reach.radius + 25. - 7.;
    for (from, vector) in [
        (above - edge * 25., edge * along_edge),
        (above + edge * 25., -edge * along_edge),
        (above + wall * 10., -wall * clearance_stand),
    ] {
        if !arrow_strikes_filtered(mesh, from, vector, FaceFilter::NotFloors).is_empty() {
            return None;
        }
    }
    // Room below for the hanging body: nothing within `bottom` down, or down
    // the top's own slope when the top leans away from the wall.
    let flat_top = (top_normal - DOWN * top_normal.dot(DOWN)).try_normalize();
    let hang = match flat_top {
        Some(flat) if !near_zero_or_greater(flat.dot(wall)) => -top_normal,
        _ => DOWN,
    };
    if !arrow_strikes(mesh, top_pos + wall * 20., hang * reach.bottom, false).is_empty() {
        return None;
    }
    Some(top_pos)
}

/// `rs::findWallCatchPos`: the body at `trans`, facing `facing` and moving
/// along `velocity`, touches the wall whose face has `wall` as normal at
/// `wall_pos`: is there a ledge on top of it that it can catch? Only a body
/// that is moving down can.
fn find_catch_pos(
    mesh: &TriangleMesh,
    trans: Vec3,
    facing: Vec3,
    velocity: Vec3,
    wall_pos: Vec3,
    wall: Vec3,
) -> Option<CatchTarget> {
    let heading = velocity.normalize_or_zero();
    if heading == Vec3::ZERO || heading.dot(DOWN) <= 0. {
        return None;
    }
    let reach = Reach {
        keep_degree: WALL_KEEP_DEGREE,
        catch_degree: WALL_CATCH_DEGREE,
        edge_top: WALL_CATCH_HEIGHT_EDGE_TOP,
        extra: 0.,
        bottom: WALL_CATCH_HEIGHT_BOTTOM,
        radius: COLLISION_RADIUS,
        radius_stand: COLLISION_RADIUS_STAND,
    };
    let pos = find_top(
        mesh,
        trans,
        facing,
        wall_pos,
        wall,
        reach,
        Search {
            stand: true,
            relative: true,
        },
    )?;
    Some(CatchTarget {
        pos,
        wall_normal: wall,
    })
}

/// `rs::judgeWallCatchClimb`: may the body climb onto the ledge where it
/// hangs? The ceiling must leave room to stand (the controller has no ceiling
/// check object, so it does), and a wall must not stand over the ledge within
/// the body's reach (`radius` plus 5 ahead, at 5 above the ledge).
fn climb_room(mesh: &TriangleMesh, pos: Vec3, front: Vec3, radius: f32) -> bool {
    let start = pos - DOWN * 5. - front * 5.;
    arrow_strikes(mesh, start, front * (radius + 5.), false)
        .into_iter()
        .min_by(|a, b| a.distance.total_cmp(&b.distance))
        .is_none_or(|hit| !is_wall_polygon(hit.normal))
}

/// `rs::findWallCatchMovePos`'s first search (the function it calls twice):
/// the wall at the new place along the ledge. From `start` (100 out from the
/// ledge) moved sideways by `step`, shoot toward the wall 5 below the ledge,
/// and at `height_range` below that when the first misses or bulges out.
/// Returns the face point, its normal and how high the top may lie above the
/// body.
fn find_move_wall(
    mesh: &TriangleMesh,
    start: Vec3,
    step: Vec3,
    toward: Vec3,
    height_range: f32,
) -> Option<(Vec3, Vec3, f32)> {
    let base = start + step;
    let hit_at = |from: Vec3| {
        first_strike(mesh, from, toward, FaceFilter::NotFloors)
            .filter(|hit| !no_wall_grab(mesh.codes_of(hit.triangle)))
    };
    let blocked_at = |from: Vec3| {
        first_strike(mesh, from, toward, FaceFilter::NotFloors)
            .is_some_and(|hit| no_wall_grab(mesh.codes_of(hit.triangle)))
    };
    let (pos, normal, rise) = if let Some(near) = hit_at(base + DOWN * 5.) {
        let rise = (height_range + 5.).max(10.);
        // A lower arrow that strikes much nearer: the wall bulges out below.
        match first_strike(
            mesh,
            base + DOWN * height_range,
            toward,
            FaceFilter::NotFloors,
        ) {
            Some(low) => {
                let along = toward.normalize_or_zero();
                let gap = (near.position - base).dot(along) - (low.position - base).dot(along);
                if gap > toward.length() * 0.25 && !no_wall_grab(mesh.codes_of(low.triangle)) {
                    (low.position, low.normal, height_range * 2.)
                } else {
                    (near.position, near.normal, rise)
                }
            }
            None => (near.position, near.normal, rise),
        }
    } else {
        if blocked_at(base + DOWN * 5.) {
            return None;
        }
        let low = hit_at(base + DOWN * height_range)?;
        (low.position, low.normal, height_range * 2.)
    };
    // The way along the wall must be open at the corner: nothing within 25 of
    // the point (10 out from the wall, 5 along it) in the direction of the move.
    let along = (step - normal * step.dot(normal)).try_normalize();
    if let Some(along) = along {
        let from = pos + normal * 10. - along * 5.;
        if first_strike(mesh, from, along * 25., FaceFilter::NotFloors).is_some() {
            return None;
        }
    }
    Some((pos, normal, rise))
}

/// `rs::findWallCatchMovePos`: the next catch point along the ledge, one
/// `step` of the stick's side away (or half a step when that finds no wall),
/// as the point, the wall's normal and the ledge top's normal.
fn find_catch_move_pos(
    mesh: &TriangleMesh,
    trans: Vec3,
    front: Vec3,
    start: Vec3,
    step: Vec3,
) -> Option<CatchTarget> {
    let reach = Reach {
        keep_degree: WALL_KEEP_DEGREE,
        catch_degree: WALL_CATCH_DEGREE,
        edge_top: 0.,
        extra: 0.,
        bottom: WALL_CATCH_HEIGHT_BOTTOM,
        radius: COLLISION_RADIUS,
        radius_stand: COLLISION_RADIUS_STAND,
    };
    let toward = front * 200.;
    let (wall_pos, wall, rise) = find_move_wall(mesh, start, step, toward, MOVE_HEIGHT_RANGE)
        .or_else(|| find_move_wall(mesh, start, step * 0.5, toward, MOVE_HEIGHT_RANGE))?;
    // A wall facing straight back must lie between a quarter and three
    // quarters of the way along the arrow.
    if front.dot(wall) <= -(0.01f32).cos() {
        let distance = (start - wall_pos).dot(wall);
        if !(50.0..=150.0).contains(&distance) {
            return None;
        }
    }
    let pos = find_top(
        mesh,
        trans,
        front,
        wall_pos,
        wall,
        Reach {
            edge_top: rise,
            ..reach
        },
        Search {
            stand: false,
            relative: true,
        },
    )?;
    Some(CatchTarget {
        pos,
        wall_normal: wall,
    })
}

impl Player {
    pub(super) fn catch_nerve(&self) -> CatchNerve {
        self.catch.nerve()
    }

    /// The states that ask `PlayerJudgeWallCatchInputDir` after their step: the
    /// jump nerve's, the fall, the long jump, the air throw and the wall states.
    fn wall_catch_applies(&self) -> bool {
        if let (Move::Fall, Some(front)) = (self.mode, self.fall_after_ledge)
            && self.state_frames() <= RECATCH_LOCK_STEPS
            && self.facing.dot(front) >= std::f32::consts::FRAC_1_SQRT_2
        {
            return false;
        }
        self.wall_keep_applies()
            || matches!(self.mode, Move::LongJump | Move::WallSlide)
            || (self.mode == Move::Throw && !self.grounded)
    }

    /// The actor's catch judge, asked after the state's own step (before the
    /// wall judge): a falling body at a ledge it can reach starts the hang.
    pub(super) fn judge_wall_catch(&mut self, scene: &Course) {
        if self.nerve.deferred || !self.wall_catch_applies() {
            return;
        }
        let Some(target) = self.catch_target(scene) else {
            return;
        };
        self.start_wall_catch(target);
    }

    /// `PlayerJudgeWallCatch::update`, on the wall the last step touched.
    fn catch_target(&self, scene: &Course) -> Option<CatchTarget> {
        // Touching a wall, not standing, and not a wall that may not be grabbed.
        let contact = self.touched.wall_contact.as_ref()?;
        if self.grounded || self.touched.wall.as_ref().is_some_and(no_wall_grab) {
            return None;
        }
        let facing = if self.mode == Move::Backflip {
            // The back jump's trigger: the stick has to be held deep, and the
            // body is judged as facing where it pushes.
            if self.raw_stick.length_squared() <= 0.64 {
                return None;
            }
            self.push().normalize_or_zero()
        } else {
            self.facing
        };
        let mesh = scene.catch_mesh()?;
        find_catch_pos(
            &mesh,
            self.pos / UNIT,
            facing,
            self.v,
            contact.position,
            contact.normal,
        )
    }

    /// `PlayerStateWallCatch::setup`, `appear` and the actor's `setNerve`: the
    /// body goes to the catch point, faces the wall and starts the catch clip,
    /// all in the step that found the ledge.
    fn start_wall_catch(&mut self, target: CatchTarget) {
        let front = -target.wall_normal;
        let mut catch = WallCatchControl::default();
        catch.snap.setup(target.pos, front, Vec3::Y);
        // `startWallCatch`: put the actor on the point, turned to the wall,
        // standing still.
        catch.snap.previous = target.pos;
        catch.snap.held = target.pos;
        self.pos = target.pos * UNIT;
        self.facing = front.with_y(0.).normalize_or(self.facing);
        self.v = Vec3::ZERO;
        self.grounded = false;
        self.catch = catch;
        self.joints.grab.appear();
        self.catch_collider_at = None;
        // `PlayerStateWallCatch::appear` (0x48fad8): `rs::resetCollisionExpandCheck`.
        self.collider
            .reset_expand_check(self.facing.x.atan2(self.facing.z));
        let ignore = self.input_kind_at_catch();
        self.catch.ignore_let_go = ignore;
        self.air_for = None;
        self.jump_extend = false;
        self.change(Move::Ledge, "WallCatchStart", |p| p.events.push(Fx::Ledge));
        // The state's `appear` starts its clip itself.
        self.nerve.keep_clip();
        self.start_clip("WallCatchStart");
    }

    /// `initIgnoreFallInput`: a stick already held away from the wall is
    /// ignored until it is let go of.
    fn input_kind_at_catch(&mut self) -> bool {
        let ask = self.stick_ask();
        if ask == StickAsk::LetGo {
            true
        } else {
            self.catch.axes = Vec2::ZERO;
            self.catch.raw = Vec2::ZERO;
            false
        }
    }

    /// The nerve changes to `nerve` at the end of the step (`al::setNerve`):
    /// its first step is the next one, and it starts `clip` then unless that
    /// is the clip already playing (a move that goes on keeps its clip).
    fn set_catch_nerve(&mut self, nerve: CatchNerve, clip: &'static str) {
        self.catch.nerve = nerve;
        self.catch.steps = 0;
        self.catch.chosen += 1;
        self.sub_restarted = true;
        let playing = self.clip() == clip;
        self.defer(Move::Ledge, clip, |_| {});
        if playing {
            self.nerve.keep_clip();
        }
    }

    /// One step of the state (`PlayerStateWallCatch::update`, which runs the
    /// nerve), then the nerve's step count moves on (a nerve that was chosen
    /// in the step starts its own count at the next one).
    pub(super) fn wall_catch(&mut self, o: &Orders, scene: &Course) {
        let mesh = scene.catch_mesh();
        let mesh = mesh.as_deref();
        let chosen = self.catch.chosen;
        match self.catch.nerve {
            CatchNerve::Start => self.exe_start(mesh),
            CatchNerve::Wait => self.exe_wait(mesh),
            CatchNerve::MoveLeft | CatchNerve::MoveRight => self.exe_move(mesh),
            CatchNerve::Climb => self.exe_climb(mesh),
            CatchNerve::ClimbFast => self.exe_climb_fast(mesh),
            CatchNerve::Jump => self.exe_jump(o),
            CatchNerve::EndFall => {}
        }
        if self.catch.chosen == chosen {
            self.catch.steps += 1;
        }
    }

    // ---- Following the snap ------------------------------------------------
    /// `PlayerActionCollisionSnap::followCollision` for static collision: the
    /// actor is put on the snap's pose.
    fn follow_snap(&mut self) {
        let pose = self.catch.snap.follow(self.pos / UNIT, Vec3::ZERO);
        self.pos = pose.pos * UNIT;
        self.facing = pose.front();
    }

    /// `PlayerStateWallCatch::followCollision`: false when the hold is lost.
    /// (Moving collision, which the original also follows here, is not ported.)
    fn catch_holds(&mut self) -> bool {
        self.follow_snap();
        if self.catch.killed {
            return true;
        }
        let steps = self.catch.steps;
        let late = match self.catch.nerve {
            CatchNerve::Jump => steps > 0,
            CatchNerve::Climb => steps > CLIMB_HANG_STEPS,
            CatchNerve::ClimbFast => steps > CLIMB_FAST_HANG_STEPS,
            _ => false,
        };
        if late {
            return true;
        }
        let trans = self.pos / UNIT;
        // Ground behind the body, or a ceiling under it, ends the hold.
        if self.grounded
            && let Some(floor) = &self.collider.floor
            && (floor.position - trans).dot(self.facing) < -5.
        {
            return false;
        }
        if let Some(ceiling) = &self.collider.ceiling
            && (ceiling.position - trans).dot(DOWN) > 0.
        {
            return false;
        }
        true
    }

    /// `PlayerStateWallCatch::enableClimb`.
    fn enable_climb(&self, mesh: Option<&TriangleMesh>) -> bool {
        mesh.is_none_or(|mesh| {
            climb_room(mesh, self.pos / UNIT, self.facing, COLLISION_RADIUS_STAND)
        })
    }

    // ---- The stick ---------------------------------------------------------
    /// The body's side (`front` cross gravity) and front axes.
    fn catch_axes(&self) -> (Vec3, Vec3) {
        let front = self.facing;
        (Vec3::new(front.z, 0., -front.x), front)
    }

    /// `calcInputKind` as `tryStartClimbFallMove` and `initIgnoreFallInput`
    /// call it. The stick is read once, when it is first held deep, in the
    /// body's axes; it stays what it was read as until the raw stick turns
    /// more than 30 degrees from where it was.
    fn stick_ask(&mut self) -> StickAsk {
        let raw = self.raw_stick;
        if raw.length_squared() <= 0.64 {
            self.catch.axes = Vec2::ZERO;
            self.catch.raw = Vec2::ZERO;
            return StickAsk::None;
        }
        if self.catch.raw.length() < 0.001 {
            let (side, front) = self.catch_axes();
            let push = self.push().normalize_or_zero();
            self.catch.axes = Vec2::new(side.dot(push), front.dot(push));
            self.catch.raw = raw;
        } else {
            let same = self
                .catch
                .raw
                .normalize_or_zero()
                .dot(raw.normalize_or_zero())
                >= (INPUT_REPEAT_ANGLE * DEGREES_TO_RADIANS).cos();
            if !same {
                self.catch.axes = Vec2::ZERO;
                self.catch.raw = Vec2::ZERO;
                return StickAsk::None;
            }
        }
        let Vec2 { x: side, y: front } = self.catch.axes;
        let climb = (CLIMB_DEGREE * DEGREES_TO_RADIANS).cos();
        let slide = (MOVE_DEGREE * DEGREES_TO_RADIANS).cos();
        if front > climb {
            StickAsk::Climb
        } else if front < -climb {
            StickAsk::LetGo
        } else if side > slide {
            StickAsk::Left
        } else if side < -slide {
            StickAsk::Right
        } else {
            StickAsk::None
        }
    }

    /// `tryStartClimbFallMove`: act on what the stick asks. True when the
    /// state chose something.
    fn try_start_climb_fall_move(&mut self, mesh: Option<&TriangleMesh>) -> bool {
        let ask = self.stick_ask();
        if self.catch.ignore_let_go {
            if ask == StickAsk::LetGo {
                return false;
            }
            if ask == StickAsk::None {
                self.catch.ignore_let_go = self.raw_stick.length_squared() > 0.64;
                return false;
            }
            self.catch.ignore_let_go = false;
        }
        match ask {
            StickAsk::None => false,
            StickAsk::LetGo => {
                self.end_fall_from_wall(mesh);
                true
            }
            StickAsk::Climb => {
                if self.enable_climb(mesh) {
                    self.set_catch_nerve(CatchNerve::Climb, "WallCatchEnd");
                    true
                } else {
                    false
                }
            }
            StickAsk::Left | StickAsk::Right => {
                let moving_now = matches!(
                    self.catch.nerve,
                    CatchNerve::MoveLeft | CatchNerve::MoveRight
                );
                if moving_now && self.catch.moving {
                    return false;
                }
                self.start_catch_move(ask, mesh)
            }
        }
    }

    /// The slide along the ledge the stick asked for.
    fn start_catch_move(&mut self, ask: StickAsk, mesh: Option<&TriangleMesh>) -> bool {
        let Some(mesh) = mesh else {
            return false;
        };
        let (side, front) = self.catch_axes();
        let side = side.normalize_or_zero();
        let step = if ask == StickAsk::Right {
            -side * MOVE_SPEED
        } else {
            side * MOVE_SPEED
        };
        let held = self.catch.snap.held;
        let trans = self.pos / UNIT;
        let Some(target) = find_catch_move_pos(mesh, trans, front, held - front * 100., step)
        else {
            return false;
        };
        let yaw = self.facing.x.atan2(self.facing.z);
        self.catch
            .snap
            .move_to(yaw, target.pos, -target.wall_normal, Vec3::Y, MOVE_FRAMES);
        let nerve = if ask == StickAsk::Left {
            CatchNerve::MoveLeft
        } else {
            CatchNerve::MoveRight
        };
        let clip = if ask == StickAsk::Left {
            "WallCatchMoveL"
        } else {
            "WallCatchMoveR"
        };
        self.set_catch_nerve(nerve, clip);
        true
    }

    /// `updateWallCatchKeep`: the hold, the pre-input jump, the stick.
    fn update_wall_catch_keep(&mut self, mesh: Option<&TriangleMesh>) -> bool {
        if !self.catch_holds() {
            self.end_fall_from_wall(mesh);
            return true;
        }
        if std::mem::take(&mut self.pre_jump) > 0 && self.enable_climb(mesh) {
            self.set_catch_nerve(CatchNerve::ClimbFast, "WallCatchEndFast");
            return true;
        }
        self.catch.counter += 1;
        self.try_start_climb_fall_move(mesh)
    }

    // ---- The nerves --------------------------------------------------------
    fn exe_start(&mut self, mesh: Option<&TriangleMesh>) {
        if !self.catch_holds() {
            self.end_fall_from_wall(mesh);
            return;
        }
        let steps = self.catch.steps;
        if self.pre_jump > 0 && self.enable_climb(mesh) && steps >= CLIMB_JUMP_START_FRAME {
            self.pre_jump = 0;
            if self.catch.counter <= CLIMB_JUMP_END_FRAME {
                self.set_catch_nerve(CatchNerve::Jump, "WallCatchEndJump");
            } else {
                self.set_catch_nerve(CatchNerve::ClimbFast, "WallCatchEndFast");
            }
            return;
        }
        if steps >= CLIMB_START_FRAME && self.try_start_climb_fall_move(mesh) {
            return;
        }
        self.catch.counter += 1;
        if self.anim_done(START_CLIP) {
            self.set_catch_nerve(CatchNerve::Wait, "WallCatch");
        }
    }

    fn exe_wait(&mut self, mesh: Option<&TriangleMesh>) {
        self.update_wall_catch_keep(mesh);
    }

    fn exe_move(&mut self, mesh: Option<&TriangleMesh>) {
        if self.catch.steps == 0 {
            // The clip runs once over the move: its rate makes it end as the
            // move does.
            self.anim.rate = MOVE_CLIP_FRAMES / (2 * self.catch.snap.frames - 1) as f32 * 0.999;
        }
        self.catch.moving = self.catch.snap.moving();
        self.catch.snap.update_move();
        self.joints.grab.ease_down();
        if !self.update_wall_catch_keep(mesh) && !self.catch.moving {
            self.joints.grab.restore();
            self.set_catch_nerve(CatchNerve::Wait, "WallCatch");
        }
    }

    /// The climbs' stand on the ledge: gravity pulls the body onto the floor
    /// there (`rs::waitGround` with `WallClimbGravity`).
    fn wait_ground(&mut self) {
        self.v.y = -CLIMB_GRAVITY.min(FALL_SPEED_MAX);
        self.pull = CLIMB_GRAVITY;
    }

    fn exe_climb(&mut self, mesh: Option<&TriangleMesh>) {
        self.joints.grab.ease_down();
        if !self.climb_holds() {
            self.end_fall_from_wall(mesh);
            return;
        }
        let steps = self.catch.steps;
        if std::mem::take(&mut self.pre_jump) > 0 && self.enable_climb(mesh) {
            if self.catch.counter <= CLIMB_JUMP_END_FRAME {
                self.set_catch_nerve(CatchNerve::Jump, "WallCatchEndJump");
                return;
            }
            if steps <= CLIMB_START_FRAME {
                self.set_catch_nerve(CatchNerve::ClimbFast, "WallCatchEndFast");
                return;
            }
        }
        if steps <= CLIMB_HANG_STEPS {
            if !self.enable_climb(mesh) {
                self.end_fall_from_wall(mesh);
                return;
            }
            if self.ceiling_over_ledge() {
                self.end_climb();
                return;
            }
            if steps == CLIMB_HANG_STEPS {
                self.wait_ground();
            }
        } else {
            self.wait_ground();
        }
        self.catch.counter += 1;
        if self.anim_done(CLIMB_CLIP) {
            self.end_climb();
        }
    }

    fn exe_climb_fast(&mut self, mesh: Option<&TriangleMesh>) {
        // Like `exeClimb`, the fast climb starts every step by easing the hip spring's target
        // down to a fifth (`converge(target, 0.2, 0.2)`).
        self.joints.grab.ease_down();
        if !self.climb_holds() {
            self.end_fall_from_wall(mesh);
            return;
        }
        let steps = self.catch.steps;
        if steps <= CLIMB_FAST_HANG_STEPS {
            if !self.enable_climb(mesh) {
                self.end_fall_from_wall(mesh);
                return;
            }
            if self.ceiling_over_ledge() {
                self.end_climb();
                return;
            }
            if steps == CLIMB_FAST_HANG_STEPS {
                // The body stands upright on the ledge and the pull starts.
                self.wait_ground();
            }
        } else {
            self.wait_ground();
        }
        if self.anim_done(CLIMB_FAST_CLIP) {
            self.end_climb();
        }
    }

    /// `followCollisionClimb`: while the body still hangs it follows the snap
    /// like the other nerves; afterwards it only keeps its facing and the snap
    /// goes on putting it on the point.
    fn climb_holds(&mut self) -> bool {
        let steps = self.catch.steps;
        let hanging = match self.catch.nerve {
            CatchNerve::Climb => steps <= CLIMB_HANG_STEPS,
            _ => steps <= CLIMB_FAST_HANG_STEPS,
        };
        if hanging {
            self.catch_holds()
        } else {
            self.follow_snap();
            true
        }
    }

    /// A ceiling contact over the ledge, above the body, stops the climb.
    fn ceiling_over_ledge(&self) -> bool {
        self.collider
            .ceiling
            .as_ref()
            .is_some_and(|c| (c.position - self.pos / UNIT).dot(DOWN) < 0.)
    }

    /// `endClimb`: the state ends on the ledge and the actor's nerve for the
    /// ground takes over (`setNerveOnGround`).
    fn end_climb(&mut self) {
        self.catch.killed = true;
        self.hold_to_ground();
    }

    /// The first step of `Jump` sets the climb jump off; the nerve is the
    /// shared air control from then on, and the ordinary landing (the contact
    /// flagged by the collision) ends it.
    fn exe_jump(&mut self, o: &Orders) {
        if self.catch.steps == 0 {
            self.v = self.facing * CLIMB_JUMP_SPEED_H;
            let takeoff = air_move::Takeoff {
                speed_limit: JUMP_MOVE_SPEED_MAX,
                base_speed: CLIMB_JUMP_SPEED_H,
                extend_frames: 0,
                power: CLIMB_JUMP_SPEED_V,
                gravity: CLIMB_JUMP_GRAVITY,
                input_lock: CLIMB_JUMP_INVALID_FRAME,
                speed_window: None,
                fall_max: t::FALL_MAX,
            };
            self.air.setup(&mut self.v, self.facing, &takeoff);
            self.gravity = CLIMB_JUMP_GRAVITY;
            self.jump_power = CLIMB_JUMP_SPEED_V;
        }
        let steer = air_move::Steering {
            front: t::JUMP_ACCEL_FRONT,
            back: t::JUMP_ACCEL_BACK,
            turn: t::JUMP_ACCEL_TURN,
        };
        let push = self.push();
        self.air.update(&mut self.v, push, o.jump_held, &steer);
        self.facing = self.air.turn_facing(self.facing, push);
        if self.anim.name == "WallCatchEndJump" && self.anim_done(CLIMB_JUMP_CLIP) {
            self.play("Fall");
        }
    }

    /// `endFallFromWall`: let go. The body moves out and down from the ledge
    /// point, rises at `WallFallJumpSpeed` unless it stands on something, the
    /// fall clip starts at once (and again with the fall's first step) and the
    /// actor's fall nerve takes over.
    fn end_fall_from_wall(&mut self, mesh: Option<&TriangleMesh>) {
        // (The original also stops the offset at the surfaces the body touched
        // last step when it would go through them; the ledge's own wall is
        // behind the offset, so this does not come into play.)
        let _ = mesh;
        let held = self.catch.snap.held;
        let offset = -self.facing * FALL_OUT - Vec3::Y * TALL;
        self.pos = (held + offset) * UNIT;
        self.v = Vec3::Y * if self.grounded { 0. } else { FALL_JUMP_SPEED };
        self.grounded = false;
        self.catch.nerve = CatchNerve::EndFall;
        self.catch.killed = true;
        self.fall_after_ledge = Some(self.facing);
        self.go(Move::Fall, "Fall");
        self.start_clip("Fall");
        self.interpolation_cleared = true;
    }

    /// `setNerveOnGround` for a body that stands where a hold (ledge or pole) ended: the run when the
    /// stick pushes, else the standing state; a jump pressed goes on.
    pub(super) fn hold_to_ground(&mut self) {
        self.dive_possible = true;
        self.gravity = t::GRAVITY_AIR;
        self.air_for = None;
        if self.grounded && self.tilt > 0.05 {
            self.run_frames = t::BRAKE_ON_COUNTER;
            self.v.y = -GRAVITY_MOVE;
            self.pull = GRAVITY_MOVE;
            self.go(Move::Run, "Move");
        } else if self.grounded {
            self.go(Move::Idle, "Wait");
        } else {
            self.go(Move::Fall, "Fall");
        }
    }
}
