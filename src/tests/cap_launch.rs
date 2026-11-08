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
    a_burn_jump_throw_hands_negated_gravity_while_the_collider_touches_the_floor();
    the_throw_frame_hands_the_up_axis_of_the_child_start_throw_picked();
    a_throw_in_the_fall_onto_a_floor_beyond_the_run_angle_starts_the_air_child();
    the_run_throw_asks_the_same_choice_at_the_leaving_edge();
    a_ground_spin_that_leaves_the_floor_keeps_the_ground_child();
    a_fall_landing_within_the_run_angle_carries_its_velocity_onto_the_slope();
    the_wall_jump_throw_records_the_wall_the_collider_holds();
    a_crouch_press_does_not_cancel_a_ground_throw_under_an_open_ceiling();
    a_ground_spin_under_a_low_ceiling_squats_ahead_of_a_jump();
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
    p.launch_cap_across(Throw::Forward, false, Vec3::X, false);
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

/// The bits of `-al::getGravity` (0x4829d4): (-0, 1, -0).
fn negated_gravity_bits() -> [u32; 3] {
    [(-0.0f32).to_bits(), 1.0f32.to_bits(), (-0.0f32).to_bits()]
}

/// A throw pressed at the takeoff of a burn jump (`exeDamageFire` -> `tryActionCapSpinAttackImpl`
/// -> the SpinCap nerve): `appear` asks `rs::isOnGround`, which a velocity leaving the floor
/// fails, so the air child hands `-gravity` although the collider still touches the floor
/// (`hop_contact`, which a burn jump rising off the floor sets).
#[cfg_attr(test, test)]
fn a_burn_jump_throw_hands_negated_gravity_while_the_collider_touches_the_floor() {
    let mut rising = Player {
        mode: Move::BurnJump,
        grounded: false,
        hop_contact: true,
        v: Vec3::new(0., 20., 0.),
        pre_throw: 1,
        mesh_ground: Some(Vec3::Y),
        ..default()
    };
    assert!(rising.spin_cap_in_air());
    rising.fire_throws(&Orders::default());
    let up = rising.launch.as_ref().expect("the cap leaves").up;
    assert_eq!(up.to_array().map(f32::to_bits), negated_gravity_bits());

    // The same burn jump over a slope hands -gravity too, not the collided normal.
    let mut over_slope = on_slope();
    over_slope.mode = Move::BurnJump;
    over_slope.grounded = false;
    over_slope.hop_contact = true;
    over_slope.v = Vec3::new(0., 20., 0.);
    over_slope.pre_throw = 1;
    over_slope.fire_throws(&Orders::default());
    let up = over_slope.launch.as_ref().expect("the cap leaves").up;
    assert_eq!(up.to_array().map(f32::to_bits), negated_gravity_bits());

    // A burn jump on the floor (not leaving it) asks the ground child's rule.
    let mut landed = on_slope();
    landed.mode = Move::BurnJump;
    landed.pre_throw = 1;
    assert!(!landed.spin_cap_in_air());
    landed.fire_throws(&Orders::default());
    assert_eq!(
        landed.launch.as_ref().expect("the cap leaves").up,
        slope_normal()
    );
}

/// The choice through `start_throw` with the states the port produces: the throw frame's up axis
/// follows the child `appear` picked when the throw began (`throwing.in_air`).
#[cfg_attr(test, test)]
fn the_throw_frame_hands_the_up_axis_of_the_child_start_throw_picked() {
    let up_at_throw_frame = |mut p: Player| {
        p.start_throw(Throw::Forward);
        let in_air = p.throwing.in_air;
        p.launch_cap(Throw::Forward, false);
        (in_air, p.launch.as_ref().unwrap().up)
    };
    // Rising off a slope the collider still touches: moving away from the floor, the air child.
    let mut rising = on_slope();
    rising.grounded = false;
    rising.hop_contact = true;
    rising.v = slope_normal() * 6.;
    let (in_air, up) = up_at_throw_frame(rising);
    assert!(in_air);
    assert_eq!(up.to_array().map(f32::to_bits), negated_gravity_bits());
    // Standing on it, or running along it: the ground child, the collided normal.
    let (in_air, up) = up_at_throw_frame(on_slope());
    assert!(!in_air);
    assert_eq!(up, slope_normal());
    let mut along = on_slope();
    along.v = Vec3::new(0., 0.6, 0.8) * 8.;
    let (in_air, up) = up_at_throw_frame(along);
    assert!(!in_air);
    assert_eq!(up, slope_normal());
    // Touching the floor while the velocity is leaving it, though the actor is grounded.
    let mut leaving = on_slope();
    leaving.v = slope_normal() * 6.;
    let (in_air, up) = up_at_throw_frame(leaving);
    assert!(in_air);
    assert_eq!(up.to_array().map(f32::to_bits), negated_gravity_bits());
}

