//! The player's collider against a triangle mesh, written from the 1.0.0
//! executable's `PlayerCollider`, `CollisionMultiShape`, `CollisionShapeKeeper`
//! and the KCL server's sphere and arrow tests (`KCHitSphereForPlayer`,
//! `KCHitArrow`). Only what the traces and the decompiled headers explain is
//! in here; the places where the original does something this port leaves out
//! are named in the comments, with the function that does it.
//!
//! Units are the game's (cm). The caller hands in start-relative positions;
//! inside, the collider works in the stage's own absolute coordinates, like the
//! game, where every sum rounds at the stage's magnitude (4.9e-4 cm steps at
//! 5000 cm): whether a body that was pushed out of a wall touches it again
//! next frame is decided by that rounding. Everything is single precision on
//! purpose: whether a fall that ends exactly on a surface touches it depends
//! on how the arrow test rounds (see `arrow_hit`).
//!
//! # What the player is made of
//!
//! `PlayerColliderHakoniwa::init` builds one shape keeper per form. The two
//! used on foot are
//!
//! * normal: spheres "Body" (radius 55 at 65 above the feet, a ground-support
//!   sphere) and "Head" (radius 55 at 105), and three vertical arrows around
//!   the feet, 30 from the axis at 120 degrees, each starting 40 above the feet
//!   and 60 long (`createShapeArrow`, margin 20);
//! * mini (crouching, sliding, rolling): the same arrows and one sphere "Body"
//!   (radius 55 at 55, so its lowest point is the feet).
//!
//! The upright body is held up by the arrows alone (the sphere is 10 above the
//! feet), the crouched one by its sphere as well, which is why the two touch
//! the floor at different moments.
//!
//! # One frame
//!
//! `PlayerCollider::collide` runs `moveCollide` twice: from the stored pose to
//! the actor's (the platform carry, nothing for static ground) and then along
//! the frame's velocity. `moveCollide` walks the shapes along the move in
//! steps no longer than the keeper's check range, stops at the first pose at
//! which a shape is in collision, pushes the body out by the combined fix
//! vector (`calcResultVec`), removes the part of the remaining move that goes
//! into the surface and goes on, up to 100 rounds.
use crate::geometry::{Aabb, MeshTriangle, TriangleMesh};
use crate::prism::{PartFrame, Point, rotate_vector, transform_point};
use bevy::prelude::{Quat, Vec3};

/// `PlayerCollider::_1b0`, the slope limit in degrees: a face whose normal is
/// within 70 degrees of straight up is floor, within 70 of straight down is
/// ceiling, anything else a wall.
const SLOPE_LIMIT_DEGREES: f32 = 70.;
/// The decomp's `0.01745329...`, as the code multiplies it in single precision.
const DEGREES_TO_RADIANS: f32 = 0.017_453_292;
/// What `KCHitArrow` allows a hit to lie outside an edge by.
const ARROW_EDGE_BAND: f32 = 0.01;
/// `KCHitSphereForPlayer`: a corner hit counts only when the offset from the
/// corner is within this slope of the height above the face (the game's
/// literal 0.707106).
#[allow(clippy::approx_constant)] // the game's own 0.707106, not exactly one over root two
const EDGE_HIT_RATIO: f32 = 0.707_106;
/// Results the keeper stores before it stops adding (`CollisionShapeKeeper(5,
/// 0x40, 0x20)`).
const MAX_RESULTS: usize = 64;
const MAX_SUPPORT_RESULTS: usize = 32;
/// `PlayerCollider::moveCollide` gives up after this many rounds.
const MAX_ROUNDS: u32 = 100;
/// The depth the contacts start at, below any real one.
const NO_CONTACT: f32 = -100_000.;
/// A sweep step is never longer than this (`fmin` in `collide`).
const LONGEST_STEP: f32 = 35.;
/// The first `moveCollide` of `collide` (the carry) steps by 0.99 of the range.
const CARRY_STEP_SCALE: f32 = 0.99;
/// `al::isNearZero(vec, 0.001)`: a vector shorter than this is no direction.
const NEAR_ZERO: f32 = 0.001;
/// `rs::resetCollisionExpandCheck`: the bounding radius the shapes start growing from after a
/// reset (the literal 20 in the function).
const EXPAND_START_RADIUS: f32 = 20.;
/// The direction bits `calcResultVec` leaves in the collider's flags word
/// (`0x4338f0`): which way a round's pushes pointed on each axis.
const PUSH_POS_X: u32 = 0x80;
const PUSH_NEG_X: u32 = 0x100;
const PUSH_POS_Y: u32 = 0x200;
const PUSH_NEG_Y: u32 = 0x400;
const PUSH_POS_Z: u32 = 0x800;
const PUSH_NEG_Z: u32 = 0x1000;
/// How far above the ground plane a floor-like contact on the body sphere's
/// flank may be and still count as ground (`keeper._58`, `CollisionSmallStepHeight`).
const SMALL_STEP: f32 = 25.;
/// A flank contact closer than this to the ground plane is ignored.
const SAME_PLANE: f32 = 2.5;

fn slope_limit() -> f32 {
    (SLOPE_LIMIT_DEGREES * DEGREES_TO_RADIANS).cos()
}

/// The smallest `y` of a face normal that is still floor.
pub fn walkable_normal_y() -> f32 {
    slope_limit()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// `changeCollisionNormal`
    Normal,
    /// `changeCollisionMini`: the squat, the slope slide and the roll.
    Mini,
    /// `changeCollisionWallGrab`: the body hanging from a ledge.
    WallGrab,
    /// `changeCollisionPoleClimb`: the body clinging to a pole.
    Pole,
}

#[derive(Clone, Debug)]
enum Shape {
    /// Local centre, radius, and for a ground-support sphere how far from its
    /// axis a contact may be to count as directly underneath.
    Sphere {
        center: Vec3,
        radius: f32,
        support: Option<f32>,
        /// `createShapeSphereIgnoreGround`: floor faces do not push it.
        ignore_ground: bool,
    },
    /// Local start and vector, the margin below which a hit only supports
    /// (`CollisionShapeInfoArrow`'s `_54`) and the index among the arrows.
    Arrow {
        start: Vec3,
        vector: Vec3,
        margin: f32,
        index: usize,
    },
}
impl Shape {
    /// `getBoundingCenter` and `getBoundingRadius`: a sphere's own centre and radius, an arrow's
    /// midpoint and half length (`CollisionShapeInfoArrow` constructor).
    fn bounding(&self) -> (Vec3, f32) {
        match self {
            Self::Sphere { center, radius, .. } => (*center, *radius),
            Self::Arrow { start, vector, .. } => (*start + *vector * 0.5, vector.length() * 0.5),
        }
    }
    /// `getCheckStepRange`: an arrow's half length, a sphere's radius.
    fn check_range(&self) -> f32 {
        match self {
            Self::Sphere { radius, .. } => *radius,
            Self::Arrow { vector, .. } => vector.length() * 0.5,
        }
    }
}

