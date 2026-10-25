//! `PlayerCollider::calcResultVec` and what it calls, operation for operation in single
//! precision: the flag pass over the results, `calcGroundArrowAverage` (with the per-arrow
//! slot update of `0x433d98`), `calcResultVecArrow` (`0x431ea4`), `calcResultVecSphere`
//! (`0x432368`), the push-direction bits (`0x4338f0`) and the averaging with the round before.
//!
//! The arithmetic is the machine code's, in its order: a rewrite that is algebraically the
//! same moves the body by a unit in the last place, and the next contact then falls on the
//! other side of a face. The functions run against the executable's own in a local interpreter
//! (`PJV_COLLIDER_CASES`, see `tests/collider_result.rs`).
//!
//! Left out: the squeeze test of the `Press` collision code (it only changes the fix for faces
//! with that code and sets the two press bytes), the moving parts' separate accumulators and
//! the per-class `HitInfo` arrays (`collectHitInfoArray`), and the disk shape.
use super::*;
use crate::movement::console_libm::cosf;

/// What `calcResultVecSphere` and `calcResultVecArrow` read from a shape of the keeper.
#[derive(Clone, Copy, Debug)]
pub(super) enum ShapeFacts {
    Sphere {
        /// `CollisionShapeInfoSphere::+0x44/+0x4c`: the body sphere that holds the player up
        /// and how far from its axis a contact still counts as directly underneath.
        support: Option<f32>,
        /// `+0x5c`: the support axis in the world (the body's up, tilted with the pose).
        support_axis: Vec3,
        /// `+0x68`, `createShapeSphereIgnoreGround`.
        ignore_ground: bool,
        /// `+0x69`.
        ignore_ceiling: bool,
    },
    Arrow {
        /// `+0x18`: the arrow's slot among the three.
        index: usize,
        /// `+0x54`: the depth below which the arrow only supports.
        margin: f32,
        /// `+0x7c`: the arrow's direction in the world (normalised here).
        direction: Vec3,
    },
}

/// One entry of the keeper's list of collided shapes.
#[derive(Clone, Copy, Debug)]
pub(super) struct ContactFacts {
    pub shape: ShapeFacts,
    pub hit: Hit,
    /// The struck face's normal.
    pub normal: Vec3,
    /// The face's index in the mesh.
    pub triangle: u32,
}

/// `PlayerCollider::+0x11c`, when a wall hit with a face right behind it is dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BorderCheck {
    Off,
    /// Edge and corner hits only (`setWallBorderCheckTypeNoFace`).
    NoFace,
    /// Every wall hit (`setWallBorderCheckTypeAll`).
    All,
}

/// What `calcResultVec` hands back.
#[derive(Clone, Copy, Debug)]
pub(super) struct Solved {
    /// The push (after the averaging with the round before).
    pub fix: Vec3,
    /// The push before that averaging.
    pub total: Vec3,
    /// The flag word's low bits: 1 floor face, 2 wall face, 4 ceiling face, 0x40 arrow on floor.
    pub flags: u32,
}

/// `PlayerCollider::+0x10d`: the byte that lets floor faces push the spheres (always set).
/// `PlayerCollider::+0x1b0` is the slope limit in degrees (`SLOPE_LIMIT_DEGREES`).
/// `CollisionShapeKeeper::+0x58`, the height of a step a body sphere's flank may stand on.

fn dot(a: Vec3, b: Vec3) -> f32 {
    (a.x * b.x + a.y * b.y) + a.z * b.z
}
fn squared_length(v: Vec3) -> f32 {
    (v.x * v.x + v.y * v.y) + v.z * v.z
}
/// `al::isNearZero(vec, limit)`.
fn near_zero_vec(v: Vec3, limit: f32) -> bool {
    squared_length(v) < limit * limit
}
/// `al::isNearZero(value, limit)`.
fn near_zero(value: f32, limit: f32) -> bool {
    value.abs() < limit
}
/// `al::verticalizeVec(out, vertical, vec)`.
fn verticalize(vertical: Vec3, vec: Vec3) -> Vec3 {
    let along = dot(vertical, vec);
    Vec3::new(
        vec.x - vertical.x * along,
        vec.y - vertical.y * along,
        vec.z - vertical.z * along,
    )
}
/// The `fcmp`/`fcsel` pair the machine code uses for a running minimum: the first when it is
/// less, else the second.
fn lesser(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}
fn greater(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}
/// `SphereHitInfo::calcFixVectorNormal`: the depth along the face normal.
fn fix_along_normal(depth: f32, normal: Vec3) -> Vec3 {
    Vec3::new(depth * normal.x, depth * normal.y, depth * normal.z)
}
/// `SphereHitInfo::calcFixVector`: along the normal on a face, along the line from the touched
/// point to the centre on an edge or corner (as long as the normal push reaches along it).
fn fix_vector(hit: &SphereHit, normal: Vec3) -> Vec3 {
    if hit.location == Location::Face {
        return fix_along_normal(hit.depth, normal);
    }
    let (to_centre, _) = try_normalize_or_zero(hit.center - hit.position);
    let pushed = fix_along_normal(hit.depth, normal);
    let along = dot(pushed, to_centre);
    Vec3::new(
        to_centre.x * along,
        to_centre.y * along,
        to_centre.z * along,
    )
}

