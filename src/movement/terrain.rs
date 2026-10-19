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
    /// The wall the body touched, where and how.
    pub wall_contact: Option<WallContact>,
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
    /// the roll, the hanging one while a ledge is held, the normal one
    /// otherwise (a dive included).
    fn collision_form(&self) -> Form {
        if self.mode == Move::Ledge && self.catch.is_hanging() {
            return Form::WallGrab;
        }
        if self.mode == Move::Pole && self.pole.is_form() {
            return Form::Pole;
        }
        if matches!(
            self.mode,
            Move::SquatBrake | Move::Crouch | Move::Crawl | Move::Roll | Move::RollEnd
        ) {
            Form::Mini
        } else {
            Form::Normal
        }
    }

    /// The collider's form and cut direction for the actor's state.
    fn prepare_collider(&mut self) {
        let form = self.collision_form();
        self.collider.set_form(form);
        // The hanging forms drop the push along the direction they face.
        self.collider.set_cut_direction(match form {
            Form::WallGrab => self.catch.front(),
            Form::Pole => self.pole.front(),
            _ => Vec3::ZERO,
        });
    }

    /// The toy course's poles (the box solids are resolved by the demo's own
    /// code): the player collides with their triangles like with a stage's
    /// and the contact is recorded for the pole judge.
    pub(super) fn collide_toy_poles(&mut self, scene: &Course) {
        if scene.poles.is_empty() {
            return;
        }
        let mesh = scene.toy_pole_mesh();
        self.prepare_collider();
        let start = to_cm(self.pos);
        let holding = matches!(self.mode, Move::Ledge | Move::Pole);
        if let Some(stood) = self.catch_collider_at.take()
            && holding
        {
            self.collider.carry_from(stood);
        }
        let yaw = self.facing.x.atan2(self.facing.z);
        let outcome = self.collider.collide(&mesh, start, Vec3::ZERO, yaw);
        if holding {
            self.catch_collider_at = Some(outcome.pos);
        }
        self.pos += (outcome.pos - start) * UNIT;
        let collider = &self.collider;
        if let Some(wall) = &collider.wall {
            self.touched.active = true;
            self.touched.wall = Some(mesh.codes_of(wall.triangle).clone());
            self.touched.wall_contact = Some(WallContact {
                normal: wall.normal,
                position: wall.position,
                at_face: wall.location == crate::collider::Location::Face,
            });
            self.wall = wall.normal.with_y(0.).normalize_or_zero();
        }
        if let Some(ceiling) = &collider.ceiling {
            self.touched.active = true;
            self.touched.ceiling = Some(mesh.codes_of(ceiling.triangle).clone());
        }
        if let Some(floor) = &collider.floor {
            self.touched.active = true;
            self.touched.floor = Some(mesh.codes_of(floor.triangle).clone());
            if self.v.y <= 0. {
                self.v.y = 0.;
                self.grounded = true;
            }
        }
    }

    /// Moves the player by this frame's velocity through the mesh.
    pub(super) fn collide_mesh(&mut self, mesh: &TriangleMesh, was_grounded: bool) {
        self.prepare_collider();
        let start = mesh.snap_to_game_precision(to_cm(self.pos));
        // On the ground the controller keeps no downward speed of its own; the
        // game's velocity carries the pull (`units_per_frame`), and it is that
        // push into the floor that keeps the ground contact every frame.
        let mut velocity = self.v;
        if was_grounded && (self.v.y == 0. || self.v.y == -self.pull) {
            velocity = self.ground_following();
        }
        let yaw = self.facing.x.atan2(self.facing.z);
        // The ledge and pole holds put the actor on their pose before the
        // collider runs; the collider carries itself from where it stood last
        // step to there.
        let holding = matches!(self.mode, Move::Ledge | Move::Pole);
        if let Some(stood) = self.catch_collider_at.take()
            && holding
        {
            self.collider.carry_from(stood);
        }
        let outcome = self.collider.collide(mesh, start, velocity, yaw);
        if holding {
            self.catch_collider_at = Some(outcome.pos);
        }
        // The plain end of the move keeps the controller's own arithmetic;
        // only what the contacts changed is taken from the collider.
        // (On the ground that includes the horizontal part of the pull along a
        // tilted floor's normal, which the push out of the floor takes back.)
        self.pos += Vec3::new(velocity.x, self.v.y, velocity.z) * UNIT;
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
            wall_contact: collider.wall.as_ref().map(|c| WallContact {
                normal: c.normal,
                position: c.position,
                at_face: c.location == crate::collider::Location::Face,
            }),
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
                // A contact reached from the air is a landing, and a ground
                // state that keeps a downward speed of its own (the pound's
                // landing) reports it; the plain pull of the ground states is
                // reported as the ground-following velocity.
                if !was_grounded || self.v.y != -self.pull {
                    self.landing_speed = self.landing_speed.min(self.v.y);
                }
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
                // The air states keep their velocity against a wall (the wall
                // slide starts from it, and the jump's own control has nothing
                // that takes it off); the collider's push alone holds the body.
                if into > 0. && !self.keeps_velocity_at_walls() {
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
            }
        }
    }

    /// The states whose velocity a wall contact leaves alone: the jump family,
    /// the fall and the wall states.
    fn keeps_velocity_at_walls(&self) -> bool {
        self.mode == Move::WallSlide || self.wall_keep_applies()
    }
}
