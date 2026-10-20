//! The game's collision arithmetic in a collision part's own frame, single precision.
//!
//! The 1.0.0 executable does not test a body against world-space triangles. A stage object's
//! collision (`al::CollisionParts`) keeps its KCL file untouched in model space together with a
//! 3x4 base matrix and the matrix's inverse; a query is carried into the part's frame with the
//! inverse (`CollisionShapeInfo*::calcRelativeShapeInfo`), the sphere and arrow tests run against
//! the raw prism records (`KCollisionServer::KCHitSphereForPlayer`, `KCHitArrow`), and the results
//! are carried back with the base matrix. Which side of a surface a body is on is decided at the
//! 1e-4 cm level in that arithmetic, so everything here performs exactly the executable's single
//! precision operations in the executable's order (a sum of three products is `(a*b + c*d) + e*f`,
//! never fused).
//!
//! Matrices are 12 floats, row-major 3x4 (`sead::Matrix34f`): `m[4*row + column]`, translation in
//! column 3. Points are absolute stage positions in single precision; the replay's start-relative
//! frame is converted at the boundary (`TriangleMesh::to_game`).
//!
//! Offsets in the comments are the executable's: `CollisionParts::resetAllMtx` 0x847828,
//! `makeEqualScale` 0x847bdc, `calcInvMtxScale` 0x847754, `sead::Matrix34CalcCommon<float>::inverse`
//! 0x34558, `KCHitSphereForPlayer` 0x8543c4, `KCHitArrow` 0x853d20, `calcSphereHitPos` 0x8518b4,
//! `KCollisionServer::calcPosLocal` 0x8525ac, `Triangle::fillData` 0x85076c.

pub type Matrix34 = [f32; 12];
pub type Point = [f32; 3];

/// `al::isNearZero(float, float)`'s limit for the scale factors.
const SCALE_EPSILON: f32 = 0.001;
/// The floor `calcPosLocal` puts under a corner's divisor (the literal 2^-23 at 0x187fe78).
const CORNER_DIVISOR_FLOOR: f32 = 1.192_092_9e-7;
/// What `KCHitArrow` allows a hit to lie outside an edge by (0x187fd64).
const ARROW_EDGE_BAND: f32 = 0.01;
/// `KCHitSphereForPlayer`: a corner counts only when its distance, scaled by this, is no more than
/// the height above the face (0x18808c8).
#[allow(clippy::approx_constant)] // the game's own 0.707106, not exactly one over root two
const CORNER_SLOPE: f32 = 0.707_106;

/// `m * point` with the translation: `((x*m0 + y*m1) + z*m2) + m3` per row.
pub fn transform_point(m: &Matrix34, p: Point) -> Point {
    let row =
        |r: usize| ((p[0] * m[4 * r] + p[1] * m[4 * r + 1]) + p[2] * m[4 * r + 2]) + m[4 * r + 3];
    [row(0), row(1), row(2)]
}

/// The 3x3 part of `m` applied to a vector (`sead::Vector3::setRotated`).
pub fn rotate_vector(m: &Matrix34, v: Point) -> Point {
    let row = |r: usize| (v[0] * m[4 * r] + v[1] * m[4 * r + 1]) + v[2] * m[4 * r + 2];
    [row(0), row(1), row(2)]
}

fn dot(a: Point, b: Point) -> f32 {
    (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]
}

fn length(v: Point) -> f32 {
    dot(v, v).sqrt()
}

/// `al::normalize`: divides by the length with one reciprocal, and leaves a zero vector alone.
pub fn normalize(v: Point) -> Point {
    let len = length(v);
    if len > 0. {
        let inv = 1. / len;
        [inv * v[0], inv * v[1], inv * v[2]]
    } else {
        v
    }
}

/// `sead::Matrix34CalcCommon<float>::inverse`: the cofactor formula in the executable's order.
pub fn invert(m: &Matrix34) -> Option<Matrix34> {
    let [m00, m01, m02, m03, m10, m11, m12, m13, m20, m21, m22, m23] = *m;
    let c00 = m11 * m22 - m21 * m12;
    let p = m12 * m20;
    let q = m22 * m10;
    let c01 = p - q;
    let c02 = m21 * m10 - m11 * m20;
    let det = m02 * c02 + (m00 * c00 + m01 * c01);
    if det == 0. {
        return None;
    }
    let d = 1. / det;
    let s23 = m22 * m01 - m21 * m02;
    let s19 = q - p;
    let s16 = m00 * m12 - m10 * m02;
    let s6 = m00 * m21 - m01 * m20;
    let s7 = m00 * m22 - m20 * m02;
    let s18 = m12 * m01 - m11 * m02;
    let s1 = m00 * m11 - m01 * m10;
    let i00 = c00 * d;
    let i01 = -(s23 * d);
    let i10 = -(s19 * d);
    let i11 = s7 * d;
    let i02 = s18 * d;
    let i12 = -(s16 * d);
    let i20 = c02 * d;
    let i21 = -(s6 * d);
    let i22 = s1 * d;
    let i03 = (m13 * (s23 * d) - i00 * m03) - i02 * m23;
    let i13 = (s19 * d * m03 - i11 * m13) + m23 * (s16 * d);
    let i23 = (m13 * (s6 * d) - i20 * m03) - i22 * m23;
    Some([i00, i01, i02, i03, i10, i11, i12, i13, i20, i21, i22, i23])
}

