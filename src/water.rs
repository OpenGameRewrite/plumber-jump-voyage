//! Water volumes and the water surface finder, as the 1.0.0 executable asks them.
//!
//! A stage places `WaterArea` objects (an area shape, a pose, a scale). The game asks them three
//! things: which one holds a point (`al::AreaObjGroup::getInVolumeAreaObj`: the highest priority,
//! the later one on a tie), whether a point counts as water (`al::isInWaterPos`: an area that is
//! not marked `IsIgnoreArea`) and where the surface is along an arrow (`al::calcFindWaterSurface`,
//! which shoots the arrow at the area's top face with `AreaShapeCube::checkArrowCollision`).
//! [`SurfaceFinder`] is `al::WaterSurfaceFinder`: the player's last answer to the third question.
//!
//! Positions are in the stage's own coordinates (cm, single precision), every operation one
//! single-precision operation in the executable's order. Not ported: the fluid surfaces of the
//! nature director (`FluidSurfaceHolder::calcIsInFluid`, a second source of water for points in no
//! area), the arrow test of the cylinder and sphere shapes (their areas hold points but never
//! report a surface), areas that follow a moving part, and the switch that validates an area.
use bevy::prelude::*;

/// The shapes of `al::AreaShapeFactory`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    CubeBase,
    CubeCenter,
    CubeTop,
    Sphere,
    CylinderBase,
    CylinderCenter,
    CylinderTop,
    Infinite,
}
impl Shape {
    pub fn from_model(name: &str) -> Option<Self> {
        Some(match name {
            "AreaCubeBase" => Self::CubeBase,
            "AreaCubeCenter" => Self::CubeCenter,
            "AreaCubeTop" => Self::CubeTop,
            "AreaSphere" => Self::Sphere,
            "AreaCylinder" => Self::CylinderBase,
            "AreaCylinderCenter" => Self::CylinderCenter,
            "AreaCylinderTop" => Self::CylinderTop,
            "AreaInfinite" => Self::Infinite,
            _ => return None,
        })
    }
    /// Local height of the bottom and the top of the shape (`calcBottom`/`calcTop`).
    fn span(self) -> (f32, f32) {
        match self {
            Self::CubeBase => (0., 1000.),
            Self::CubeTop => (-1000., 0.),
            Self::CylinderBase => (0., 500.),
            Self::CylinderTop => (-500., 0.),
            Self::CylinderCenter => (-250., 250.),
            _ => (-500., 500.),
        }
    }
}

/// One placed `WaterArea`.
#[derive(Clone, Debug)]
pub struct Area {
    pub shape: Shape,
    /// `al::tryGetMatrixTR`: rotation and translation, row-major 3x4 (`sead::Matrix34f`).
    pub matrix: [f32; 12],
    pub scale: Vec3,
    /// `Priority` of the placement (-1 without one).
    pub priority: i32,
    /// `IsIgnoreArea`: a volume that is no water for the player.
    pub ignore: bool,
}

/// `al::isNearZero` on a scale component (the shape refuses a flat scale).
fn flat(scale: f32) -> bool {
    scale.abs() <= 0.001
}

impl Area {
    /// `al::AreaShape::calcLocalPos`: the point in the shape's frame, per unit of scale. `None`
    /// for a scale the shape cannot divide by.
    fn local(&self, p: Vec3) -> Option<Vec3> {
        if flat(self.scale.x) || flat(self.scale.y) || flat(self.scale.z) {
            return None;
        }
        let m = &self.matrix;
        // `al::calcMtxLocalTrans`: the transposed rotation applied to the offset.
        let d = Vec3::new(p.x - m[3], p.y - m[7], p.z - m[11]);
        let x = (d.x * m[0] + d.y * m[4]) + d.z * m[8];
        let y = (d.x * m[1] + d.y * m[5]) + d.z * m[9];
        let z = (d.x * m[2] + d.y * m[6]) + d.z * m[10];
        Some(Vec3::new(x / self.scale.x, y / self.scale.y, z / self.scale.z))
    }

    /// `al::AreaShape::calcWorldPos`.
    fn world_pos(&self, local: Vec3) -> Vec3 {
        let v = Vec3::new(
            local.x * self.scale.x,
            local.y * self.scale.y,
            local.z * self.scale.z,
        );
        let m = &self.matrix;
        Vec3::new(
            m[3] + ((v.x * m[0] + v.y * m[1]) + v.z * m[2]),
            m[7] + ((v.x * m[4] + v.y * m[5]) + v.z * m[6]),
            m[11] + ((v.x * m[8] + v.y * m[9]) + v.z * m[10]),
        )
    }

