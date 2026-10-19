//! The wall states of the air: `PlayerStateWallAir` with its two sub-states,
//! the slide (`PlayerStateNormalWallSlide`, nerves `Keep` and `Slide`) and the
//! wall jump (`PlayerStateNormalWallJump`), and the judge that starts them,
//! `PlayerJudgeWallKeep` with `PlayerWallActionHistory`.
//!
//! Written from the 1.0.0 executable (the slide state has no decompiled
//! source; the wall jump, the judge and the history do) and confirmed with the
//! recordings `wall-slide` and `wall-jump`, which run into the tower wall of
//! the Cap Kingdom plaza. The values are `PlayerConst`'s defaults, which the
//! game's data does not override for Mario.
//!
//! * The jump family and the fall ask the judge after their own step. When the
//!   player falls onto a wall he faces, the slide's `appear` runs in that very
//!   step: he turns to face the wall, his velocity becomes 15 units a frame
//!   into it, and he is pulled to the wall's surface.
//! * `Keep` holds that press for `WallKeepFrame` (3) steps, then both nerves
//!   slide: the pull into the wall, gravity along the wall (`GravityWallSlide`,
//!   up to 15) and a sideways speed that follows the stick along the wall.
//!   `Keep` plays `WallKeep`/`WallKeepReverse` and hands over to `Slide`
//!   (`WallSlideL`/`WallSlideR`) when its clip ends.
//! * Each step the state follows the wall's normal; losing the wall, a wall
//!   that turns too far or the ground ends the state, taking the velocity
//!   into the wall off.
//! * A jump press (`PlayerJudgePreInputJump`) starts the wall jump: 8.6 away
//!   from the wall and 23 up through the shared air control, whose stick is
//!   locked for 25 steps.
use super::*;
use crate::collider::arrow_strikes;
use crate::geometry::{CollisionCodes, TriangleMesh};

/// `WallKeepDegree`: how squarely the player faces, and pushes toward, the
/// wall for the judge.
const WALL_KEEP_DEGREE: f32 = 60.;
/// `WallHeightLowLimit`: nearer the ground than this the judge is off.
const WALL_HEIGHT_LOW_LIMIT: f32 = 120.;
/// `Tall`: how far below the place he left a wall the player has to be before
/// the history lets him hold a wall again.
const TALL: f32 = 160.;
/// `WallFollowAngleH` and `WallFollowAngleV`: how far (degrees) the wall's
/// normal may turn from one step to the next around the vertical and across.
const WALL_FOLLOW_ANGLE_H: f32 = 20.;
const WALL_FOLLOW_ANGLE_V: f32 = 30.;
/// The slide's pull into the wall and its speed limit (the literals of
/// `PlayerStateNormalWallSlide`; the same limit holds gravity along the wall).
const WALL_PRESS: f32 = 15.;
const SLIDE_SPEED_MAX: f32 = 15.;
/// The sideways speed aims at this much of the stick along the wall, and
/// moves toward it by 0.1 a step.
const SIDE_SPEED_PER_STICK: f32 = 3.;
const SIDE_SPEED_STEP: f32 = 0.1;
/// The body pulled to the wall stops this far from it (`CollisionRadius`).
const COLLISION_RADIUS: f32 = 55.;
/// The judge's probe from the head (120 above the feet) toward the wall.
const PROBE_HEIGHT: f32 = 120.;
const PROBE_LENGTH: f32 = 77.;
/// The slide turns to face the wall by this share of the way each step.
const FACE_WALL_RATE: f32 = 0.25;
/// Length of the `WallKeep` clips in steps (13 frames at 30 per second).
const WALL_KEEP_CLIP: u32 = 26;
/// Length of the `WallJump` clip in steps (18.5 frames at 30 per second).
const WALL_JUMP_CLIP: u32 = 37;
/// The speed the wall jump's air control lets the take-off keep (the first
/// argument of `PlayerActionAirMoveControl::setup`).
const WALL_JUMP_SPEED_LIMIT: f32 = 100.;

/// The nerves of `PlayerStateNormalWallSlide`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SlideNerve {
    #[default]
    Keep,
    Slide,
}

