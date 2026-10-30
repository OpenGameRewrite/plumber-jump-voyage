//! Dimension ownership and movement surfaces (SMO 1.0.0).
//!
//! This is the keeper, not a second player. Pipe puppets, model switching,
//! collision-form switching and the player's gravity/rotation-centre correction
//! are separate consumers; none is implied by merely containing a point.
use super::pose_math::{
    cross, dot, normalize, quat_front_up, squared_length, try_normalize_or_zero,
};
use super::vector_lib::{parallelize_vec, verticalize_vec};
use crate::water::{Area, Shape, WaterAreas};
use bevy::prelude::{Quat, Vec3};

/// `MoveArea2D` predicates at 0x6148/0x6270/0x6384 load 0x187fc80.
/// `isNearZeroOrLess` 0x91f568 / `OrGreater` 0x91f544 use a strict abs test.
const NEAR_ZERO: f32 = f32::from_bits(0x3a83126f);

#[derive(Clone, Debug)]
pub struct MoveArea {
    pub id: String,
    pub volume: Area,
    pub surface_distance: f32,
    pub gravity_offset: f32,
    /// `AreaCylinderCenter`: type 1 attracts; default/other values repel
    /// (`MoveArea2D::init`, 0x5ffc).
    pub radial_gravity_outward: bool,
}

impl MoveArea {
    fn axis(&self, column: usize) -> Vec3 {
        let m = &self.volume.matrix;
        Vec3::new(m[column], m[column + 4], m[column + 8])
    }

    fn offset(&self, position: Vec3) -> Vec3 {
        let m = &self.volume.matrix;
        Vec3::new(m[3] - position.x, m[7] - position.y, m[11] - position.z)
    }

    /// `calcSnapPower` 0x60f0 and its cube/cylinder/disk branches.
    /// Failure leaves the caller's zero-initialized direction/power intact.
    pub fn snap(&self, position: Vec3, surface_distance: f32) -> Option<(Vec3, f32)> {
        let offset = self.offset(position);
        let (mut direction, mut power) = match self.volume.shape {
            Shape::CubeBase | Shape::CylinderCenter => {
                let column = if self.volume.shape == Shape::CubeBase {
                    2
                } else {
                    1
                };
                let axis = self.axis(column);
                let distance = dot(axis, offset);
                // 0x61f4/0x6430 selects distance only when > 0; unlike
                // f32::abs this preserves the original negative zero at zero.
                let magnitude = if distance > 0. { distance } else { -distance };
                if distance <= 0. || distance.abs() < NEAR_ZERO {
                    (-axis, magnitude - surface_distance)
                } else {
                    (axis, magnitude + surface_distance)
                }
            }
            Shape::CylinderBase => {
                let radial = verticalize_vec(self.axis(1), offset);
                let (direction, usable) = try_normalize_or_zero(radial);
                if !usable {
                    return None;
                }
                (direction, squared_length(radial).sqrt() - surface_distance)
            }
            _ => return None,
        };
        if !(power >= 0. || power.abs() < NEAR_ZERO) {
            direction = -direction;
            power = -power;
        }
        Some((direction, power))
    }

    /// `calcGravityDir` 0x64ac; the disk measures distance BEFORE projecting.
    pub fn gravity(&self, position: Vec3) -> Option<(Vec3, f32)> {
        match self.volume.shape {
            Shape::CubeBase | Shape::CylinderBase => Some((-self.axis(1), 1.)),
            Shape::CylinderCenter => {
                let offset = self.offset(position);
                let distance = squared_length(offset).sqrt();
                let (direction, usable) =
                    try_normalize_or_zero(verticalize_vec(self.axis(1), offset));
                usable.then_some((
                    if self.radial_gravity_outward {
                        -direction
                    } else {
                        direction
                    },
                    distance,
                ))
            }
            _ => None,
        }
    }

    /// `rs::calc2DAreaDistanceGravity` 0x54532c. Its numerator is the
    /// original distance, not the clamped/offset denominator. 4,000,000 is
    /// literal 0x1880724; the denominator floor 1 is at 0x545378.
    fn weighted_gravity(&self, position: Vec3) -> Option<Vec3> {
        let (direction, distance) = self.gravity(position)?;
        let denominator = (distance - self.gravity_offset).max(1.);
        let weight = distance * (4_000_000. / (denominator * denominator));
        Some(direction * weight)
    }
}

#[derive(Clone, Debug, Default)]
pub struct DimensionAreas {
    pub movement: Vec<MoveArea>,
    keep: WaterAreas,
}