/// `exeFall` sets action trigger 2 when its throw succeeds on ground that is not within the run
/// angle (`rs::isLandGroundRunAngle` false): `appear` then starts the air child although
/// `rs::isOnGround` holds. On a floor within the angle the fall's throw starts the ground child.
#[cfg_attr(test, test)]
fn a_throw_in_the_fall_onto_a_floor_beyond_the_run_angle_starts_the_air_child() {
    let steep = Vec3::new(75f32.to_radians().sin(), 75f32.to_radians().cos(), 0.);
    let mut on_steep = Player {
        mode: Move::Fall,
        grounded: true,
        mesh_ground: Some(steep),
        ..default()
    };
    assert!(on_steep.is_on_ground_by(on_steep.v));
    assert!(!on_steep.is_land_ground_run_angle());
    assert!(on_steep.spin_cap_in_air());
    on_steep.start_throw(Throw::Forward);
    assert!(on_steep.throwing.in_air);
    // The same floor from another state: no trigger 2, the ground child.
    let mut standing = Player {
        mode: Move::Idle,
        grounded: true,
        mesh_ground: Some(steep),
        ..default()
    };
    assert!(!standing.spin_cap_in_air());
    // A fall onto a floor within the angle starts the ground child.
    let mut on_slope_fall = on_slope();
    on_slope_fall.mode = Move::Fall;
    assert!(on_slope_fall.is_land_ground_run_angle());
    assert!(!on_slope_fall.spin_cap_in_air());
    standing.start_throw(Throw::Forward);
    assert!(!standing.throwing.in_air);
}

/// The run throw launches the cap without a state of its own and asks the same choice: a run
/// whose collider touches the floor while the velocity leaves it (the leaving edge) gets the air
/// child's `-gravity`; a run along the floor gets the collided normal.
#[cfg_attr(test, test)]
fn the_run_throw_asks_the_same_choice_at_the_leaving_edge() {
    let mut along = on_slope();
    along.mode = Move::Run;
    along.v = Vec3::new(0., 0.6, 0.8) * 10.;
    along.launch_cap(Throw::Forward, false);
    assert_eq!(along.launch.as_ref().unwrap().up, slope_normal());

    let mut leaving = on_slope();
    leaving.mode = Move::Run;
    leaving.v = Vec3::new(0., 0.6, 0.8) * 10. + slope_normal() * 4.;
    assert!(leaving.is_collided_ground() && leaving.spin_cap_in_air());
    leaving.launch_cap(Throw::Forward, false);
    let up = leaving.launch.as_ref().unwrap().up;
    assert_eq!(up.to_array().map(f32::to_bits), negated_gravity_bits());
}

/// A narrow floor from `edge_z` to the far side: the actor runs toward -z and leaves it there.
fn floor_ending_at(edge_z: f32) -> Course {
    Course {
        solids: vec![Solid {
            center: Vec3::new(0., -0.5, edge_z + 100.),
            half: Vec3::new(100., 0.5, 100.),
            slope: 0.,
            moving: false,
            floor_code: "Ground",
        }],
        ..default()
    }
}

/// A standing actor that still carries a run's momentum presses the throw: a ground spin that
/// brakes over its first steps. Returns the first step the actor was off the floor, the step the
/// cap left on, and the player after 30 steps.
fn spin_throw_over(course: &Course) -> (Option<u32>, Option<u32>, Player) {
    let mut p = Player::default();
    p.pos.z = 40.;
    p.v = Vec3::new(0., 0., -9.);
    p.step(
        &Orders {
            throw_pressed: true,
            throw_held: true,
            ..default()
        },
        course,
    );
    let (mut launched, mut left_floor) = (None, None);
    for _ in 0..30 {
        let step = p.frame;
        p.step(&Orders::default(), course);
        if left_floor.is_none() && !p.grounded {
            left_floor = Some(step);
        }
        if launched.is_none() && p.launch.is_some() {
            launched = Some(step);
        }
    }
    (left_floor, launched, p)
}

