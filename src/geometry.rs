//! The real game's stage collision as a triangle soup, for replays.
//!
//! The local export tool writes a stage's static collision as a JSON file of
//! triangles in game units (cm, y up) with the collision attributes of the
//! KCL prism each one comes from. This module loads such a file into the
//! replay's collision scene (`movement::Course`), shifted so that the replay's
//! start point is the origin like the traces are, and answers the queries the
//! player's collider asks (`collider.rs`): which triangles lie near a box, and
//! the floor under a point.
//!
//! The data is read from the user's own local export and is never part of the
//! repository; nothing here ships game data.
//!
//! Each triangle keeps what a KCL prism has: the face normal and the three
//! outward edge normals (the game's sphere and arrow tests are written
//! against them). Edge 1 is the edge from the first corner to the third, edge
//! 2 from the first to the second and edge 3 from the second to the third,
//! the way `kcl.py` builds the corners from a prism.
use bevy::prelude::Vec3;
use std::collections::HashMap;

/// Collision attribute of a prism: the codes the game resolves against
/// `SystemData/CollisionAttributeList` (names and numeric positions).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CollisionCodes {
    pub floor: String,
    pub material: String,
    pub wall: String,
    pub camera: String,
    pub floor_index: u16,
    pub material_index: u16,
    pub wall_index: u16,
    pub camera_index: u16,
}

#[derive(Clone, Debug)]
pub struct MeshTriangle {
    /// Corners, counter-clockwise seen from the face normal (cm).
    pub corners: [Vec3; 3],
    pub normal: Vec3,
    /// Outward unit normals, in the triangle's plane, of the edges 1, 2 and 3.
    pub edges: [Vec3; 3],
    /// Index into `TriangleMesh::codes`.
    pub attribute: u16,
    /// Index into `TriangleMesh::objects`.
    pub object: u16,
    /// The prism's own triangle index in its KCL (what the game reports for a hit).
    pub kcl_index: u32,
}

/// Axis-aligned box in cm.
#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}
impl Aabb {
    pub fn around(center: Vec3, radius: f32) -> Self {
        Self {
            min: center - Vec3::splat(radius),
            max: center + Vec3::splat(radius),
        }
    }
}

/// How far a KCL prism extends below its face (`KCPrismHeader` thickness, 40
/// in every file of the base game's stages that was checked). A sphere whose
/// centre is deeper than the sphere's radius minus this behind a face does
/// not touch it any more.
pub const PRISM_THICKNESS: f32 = 40.;

const CELL: f32 = 400.;

#[derive(Debug, Default)]
pub struct TriangleMesh {
    pub triangles: Vec<MeshTriangle>,
    pub codes: Vec<CollisionCodes>,
    /// Model name of every object the triangles come from.
    pub objects: Vec<String>,
    pub thickness: f32,
    cells: HashMap<(i32, i32), Vec<u32>>,
}

fn cell_of(v: f32) -> i32 {
    (v / CELL).floor() as i32
}

/// The outward edge normal of the edge a-b of a triangle whose third corner
/// is `other`: in the plane of `normal`, pointing away from `other`.
fn outward(a: Vec3, b: Vec3, other: Vec3, normal: Vec3) -> Vec3 {
    let mut out = (b - a).cross(normal).normalize_or_zero();
    if out.dot(other - a) > 0. {
        out = -out;
    }
    out
}

/// Whether (x, z) lies inside the triangle seen from above (edges included).
fn contains_xz(t: &MeshTriangle, x: f32, z: f32) -> bool {
    let [a, b, c] = t.corners;
    let side = |p: Vec3, q: Vec3| (q.x - p.x) * (z - p.z) - (q.z - p.z) * (x - p.x);
    let s = [side(a, b), side(b, c), side(c, a)];
    s.iter().all(|v| *v >= -1e-3) || s.iter().all(|v| *v <= 1e-3)
}

