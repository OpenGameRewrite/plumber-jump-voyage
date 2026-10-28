//! `PlayerActionCollisionSnap`: what holds the body to a surface it hangs from
//! or climbs along. The ledge hang (`PlayerStateWallCatch`, `ledge.rs`) and the
//! pole climb (`PlayerStatePoleClimb`, `pole.rs`) both set a pose on it, move
//! the body to a new pose over some steps (eased out) and put the actor on the
//! current pose at the start of every step (`followCollision`).
//!
//! Written from the 1.0.0 executable. The game keeps full matrices; the
//! ledge and pole consumers retain their upright position/yaw path. GrabCeil
//! uses full static orientation without changing that arithmetic. Following
//! moving collision parts is not ported.
use bevy::prelude::{Quat, Vec3};

/// The single-precision degree factor the game multiplies by.
const DEGREES_TO_RADIANS: f32 = 0.017_453_292;

/// `al::easeOut`.
pub(super) fn ease_out(t: f32) -> f32 {
    (-0.5 * t * t + 1.5) * t
}

/// An upright pose: where the body is and which way it faces (the yaw).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Pose {
    pub(super) pos: Vec3,
    pub(super) yaw: f32,
}
impl Pose {
    /// `al::blendMtx`: the position blends linearly, the orientation by
    /// spherical interpolation of the quaternions.
    pub(super) fn blend(from: Pose, to: Pose, rate: f32) -> Pose {
        let turn = Quat::from_rotation_y(from.yaw).slerp(Quat::from_rotation_y(to.yaw), rate);
        let front = turn * Vec3::Z;
        Pose {
            pos: from.pos * (1. - rate) + to.pos * rate,
            yaw: front.x.atan2(front.z),
        }
    }
    /// The direction the pose faces.
    pub(super) fn front(self) -> Vec3 {
        Vec3::new(self.yaw.sin(), 0., self.yaw.cos())
    }
}

