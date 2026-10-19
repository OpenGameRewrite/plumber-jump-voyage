//! The player's collider against a triangle mesh, written from the 1.0.0
//! executable's `PlayerCollider`, `CollisionMultiShape`, `CollisionShapeKeeper`
//! and the KCL server's sphere and arrow tests (`KCHitSphereForPlayer`,
//! `KCHitArrow`). Only what the traces and the decompiled headers explain is
//! in here; the places where the original does something this port leaves out
//! are named in the comments, with the function that does it.
//!
//! Units are the game's (cm), in the replay's start-relative frame; the
//! caller converts. Everything is single precision on purpose: whether a fall
//! that ends exactly on a surface touches it depends on how the arrow test
//! rounds (see `arrow_hit`).
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
use bevy::prelude::Vec3;

/// `PlayerCollider::_1b0`, the slope limit in degrees: a face whose normal is
/// within 70 degrees of straight up is floor, within 70 of straight down is
/// ceiling, anything else a wall.
const SLOPE_LIMIT_DEGREES: f32 = 70.;
/// The decomp's `0.01745329...`, as the code multiplies it in single precision.
const DEGREES_TO_RADIANS: f32 = 0.017_453_292;
/// What `KCHitArrow` allows a hit to lie outside an edge by.
const ARROW_EDGE_BAND: f32 = 0.01;
/// `KCHitSphereForPlayer`: an edge or corner hit counts only when the sphere
/// centre is within about 55 degrees of the face normal (the game's literal
/// is 0.707106).
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
}

