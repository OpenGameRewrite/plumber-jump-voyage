//! Picking an object up in the ground spin (`PlayerActorHakoniwa::attackSensor` 0x427818,
//! `PlayerStateSpinCap::isEnableCancelCarry` 0x481f10) and the diversion that follows it
//! (`exeSpinCap` 0x423a1c).
use super::*;

pub(super) fn run() {
    an_active_ground_spin_picks_an_object_up_after_the_cap_has_left();
    a_carried_object_ends_the_ground_spin_through_set_nerve_on_ground();
    the_air_spin_and_the_other_refused_states_pick_nothing_up();
}

fn slab(center_y: f32, floor_code: &'static str, slope: f32) -> Solid {
    Solid {
        center: Vec3::new(0., center_y, 0.),
        half: Vec3::new(100., 0.5, 100.),
        slope,
        moving: false,
        floor_code,
    }
}

/// The stage's collision: the floor as a triangle mesh, which is where a pickup looks for the
/// object (`carry_sensor_phase`).
fn stage() -> Course {
    let solids = vec![slab(-0.5, "Ground", 0.)];
    let mesh = Arc::new(toy_mesh(&solids, &[]));
    Course {
        solids,
        mesh: Some(mesh),
        ..default()
    }
}

/// A stone lying beside the player's feet. (A seed would do as well, but its pickup also shoots an
/// arrow at the player's feet, which touches the floor the toy stage ends the arrow on.)
fn stone_beside(p: &Player, course: &Course) -> kick_stone::Stone {
    let feet = course
        .mesh
        .as_ref()
        .map_or(p.position_cm(), |mesh| mesh.to_game_vec(p.position_cm()));
    kick_stone::Stone::new(&kick_stone::Source {
        id: "stone".into(),
        position: feet + Vec3::new(20., 10., 0.),
        joint: Quat::IDENTITY,
    })
}

/// A seed lying beside the player's feet, old enough to be picked up.
fn seed_beside(p: &Player, course: &Course) -> carry::Seed {
    let feet = course
        .mesh
        .as_ref()
        .map_or(p.position_cm(), |mesh| mesh.to_game_vec(p.position_cm()));
    let mut seed = carry::Seed::new("seed".into(), feet + Vec3::X * 20.);
    seed.age = grow_seed::PICKUP_AGE;
    seed
}

fn press() -> Orders {
    Orders {
        throw_pressed: true,
        throw_held: true,
        ..default()
    }
}

fn hold() -> Orders {
    Orders {
        throw_held: true,
        ..default()
    }
}

/// A ground spin whose cap is on its way: the step the throw frame launched it on.
fn spin_until_the_cap_launches(course: &Course) -> Player {
    let mut p = Player::default();
    p.step(&press(), course);
    assert_eq!(p.mode, Move::Throw);
    for _ in 0..40 {
        if p.launch.is_some() {
            return p;
        }
        p.step(&Orders::default(), course);
    }
    panic!("the cap never left");
}

/// `attackSensor` (0x427818) asks `PlayerStateSpinCap::isEnableCancelCarry` (0x481f10) while the
/// actor nerve is SpinCap: the ground child answers true when `isCapSpinAttack` (0x465420)
/// is false, before the first executed child step or after the cap's throw started (`startThrow`
/// 0x3fc660 replaces the spin-attack nerve 0x3fc59c set). This test covers the active wind and
/// after-launch window. The pickup sends the actor to `setNerveOnGround` at its next
/// `exeSpinCap` (0x423a1c), which is the same step in the port.
#[cfg_attr(test, test)]
fn an_active_ground_spin_picks_an_object_up_after_the_cap_has_left() {
    let course = stage();
    let mut p = Player::default();
    p.step(&press(), &course);
    assert_eq!(p.mode, Move::Throw);
    // The state's first step starts the spin attack (`exeSpinGround` 0x4820ac).
    p.step(&Orders::default(), &course);
    p.carry.stone = Some(stone_beside(&p, &course));
    // The button is held and the stone is in reach, but the cap is still the spin attack.
    for _ in 0..3 {
        p.step(&hold(), &course);
        assert!(p.winding, "the cap has not left yet");
        assert!(!p.is_carry(), "no pickup while the cap spins");
        assert_eq!(p.mode, Move::Throw);
    }
    while p.launch.is_none() {
        p.step(&hold(), &course);
        assert!(!p.is_carry());
    }
    // The next frame's cap movement starts its throw; the sensors then see the spin attack over.
    p.step(&hold(), &course);
    assert!(!p.winding);
    assert!(p.is_carry(), "the stone is picked up once the cap has left");
    assert!(p.carry.picked_up);
    assert_eq!(p.carry.upper, Some(("CarryFrontStart", 0.)));
}

/// 0x423a1c: after the child's end test, a carried actor goes to
/// `setNerveOnGround` before the water judge and the chain's cancels. Without a carry the same
/// step stays in the spin. The binary's history restore precedes this diversion (0x4239d4);
/// the port restores after run_state and the post-state judges.
#[cfg_attr(test, test)]
fn a_carried_object_ends_the_ground_spin_through_set_nerve_on_ground() {
    let course = stage();
    let mut free = spin_until_the_cap_launches(&course);
    let mut carrying = spin_until_the_cap_launches(&course);
    carrying.carry.held = true;
    carrying.carry.phase = 2;
    carrying.carry.upper = Some(("CarryFront", 0.));
    free.step(&Orders::default(), &course);
    carrying.step(&Orders::default(), &course);
    assert_eq!(free.mode, Move::Throw, "the spin goes on without a carry");
    assert_eq!(
        carrying.mode,
        Move::Idle,
        "the carrier is sent to the ground nerve"
    );
    assert!(
        carrying.is_carry(),
        "the object is kept: the diversion does not release it"
    );
}

/// The air child answers `isEnableCancelAir` (0x481da4) alone: false for its first 23 steps. The
/// states `attackSensor` refuses (0x429b44, 0x429c68: the squat, rolling, the dive, the grab and
/// wall catches) pick nothing up.
#[cfg_attr(test, test)]
fn the_air_spin_and_the_other_refused_states_pick_nothing_up() {
    let course = stage();
    let mut p = Player::default();
    p.step(&press(), &course);
    p.carry.seed = Some(seed_beside(&p, &course));
    p.mode = Move::Throw;
    p.throwing.in_air = true;
    p.winding = false;
    assert!(!p.spin_cap_enable_cancel_carry());
    for mode in [
        Move::SquatBrake,
        Move::Crouch,
        Move::Roll,
        Move::Dive,
        Move::Ledge,
    ] {
        p.mode = mode;
        p.carry_sensor_phase(&hold(), &course);
        assert!(!p.is_carry(), "{mode:?} refuses the pickup");
    }
    // The ground spin whose state is dead (the byte at +0x10) answers false.
    p.mode = Move::Throw;
    p.throwing.in_air = false;
    assert!(p.spin_cap_enable_cancel_carry());
    p.throwing.child_ended = true;
    assert!(!p.spin_cap_enable_cancel_carry());
}