/// `PlayerActionCollisionSnap`, for static collision: the pose the body holds
/// (position, the direction it faces into the surface and the axis it is
/// upright along) and the move between two poses.
#[derive(Clone, Debug)]
pub(super) struct Snap {
    pub(super) pos: Vec3,
    pub(super) front: Vec3,
    pub(super) axis: Vec3,
    /// Where the body stood when the last follow began (`_3c`): a move starts
    /// from there.
    pub(super) previous: Vec3,
    /// The pose the actor was put in by the last follow (`_48`).
    pub(super) held: Vec3,
    pub(super) start: Pose,
    pub(super) target: Pose,
    pub(super) current: Pose,
    /// Steps of the move and steps taken (`_98`, `_9c`).
    pub(super) frames: i32,
    pub(super) at: i32,
}
impl Default for Snap {
    fn default() -> Self {
        Self {
            pos: Vec3::ZERO,
            front: Vec3::ZERO,
            axis: Vec3::Y,
            previous: Vec3::ZERO,
            held: Vec3::ZERO,
            start: Pose::default(),
            target: Pose::default(),
            current: Pose::default(),
            frames: 0,
            at: 0,
        }
    }
}
impl Snap {
    /// Full static snap orientation for GrabCeil. Upright consumers retain their
    /// existing yaw path and its arithmetic.
    pub(super) fn orientation(front: Vec3, up: Vec3) -> Quat {
        super::pose_math::quat_up_front(up, front)
    }
    /// The pose of the point: the point itself, facing along the front.
    fn snap_pose(&self) -> Pose {
        let front = self.front.with_y(0.).try_normalize().unwrap_or(Vec3::Z);
        Pose {
            pos: self.pos,
            yaw: front.x.atan2(front.z),
        }
    }
    /// `PlayerActionCollisionSnap::setup`.
    pub(super) fn setup(&mut self, pos: Vec3, front: Vec3, axis: Vec3) {
        self.pos = pos;
        self.front = front;
        self.axis = axis;
        self.target = self.snap_pose();
        self.start = self.target;
        self.current = self.target;
        self.frames = 0;
        self.at = 0;
    }
    /// `moveSnapPos`: a move to another point over `frames` steps, from where
    /// the body stood and the way it faced.
    pub(super) fn move_to(
        &mut self,
        facing_yaw: f32,
        pos: Vec3,
        front: Vec3,
        axis: Vec3,
        frames: i32,
    ) {
        self.pos = pos;
        self.front = front;
        self.axis = axis;
        self.start = Pose {
            pos: self.previous,
            yaw: facing_yaw,
        };
        self.target = self.snap_pose();
        self.current = self.start;
        self.frames = frames;
        self.at = 0;
    }
    /// `updateMove`: one step of the move, eased out.
    pub(super) fn update_move(&mut self) {
        if self.at >= self.frames {
            return;
        }
        self.at += 1;
        let rate = (self.at as f32 / self.frames as f32).clamp(0., 1.);
        self.current = Pose::blend(self.start, self.target, ease_out(rate));
    }
    pub(super) fn moving(&self) -> bool {
        self.frames != self.at
    }
    /// `restartMoveCurrentMtx`: the move on to the same target starts again
    /// from the pose the body has now, over `frames` steps.
    pub(super) fn restart_move(&mut self, frames: i32) {
        self.start = self.current;
        self.frames = frames;
        self.at = 0;
    }
    /// `turnSnapFrontAxisUp`: the front turns about the axis by `degrees`; the
    /// pose follows at once unless a move is still running (then it is the
    /// move's target that turns).
    pub(super) fn turn_front(&mut self, degrees: f32) {
        let turn = Quat::from_axis_angle(
            self.axis.try_normalize().unwrap_or(Vec3::Y),
            degrees * DEGREES_TO_RADIANS,
        );
        self.front = (turn * self.front).normalize_or(self.front);
        self.update_pose();
    }
    /// `updateSnapPose`.
    fn update_pose(&mut self) {
        if self.frames == 0 {
            self.current = self.snap_pose();
            return;
        }
        self.target = self.snap_pose();
        if self.at < self.frames {
            return;
        }
        // The move is over: the pose is the target.
        self.at = self.frames;
        self.current = Pose::blend(self.start, self.target, ease_out(1.));
    }
    /// `forceMoveEndNearestLeaveDir`: a body that leaves while a move is
    /// running ends it at the end that faces more nearly along (or against)
    /// the way it leaves: the target when that is at least as aligned, else
    /// where the move began.
    pub(super) fn end_move_nearest(&mut self, leave: Vec3) {
        if self.frames == 0 {
            return;
        }
        let along = |pose: Pose| pose.front().dot(leave).abs();
        let rate = if near_zero_or_greater(along(self.target) - along(self.start), 0.001) {
            1.
        } else {
            0.
        };
        self.at = self.frames;
        self.current = Pose::blend(self.start, self.target, rate);
    }
    /// What `followCollision` does for static collision: remembers where the
    /// actor stood (`_3c`), puts it on the current pose and keeps as the held
    /// position what that moved it, less any part of the move that goes
    /// against the push of the collision last step (`fix`).
    pub(super) fn follow(&mut self, actor_pos: Vec3, fix: Vec3) -> Pose {
        self.previous = actor_pos;
        let pose = self.current;
        self.held = pose.pos;
        if fix.length_squared() >= 1e-6 {
            let fix = fix.normalize();
            let moved = pose.pos - actor_pos;
            let against = moved.dot(fix);
            if against < 0. {
                self.held = actor_pos + (moved - fix * against);
            }
        }
        pose
    }
}

/// `al::isNearZeroOrGreater`.
pub(super) fn near_zero_or_greater(value: f32, tolerance: f32) -> bool {
    value >= 0. || value.abs() < tolerance
}
