//! PlayerStateRun2D's ordinary Run child (0x47da54 / 0x47db54 / 0x47dd0c).
use super::*;

impl Player {
    pub(super) fn run_2d_step(&mut self, o: &Orders) {
        let up = self.mesh_ground.unwrap_or(Vec3::Y);
        let velocity = self.velocity_in_plane(up);
        let (actor_up, actor_front) = self.pose_axes();
        if self.run_steps == 0 {
            self.ground_2d.reset(velocity, actor_front, up);
        }
        // PlayerJudgeStartDash 0x45b374 -> isEnableDashInput 0x44d9c8:
        // held Action. PlayerConst stores max17/accel15 at 0x438b84,
        // dash accel30 at 0x438c30, normal max10 at 0x4385c8.
        let (max_speed, accel_frames) = if o.throw_held { (17., 30) } else { (10., 15) };
        let steer = self.ground_2d.update(&SteerInput {
            skate: false,
            velocity,
            up,
            actor_front,
            actor_up,
            stick: self.plane_stick(up),
            wall: self
                .touched
                .wall_contact
                .as_ref()
                .map(|w| (w.normal, w.at_face)),
            max_speed,
            accel_frames,
        });
        self.run_steps += 1;
        self.steer_pose(up, steer.front);
        self.set_h(steer.velocity);
        self.v.y = -GRAVITY_MOVE;
        self.pull = GRAVITY_MOVE;
        self.run_up = Some((Vec2::new(self.v.x, self.v.z), up, steer.set_velocity));
        self.mode = Move::Run;
        // exeRun 0x47dd68..0x47de04: easeOut of the speed above normalMax,
        // over min(dashMax-normalMax, AnimFrameRateRange2D), then lerp.
        // Constructor stores range10 0x438c40, rates1/2.5 0x4388b4/0x438974.
        let rate = joint_math::ease_out((steer.speed - 10.).max(0.) / (17_f32 - 10.).min(10.));
        self.anim.rate = joint_math::lerp_value(1., 2.5, rate);
        // Run wrapper projects pose up off the area lock after its child (0x480434).
        if self.grounded
            && let Some(frame) = &self.input_2d_frame
        {
            let (up, usable) = pose_math::try_normalize_or_zero(vector_lib::verticalize_vec(
                frame.lock,
                self.actor_pose().up(),
            ));
            if usable {
                self.set_pose(pose_math::quat_up_front(up, self.actor_pose().front()));
            }
        }
        if steer.brake_run {
            // Dedicated Brake/Turn execution remains outside this bounded port.
            self.switch(Move::Skid, "Brake", |p| p.enter_nerve(RunNerve::Brake));
        } else if steer.stopped {
            self.end_ground_child();
        } else {
            self.ground_actions(o);
        }
    }
}