    /// `al::AreaShape::calcWorldDir`: a local direction, scaled, rotated and made a unit vector.
    fn world_dir(&self, local: Vec3) -> Vec3 {
        let v = Vec3::new(
            local.x * self.scale.x,
            local.y * self.scale.y,
            local.z * self.scale.z,
        );
        let m = &self.matrix;
        let r = Vec3::new(
            (v.x * m[0] + v.y * m[1]) + v.z * m[2],
            (v.x * m[4] + v.y * m[5]) + v.z * m[6],
            (v.x * m[8] + v.y * m[9]) + v.z * m[10],
        );
        crate::movement::pose_math::try_normalize_or_zero(r).0
    }

    /// `al::AreaShape::isInVolume` of the shapes (`AreaShapes.cpp`).
    pub fn contains(&self, p: Vec3) -> bool {
        let (bottom, top) = self.shape.span();
        match self.shape {
            Shape::Infinite => true,
            Shape::CubeBase | Shape::CubeCenter | Shape::CubeTop => {
                let Some(l) = self.local(p) else { return false };
                !(l.y < bottom || top < l.y || l.x < -500. || 500. < l.x || l.z < -500. || 500. < l.z)
            }
            Shape::CylinderBase | Shape::CylinderCenter | Shape::CylinderTop => {
                let Some(l) = self.local(p) else { return false };
                !(l.y < bottom || top < l.y) && l.x * l.x + l.z * l.z <= 500. * 500.
            }
            Shape::Sphere => {
                let Some(l) = self.local(p) else { return false };
                l.length_squared() <= 500. * 500.
            }
        }
    }

    /// `AreaShapeCube::checkArrowCollision`: where the segment `from -> to` meets the cube's
    /// surface and the outward normal of the face it meets. A start inside the cube finds the
    /// face it leaves through, one outside the face it enters by; the faces are tried in the
    /// executable's order (height, x, z) and the first whose point lies inside the others'
    /// bounds wins. Cylinders and spheres report nothing.
    pub fn arrow(&self, from: Vec3, to: Vec3) -> Option<(Vec3, Vec3)> {
        if !matches!(
            self.shape,
            Shape::CubeBase | Shape::CubeCenter | Shape::CubeTop
        ) {
            return None;
        }
        let (bottom, top) = self.shape.span();
        // (If the shape refuses its scale the local points keep the unit vector the executable
        // initialised them with; no water is placed so.)
        let a = self.local(from)?;
        let b = self.local(to)?;
        let d = b - a;
        let inside = !(a.y < bottom
            || top < a.y
            || a.x < -500.
            || 500. < a.x
            || a.z > 500.
            || a.z < -500.);
        // The point of the segment at `t`, as the executable computes it: start + step * t.
        let at = |t: f32| Vec3::new(d.x * t + a.x, d.y * t + a.y, d.z * t + a.z);
        let in_range = |t: f32| t >= 0. && t <= 1.;
        let in_xz = |h: Vec3| h.x >= -500. && h.x <= 500. && h.z >= -500. && h.z <= 500.;
        let in_yz = |h: Vec3| h.z >= -500. && h.z <= 500. && h.y >= bottom && h.y <= top;
        let in_xy = |h: Vec3| h.x >= -500. && h.x <= 500. && h.y >= bottom && h.y <= top;
        let hit = |h: Vec3, normal: Vec3| Some((self.world_pos(h), self.world_dir(normal)));
        // Height faces. Leaving upward or entering downward meets the top, the reverse the bottom.
        let plane_y = if (d.y > 0.) == inside { top } else { bottom };
        if d.y != 0. {
            let t = (plane_y - a.y) / d.y;
            if in_range(t) {
                let h = at(t);
                if in_xz(h) {
                    let up = (plane_y == top) as i32 as f32 * 2. - 1.;
                    return hit(h, Vec3::new(0., up, 0.));
                }
            }
        }
        // The x faces: the +x plane is met going +x from inside or going -x from outside.
        if d.x != 0. {
            let plane = if (d.x > 0.) == inside { 500. } else { -500. };
            let t = (plane - a.x) / d.x;
            if in_range(t) {
                let h = at(t);
                if in_yz(h) {
                    return hit(h, Vec3::new(plane / 500., 0., 0.));
                }
            }
        }
        if d.z != 0. {
            let plane = if (d.z > 0.) == inside { 500. } else { -500. };
            let t = (plane - a.z) / d.z;
            if in_range(t) {
                let h = at(t);
                if in_xy(h) {
                    return hit(h, Vec3::new(0., 0., plane / 500.));
                }
            }
        }
        None
    }
}