/// The first pass of `calcResultVec`: what kinds of faces are among the results (a sphere
/// counts only on a face, not on an edge or corner).
#[derive(Default)]
struct Flags {
    floor: bool,
    wall: bool,
    ceiling: bool,
    /// An arrow found floor.
    arrow_floor: bool,
}
impl Flags {
    fn of(results: &[ContactFacts]) -> Self {
        let mut flags = Self::default();
        for result in results {
            match result.hit {
                Hit::Arrow(_) => {
                    // An arrow's face with no normal is skipped, as is a non-floor one.
                    if !near_zero_vec(result.normal, NEAR_ZERO)
                        && classify(result.normal) == Class::Floor
                    {
                        flags.arrow_floor = true;
                    }
                }
                Hit::Sphere(hit) if hit.location == Location::Face => {
                    match classify(result.normal) {
                        Class::Floor => flags.floor = true,
                        Class::Wall => flags.wall = true,
                        Class::Ceiling => flags.ceiling = true,
                    }
                }
                Hit::Sphere(_) => {}
            }
        }
        flags
    }
    fn bits(&self) -> u32 {
        u32::from(self.floor)
            | u32::from(self.wall) << 1
            | u32::from(self.ceiling) << 2
            | u32::from(self.arrow_floor) << 6
    }
}

/// Per axis, the most negative and most positive fix any contact asked for (starting at
/// zero); the body moves by their sum.
#[derive(Default)]
struct Fixes {
    low: Vec3,
    high: Vec3,
    /// Which ways the contributions pushed, as the `PUSH_*` bits.
    directions: u32,
}
impl Fixes {
    /// A contribution `fix`; `normal` stands in for its direction when it is too short to
    /// have one (`0x4338f0`).
    fn add(&mut self, fix: Vec3, normal: Vec3) {
        self.low = Vec3::new(
            lesser(self.low.x, fix.x),
            lesser(self.low.y, fix.y),
            lesser(self.low.z, fix.z),
        );
        self.high = Vec3::new(
            greater(self.high.x, fix.x),
            greater(self.high.y, fix.y),
            greater(self.high.z, fix.z),
        );
        self.directions |= push_directions(fix, normal);
    }
}

/// `0x4338f0`: the push direction bits of a fix. A fix too short to point anywhere is
/// replaced by `normal` with its components under 0.05 dropped, normalised. Otherwise a
/// component whose share of the fix's length is under 0.05 is dropped (the fix itself, not
/// the share, is what is kept for the sign test). Each remaining component of at least 0.001
/// sets the bit of its sign.
fn push_directions(fix: Vec3, normal: Vec3) -> u32 {
    let mut direction = fix.to_array();
    if near_zero_vec(fix, NEAR_ZERO) {
        direction = normal.to_array();
        for c in &mut direction {
            if near_zero(*c, 0.05) {
                *c = 0.;
            }
        }
        direction = normalize(Vec3::from_array(direction)).to_array();
    } else {
        let length = squared_length(fix).sqrt();
        for (c, whole) in direction.iter_mut().zip(fix.to_array()) {
            if near_zero(whole / length, 0.05) {
                *c = 0.;
            }
        }
    }
    let mut bits = 0;
    for (component, (positive, negative)) in direction.into_iter().zip([
        (PUSH_POS_X, PUSH_NEG_X),
        (PUSH_POS_Y, PUSH_NEG_Y),
        (PUSH_POS_Z, PUSH_NEG_Z),
    ]) {
        if near_zero(component, NEAR_ZERO) {
            continue;
        }
        bits |= if component > 0. { positive } else { negative };
    }
    bits
}