/// `CollisionShapeKeeper`: the shapes of one form.
#[derive(Clone, Debug)]
pub struct Keeper {
    shapes: Vec<Shape>,
    check_step: f32,
    /// `CollisionShapeKeeper::_c`, the radius of the sphere around all the shapes
    /// (`updateShape`): what the shape scale is measured against.
    bounding_radius: f32,
    /// Whether the keeper has arrows (`CollisionShapeKeeper::mHasShapeArrow`):
    /// the player's has, Cappy's does not, and the floor rules differ.
    has_arrow: bool,
}
impl Keeper {
    pub fn new(form: Form) -> Self {
        let mut shapes = Vec::new();
        let support = Some(14.5);
        let sphere = |center: Vec3, support: Option<f32>, ignore_ground: bool| Shape::Sphere {
            center,
            radius: 55.,
            support,
            ignore_ground,
        };
        // Where the three arrows hang from the feet: the ledge form puts them
        // 140 below and 50 behind, the pole form 35 below and 50 behind
        // (`PlayerColliderHakoniwa::init`, the keepers `changeCollisionWallGrab`
        // and `changeCollisionPoleClimb` install).
        let mut arrows_at = Vec3::ZERO;
        match form {
            Form::Normal => {
                shapes.push(sphere(Vec3::new(0., 65., 0.), support, false));
                shapes.push(sphere(Vec3::new(0., 105., 0.), None, false));
            }
            Form::Mini => shapes.push(sphere(Vec3::new(0., 55., 0.), support, false)),
            Form::WallGrab => {
                // Body and head hang below and out from the wall, and ignore floors.
                shapes.push(sphere(Vec3::new(0., -75., -50.), None, true));
                shapes.push(sphere(Vec3::new(0., -35., -50.), None, true));
                arrows_at = Vec3::new(0., -140., -50.);
            }
            Form::Pole => {
                // Two spheres behind the body that floors do not push.
                shapes.push(sphere(Vec3::new(0., 30., -50.), None, true));
                shapes.push(sphere(Vec3::new(0., 60., -50.), None, true));
                arrows_at = Vec3::new(0., -35., -50.);
            }
        }
        // Three arrows (the helper called right after each keeper is made):
        // ring point at 120 degrees apart, 30 out, 40 up, pointing 60 down.
        for index in 0..3 {
            let angle = (index as f32 * 120.).to_radians();
            shapes.push(Shape::Arrow {
                start: Vec3::new(30. * angle.sin(), 40., 30. * angle.cos()) + arrows_at,
                vector: Vec3::new(0., -60., 0.),
                margin: 20.,
                index,
            });
        }
        // `updateShape`: the step range is the smallest range of any shape
        // (the bounding sphere is never smaller than one of them).
        let check_step = shapes
            .iter()
            .map(Shape::check_range)
            .fold(f32::MAX, f32::min);
        Self {
            bounding_radius: enclosing_radius(&shapes),
            shapes,
            check_step,
            has_arrow: true,
        }
    }

    /// `PlayerColliderHackCap::init`: Cappy is one plain sphere, "Body", of
    /// radius 60 centred 10 above the cap's position.
    pub fn cap() -> Self {
        let shapes = vec![Shape::Sphere {
            center: Vec3::new(0., 10., 0.),
            radius: 60.,
            support: None,
            ignore_ground: false,
        }];
        Self {
            check_step: 60.,
            bounding_radius: enclosing_radius(&shapes),
            shapes,
            has_arrow: false,
        }
    }
}

/// `CollisionShapeKeeper::updateShape`: the radius of the sphere that grows around the shapes
/// one at a time. A shape inside the sphere changes nothing; one that sticks out replaces it by
/// the sphere spanning the far end of the old sphere and the far end of the shape along the line
/// between their centres.
fn enclosing_radius(shapes: &[Shape]) -> f32 {
    let mut shapes = shapes.iter().map(Shape::bounding);
    let Some((mut center, mut radius)) = shapes.next() else {
        return 0.;
    };
    for (other, other_radius) in shapes {
        let offset = other - center;
        let distance = ((offset.x * offset.x + offset.y * offset.y) + offset.z * offset.z).sqrt();
        if distance.abs() < NEAR_ZERO {
            radius = radius.max(other_radius);
            continue;
        }
        if other_radius + distance <= radius {
            continue;
        }
        let toward = crate::prism::normalize(offset.to_array());
        let toward = Vec3::from_array(toward);
        let far_new = other + toward * other_radius;
        let far_old = center - toward * radius;
        let span = far_new - far_old;
        center = far_old + span * 0.5;
        radius = ((span.x * span.x + span.y * span.y) + span.z * span.z).sqrt() * 0.5;
    }
    radius
}

/// The body's pose: where its feet are and which way it faces (a rotation
/// about the up axis; the game's pose is the actor's full orientation).
#[derive(Clone, Copy, Debug)]
struct Pose {
    /// Where the feet are, in the stage's absolute coordinates.
    pos: Vec3,
    /// The facing the body had when the collider last ran, and the one it has
    /// now: a sweep turns from one to the other (`SpherePoseInterpolator`
    /// interpolates the orientation along with the position).
    from_yaw: f32,
    yaw: f32,
    /// The orientation as quaternions when either end of the sweep is tilted
    /// off the vertical (a run up a slope): the shapes hang from the whole
    /// pose, not only its facing.
    tilt: Option<Tilt>,
    /// The rotation of the pose the sweep has reached, row-major 3x3, built
    /// from its quaternion the way `PlayerCollider::findCollidePos` builds the
    /// matrix the shapes hang from.
    rotation: [f32; 9],
}
/// The orientations a sweep turns between.
#[derive(Clone, Copy, Debug)]
struct Tilt {
    from: Quat,
    to: Quat,
}
/// The rotation matrix `findCollidePos` makes from a unit quaternion (the
/// products `2*q*q` first, then sums and differences: for a turn about the
/// vertical alone the up column is exactly (0, 1, 0)).
fn rotation_of(q: Quat) -> [f32; 9] {
    let (x2, y2, z2, w2) = (q.x + q.x, q.y + q.y, q.z + q.z, q.w + q.w);
    let xy = q.y * x2;
    let yy = q.y * y2;
    let zz = q.z * z2;
    let xx = q.x * x2;
    let xz = q.z * x2;
    let yz = y2 * q.z;
    let zw = q.z * w2;
    let xw = q.x * w2;
    let yw = q.y * w2;
    let one_minus_yy = 1. - yy;
    let one_minus_xx = 1. - xx;
    [
        one_minus_yy - zz, // m00
        xy - zw,           // m01
        xz + yw,           // m02
        xy + zw,           // m10
        one_minus_xx - zz, // m11
        yz - xw,           // m12
        xz - yw,           // m20
        yz + xw,           // m21
        one_minus_xx - yy, // m22
    ]
}
impl Pose {
    fn new(pos: Vec3, from_yaw: f32, yaw: f32, tilt: Option<Tilt>) -> Self {
        let q = tilt.map_or_else(|| Quat::from_rotation_y(from_yaw), |t| t.from);
        Self {
            pos,
            from_yaw,
            yaw,
            tilt,
            rotation: rotation_of(q),
        }
    }
    /// The pose `sweep` has reached: its position and the interpolated facing.
    fn along(&self, sweep: &Sweep) -> Self {
        if let Some(tilt) = self.tilt {
            // `SpherePoseInterpolator::calcInterp`: the quaternions' spherical
            // blend, normalised.
            let now = crate::movement::plane::slerp(tilt.from, tilt.to, sweep.current).normalize();
            return Self {
                pos: sweep.pos(),
                rotation: rotation_of(now),
                ..*self
            };
        }
        let yaw = sweep.yaw(self.from_yaw, self.yaw);
        Self {
            pos: sweep.pos(),
            rotation: rotation_of(Quat::from_rotation_y(yaw)),
            ..*self
        }
    }
    /// `v` turned by the pose (`CollisionShapeInfo*::calcWorldShapeInfo`): each
    /// component `(x*m0 + y*m1) + z*m2`.
    fn rotate(&self, local: Vec3) -> Vec3 {
        let m = &self.rotation;
        Vec3::new(
            (local.x * m[0] + local.y * m[1]) + local.z * m[2],
            (local.x * m[3] + local.y * m[4]) + local.z * m[5],
            (local.x * m[6] + local.y * m[7]) + local.z * m[8],
        )
    }
    /// A point of the body in the stage's coordinates: the rotated offset added to the
    /// position, the last step of `calcWorldShapeInfo`.
    fn world_point(&self, local: Vec3) -> Vec3 {
        self.pos + self.rotate(local)
    }
}

/// `al::CollisionLocation`: where on the triangle a sphere touched it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    Face,
    /// 1..=3, the edge normals of the prism (1: first to third corner, 2:
    /// first to second, 3: second to third).
    Edge(u8),
    /// 1..=3, the corners.
    Corner(u8),
}

