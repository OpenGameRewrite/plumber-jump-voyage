//! Mario bound to a pipe: `PlayerStateBind` plus the pipe's `DokanPuppetController`
//! (SMO 1.0.0 0x4671dc / 0x25b2f0..0x25c504) for the ordinary 3D side entrance.
//!
//! Frame order, as the recorded scheduler phases show it: the pipe's sensor message
//! reaches Mario before his update; the pipe actor (nerve, then `control`, which steps the
//! controller) updates after Mario's update of the same frame. This port runs that pipe
//! update at the start of the next player step, so the sample after Mario's update stays
//! the state Mario's own update left.
use super::dokan::Dokan;
use super::joint_math::ease_out;
use super::pose_math::{quat_front_up, quat_to_axes, slerp, to_quat};
use super::*;

/// `DokanPuppetController::exeMoveIn` blends the saved pose into the target over 12 steps
/// (`calcNerveEaseOutRate(this, 12)`, 0x25c2a0 / literal 0xc).
const MOVE_IN_STEPS: u32 = 12;
/// `exeIn` hides the puppet from this step when the pipe and its destination are in
/// different dimensions (`mov w9, #0x32` at 0x25c4ac) and from 60 otherwise (`mov w8, #0x3c` at 0x25c4a8).
const IN_STEPS_BETWEEN_DIMENSIONS: u32 = 50;
const IN_STEPS_SAME_DIMENSION: u32 = 60;
/// Original PlayerActorHakoniwa.szs / InitSensor.byml Body Radius/Y.
/// initPlayer loads that actor archive at 0x41b384, then retains its Body sensor
/// in the bind keeper path (getHitSensor at 0x41b430, keeper constructor at 0x41bcd0).
const BODY_RADIUS: f32 = 50.;
const BODY_HEIGHT: f32 = 75.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Controller {
    Wait,
    MoveIn,
    In,
    Out,
}

/// `Dokan`'s own nerves along the trip (the pipe actor owns the controller).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActorNerve {
    ReadyBind,
    In,
    WaitCameraInterpolate,
    Out,
}

/// One exported pipe's actor and puppet controller.
#[derive(Clone, Debug)]
pub(super) struct DokanActor {
    actor: ActorNerve,
    actor_step: u32,
    next_actor: Option<ActorNerve>,
    controller: Controller,
    /// `al::getNerveStep` of the controller's nerve.
    step: u32,
    next_controller: Option<Controller>,
    /// Mario's pose when the bind started (`startBind` calcMoveMtx*), and the mouth pose.
    saved: [f32; 12],
    target: [f32; 12],
    /// The controller's finished flag (+0xa0): the pipe actor warps on its next nerve step.
    in_done: bool,
    /// Mario is bound to this pipe's controller (`+0x18` puppet pointer).
    bound: bool,
}

impl DokanActor {
    fn ready() -> Self {
        Self {
            actor: ActorNerve::ReadyBind,
            actor_step: 0,
            next_actor: None,
            controller: Controller::Wait,
            step: 0,
            next_controller: None,
            saved: [0.; 12],
            target: [0.; 12],
            in_done: false,
            bound: false,
        }
    }
}

fn axes_of(m: &[f32; 12]) -> [Vec3; 3] {
    [
        Vec3::new(m[0], m[4], m[8]),
        Vec3::new(m[1], m[5], m[9]),
        Vec3::new(m[2], m[6], m[10]),
    ]
}

/// `al::blendMtx` (0x9358dc): linear translation, `slerpQuat` of the two rotations.
fn blend_matrix(from: &[f32; 12], to: &[f32; 12], rate: f32) -> ([Vec3; 3], Vec3) {
    let weight = 1. - rate;
    let position = Vec3::new(
        weight * from[3] + to[3] * rate,
        weight * from[7] + to[7] * rate,
        weight * from[11] + to[11] * rate,
    );
    let q = slerp(to_quat(axes_of(from)), to_quat(axes_of(to)), rate);
    (quat_to_axes(q), position)
}