impl DimensionAreas {
    /// A consumer of rs::tryFind2DAreaObj that currently supports only a
    /// single containing area must reject overlaps rather than replace the
    /// original single-area query with the keeper's weighted gravity sum.
    pub(crate) fn single_movement_at(&self, position: Vec3) -> Option<&MoveArea> {
        let selected = self.select(position);
        if selected.len() == 1 {
            Some(&self.movement[selected[0]])
        } else {
            None
        }
    }

    /// Original placement data only. Like the existing area loader, this
    /// represents static volumes; switch-controlled/moving areas need their
    /// own independently simulated activation/pose before they can qualify.
    pub fn from_export(table: &serde_json::Value) -> Self {
        let mut movement = Vec::new();
        for entry in table.as_array().into_iter().flatten() {
            if entry["name"].as_str() != Some("2DMoveArea") {
                continue;
            }
            let volumes = WaterAreas::from_export_named(&serde_json::json!([entry]), "2DMoveArea");
            let Some(volume) = volumes.into_areas().into_iter().next() else {
                continue;
            };
            // Unsupported shapes are invalidated by MoveArea2D::init.
            if !matches!(
                volume.shape,
                Shape::CubeBase | Shape::CylinderBase | Shape::CylinderCenter
            ) {
                continue;
            }
            let number = |name: &str| entry["args"][name].as_f64().unwrap_or(0.) as f32;
            movement.push(MoveArea {
                id: entry["id"].as_str().unwrap_or_default().to_owned(),
                volume,
                surface_distance: number("SurfaceDistance"),
                gravity_offset: number("GravityOffset"),
                radial_gravity_outward: entry["args"]["SnapGravityType"].as_i64() != Some(1),
            });
        }
        Self {
            movement,
            keep: WaterAreas::from_export_named(table, "2DKeepArea"),
        }
    }

    /// `get2DMoveAreas` 0x541820: an ignore raises the threshold by one,
    /// rejecting subsequent equal-priority areas, even after clearing the list.
    fn select(&self, position: Vec3) -> Vec<usize> {
        let mut priority = -1;
        let mut selected = Vec::new();
        for (index, area) in self.movement.iter().enumerate() {
            if !area.volume.contains(position) || area.volume.priority < priority {
                continue;
            }
            if area.volume.ignore {
                priority = area.volume.priority.wrapping_add(1);
                selected.clear();
            } else {
                if priority < area.volume.priority {
                    priority = area.volume.priority;
                    selected.clear();
                }
                // FixedPtrArray<AreaObj, 8>; a full list skips the append
                // (0x5418ec..0x5418f8), but still evaluates later priorities.
                if selected.len() < 8 {
                    selected.push(index);
                }
            }
        }
        selected
    }
}

/// `ActorDimensionKeeper` six flags, plus `In2DAreaMoveControl` current/last
/// nonempty area sets. Area indexes refer to the owning scene's placement list.
#[derive(Clone, Debug)]
pub struct DimensionKeeper {
    pub valid: bool,
    pub is_2d: bool,
    pub in_area: bool,
    pub keep_2d: bool,
    pub change_2d: bool,
    pub change_3d: bool,
    current: Vec<usize>,
    last: Vec<usize>,
}

impl Default for DimensionKeeper {
    /// Constructor 0x541644 starts valid. Mario explicitly invalidates it
    /// during initialization; that is a consumer decision, not this default.
    fn default() -> Self {
        Self {
            valid: true,
            is_2d: false,
            in_area: false,
            keep_2d: false,
            change_2d: false,
            change_3d: false,
            current: Vec::new(),
            last: Vec::new(),
        }
    }
}

impl DimensionKeeper {
    /// Pose part of 0x54229c (Mario) and 0x5425b0 (Dokan2D's
    /// parallel-front initialization). Query the lock after positional snap.
    pub fn snap_pose(
        &self,
        areas: &DimensionAreas,
        position: Vec3,
        up: Vec3,
        front: Vec3,
        parallel_front: bool,
    ) -> Quat {
        let (lock, _) = self.lock(areas, position);
        let (projected_up, usable) = try_normalize_or_zero(verticalize_vec(lock, up));
        let up = if usable {
            projected_up
        } else {
            // al::sign 0x920acc keeps zero, otherwise returns +/-1.
            let alignment = dot(lock, up);
            let sign = if alignment > 0. {
                1.
            } else if alignment < 0. {
                -1.
            } else {
                alignment
            };
            normalize(up * sign)
        };
        let projected = if parallel_front {
            parallelize_vec(lock, front)
        } else {
            verticalize_vec(lock, front)
        };
        let (front, usable) = try_normalize_or_zero(projected);
        let front = if usable {
            front
        } else if parallel_front {
            -lock
        } else {
            normalize(cross(up, lock))
        };
        quat_front_up(front, up)
    }