#[derive(Clone, Copy, Debug)]
pub struct SphereHit {
    /// How far the centre must move along the face normal to just touch.
    pub depth: f32,
    pub location: Location,
    /// The touched point of the triangle (stage coordinates).
    pub position: Vec3,
    pub center: Vec3,
}

/// Which of the prism's three edge planes a region is about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Plane {
    A,
    B,
    C,
}
impl Plane {
    fn index(self) -> usize {
        self as usize
    }
}

/// `KCHitSphereForPlayer` plus `calcSphereHitPos` for a triangle of the mesh: in the part's own
/// frame on the raw prism when the export has it (see `prism.rs`), otherwise on the world
/// triangle (`sphere_hit_world`).
fn sphere_hit(
    mesh: &TriangleMesh,
    t: &MeshTriangle,
    center: Vec3,
    radius: f32,
) -> Option<SphereHit> {
    let Some(local) = &t.local else {
        // A world-space triangle lives in the replay's frame.
        let hit = sphere_hit_world(t, mesh.from_game_vec(center), radius, mesh.thickness)?;
        return Some(SphereHit {
            position: mesh.to_game_vec(hit.position),
            center,
            ..hit
        });
    };
    let frame = &mesh.parts[local.part as usize];
    // `CollisionMultiShape::callbackFromServer`: the centre and the radius are carried into the
    // part's frame, the depth and the touched point back out.
    let center_local = transform_point(&frame.inverse, center.to_array());
    let (depth, region) = local.prism.sphere_test(
        center_local,
        radius * frame.inverse_scale,
        frame.inverse_scale,
    )?;
    let position = local.prism.sphere_hit_position(center_local, region);
    Some(SphereHit {
        depth: frame.world_per_local() * depth,
        location: match region {
            1 => Location::Face,
            2..=4 => Location::Edge(region - 1),
            _ => Location::Corner(region - 4),
        },
        position: Vec3::from_array(transform_point(&frame.base, position)),
        center,
    })
}