/// A collision part's frame: the base matrix, its inverse and the scale factors the tests use.
#[derive(Clone, Debug)]
pub struct PartFrame {
    pub base: Matrix34,
    pub inverse: Matrix34,
    /// `CollisionParts::mMtxScale` (+0x124): the mean length of the inverse's columns
    /// (`calcInvMtxScale`), the factor that carries a world length into the part's frame.
    pub inverse_scale: f32,
}

impl PartFrame {
    /// The frame of a part with base matrix `base`; `None` for a singular matrix.
    pub fn new(base: Matrix34) -> Option<Self> {
        let inverse = invert(&base)?;
        let column = |c: usize| {
            ((inverse[c] * inverse[c] + inverse[4 + c] * inverse[4 + c])
                + inverse[8 + c] * inverse[8 + c])
                .sqrt()
        };
        // `((x + y) + z) / 3`
        let inverse_scale = ((column(0) + column(1)) + column(2)) / 3.;
        Some(Self {
            base,
            inverse,
            inverse_scale,
        })
    }

    /// The factor that turns a depth found in the part's frame back into world units
    /// (`1 / mMtxScale`, zero when the scale is degenerate).
    pub fn world_per_local(&self) -> f32 {
        if self.inverse_scale.abs() < SCALE_EPSILON {
            0.
        } else {
            1. / self.inverse_scale
        }
    }
}

/// One KCL prism exactly as stored: a triangle extruded along its face normal by `length`.
/// `origin` is the first corner; the edge normals are in the order of the file (edge 1 from the
/// first corner to the third, edge 2 first to second, edge 3 second to third).
#[derive(Clone, Copy, Debug)]
pub struct Prism {
    pub length: f32,
    pub origin: Point,
    pub face: Point,
    pub edges: [Point; 3],
    /// `KCPrismHeader` thickness of the sub-model the prism belongs to.
    pub thickness: f32,
}

/// Which region of a prism a sphere touched, as `KCHitSphereForPlayer` reports it (the byte it
/// writes): 1 face, 2..=4 edge normals 1..=3, 5..=7 the corners where edges (1,2), (2,3) and
/// (3,1) meet, that is the first, second and third corner.
pub type SphereRegion = u8;

impl Prism {
    /// `KCollisionServer::calcPosLocal`: corner `index` (0..=2) in the part's frame.
    pub fn corner(&self, index: usize) -> Point {
        let [a, b, c] = self.edges;
        let cross = |u: Point, v: Point| {
            [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ]
        };
        let along = match index {
            0 => return self.origin,
            1 => cross(self.face, b),
            _ => cross(a, self.face),
        };
        let divisor = dot(along, c).max(CORNER_DIVISOR_FLOOR);
        let scale = self.length / divisor;
        [
            self.origin[0] + along[0] * scale,
            self.origin[1] + along[1] * scale,
            self.origin[2] + along[2] * scale,
        ]
    }