/// What the wall slide keeps between steps.
#[derive(Clone, Debug, Default)]
pub(super) struct WallAirControl {
    nerve: SlideNerve,
    /// Steps the nerve has run.
    steps: u32,
    /// The wall's normal as the last step saw it (`mNormalStart`), and the
    /// one the slide moves by, which follows the way the player faces
    /// (`mNormalEnd`).
    normal_start: Vec3,
    normal_end: Vec3,
    /// The speed along the wall the stick asks for (`_58`).
    side_speed: f32,
    /// The player's side was toward the camera when the slide began: the clips
    /// are `WallKeep` and `WallSlideR` then, else the mirrored ones.
    side_to_camera: bool,
}

/// `PlayerWallActionHistory`: where the last wall jump started and where the
/// last slide ended (position in cm and the wall's normal), forgotten on the
/// ground. The judge uses it to keep a player from grabbing the wall he has
/// just left.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct WallHistory {
    jump: Option<(Vec3, Vec3)>,
    leave: Option<(Vec3, Vec3)>,
}

/// A wall the body touched in the last step.
#[derive(Clone, Debug, PartialEq)]
pub struct WallContact {
    /// The face's normal.
    pub normal: Vec3,
    /// The touched point (cm, replay frame).
    pub position: Vec3,
    /// The sphere touched the face itself, not an edge or corner.
    pub at_face: bool,
}

/// `al::converge`: `value` moves toward `target` by at most `step`.
fn converge(value: f32, target: f32, step: f32) -> f32 {
    if (target - value).abs() <= step {
        target
    } else {
        value + step * (target - value).signum()
    }
}

/// The angle (degrees) between two vectors seen along `axis`
/// (`al::calcAngleOnPlaneDegree`, which is signed; only its size is used).
fn angle_on_plane(a: Vec3, b: Vec3, axis: Vec3) -> f32 {
    let flatten = |v: Vec3| v - axis * v.dot(axis);
    let (a, b) = (flatten(a), flatten(b));
    a.cross(b).dot(axis).abs().atan2(a.dot(b)).to_degrees()
}

/// `rs::calcExistCollisionBorder`: is there a face right behind the one the
/// arrow hit, turned the other way (the back of a thin wall)?
fn collision_border_behind(mesh: &TriangleMesh, position: Vec3, normal: Vec3) -> bool {
    arrow_strikes(mesh, position - normal, normal * 2., false)
        .into_iter()
        .min_by(|a, b| a.distance.total_cmp(&b.distance))
        .is_some_and(|hit| hit.normal.dot(normal) <= -(0.01f32).cos())
}

/// What the wall jump asks of the air control (`PlayerActionAirMoveControl::setup`
/// in `PlayerStateNormalWallJump::exeJump`).
pub(super) fn wall_jump_takeoff() -> air_move::Takeoff {
    air_move::Takeoff {
        speed_limit: WALL_JUMP_SPEED_LIMIT,
        base_speed: t::WALL_JUMP_H,
        extend_frames: 0,
        power: t::WALL_JUMP_POWER,
        gravity: t::WALL_JUMP_GRAVITY,
        input_lock: t::WALL_JUMP_NO_INPUT,
        speed_window: None,
        fall_max: t::FALL_MAX,
    }
}

impl WallHistory {
    /// `rs::judgeEnableWallKeepHistory`: may the player grab the wall (its
    /// normal `normal`) at `trans`? Not while he is less than his height below
    /// the place he last left a wall, and not while he is still above the
    /// place he last jumped from by more than the wall's distance from there
    /// allows (a wall facing the same way rules out the whole stretch up to
    /// 750 away sideways, any other up to 1000, one facing the other way only
    /// what is beyond 1500).
    fn enables(&self, trans: Vec3, normal: Vec3) -> bool {
        if let Some((from, _)) = self.leave
            && (trans - from).y > -TALL
        {
            return false;
        }
        let Some((from, jump_normal)) = self.jump else {
            return true;
        };
        let offset = trans - from;
        let sideways = offset.with_y(0.).length();
        let facing = normal.dot(jump_normal);
        let allowed_up = if facing > -0.342_020_15 {
            let (distance, span) = if facing > 0.087_155_74 {
                (750. - sideways, 750.)
            } else {
                (1000. - sideways, 500.)
            };
            ((distance / span).clamp(-1., 1.) - 1.) * 1500. * 0.5
        } else {
            if sideways <= 1500. {
                return true;
            }
            1500. - sideways
        };
        offset.y <= allowed_up
    }
}

