//! ExtForceArea and the player's area-velocity accumulator (1.0.0).
use super::*;
use crate::water::{Area, WaterAreas};

#[derive(Clone, Debug)]
pub struct ForceArea {
    pub id: String,
    pub volume: Area,
    step: f32,
}

impl ForceArea {
    pub fn from_export(table: &serde_json::Value) -> Vec<Self> {
        table
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                if entry["name"] != "ExtForceArea"
                    || entry["links"].as_array().is_some_and(|v| !v.is_empty())
                {
                    return None;
                }
                let step = match entry["args"]["ForceType"].as_str()? {
                    "滝" => 2.,
                    "滝の入り口" => 0.25,
                    _ => return None,
                };
                let volume =
                    WaterAreas::from_export_named(&serde_json::json!([entry]), "ExtForceArea")
                        .into_areas()
                        .into_iter()
                        .next()?;
                Some(Self {
                    id: entry["id"].as_str()?.into(),
                    volume,
                    step,
                })
            })
            .collect()
    }

    /// calcExtForce: the prior accumulator has already received its drag.
    pub fn force(&self, p: Vec3, previous: Vec3, velocity: Vec3) -> Vec3 {
        if !self.volume.contains(p) {
            return Vec3::ZERO;
        }
        let m = self.volume.matrix;
        let up = Vec3::new(m[1], m[5], m[9]);
        let vertical = up * pose_math::dot(up, velocity);
        let horizontal = velocity - vertical;
        let prior_vertical = up * pose_math::dot(up, previous);
        let prior_horizontal = previous - prior_vertical;
        let speed = pose_math::dot(up, vertical);
        let prior_speed = pose_math::dot(up, prior_vertical);
        let change = if speed < -40. {
            -prior_speed
        } else {
            let current = speed + prior_speed;
            let next = if current < -40. {
                (current + self.step).min(-40.)
            } else {
                (current - self.step).max(-40.)
            };
            next - current
        };
        let (direction, valid) = pose_math::try_normalize_or_zero(horizontal);
        let force_h = if valid {
            let difference = vector_lib::length(horizontal) - vector_lib::length(prior_horizontal);
            direction * (difference.max(0.) * 0.98) - (prior_horizontal + horizontal)
        } else {
            -prior_horizontal
        };
        up * change + force_h
    }
}

impl Player {
    pub(super) fn update_area_velocity(&mut self, scene: &Course) {
        self.area_source.clear();
        let Some(mesh) = &scene.mesh else { return };
        if mesh.force_areas.is_empty() {
            return;
        }
        let drag = if self.is_on_ground_by(self.units_per_frame()) {
            0.95
        } else {
            0.955
        };
        self.area_velocity *= drag;
        let previous = self.area_velocity;
        let at = self
            .stage_translation
            .filter(|(p, _)| *p == self.pos)
            .map_or_else(|| mesh.to_game_vec(self.position_cm()), |(_, p)| p);
        let mut force = Vec3::ZERO;
        let mut inside = Vec::new();
        for area in &mesh.force_areas {
            if area.volume.contains(at) {
                force += area.force(at, self.area_velocity, self.units_per_frame());
                inside.push(area.id.as_str());
            }
        }
        self.area_velocity += force;
        if vector_lib::length(self.area_velocity) < 0.1 {
            self.area_velocity = Vec3::ZERO;
        }
        let bits = |v: Vec3| {
            v.to_array()
                .map(|x| format!("{:08x}", x.to_bits()))
                .join("/")
        };
        self.area_source = format!(
            "forcearea={};areaforce={};areavelocity={};areapos={};areaprior={};areainput={}",
            inside.join("|"),
            bits(force),
            bits(self.area_velocity),
            bits(at),
            bits(previous),
            bits(self.units_per_frame())
        );
    }
}

pub(super) fn self_check() {
    waterfall_membership_and_accumulation();
}

#[cfg_attr(test, test)]
fn waterfall_membership_and_accumulation() {
    let area = ForceArea {
        id: "test".into(),
        volume: Area {
            shape: crate::water::Shape::CylinderBase,
            matrix: [1., 0., 0., 0., 0., 1., 0., 550., 0., 0., 1., 0.],
            scale: Vec3::new(1.5, 3., 2.5),
            priority: -1,
            ignore: false,
        },
        step: 2.,
    };
    assert!(area.volume.contains(Vec3::Y * 2050.));
    assert!(
        !area
            .volume
            .contains(Vec3::Y * f32::from_bits(2050_f32.to_bits() + 1))
    );
    assert_eq!(
        area.force(Vec3::Y * 1000., Vec3::ZERO, Vec3::NEG_Y * 20.),
        Vec3::NEG_Y * 2.
    );
    assert_eq!(
        area.force(Vec3::Y * 1000., Vec3::NEG_Y * 4., Vec3::NEG_Y * 41.),
        Vec3::Y * 4.
    );
    assert_eq!(
        area.force(Vec3::Y * 3000., Vec3::NEG_Y * 4., Vec3::ZERO),
        Vec3::ZERO
    );
}
