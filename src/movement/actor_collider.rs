//! The sphere collider of an ordinary actor (`al::Collider`, `LiveActor::updateCollider`)
//! against static terrain.
//!
//! An actor with a collider moves by what `Collider::collide` makes of its velocity: the check
//! sphere sweeps along the motion in steps, every contact pushes the centre out, and the rest
//! of the motion slides along the plane of the contact. The class also keeps the deepest floor,
//! wall and ceiling contact of the last move, which the actor's states read back
//! (`al::isCollidedGround`, `getCollidedGroundNormal`, ...).
//!
//! Scope: the shapes and flags of an actor made with `initCollider` (robust check on, edge-aware
//! fixes off) meeting terrain that does not move. Moving collision parts (the reaction to a
//! host's own motion) are not ported; the hit's host-motion fields stay zero for static parts.
//! Everything runs in the game's coordinates in single precision, in the order the executable
//! does it.
use super::pose_math::{dot, squared_length, try_normalize_or_zero};
use bevy::prelude::Vec3;

/// The contact planes' depth before any contact (`-99999`): a plane is present when its depth is
/// not negative.
const NO_CONTACT: f32 = -99_999.;
/// `isFloorPolygon` / `isWallPolygon`: a face is a wall under this slope of its normal to the
/// gravity, a floor or ceiling from it on (cosine of 70 degrees).
const WALL_SLOPE: f32 = 0.342_020_15;
/// The sweep takes steps of at most this fraction of the radius, and never more than 35.
const SWEEP_STEP_PER_RADIUS: f32 = 0.9;
const SWEEP_STEP_MAX: f32 = 35.;
/// `isNearZero` tolerance of the motion tests.
const NEAR_ZERO: f32 = 0.001;
/// An actor's own gravity: `LiveActor` colliders use the actor's gravity pointer, (0, -1, 0).
const GRAVITY: Vec3 = Vec3::new(0., -1., 0.);

/// What `Collider::collide` needs from the world: the sphere's contacts with the terrain.
pub trait Terrain {
    /// Every face the sphere at `centre` (game coordinates) touches.
    fn strike_sphere(&self, centre: Vec3, radius: f32) -> Vec<StrikeHit>;
}

/// `al::SphereHitInfo`: one face touched by the check sphere.
#[derive(Clone, Copy, Debug, Default)]
pub struct StrikeHit {
    /// Index of the face in the terrain (identifies the triangle for callers).
    pub face: u32,
    /// The face normal (`Triangle::getFaceNormal`).
    pub normal: Vec3,
    /// How far the centre must move along the normal to just touch.
    pub depth: f32,
    /// The touched point of the face.
    pub position: Vec3,
    /// The sphere centre of the check.
    pub centre: Vec3,
    /// The centre lies over the face (`HitInfo::isCollisionAtFace`), not beside an edge or corner.
    pub at_face: bool,
}

/// One of the three stored contacts (floor, wall, ceiling): the hit with the greatest depth.
#[derive(Clone, Copy, Debug)]
pub struct Contact {
    pub depth: f32,
    pub hit: StrikeHit,
}

impl Default for Contact {
    fn default() -> Self {
        Self {
            depth: NO_CONTACT,
            hit: StrikeHit::default(),
        }
    }
}

impl Contact {
    pub fn is_present(&self) -> bool {
        self.depth >= 0.
    }
}

/// `al::SphereInterpolator`: the check position along a sweep, from the first sample to the
/// last, with the radius changing on the way.
#[derive(Clone, Copy, Debug)]
struct Sweep {
    start: Vec3,
    motion: Vec3,
    radius_start: f32,
    radius_end: f32,
    step: f32,
    t: f32,
    previous_t: f32,
}

impl Sweep {
    /// `SphereInterpolator::startInterp`: the step is the longest the sphere may go in one sample.
    fn new(start: Vec3, end: Vec3, radius_start: f32, radius_end: f32, max_step: f32) -> Self {
        let motion = end - start;
        let length = squared_length(motion).sqrt();
        let span = (length + radius_end) - radius_start;
        Self {
            start,
            motion,
            radius_start,
            radius_end,
            step: if span > 0. { max_step / span } else { 1. },
            t: 0.,
            previous_t: 0.,
        }
    }