impl Player {
    fn ensure_pipes(&mut self, count: usize) {
        if self.pipes.len() != count {
            self.pipes = vec![DokanActor::ready(); count];
        }
    }

    /// `PlayerBindKeeper::sendStartMsg` (0x42d948) at the start of Mario's update: every
    /// pipe bind sensor the Body sensor met answers `BindStart` (judge), then `BindInit`
    /// starts the bind. The sensor pair list is two updates old: the director tests the
    /// positions of the end of the previous frame, the callbacks collect during the next
    /// frame's sensor update, and Mario's following update sends the message.
    pub(super) fn dokan_sensor_phase(&mut self, scene: &Course) {
        let ready = std::mem::take(&mut self.bind_ready);
        if self.bound_to.is_some() || self.is_dead() || self.recovering() {
            return;
        }
        let Some(mesh) = &scene.mesh else { return };
        self.ensure_pipes(mesh.dokans.len());
        let feet = mesh.to_game_vec(self.position_cm());
        let (front, velocity) = (self.actor_pose().front(), self.units_per_frame());
        for index in ready {
            // The controller refuses while it holds a puppet (judge 0x25b320).
            if !self.pipes[index].bound
                && mesh.dokans[index].accepts_side_bind(feet, front, velocity)
            {
                self.start_dokan_bind(mesh, index);
                return;
            }
        }
    }

    /// End of the update: the Body sensor positions the director tests next.
    pub(super) fn dokan_sensor_trail(&mut self, scene: &Course) {
        self.bind_ready = std::mem::take(&mut self.bind_staged);
        let Some(mesh) = &scene.mesh else { return };
        let body = mesh.to_game_vec(self.position_cm()) + Vec3::Y * BODY_HEIGHT;
        self.bind_staged = mesh
            .dokans
            .iter()
            .enumerate()
            .filter(|(_, dokan)| {
                let reach = BODY_RADIUS + dokan.bind_radius();
                let offset = body - dokan.bind_center();
                offset.dot(offset) < reach * reach
            })
            .map(|(index, _)| index)
            .collect();
    }

    /// `DokanPuppetController::startBind` (0x25b6f0) for the side entrance.
    fn start_dokan_bind(&mut self, mesh: &crate::geometry::TriangleMesh, source: usize) {
        let dokan = &mesh.dokans[source];
        let Some(target) = dokan.side_entry_target() else {
            return;
        };
        // calcMoveMtxSide 0x25be70..0x25bed0: the saved pose is Mario's own front, up and
        // position (makeMtxFrontUpPos).
        let pose = self.actor_pose();
        let feet = mesh.to_game_vec(self.position_cm());
        let (side, up, front) = (pose.side(), pose.up(), pose.front());
        let pipe = &mut self.pipes[source];
        pipe.saved = [
            side.x, up.x, front.x, feet.x, side.y, up.y, front.y, feet.y, side.z, up.z, front.z,
            feet.z,
        ];
        pipe.target = target;
        pipe.bound = true;
        pipe.in_done = false;
        // `Dokan::receiveMsg` BindInit: the pipe actor enters In, the controller MoveIn.
        pipe.actor = ActorNerve::In;
        pipe.actor_step = 0;
        pipe.next_actor = None;
        pipe.controller = Controller::MoveIn;
        pipe.step = 0;
        pipe.next_controller = None;
        self.bound_to = Some(source);
        // PlayerStateBind::appear (0x467288): the velocity is cleared and the bind nerve runs.
        // The state is entered before this update's nerve step runs; the clip the player
        // was playing goes on until the controller starts the pipe's own (startPlayerPuppet
        // only ends the sub animation).
        self.v = Vec3::ZERO;
        self.grounded = false;
        self.nerve.clear();
        self.nerve.actor = "Bind";
        self.nerve.step = 0;
        self.frame = 0;
        self.mode = Move::Bind;
        self.sub.active = false;
        self.sub_anim = None;
        self.interpolation_cleared = true;
    }

