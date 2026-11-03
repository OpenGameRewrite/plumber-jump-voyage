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
    after_a_wall_jump_the_input_pointing_back_is_clamped_sideways();
    a_throw_on_the_wall_jumps_first_step_sets_the_trigger_and_turns_the_actor_back();
    the_wall_jump_throw_resets_the_wall_along_memory_before_the_helper_reads_the_stick();
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
    let dir = p.throw_direction_across(slope_normal(), false);
    assert!((dir - Vec3::new(0., 0.6, 0.8)).length() < 1e-6, "{dir:?}");
    assert!(dir.dot(slope_normal()).abs() < 1e-6);
    // Without a direction the actor's front is carried onto the plane (0x4832c8..0x483300).
    p.tilt = 0.;
    p.stick = Vec3::ZERO;
    let dir = p.throw_direction_across(slope_normal(), false);
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

/// A player facing +z on level ground whose stick is pushed along `direction` by `tilt`.
fn facing_z(direction: Vec3, tilt: f32) -> Player {
    let mut p = Player {
        grounded: true,
        stick: direction,
        tilt,
        ..default()
    };
    p.set_pose(pose_math::quat_front_up(Vec3::Z, Vec3::Y));
    p
}

/// The helper's `PlayerTrigger` 0 branch (0x4831fc..0x4832b8): the move input that points back
/// (along the negated front) between 90 and 15 degrees of it loses its backward part; the
/// comparison takes the move input as it is, so a smaller tilt reaches the band at a smaller angle.
#[cfg_attr(test, test)]
fn after_a_wall_jump_the_input_pointing_back_is_clamped_sideways() {
    let sixty_off_back = Vec3::new(60f32.to_radians().sin(), 0., -60f32.to_radians().cos());
    let p = facing_z(sixty_off_back, 1.);
    let dir = p.throw_direction_across(Vec3::Y, true);
    assert!((dir - Vec3::X).length() < 1e-6, "{dir:?}");
    // Without the trigger the input is the direction as it is.
    let plain = p.throw_direction_across(Vec3::Y, false);
    assert!((plain - sixty_off_back).length() < 1e-6, "{plain:?}");
    // Within 15 degrees of straight back, and ahead, the input stays.
    let ten_off_back = Vec3::new(10f32.to_radians().sin(), 0., -10f32.to_radians().cos());
    let back = facing_z(ten_off_back, 1.).throw_direction_across(Vec3::Y, true);
    assert!((back - ten_off_back).length() < 1e-6, "{back:?}");
    let ahead = Vec3::new(0.5, 0., 0.5_f32.sqrt() * 1.2).normalize();
    let forward = facing_z(ahead, 1.).throw_direction_across(Vec3::Y, true);
    assert!((forward - ahead).length() < 1e-6, "{forward:?}");
    // The same ten degrees at half tilt: the dot product (0.49) falls inside the band.
    let half = facing_z(ten_off_back, 0.5).throw_direction_across(Vec3::Y, true);
    assert!((half - Vec3::X).length() < 1e-6, "{half:?}");
}

/// Without a move input the direction is the actor's front, which the trigger's throw has
/// just turned back (`startSlideSpinAttack`).
#[cfg_attr(test, test)]
fn a_throw_on_the_wall_jumps_first_step_sets_the_trigger_and_turns_the_actor_back() {
    let mut p = facing_z(Vec3::ZERO, 0.);
    p.mode = Move::WallJump;
    p.frame = 0;
    p.start_throw(Throw::Forward);
    assert!(p.throwing.after_wall_air);
    let front = p.actor_pose().front();
    assert!((front - Vec3::NEG_Z).length() < 1e-6, "{front:?}");
    p.face_throw_direction();
    let dir = p.throwing.dir.unwrap();
    assert!((dir - Vec3::NEG_Z).length() < 1e-6, "{dir:?}");
    // The jump's later steps and the other states leave the trigger off and the pose alone.
    for (mode, frame) in [(Move::WallJump, 1), (Move::Idle, 0)] {
        let mut other = facing_z(Vec3::ZERO, 0.);
        other.mode = mode;
        other.frame = frame;
        other.start_throw(Throw::Forward);
        assert!(!other.throwing.after_wall_air);
        let front = other.actor_pose().front();
        assert!((front - Vec3::Z).length() < 1e-6, "{front:?}");
    }
}

/// `exeWallAir`'s throw press calls `PlayerInput::resetAlongWall` (0x4245ec) before the spin's
/// helper runs, so a stick held along the wall the jump left is not bent along it: the helper's
/// direction is the stick as it is. The same press records the wall jump again from 20 below the
/// body (`startSlideSpinAttack`, 0x48f6c8).
#[cfg_attr(test, test)]
fn the_wall_jump_throw_resets_the_wall_along_memory_before_the_helper_reads_the_stick() {
    // A wall facing +z remembered at full weight; the stick is 20 degrees off the wall's
    // direction (x) into the wall, within the 50 degrees the bend runs along it. The press turns
    // the actor back to -z, so the stick is ahead of it and the helper's clamp leaves it alone.
    let stick = Vec3::new(20f32.to_radians().cos(), 0., -20f32.to_radians().sin());
    let mut p = facing_z(stick, 1.);
    for _ in 0..40 {
        p.wall_along.update(Some(Vec3::Z), Vec2::Y);
    }
    p.pos = Vec3::new(1., 2., 3.);
    // The memory alone would bend the input onto the wall's direction.
    let bent = p.plane_stick(Vec3::Y);
    assert!((bent - Vec3::X).length() < 0.03, "{bent:?}");

    p.mode = Move::WallJump;
    p.frame = 0;
    p.start_throw(Throw::Forward);
    assert!(p.throwing.after_wall_air);
    p.face_throw_direction();
    let dir = p.throwing.dir.unwrap();
    assert!((dir - stick).length() < 1e-6, "{dir:?}");
    let (from, _) = p
        .wall_history
        .jump
        .expect("the wall jump is recorded again");
    assert!(
        (from - Vec3::new(100., 200. - 20., 300.)).length() < 1e-3,
        "{from:?}"
    );

    // Another state's throw leaves the memory and the history alone.
    let mut other = facing_z(stick, 1.);
    for _ in 0..40 {
        other.wall_along.update(Some(Vec3::Z), Vec2::Y);
    }
    other.mode = Move::Idle;
    other.start_throw(Throw::Forward);
    let kept = other.plane_stick(Vec3::Y);
    assert!((kept - Vec3::X).length() < 0.03, "{kept:?}");
    assert!(other.wall_history.jump.is_none());
}