/// What `al::WaterSurfaceFinder` holds: the answer of the player's last query.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SurfaceFinder {
    pub found: bool,
    /// Distance from the queried point to the surface along the finder's direction (positive:
    /// the surface lies that far above the player).
    pub distance: f32,
    pub position: Vec3,
    pub normal: Vec3,
}

/// A stage's `WaterArea`s, in the order the placement lists them.
#[derive(Clone, Debug, Default)]
pub struct WaterAreas {
    areas: Vec<Area>,
}

impl WaterAreas {
    /// The `WaterArea` entries of an export's `areas` table.
    pub fn from_export(table: &serde_json::Value) -> Self {
        let float = |v: &serde_json::Value| v.as_f64().unwrap_or(0.) as f32;
        let mut areas = Vec::new();
        for entry in table.as_array().into_iter().flatten() {
            if entry["name"].as_str() != Some("WaterArea") {
                continue;
            }
            let Some(shape) = entry["shape"].as_str().and_then(Shape::from_model) else {
                continue;
            };
            let Some(matrix) = entry["matrix"].as_array().filter(|m| m.len() == 12) else {
                continue;
            };
            let scale = entry["scale"]
                .as_array()
                .filter(|s| s.len() == 3)
                .map(|s| Vec3::new(float(&s[0]), float(&s[1]), float(&s[2])))
                .unwrap_or(Vec3::ONE);
            areas.push(Area {
                shape,
                matrix: std::array::from_fn(|i| float(&matrix[i])),
                scale,
                priority: entry["priority"].as_i64().unwrap_or(-1) as i32,
                ignore: entry["args"]["IsIgnoreArea"].as_bool().unwrap_or(false),
            });
        }
        Self { areas }
    }

    /// A box of water for the demo course: its surface at `top`, the given extent below it
    /// (`AreaCubeTop`, no rotation), in the replay-relative frame the course lives in.
    pub fn demo_box(center_x: f32, center_z: f32, half_x: f32, half_z: f32, top: f32, depth: f32) -> Self {
        Self {
            areas: vec![Area {
                shape: Shape::CubeTop,
                matrix: [1., 0., 0., center_x, 0., 1., 0., top, 0., 0., 1., center_z],
                scale: Vec3::new(half_x * 2. / 1000., depth / 1000., half_z * 2. / 1000.),
                priority: -1,
                ignore: false,
            }],
        }
    }

    /// `al::AreaObjGroup::getInVolumeAreaObj`.
    pub fn volume_at(&self, p: Vec3) -> Option<&Area> {
        let mut best: Option<&Area> = None;
        for area in &self.areas {
            if (best.is_none() || best.is_some_and(|b| b.priority <= area.priority))
                && area.contains(p)
            {
                best = Some(area);
            }
        }
        best
    }

    /// `al::isInWaterPos`.
    pub fn is_water(&self, p: Vec3) -> bool {
        self.volume_at(p).is_some_and(|area| !area.ignore)
    }

    /// `al::calcFindWaterSurface` for the "Water" kind: the arrow runs from `start + dir * dist`
    /// to `start - dir * dist`. It finds nothing when the upper end is itself in water; the area
    /// shot at is the one holding the start, else the one holding the lower end.
    pub fn find_surface(&self, start: Vec3, dir: Vec3, dist: f32) -> Option<(Vec3, Vec3)> {
        let upper = Vec3::new(
            dir.x * dist + start.x,
            dir.y * dist + start.y,
            dir.z * dist + start.z,
        );
        let lower = Vec3::new(
            start.x - dir.x * dist,
            start.y - dir.y * dist,
            start.z - dir.z * dist,
        );
        let usable = |p: Vec3| self.volume_at(p).filter(|area| !area.ignore);
        if usable(upper).is_none() {
            let target = usable(start).or_else(|| usable(lower));
            if let Some(area) = target
                && let Some(found) = area.arrow(upper, lower)
            {
                return Some(found);
            }
        }
        None
    }

    /// `al::WaterSurfaceFinder::update`: ask for the surface within `dist` of `position` along
    /// `up` (the player passes its up direction and 200).
    pub fn update_finder(&self, position: Vec3, up: Vec3, dist: f32) -> SurfaceFinder {
        match self.find_surface(position, up, dist) {
            Some((surface, normal)) => SurfaceFinder {
                found: true,
                // "requires this manual dot product calculation to match"
                distance: up.x * (surface.x - position.x)
                    + up.y * (surface.y - position.y)
                    + up.z * (surface.z - position.z),
                position: surface,
                normal,
            },
            None => SurfaceFinder::default(),
        }
    }
}
