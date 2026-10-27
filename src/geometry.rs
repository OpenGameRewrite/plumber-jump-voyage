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
//!
//! A newer export also carries what the game itself tests against: every
//! placed object's base matrix and the raw prisms of its KCL (`prism.rs`).
//! Triangles with such a prism are tested in their part's own single
//! precision frame; their world corners, normals and edge normals are
//! derived the way `Triangle::fillData` derives them. Older exports (world
//! triangles only) keep working through the world-space tests.
use crate::prism::{PartFrame, Point, Prism};
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
    /// The raw prism and the part it lives in, when the export has them.
    pub local: Option<LocalPrism>,
}

/// A triangle's raw KCL prism and the index (into `TriangleMesh::parts`) of the part whose
/// frame it is expressed in.
#[derive(Clone, Copy, Debug)]
pub struct LocalPrism {
    pub part: u32,
    pub prism: Prism,
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
    /// The collision parts of the objects the local prisms belong to.
    pub parts: Vec<PartFrame>,
    /// The replay's start in the stage's own (absolute) coordinates, cm. The
    /// game keeps every position in single precision at that magnitude, whose
    /// steps are far coarser than the replay's start-relative numbers.
    origin: [f64; 3],
    cells: HashMap<(i32, i32), Vec<u32>>,
    /// The stage's `WaterArea`s, in the stage's own coordinates (the export's `areas`).
    pub water: crate::water::WaterAreas,
    pub death: crate::water::WaterAreas,
    pub recovery: crate::water::WaterAreas,
    pub recovery_ban: crate::water::WaterAreas,
    pub recovery_invalidate: crate::water::WaterAreas,
    pub safety_hosts: Vec<bool>,
    pub temperature: Vec<(crate::water::WaterAreas, i32)>,
    /// Original PlayerList / checkpoint restart placements, absolute centimetres.
    pub restart_points: Vec<serde_json::Value>,
    /// The hit sensors of the stage's placed actors the head can look at (the export's
    /// `look_sensors`), in the replay's frame.
    pub look_sensors: Vec<crate::movement::look_at::LookSensor>,
    /// Settled, initially lit candlestands; positions stay in absolute stage centimetres.
    pub candle_fire: Vec<Vec3>,
}

/// `N` floats from a JSON array (`None` when absent, short or not numbers).
fn floats<const N: usize>(value: Option<&serde_json::Value>) -> Option<[f32; N]> {
    let list = value?.as_array()?;
    let mut out = [0.; N];
    if list.len() < N {
        return None;
    }
    for (slot, item) in out.iter_mut().zip(list) {
        *slot = item.as_f64()? as f32;
    }
    Some(out)
}