/// The last step of `calcResultVec`: a push along an axis that goes the way the round
/// before pushed back against the move (`previous` is the earlier round's push less the move
/// left, see `move_collide`) is averaged with it, or replaced by it when this round has
/// nothing pushing the other way.
fn average_with_previous(fix: Vec3, now: u32, previous: Vec3, before: u32) -> Vec3 {
    let mut result = fix.to_array();
    let axes = [
        (PUSH_POS_X, PUSH_NEG_X),
        (PUSH_POS_Y, PUSH_NEG_Y),
        (PUSH_POS_Z, PUSH_NEG_Z),
    ];
    for (axis, (positive, negative)) in axes.into_iter().enumerate() {
        let current = result[axis];
        let earlier = previous[axis];
        let faint = near_zero(earlier, NEAR_ZERO);
        let both_ways = |other: u32| {
            if now & other != 0 {
                (current + earlier) * 0.5
            } else {
                earlier
            }
        };
        if before & negative != 0 {
            if current > 0. && current > earlier && (earlier >= 0. || faint) {
                result[axis] = both_ways(positive);
            }
        } else if before & positive != 0
            && current < 0.
            && current < earlier
            && (earlier <= 0. || faint)
        {
            result[axis] = both_ways(negative);
        }
    }
    Vec3::from_array(result)
}

impl PlayerCollider {
    /// `PlayerCollider::calcResultVec`, for static geometry: the single fix vector that
    /// resolves the results of one pose, and the bookkeeping of the deepest contacts.
    /// `supports` are the arrows that only support. `behind` answers
    /// `rs::calcExistCollisionBorder` (a face right behind a point), `previous` is what the
    /// round before told this one (see `move_collide`).
    pub(super) fn solve_results(
        &mut self,
        results: &[ContactFacts],
        supports: &[ContactFacts],
        previous: Vec3,
        border: BorderCheck,
        behind: &mut dyn FnMut(Vec3, Vec3) -> bool,
    ) -> Solved {
        // `str wzr, [x21, #0x108]`: this round's flags start empty; the last round's push
        // bits are kept for the averaging.
        let before = self.push_directions;
        let flags = Flags::of(results);
        self.average_ground(results, supports);
        let mut fixes = Fixes::default();
        for result in results {
            match result.shape {
                ShapeFacts::Arrow { .. } => {
                    if flags.arrow_floor {
                        self.arrow_fix(result, &mut fixes);
                    }
                }
                ShapeFacts::Sphere { .. } => {
                    self.sphere_fix(result, &flags, border, behind, &mut fixes);
                }
            }
        }
        // The deepest floor found so far provides the ground reference the arrows did not
        // (after the loop: the contacts of this round count).
        if let Some(floor) = &self.floor {
            if !self.has_ground_position {
                self.ground_position = floor.position;
                self.has_ground_position = true;
            }
            if !self.has_ground_normal {
                self.ground_normal = floor.normal;
                self.has_ground_normal = true;
            }
        }
        let total = fixes.low + fixes.high;
        let fix = average_with_previous(total, fixes.directions, previous, before);
        self.push_directions = fixes.directions;
        Solved {
            fix,
            total,
            flags: flags.bits(),
        }
    }

