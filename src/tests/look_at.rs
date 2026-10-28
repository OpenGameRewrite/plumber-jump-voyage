use super::*;

pub(super) fn run() {
    the_states_that_may_look_are_the_originals();
    clipping_invalidates_the_holder_until_the_sensor_returns();
    the_holder_keeps_the_nearest_sensor_the_eye_overlaps();
    the_holder_compares_distances_from_the_eye_of_the_frame_before();
    the_keeper_takes_a_target_in_reach_and_asks_for_it();
    the_keeper_drops_a_target_that_leaves_the_gate_after_forty_frames();
    a_nearer_sensor_replaces_the_target_and_a_state_without_look_ends_it();
}

fn sensor(name: &str, at: Vec3) -> LookSensor {
    LookSensor {
        host: name.into(),
        name: "Body".into(),
        kind: 12,
        position: at,
        stage_position: at,
        radius: 70.,
    }
}

/// An upright actor at `feet` facing +z, as the matrix `LookAt::step` takes.
fn actor_at(feet: Vec3) -> Mat34 {
    [
        [1., 0., 0., feet.x],
        [0., 1., 0., feet.y],
        [0., 0., 1., feet.z],
    ]
}

#[cfg_attr(test, test)]
fn the_states_that_may_look_are_the_originals() {
    for label in [
        "Wait/Wait",
        "Wait/WaitRelaxStart",
        "Wait/WaitRelax",
        "Run/Run3D",
    ] {
        assert!(look_enabled(label), "{label}");
    }
    for label in [
        "Wait/Land",
        "Wait/LandStiffen",
        "Wait/WaitSnooze",
        "Wait/WaitSleepStart",
        "Jump/Jump",
        "Fall/Fall",
        "Swim/SwimDive",
        "Swim/SwimHipDropStart",
        "Swim/SwimFallIn",
    ] {
        assert!(!look_enabled(label), "{label}");
    }
    for label in [
        "Swim/SwimSurface",
        "Swim/SwimMove",
        "Swim/SwimPaddle",
        "Swim/SwimLand",
        "Swim/SwimWalk",
        "Swim/SwimTrample",
        "Swim/SwimReflectDownFall",
    ] {
        assert!(look_enabled(label), "{label}");
    }
}

#[cfg_attr(test, test)]
fn the_holder_keeps_the_nearest_sensor_the_eye_overlaps() {
    let sensors = [
        sensor("far", Vec3::new(0., 80., 800.)),
        sensor("near", Vec3::new(0., 80., 500.)),
        sensor("out of reach", Vec3::new(0., 80., 3000.)),
    ];
    let mut look = LookAt::default();
    look.step(&actor_at(Vec3::ZERO), false, &sensors);
    // The far one overlaps first and the nearer replaces it.
    assert_eq!(look.sensors_cell(&sensors), "candidate=near/Body/12");
    // The candidate stays after the player walks out of its reach.
    look.step(&actor_at(Vec3::new(0., 0., -5000.)), false, &sensors);
    look.step(&actor_at(Vec3::new(0., 0., -5000.)), false, &sensors);
    assert_eq!(look.sensors_cell(&sensors), "candidate=near/Body/12");
}

#[cfg_attr(test, test)]
fn the_holder_compares_distances_from_the_eye_of_the_frame_before() {
    let sensors = [
        sensor("a", Vec3::new(0., 80., 100.)),
        sensor("b", Vec3::new(0., 80., -100.)),
    ];
    let mut look = LookAt::default();
    look.step(&actor_at(Vec3::ZERO), false, &sensors);
    assert_eq!(look.sensors_cell(&sensors), "candidate=a/Body/12");
    // The eye moves next to "b", but the sensor phase still sees it where it was: a tie
    // keeps the candidate. A step later the new position decides.
    look.step(&actor_at(Vec3::new(0., 0., -300.)), false, &sensors);
    assert_eq!(look.sensors_cell(&sensors), "candidate=a/Body/12");
    look.step(&actor_at(Vec3::new(0., 0., -300.)), false, &sensors);
    assert_eq!(look.sensors_cell(&sensors), "candidate=b/Body/12");
}

#[cfg_attr(test, test)]
fn the_keeper_takes_a_target_in_reach_and_asks_for_it() {
    let sensors = [sensor("coin", Vec3::new(0., 90., 300.))];
    let mut look = LookAt::default();
    look.set_head_position(Vec3::new(0., 90., 0.));
    let actor = actor_at(Vec3::ZERO);
    // The sensor phase records the candidate and the keeper, running after it in the same
    // step, takes it.
    look.step(&actor, true, &sensors);
    assert_eq!(
        look.keeper_cell(),
        "mode=1;count=0;suppress=0;headjoint=14;requested=1;ctrlmode=1;accept=1"
    );
    assert_eq!(look.requested_sensor(), Some(0));
    assert_eq!(
        look.sensors_cell(&sensors),
        "candidate=coin/Body/12;target=coin/Body/12"
    );
    look.step(&actor, true, &sensors);
    assert!(look.keeper_cell().starts_with("mode=1;count=1;"));
}

#[cfg_attr(test, test)]
fn the_keeper_drops_a_target_that_leaves_the_gate_after_forty_frames() {
    let sensors = [sensor("coin", Vec3::new(0., 90., 300.))];
    let mut look = LookAt::default();
    let actor = actor_at(Vec3::ZERO);
    look.set_head_position(Vec3::new(0., 90., 0.));
    // Taken, then the head moves 1000 cm away: the request goes on without the gate while the
    // counter is at most 39 and stops at the first check after that.
    look.step(&actor, true, &sensors);
    look.set_head_position(Vec3::new(0., 90., -1000.));
    let mut asked = 0;
    for _ in 0..60 {
        look.step(&actor, true, &sensors);
        if look.requested_sensor().is_some() {
            asked += 1;
        }
    }
    assert_eq!(asked, 40, "frames asked before the gate dropped the target");
    assert_eq!(look.sensors_cell(&sensors), "candidate=coin/Body/12");
    assert!(look.keeper_cell().starts_with("mode=1;count=-1;"));
}

#[cfg_attr(test, test)]
fn a_nearer_sensor_replaces_the_target_and_a_state_without_look_ends_it() {
    let sensors = [
        sensor("first", Vec3::new(0., 90., 350.)),
        sensor("second", Vec3::new(0., 90., 120.)),
    ];
    let mut look = LookAt::default();
    look.set_head_position(Vec3::new(0., 90., 0.));
    let actor = actor_at(Vec3::ZERO);
    // Both overlap from the start; the nearer is the holder's candidate, so it is the target.
    look.step(&actor, true, &sensors);
    look.step(&actor, true, &sensors);
    assert_eq!(
        look.sensors_cell(&sensors),
        "candidate=second/Body/12;target=second/Body/12"
    );
    // A state that may not look forgets it.
    look.step(&actor, false, &sensors);
    assert_eq!(look.requested_sensor(), None);
    assert!(look.keeper_cell().starts_with("mode=0;count=-1;"));
}

#[cfg_attr(test, test)]
fn clipping_invalidates_the_holder_until_the_sensor_returns() {
    let sensors = [sensor("candle", Vec3::new(0., 80., 100.))];
    let mut look = LookAt::default();
    let actor = actor_at(Vec3::ZERO);
    look.step(&actor, false, &sensors);
    assert!(look.candidate.is_some());
    look.set_invalid_sensors(vec![0]);
    look.step(&actor, false, &sensors);
    assert!(look.candidate.is_none());
    look.set_invalid_sensors(Vec::new());
    look.step(&actor, false, &sensors);
    assert_eq!(look.candidate, Some(0));
}