impl TriangleMesh {
    /// Builds the mesh from the export's JSON text. `origin` is the replay's
    /// start point in game units; every corner is stored relative to it.
    pub fn from_json(text: &str, origin: Vec3) -> Result<Self, String> {
        let root: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("geometry: {e}"))?;
        let text_of = |v: &serde_json::Value, key: &str| {
            v.get(key)
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string()
        };
        let index_of = |v: &serde_json::Value, key: &str| {
            v.get(key).and_then(|n| n.as_u64()).unwrap_or(0) as u16
        };
        let codes = root["attributes"]
            .as_array()
            .ok_or("geometry: no attributes table")?
            .iter()
            .map(|a| CollisionCodes {
                floor: text_of(a, "FloorCode"),
                material: text_of(a, "MaterialCode"),
                wall: text_of(a, "WallCode"),
                camera: text_of(a, "CameraCode"),
                floor_index: index_of(a, "FloorCodeIndex"),
                material_index: index_of(a, "MaterialCodeIndex"),
                wall_index: index_of(a, "WallCodeIndex"),
                camera_index: index_of(a, "CameraCodeIndex"),
            })
            .collect();
        let objects = root["objects"]
            .as_array()
            .map(|list| list.iter().map(|o| text_of(o, "model")).collect())
            .unwrap_or_default();
        let rows = root["triangles"]
            .as_array()
            .ok_or("geometry: no triangle list")?;
        let origin = [origin.x as f64, origin.y as f64, origin.z as f64];
        let mut mesh = Self {
            codes,
            objects,
            thickness: PRISM_THICKNESS,
            ..Self::default()
        };
        for (i, row) in rows.iter().enumerate() {
            let n: Vec<f64> = row
                .as_array()
                .ok_or_else(|| format!("geometry: triangle {i} is not a list"))?
                .iter()
                .map(|x| x.as_f64().unwrap_or(0.))
                .collect();
            if n.len() < 16 {
                return Err(format!("geometry: triangle {i} has {} fields", n.len()));
            }
            // Subtract in double precision: the stage's coordinates are large.
            let corner = |k: usize| {
                Vec3::new(
                    (n[3 * k] - origin[0]) as f32,
                    (n[3 * k + 1] - origin[1]) as f32,
                    (n[3 * k + 2] - origin[2]) as f32,
                )
            };
            let corners = [corner(0), corner(1), corner(2)];
            let normal = Vec3::new(n[9] as f32, n[10] as f32, n[11] as f32).normalize_or_zero();
            if normal == Vec3::ZERO {
                continue;
            }
            let [a, b, c] = corners;
            mesh.add(MeshTriangle {
                corners,
                normal,
                edges: [
                    outward(a, c, b, normal),
                    outward(a, b, c, normal),
                    outward(b, c, a, normal),
                ],
                attribute: n[12] as u16,
                object: n[13] as u16,
                kcl_index: n[14] as u32,
            });
        }
        Ok(mesh)
    }

    fn add(&mut self, triangle: MeshTriangle) {
        let index = self.triangles.len() as u32;
        let lo = triangle.corners[0]
            .min(triangle.corners[1])
            .min(triangle.corners[2]);
        let hi = triangle.corners[0]
            .max(triangle.corners[1])
            .max(triangle.corners[2]);
        for x in cell_of(lo.x)..=cell_of(hi.x) {
            for z in cell_of(lo.z)..=cell_of(hi.z) {
                self.cells.entry((x, z)).or_default().push(index);
            }
        }
        self.triangles.push(triangle);
    }

    /// Builds a mesh from triangles given directly (tests and the flat
    /// floor): `corners` counter-clockwise from the normal side.
    pub fn from_corners(list: &[[Vec3; 3]]) -> Self {
        let mut mesh = Self {
            codes: vec![CollisionCodes::default()],
            objects: vec![String::new()],
            thickness: PRISM_THICKNESS,
            ..Self::default()
        };
        for (i, corners) in list.iter().enumerate() {
            let [a, b, c] = *corners;
            let normal = (b - a).cross(c - a).normalize_or_zero();
            mesh.add(MeshTriangle {
                corners: *corners,
                normal,
                edges: [
                    outward(a, c, b, normal),
                    outward(a, b, c, normal),
                    outward(b, c, a, normal),
                ],
                attribute: 0,
                object: 0,
                kcl_index: i as u32,
            });
        }
        mesh
    }

    /// Indices of the triangles whose cells overlap `area`, each once and in
    /// ascending order (the order the export lists them in).
    pub fn candidates(&self, area: Aabb) -> Vec<u32> {
        let mut found = Vec::new();
        for x in cell_of(area.min.x)..=cell_of(area.max.x) {
            for z in cell_of(area.min.z)..=cell_of(area.max.z) {
                if let Some(list) = self.cells.get(&(x, z)) {
                    found.extend_from_slice(list);
                }
            }
        }
        found.sort_unstable();
        found.dedup();
        found.retain(|&i| {
            let t = &self.triangles[i as usize];
            let lo = t.corners[0].min(t.corners[1]).min(t.corners[2]);
            let hi = t.corners[0].max(t.corners[1]).max(t.corners[2]);
            lo.cmple(area.max).all() && hi.cmpge(area.min).all()
        });
        found
    }

    /// The model the triangle's object was built from (`CapWorldHomeGround000`).
    pub fn model_of(&self, triangle: u32) -> &str {
        let object = self.triangles[triangle as usize].object as usize;
        self.objects.get(object).map_or("", String::as_str)
    }

    pub fn codes_of(&self, triangle: u32) -> &CollisionCodes {
        &self.codes[self.triangles[triangle as usize].attribute as usize % self.codes.len()]
    }

    /// Height (cm) of the highest floor-like surface at or below `y_max` at
    /// the horizontal position (x, z): a vertical ray against the triangles.
    pub fn floor_below(&self, x: f32, z: f32, y_max: f32, min_up: f32) -> Option<f32> {
        let mut best: Option<f32> = None;
        // Anything under the probe, however far down.
        let column = Aabb {
            min: Vec3::new(x - 0.01, f32::MIN, z - 0.01),
            max: Vec3::new(x + 0.01, y_max, z + 0.01),
        };
        for i in self.candidates(column) {
            let t = &self.triangles[i as usize];
            if t.normal.y < min_up {
                continue;
            }
            if !contains_xz(t, x, z) {
                continue;
            }
            let y = t.corners[0].y
                - ((x - t.corners[0].x) * t.normal.x + (z - t.corners[0].z) * t.normal.z)
                    / t.normal.y;
            if y <= y_max && best.is_none_or(|b| y > b) {
                best = Some(y);
            }
        }
        best
    }
}

#[path = "tests/geometry.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
