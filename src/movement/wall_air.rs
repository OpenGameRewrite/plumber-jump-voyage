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
use super::wall_slide::{FOLLOW_ANGLE_H, FOLLOW_ANGLE_V, WallSlide};
use super::*;
use crate::collider::{arrow_strikes, border_behind};
use crate::geometry::CollisionCodes;

/// `WallKeepDegree`: how squarely the player faces, and pushes toward, the
/// wall for the judge.
const WALL_KEEP_DEGREE: f32 = 60.;
/// `WallHeightLowLimit`: nearer the ground than this the judge is off.
const WALL_HEIGHT_LOW_LIMIT: f32 = 120.;
/// `Tall`: how far below the place he left a wall the player has to be before
/// the history lets him hold a wall again.
const TALL: f32 = 160.;
/// The body pulled to the wall stops this far from it (`CollisionRadius`).
const COLLISION_RADIUS: f32 = 55.;
/// The judge's probe from the head (120 above the feet) toward the wall.
const PROBE_HEIGHT: f32 = 120.;
const PROBE_LENGTH: f32 = 77.;
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
    /// The wall as the last step saw it (`mNormalStart`), the direction the slide moves by, which
    /// follows the pose's front (`mNormalEnd`), and the speed along the wall the stick asks for.
    slide: WallSlide,
    /// The speed into the wall has come off already (`followNormal` did it as it lost the wall
    /// or met the ground).
    left: bool,
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
    pub(super) jump: Option<(Vec3, Vec3)>,
    pub(super) leave: Option<(Vec3, Vec3)>,
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
pub(super) fn converge(value: f32, target: f32, step: f32) -> f32 {
    if (target - value).abs() <= step {
        target
    } else {
        value + step * (target - value).signum()
    }
}