    /// `calcGroundArrowAverage`: the floor normal is the average of the distinct normals
    /// under the arrows (collisions and supports alike), the ground position the average of
    /// the contact points of the arrows that collided. Each arrow remembers its deepest floor
    /// hit over every round of the `collide` (the slots are cleared only when it starts).
    fn average_ground(&mut self, results: &[ContactFacts], supports: &[ContactFacts]) {
        self.ground_normal = Vec3::ZERO;
        self.ground_position = Vec3::ZERO;
        self.has_ground_normal = false;
        self.has_ground_position = false;
        for result in results.iter().chain(supports) {
            let (Hit::Arrow(hit), ShapeFacts::Arrow { index, margin, .. }) =
                (result.hit, result.shape)
            else {
                continue;
            };
            if near_zero_vec(result.normal, NEAR_ZERO) || classify(result.normal) != Class::Floor {
                continue;
            }
            let slot = &mut self.arrow_slots[index];
            if slot.is_none_or(|held| held.0 < hit.depth) {
                *slot = Some((hit.depth, margin, hit.position, result.normal));
            }
        }
        let mut position = Vec3::ZERO;
        let mut positions = 0;
        let mut normal = Vec3::ZERO;
        let mut normals = 0;
        let mut seen: Vec<Vec3> = Vec::new();
        for slot in self.arrow_slots.iter().flatten() {
            let (depth, margin, at, n) = *slot;
            if depth >= margin {
                position += at;
                positions += 1;
            }
            // Arrows on the same plane count once.
            if !seen.iter().any(|s| is_near_direction(*s, n, 0.01)) {
                normal += n;
                normals += 1;
            }
            seen.push(n);
        }
        // (The averages multiply by the reciprocal of the count, and the normal is made a
        // unit vector with `al::normalize`.)
        if positions > 0 {
            let reciprocal = 1. / positions as f32;
            self.ground_position = Vec3::new(
                position.x * reciprocal,
                position.y * reciprocal,
                position.z * reciprocal,
            );
            self.has_ground_position = true;
        }
        if normals > 0 {
            let reciprocal = 1. / normals as f32;
            self.ground_normal = normalize(Vec3::new(
                normal.x * reciprocal,
                normal.y * reciprocal,
                normal.z * reciprocal,
            ));
            self.has_ground_normal = true;
        }
    }

    /// `calcResultVecArrow`: an arrow that went in past its margin pushes the body along the
    /// average floor normal by the part it went in.
    fn arrow_fix(&mut self, result: &ContactFacts, fixes: &mut Fixes) {
        let (
            Hit::Arrow(hit),
            ShapeFacts::Arrow {
                margin, direction, ..
            },
        ) = (result.hit, result.shape)
        else {
            return;
        };
        if near_zero_vec(result.normal, NEAR_ZERO) || classify(result.normal) != Class::Floor {
            return;
        }
        let (direction, _) = try_normalize_or_zero(direction);
        let depth = (hit.depth - margin).max(0.);
        let ground = self.ground_normal;
        // The depth's share along the average floor normal, in the machine code's order.
        let push = ((direction.x * -depth) * ground.x - (direction.y * depth) * ground.y)
            - (direction.z * depth) * ground.z;
        let fix = Vec3::new(ground.x * push, ground.y * push, ground.z * push);
        fixes.add(fix, ground);
        self.record(
            Class::Floor,
            hit.depth,
            result,
            hit.position,
            Location::Face,
        );
    }

    /// `calcResultVecSphere`.
    fn sphere_fix(
        &mut self,
        result: &ContactFacts,
        flags: &Flags,
        border: BorderCheck,
        behind: &mut dyn FnMut(Vec3, Vec3) -> bool,
        fixes: &mut Fixes,
    ) {
        let (
            Hit::Sphere(hit),
            ShapeFacts::Sphere {
                support,
                support_axis,
                ignore_ground,
                ignore_ceiling,
            },
        ) = (result.hit, result.shape)
        else {
            return;
        };
        let normal = result.normal;
        let class = classify(normal);
        let at_face = hit.location == Location::Face;
        let fix = match class {
            Class::Floor => {
                if ignore_ground {
                    return;
                }
                if !self.keeper.has_arrow {
                    // No arrows: the sphere itself is the floor contact.
                    if flags.floor {
                        fix_along_normal(hit.depth, normal)
                    } else {
                        fix_vector(&hit, normal)
                    }
                } else if self.holds_up(&hit, support, support_axis, flags) {
                    fix_along_normal(hit.depth, normal)
                } else {
                    // Any other floor-like contact (the head's, or a step) can only push
                    // the body sideways, along the line to the sphere's centre.
                    if flags.arrow_floor {
                        let to_contact = hit.position - self.ground_position;
                        if dot(to_contact, self.ground_normal) < SAME_PLANE {
                            return;
                        }
                    }
                    let (to_centre, usable) = try_normalize_or_zero(hit.center - hit.position);
                    if !usable {
                        return;
                    }
                    let pushed = fix_along_normal(hit.depth, normal);
                    let along = dot(pushed, to_centre);
                    let side = verticalize(
                        GRAVITY,
                        Vec3::new(
                            to_centre.x * along,
                            to_centre.y * along,
                            to_centre.z * along,
                        ),
                    );
                    let (direction, usable) = try_normalize_or_zero(side);
                    if !usable {
                        return;
                    }
                    // A sideways push is no contact to remember.
                    fixes.add(side, direction);
                    return;
                }
            }
            Class::Wall => {
                // A wall hit with a face right behind it (the back of a thin wall) is
                // dropped.
                let checks = match border {
                    BorderCheck::Off => false,
                    BorderCheck::NoFace => !at_face,
                    BorderCheck::All => true,
                };
                if checks && behind(hit.position, normal) {
                    return;
                }
                if flags.wall {
                    fix_along_normal(hit.depth, normal)
                } else {
                    let fix = fix_vector(&hit, normal);
                    if at_face {
                        fix
                    } else {
                        verticalize(GRAVITY, fix)
                    }
                }
            }
            Class::Ceiling => {
                if ignore_ceiling {
                    return;
                }
                if flags.ceiling {
                    fix_along_normal(hit.depth, normal)
                } else {
                    fix_vector(&hit, normal)
                }
            }
        };
        fixes.add(fix, normal);
        self.record(class, hit.depth, result, hit.position, hit.location);
    }