/// The export's `look_sensors`: `{host, name, type, radius, position: [x, y, z]}` in the stage's
/// own coordinates, moved to the replay's frame.
fn look_sensors_from_export(
    list: &serde_json::Value,
    origin: [f64; 3],
) -> Vec<crate::movement::look_at::LookSensor> {
    let Some(list) = list.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|item| {
            let position = floats::<3>(item.get("position"))?;
            Some(crate::movement::look_at::LookSensor {
                host: item.get("host")?.as_str()?.to_string(),
                name: item.get("name")?.as_str()?.to_string(),
                kind: item.get("type")?.as_u64()? as u32,
                position: Vec3::new(
                    (f64::from(position[0]) - origin[0]) as f32,
                    (f64::from(position[1]) - origin[1]) as f32,
                    (f64::from(position[2]) - origin[2]) as f32,
                ),
                stage_position: Vec3::from(position),
                radius: item.get("radius")?.as_f64()? as f32,
            })
        })
        .collect()
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
        let codes: Vec<CollisionCodes> = root["attributes"]
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
        if codes.is_empty() {
            return Err("geometry: the attributes table is empty".into());
        }
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
            origin,
            water: crate::water::WaterAreas::from_export(&root["areas"])
                .with_oceans(&root["ocean_waves"]),
            death: crate::water::WaterAreas::from_export_named(&root["areas"], "DeathArea"),
            recovery: crate::water::WaterAreas::from_export_named(&root["areas"], "RecoveryArea"),
            recovery_ban: crate::water::WaterAreas::from_export_named(
                &root["areas"],
                "RecoveryBanArea",
            ),
            recovery_invalidate: crate::water::WaterAreas::from_export_named(
                &root["areas"],
                "InvalidateRecoveryPosArea",
            ),
            safety_hosts: root["objects"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|o| o["ask_safety_point"].as_bool().unwrap_or(false))
                .collect(),
            temperature: root["areas"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|a| a["name"].as_str() == Some("TemperatureArea"))
                .map(|a| {
                    (
                        crate::water::WaterAreas::from_export_named(
                            &serde_json::json!([a]),
                            "TemperatureArea",
                        ),
                        a["args"]["Temperature"].as_i64().unwrap_or(0) as i32,
                    )
                })
                .collect(),
            restart_points: root["restart_points"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
            look_sensors: look_sensors_from_export(&root["look_sensors"], origin),
            candle_fire: root["candlestands"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|c| c["is_burn_out"] == false && c["settled"] == true)
                .filter_map(|c| floats::<3>(c.get("fire_position")).map(Vec3::from))
                .collect(),
            ..Self::default()
        };
        // Per object: the part frame of its base matrix (newer exports only).
        let mut object_part: Vec<Option<u32>> = Vec::new();
        for (i, object) in root["objects"].as_array().into_iter().flatten().enumerate() {
            let Some(base) = floats::<12>(object.get("matrix")) else {
                object_part.push(None);
                continue;
            };
            let frame = PartFrame::new(base)
                .ok_or_else(|| format!("geometry: object {i} has a singular matrix"))?;
            // The exporter writes the inverse it computed with the same arithmetic:
            // a difference means the two implementations disagree.
            if let Some(exported) = floats::<12>(object.get("inverse"))
                && exported != frame.inverse
            {
                return Err(format!(
                    "geometry: object {i}: the exported inverse matrix differs from the computed one"
                ));
            }
            object_part.push(Some(mesh.parts.len() as u32));
            mesh.parts.push(frame);
        }
        // The raw prisms, by "<object model>#<sub-model>": thickness and rows of
        // length, first corner, face normal, edge normals 1 to 3.
        let models = root.get("models").and_then(|m| m.as_object());
        let prism_of = |model: &str, sub: usize, slot: usize| -> Option<Prism> {
            let entry = models?.get(&format!("{model}#{sub}"))?;
            let row = floats::<16>(entry["prisms"].get(slot))?;
            let thickness = entry["thickness"].as_f64()? as f32;
            let at = |k: usize| [row[k], row[k + 1], row[k + 2]];
            Some(Prism {
                length: row[0],
                origin: at(1),
                face: at(4),
                edges: [at(7), at(10), at(13)],
                thickness,
            })
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
            let object = n[13] as usize;
            let local = match (object_part.get(object).copied().flatten(), n.get(17)) {
                (Some(part), Some(slot)) => {
                    let model = mesh.objects.get(object).map_or("", String::as_str);
                    let prism =
                        prism_of(model, n[16] as usize, *slot as usize).ok_or_else(|| {
                            format!("geometry: triangle {i}: no prism {model}#{} {slot}", n[16])
                        })?;
                    Some(LocalPrism { part, prism })
                }
                _ => None,
            };
            let (corners, normal, edges) = if let Some(local) = &local {
                // The game's own derivation of the triangle (`Triangle::fillData`).
                let world = local.prism.world_triangle(&mesh.parts[local.part as usize]);
                (
                    world.corners.map(|c| mesh.relative_point(c)),
                    Vec3::from_array(world.normal),
                    world.edges.map(Vec3::from_array),
                )
            } else {
                // The corners are single-precision absolute positions, as the stage
                // file holds them; subtract in double precision, the numbers are large.
                let corner = |k: usize| {
                    Vec3::new(
                        (f64::from(n[3 * k] as f32) - origin[0]) as f32,
                        (f64::from(n[3 * k + 1] as f32) - origin[1]) as f32,
                        (f64::from(n[3 * k + 2] as f32) - origin[2]) as f32,
                    )
                };
                let corners = [corner(0), corner(1), corner(2)];
                let normal = Vec3::new(n[9] as f32, n[10] as f32, n[11] as f32).normalize_or_zero();
                let [a, b, c] = corners;
                (
                    corners,
                    normal,
                    [
                        outward(a, c, b, normal),
                        outward(a, b, c, normal),
                        outward(b, c, a, normal),
                    ],
                )
            };
            if normal == Vec3::ZERO {
                continue;
            }
            mesh.add(MeshTriangle {
                corners,
                normal,
                edges,
                attribute: n[12] as u16,
                object: n[13] as u16,
                kcl_index: n[14] as u32,
                local,
            });
        }
        Ok(mesh)
    }

    /// A start-relative position as the game holds it: the single-precision
    /// absolute stage position nearest to it.
    pub fn to_game(&self, relative: Vec3) -> Point {
        [
            (f64::from(relative.x) + self.origin[0]) as f32,
            (f64::from(relative.y) + self.origin[1]) as f32,
            (f64::from(relative.z) + self.origin[2]) as f32,
        ]
    }

    /// [`Self::to_game`] as a vector (the collider works in the game's own coordinates).
    pub fn to_game_vec(&self, relative: Vec3) -> Vec3 {
        Vec3::from_array(self.to_game(relative))
    }

    /// [`Self::relative_point`] for a vector.
    pub fn relative_vec(&self, absolute: Vec3) -> Vec3 {
        self.relative_point(absolute.to_array())
    }

    /// The start-relative position of an absolute single-precision one.
    pub fn relative_point(&self, absolute: Point) -> Vec3 {
        Vec3::new(
            (f64::from(absolute[0]) - self.origin[0]) as f32,
            (f64::from(absolute[1]) - self.origin[1]) as f32,
            (f64::from(absolute[2]) - self.origin[2]) as f32,
        )
    }

    /// A start-relative position (cm) as the game would hold it: rounded to
    /// the single-precision value of its absolute position.
    pub fn snap_to_game_precision(&self, relative: Vec3) -> Vec3 {
        let snap = |value: f32, origin: f64| {
            let absolute = (f64::from(value) + origin) as f32;
            (f64::from(absolute) - origin) as f32
        };
        Vec3::new(
            snap(relative.x, self.origin[0]),
            snap(relative.y, self.origin[1]),
            snap(relative.z, self.origin[2]),
        )
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
        let coded: Vec<([Vec3; 3], u16)> = list.iter().map(|corners| (*corners, 0)).collect();
        Self::from_coded_corners(&coded, vec![CollisionCodes::default()])
    }

    /// The same with an attribute (an index into `codes`) for every triangle:
    /// the toy course's poles carry the pole codes.
    pub fn from_coded_corners(list: &[([Vec3; 3], u16)], codes: Vec<CollisionCodes>) -> Self {
        let mut mesh = Self {
            codes,
            objects: vec![String::new()],
            thickness: PRISM_THICKNESS,
            ..Self::default()
        };
        for (i, (corners, attribute)) in list.iter().enumerate() {
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
                attribute: *attribute,
                object: 0,
                kcl_index: i as u32,
                local: None,
            });
        }
        mesh
    }

    /// Indices of the triangles whose cells overlap `area`, each once and in
    /// ascending order (the order the export lists them in).
    pub fn candidates(&self, area: Aabb) -> Vec<u32> {
        let mut found = self.cell_triangles(area);
        found.retain(|&i| {
            let t = &self.triangles[i as usize];
            let lo = t.corners[0].min(t.corners[1]).min(t.corners[2]);
            let hi = t.corners[0].max(t.corners[1]).max(t.corners[2]);
            lo.cmple(area.max).all() && hi.cmpge(area.min).all()
        });
        found
    }

    /// Every triangle of the cells the box touches, in mesh order. The game's KCL server hands
    /// the prisms of the touched cells to the shape's own test without comparing boxes first,
    /// so a sphere that rests exactly on a face the world-frame numbers put a rounding step
    /// away still reaches the prism's exact test, which runs in the part's frame.
    pub fn cell_triangles(&self, area: Aabb) -> Vec<u32> {
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