    fn next_step(&mut self) {
        self.previous_t = self.t;
        self.t = (self.t + self.step).min(1.);
    }

    fn finished(&self) -> bool {
        self.previous_t == 1. && self.t == 1.
    }

    fn position(&self) -> Vec3 {
        self.start + self.motion * self.t
    }

    fn radius(&self) -> f32 {
        self.radius_start + (self.radius_end - self.radius_start) * self.t
    }

    /// The part of the motion still ahead of the current sample.
    fn remaining(&self) -> Vec3 {
        self.motion * (1. - self.t)
    }
}

#[derive(Clone, Debug)]
pub struct ActorCollider {
    radius: f32,
    /// The check centre sits this far along the actor's up axis from its translation.
    offset_y: f32,
    previous_centre: Vec3,
    previous_radius: f32,
    hits: Vec<StrikeHit>,
    pub floor: Contact,
    pub wall: Contact,
    pub ceiling: Contact,
    /// Frames since the last floor contact (`None` until there was one since the last reset).
    no_ground_counter: Option<u32>,
    face_floor: bool,
    face_wall: bool,
    face_ceiling: bool,
}

impl ActorCollider {
    /// `Collider::Collider` with `LiveActor::initCollider`'s flags: the previous check position
    /// starts at the translation itself.
    pub fn new(radius: f32, offset_y: f32, translation: Vec3) -> Self {
        Self {
            radius,
            offset_y,
            previous_centre: translation,
            previous_radius: radius,
            hits: Vec::new(),
            floor: Contact::default(),
            wall: Contact::default(),
            ceiling: Contact::default(),
            no_ground_counter: None,
            face_floor: false,
            face_wall: false,
            face_ceiling: false,
        }
    }

    pub fn set_offset_y(&mut self, offset_y: f32) {
        self.offset_y = offset_y;
    }

    pub fn set_radius(&mut self, radius: f32) {
        self.radius = radius;
    }

    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// `Collider::calcCheckPos`: the centre of the check sphere.
    fn check_centre(&self, translation: Vec3, up: Vec3) -> Vec3 {
        Vec3::new(
            translation.x + up.x * self.offset_y,
            translation.y + up.y * self.offset_y,
            translation.z + up.z * self.offset_y,
        )
    }

    /// `Collider::onInvalidate` (`al::onCollide`): forget the contacts and start the sweep from
    /// where the actor stands now.
    pub fn invalidate(&mut self, translation: Vec3, up: Vec3) {
        self.floor = Contact::default();
        self.wall = Contact::default();
        self.ceiling = Contact::default();
        self.no_ground_counter = None;
        self.hits.clear();
        self.previous_centre = self.check_centre(translation, up);
        self.previous_radius = self.radius;
    }

    /// `Collider::collide`: the displacement of the actor for `velocity`.
    pub fn collide(
        &mut self,
        terrain: &impl Terrain,
        translation: Vec3,
        up: Vec3,
        velocity: Vec3,
    ) -> Vec3 {
        self.hits.clear();
        self.floor = Contact::default();
        self.wall = Contact::default();
        self.ceiling = Contact::default();
        let centre = self.check_centre(translation, up);
        let mut position = self.previous_centre;
        let mut radius = self.previous_radius;
        let catch_up_step = (self.radius * SWEEP_STEP_PER_RADIUS)
            .min(self.previous_radius * SWEEP_STEP_PER_RADIUS)
            .min(SWEEP_STEP_MAX);
        let mut sweep = Sweep::new(
            self.previous_centre,
            centre,
            self.previous_radius,
            self.radius,
            catch_up_step,
        );
        // The centre may have moved since the last collision (a pose change, a teleport):
        // resolve the contacts of that jump first.
        let jump = centre - self.previous_centre;
        if !(is_near_zero(jump) && (self.previous_radius - self.radius).abs() < NEAR_ZERO) {
            self.pre_collide(terrain, &mut sweep, &mut position, &mut radius);
        }
        let motion_step = (self.previous_radius * SWEEP_STEP_PER_RADIUS).min(SWEEP_STEP_MAX);
        sweep = Sweep::new(
            position,
            position + velocity,
            self.radius,
            self.radius,
            motion_step,
        );
        let mut found = self.find_collide_pos(terrain, &mut sweep);
        let displacement = if !found && sweep.finished() {
            (position - centre) + velocity
        } else {
            let mut index = 0;
            loop {
                let (reaction, summed_normal) = self.moment_fix_reaction(index, true);
                let found_now = self.hits.len() - index;
                position = sweep.position();
                radius = sweep.radius();
                let mut remaining = sweep.remaining();
                position += reaction;
                index += found_now;
                let direction = try_normalize_or_zero(if is_near_zero(reaction) {
                    summed_normal
                } else {
                    reaction
                })
                .0;
                let along = dot(remaining, direction);
                if along < 0. {
                    remaining -= direction * along;
                }
                if dot(velocity, remaining) < 0. {
                    break;
                }
                sweep = Sweep::new(
                    position,
                    position + remaining,
                    radius,
                    self.radius,
                    motion_step,
                );
                sweep.next_step();
                let before = self.hits.len();
                found = self.find_collide_pos(terrain, &mut sweep);
                let stored = self.hits.len() - before;
                if found && sweep.t < 1. {
                    continue;
                }
                position = sweep.position();
                if found && stored > 0 {
                    let (reaction, _) = self.moment_fix_reaction(index, false);
                    position += reaction;
                }
                break;
            }
            self.store_contact_planes();
            position - centre
        };
        self.previous_centre = displacement + centre;
        self.previous_radius = self.radius;
        self.no_ground_counter = if self.floor.depth < 0. {
            self.no_ground_counter.map(|n| n.saturating_add(1))
        } else {
            Some(0)
        };
        displacement
    }

