//! The ground motion of the spin-cap state while it brakes (`PlayerStateSpinCap::updateSpinGroundNerve`
//! on its first `SpinBrakeFrame` steps): `rs::moveInertiaTurn`.
//!
//! The velocity is split along the actor's front, its side and the floor's normal
//! (`PlayerActionVelocityControl`) and only the front part is braked, so a velocity that does not
//! point along the front keeps its sideways part: a throw begun on the run, with the facing turned
//! to the stick, carries on across its new front while the part against it dies away. A velocity
//! along the front (a throw from standing, or along the run) loses `SpinBrakeRate` of itself whole.
//! The stick then turns the direction of that velocity by at most `SpinRoundLimitDegree` a step
//! unless it points against it (`PlayerActionFunction::isOppositeDir`, a hundred degrees or more
//! away); the actor's front turns by the same rotation.
//!
//! The inertia belongs to the braking steps only while the force-run counter is down and the
//! player is not running on the water's surface (`PlayerCounterForceRun`,
//! `PlayerJudgeWaterSurfaceRun`; 0x48242c..0x482474); otherwise the state's ground move control
//! steers from the first step (`spin_ground`). An ice floor (`isOnGroundSkateCode`) takes the
//! control's branch at once, and its first step lays the velocity on the throw direction
//! (`spin_on_ice_first_step`, 0x482278..0x4823f8); on the water's surface the nerve's
//! `al::approachWaterSurfaceRate` follows every step (`approach_water_surface`, 0x4827a8..0x482804).
//! Not played: the ice's hit reaction effect.
use super::pose_math::{
    DEGREES_TO_RADIANS, along_vector_normal_h, cross, dot, make_quat_rotation_limit, normalize,
    rotate_by_quat, try_normalize_or_zero,
};
use super::vector_lib::{parallelize_vec, separate_parallel_vertical, verticalize_vec};
use super::*;

/// `PlayerActionFunction::isOppositeDir`: the two unit directions are further apart than the
/// sine of five degrees short of a right angle.
fn is_opposite_dir(a: Vec3, b: Vec3) -> bool {
    dot(b, a) <= -0.087_156
}

impl Player {
    /// One step of `rs::moveInertiaTurn` on the floor: `GRAVITY_MOVE` snaps the velocity to it.
    pub(super) fn spin_cap_inertia(&mut self) {
        let normal = self
            .mesh_ground
            .filter(|_| self.grounded)
            .unwrap_or(Vec3::Y);
        let (actor_up, actor_front) = self.pose_axes();
        // `PlayerActionVelocityControl`: the front is the actor's, carried along the floor.
        let front = normalize(along_vector_normal_h(actor_front, actor_up, normal));
        let side = normalize(cross(normal, front));
        let mut velocity_front = parallelize_vec(front, self.v);
        let mut velocity_side = parallelize_vec(side, self.v);
        let mut velocity_up = self.v - velocity_front - velocity_side;
        velocity_front = verticalize_vec(normal, velocity_front) * t::SPIN_BRAKE_RATE;
        velocity_side = verticalize_vec(normal, velocity_side);
        velocity_up = verticalize_vec(normal, velocity_up);
        // `calcSnap(-normal, GravityMove)`, component by component as the original adds it.
        let snap = -normal;
        velocity_up = Vec3::new(
            snap.x * GRAVITY_MOVE + velocity_up.x,
            snap.y * GRAVITY_MOVE + velocity_up.y,
            snap.z * GRAVITY_MOVE + velocity_up.z,
        );
        let velocity = velocity_front + velocity_side + velocity_up;
        // The velocity's direction along the floor turns toward the stick.
        let (across, along) = {
            let (parallel, vertical) = separate_parallel_vertical(normal, velocity);
            (vertical, parallel)
        };
        let speed = super::vector_lib::length(across);
        let (direction, moving) = try_normalize_or_zero(across);
        let mut turned = direction;
        let mut turn = Quat::IDENTITY;
        let (stick, pushed) = try_normalize_or_zero(verticalize_vec(normal, self.ground_stick()));
        if moving && pushed && !is_opposite_dir(stick, direction) {
            turn = make_quat_rotation_limit(
                direction,
                stick,
                t::SPIN_ROUND_LIMIT_DEGREE * DEGREES_TO_RADIANS,
            );
            turned = rotate_by_quat(direction, turn);
        }
        let result = if moving {
            Vec3::new(
                turned.x * speed + along.x,
                turned.y * speed + along.y,
                turned.z * speed + along.z,
            )
        } else {
            velocity
        };
        self.v = result;
        if turn != Quat::IDENTITY {
            // The state turns the actor's front by the same rotation (`rs::slerpUpFront`).
            let front = rotate_by_quat(actor_front, turn);
            let stored = self.actor_pose().slerp_up_front(
                normal,
                front,
                SLERP_QUAT_RATE,
                t::HILL_POSE_DEGREE_MAX,
            );
            self.set_pose(stored);
        }
    }
}

