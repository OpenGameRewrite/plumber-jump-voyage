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
//! steers from the first step (`spin_ground`). Not ported: the ice floor's own handling
//! (`isOnGroundSkateCode`, the hit reactions and the first-step velocity at 0x482278..0x4823f8)
//! and, on the water's surface, the nerve's `al::approachWaterSurfaceRate` after the control
//! (0x4827a8..0x482804, from `PlayerConst`'s SwimRunSurface constants).
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
}