    /// `Collider::preCollide`: sweep the check sphere from where it was to where the actor's
    /// pose now puts it, pushing it out of what it meets.
    fn pre_collide(
        &mut self,
        terrain: &impl Terrain,
        sweep: &mut Sweep,
        position: &mut Vec3,
        radius: &mut f32,
    ) {
        let mut pushed = Vec3::ZERO;
        let mut touched = false;
        while !sweep.finished() {
            let at = sweep.position() + pushed;
            if self.strike(terrain, at, sweep.radius()) {
                touched = true;
                let (reaction, _) = self.moment_fix_reaction(0, false);
                pushed += reaction;
                if sweep.t >= 1. {
                    break;
                }
            }
            sweep.next_step();
        }
        *position = sweep.position() + pushed;
        *radius = sweep.radius();
        if touched {
            self.store_contact_planes();
            self.hits.clear();
        }
    }

    /// `Collider::findCollidePos`: walk the sweep until the sphere touches something.
    fn find_collide_pos(&mut self, terrain: &impl Terrain, sweep: &mut Sweep) -> bool {
        while !sweep.finished() {
            if self.strike(terrain, sweep.position(), sweep.radius()) {
                return true;
            }
            sweep.next_step();
        }
        false
    }

    /// The check at one sample; the contacts are appended to the hits of this move.
    fn strike(&mut self, terrain: &impl Terrain, centre: Vec3, radius: f32) -> bool {
        let found = terrain.strike_sphere(centre, radius);
        self.hits.extend(found.iter().copied());
        !found.is_empty()
    }

    /// `Collider::obtainMomentFixReaction`: the push-out of the hits from `start` on (the
    /// componentwise extremes of the single pushes, summed) and, when asked for, the same sum of
    /// their push directions.
    fn moment_fix_reaction(&mut self, start: usize, want_normals: bool) -> (Vec3, Vec3) {
        self.face_floor = false;
        self.face_wall = false;
        self.face_ceiling = false;
        for hit in &self.hits[start..] {
            if is_floor(hit.normal) {
                self.face_floor |= hit.at_face;
            } else if is_wall(hit.normal) {
                self.face_wall |= hit.at_face;
            } else {
                self.face_ceiling |= hit.at_face;
            }
        }
        let mut fix = Extremes::default();
        let mut directions = Extremes::default();
        for hit in &self.hits[start..] {
            // The collider's edge-aware floor fixes are off: a floor always pushes along its
            // normal. Walls and ceilings do too once one of them was touched by its face.
            let by_normal = if is_floor(hit.normal) {
                true
            } else if is_wall(hit.normal) {
                self.face_wall
            } else {
                self.face_ceiling
            };
            let (push, direction) = if by_normal {
                (hit.normal * hit.depth, hit.normal)
            } else {
                edge_fix(hit)
            };
            fix.include(push);
            if want_normals {
                directions.include(direction);
            }
        }
        (fix.sum(), directions.sum())
    }