/// `PlayerStateSpinCap::appear` (0x481958) starts one child and nothing but `exeSpinAir`'s own end
/// (the fall nerve after step 22, 0x482a38) changes it: the only `setNerve` calls of the class are
/// 0x481ad4, 0x481b04 (`appear`) and that one, and the nerve objects are referenced from no
/// function outside `PlayerStateSpinCap`. A ground spin whose actor coasts off an edge before the
/// throw frame stays `exeSpinGround`: it throws on the ground frame (12, `getThrowFrameGround`),
/// not on the air frame (8), and the actor keeps the ground control's pull.
#[cfg_attr(test, test)]
fn a_ground_spin_that_leaves_the_floor_keeps_the_ground_child() {
    // On a floor wide enough to stay on, the throw frame is the ground one.
    let (left_floor, launched, p) = spin_throw_over(&floor_ending_at(-100.));
    assert_eq!(
        (left_floor, launched),
        (None, Some(t::SPIN_CAP_THROW_FRAME))
    );
    assert!(!p.throwing.in_air);

    // The same spin with the floor ending where the actor is at the spin's first step.
    let mut probe = Player::default();
    probe.pos.z = 40.;
    probe.v = Vec3::new(0., 0., -9.);
    let wide = floor_ending_at(-100.);
    probe.step(
        &Orders {
            throw_pressed: true,
            throw_held: true,
            ..default()
        },
        &wide,
    );
    while probe.frame < 1 {
        probe.step(&Orders::default(), &wide);
    }
    let (left_floor, launched, p) = spin_throw_over(&floor_ending_at(probe.pos.z));
    let left = left_floor.expect("the spin coasts off the floor");
    assert!(left < t::SPIN_CAP_THROW_FRAME_AIR, "left at step {left}");
    assert!(!p.throwing.in_air);
    assert_eq!(
        launched,
        Some(t::SPIN_CAP_THROW_FRAME),
        "the ground child throws on its own frame whatever the actor stands on"
    );
}

/// `exeFall` sets trigger 1 for a throw that lands within the run angle (0x425280); `appear` then
/// carries the fall's velocity onto the collided ground before the ground child's brake:
/// `alongVectorNormalH(velocity, velocity, -gravity, collidedGroundNormal)` (0x481a1c..0x481a70),
/// which drops the vertical part and turns the horizontal part into the ground's plane,
/// keeping its length.
#[cfg_attr(test, test)]
fn a_fall_landing_within_the_run_angle_carries_its_velocity_onto_the_slope() {
    let angle = 30f32.to_radians();
    // A ground rising toward +z, its normal tilted toward -z.
    let normal = Vec3::new(0., angle.cos(), -angle.sin());
    let uphill = Vec3::new(0., angle.sin(), angle.cos());
    let landing = |mode| Player {
        mode,
        grounded: true,
        mesh_ground: Some(normal),
        // 10 along the ground's horizontal heading, and a fall into it.
        v: Vec3::new(0., -20., 10.),
        facing: Vec3::Z,
        ..default()
    };
    let mut p = landing(Move::Fall);
    assert!(p.is_land_ground_run_angle());
    p.start_throw(Throw::Forward);
    assert!(!p.throwing.in_air);
    // The carried velocity lies in the plane of the ground and keeps the horizontal length 10;
    // the floor pull of the ground control sits on top of it along -normal.
    let carried = p.v + normal * GRAVITY_MOVE;
    assert!(carried.dot(normal).abs() < 1e-4, "{carried}");
    assert!((carried - uphill * 10.).length() < 1e-4, "{carried}");
    // The brake of the first step then keeps 0.95 of that length, not 0.95 of the horizontal
    // part laid onto the slope (10 cos 30).
    p.spin_cap_inertia();
    let braked = (p.v + normal * GRAVITY_MOVE).length();
    assert!(
        (braked - 10. * t::SPIN_BRAKE_RATE).abs() < 1e-4,
        "{braked} from {}",
        p.v
    );

    // Only the fall's trigger 1 carries: the same ground and velocity from another state
    // keeps the raw velocity with the pull as the port's ground states hold it.
    let mut standing = landing(Move::Idle);
    standing.start_throw(Throw::Forward);
    assert_eq!(standing.v, Vec3::new(0., -GRAVITY_MOVE, 10.));

    // Level ground: the vertical part of the fall is dropped and the pull stands in its place.
    let mut level = Player {
        mode: Move::Fall,
        grounded: true,
        v: Vec3::new(3., -20., 4.),
        ..default()
    };
    assert!(level.is_land_ground_run_angle());
    level.start_throw(Throw::Forward);
    assert_eq!(level.v, Vec3::new(3., -GRAVITY_MOVE, 4.));
}

/// `recordWallJump` (0x492d60) stores `rs::getCollidedWallNormal` (0x565fd4), the face normal of
/// the wall the collider holds at the press, not the normal the slide last followed.
#[cfg_attr(test, test)]
fn the_wall_jump_throw_records_the_wall_the_collider_holds() {
    let followed = Vec3::Z;
    let held = Vec3::new(0.6, 0., 0.8);
    let press = |contact: Option<Vec3>| {
        let mut p = Player {
            mode: Move::WallJump,
            pos: Vec3::new(1., 2., 3.),
            ..default()
        };
        p.wall_air.slide.normal_start = followed;
        p.touched.wall_contact = contact.map(|normal| WallContact {
            normal,
            position: Vec3::ZERO,
            at_face: true,
        });
        p.start_throw(Throw::Forward);
        p.wall_history
            .jump
            .expect("the press records the wall jump")
    };
    let (position, normal) = press(Some(held));
    assert_eq!(normal, held);
    assert_eq!(position, Vec3::new(1., 2., 3.) / UNIT + Vec3::NEG_Y * 20.);
    // No contact: the slide's normal stands in.
    assert_eq!(press(None).1, followed);
}