    /// Mario's own update while bound: `PlayerStateBind::exeBind` does nothing.
    pub(super) fn bind_state(&mut self) {
        if matches!(self.anim.name, "Move" | "MoveMoon") {
            self.cycle.move_on_without_control();
        }
        self.v = Vec3::ZERO;
        self.grounded = false;
    }

    /// `PlayerStateBind::exeEndFall` (0x4675b0): the air control keeps falling; once the
    /// fall has reached the ground the player's wait state takes over.
    pub(super) fn bind_fall_state(&mut self) {
        self.v.y -= self.constants.gravity_air;
        self.v.y = self.v.y.max(-t::FALL_MAX);
        if self.grounded {
            self.go(Move::Idle, "Wait");
        }
    }

    /// The pipe actors' update of the previous frame, in the original order: every actor's
    /// nerve step, then its `control` (the controller's nerve step).
    pub(super) fn dokan_update(&mut self, scene: &Course, o: &Orders) {
        let Some(mesh) = &scene.mesh else { return };
        self.ensure_pipes(mesh.dokans.len());
        for index in 0..self.pipes.len() {
            self.update_pipe_actor(mesh, index, o);
            self.update_pipe_controller(mesh, index);
        }
    }

    fn destination_of(mesh: &crate::geometry::TriangleMesh, index: usize) -> &Dokan {
        mesh.dokans
            .iter()
            .find(|d| d.id == mesh.dokans[index].destination_id)
            .expect("destination checked when the export was loaded")
    }

    fn update_pipe_actor(
        &mut self,
        mesh: &crate::geometry::TriangleMesh,
        index: usize,
        o: &Orders,
    ) {
        if let Some(next) = self.pipes[index].next_actor.take() {
            self.pipes[index].actor = next;
            self.pipes[index].actor_step = 0;
        }
        let (actor, step, in_done) = {
            let pipe = &self.pipes[index];
            (pipe.actor, pipe.actor_step, pipe.in_done)
        };
        match actor {
            ActorNerve::In if in_done => {
                // Dokan::exeIn 0x257e40: the controller has finished its descent; warp
                // (0x25b928) and start the camera interpolation, then wait for it.
                self.pipes[index].next_actor = Some(ActorNerve::WaitCameraInterpolate);
                self.warp_to(Self::destination_of(mesh, index), mesh);
            }
            ActorNerve::WaitCameraInterpolate if !o.camera_interpolating => {
                // exeWaitCameraInterpolate 0x257f38: the camera has caught up; leave.
                self.pipes[index].next_actor = Some(ActorNerve::Out);
            }
            ActorNerve::Out if step == 0 => {
                // Dokan::exeOut first step: `DokanPuppetController::out`.
                self.pipes[index].next_controller = Some(Controller::Out);
            }
            _ => {}
        }
        self.pipes[index].actor_step += 1;
    }

    /// `DokanPuppetController::warp` for an upright 2D destination (0x25b928): the player is
    /// reset to the mouth, the keeper validated for 2D.
    fn warp_to(&mut self, destination: &Dokan, mesh: &crate::geometry::TriangleMesh) {
        let Some((position, pose)) = destination.upright_2d_exit() else {
            return;
        };
        // PuppetFunction2D3D::validate2DPuppet: the keeper becomes valid.
        self.dimension.valid = true;
        self.pos = mesh.relative_vec(position) * UNIT;
        self.set_pose(pose);
    }

