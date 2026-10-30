//! Jump request 13, PlayerStateJump::exeJumpSpinFlower (0x472c84).
use super::*;

#[derive(Clone, Debug, Default)]
pub(super) struct SpinFlower {
    pub power: f32,
    pub stay: i32,
}

impl Player {
    pub(super) fn request_spin_flower(&mut self, power: f32) {
        self.spin_flower.power = power;
        self.go(Move::SpinFlower, "SpinJumpStart");
        // A sensor request changes the actor before its pre-movement nerve update.
        self.nerve.deferred = false;
        self.nerve.renewed = false;
        self.nerve.actor = "Jump";
        self.nerve.step = 0;
        self.sensor_restart = true;
        self.nerve.touchdown = false;
    }

    pub(super) fn spin_flower_step(&mut self, _o: &Orders) {
        if self.frame == 0 {
            self.v = vector_lib::verticalize_vec(Vec3::NEG_Y, self.v);
            self.v += Vec3::Y * self.spin_flower.power;
        }
        if self.anim.name == "SpinJumpStart" && self.anim.frame >= 26. {
            self.start_clip("SpinJumpLoop");
        }
        if self.touched.ceiling.is_some() {
            self.v.y = self.v.y.min(0.);
        }
        let mut gravity = self.constants.jump_gravity;
        let mut fall_max = 8.;
        if self.v.y < 0. || self.v.y.abs() < 0.001 {
            gravity = 0.1;
            if self.spin_flower.stay - 1 >= 0 {
                self.spin_flower.stay -= 1;
                fall_max = 1.;
            }
        } else {
            self.spin_flower.stay = 80;
        }
        let input = self.view.as_ref().map_or_else(
            || self.push(),
            |view| move_input::move_input(self.raw_stick, view, Vec3::Y),
        );
        air_move::move_parallel_jump(
            &mut self.v,
            input,
            &air_move::ParallelJump {
                front: 1.,
                back: 1.,
                side: 1.,
                speed_max: 9.,
                gravity,
                fall_max,
            },
        );
        if self.tilt < STICK_DEADZONE {
            self.set_h(self.v.with_y(0.) * 0.95);
        }
    }
}