#[derive(Clone, Debug)]
enum Shape {
    /// Local centre, radius, and for a ground-support sphere how far from its
    /// axis a contact may be to count as directly underneath.
    Sphere {
        center: Vec3,
        radius: f32,
        support: Option<f32>,
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
    /// Whether the keeper has arrows (`CollisionShapeKeeper::mHasShapeArrow`):
    /// the player's has, Cappy's does not, and the floor rules differ.
    has_arrow: bool,
}
impl Keeper {
    pub fn new(form: Form) -> Self {
        let mut shapes = Vec::new();
        let support = Some(14.5);
        match form {
            Form::Normal => {
                shapes.push(Shape::Sphere {
                    center: Vec3::new(0., 65., 0.),
                    radius: 55.,
                    support,
                });
                shapes.push(Shape::Sphere {
                    center: Vec3::new(0., 105., 0.),
                    radius: 55.,
                    support: None,
                });
            }
            Form::Mini => shapes.push(Shape::Sphere {
                center: Vec3::new(0., 55., 0.),
                radius: 55.,
                support,
            }),
        }
        // Three arrows (the helper called right after each keeper is made):
        // ring point at 120 degrees apart, 30 out, 40 up, pointing 60 down.
        for index in 0..3 {
            let angle = (index as f32 * 120.).to_radians();
            shapes.push(Shape::Arrow {
                start: Vec3::new(30. * angle.sin(), 40., 30. * angle.cos()),
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
        }];
        Self {
            check_step: 60.,
            shapes,
            has_arrow: false,
        }
    }
}

/// The body's pose: where its feet are and which way it faces (a rotation
/// about the up axis; the game's pose is the actor's full orientation).
#[derive(Clone, Copy, Debug)]
struct Pose {
    pos: Vec3,
    sin: f32,
    cos: f32,
}
impl Pose {
    fn rotate(&self, local: Vec3) -> Vec3 {
        Vec3::new(
            local.x * self.cos + local.z * self.sin,
            local.y,
            -local.x * self.sin + local.z * self.cos,
        )
    }
    fn at(&self, pos: Vec3) -> Self {
        Self { pos, ..*self }
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
    /// The touched point of the triangle.
    pub position: Vec3,
    pub center: Vec3,
}

/// `KCHitSphereForPlayer` plus `calcSphereHitPos`.
fn sphere_hit(t: &MeshTriangle, center: Vec3, radius: f32, thickness: f32) -> Option<SphereHit> {
    let from_first = center - t.corners[0];
    let height = from_first.dot(t.normal);
    // The centre is beyond the radius: no contact on any region.
    if radius - height < 0. {
        return None;
    }
    let d = [
        from_first.dot(t.edges[0]),
        from_first.dot(t.edges[1]),
        (center - t.corners[1]).dot(t.edges[2]),
    ];
    if d.iter().any(|v| *v >= radius) {
        return None;
    }
    let projected = center - t.normal * height;
    if d.iter().all(|v| *v <= 0.) {
        // The face: the depth is the whole penetration, and a prism is only
        // as thick as its header says.
        let depth = radius - height;
        if depth > thickness {
            return None;
        }
        return Some(SphereHit {
            depth,
            location: Location::Face,
            position: projected,
            center,
        });
    }
    let (nearest, location) = nearest_on_triangle(t, projected);
    let lateral = (projected - nearest).length();
    if lateral >= radius || lateral * EDGE_HIT_RATIO > height {
        return None;
    }
    let depth = (radius * radius - lateral * lateral).max(0.).sqrt() - height;
    (depth >= 0.).then_some(SphereHit {
        depth,
        location,
        position: nearest,
        center,
    })
}

/// The point of the triangle nearest to `p` (which is in its plane) and the
/// feature it lies on.
fn nearest_on_triangle(t: &MeshTriangle, p: Vec3) -> (Vec3, Location) {
    let [a, b, c] = t.corners;
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0. && d2 <= 0. {
        return (a, Location::Corner(1));
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0. && d4 <= d3 {
        return (b, Location::Corner(2));
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0. && d1 >= 0. && d3 <= 0. {
        // First-to-second edge: the prism's edge 2.
        return (a + ab * (d1 / (d1 - d3)), Location::Edge(2));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0. && d5 <= d6 {
        return (c, Location::Corner(3));
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0. && d2 >= 0. && d6 <= 0. {
        // First-to-third edge: the prism's edge 1.
        return (a + ac * (d2 / (d2 - d6)), Location::Edge(1));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0. && d4 - d3 >= 0. && d5 - d6 >= 0. {
        // Second-to-third edge: the prism's edge 3.
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (b + (c - b) * w, Location::Edge(3));
    }
    (p, Location::Face)
}

#[derive(Clone, Copy, Debug)]
pub struct ArrowHit {
    /// The length of the arrow beyond the surface.
    pub depth: f32,
    pub position: Vec3,
}

/// `KCHitArrow`: the arrow starts in front of the face, ends behind it, and
/// the point where it crosses lies inside the prism's edges (with a 0.01 band).
///
/// The arrow's depth is `(1 - t) * length`; that product is rounded like the
/// game's: an arrow whose tip is 20 below a surface the feet stand on gets
/// `t = 40 / 60`, which in single precision is a hair above two thirds, so
/// the depth comes out a hair under the margin 20 and the surface only
/// supports. A fall has to go below the surface to collide.
fn arrow_hit(t: &MeshTriangle, start: Vec3, vector: Vec3) -> Option<ArrowHit> {
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
    Some(ArrowHit {
        depth: (1. - fraction) * length,
        position: start + vector * fraction,
    })
}

/// A face an arrow struck: the point, the face's normal and how far along.
#[derive(Clone, Copy, Debug)]
pub struct Strike {
    pub position: Vec3,
    pub normal: Vec3,
    /// How far along the arrow (from its start) the face lies, in cm.
    pub distance: f32,
}

/// Every face the arrow from `start` along `vector` strikes (cm, replay frame),
/// in the mesh's order: `alCollisionUtil::checkStrikeArrow` asks whether there
/// is one, `getHitPosAndNormalOnArrow` the nearest and `getLastPolyOnArrow` the
/// farthest. `walls_only` is `al::TriangleFilterWallOnly`: only faces that are
/// neither floor nor ceiling count.
pub fn arrow_strikes(
    mesh: &TriangleMesh,
    start: Vec3,
    vector: Vec3,
    walls_only: bool,
) -> Vec<Strike> {
    let tip = start + vector;
    let area = Aabb {
        min: start.min(tip),
        max: start.max(tip),
    };
    let length = vector.length();
    mesh.candidates(area)
        .into_iter()
        .filter_map(|i| {
            let t = &mesh.triangles[i as usize];
            if walls_only && classify(t.normal) != Class::Wall {
                return None;
            }
            let hit = arrow_hit(t, start, vector)?;
            Some(Strike {
                position: hit.position,
                normal: t.normal,
                distance: length - hit.depth,
            })
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
fn check(mesh: &TriangleMesh, keeper: &Keeper, pose: &Pose) -> Results {
    let mut results = Results::default();
    for (index, shape) in keeper.shapes.iter().enumerate() {
        match shape {
            Shape::Sphere { center, radius, .. } => {
                let world = pose.pos + pose.rotate(*center);
                for i in mesh.candidates(Aabb::around(world, *radius)) {
                    let t = &mesh.triangles[i as usize];
                    if let Some(hit) = sphere_hit(t, world, *radius, mesh.thickness)
                        && results.collide.len() < MAX_RESULTS
                    {
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
                let from = pose.pos + pose.rotate(*start);
                let vec = pose.rotate(*vector);
                let tip = from + vec;
                let area = Aabb {
                    min: from.min(tip),
                    max: from.max(tip),
                };
                for i in mesh.candidates(area) {
                    let t = &mesh.triangles[i as usize];
                    let Some(hit) = arrow_hit(t, from, vec) else {
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

/// `al::SpherePoseInterpolator` reduced to the position: the move from
/// `start` by `motion` in equal steps of at most `max_step`.
struct Sweep {
    start: Vec3,
    motion: Vec3,
    step: f32,
    current: f32,
    previous: f32,
}
impl Sweep {
    fn new(start: Vec3, end: Vec3, max_step: f32) -> Self {
        let motion = end - start;
        let length = motion.length();
        Self {
            start,
            motion,
            step: if length <= 0. { 1. } else { max_step / length },
            current: 0.,
            previous: 0.,
        }
    }
    fn advance(&mut self) {
        self.previous = self.current;
        self.current = (self.current + self.step).min(1.);
    }
    fn pos(&self) -> Vec3 {
        self.start + self.motion * self.current
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
}
impl Fixes {
    fn add(&mut self, fix: Vec3) {
        self.low = self.low.min(fix);
        self.high = self.high.max(fix);
    }
    fn total(&self) -> Vec3 {
        self.low + self.high
    }
}

/// The part of `v` across `axis` (`al::verticalizeVec`).
fn across(v: Vec3, axis: Vec3) -> Vec3 {
    v - axis * v.dot(axis)
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
        }
    }

    /// `PlayerColliderHakoniwa::changeCollisionNormal` / `changeCollisionMini`.
    pub fn set_form(&mut self, form: Form) {
        if form != self.form {
            self.form = form;
            self.keeper = Keeper::new(form);
        }
    }

    pub fn grounded(&self) -> bool {
        self.floor.is_some()
    }

    /// `PlayerCollider::collide`: moves the feet from `pos` by `velocity`,
    /// both cm, facing `yaw` radians about the up axis (0 faces +z).
    pub fn collide(&mut self, mesh: &TriangleMesh, pos: Vec3, velocity: Vec3, yaw: f32) -> Outcome {
        self.floor = None;
        self.wall = None;
        self.ceiling = None;
        self.best = [NO_CONTACT; 3];
        let (sin, cos) = yaw.sin_cos();
        let pose = Pose { pos, sin, cos };
        let range = self.keeper.check_step;
        // The carry: from where the collider was to where the actor is. The
        // replay hands the collider the actor's own position, so the move is
        // empty; it still resolves an overlap the body starts in.
        let mut at = pose.pos;
        let mut collided = self.move_collide(
            mesh,
            &pose,
            &mut at,
            pose.pos,
            Vec3::ZERO,
            (range * CARRY_STEP_SCALE).min(LONGEST_STEP),
        );
        // The frame's own move.
        let target = at + velocity;
        collided |= self.move_collide(
            mesh,
            &pose,
            &mut at,
            target,
            velocity,
            range.min(LONGEST_STEP),
        );
        Outcome { pos: at, collided }
    }

    /// `PlayerCollider::moveCollide`.
    fn move_collide(
        &mut self,
        mesh: &TriangleMesh,
        pose: &Pose,
        at: &mut Vec3,
        target: Vec3,
        intended: Vec3,
        step: f32,
    ) -> bool {
        let mut sweep = Sweep::new(*at, target, step);
        let mut results = Results::default();
        if !self.find_collide_pos(mesh, pose, &mut sweep, &mut results) {
            *at = sweep.pos();
            return false;
        }
        let mut rounds = 0;
        let mut previous_direction = Vec3::ZERO;
        loop {
            *at = sweep.pos();
            let mut remain = sweep.remaining();
            let fix = self.calc_result_vec(mesh, pose, &results);
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
            if against < 0. && against.abs() > 1e-3 {
                return true;
            }
            if remain.length() <= 1e-3 && previous_direction.dot(direction) < 0. {
                return true;
            }
            previous_direction = direction;
            // Sweep again from the pose that was in collision to where the
            // corrected body wants to go, skipping the pose already handled.
            sweep = Sweep::new(*at - fix, *at + remain, step);
            sweep.advance();
            results = Results::default();
            if !self.find_collide_pos(mesh, pose, &mut sweep, &mut results) {
                *at = sweep.pos();
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
            *results = check(mesh, &self.keeper, &pose.at(sweep.pos()));
            if !results.collide.is_empty() {
                return true;
            }
            sweep.advance();
        }
    }

    /// `PlayerCollider::calcResultVec`, for static geometry: the single fix
    /// vector that resolves the results of one pose, and the bookkeeping of
    /// the deepest contacts.
    ///
    /// Left out: the squeeze test of the `Press` collision code, the moving
    /// parts' separate accumulators, the direction flags of the previous round
    /// that average a fix with the carried one (`calcResultVec`, the
    /// `0x80..0x1000` flag bits) and the border check that drops a wall's edge
    /// hit when a thin wall's back face is there (`rs::calcExistCollisionBorder`).
    fn calc_result_vec(&mut self, mesh: &TriangleMesh, pose: &Pose, results: &Results) -> Vec3 {
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
        fixes.total()
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
    /// collided.
    fn average_ground(&mut self, mesh: &TriangleMesh, results: &Results) {
        self.ground_normal = Vec3::ZERO;
        self.ground_position = Vec3::ZERO;
        self.has_ground_normal = false;
        self.has_ground_position = false;
        // Per arrow: the deepest floor-like hit and its margin.
        let mut slots: [Option<(f32, f32, Vec3, Vec3)>; 3] = [None; 3];
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
            if slots[index].is_none_or(|slot| hit.depth > slot.0) {
                slots[index] = Some((hit.depth, margin, hit.position, normal));
            }
        }
        let mut position = Vec3::ZERO;
        let mut positions = 0;
        let mut normal = Vec3::ZERO;
        let mut normals = 0;
        let mut seen: Vec<Vec3> = Vec::new();
        for slot in slots.iter().flatten() {
            let (depth, margin, at, n) = *slot;
            if depth >= margin {
                position += at;
                positions += 1;
            }
            // Arrows on the same plane count once (`isNearDirection`, 0.01).
            if !seen.iter().any(|s| s.dot(n) >= (0.01f32).cos()) {
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
        fixes.add(self.ground_normal * push);
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
        let Shape::Sphere { support, .. } = self.keeper.shapes[result.shape] else {
            return;
        };
        let normal = triangle.normal;
        let at_face = hit.location == Location::Face;
        // The push along the face normal, and the radial one for an edge.
        let by_normal = normal * hit.depth;
        let radial = || {
            let to_centre = (hit.center - hit.position).normalize_or_zero();
            to_centre * (hit.depth * normal.dot(to_centre))
        };
        let class = classify(normal);
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
                    fixes.add(side);
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
        fixes.add(fix);
        self.record(mesh, class, hit.depth, result, hit.position, hit.location);
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