    pub fn diagnostic(&self, areas: &DimensionAreas) -> String {
        if areas.movement.is_empty() {
            return String::new();
        }
        let names = |indices: &[usize]| {
            indices
                .iter()
                .map(|&i| areas.movement[i].id.as_str())
                .collect::<Vec<_>>()
                .join("/")
        };
        format!(
            "flags={}/{}/{}/{}/{}/{};current={};last={}",
            u8::from(self.valid),
            u8::from(self.is_2d),
            u8::from(self.in_area),
            u8::from(self.keep_2d),
            u8::from(self.change_2d),
            u8::from(self.change_3d),
            names(&self.current),
            names(&self.last)
        )
    }

    pub fn invalidate(&mut self) {
        self.valid = false;
        if self.is_2d {
            self.is_2d = false;
            self.in_area = false;
            self.keep_2d = false;
            self.change_3d = true;
        }
    }

    /// `ActorDimensionKeeper::update` 0x5416cc; invalid keepers refresh
    /// containment only, preserving the movement-control history.
    pub fn update(&mut self, areas: &DimensionAreas, position: Vec3) {
        let selected = areas.select(position);
        self.in_area = !selected.is_empty();
        if !self.valid {
            self.change_3d = false;
            return;
        }
        self.current = selected;
        if self.in_area {
            self.last.clone_from(&self.current);
            self.keep_2d = false;
        }
        if areas.keep.volume_at(position).is_some() {
            self.keep_2d = true;
        }
        self.change_2d = (self.in_area || self.keep_2d) && !self.is_2d;
        self.change_3d = !self.in_area && self.is_2d && !self.keep_2d;
        if self.change_2d {
            self.is_2d = true;
        }
        if self.change_3d {
            self.is_2d = false;
        }
    }

    /// `calcGravityDir` 0x5a58, with its caller-supplied fallback.
    pub fn gravity(&self, areas: &DimensionAreas, position: Vec3, fallback: Vec3) -> Vec3 {
        let mut sum = Vec3::ZERO;
        for &index in &self.current {
            if let Some(gravity) = areas.movement[index].weighted_gravity(position) {
                sum += gravity;
            }
        }
        let (direction, usable) = try_normalize_or_zero(sum);
        if usable { direction } else { fallback }
    }

    /// `calcLockDir` 0x5b3c: minimum power, normalized SUM of directions.
    /// The original sentinel 100,000 is constructed at 0x5b54..0x5b70.
    pub fn lock(&self, areas: &DimensionAreas, position: Vec3) -> (Vec3, f32) {
        let mut sum = Vec3::ZERO;
        let mut distance: f32 = 100_000.;
        for &index in &self.current {
            let area = &areas.movement[index];
            let (direction, power) = area
                .snap(position, area.surface_distance)
                .unwrap_or((Vec3::ZERO, 0.));
            sum += direction;
            distance = if distance < power { distance } else { power };
        }
        let (direction, usable) = try_normalize_or_zero(sum);
        (if usable { direction } else { Vec3::Z }, distance)
    }

    /// Translation/velocity part of snap2D, 0x542170. The pose and the
    /// rotation-centre adjustment of 0x54271c must be performed by its caller.
    pub fn snap(
        &self,
        areas: &DimensionAreas,
        position: &mut Vec3,
        velocity: &mut Vec3,
        length: f32,
    ) {
        let (direction, distance) = self.lock(areas, *position);
        *velocity = verticalize_vec(direction, *velocity);
        // 0x5421f4 loads 0x187fc80 for al::isNearZero(length, .001).
        if length.abs() < NEAR_ZERO {
            return;
        }
        // Preserve FMIN's negative zero; Rust's min need not keep its sign.
        let negative = if distance <= 0. { distance } else { 0. };
        let positive = if distance < 0. {
            0.
        } else {
            let cap = length + length;
            if distance > cap { cap } else { distance }
        };
        *position = Vec3::new(
            direction.x * negative + (direction.x * positive + position.x),
            direction.y * negative + (direction.y * positive + position.y),
            direction.z * negative + (direction.z * positive + position.z),
        );
    }
}

#[path = "../tests/dimension.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
