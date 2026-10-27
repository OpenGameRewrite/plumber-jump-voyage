//! PlayerJudgeDeadWipeStart and the scene-owned Miss/WipeMiss lifecycle.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Damage,
    Abyss,
    AbyssWithCapMessage,
}
impl Kind {
    pub fn start(self) -> i32 {
        match self {
            Self::Damage | Self::Abyss => 30,
            Self::AbyssWithCapMessage => 90,
        }
    }
    pub fn wait(self) -> u32 {
        match self {
            Self::Damage => 96,
            Self::Abyss | Self::AbyssWithCapMessage => 30,
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
enum Miss {
    #[default]
    Playing,
    Miss,
    CloseStart(u32),
    CloseWait(u32),
    CloseEnd(u32),
    Closed,
}
#[derive(Debug)]
pub struct Death {
    pub kind: Kind,
    pub counter: i32,
    pub restart_pending: bool,
    pub generation: u32,
    /// Restart placement selected by the scene/save, distinct from bubble safety.
    pub restart_pose: Option<(Vec3, f32)>,
    /// Replays consume scene-reload boundaries from the capture's discontinuous clock.
    pub external_reload: bool,
    pub(super) abyss_pending: bool,
    miss: Miss,
}
impl Default for Death {
    fn default() -> Self {
        Self {
            kind: Kind::Damage,
            counter: -1,
            restart_pending: false,
            generation: 0,
            restart_pose: None,
            external_reload: false,
            abyss_pending: false,
            miss: Miss::Playing,
        }
    }
}
impl Death {
    pub fn abyss_pending_for_trace(&self) -> bool { self.abyss_pending }
    pub fn ready(&self) -> bool {
        self.counter >= self.kind.start()
    }
    pub(super) fn update_judge(&mut self, dead: bool) {
        if dead {
            self.counter += 1;
        }
    }
    pub(super) fn update_scene(&mut self, dead: bool) {
        // WipeCloseAppearWaitEnd: each nerve chooses the next one for the next update.
        // WipeMiss's original layout has CloseStart=22 and CloseEnd=28 (pai1 frames).
        self.miss = match self.miss {
            Miss::Playing if dead => Miss::Miss,
            Miss::Miss if self.ready() => Miss::CloseStart(0),
            Miss::CloseStart(22) => Miss::CloseWait(0),
            Miss::CloseStart(n) => Miss::CloseStart(n + 1),
            Miss::CloseWait(n) if n >= self.kind.wait() => Miss::CloseEnd(0),
            Miss::CloseWait(n) => Miss::CloseWait(n + 1),
            Miss::CloseEnd(28) => Miss::Closed,
            Miss::CloseEnd(n) => Miss::CloseEnd(n + 1),
            Miss::Closed => {
                self.restart_pending = !self.external_reload;
                Miss::Closed
            }
            state => state,
        };
    }
    /// Presentation adapter; the original uses its animated WipeMiss layout.
    pub fn wipe_alpha(&self) -> f32 {
        match self.miss {
            Miss::Playing | Miss::Miss => 0.,
            Miss::CloseStart(n) => n as f32 / 22.,
            _ => 1.,
        }
    }
}
impl Player {
    pub fn restart_after_death(&mut self, scene: &Course) {
        let pose = self.death.restart_pose.unwrap_or((self.checkpoint, 180.));
        let external = self.death.external_reload;
        let generation = self.death.generation + 1;
        let camera = self.camera_cm;
        *self = Self::spawn_facing(pose.0, pose.1);
        self.ground_contact = 0;
        // al::tryGetQuat(PlacementInfo) uses f32 radians and half-angle libm.
        let half = (pose.1 * pose_math::DEGREES_TO_RADIANS) * 0.5;
        let native = Quat::from_xyzw(0., console_libm::sinf(half), 0., console_libm::cosf(half));
        self.set_pose(pose_math::to_quat(pose_math::quat_to_axes(native)));
        self.death.restart_pose = Some(pose);
        self.death.external_reload = external;
        self.death.generation = generation;
        self.camera_cm = camera;
        self.entrance_camera = entrance_camera::EntranceCamera::Starting;
        // The new actor is constructed at the selected placement before its first Wait.
        if let Some(mesh) = &scene.mesh {
            let cm = mesh.snap_to_game_precision(terrain::to_cm_exact(pose.0));
            self.collider_end = Some((self.pos, cm));
        }
    }
}