    fn update_pipe_controller(&mut self, mesh: &crate::geometry::TriangleMesh, index: usize) {
        if let Some(next) = self.pipes[index].next_controller.take() {
            self.pipes[index].controller = next;
            self.pipes[index].step = 0;
        }
        let (controller, step) = (self.pipes[index].controller, self.pipes[index].step);
        match controller {
            Controller::Wait => {}
            Controller::MoveIn => {
                if step == 0 {
                    self.play_puppet_action("DokanSideIn");
                }
                let rate = ease_out((step as f32 / MOVE_IN_STEPS as f32).clamp(0., 1.));
                let (axes, position) =
                    blend_matrix(&self.pipes[index].saved, &self.pipes[index].target, rate);
                self.set_puppet_pose(mesh, position, axes[2], axes[1]);
                if step >= MOVE_IN_STEPS {
                    self.pipes[index].next_controller = Some(Controller::In);
                }
            }
            Controller::In => {
                let between_dimensions =
                    Self::destination_of(mesh, index).is_2d != mesh.dokans[index].is_2d;
                let hide_at = if between_dimensions {
                    IN_STEPS_BETWEEN_DIMENSIONS
                } else {
                    IN_STEPS_SAME_DIMENSION
                };
                if step >= hide_at {
                    self.puppet_hidden = true;
                    self.pipes[index].in_done = true;
                    self.pipes[index].next_controller = Some(Controller::Wait);
                }
            }
            Controller::Out => {
                // exeOut 0x25c7b8 into an upright 2D mouth.
                if step == 0 {
                    if let Some((position, pose)) =
                        Self::destination_of(mesh, index).upright_2d_exit()
                    {
                        self.pos = mesh.relative_vec(position) * UNIT;
                        self.set_pose(pose);
                    }
                    self.puppet_hidden = false;
                    self.play_puppet_action("DokanOut");
                }
                // isPuppetActionEnd: the clip has played to its last frame.
                if step > 0 && self.anim_done(clip::DOKAN_OUT) {
                    self.end_dokan_bind(mesh, index);
                }
            }
        }
        self.pipes[index].step += 1;
    }

    /// `rs::endBindAndPuppetNull` with the 2D mouth's fall velocity (0x25c9a0..0x25cb70).
    fn end_dokan_bind(&mut self, mesh: &crate::geometry::TriangleMesh, index: usize) {
        let at = Self::destination_of(mesh, index).position();
        let gravity = mesh
            .dimension
            .single_movement_at(at)
            .and_then(|area| area.gravity(at))
            .map_or(Vec3::NEG_Y, |(direction, _)| direction);
        // exeOut 0x25cb1c: twice the 2D area's gravity direction.
        self.v = gravity + gravity;
        self.mode = Move::BindFall;
        self.frame = 0;
        self.puppet_hidden = false;
        let pipe = &mut self.pipes[index];
        pipe.bound = false;
        pipe.next_controller = Some(Controller::Wait);
        self.bound_to = None;
    }

    /// The trace's sub-nerve step counter of the bind states: Bind is one nerve for the
    /// whole trip; EndFall counts from its own first step.
    pub(super) fn bind_sub_steps(&self) -> Option<u32> {
        match self.mode {
            Move::Bind => Some(self.nerve.step),
            Move::BindFall => Some(self.frame),
            _ => None,
        }
    }

    /// `rs::startPuppetAction`: the clip starts with the player's next update; the counter
    /// of that first row is the synced one (`PlayerAnimFrameCtrl::updateSync`: the frame
    /// plus the rate) while the pose layer still shows frame 0.
    fn play_puppet_action(&mut self, name: &'static str) {
        self.play(name);
        self.puppet_clip_pending = true;
    }

    /// `rs::setPuppetTrans` + `rs::setPuppetQuat(makeQuatFrontUp(front, up))`.
    fn set_puppet_pose(
        &mut self,
        mesh: &crate::geometry::TriangleMesh,
        position: Vec3,
        front: Vec3,
        up: Vec3,
    ) {
        self.pos = mesh.relative_vec(position) * UNIT;
        self.set_pose(quat_front_up(front, up));
    }
}
