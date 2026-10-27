//! The cap throw in the water: `SwimSpinCapSurface` (thrown from the surface) and `SwimSpinCap`
//! (thrown under it or on the bottom), the two inner nerves of `PlayerStateSwim` that
//! `PlayerActorHakoniwa::exeSwim` starts through `startCapThrowSurface` / `startCapThrow`.
//!
//! Written from the 1.0.0 executable and checked against `swim-lake-spincap`. The nerves play
//! `SwimSpinCapStart` (50 frames, the spin attack of the model), keep moving like the nerves they
//! replace (the surface's spring and stroke; the open water's and the bottom's), and send the cap
//! away on the clip's throw frame (`PlayerSpinCapAttack::getThrowFrameSwim`). Once it is away the
//! state may be cancelled by the jump, the pound or the paddle, else it ends with the clip.
//!
//! The cap's flight in the water is in `cap.rs`.
use super::swim::{SwimEnd, SwimNerve};
use super::swim_const as k;
use super::swim_dive::limit_velocity_dir;
use super::swim_move::Stroke;
use super::vector_lib as lib;
use super::wall_slide::angle_on_plane_degree;
use super::*;

const UP: Vec3 = Vec3::Y;
/// The clip of the swim's spin attack (`startCapSpinAttackSwim`) and its length.
const SPIN_CAP_CLIP: &str = "SwimSpinCapStart";
/// `SwimSurfaceSpinCapFrame`: the surface throw's own stroke (the faster top speed) lasts this
/// many steps.
const SURFACE_SPIN_CAP_FRAME: u32 = 45;

impl Player {
    // ---- The actor's decision --------------------------------------------------------------

    /// `PlayerStateSwim::isEnableCapThrowSurface`: swimming at the surface (or on the head
    /// slide's end, which is not ported).
    fn is_enable_cap_throw_surface(&self) -> bool {
        self.water_finder.found && self.swim.nerve == SwimNerve::Surface
    }

    /// `PlayerStateSwim::isEnableCapThrow`: under the surface, on the bottom, in the paddle and
    /// after the hip drop's landing has run its first steps.
    fn is_enable_cap_throw(&self) -> bool {
        match self.swim.nerve {
            SwimNerve::Move | SwimNerve::Paddle | SwimNerve::Land | SwimNerve::Walk => true,
            SwimNerve::HipDropLand => self.swim.steps as i32 >= k::DIVE_LAND_CANCEL_FRAME,
            _ => false,
        }
    }

    /// The end of `PlayerActorHakoniwa::exeSwim`: when the state allows a throw, a buffered
    /// throw press (`tryActionCapSpinAttackImpl`: the pre-input judge, the cap ready) starts the
    /// nerve that spins the cap off.
    pub(super) fn try_swim_cap_throw(&mut self) {
        let surface = self.is_enable_cap_throw_surface();
        if !surface && !self.is_enable_cap_throw() {
            return;
        }
        if self.pre_throw == 0 || self.cap.is_some() || self.launch.is_some() || self.winding {
            return;
        }
        if matches!(
            self.swim.nerve,
            SwimNerve::SpinCapSurface | SwimNerve::SpinCap
        ) {
            return;
        }
        self.pre_throw = 0;
        self.start_swim_cap_throw(surface);
    }

    /// `PlayerStateSwim::startCapThrow` / `startCapThrowSurface`: the attack info is set up and
    /// the nerve is chosen; its first step starts the clip.
    fn start_swim_cap_throw(&mut self, surface: bool) {
        self.setup_attack_info();
        self.throwing.kind = Some(Throw::Forward);
        self.throwing.in_air = false;
        self.throwing.dir = None;
        self.throw_clip = self.swim_throw_clip();
        // The actor chooses it after the state's update: the trace names it from the next one.
        self.swim.chosen_after_update = Some(self.swim.nerve);
        self.set_swim_nerve(if surface {
            SwimNerve::SpinCapSurface
        } else {
            SwimNerve::SpinCap
        });
    }

