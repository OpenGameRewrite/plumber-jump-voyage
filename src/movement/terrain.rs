//! The player's contact with a triangle mesh: the real game's stage collision
//! a replay loads (`Course::mesh`, see `geometry.rs`).
//!
//! The collider itself is `collider.rs`, a port of the game's `PlayerCollider`.
//! This is what the controller does with its answer: which form the body
//! has, how the position and velocity follow the contacts and which surfaces
//! were touched (their collision codes ride along for the states that will
//! need them: slope slides, wall jumps, materials).
use super::*;
use crate::collider::Form;
use crate::geometry::CollisionCodes;

/// The collision codes of the surfaces the body touched in the last step.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Touched {
    /// The last step was collided against a mesh (the other fields are only
    /// meaningful then).
    pub active: bool,
    pub floor: Option<CollisionCodes>,
    pub wall: Option<CollisionCodes>,
    pub ceiling: Option<CollisionCodes>,
}

/// The player's position is kept in metres in single precision, so a fall
/// the game ends exactly on a surface (heights it holds as exact sums of
/// half centimetres) can arrive a hair above or below it here. The collider
/// takes the position on a grid of 1/1000 cm, which is finer than anything the
/// game's own values resolve and coarser than that storage error. This is
/// numeric conditioning of the unit conversion, not a rule of the game.
const GRID_CM: f64 = 1e-3;

fn to_cm(metres: Vec3) -> Vec3 {
    let snap = |m: f32| ((m as f64 * 100. / GRID_CM).round() * GRID_CM) as f32;
    Vec3::new(snap(metres.x), snap(metres.y), snap(metres.z))
}

impl Player {
    /// The collision form `PlayerFormSensorCollisionArranger::syncForm` picks
    /// for the actor's nerve: the mini one for the squat, the slope slide and
    /// the roll, the normal one otherwise (a dive included).
    fn collision_form(&self) -> Form {
        if matches!(
            self.mode,
            Move::SquatBrake | Move::Crouch | Move::Crawl | Move::Roll | Move::RollEnd
        ) {
            Form::Mini
        } else {
            Form::Normal
        }
    }

    /// Moves the player by this frame's velocity through the mesh.
    pub(super) fn collide_mesh(&mut self, o: &Orders, mesh: &TriangleMesh, was_grounded: bool) {
        self.collider.set_form(self.collision_form());
        let start = to_cm(self.pos);
        // On the ground the controller keeps no downward speed of its own; the
        // game's velocity carries the pull (`units_per_frame`), and it is that
        // push into the floor that keeps the ground contact every frame.
        let mut velocity = self.v;
        if was_grounded && (self.v.y == 0. || self.v.y == -self.pull) {
            velocity.y = -self.pull;
        }
        let yaw = self.facing.x.atan2(self.facing.z);
        let outcome = self.collider.collide(mesh, start, velocity, yaw);
        // The plain end of the move keeps the controller's own arithmetic;
        // only what the contacts changed is taken from the collider.
        self.pos += self.v * UNIT;
        if outcome.collided {
            let correction = outcome.pos - (start + velocity);
            self.pos.x += correction.x * UNIT;
            self.pos.z += correction.z * UNIT;
            if correction.y != 0. {
                self.pos.y = outcome.pos.y * UNIT;
            }
        }
        let collider = &self.collider;
        self.touched = Touched {
            active: true,
            floor: collider
                .floor
                .as_ref()
                .map(|c| mesh.codes_of(c.triangle).clone()),
            wall: collider
                .wall
                .as_ref()
                .map(|c| mesh.codes_of(c.triangle).clone()),
            ceiling: collider
                .ceiling
                .as_ref()
                .map(|c| mesh.codes_of(c.triangle).clone()),
        };
        self.mesh_ground = None;
        if let Some(floor) = &collider.floor {
            let normal = if collider.ground_normal == Vec3::ZERO {
                floor.normal
            } else {
                collider.ground_normal
            };
            self.mesh_ground = Some(normal);
            // Kept for the box-ramp states until they take the normal itself.
            self.slope = if normal.y > 0. {
                -normal.x / normal.y
            } else {
                0.
            };
            if self.v.y <= 0. {
                self.landing_speed = self.landing_speed.min(self.v.y);
                self.v.y = 0.;
                self.grounded = true;
            } else {
                // The collider still reports the floor under a hop that
                // leaves it (the roll's start).
                self.hop_contact = true;
            }
        }
        if collider.ceiling.is_some() && self.v.y > 0. {
            self.v.y = 0.;
            // A hit head ends the jump's extend window.
            self.air.cancel_extend();
        }
        if let Some(wall) = collider.wall.clone() {
            let normal = wall.normal.with_y(0.).normalize_or_zero();
            if normal != Vec3::ZERO {
                self.wall = normal;
                let into = -self.v.dot(normal);
                if into > 0. {
                    self.v += normal * into;
                }
                if self.mode == Move::Roll && into > 12. {
                    // Rolling into a wall knocks the roll back out.
                    self.facing = (self.facing - self.wall * 2. * self.facing.dot(self.wall))
                        .normalize_or(self.wall);
                    self.set_h(self.facing * 6.);
                    self.set(Move::RollEnd);
                    self.play("RollingStandUp");
                }
                if !was_grounded && self.v.y < 0. && into > 0.1 {
                    let top = self.wall_top(mesh, wall.position, normal);
                    self.touch_wall(top, o);
                }
            }
        }
    }

    /// Height (metres) of the top of the wall touched at `contact`: the
    /// highest floor-like surface just behind it, or well out of reach.
    fn wall_top(&self, mesh: &TriangleMesh, contact: Vec3, normal: Vec3) -> f32 {
        let behind = contact - normal * 5.;
        mesh.floor_below(
            behind.x,
            behind.z,
            self.pos.y / UNIT + 300.,
            crate::collider::walkable_normal_y(),
        )
        .map_or(self.pos.y + 10., |y| y * UNIT)
    }
}
