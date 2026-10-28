//! SpinAir's handover from a tilted model root (updateSpinAirNerve, 0x482a50).
//! The actor takes the model orientation, keeping the root's world translation,
//! then pays the translation back over ten steps. This is exercised by throwing
//! out of DiveInWater; an upright air throw keeps the ordinary path.
use super::*;

impl Player {
    pub(super) fn start_spin_air_model_pose(&mut self) {
        let root = self.model_axes.root;
        let axes = std::array::from_fn(|c| Vec3::new(root[0][c], root[1][c], root[2][c]));
        let root_at = Vec3::new(root[0][3], root[1][3], root[2][3]);
        let old = self.actor_pose();
        let relative = root_at - self.position_cm();
        let local = Vec3::new(
            pose_math::dot(relative, old.side()),
            pose_math::dot(relative, old.up()),
            pose_math::dot(relative, old.front()),
        );
        let direction = pose_math::normalize(vector_lib::verticalize_vec(
            Vec3::Y,
            if self.tilt >= STICK_DEADZONE {
                self.stick
            } else {
                self.facing
            },
        ));
        self.set_pose(pose_math::to_quat(axes));
        let pose = self.actor_pose();
        let (mut up, mut front) = (pose.up(), pose.front());
        let along_up = pose_math::dot(up, direction);
        let along_front = pose_math::dot(front, direction);
        if along_up.abs() > along_front.abs() {
            let vertical = vector_lib::parallelize_vec(Vec3::Y, up);
            let horizontal = vector_lib::verticalize_vec(Vec3::Y, up);
            let mut length = vector_lib::length(horizontal);
            let front_up = pose_math::dot(front, Vec3::Y);
            if (along_up > 0. && front_up > 0.) || (along_up < 0. && front_up < 0.) {
                length = -length;
            }
            up = vertical + direction * length;
        } else {
            front = direction;
        }
        if pose_math::is_parallel_direction(up, front, 0.01) {
            up = Vec3::Y;
            front = direction;
        }
        self.set_pose(self.actor_pose().slerp_up_front(up, front, 1., 180.));
        let pose = self.actor_pose();
        let transformed = pose.side() * local.x
            + pose.up() * local.y
            + pose.front() * local.z
            + self.position_cm();
        let delta = root_at - transformed;
        self.pos += delta * UNIT;
        self.throwing.model_shift = Some(delta * 0.1);
        self.throwing.dir = Some(direction);
    }
}