    /// The clip `startCapSpinAttackSwim` names (`sub_464c88` builds the name from the throw
    /// count: the plain name for the first throw, the count and the hand after it; GUESS for
    /// the later throws of a row, which no recording reaches).
    fn swim_throw_clip(&self) -> &'static str {
        match (self.throwing.count, self.throwing.continuous) {
            (1, false) => SPIN_CAP_CLIP,
            (2, false) => "SwimSpinCapStart2Right",
            _ => "SwimSpinCapStart3Right",
        }
    }

    // ---- The nerves ------------------------------------------------------------------------

    /// The first step of both nerves: the clip, and the direction the cap goes in: the stick's
    /// on the plane of `up`, else the front, flattened onto it. The body turns to it at once.
    fn start_swim_spin_attack(&mut self, up: Vec3) {
        self.start_clip(self.throw_clip);
        // The cap is the spin attack from this step (the trace shows it from the next).
        self.winding = true;
        self.winding_age = 0;
        let (mut dir, has_direction) = pose_math::try_normalize_or_zero(self.plane_stick(up));
        if !has_direction {
            dir = pose_math::try_normalize_or_zero(lib::verticalize_vec(up, self.facing)).0;
        }
        let (pose_up, _) = self.pose_axes();
        let stored = self
            .actor_pose()
            .slerp_up_front(pose_up, dir, 1., t::HILL_POSE_DEGREE_MAX);
        self.set_pose(stored);
        self.throwing.dir = Some(dir);
    }

    /// The cap leaves on the throw frame (`PlayerSpinCapAttack::startCapThrow`).
    fn swim_throw_cap_on_frame(&mut self) {
        let frame = k::SPIN_CAP_THROW_FRAME;
        if self.swim.steps == frame && self.winding && self.launch.is_none() {
            self.launch_cap(Throw::Forward, self.throwing.continuous);
        }
    }

    /// Whether the cap is still the spin attack (`PlayerSpinCapAttack::isCapSpinAttack`): it is
    /// not any more once the throw has started it.
    fn swim_spin_attack_runs(&self) -> bool {
        self.winding && self.launch.is_none()
    }

    /// `PlayerStateSwim::exeSwimSpinCapSurface`.
    pub(super) fn exe_swim_spin_cap_surface(
        &mut self,
        first: bool,
        o: &Orders,
        _scene: &Course,
    ) -> Option<SwimEnd> {
        if first {
            self.start_swim_spin_attack(UP);
            // The body leans as much as the model's root does (the angle of its up axis from
            // the world's, seen along its side axis; never backward).
            let axes = self.model_axes;
            let lean = angle_on_plane_degree(UP, axes.root_up, axes.root_side).max(0.);
            self.joints.swim.set_pitch(lean);
        }
        let step = self.swim.steps;
        // The nerve step stands where the surface nerve keeps its damper counter.
        self.swim_surface_motion(
            Stroke {
                surface: true,
                spin_cap: step <= SURFACE_SPIN_CAP_FRAME,
                ..Stroke::default()
            },
            step as i32,
        );
        self.swim_turn(1.);
        self.joints.swim.ease_off();
        self.swim_throw_cap_on_frame();
        if self.swim_spin_attack_runs() {
            return None;
        }
        if self.start_swim_jump_judged(o) {
            return Some(SwimEnd::Jump);
        }
        if o.crouch_pressed {
            self.set_swim_nerve(SwimNerve::HipDropStart);
            return None;
        }
        if self.anim_done(clip::SWIM_SPIN_CAP_START) {
            let finder = self.water_finder;
            if finder.found && finder.distance < k::SURFACE_END_DIST {
                self.set_swim_nerve(SwimNerve::Surface);
            } else {
                self.set_swim_nerve(SwimNerve::Move);
            }
        }
        None
    }

    /// `PlayerStateSwim::exeSwimSpinCap`.
    pub(super) fn exe_swim_spin_cap(
        &mut self,
        first: bool,
        o: &Orders,
        _scene: &Course,
    ) -> Option<SwimEnd> {
        if first {
            let normal = self.ground_normal_or_up();
            self.start_swim_spin_attack(normal);
            if !self.is_on_ground_by(self.v) {
                // In the open water the throw lifts the body.
                let lift = k::SPIN_CAP_UP_POWER;
                self.v = Vec3::new(
                    self.v.x + UP.x * lift,
                    self.v.y + UP.y * lift,
                    self.v.z + UP.z * lift,
                );
                self.v = limit_velocity_dir(self.v, UP, k::SPIN_CAP_UP_SPEED_MAX);
            }
        }
        self.swim_underwater_motion();
        self.joints.swim.ease_off();
        self.swim_throw_cap_on_frame();
        if self.swim_spin_attack_runs() {
            return None;
        }
        // (The attack sensors' trample is not ported.)
        if o.jump {
            self.set_swim_nerve(SwimNerve::Paddle);
            return None;
        }
        if o.crouch_pressed {
            self.set_swim_nerve(SwimNerve::HipDropStart);
            return None;
        }
        if self.anim_done(clip::SWIM_SPIN_CAP_START) {
            let finder = self.water_finder;
            if finder.found && finder.distance < k::SURFACE_START_DIST {
                self.set_swim_nerve(SwimNerve::Surface);
            } else if self.is_collided_ground() {
                self.swim_walk_velocity();
                self.set_swim_nerve(SwimNerve::Walk);
            } else {
                self.set_swim_nerve(SwimNerve::Move);
            }
        }
        None
    }
}