impl Player {
    /// The first step of `updateSpinGroundNerve` on an ice floor (0x482270..0x4823f8,
    /// `rs::isOnGroundSkateCode`): the velocity along the floor is laid on the throw direction.
    /// With `along` its part along the throw direction and `speed` its length, the part that is
    /// not along the direction, `behind = speed - |along|`, joins it: a velocity that points
    /// forward (or sideways) keeps its whole speed; one that points back keeps `along` when
    /// that is the larger part and else turns into `behind`. The hit reaction the executable
    /// starts there (the ice spin reaction, the weak one under a quarter of `NormalMaxSpeed` or
    /// within fifteen degrees of the direction) is an effect and is not played.
    pub(super) fn spin_on_ice_first_step(&mut self) {
        if !self.on_ground_skate() {
            return;
        }
        let Some(front) = self.throwing.dir else {
            return;
        };
        let up = if self.is_collided_ground() {
            self.collided_ground_normal()
        } else {
            self.actor_pose().up()
        };
        let (_, horizontal) = separate_parallel_vertical(up, self.v);
        let speed = super::vector_lib::length(horizontal);
        let along = dot(front, horizontal);
        let behind = speed - along.abs();
        let kept = if along < 0. {
            if along.abs() > behind { along } else { behind }
        } else {
            along + behind
        };
        self.v = Vec3::new(kept * front.x, kept * front.y, kept * front.z);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spinner() -> Player {
        Player {
            mode: Move::Throw,
            grounded: true,
            ground_contact: 2,
            v: Vec3::new(10., -0.5, 0.),
            facing: Vec3::X,
            ..default()
        }
    }

    /// The braking steps of the ground spin run `moveInertiaTurn` unless the force-run counter is
    /// up or the player runs on the water's surface (0x48242c..0x482474); then the state's ground
    /// control steers from the first step.
    #[test]
    fn a_forced_run_or_a_surface_run_steers_the_braking_steps_with_the_ground_control() {
        let o = Orders::default();
        let scene = Course::default();
        let mut inertia = spinner();
        inertia.spin_ground(&o, 3, &scene);
        // The control: no stick, the plain ground control brakes by its frames, not by 0.95.
        let mut expected = spinner();
        expected
            .spin_ground_move
            .reset(expected.v, expected.facing, Vec3::Y);
        let (actor_up, actor_front) = expected.pose_axes();
        let steer = expected.spin_ground_move.update(&SteerInput {
            skate: expected.on_ground_skate(),
            up: Vec3::Y,
            wall: None,
            velocity: expected.v,
            actor_front,
            actor_up,
            stick: expected.ground_stick(),
            max_speed: t::NORMAL_MAX,
            accel_frames: t::ACCEL_FRAMES as i32,
        });
        assert_ne!(
            steer.velocity.x, inertia.v.x,
            "the fixture tells the two apart"
        );
        for forced in [true, false] {
            let mut p = spinner();
            if forced {
                p.force_run.setup(5, 38.);
            } else {
                p.surface_run.result = true;
            }
            p.spin_ground(&o, 3, &scene);
            assert_eq!(p.v.x, steer.velocity.x, "forced run {forced}");
        }
    }

    /// [I] Step 0 stands in for the original appear's reset. A previous spin must not leave
    /// brake/turn history in the new controlled spin (appear chain 0x481a78 -> 0x416a7c).
    #[test]
    fn the_first_controlled_spin_step_resets_the_previous_controls_history() {
        for forced in [true, false] {
            let mut fresh = spinner();
            let mut reused = spinner();
            reused.spin_ground_move.brake_counter = 123;
            reused.spin_ground_move.turn.resume(0.8, Vec3::Z);
            for player in [&mut fresh, &mut reused] {
                if forced {
                    player.force_run.setup(5, 38.);
                } else {
                    player.surface_run.result = true;
                }
                player.spin_ground(&Orders::default(), 0, &Course::default());
            }
            assert_eq!(reused.spin_ground_move.brake_counter, 0);
            assert_eq!(reused.v, fresh.v);
            assert_eq!(
                format!("{:?}", reused.spin_ground_move.turn),
                format!("{:?}", fresh.spin_ground_move.turn),
            );
        }
    }
    fn on_ice(v: Vec3) -> Player {
        let mut p = Player {
            v,
            throwing: ThrowControl {
                dir: Some(Vec3::X),
                ..default()
            },
            ..spinner()
        };
        p.touched.floor = Some(crate::geometry::CollisionCodes {
            floor: "Skate".into(),
            ..default()
        });
        p
    }

    /// The queued first step must call the ice helper after resolving the throw direction.
    /// Testing the helper alone would miss a disconnected call in `start_throw`.
    #[test]
    fn the_spin_start_runs_the_ice_velocity_conversion() {
        let mut p = on_ice(Vec3::new(0., -0.5, 6.));
        p.start_throw(Throw::Forward);
        assert_eq!(p.throwing.dir, None, "direction waits for the first step");
        let acting = p.acting_nerve();
        p.begin_step(acting);
        assert_eq!(p.throwing.dir, Some(Vec3::X));
        assert_eq!(p.v, Vec3::new(6., 0., 0.));
    }

    /// 0x482270..0x4823f8: on an ice floor the first step lays the velocity along the floor on the
    /// throw direction. Forward or sideways velocity keeps its whole speed there; a velocity
    /// pointing back keeps `along` when that is the larger part, else the part across it.
    #[test]
    fn the_ice_first_step_lays_the_velocity_on_the_throw_direction() {
        // Along the direction: all of it.
        let mut p = on_ice(Vec3::new(8., -0.5, 0.));
        p.spin_on_ice_first_step();
        assert_eq!(p.v, Vec3::new(8., 0., 0.));
        // Across it: the whole speed turns onto the direction.
        let mut p = on_ice(Vec3::new(0., -0.5, 6.));
        p.spin_on_ice_first_step();
        assert_eq!(p.v, Vec3::new(6., 0., 0.));
        // Mostly backward (|along| > speed - |along|): the backward speed stays.
        let mut p = on_ice(Vec3::new(-8., -0.5, 2.));
        let kept = -8.;
        p.spin_on_ice_first_step();
        assert_eq!(p.v, Vec3::new(kept, 0., 0.));
        // Mostly sideways with a little backward: the speed beyond the backward part goes forward.
        let mut p = on_ice(Vec3::new(-1., -0.5, 6.));
        let speed = (1_f32 * 1. + 6. * 6.).sqrt();
        p.spin_on_ice_first_step();
        assert_eq!(p.v, Vec3::new(speed - 1., 0., 0.));
        // Off the ice nothing happens.
        let mut p = on_ice(Vec3::new(-1., -0.5, 6.));
        p.touched.floor = None;
        p.spin_on_ice_first_step();
        assert_eq!(p.v, Vec3::new(-1., -0.5, 6.));
    }

    /// 0x48240c: the ice takes the control's branch before the inertia is asked, and sets the
    /// control up on every first step; leaving the ice sets it up once more (0x482690).
    #[test]
    fn the_ice_steers_with_the_control_and_the_setup_flag_follows_it() {
        let o = Orders::default();
        let scene = Course::default();
        let mut p = on_ice(Vec3::new(9., -0.5, 0.));
        p.spin_ground_move.brake_counter = 77;
        p.spin_ground(&o, 3, &scene);
        assert!(p.spin_skate_setup, "the setup set the flag");
        assert_eq!(
            p.spin_ground_move.brake_counter, 0,
            "the control was set up"
        );
        // A later step on the ice only updates the control; a clear flag sets it up again.
        assert!(!spin_control_setup(true, true, 4));
        assert!(spin_control_setup(true, false, 4));
        assert!(spin_control_setup(true, true, 0));
        // Off the ice: the flag the ice left, and SpinBrakeFrame, set it up; other steps do not.
        assert!(spin_control_setup(false, true, 5));
        assert!(spin_control_setup(false, false, t::SPIN_BRAKE_FRAMES));
        assert!(!spin_control_setup(false, false, t::SPIN_BRAKE_FRAMES + 1));
        // Off the ice and in the braking frames the inertia returns and the flag waits; the first
        // controlled step clears it.
        p.touched.floor = None;
        p.spin_ground(&o, 5, &scene);
        assert!(p.spin_skate_setup);
        p.spin_ground(&o, t::SPIN_BRAKE_FRAMES, &scene);
        assert!(!p.spin_skate_setup);
    }

    /// 0x4827a8..0x482804: while the surface-run judge holds, every ground step ends with
    /// `al::approachWaterSurfaceRate` at the constant rate: the vertical speed gains the
    /// distance to the surface times the rate, limited to the approach limit.
    #[test]
    fn the_surface_run_approaches_the_surface_after_the_control() {
        let o = Orders::default();
        let scene = Course::default();
        let mut p = spinner();
        p.surface_run.result = true;
        p.water_finder.found = true;
        p.water_finder.distance = 4.;
        p.spin_ground(&o, 3, &scene);
        assert_eq!(p.v.y, (-GRAVITY_MOVE + 4. * 0.5).clamp(-5., 5.));
        p.water_finder.distance = 40.;
        p.spin_ground(&o, 4, &scene);
        assert_eq!(p.v.y, 5., "limited");
        // Without the judge the water does not matter.
        let mut dry = spinner();
        dry.water_finder.found = true;
        dry.water_finder.distance = 4.;
        dry.spin_ground(&o, 3, &scene);
        assert_eq!(dry.v.y, -GRAVITY_MOVE);
    }
}