    /// `KCollisionServer::KCHitSphereForPlayer`: whether a sphere of `radius` at `center` (the
    /// part's frame) touches the prism, the depth to push it out along the face normal and the
    /// region it touches. `scale` is the part's `mMtxScale`, which the game multiplies into the
    /// thickness limit.
    pub fn sphere_test(
        &self,
        center: Point,
        radius: f32,
        scale: f32,
    ) -> Option<(f32, SphereRegion)> {
        let d = [
            center[0] - self.origin[0],
            center[1] - self.origin[1],
            center[2] - self.origin[2],
        ];
        let [a, b, c] = self.edges;
        let plane = [dot(d, a), dot(d, b), dot(d, c) - self.length];
        if plane.iter().any(|v| *v >= radius) {
            return None;
        }
        let height = dot(d, self.face);
        let face_depth = radius - height;
        if face_depth < 0. {
            return None;
        }
        let radius_sq = radius * radius;
        let thickness = self.thickness * scale;
        // The plane the centre is furthest outside of leads (the original's order of comparisons).
        let lead = if plane[0] > plane[1] {
            if plane[0] > plane[2] { 0 } else { 2 }
        } else if plane[1] > plane[2] {
            1
        } else {
            2
        };
        if plane[lead] <= 0. {
            // Over the face: only as deep as the prism is thick.
            return (face_depth <= thickness).then_some((face_depth, 1));
        }
        let second = match lead {
            0 => {
                if plane[1] <= plane[2] {
                    2
                } else {
                    1
                }
            }
            1 => {
                if plane[2] <= plane[0] {
                    0
                } else {
                    2
                }
            }
            _ => {
                if plane[0] > plane[1] {
                    0
                } else {
                    1
                }
            }
        };
        let normals = [a, b, c];
        let cosine = dot(normals[lead], normals[second]);
        let lead_distance = plane[lead];
        let (squared, region) = if lead_distance * cosine > plane[second] {
            // Beside the leading edge alone.
            if lead_distance > height {
                return None;
            }
            (radius_sq - lead_distance * lead_distance, 2 + lead as u8)
        } else {
            // The corner where the two planes meet: the offset from it solves
            // `offset . p = plane_p`, `offset . q = plane_q`; `p` is the first plane of the pair
            // in the order (1,2), (2,3), (3,1).
            let (p, q, corner_region) = match (lead, second) {
                (0, 1) | (1, 0) => (0, 1, 5),
                (1, 2) | (2, 1) => (1, 2, 6),
                _ => (2, 0, 7),
            };
            let cos = dot(normals[p], normals[q]);
            let along = (plane[q] * cos - plane[p]) / (cos * cos + -1.);
            let across = plane[q] - cos * along;
            let offset = [
                along * normals[p][0] + normals[q][0] * across,
                along * normals[p][1] + normals[q][1] * across,
                along * normals[p][2] + normals[q][2] * across,
            ];
            let distance_sq = dot(offset, offset);
            let distance = distance_sq.sqrt();
            // (A distance that is not a number fails the comparison too.)
            if distance.is_nan() || distance >= radius || distance * CORNER_SLOPE > height {
                return None;
            }
            (radius_sq - distance_sq, corner_region)
        };
        let depth = squared.max(0.).sqrt() - height;
        (depth >= 0.).then_some((depth, region))
    }

    /// `alKCollisionFunc::calcSphereHitPos`: the touched point of the prism, in the part's frame.
    pub fn sphere_hit_position(&self, center: Point, region: SphereRegion) -> Point {
        let corner = |index| self.corner(index);
        match region {
            5 => return corner(0),
            6 => return corner(1),
            7 => return corner(2),
            _ => {}
        }
        // The centre's projection onto the face's plane.
        let offset = |from: Point| {
            [
                center[0] - from[0],
                center[1] - from[1],
                center[2] - from[2],
            ]
        };
        let off_plane = dot(offset(self.origin), self.face);
        let mut at = [
            center[0] - self.face[0] * off_plane,
            center[1] - self.face[1] * off_plane,
            center[2] - self.face[2] * off_plane,
        ];
        if region >= 2 {
            // Onto the edge's line: the edge normal is the plane's normal and the plane passes
            // through the first corner (edges 1 and 2) or the second one (edge 3). The distance
            // to the plane is measured from the centre, not from the projection.
            let (normal, through) = match region {
                2 => (self.edges[0], self.origin),
                3 => (self.edges[1], self.origin),
                _ => (self.edges[2], corner(1)),
            };
            let t = dot(offset(through), normal);
            at = [
                at[0] - normal[0] * t,
                at[1] - normal[1] * t,
                at[2] - normal[2] * t,
            ];
        }
        at
    }

    /// `KCollisionServer::KCHitArrow`: where along the arrow (fraction `t` of its vector, in the
    /// part's frame) it crosses the prism, if it does: it starts in front of the face, ends
    /// behind it, and crosses inside the three edge planes (with a 0.01 band).
    pub fn arrow_test(&self, start: Point, vector: Point) -> Option<f32> {
        let d = [
            start[0] - self.origin[0],
            start[1] - self.origin[1],
            start[2] - self.origin[2],
        ];
        let height = dot(d, self.face);
        if height <= 0. {
            return None;
        }
        let along = dot(self.face, vector);
        if height + along > 0. {
            return None;
        }
        let t = height / -along;
        let at = [
            d[0] + vector[0] * t,
            d[1] + vector[1] * t,
            d[2] + vector[2] * t,
        ];
        let [a, b, c] = self.edges;
        if dot(at, a) > ARROW_EDGE_BAND || dot(at, b) > ARROW_EDGE_BAND {
            return None;
        }
        // Edge 3's plane lies `length` along the first corner's direction.
        (self.length + ARROW_EDGE_BAND >= dot(at, c)).then_some(t)
    }
}

/// A part's triangle as `Triangle::fillData` hands it to the rest of the game: the normals
/// carried to world space and normalized, the corners carried with the base matrix.
#[derive(Clone, Copy, Debug)]
pub struct WorldTriangle {
    pub corners: [Point; 3],
    pub normal: Point,
    pub edges: [Point; 3],
}

impl Prism {
    pub fn world_triangle(&self, frame: &PartFrame) -> WorldTriangle {
        let rotated = |v: Point| normalize(rotate_vector(&frame.base, v));
        WorldTriangle {
            corners: [0, 1, 2].map(|i| transform_point(&frame.base, self.corner(i))),
            normal: rotated(self.face),
            edges: self.edges.map(rotated),
        }
    }
}

#[path = "tests/prism.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
