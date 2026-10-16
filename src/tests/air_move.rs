use super::*;

pub(super) fn run() {
    jump_power_between_its_bounds();
    standing_jump_keeps_gravity_off_while_held();
    tapped_jump_falls_after_one_frame();
    stick_pushes_along_the_travel();
    backflip_and_long_jump_arithmetic();
    combo_counts_and_expires();
}

fn takeoff(extend: u32) -> Takeoff {
    Takeoff {
        speed_limit: 30.,
        base_speed: 14.,
        extend_frames: extend,
        power: 17.,
        gravity: 1.5,
        input_lock: 0,
        speed_window: Some((0., tune::JUMP_BASE_SPEED_MAX)),
        fall_max: 35.,
    }
}
const STEER: Steering = Steering {
    front: 0.5,
    back: 1.,
    turn: 0.3,
};

#[cfg_attr(test, test)]
fn jump_power_between_its_bounds() {
    // Standing still, running at the top speed, and in between.
    assert_eq!(jump_power(0., 3., 14., 17., 19.5), 17.);
    assert_eq!(jump_power(14., 3., 14., 17., 19.5), 19.5);
    assert!((jump_power(8.5, 3., 14., 17., 19.5) - 18.25).abs() < 1e-5);
    assert_eq!(jump_power(30., 3., 14., 17., 19.5), 19.5);
}

#[cfg_attr(test, test)]
fn standing_jump_keeps_gravity_off_while_held() {
    let mut air = AirMove::default();
    let mut v = Vec3::new(0., -3., 0.);
    air.setup(&mut v, Vec3::Z, &takeoff(10));
    assert_eq!(v, Vec3::new(0., 17., 0.));
    // Ten frames with the button down keep the power, the eleventh pulls.
    for _ in 0..10 {
        air.update(&mut v, Vec3::ZERO, true, &STEER);
        assert_eq!(v.y, 17.);
    }
    air.update(&mut v, Vec3::ZERO, true, &STEER);
    assert_eq!(v.y, 15.5);
}

#[cfg_attr(test, test)]
fn tapped_jump_falls_after_one_frame() {
    let mut air = AirMove::default();
    let mut v = Vec3::ZERO;
    air.setup(&mut v, Vec3::Z, &takeoff(10));
    air.update(&mut v, Vec3::ZERO, false, &STEER);
    assert_eq!(v.y, 17., "the first frame still skips gravity");
    air.update(&mut v, Vec3::ZERO, false, &STEER);
    assert_eq!(v.y, 15.5);
}

#[cfg_attr(test, test)]
fn stick_pushes_along_the_travel() {
    let mut air = AirMove::default();
    let mut v = Vec3::new(0., 0., 6.);
    air.setup(&mut v, Vec3::Z, &takeoff(0));
    // Forward half a unit per frame up to the cap (the base speed, 14).
    for _ in 0..40 {
        air.update(&mut v, Vec3::Z, false, &STEER);
    }
    assert!((v.z - 14.).abs() < 1e-4, "{v}");
    // Pulling back brakes twice as hard.
    let before = v.z;
    air.update(&mut v, Vec3::NEG_Z, false, &STEER);
    assert!((before - v.z - 1.).abs() < 1e-4);
}

#[cfg_attr(test, test)]
fn backflip_and_long_jump_arithmetic() {
    // The backflip keeps its speed with the stick at rest and falls at its gravity.
    let mut v = Vec3::new(0., 32., -5.);
    let back = ParallelJump {
        front: 0.2,
        back: 0.2,
        side: 0.2,
        speed_max: 9.,
        gravity: 1.,
        fall_max: 35.,
    };
    move_parallel_jump(&mut v, Vec3::ZERO, &back);
    assert_eq!(v, Vec3::new(0., 31., -5.));
    // The long jump leaves at the clamped speed and the power, then speeds up.
    let mut v = air_move_long(Vec3::new(0., -3., 15.96));
    assert!((v.z - 14.).abs() < 1e-5 && (v.y - 12.).abs() < 1e-5);
    let mut facing = Vec3::Z;
    let dive = DivingJump {
        accel: 0.25,
        brake: 0.5,
        speed_max: 23.,
        speed_min: 2.5,
        side_accel: 0.25,
        gravity: 0.48,
        fall_max: 35.,
        turn_rate: 0.4,
    };
    move_diving_jump(&mut v, &mut facing, Vec3::Z, &dive);
    assert!((v.z - 14.25).abs() < 1e-5 && (v.y - 11.52).abs() < 1e-5);
}
fn air_move_long(v: Vec3) -> Vec3 {
    long_jump_velocity(
        v,
        &LongJumpTakeoff {
            move_pow: 4.,
            speed_min: 2.5,
            init_speed: 14.,
            power: 12.,
        },
    )
}

#[cfg_attr(test, test)]
fn combo_counts_and_expires() {
    let mut combo = ContinuousJump::default();
    combo.count_up(Vec3::Z);
    assert_eq!((combo.count, combo.timer), (1, 0));
    for _ in 0..9 {
        combo.update(true, 10);
    }
    assert_eq!((combo.count, combo.timer), (1, 9));
    combo.update(true, 10);
    assert_eq!((combo.count, combo.timer), (0, 10), "the window ran out");
    combo.count_up(Vec3::Z);
    combo.update(false, 10);
    assert_eq!(
        (combo.count, combo.timer),
        (0, 10),
        "any other state clears it"
    );
    let mut long = ContinuousLongJump::default();
    long.count_up();
    assert_eq!((long.count, long.timer), (1, 0));
    for _ in 0..15 {
        long.update(15);
    }
    assert_eq!((long.count, long.timer), (0, 0));
}