/// The same on a world-space triangle (older exports, the toy course and the tests).
///
/// The prism has three edge planes (normals in the face's plane); the signed
/// distances of the centre to them (`d`, positive outside) pick the region the
/// sphere touches. The plane the centre is furthest outside of leads: when
/// the centre is inside all three it is the face; otherwise the second
/// furthest decides between the leading edge alone (the centre is beside it)
/// and the corner where the two meet. This is the original's decision tree,
/// not the nearest point of the triangle: a thin triangle's corner is far away,
/// so a centre beside its long edge is not in the corner's region. An edge
/// only counts when the centre is no further outside it than above the face
/// (45 degrees), a corner when its distance is within that slope of 0.7071.
fn sphere_hit_world(
    t: &MeshTriangle,
    center: Vec3,
    radius: f32,
    thickness: f32,
) -> Option<SphereHit> {
    let from_first = center - t.corners[0];
    let d = [
        from_first.dot(t.edges[0]),
        from_first.dot(t.edges[1]),
        (center - t.corners[1]).dot(t.edges[2]),
    ];
    if d.iter().any(|v| *v >= radius) {
        return None;
    }
    let height = from_first.dot(t.normal);
    // The centre is beyond the radius: no contact on any region.
    let face_depth = radius - height;
    if face_depth < 0. {
        return None;
    }
    let projected = center - t.normal * height;
    // The leading plane, with the original's order of comparisons.
    let leading = if d[0] > d[1] {
        if d[0] > d[2] { Plane::A } else { Plane::C }
    } else if d[1] > d[2] {
        Plane::B
    } else {
        Plane::C
    };
    let lead = d[leading.index()];
    if lead <= 0. {
        // The face: a prism is only as thick as its header says.
        if thickness < face_depth {
            return None;
        }
        return Some(SphereHit {
            depth: face_depth,
            location: Location::Face,
            position: projected,
            center,
        });
    }
    let second = match leading {
        Plane::A => {
            if d[1] <= d[2] {
                Plane::C
            } else {
                Plane::B
            }
        }
        Plane::B => {
            if d[2] <= d[0] {
                Plane::A
            } else {
                Plane::C
            }
        }
        Plane::C => {
            if d[0] > d[1] {
                Plane::A
            } else {
                Plane::B
            }
        }
    };
    let cos = t.edges[leading.index()].dot(t.edges[second.index()]);
    let radius_sq = radius * radius;
    if lead * cos > d[second.index()] {
        // Beside the leading edge alone.
        if lead > height {
            return None;
        }
        let depth = (radius_sq - lead * lead).max(0.).sqrt() - height;
        if depth < 0. {
            return None;
        }
        // The point of the edge's line nearest the centre.
        let edge_normal = t.edges[leading.index()];
        let on_plane = if leading == Plane::C {
            t.corners[1]
        } else {
            t.corners[0]
        };
        let position = projected - edge_normal * edge_normal.dot(projected - on_plane);
        return Some(SphereHit {
            depth,
            location: Location::Edge(leading.index() as u8 + 1),
            position,
            center,
        });
    }
    // The corner where the two planes meet: the offset from it solves
    // `offset . e_p = d_p` and `offset . e_q = d_q` in the face's plane.
    let (p, q, corner) = match (leading, second) {
        (Plane::A, Plane::B) | (Plane::B, Plane::A) => (Plane::A, Plane::B, 0),
        (Plane::B, Plane::C) | (Plane::C, Plane::B) => (Plane::B, Plane::C, 1),
        _ => (Plane::C, Plane::A, 2),
    };
    let (ep, eq) = (t.edges[p.index()], t.edges[q.index()]);
    let c = ep.dot(eq);
    let along = (d[q.index()] * c - d[p.index()]) / (c * c - 1.);
    let offset = ep * along + eq * (d[q.index()] - c * along);
    let distance = offset.length();
    if distance >= radius || distance * EDGE_HIT_RATIO > height {
        return None;
    }
    let depth = (radius_sq - distance * distance).max(0.).sqrt() - height;
    if depth < 0. {
        return None;
    }
    Some(SphereHit {
        depth,
        location: Location::Corner(corner as u8 + 1),
        position: t.corners[corner],
        center,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct ArrowHit {
    /// The length of the arrow beyond the surface.
    pub depth: f32,
    /// Where it crosses the surface (stage coordinates).
    pub position: Vec3,
    /// How far along the arrow, from its start, the surface lies.
    pub distance: f32,
}

/// `KCHitArrow` for a triangle of the mesh as the player's own arrows run it
/// (`CollisionMultiShape::callbackFromServer`): in the part's frame on the raw prism when the
/// export has it, otherwise on the world triangle. The arrow's vector is carried by the
/// inverse's rotation alone, its start by the whole inverse.
fn arrow_hit(mesh: &TriangleMesh, t: &MeshTriangle, start: Vec3, vector: Vec3) -> Option<ArrowHit> {
    let Some(local) = &t.local else {
        return arrow_hit_world_game(mesh, t, start, vector);
    };
    let frame = &mesh.parts[local.part as usize];
    let start_local = transform_point(&frame.inverse, start.to_array());
    let vector_local = rotate_vector(&frame.inverse, vector.to_array());
    let fraction = local.prism.arrow_test(start_local, vector_local)?;
    Some(arrow_result(
        mesh,
        frame,
        start_local,
        vector_local,
        fraction,
        vector.length(),
    ))
}

/// `al::CollisionParts::checkStrikeArrow`, the query `alCollisionUtil` asks (ledge and pole
/// probes, border checks): the same prism test, but the vector is carried as the difference of
/// the carried end and start points.
fn strike_hit(mesh: &TriangleMesh, t: &MeshTriangle, start: Vec3, vector: Vec3) -> Option<ArrowHit> {
    let Some(local) = &t.local else {
        return arrow_hit_world_game(mesh, t, start, vector);
    };
    let frame = &mesh.parts[local.part as usize];
    let end = start + vector;
    let start_local = transform_point(&frame.inverse, start.to_array());
    let end_local = transform_point(&frame.inverse, end.to_array());
    let vector_local = [
        end_local[0] - start_local[0],
        end_local[1] - start_local[1],
        end_local[2] - start_local[2],
    ];
    let fraction = local.prism.arrow_test(start_local, vector_local)?;
    Some(arrow_result(
        mesh,
        frame,
        start_local,
        vector_local,
        fraction,
        vector.length(),
    ))
}

/// A world-space triangle's arrow test with the stage-coordinate start converted to and the
/// result back from the replay's frame.
fn arrow_hit_world_game(
    mesh: &TriangleMesh,
    t: &MeshTriangle,
    start: Vec3,
    vector: Vec3,
) -> Option<ArrowHit> {
    let hit = arrow_hit_world(t, mesh.from_game_vec(start), vector)?;
    Some(ArrowHit {
        position: mesh.to_game_vec(hit.position),
        ..hit
    })
}

/// The crossing found at `fraction` of an arrow of world length `length`: how far beyond the
/// surface the arrow reaches, how far along it lies and where (stage coordinates).
fn arrow_result(
    _mesh: &TriangleMesh,
    frame: &PartFrame,
    start_local: Point,
    vector_local: Point,
    fraction: f32,
    length: f32,
) -> ArrowHit {
    let at_local = [0, 1, 2].map(|i| start_local[i] + fraction * vector_local[i]);
    ArrowHit {
        depth: (1. - fraction) * length,
        position: Vec3::from_array(transform_point(&frame.base, at_local)),
        distance: length * fraction,
    }
}

/// `KCHitArrow`: the arrow starts in front of the face, ends behind it, and
/// the point where it crosses lies inside the prism's edges (with a 0.01 band).
///
/// The arrow's depth is `(1 - t) * length`; that product is rounded like the
/// game's: an arrow whose tip is 20 below a surface the feet stand on gets
/// `t = 40 / 60`, which in single precision is a hair above two thirds, so
/// the depth comes out a hair under the margin 20 and the surface only
/// supports. A fall has to go below the surface to collide.
fn arrow_hit_world(t: &MeshTriangle, start: Vec3, vector: Vec3) -> Option<ArrowHit> {
    let p = start - t.corners[0];
    let height = p.dot(t.normal);
    if height <= 0. {
        return None;
    }
    let along = vector.dot(t.normal);
    if height + along > 0. {
        return None;
    }
    let fraction = height / -along;
    let at = p + vector * fraction;
    let third = (t.corners[1] - t.corners[0]).dot(t.edges[2]);
    if at.dot(t.edges[0]) > ARROW_EDGE_BAND
        || at.dot(t.edges[1]) > ARROW_EDGE_BAND
        || at.dot(t.edges[2]) > third + ARROW_EDGE_BAND
    {
        return None;
    }
    let length = vector.length();
    let depth = (1. - fraction) * length;
    Some(ArrowHit {
        depth,
        position: start + vector * fraction,
        distance: length - depth,
    })
}

/// A face an arrow struck: the point, the face's normal and how far along.
#[derive(Clone, Copy, Debug)]
pub struct Strike {
    pub position: Vec3,
    pub normal: Vec3,
    /// How far along the arrow (from its start) the face lies, in cm.
    pub distance: f32,
    /// The face's index in the mesh (its collision codes are `codes_of`).
    pub triangle: u32,
}

/// The game's triangle filters for arrows (`al::TriangleFilterWallOnly`,
/// `TriangleFilterGroundOnly`, `TriangleFilterIgnoreGround`): which faces an
/// arrow may strike, by their class against gravity (straight down here).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceFilter {
    /// Every face.
    Any,
    /// Only walls (`isWallPolygon`).
    WallsOnly,
    /// Only floors (`isFloorPolygon`).
    FloorsOnly,
    /// Everything but floors.
    NotFloors,
}
impl FaceFilter {
    fn allows(self, normal: Vec3) -> bool {
        match self {
            Self::Any => true,
            Self::WallsOnly => classify(normal) == Class::Wall,
            Self::FloorsOnly => classify(normal) == Class::Floor,
            Self::NotFloors => classify(normal) != Class::Floor,
        }
    }
}

/// Every face the arrow from `start` along `vector` strikes (cm, replay frame),
/// in the mesh's order: `alCollisionUtil::checkStrikeArrow` asks whether there
/// is one, `getFirstPolyOnArrow` the nearest and `getLastPolyOnArrow` the
/// farthest. `walls_only` is `al::TriangleFilterWallOnly`: only faces that are
/// neither floor nor ceiling count.
pub fn arrow_strikes(
    mesh: &TriangleMesh,
    start: Vec3,
    vector: Vec3,
    walls_only: bool,
) -> Vec<Strike> {
    let filter = if walls_only {
        FaceFilter::WallsOnly
    } else {
        FaceFilter::Any
    };
    arrow_strikes_filtered(mesh, start, vector, filter)
}

/// [`arrow_strikes`] with any of the game's triangle filters.
pub fn arrow_strikes_filtered(
    mesh: &TriangleMesh,
    start: Vec3,
    vector: Vec3,
    filter: FaceFilter,
) -> Vec<Strike> {
    let mut strikes = arrow_strikes_game(mesh, mesh.to_game_vec(start), vector, filter);
    for strike in &mut strikes {
        strike.position = mesh.from_game_vec(strike.position);
    }
    strikes
}

/// [`arrow_strikes_filtered`] for a start in the stage's coordinates; the strikes' positions
/// are in the stage's coordinates too.
fn arrow_strikes_game(
    mesh: &TriangleMesh,
    start: Vec3,
    vector: Vec3,
    filter: FaceFilter,
) -> Vec<Strike> {
    let tip = start + vector;
    let area = Aabb {
        min: mesh.from_game_vec(start.min(tip)),
        max: mesh.from_game_vec(start.max(tip)),
    };
    mesh.candidates(area)
        .into_iter()
        .filter_map(|i| {
            let t = &mesh.triangles[i as usize];
            if !filter.allows(t.normal) {
                return None;
            }
            let hit = strike_hit(mesh, t, start, vector)?;
            Some(Strike {
                position: hit.position,
                normal: t.normal,
                distance: hit.distance,
                triangle: i,
            })
        })
        .collect()
}

/// `rs::calcExistCollisionBorder`: is there a face right behind the one the
/// arrow hit, turned the other way (the back of a thin wall)?
pub fn border_behind(mesh: &TriangleMesh, position: Vec3, normal: Vec3) -> bool {
    border_behind_game(mesh, mesh.to_game_vec(position), normal)
}

/// [`border_behind`] for a position in the stage's coordinates.
fn border_behind_game(mesh: &TriangleMesh, position: Vec3, normal: Vec3) -> bool {
    arrow_strikes_game(mesh, position - normal, normal * 2., FaceFilter::Any)
        .into_iter()
        .min_by(|a, b| a.distance.total_cmp(&b.distance))
        .is_some_and(|hit| hit.normal.dot(normal) <= -(0.01f32).cos())
}

/// The face `getFirstPolyOnArrow` returns: the nearest to the arrow's start.
pub fn first_strike(
    mesh: &TriangleMesh,
    start: Vec3,
    vector: Vec3,
    filter: FaceFilter,
) -> Option<Strike> {
    arrow_strikes_filtered(mesh, start, vector, filter)
        .into_iter()
        .min_by(|a, b| a.distance.total_cmp(&b.distance))
}

/// Every face a sphere at `center` touches (`alCollisionUtil::checkStrikeSphere`,
/// the same prism test the player's own spheres use): the face's index in the
/// mesh and the touched point on it.
pub fn sphere_strikes(mesh: &TriangleMesh, center: Vec3, radius: f32) -> Vec<(u32, Vec3)> {
    let center_game = mesh.to_game_vec(center);
    mesh.candidates(Aabb::around(center, radius))
        .into_iter()
        .filter_map(|i| {
            let t = &mesh.triangles[i as usize];
            let hit = sphere_hit(mesh, t, center_game, radius)?;
            Some((i, mesh.from_game_vec(hit.position)))
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
enum Hit {
    Sphere(SphereHit),
    Arrow(ArrowHit),
}
#[derive(Clone, Copy, Debug)]
struct ShapeResult {
    shape: usize,
    triangle: u32,
    hit: Hit,
}
#[derive(Default)]
struct Results {
    collide: Vec<ShapeResult>,
    support: Vec<ShapeResult>,
}

/// `CollisionMultiShape::check`: every shape of the keeper against the
/// triangles near it. An arrow whose depth is under its margin only supports
/// (its own list); everything else is a collision.
fn check(mesh: &TriangleMesh, keeper: &Keeper, pose: &Pose, scale: f32) -> Results {
    let mut results = Results::default();
    for (index, shape) in keeper.shapes.iter().enumerate() {
        match shape {
            Shape::Sphere { center, radius, .. } => {
                // The scale shrinks the offset from the feet and the radius alike
                // (`calcWorldShapeInfo`).
                let center = *center * scale;
                let radius = *radius * scale;
                // The centre as the game sums it: the position plus the rotated offset.
                let world = pose.world_point(center);
                let world_rel = mesh.from_game_vec(world);
                for i in mesh.candidates(Aabb::around(world_rel, radius)) {
                    let t = &mesh.triangles[i as usize];
                    let Some(hit) = sphere_hit(mesh, t, world, radius) else {
                        continue;
                    };
                    if results.collide.len() < MAX_RESULTS {
                        results.collide.push(ShapeResult {
                            shape: index,
                            triangle: i,
                            hit: Hit::Sphere(hit),
                        });
                    }
                }
            }
            Shape::Arrow {
                start,
                vector,
                margin,
                ..
            } => {
                // The start of the arrow is a world position that the game holds
                // in single precision at the stage's magnitude: a body standing
                // flush on the floor keeps the exact height 40 above it; the
                // depth then falls a hair under the margin and the arrow only
                // supports.
                let from = pose.world_point(*start * scale);
                let vec = pose.rotate(*vector * scale);
                let tip = from + vec;
                let area = Aabb {
                    min: mesh.from_game_vec(from.min(tip)),
                    max: mesh.from_game_vec(from.max(tip)),
                };
                for i in mesh.candidates(area) {
                    let t = &mesh.triangles[i as usize];
                    let Some(hit) = arrow_hit(mesh, t, from, vec) else {
                        continue;
                    };
                    let result = ShapeResult {
                        shape: index,
                        triangle: i,
                        hit: Hit::Arrow(hit),
                    };
                    if hit.depth >= *margin {
                        if results.collide.len() < MAX_RESULTS {
                            results.collide.push(result);
                        }
                    } else if results.support.len() < MAX_SUPPORT_RESULTS {
                        results.support.push(result);
                    }
                }
            }
        }
    }
    results
}

/// `al::SpherePoseInterpolator` reduced to the position and the shape scale: the move from
/// `start` by `motion` in equal steps of at most `max_step`, while the bounding radius of the
/// shapes grows from `radius_from` to `radius_to` (the shapes are scaled by the fraction of
/// the final radius reached; a body that was just reset starts small, see
/// `PlayerCollider::reset_expand_check`). The steps measure the length of the move plus the
/// growth of the radius.
struct Sweep {
    start: Vec3,
    motion: Vec3,
    step: f32,
    current: f32,
    previous: f32,
    radius_from: f32,
    radius_to: f32,
}
impl Sweep {
    fn new(start: Vec3, end: Vec3, max_step: f32, radius_from: f32, radius_to: f32) -> Self {
        let motion = end - start;
        let span = (motion.length() + radius_to) - radius_from;
        Self {
            start,
            motion,
            step: if span <= 0. { 1. } else { max_step / span },
            current: 0.,
            previous: 0.,
            radius_from,
            radius_to,
        }
    }
    /// The bounding radius the sweep has reached (`calcInterp`).
    fn radius(&self) -> f32 {
        self.radius_from + (self.radius_to - self.radius_from) * self.current
    }
    /// The scale the shapes are checked with: the fraction of the final radius reached
    /// (`calcRadiusBaseScale`, `al::calcRate01(radius, 0, radius_to)`), times the collider's own.
    fn shape_scale(&self, own: f32) -> f32 {
        let span = self.radius_to;
        let rate = if span.abs() < NEAR_ZERO {
            1.
        } else {
            (self.radius() / span).clamp(0., 1.)
        };
        rate * own
    }
    fn advance(&mut self) {
        self.previous = self.current;
        self.current = (self.current + self.step).min(1.);
    }
    fn pos(&self) -> Vec3 {
        self.start + self.motion * self.current
    }
    /// The facing along the sweep: the orientation interpolates from `from` to
    /// `to` (a spherical interpolation of two turns about the up axis takes
    /// the shorter way round).
    fn yaw(&self, from: f32, to: f32) -> f32 {
        let turn = (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        from + turn * self.current
    }
    fn remaining(&self) -> Vec3 {
        self.motion * (1. - self.current)
    }
    fn finished(&self) -> bool {
        self.previous == 1. && self.current == 1.
    }
}

/// A surface the body touched this frame: the deepest floor, wall and ceiling
/// contact, with what the game keeps of it (`HitInfo`).
#[derive(Clone, Debug)]
pub struct Contact {
    pub normal: Vec3,
    pub position: Vec3,
    /// Index of the triangle in the mesh (its collision codes and KCL index
    /// are there).
    pub triangle: u32,
    pub location: Location,
}

/// The collider's persistent state and the contacts of its last frame.
#[derive(Clone, Debug)]
pub struct PlayerCollider {
    pub form: Form,
    keeper: Keeper,
    pub floor: Option<Contact>,
    pub wall: Option<Contact>,
    pub ceiling: Option<Contact>,
    /// The average normal of the floor the arrows found (zero when none did),
    /// and the average of their contact points (`calcGroundArrowAverage`).
    pub ground_normal: Vec3,
    ground_position: Vec3,
    has_ground_normal: bool,
    has_ground_position: bool,
    best: [f32; 3],
    /// `onCutCollideAffectDir`: the collision's push along this direction (the
    /// hanging body's, into the wall) is dropped; what the move itself had
    /// along it stays. Zero when off.
    cut_direction: Vec3,
    /// Where the collider stood after its last move, when the actor has been
    /// put somewhere else since (the ledge hold sets the pose by hand): the
    /// next `collide` first carries it from there to the actor.
    carry_from: Option<Vec3>,
    /// The last `collide`'s fix reaction (`Outcome::fix`).
    fix_reaction: Vec3,
    /// The `PUSH_*` bits of the last `calcResultVec` round (`+0x108`).
    push_directions: u32,
    /// The deepest floor hit of each arrow so far in this `collide`: its
    /// depth, margin, contact point and face normal.
    arrow_slots: [Option<(f32, f32, Vec3, Vec3)>; 3],
    /// The facing at the end of the last `collide` (the collider keeps the
    /// actor's matrix of that time): the next sweeps turn from it.
    last_yaw: Option<f32>,
    /// The same orientation when it was tilted.
    last_tilt: Option<Quat>,
    /// The collider's own shape scale (`+0xf8`, `setCollisionShapeScale`), 1 unless a reset
    /// has it shrunk for a moment.
    shape_scale: f32,
    /// The bounding radius the shapes had at the end of the last `collide` (`+0x34`). When it
    /// differs from the current one (after a reset, or a change of form), the next `collide`
    /// first sweeps the shapes' growth from the old radius to the new one in place.
    last_radius: f32,
}
impl Default for PlayerCollider {
    fn default() -> Self {
        Self::new(Form::Normal)
    }
}

/// The three classes `calcResultVecSphere` sorts a face into.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    Floor,
    Wall,
    Ceiling,
}
fn classify(normal: Vec3) -> Class {
    // n . gravity with gravity pointing down is -n.y.
    let d = -normal.y;
    if d < 0. && d.abs() >= slope_limit() {
        Class::Floor
    } else if d.abs() < slope_limit() {
        Class::Wall
    } else {
        Class::Ceiling
    }
}

/// The first pass of `calcResultVec`: what kinds of faces are among the
/// results (a sphere counts only on a face, not on an edge or corner).
#[derive(Default)]
struct Flags {
    floor: bool,
    wall: bool,
    ceiling: bool,
    /// An arrow found floor.
    arrow_floor: bool,
}

/// Per axis, the most negative and most positive fix any contact asked for
/// (starting at zero); the body moves by their sum.
#[derive(Default)]
struct Fixes {
    low: Vec3,
    high: Vec3,
    /// Which ways the contributions pushed, as the `PUSH_*` bits.
    directions: u32,
}
impl Fixes {
    /// A contribution `fix`; `normal` stands in for its direction when it is
    /// too short to have one (`0x4338f0`).
    fn add(&mut self, fix: Vec3, normal: Vec3) {
        self.low = self.low.min(fix);
        self.high = self.high.max(fix);
        self.directions |= push_directions(fix, normal);
    }
    fn total(&self) -> Vec3 {
        self.low + self.high
    }
}

/// `0x4338f0`: the push direction bits of a fix. The direction is the fix
/// normalised (the normal when the fix is shorter than 0.001), with its
/// components under 0.05 dropped; each remaining component sets the bit of
/// its sign.
fn push_directions(fix: Vec3, normal: Vec3) -> u32 {
    let near_zero = |v: f32, limit: f32| v.abs() < limit;
    let direction = if fix.length_squared() < NEAR_ZERO * NEAR_ZERO {
        let cleaned = normal
            .to_array()
            .map(|c| if near_zero(c, 0.05) { 0. } else { c });
        Vec3::from_array(cleaned).normalize_or_zero()
    } else {
        let unit = fix / fix.length();
        Vec3::from_array(
            unit.to_array()
                .map(|c| if near_zero(c, 0.05) { 0. } else { c }),
        )
    };
    let mut bits = 0;
    for (component, (positive, negative)) in direction.to_array().into_iter().zip([
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

/// The last step of `calcResultVec`: a push along an axis that goes the way
/// the round before pushed back against the move (`previous` is the earlier
/// round's push less the move left, see `move_collide`) is averaged with it,
/// or replaced by it when this round has nothing pushing the other way.
fn average_with_previous(fix: Vec3, now: u32, previous: Vec3, before: u32) -> Vec3 {
    let mut result = fix.to_array();
    let before_axes = [
        (PUSH_POS_X, PUSH_NEG_X),
        (PUSH_POS_Y, PUSH_NEG_Y),
        (PUSH_POS_Z, PUSH_NEG_Z),
    ];
    for (axis, (positive, negative)) in before_axes.into_iter().enumerate() {
        let current = result[axis];
        let earlier = previous[axis];
        let faint = earlier.abs() < NEAR_ZERO;
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

/// The part of `v` across `axis` (`al::verticalizeVec`).
fn across(v: Vec3, axis: Vec3) -> Vec3 {
    v - axis * v.dot(axis)
}

/// `al::isNearDirection`: the vectors do not point apart and every component of
/// their cross product is within `limit`.
fn is_near_direction(a: Vec3, b: Vec3, limit: f32) -> bool {
    a.dot(b) >= 0. && a.cross(b).abs().max_element() <= limit
}

/// What a collide call found.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// Where the feet end up (cm, replay frame).
    pub pos: Vec3,
    /// A shape was in collision somewhere along the move (the position may
    /// still be the plain end of the move, when the contact asked for nothing).
    pub collided: bool,
}

impl PlayerCollider {
    pub fn new(form: Form) -> Self {
        Self::with_keeper(form, Keeper::new(form))
    }

    /// Cappy's collider (`PlayerColliderHackCap`).
    pub fn for_cap() -> Self {
        Self::with_keeper(Form::Normal, Keeper::cap())
    }

    fn with_keeper(form: Form, keeper: Keeper) -> Self {
        let last_radius = keeper.bounding_radius;
        Self {
            form,
            keeper,
            floor: None,
            wall: None,
            ceiling: None,
            ground_normal: Vec3::ZERO,
            ground_position: Vec3::ZERO,
            has_ground_normal: false,
            has_ground_position: false,
            best: [NO_CONTACT; 3],
            cut_direction: Vec3::ZERO,
            carry_from: None,
            fix_reaction: Vec3::ZERO,
            push_directions: 0,
            arrow_slots: [None; 3],
            last_yaw: None,
            last_tilt: None,
            shape_scale: 1.,
            last_radius,
        }
    }

    /// `PlayerColliderHakoniwa::changeCollisionNormal` / `changeCollisionMini`.
    pub fn set_form(&mut self, form: Form) {
        if form != self.form {
            self.form = form;
            self.keeper = Keeper::new(form);
            self.last_radius = self.keeper.bounding_radius * self.shape_scale;
        }
    }

    /// `onCutCollideAffectDir` / `offCutCollideAffectDir` (the ledge form
    /// turns it on along the direction the body faces, the other forms off).
    pub fn set_cut_direction(&mut self, direction: Vec3) {
        self.cut_direction = direction;
    }

    /// The collider stood at `pos` (cm) after its last move; the actor has
    /// been moved since, and the next `collide` carries the collider to it.
    pub fn carry_from(&mut self, pos: Vec3) {
        self.carry_from = Some(pos);
    }

    pub fn grounded(&self) -> bool {
        self.floor.is_some()
    }

    /// `rs::resetCollision` (`PlayerCollider::onInvalidate`): the contacts are
    /// forgotten and the collider takes the actor's pose as the one it stood
    /// in.
    pub fn invalidate(&mut self, yaw: f32) {
        self.floor = None;
        self.wall = None;
        self.ceiling = None;
        self.best = [NO_CONTACT; 3];
        self.carry_from = None;
        self.last_yaw = Some(yaw);
        self.last_tilt = None;
        self.last_radius = self.keeper.bounding_radius * self.shape_scale;
    }

    /// `rs::resetCollisionExpandCheck`: [`Self::invalidate`] in a way that makes the next
    /// `collide` grow the shapes in place. The shapes are shrunk to a bounding radius of 20 (or
    /// left alone when they are smaller), the reset records that size as the radius the
    /// collider last had, and the scale goes back; the next `collide` finds the radius changed
    /// and sweeps the growth from 20 to full size before the frame's own move, which then
    /// starts from its second pose.
    pub fn reset_expand_check(&mut self, yaw: f32) {
        let radius = self.keeper.bounding_radius * self.shape_scale;
        if radius.abs() < NEAR_ZERO || self.shape_scale.abs() < NEAR_ZERO {
            self.invalidate(yaw);
            return;
        }
        let ratio = radius / self.shape_scale;
        let shrunk = (ratio.min(EXPAND_START_RADIUS) / ratio).clamp(0., 1.);
        let own = self.shape_scale;
        self.set_shape_scale(shrunk);
        self.invalidate(yaw);
        self.set_shape_scale(own);
    }

    /// `PlayerCollider::setCollisionShapeScale`: the radius the collider last had can only
    /// shrink to the one the old scale gave.
    fn set_shape_scale(&mut self, scale: f32) {
        self.last_radius = self
            .last_radius
            .min(self.keeper.bounding_radius * self.shape_scale);
        self.shape_scale = scale;
    }

    /// `rs::getCollidedFixReaction`: how far the last `collide` moved the feet
    /// from where the actor's position plus the move would have put them (the
    /// carry from where the collider stood is not part of it).
    pub fn fix_reaction(&self) -> Vec3 {
        self.fix_reaction
    }

    /// `PlayerCollider::collide`: moves the feet from `pos` by `velocity`,
    /// both cm, facing `yaw` radians about the up axis (0 faces +z).
    pub fn collide(&mut self, mesh: &TriangleMesh, pos: Vec3, velocity: Vec3, yaw: f32) -> Outcome {
        self.collide_tilted(mesh, pos, velocity, yaw, None)
    }

    /// [`Self::collide`] for a body whose pose may be tilted off the vertical:
    /// `tilt` is the whole orientation then.
    pub fn collide_tilted(
        &mut self,
        mesh: &TriangleMesh,
        pos: Vec3,
        velocity: Vec3,
        yaw: f32,
        tilt: Option<Quat>,
    ) -> Outcome {
        self.floor = None;
        self.wall = None;
        self.ceiling = None;
        self.best = [NO_CONTACT; 3];
        let from_yaw = self.last_yaw.unwrap_or(yaw);
        let orientation = |yaw: f32, tilt: Option<Quat>| tilt.unwrap_or(Quat::from_rotation_y(yaw));
        let tilted = if tilt.is_some() || self.last_tilt.is_some() {
            let from = orientation(from_yaw, self.last_tilt);
            Some(Tilt {
                from,
                to: orientation(yaw, tilt),
            })
        } else {
            None
        };
        // Inside, positions are the game's own absolute ones.
        let pos = mesh.to_game_vec(pos);
        let debug = std::env::var("PJV_DBG").is_ok();
        if debug {
            eprintln!("DBG collide pos {pos:?} velocity {velocity:?} yaw {yaw} bits {:x} {:x}", pos.z.to_bits(), velocity.z.to_bits());
        }
        let pose = Pose::new(pos, from_yaw, yaw, tilted);
        self.last_yaw = Some(yaw);
        self.last_tilt = tilt;
        // `collide` forgets the last frame's push directions and the deepest
        // floor hit of each arrow.
        self.push_directions = 0;
        self.arrow_slots = [None; 3];
        let range = self.keeper.check_step;
        // The carry: from where the collider was to where the actor is. The
        // replay hands the collider the actor's own position, so the move is
        // empty, except for the states that put the actor on a pose by hand.
        // An empty carry is not swept at all; the move's own sweep then also
        // checks the pose it starts from.
        let begin = self
            .carry_from
            .take()
            .map_or(pose.pos, |from| mesh.to_game_vec(from));
        let mut at = begin;
        let carry = pose.pos - begin;
        // The bounding radius now, and the one the collider last had: when either the position
        // or the radius differs, the carry sweeps from the old state to the new.
        let radius_now = self.keeper.bounding_radius * self.shape_scale;
        let mut radius = self.last_radius;
        let carried = carry.length_squared() >= NEAR_ZERO * NEAR_ZERO
            || (radius - radius_now).abs() >= NEAR_ZERO;
        let mut collided = false;
        if carried {
            collided |= self.move_collide(
                mesh,
                &pose,
                &mut at,
                (&mut radius, radius_now),
                pose.pos,
                carry,
                (range * CARRY_STEP_SCALE).min(LONGEST_STEP),
                false,
            );
        }
        // The frame's own move.
        let target = at + velocity;
        collided |= self.move_collide(
            mesh,
            &pose,
            &mut at,
            (&mut radius, radius_now),
            target,
            velocity,
            range.min(LONGEST_STEP),
            carried,
        );
        self.last_radius = radius_now;
        // `onCutCollideAffectDir`: along the cut direction the move keeps what
        // it asked for, whatever the contacts pushed.
        if self.cut_direction != Vec3::ZERO {
            let moved = at - pose.pos;
            let cut = self.cut_direction;
            at = pose.pos + across(moved, cut) + cut * velocity.dot(cut);
        }
        let fix = at - (pose.pos + velocity);
        self.fix_reaction = fix;
        // The contacts' points go back to the replay's frame for the caller.
        for contact in [&mut self.floor, &mut self.wall, &mut self.ceiling]
            .into_iter()
            .flatten()
        {
            contact.position = mesh.from_game_vec(contact.position);
        }
        if debug {
            eprintln!("DBG   -> {at:?} collided {collided}");
        }
        Outcome {
            pos: mesh.from_game_vec(at),
            collided,
        }
    }

    /// `PlayerCollider::moveCollide`. `skip_start` is its last argument: the
    /// pose at the start of the sweep was just handled (by the carry), so the
    /// first pose checked is the next one.
    fn move_collide(
        &mut self,
        mesh: &TriangleMesh,
        pose: &Pose,
        at: &mut Vec3,
        (radius, radius_to): (&mut f32, f32),
        target: Vec3,
        intended: Vec3,
        step: f32,
        skip_start: bool,
    ) -> bool {
        let mut sweep = Sweep::new(*at, target, step, *radius, radius_to);
        if skip_start {
            sweep.advance();
        }
        let mut results = Results::default();
        if !self.find_collide_pos(mesh, pose, &mut sweep, &mut results) {
            *at = sweep.pos();
            *radius = sweep.radius();
            return false;
        }
        let mut rounds = 0;
        let mut previous_direction = Vec3::ZERO;
        // The third argument of `calcResultVec`: nothing in the first round.
        let mut previous = Vec3::ZERO;
        loop {
            *at = sweep.pos();
            *radius = sweep.radius();
            let mut remain = sweep.remaining();
            let (fix, unaveraged) =
                self.calc_result_vec(mesh, &pose.along(&sweep), &results, previous);
            if std::env::var("PJV_DBG").is_ok() {
                eprintln!("DBG   round at {:?} t {} fix {:?} results {:?}", *at, sweep.current, fix,
                    results.collide.iter().map(|r| match r.hit { Hit::Sphere(h) => ('S', r.shape, r.triangle, h.depth), Hit::Arrow(h) => ('A', r.shape, r.triangle, h.depth) }).collect::<Vec<_>>());
            }
            *at += fix;
            // A hit on the last pose: nothing is left to sweep.
            if sweep.current >= 1. {
                return true;
            }
            let length = fix.length();
            let mut direction = Vec3::ZERO;
            if length > 1e-7 {
                direction = fix / length;
                let along = direction.dot(remain);
                // Whatever of the move goes into the surface is dropped, and a
                // move that already leaves it is cut by the push itself.
                let removed = if along < 0. { along } else { along.min(length) };
                remain -= direction * removed;
            }
            // A remaining move against the intended one ends the sweep.
            let against = remain.dot(intended);
            if against < 0. && against.abs() >= NEAR_ZERO {
                return true;
            }
            if remain.length_squared() < NEAR_ZERO * NEAR_ZERO
                && previous_direction.dot(direction) < 0.
            {
                return true;
            }
            previous_direction = direction;
            // Sweep again from the pose that was in collision to where the
            // corrected body wants to go, skipping the pose already handled.
            let collided_at = *at - fix;
            sweep = Sweep::new(collided_at, *at + remain, step, *radius, radius_to);
            sweep.advance();
            // What the next round is told of this one: how far the pose it
            // starts from lies back from the one that was in collision, plus
            // the push this round asked for before averaging.
            previous = collided_at - sweep.pos() + unaveraged;
            results = Results::default();
            if !self.find_collide_pos(mesh, pose, &mut sweep, &mut results) {
                *at = sweep.pos();
                *radius = sweep.radius();
                return true;
            }
            rounds += 1;
            if rounds > MAX_ROUNDS - 1 {
                return true;
            }
        }
    }

    /// `PlayerCollider::findCollidePos`: advances to the first pose in
    /// collision; false when the whole move is free.
    fn find_collide_pos(
        &self,
        mesh: &TriangleMesh,
        pose: &Pose,
        sweep: &mut Sweep,
        results: &mut Results,
    ) -> bool {
        loop {
            if sweep.finished() {
                return false;
            }
            *results = check(
                mesh,
                &self.keeper,
                &pose.along(sweep),
                sweep.shape_scale(self.shape_scale),
            );
            if sweep.current == 0. && std::env::var("PJV_SKIP0").is_ok() {
                results.collide.retain(|r| match r.hit {
                    Hit::Sphere(h) => h.depth > 3e-4,
                    _ => true,
                });
            }
            if !results.collide.is_empty() {
                return true;
            }
            sweep.advance();
        }
    }

    /// `PlayerCollider::calcResultVec`, for static geometry: the single fix
    /// vector that resolves the results of one pose, and the bookkeeping of
    /// the deepest contacts. `previous` is what the round before told this
    /// one (see `move_collide`); the second result is the fix before it was
    /// averaged with that.
    ///
    /// Left out: the squeeze test of the `Press` collision code and the
    /// moving parts' separate accumulators.
    fn calc_result_vec(
        &mut self,
        mesh: &TriangleMesh,
        pose: &Pose,
        results: &Results,
        previous: Vec3,
    ) -> (Vec3, Vec3) {
        let flags = self.flags(mesh, results);
        self.average_ground(mesh, results);
        // The deepest floor of the earlier rounds provides the ground
        // reference the arrows did not.
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
        let mut fixes = Fixes::default();
        for result in &results.collide {
            let triangle = &mesh.triangles[result.triangle as usize];
            match result.hit {
                Hit::Arrow(hit) => {
                    if flags.arrow_floor {
                        self.arrow_fix(mesh, pose, result, &hit, &mut fixes);
                    }
                }
                Hit::Sphere(hit) => {
                    self.sphere_fix(mesh, triangle, result, &hit, &flags, &mut fixes);
                }
            }
        }
        let total = fixes.total();
        let fix = average_with_previous(total, fixes.directions, previous, self.push_directions);
        self.push_directions = fixes.directions;
        (fix, total)
    }

    fn flags(&self, mesh: &TriangleMesh, results: &Results) -> Flags {
        let mut flags = Flags::default();
        for result in &results.collide {
            let normal = mesh.triangles[result.triangle as usize].normal;
            match result.hit {
                Hit::Arrow(_) => {
                    if classify(normal) == Class::Floor {
                        flags.arrow_floor = true;
                    }
                }
                Hit::Sphere(hit) if hit.location == Location::Face => match classify(normal) {
                    Class::Floor => flags.floor = true,
                    Class::Wall => flags.wall = true,
                    Class::Ceiling => flags.ceiling = true,
                },
                Hit::Sphere(_) => {}
            }
        }
        flags
    }

    /// `calcGroundArrowAverage`: the floor normal is the average of the
    /// distinct normals under the arrows (collisions and supports alike), the
    /// ground position the average of the contact points of the arrows that
    /// collided. Each arrow remembers its deepest floor hit over every round
    /// of the `collide` (the slots are cleared only when it starts).
    fn average_ground(&mut self, mesh: &TriangleMesh, results: &Results) {
        self.ground_normal = Vec3::ZERO;
        self.ground_position = Vec3::ZERO;
        self.has_ground_normal = false;
        self.has_ground_position = false;
        let both = results.collide.iter().chain(&results.support);
        for result in both {
            let Hit::Arrow(hit) = result.hit else {
                continue;
            };
            let Shape::Arrow { index, margin, .. } = self.keeper.shapes[result.shape] else {
                continue;
            };
            let normal = mesh.triangles[result.triangle as usize].normal;
            if classify(normal) != Class::Floor {
                continue;
            }
            let slot = &mut self.arrow_slots[index];
            if slot.is_none_or(|held| held.0 < hit.depth) {
                *slot = Some((hit.depth, margin, hit.position, normal));
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
        if positions > 0 {
            self.ground_position = position / positions as f32;
            self.has_ground_position = true;
        }
        if normals > 0 {
            self.ground_normal = (normal / normals as f32).normalize_or_zero();
            self.has_ground_normal = true;
        }
    }

    /// `calcResultVecArrow`: an arrow that went in past its margin pushes the
    /// body up along the average floor normal by the part it went in.
    fn arrow_fix(
        &mut self,
        mesh: &TriangleMesh,
        pose: &Pose,
        result: &ShapeResult,
        hit: &ArrowHit,
        fixes: &mut Fixes,
    ) {
        let Shape::Arrow { margin, vector, .. } = self.keeper.shapes[result.shape] else {
            return;
        };
        let normal = mesh.triangles[result.triangle as usize].normal;
        if classify(normal) != Class::Floor {
            return;
        }
        let direction = pose.rotate(vector).normalize_or_zero();
        let depth = (hit.depth - margin).max(0.);
        let push = -depth * direction.dot(self.ground_normal);
        fixes.add(self.ground_normal * push, self.ground_normal);
        self.record(
            mesh,
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
        mesh: &TriangleMesh,
        triangle: &MeshTriangle,
        result: &ShapeResult,
        hit: &SphereHit,
        flags: &Flags,
        fixes: &mut Fixes,
    ) {
        let Shape::Sphere {
            support,
            ignore_ground,
            ..
        } = self.keeper.shapes[result.shape]
        else {
            return;
        };
        let normal = triangle.normal;
        let at_face = hit.location == Location::Face;
        if ignore_ground && classify(normal) == Class::Floor {
            return;
        }
        // The push along the face normal, and the radial one for an edge.
        let by_normal = normal * hit.depth;
        let radial = || {
            let to_centre = (hit.center - hit.position).normalize_or_zero();
            to_centre * (hit.depth * normal.dot(to_centre))
        };
        let class = classify(normal);
        // A wall hit with a face right behind it (the back of a thin wall) is
        // dropped: by every wall hit in the hanging forms, by those on an edge
        // or corner in the others (`setWallBorderCheckTypeAll` / `NoFace`).
        if class == Class::Wall
            && self.keeper.has_arrow
            && (!at_face || self.form_checks_faces())
            && border_behind_game(mesh, hit.position, normal)
        {
            return;
        }
        let fix = match class {
            Class::Floor if !self.keeper.has_arrow => {
                // No arrows: the sphere itself is the floor contact.
                if flags.floor {
                    by_normal
                } else {
                    radial_or_normal(at_face, by_normal, radial())
                }
            }
            Class::Floor => {
                // A floor-like face under a sphere that supports the body is
                // ground when the contact is directly underneath it, or on
                // its flank no higher than a small step above the ground
                // plane the arrows found. Any other floor-like contact (the
                // head's, or a step) can only push the body sideways.
                let underneath =
                    |range: f32| across(hit.position - hit.center, Vec3::Y).length() < range;
                let ground_like = support.is_some_and(|range| {
                    underneath(range)
                        || flags.arrow_floor
                            && SMALL_STEP - (hit.position - self.ground_position).y >= -1e-3
                });
                if ground_like {
                    by_normal
                } else {
                    if flags.arrow_floor
                        && (hit.position - self.ground_position).dot(self.ground_normal)
                            < SAME_PLANE
                    {
                        return;
                    }
                    let to_centre = (hit.center - hit.position).normalize_or_zero();
                    let side = across(to_centre * by_normal.dot(to_centre), Vec3::Y);
                    if to_centre == Vec3::ZERO || side.normalize_or_zero() == Vec3::ZERO {
                        return;
                    }
                    fixes.add(side, side.normalize_or_zero());
                    return;
                }
            }
            Class::Wall => {
                if flags.wall {
                    by_normal
                } else {
                    let fix = radial_or_normal(at_face, by_normal, radial());
                    if at_face { fix } else { across(fix, Vec3::Y) }
                }
            }
            Class::Ceiling => {
                if flags.ceiling {
                    by_normal
                } else {
                    radial_or_normal(at_face, by_normal, radial())
                }
            }
        };
        fixes.add(fix, normal);
        self.record(mesh, class, hit.depth, result, hit.position, hit.location);
    }

    /// `setWallBorderCheckTypeAll`: the hanging forms check the faces' hits too.
    fn form_checks_faces(&self) -> bool {
        matches!(self.form, Form::WallGrab | Form::Pole)
    }

    /// Keeps the deepest contact of each class.
    fn record(
        &mut self,
        mesh: &TriangleMesh,
        class: Class,
        depth: f32,
        result: &ShapeResult,
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
                normal: mesh.triangles[result.triangle as usize].normal,
                position,
                triangle: result.triangle,
                location,
            });
        }
    }
}

/// `calcFixVector`: the push along the face normal on a face, the radial one
/// on an edge or corner.
fn radial_or_normal(at_face: bool, by_normal: Vec3, radial: Vec3) -> Vec3 {
    if at_face { by_normal } else { radial }
}

#[path = "tests/collider.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