/// A wide floor (top at y = 0) with a slab over it whose underside is `height` metres up.
/// `PlayerCeilingCheck::isEnableStandUp` holds for a ceiling 1.6 m up or higher. The collider
/// pushes a standing body sideways out from under a lower slab, so a test that wants the ceiling
/// check to refuse while the actor still stands there brings the slab in only after the throw.
fn floor_under_ceiling_at(height: f32) -> Course {
    let slab = |center_y: f32, floor_code| Solid {
        center: Vec3::new(0., center_y, 0.),
        half: Vec3::new(100., 0.5, 100.),
        slope: 0.,
        moving: false,
        floor_code,
    };
    Course {
        solids: vec![slab(-0.5, "Ground"), slab(height + 0.5, "Ground")],
        ..default()
    }
}

/// A standing actor throws on `before`; from the step after the cap has left it steps on
/// `after`, with `held` as the order. Returns the step the cap left on, the step the actor
/// stopped being in the throw (`None` when it did not within 80 steps) and the mode it was in
/// then.
fn ground_throw_then(held: &Orders, before: &Course, after: &Course) -> (u32, Option<u32>, Move) {
    let mut p = Player::default();
    p.step(
        &Orders {
            throw_pressed: true,
            throw_held: true,
            ..default()
        },
        before,
    );
    let (mut cap_left, mut ended) = (None, None);
    for step in 1..80 {
        let (order, course) = if cap_left.is_some() {
            (held.clone(), after)
        } else {
            (Orders::default(), before)
        };
        p.step(&order, course);
        if cap_left.is_none() && p.launch.is_some() {
            cap_left = Some(step);
        }
        if cap_left.is_some() && p.mode != Move::Throw {
            ended = Some(step);
            break;
        }
    }
    (cap_left.expect("the cap leaves"), ended, p.mode)
}

/// The ground chain of `exeSpinCap` (0x423aa4..0x423d80) reads no crouch button: its squat is
/// judge +0x3e8 (`PlayerJudgeEnableStandUp`, 0x459514), taken only when a ceiling forbids
/// standing on a collided floor. The StartSquat judge (+0x480) is asked by `setNerveOnGround`
/// (0x421a44) alone, which the child reaches at its end. So a crouch held after a throw under an
/// open ceiling does not squat before the clip is over, and then it squats.
#[cfg_attr(test, test)]
fn a_crouch_press_does_not_cancel_a_ground_throw_under_an_open_ceiling() {
    let open = floor_under_ceiling_at(3.);
    let crouch = Orders {
        crouch: true,
        crouch_pressed: true,
        ..default()
    };
    let (_, plain_end, _) = ground_throw_then(&Orders::default(), &open, &open);
    let plain_end = plain_end.expect("an untouched spin ends");
    let (cap_left, held_end, mode) = ground_throw_then(&crouch, &open, &open);
    assert!(
        cap_left + 3 < plain_end,
        "the clip outlasts the cap's leaving"
    );
    assert_eq!(
        held_end,
        Some(plain_end),
        "the spin runs to the same end with the crouch held"
    );
    assert_eq!(mode, Move::SquatBrake, "setNerveOnGround then squats");
}

/// With a ceiling that forbids standing the chain squats at once after the cap has left, with no
/// button read, and ahead of the jump judge (0x423cb0 before 0x423d6c): a jump held from the
/// cap's leaving is not taken, while under an open ceiling the same press jumps.
#[cfg_attr(test, test)]
fn a_ground_spin_under_a_low_ceiling_squats_ahead_of_a_jump() {
    let open = floor_under_ceiling_at(3.);
    let low = floor_under_ceiling_at(1.4);
    let jump = Orders {
        jump: true,
        jump_held: true,
        ..default()
    };
    let (cap_left, ended, mode) = ground_throw_then(&Orders::default(), &open, &low);
    assert_eq!(mode, Move::SquatBrake);
    assert_eq!(
        ended,
        Some(cap_left + 1),
        "the squat is the step after the cap left"
    );
    let (_, _, jumped) = ground_throw_then(&jump, &open, &low);
    assert_eq!(
        jumped,
        Move::SquatBrake,
        "the low ceiling takes the squat, not the jump"
    );
    let (_, _, free) = ground_throw_then(&jump, &open, &open);
    assert_ne!(free, Move::SquatBrake);
    assert_ne!(free, Move::Throw);
}