    /// Whether a floor-like contact of a keeper with arrows pushes the body as ground does:
    /// the contact is directly underneath a ground-support sphere, or on its flank no higher
    /// than a small step above the ground plane the arrows found.
    fn holds_up(
        &self,
        hit: &SphereHit,
        support: Option<f32>,
        support_axis: Vec3,
        flags: &Flags,
    ) -> bool {
        let Some(range) = support else {
            return false;
        };
        let across = verticalize(support_axis, hit.position - hit.center);
        if squared_length(across).sqrt() < range {
            return true;
        }
        if !flags.arrow_floor {
            return false;
        }
        // `al::isNearZeroOrGreater(smallStep + gravity . (position - ground), 0.001)`.
        let height = SMALL_STEP + dot(GRAVITY, hit.position - self.ground_position);
        height >= 0. || near_zero(height, NEAR_ZERO)
    }

    /// Keeps the deepest contact of each class (the machine code copies the whole `HitInfo`).
    fn record(
        &mut self,
        class: Class,
        depth: f32,
        result: &ContactFacts,
        position: Vec3,
        location: Location,
    ) {
        let (slot, contact) = match class {
            Class::Floor => (0, &mut self.floor),
            Class::Wall => (1, &mut self.wall),
            Class::Ceiling => (2, &mut self.ceiling),
        };
        if self.best[slot] < depth {
            self.best[slot] = depth;
            *contact = Some(Contact {
                normal: result.normal,
                position,
                triangle: result.triangle,
                location,
            });
        }
    }
}

/// The slope limit as the machine code holds it: `cosf` of `+0x1b0` times pi/180.
pub(super) fn slope_cos() -> f32 {
    use std::sync::OnceLock;
    static LIMIT: OnceLock<f32> = OnceLock::new();
    *LIMIT.get_or_init(|| cosf(SLOPE_LIMIT_DEGREES * DEGREES_TO_RADIANS))
}

/// `al::isNearDirection`: the vectors do not point apart and every component of their cross
/// product is within `limit`.
pub(super) fn is_near_direction(a: Vec3, b: Vec3, limit: f32) -> bool {
    let cross = Vec3::new(
        a.y * b.z - b.y * a.z,
        b.x * a.z - a.x * b.z,
        a.x * b.y - b.x * a.y,
    );
    dot(a, b) >= 0. && cross.x.abs() <= limit && cross.y.abs() <= limit && cross.z.abs() <= limit
}

/// The three classes `calcResultVecSphere` sorts a face into: with gravity straight down, a
/// face is floor when its normal points up within the slope limit.
pub(super) fn classify_face(normal: Vec3) -> Class {
    if near_zero_vec(normal, NEAR_ZERO) {
        // The machine code files a face without a normal under the ceiling.
        return Class::Ceiling;
    }
    let against = dot(normal, GRAVITY);
    let limit = slope_cos();
    if against < 0. && against.abs() >= limit {
        Class::Floor
    } else if against.abs() < limit {
        Class::Wall
    } else {
        Class::Ceiling
    }
}

#[path = "tests/collider_result.rs"]
mod checks;

pub(super) fn self_check() {
    checks::run();
}