impl Player {
    /// The states that ask `PlayerJudgeWallKeep` after their step: the jump
    /// nerve's (all the jumps and flips), the fall, and the wall jump itself.
    pub(super) fn wall_keep_applies(&self) -> bool {
        matches!(
            self.mode,
            Move::Jump
                | Move::DoubleJump
                | Move::TripleJump
                | Move::PoundJump
                | Move::SpinJump
                | Move::CapBounce
                | Move::CatchJump
                | Move::Backflip
                | Move::Sideflip
                | Move::Fall
                | Move::WallJump
        )
    }

    /// The actor's wall judge, asked after the state's own step: when the
    /// player is falling against a wall he faces, the slide starts.
    pub(super) fn judge_wall_keep(&mut self, scene: &Course) {
        if self.nerve.deferred || !self.wall_keep_applies() {
            return;
        }
        if self.wall_keep_judged(scene) {
            self.start_wall_slide();
        }
    }

    /// `PlayerJudgeWallKeep::update`, on the contacts the last step ended
    /// with. Not ported: the force-run, upper-punch and carry conditions (the
    /// controller has none of them) and the 2D rules.
    fn wall_keep_judged(&self, scene: &Course) -> bool {
        let trans = self.pos / UNIT;
        // Neither on the ground nor just above it.
        if self.ground_height < WALL_HEIGHT_LOW_LIMIT || self.grounded {
            return false;
        }
        let Some(contact) = &self.touched.wall_contact else {
            return false;
        };
        if self.no_wall_keep_wall() {
            return false;
        }
        let normal = contact.normal;
        if !self.wall_history.enables(trans, normal) {
            return false;
        }
        // Falling.
        let Some(velocity) = self.v.try_normalize() else {
            return false;
        };
        if velocity.dot(Vec3::NEG_Y) <= 0. {
            return false;
        }
        let keep_cos = WALL_KEEP_DEGREE.to_radians().cos();
        if self.mode == Move::Backflip {
            // `EActionTrigger_val30` (set while the back jump flies): the stick
            // has to push deep into the wall.
            if self.tilt < DEEP_STICK || (-self.stick).dot(normal) < keep_cos {
                return false;
            }
        } else {
            if (-self.facing).dot(normal) < keep_cos {
                return false;
            }
            let wish = self.ground_stick().normalize_or_zero();
            let dot = normal.dot(wish);
            if dot > 0. && dot < keep_cos {
                return false;
            }
        }
        if self.v.y.abs() < 0.001 {
            return false;
        }
        // The wall must be within reach of the head.
        let probe = trans + Vec3::Y * PROBE_HEIGHT;
        if let Some(mesh) = &scene.mesh
            && arrow_strikes(mesh, probe, normal * -PROBE_LENGTH, false).is_empty()
        {
            return false;
        }
        if contact.at_face {
            return true;
        }
        // An edge or a corner: the wall must go on to both sides in about the
        // same plane, unless it is only the thin edge of a wall seen from behind.
        let Some(mesh) = &scene.mesh else {
            return true;
        };
        let side = normal.cross(Vec3::NEG_Y).normalize_or_zero();
        let near = contact.position - normal * 5.;
        let offset = side * 50.;
        for (start, vector) in [(near + offset, -offset), (near - offset, offset)] {
            // `getLastPolyOnArrow`: the face the arrow strikes last.
            let last = arrow_strikes(mesh, start, vector, true)
                .into_iter()
                .max_by(|a, b| a.distance.total_cmp(&b.distance));
            let Some(hit) = last else {
                continue;
            };
            if collision_border_behind(mesh, hit.position, hit.normal) {
                continue;
            }
            if angle_on_plane(hit.normal, normal, Vec3::NEG_Y) > WALL_FOLLOW_ANGLE_H
                || angle_on_plane(hit.normal, normal, side) > WALL_FOLLOW_ANGLE_V
            {
                return false;
            }
        }
        true
    }

    /// `rs::isActionCodeNoWallKeepWall`: the wall's codes say it is no place to
    /// hold on.
    fn no_wall_keep_wall(&self) -> bool {
        let codes: Option<&CollisionCodes> = self.touched.wall.as_ref();
        codes.is_some_and(|c| {
            c.wall == "NoAction"
                || c.wall == "OnlyWallHitDown"
                || c.floor == "GrabCeil"
                || c.floor == "Pole"
        })
    }

