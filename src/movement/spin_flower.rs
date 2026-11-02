//! Jump request 13, PlayerStateJump::exeJumpSpinFlower (0x472c84).
use super::*;

#[derive(Clone, Debug, Default)]
pub(super) struct SpinFlower {
    pub power: f32,
    pub stay: i32,
}

impl Player {
    pub(super) fn request_spin_flower(&mut self, power: f32) {
        // appear 0x470e9c..0x470fc4 clears +0xb4 for special requests;
        // tryCountUpContinuousJump 0x471f70..0x471fa8 clears on ground exit.
        self.combo_kept = false;
        self.combo.clear(t::CONTINUOUS_JUMP_TIMER);
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

    pub(super) fn spin_flower_step(&mut self) {
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
        // exeJumpSpinFlower 0x472e90..0x472eac: descending, or `al::isNearZero(calcSpeedV, 0.001)`
        // with the tolerance passed explicitly from the literal at 0x187fc80.
        if self.v.y < 0. || self.v.y.abs() < 0.001 {
            gravity = 0.1;
            if self.spin_flower.stay > 0 {
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

pub(super) fn self_check() {
    checks::a_request_preserves_horizontal_velocity_and_replaces_the_fall();
    checks::descent_spends_the_stay_counter_before_the_full_fall_limit();
}

mod checks {
    use super::*;

    #[cfg_attr(test, test)]
    pub(super) fn a_request_preserves_horizontal_velocity_and_replaces_the_fall() {
        let scene = Course::default();
        let mut p = Player {
            mode: Move::GroundPound,
            v: Vec3::new(3., -45., 4.),
            ..default()
        };
        p.combo_kept = true;
        p.combo.count = 1;
        p.request_spin_flower(60.);
        assert!(!p.combo_kept);
        assert_eq!(p.combo.count, 0);
        p.begin_step(p.acting_nerve());
        p.spin_flower_step();
        assert_eq!(p.nerve_name().name, "Jump/JumpSpinFlower");
        assert_eq!(p.anim.name, "SpinJumpStart");
        assert_eq!(p.v, Vec3::new(3. * 0.95, 58.5, 4. * 0.95));
        assert_eq!(p.spin_flower.stay, 80);
        assert!(p.restarted_by_sensor());
        p.anim.frame = 26.;
        p.frame = 26;
        p.spin_flower_step();
        assert_eq!((p.anim.name, p.anim.frame), ("SpinJumpLoop", 0.));
        let before = p.trampoline_diagnostic();
        p.repeat_step_for_lockstep(&Orders::default(), &scene);
        assert_eq!(p.trampoline_diagnostic(), before);
    }

    #[cfg_attr(test, test)]
    pub(super) fn descent_spends_the_stay_counter_before_the_full_fall_limit() {
        let mut p = Player {
            mode: Move::SpinFlower,
            frame: 1,
            v: Vec3::NEG_Y,
            ..default()
        };
        p.spin_flower.stay = 80;
        for left in (0..80).rev() {
            p.spin_flower_step();
            assert_eq!(p.spin_flower.stay, left);
            assert_eq!(p.v.y, -1.);
        }
        p.spin_flower_step();
        assert_eq!(p.v.y, -1.1);
        assert_eq!(p.spin_flower.stay, 0);
        for _ in 0..100 {
            p.spin_flower_step();
        }
        assert_eq!(p.v.y, -8.);
    }
}
