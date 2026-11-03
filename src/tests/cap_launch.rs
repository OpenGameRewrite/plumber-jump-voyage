//! The up axis the throw hands the cap (`PlayerSpinCapAttack::startCapThrow`, called from
//! `exeSpinGround` 0x482174, `exeSpinAir` 0x4829d4, `exeSwimSpinCap` 0x489ed4 and
//! `exeSwimSpinCapSurface` 0x48a424) and the direction it flies along on a slope.
use super::*;

pub(super) fn run() {
    an_air_throw_hands_the_cap_negated_gravity();
    a_ground_throw_hands_the_cap_the_collided_ground_normal();
    a_throw_from_level_ground_keeps_the_horizontal_front();
    a_swim_throw_keeps_the_up_axis_it_stored();
    the_slope_throw_direction_lies_across_the_ground_normal();
    the_first_step_takes_the_direction_across_the_ground_the_cap_is_handed();
    the_collider_alone_decides_the_ground_a_ground_spin_hands_over();
}

/// The normal of a ground that rises towards +z (a 36.87 degree slope).
fn slope_normal() -> Vec3 {
    Vec3::new(0., 0.8, -0.6)
}
fn on_slope() -> Player {
    Player {
        grounded: true,
        mesh_ground: Some(slope_normal()),
        ..default()
    }
}

/// `exeSpinAir` hands `-al::getGravity` (0x4829d4: three negations of gravity (0, -1, 0)), so
/// the up axis carries the signed zeros.
#[cfg_attr(test, test)]
fn an_air_throw_hands_the_cap_negated_gravity() {
    // Even over a slope the collider still holds, the air spin takes the gravity.
    let mut p = on_slope();
    p.grounded = false;
    p.hop_contact = true;
    p.mode = Move::Throw;
    p.throwing.in_air = true;
    p.launch_cap(Throw::Forward, false);
    let up = p.launch.as_ref().unwrap().up;
    assert_eq!(
        up.to_array().map(f32::to_bits),
        [(-0.0f32).to_bits(), 1.0f32.to_bits(), (-0.0f32).to_bits()]
    );
}

/// `exeSpinGround` hands `rs::calcGroundNormalOrGravityDir` (0x482174 -> 0x55baac): the collided
/// ground's normal.
#[cfg_attr(test, test)]
fn a_ground_throw_hands_the_cap_the_collided_ground_normal() {
    let mut p = on_slope();
    p.throwing.dir = Some(Vec3::new(0., 0.6, 0.8));
    p.launch_cap(Throw::Forward, false);
    let launch = p.launch.as_ref().unwrap();
    assert_eq!(launch.up, slope_normal());
    // The direction lies across that axis and is given to the cap as it is.
    assert_eq!(launch.front, Vec3::new(0., 0.6, 0.8));
}

/// On level ground the normal is the world's up and the cap flies along the horizontal.
#[cfg_attr(test, test)]
fn a_throw_from_level_ground_keeps_the_horizontal_front() {
    let mut p = Player {
        grounded: true,
        mesh_ground: Some(Vec3::Y),
        ..default()
    };
    p.throwing.dir = Some(Vec3::new(1., 0.25, 0.));
    p.launch_cap(Throw::Forward, false);
    let launch = p.launch.as_ref().unwrap();
    assert_eq!(launch.up, Vec3::Y);
    assert_eq!(launch.front, Vec3::X);
}

/// The swim spin hands the vector it stored (0x489ed4, 0x48a424), whatever the ground is.
#[cfg_attr(test, test)]
fn a_swim_throw_keeps_the_up_axis_it_stored() {
    let mut p = on_slope();
    p.throwing.dir = Some(Vec3::Z);
    p.launch_cap_across(Throw::Forward, false, Vec3::X);
    let launch = p.launch.as_ref().unwrap();
    assert_eq!((launch.up, launch.front), (Vec3::X, Vec3::Z));
}

/// The first step's helper (0x4831b8) takes the move input in the plane of the ground normal.
#[cfg_attr(test, test)]
fn the_slope_throw_direction_lies_across_the_ground_normal() {
    let mut p = on_slope();
    p.stick = Vec3::Z;
    p.tilt = 1.;
    let dir = p.throw_direction_across(slope_normal());
    assert!((dir - Vec3::new(0., 0.6, 0.8)).length() < 1e-6, "{dir:?}");
    assert!(dir.dot(slope_normal()).abs() < 1e-6);
    // Without a direction the actor's front is carried onto the plane (0x4832c8..0x483300).
    p.tilt = 0.;
    p.stick = Vec3::ZERO;
    let dir = p.throw_direction_across(slope_normal());
    assert!((dir.length() - 1.).abs() < 1e-6);
    assert!(dir.dot(slope_normal()).abs() < 1e-6, "{dir:?}");
}

/// The first step fixes the direction across the same ground normal the throw frame hands the
/// cap: here the toy course's ramp, which rises along +x (no collision mesh).
#[cfg_attr(test, test)]
fn the_first_step_takes_the_direction_across_the_ground_the_cap_is_handed() {
    let mut p = Player {
        grounded: true,
        slope: 0.5,
        stick: Vec3::X,
        tilt: 1.,
        ..default()
    };
    p.face_throw_direction();
    let dir = p.throwing.dir.unwrap();
    let normal = p.collided_ground_normal();
    assert!((dir.y - 0.447_213_6).abs() < 1e-5, "{dir:?}");
    assert!(dir.dot(normal).abs() < 1e-6);
    p.launch_cap(Throw::Forward, false);
    let launch = p.launch.as_ref().unwrap();
    assert_eq!((launch.up, launch.front), (normal, dir));
    // Level ground keeps the stick as it is.
    let mut level = Player {
        grounded: true,
        stick: Vec3::X,
        tilt: 1.,
        ..default()
    };
    level.face_throw_direction();
    assert_eq!(level.throwing.dir, Some(Vec3::X));
}

/// `exeSpinGround` asks `rs::isCollidedGround` alone (0x482174 -> 0x55baac, and the first step's
/// helper through 0x55ba38): a collider that touches the floor while the actor is not grounded
/// (`hop_contact`: it moves up off it) still hands the collided normal and the plane direction.
#[cfg_attr(test, test)]
fn the_collider_alone_decides_the_ground_a_ground_spin_hands_over() {
    let mut p = Player {
        grounded: false,
        hop_contact: true,
        mesh_ground: Some(slope_normal()),
        mode: Move::Throw,
        stick: Vec3::Z,
        tilt: 1.,
        ..default()
    };
    p.face_throw_direction();
    let dir = p.throwing.dir.unwrap();
    assert!((dir - Vec3::new(0., 0.6, 0.8)).length() < 1e-6, "{dir:?}");
    p.launch_cap(Throw::Forward, false);
    let launch = p.launch.as_ref().unwrap();
    assert_eq!((launch.up, launch.front), (slope_normal(), dir));
    // The collider holding nothing hands -gravity, grounded or not.
    let mut released = Player {
        grounded: false,
        hop_contact: false,
        mesh_ground: Some(slope_normal()),
        mode: Move::Throw,
        ..default()
    };
    released.launch_cap(Throw::Forward, false);
    assert_eq!(released.launch.as_ref().unwrap().up, Vec3::Y);
}