    /// `Collider::storeContactPlane`: keep the deepest floor, wall and ceiling hit.
    fn store_contact_planes(&mut self) {
        for hit in &self.hits {
            let slot = if is_floor(hit.normal) {
                &mut self.floor
            } else if is_wall(hit.normal) {
                &mut self.wall
            } else {
                &mut self.ceiling
            };
            if slot.depth < hit.depth {
                *slot = Contact {
                    depth: hit.depth,
                    hit: *hit,
                };
            }
        }
    }
}

/// `SphereHitInfo::calcFixVector` for a hit beside an edge or corner: the push goes from the
/// touched point to the centre, as long as the penetration measured along the normal.
fn edge_fix(hit: &StrikeHit) -> (Vec3, Vec3) {
    let direction = try_normalize_or_zero(hit.centre - hit.position).0;
    let n = hit.normal;
    let along = (n.x * direction.x + n.y * direction.y) + n.z * direction.z;
    let depth_along = ((hit.depth * n.x) * direction.x + (hit.depth * n.y) * direction.y)
        + (hit.depth * n.z) * direction.z;
    (direction * depth_along, direction * along)
}

/// Componentwise minimum and maximum of the vectors seen (starting from zero), whose sum is
/// the combined reaction.
#[derive(Default)]
struct Extremes {
    min: Vec3,
    max: Vec3,
}

impl Extremes {
    fn include(&mut self, v: Vec3) {
        self.max = self.max.max(v);
        self.min = self.min.min(v);
    }

    fn sum(&self) -> Vec3 {
        self.max + self.min
    }
}

/// `al::isNearZero(vector, 0.001)`.
fn is_near_zero(v: Vec3) -> bool {
    squared_length(v) < NEAR_ZERO * NEAR_ZERO
}

fn slope(normal: Vec3) -> Option<f32> {
    (!is_near_zero(normal)).then(|| dot(normal, GRAVITY))
}

/// `al::isFloorPolygon`: facing against the gravity, within 70 degrees of it.
fn is_floor(normal: Vec3) -> bool {
    slope(normal).is_some_and(|d| d < 0. && d.abs() >= WALL_SLOPE)
}

/// `al::isWallPolygon`: steeper than 70 degrees.
fn is_wall(normal: Vec3) -> bool {
    slope(normal).is_some_and(|d| d.abs() < WALL_SLOPE)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Plane(f32);

    impl Terrain for Plane {
        fn strike_sphere(&self, centre: Vec3, radius: f32) -> Vec<StrikeHit> {
            let depth = radius - (centre.y - self.0);
            if depth < 0. {
                return Vec::new();
            }
            vec![StrikeHit {
                normal: Vec3::Y,
                depth,
                position: Vec3::new(centre.x, self.0, centre.z),
                centre,
                at_face: true,
                ..default()
            }]
        }
    }

    fn default<T: Default>() -> T {
        T::default()
    }

    #[test]
    fn free_flight_moves_by_the_velocity_and_reports_no_contact() {
        let mut collider = ActorCollider::new(40., 40., Vec3::new(0., 100., 0.));
        let step = collider.collide(
            &Plane(0.),
            Vec3::new(0., 100., 0.),
            Vec3::Y,
            Vec3::new(1., -5., 2.),
        );
        assert_eq!(step, Vec3::new(1., -5., 2.));
        assert!(!collider.floor.is_present());
    }

    #[test]
    fn a_fall_into_the_floor_stops_at_the_surface_and_reports_the_ground() {
        let mut collider = ActorCollider::new(40., 40., Vec3::new(0., 5., 0.));
        collider.invalidate(Vec3::new(0., 5., 0.), Vec3::Y);
        let step = collider.collide(
            &Plane(0.),
            Vec3::new(0., 5., 0.),
            Vec3::Y,
            Vec3::new(0., -30., 0.),
        );
        // The centre ends on the sphere's radius above the plane: the feet on the floor.
        assert_eq!(5. + step.y, 0.);
        assert!(collider.floor.is_present());
        assert_eq!(collider.floor.hit.normal, Vec3::Y);
    }
}
