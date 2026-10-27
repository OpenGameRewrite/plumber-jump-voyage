//! The ordinary run's wall-push child, reached after jumping out of sand.
//! PlayerJudgeWallPush 0x45d414, exeRun 0x47f4c4, exeWallPush 0x47f9c0.
use super::vector_lib::verticalize_vec;
use super::*;

impl Player {
    /// The step that runs the push nerve (or chooses its end) keeps the nerve's own velocity
    /// through the collision, like the other velocity-owning states.
    pub(super) fn in_wall_push_frame(&self) -> bool {
        self.run_nerve==RunNerve::WallPush
            && (matches!(self.mode,Move::Walk|Move::Run) || self.mode==Move::Idle && self.nerve.deferred)
    }
    fn wall_push_judge(&self) -> bool {
        let Some(wall) = &self.touched.wall_contact else {return false;};
        if !self.is_collided_ground() || !self.is_move()
            || self.touched.wall.as_ref().is_some_and(|c| c.wall == "NoActionWall") {return false;}
        let up=self.collided_ground_normal();
        if pose_math::dot(up,wall.normal).abs()>0.173_648_18 {return false;}
        let (front,valid)=pose_math::try_normalize_or_zero(verticalize_vec(up,self.pose_axes().1));
        valid && -pose_math::dot(wall.normal,front)>=0.707_106_77
            && pose_math::dot(self.plane_stick(self.pose_axes().0).normalize_or_zero(),front)>=0.707_106_77
    }
    /// Two body-width arrows must both hit: a single corner cannot be pushed.
    fn find_wall_push(&self, scene: &Course) -> bool {
        let (Some(mesh),Some(wall))=(&scene.mesh,&self.touched.wall_contact) else {return false;};
        let up=self.collided_ground_normal();
        let side=self.actor_pose().side();
        let at=self.position_cm()+up*((160.+160.)/3.);
        let vector=-wall.normal*(55.+10.);
        [at+side*(55.*0.5),at-side*(55.*0.5)].into_iter().all(|p|
            crate::collider::first_strike(mesh,p,vector,crate::collider::FaceFilter::Any).is_some())
    }
    pub(super) fn judge_wall_push(&mut self, scene: &Course) {
        if !matches!(self.mode,Move::Walk|Move::Run) || self.nerve.deferred || self.run_nerve==RunNerve::WallPush {return;}
        self.wall_push_counter=if self.wall_push_judge(){(self.wall_push_counter+1).min(15)}else{0};
        if self.wall_push_counter==15 {
            if self.find_wall_push(scene) {
                self.switch(Move::Run,"Push",|p|p.enter_nerve(RunNerve::WallPush));
                self.run_nerve=RunNerve::WallPush;
            } else {self.wall_push_counter=0;}
        }
    }
    pub(super) fn wall_push_step(&mut self,o:&Orders) {
        self.pull = t::GRAVITY;
        if self.touched.wall_contact.is_none() || !self.is_move() {
            // The stand is chosen now but the push's step still collides with its own velocity.
            self.change(Move::Idle,"Wait",|p|{p.set_h(Vec3::ZERO);p.run_nerve=RunNerve::Run;});
            return;
        }
        let wall=self.touched.wall_contact.as_ref().unwrap().normal;
        let up=self.collided_ground_normal();
        let front=joint_math::lerp_vec(self.pose_axes().1,-wall,0.1);
        self.set_pose(self.actor_pose().slerp_up_front(up,front,t::SLERP_QUAT_RATE_WAIT,t::WAIT_POSE_DEGREE_MAX));
        // Zero-restitution rebound, gravity along support, then NormalMinSpeed into wall.
        let d=pose_math::dot(self.v,up);
        if d<0. {self.v=Vec3::ZERO;}
        self.v=vector_lib::limit_length(self.v-up*t::GRAVITY,t::FALL_MAX)-wall*t::NORMAL_MIN;
        self.run_steps+=1;
        if !self.wall_push_judge() {
            self.switch(Move::Run,"Move",|p|p.enter_nerve(RunNerve::Run));
        }
        if self.ground_actions(o) {return;}
    }
}