    /// `PlayerStateNormalWallSlide::appear`, which runs as the nerve is
    /// chosen: face the wall, press into it at 15 units a frame and move up
    /// to its surface. The state's first step (its clip, its sound) is the
    /// next one.
    fn start_wall_slide(&mut self) {
        let Some(contact) = self.touched.wall_contact.clone() else {
            return;
        };
        let normal = contact.normal;
        self.wall_air = WallAirControl {
            normal_start: normal,
            normal_end: normal,
            ..WallAirControl::default()
        };
        self.facing = (-normal).with_y(0.).normalize_or(self.facing);
        self.v = -normal * WALL_PRESS;
        let mut trans = self.pos / UNIT;
        if !contact.at_face {
            // A corner or an edge: slide along the wall until the contact is
            // straight ahead.
            let side = normal.cross(Vec3::NEG_Y).normalize_or_zero();
            trans -= side * (trans - contact.position).dot(side);
        }
        let distance = (trans - contact.position).dot(normal);
        trans -= normal * (distance - COLLISION_RADIUS).max(0.);
        self.pos = trans * UNIT;
        self.wall_air.side_to_camera = self.side_faces_camera();
        let clip = if self.wall_air.side_to_camera {
            "WallKeep"
        } else {
            "WallKeepReverse"
        };
        let first_step = |p: &mut Player| p.events.push(Fx::WallSlide);
        if self.mode == Move::WallJump {
            // Grabbing the wall again in the jump: another sub-state of the
            // same actor nerve.
            self.defer(Move::WallSlide, clip, first_step);
        } else {
            self.change(Move::WallSlide, clip, first_step);
        }
    }

    /// One step of `PlayerStateWallAir::exeSlide`: the slide state's nerve,
    /// then the pre-input jump.
    pub(super) fn wall_slide(&mut self, o: &Orders) {
        if !self.follow_wall_normal() {
            self.end_wall_slide(o);
            return;
        }
        match self.wall_air.nerve {
            SlideNerve::Keep => {
                if self.wall_air.steps > t::WALL_KEEP_FRAMES {
                    self.slide_along_wall();
                } else {
                    self.press_into_wall();
                }
                self.wall_air.steps += 1;
                if self.anim_done(WALL_KEEP_CLIP) {
                    // `Keep` ends with its clip; `Slide` starts the next one.
                    self.wall_air.nerve = SlideNerve::Slide;
                    self.wall_air.steps = 0;
                    self.play(if self.wall_air.side_to_camera {
                        "WallSlideR"
                    } else {
                        "WallSlideL"
                    });
                }
            }
            SlideNerve::Slide => {
                self.slide_along_wall();
                self.wall_air.steps += 1;
            }
        }
        self.wall_jump_pressed();
    }

    /// `PlayerStateNormalWallSlide::followNormal`: take the wall's normal as
    /// it is now. False when the state is over: the wall is gone, is no place
    /// to hold on, or has turned too far since the last step, or the player is
    /// on the ground.
    fn follow_wall_normal(&mut self) -> bool {
        let Some(contact) = self.touched.wall_contact.clone() else {
            return false;
        };
        if self.no_wall_keep_wall() {
            return false;
        }
        let normal = contact.normal;
        let from = self.wall_air.normal_start;
        let tangent = normal.cross(Vec3::NEG_Y).normalize_or_zero();
        if angle_on_plane(from, normal, Vec3::NEG_Y) > WALL_FOLLOW_ANGLE_H
            || angle_on_plane(from, normal, tangent) > WALL_FOLLOW_ANGLE_V
        {
            return false;
        }
        self.wall_air.normal_start = normal;
        // The body swings a quarter of the way to face the wall; the normal
        // the slide moves by is the way he faces, turned round. (The velocity
        // is turned with it in the original; for a wall that does not turn,
        // as the recordings' do not, that changes nothing and is not ported.)
        let toward = (-normal).with_y(0.).normalize_or(self.facing);
        self.facing = air_move::turn_toward(self.facing, toward, FACE_WALL_RATE);
        self.wall_air.normal_end = -self.facing;
        !self.grounded
    }

    /// The state is over (the wall was left): take the speed into the wall
    /// off, remember where, and let the actor choose what follows.
    pub(super) fn end_wall_slide(&mut self, o: &Orders) {
        self.leave_wall_slide();
        if self.grounded {
            self.land(o);
        } else {
            self.go(Move::Fall, "Fall");
        }
    }