/// The angle (degrees) between two vectors seen along `axis`
/// (`al::calcAngleOnPlaneDegree`, which is signed; only its size is used).
pub(super) fn angle_on_plane(a: Vec3, b: Vec3, axis: Vec3) -> f32 {
    wall_slide::angle_on_plane_degree(a, b, axis).abs()
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
    pub(super) fn enables(&self, trans: Vec3, normal: Vec3) -> bool {
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
    /// `rs::scaleVelocityInertiaWallHit`, asked at the start of the air control
    /// by the jump nerve (scale 0.25, floor 1) and by the fall (0.5, 0), both
    /// with `NormalMaxSpeed` as the cap: a body that flew into the wall it
    /// touched last step, within 60 degrees of its normal, keeps only a share
    /// of its horizontal speed. The new speed is the share (`scale` times the
    /// speed) unless that falls under the lesser of the speed and `floor`,
    /// and never more than the cap. (A moving wall adds its own speed to this;
    /// the stage's static collision has none.)
    pub(super) fn scale_velocity_at_wall(&mut self) {
        let (scale, floor) = match self.mode {
            Move::Jump | Move::DoubleJump | Move::TripleJump | Move::SpinJump => (0.25, 1.),
            Move::Fall => (t::FALL_WALL_SCALE_VELOCITY, 0.),
            _ => return,
        };
        let Some(contact) = &self.touched.wall_contact else {
            return;
        };
        let normal = contact.normal.with_y(0.).normalize_or_zero();
        let flat = self.v.with_y(0.);
        let direction = flat.normalize_or_zero();
        if normal == Vec3::ZERO || direction.dot(normal) >= -0.5 {
            return;
        }
        let speed = flat.length();
        let kept = speed.min(floor);
        let share = speed * scale;
        let target = if share < kept {
            kept
        } else {
            share.min(t::NORMAL_MAX)
        };
        let flat = direction * target;
        self.v = Vec3::new(flat.x, self.v.y, flat.z);
    }

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
            if border_behind(mesh, hit.position, hit.normal) {
                continue;
            }
            if angle_on_plane(hit.normal, normal, Vec3::NEG_Y) > FOLLOW_ANGLE_H
                || angle_on_plane(hit.normal, normal, side) > FOLLOW_ANGLE_V
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
        let stood = self.pos;
        let trans = self.position_cm();
        let grab = wall_slide::appear(
            contact.normal,
            contact.position,
            contact.at_face,
            trans,
            COLLISION_RADIUS,
        );
        self.wall_air = WallAirControl {
            slide: grab.slide,
            ..WallAirControl::default()
        };
        // The pose faces the wall (`makeQuatFrontUp`), the velocity presses into it.
        self.set_pose(grab.pose);
        self.v = grab.velocity;
        // `rs::resetCollisionExpandCheck`: the collider takes the new pose as the one it stood
        // in and the shapes grow back to full size in the next `collide`, whose own move then
        // starts at its second pose.
        self.collider
            .reset_expand_check(self.facing.x.atan2(self.facing.z));
        if grab.trans != trans {
            self.pos = grab.trans * UNIT;
            // The move up to the wall is a carry from where the collider stood.
            self.collider.carry_from(stood / UNIT);
        }
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
        let wall = self
            .touched
            .wall_contact
            .as_ref()
            .filter(|_| !self.no_wall_keep_wall())
            .map(|contact| contact.normal);
        let stored = self
            .pose_quat()
            .unwrap_or_else(|| plane::quat_up_front(Vec3::Y, self.facing));
        let follow = self.wall_air.slide.follow_normal(
            wall,
            stored,
            self.v,
            self.grounded,
            (FOLLOW_ANGLE_H, FOLLOW_ANGLE_V),
        );
        self.wall_air.slide = follow.slide;
        self.v = follow.velocity;
        if wall.is_some() && follow.pose != stored {
            self.set_pose(follow.pose);
        }
        if !follow.holds {
            self.wall_air.left = true;
        }
        follow.holds
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
        let side_speed = self.v.dot(tangent);
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
            slide: WallSlide {
                normal_start: normal,
                normal_end: normal,
                side_speed,
            },
            side_to_camera: matches!(clip, "WallKeep" | "WallSlideR"),
            left: false,
        };
    }

    /// What leaving the wall does (the end of `exeSlide`, and the ground
    /// reached by the slide): the speed into the wall comes off the velocity
    /// and the history records where the wall was left.
    pub(super) fn leave_wall_slide(&mut self) {
        if !self.wall_air.left {
            self.v = wall_slide::take_off_wall_speed(self.wall_air.slide.normal_end, self.v);
            self.wall_air.left = true;
        }
        self.wall_history.leave = Some((self.pos / UNIT, self.wall_air.slide.normal_start));
    }

    /// `Keep`'s first steps: only the press into the wall.
    fn press_into_wall(&mut self) {
        self.v = self.wall_air.slide.press(self.v);
    }

    /// The slide's movement: the press into the wall, gravity along it and the speed along the
    /// wall the stick asks for.
    fn slide_along_wall(&mut self) {
        // The slide asks `calcMoveInput` for the stick on the level (caller 0x4766b4, up = the
        // negated gravity); with the camera's view known that is the game's own arithmetic,
        // the wall-along bend included (a weight of 1e-4 at the start of the slide).
        let stick = if self.view.is_some() {
            self.plane_stick(super::pose_math::NEGATED_GRAVITY)
        } else {
            self.push()
        };
        self.v = self
            .wall_air
            .slide
            .slide(self.v, stick, t::WALL_SLIDE_GRAVITY);
    }

    /// `PlayerStateWallAir::exeSlide`'s pre-input jump: the wall jump is the
    /// next nerve, and its first step the next frame.
    fn wall_jump_pressed(&mut self) {
        if self.pre_jump == 0 || self.nerve.deferred {
            return;
        }
        self.pre_jump = 0;
        self.wall_history.jump = Some((self.pos / UNIT, self.wall_air.slide.normal_start));
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