    /// Put the slide into the middle of a recorded one (lockstep): the wall is
    /// where the player faces, the nerve and its steps follow the clip and the
    /// row's sub-state step, the sideways speed is what the velocity has along
    /// the wall.
    pub(super) fn resume_wall_slide(&mut self, s: &Snapshot) {
        let normal = (-self.facing).with_y(0.).normalize_or(Vec3::Z);
        let clip = s.anim.map_or("", |(name, _)| name);
        let keeping = !matches!(clip, "WallSlideL" | "WallSlideR");
        let steps = s.sub_steps.unwrap_or(0);
        let down = Vec3::NEG_Y;
        let tangent = normal.cross(down).normalize_or_zero();
        self.wall_air = WallAirControl {
            nerve: if keeping {
                SlideNerve::Keep
            } else {
                SlideNerve::Slide
            },
            steps: if keeping {
                steps
            } else {
                steps.saturating_sub(WALL_KEEP_CLIP + 1)
            },
            normal_start: normal,
            normal_end: normal,
            side_speed: self.v.dot(tangent),
            side_to_camera: matches!(clip, "WallKeep" | "WallSlideR"),
        };
    }

    /// What leaving the wall does (the end of `exeSlide`, and the ground
    /// reached by the slide): the speed into the wall comes off the velocity
    /// and the history records where the wall was left.
    pub(super) fn leave_wall_slide(&mut self) {
        let normal = self.wall_air.normal_end;
        self.v -= normal * self.v.dot(normal);
        self.wall_history.leave = Some((self.pos / UNIT, self.wall_air.normal_start));
    }

    /// `Keep`'s first steps: only the press into the wall.
    fn press_into_wall(&mut self) {
        let press = -self.wall_air.normal_end * WALL_PRESS;
        air_move::add_vector_limit(&mut self.v, press, SLIDE_SPEED_MAX);
    }

    /// The slide's movement: the press into the wall, gravity along it and
    /// the speed along the wall the stick asks for. What the velocity had
    /// across the wall's face (neither into it nor down it) is replaced by that.
    fn slide_along_wall(&mut self) {
        let normal = self.wall_air.normal_end;
        let down = (Vec3::NEG_Y - normal * Vec3::NEG_Y.dot(normal))
            .try_normalize()
            .unwrap_or(Vec3::NEG_Y);
        let tangent = normal.cross(down).normalize_or_zero();
        let target = SIDE_SPEED_PER_STICK * self.push().dot(tangent);
        self.wall_air.side_speed = converge(self.wall_air.side_speed, target, SIDE_SPEED_STEP);
        let mut into = normal * self.v.dot(normal);
        let in_plane = self.v - into;
        let mut along = down * in_plane.dot(down);
        air_move::add_vector_limit(&mut into, -normal * WALL_PRESS, SLIDE_SPEED_MAX);
        air_move::add_vector_limit(&mut along, down * t::WALL_SLIDE_GRAVITY, SLIDE_SPEED_MAX);
        self.v = into + along + tangent * self.wall_air.side_speed;
    }

    /// `PlayerStateWallAir::exeSlide`'s pre-input jump: the wall jump is the
    /// next nerve, and its first step the next frame.
    fn wall_jump_pressed(&mut self) {
        if self.pre_jump == 0 || self.nerve.deferred {
            return;
        }
        self.pre_jump = 0;
        self.wall_history.jump = Some((self.pos / UNIT, self.wall_air.normal_start));
        // The wall jump is a sub-state of the actor's `WallAir` nerve, which goes
        // on counting its steps.
        self.defer(Move::WallJump, "WallJump", |p| {
            // `PlayerStateNormalWallJump::exeJump`'s first step: away from the
            // wall at the wall-jump speed, the body turned that way, and the
            // shared air control set up with the stick locked.
            let away = -p.facing;
            p.facing = away;
            p.v = away * t::WALL_JUMP_H;
            p.dive_possible = true;
            p.air_throw_used = false;
            p.jump_extend = false;
            p.gravity = t::WALL_JUMP_GRAVITY;
            p.grounded = false;
            p.events.push(Fx::Jump(Move::WallJump));
            p.air.setup(&mut p.v, p.facing, &wall_jump_takeoff());
            p.air_for = Some(Move::WallJump);
            p.jump_power = t::WALL_JUMP_POWER;
        });
    }

    /// One step of the wall jump nerve: the shared air control, then the clip
    /// that follows the jump's own (`WallJump`, then `Fall`, started in the
    /// step the first one ends).
    pub(super) fn wall_jump(&mut self, o: &Orders) {
        self.jump_air(o);
        if self.anim.name == "WallJump" && self.anim_done(WALL_JUMP_CLIP) {
            self.start_clip("Fall");
        }
    }
}
