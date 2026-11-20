//! `PlayerExternalVelocity` (1.0.0 0x440dec): the player's external-velocity accumulators. The
//! `ByugoBlow` one (`ExternalForceKeeper`, fed by `AirCurrent`, see `air_current.rs`) and the
//! ExtForceArea one (waterfalls) are two terms of the same update; the actor adds their total
//! to its velocity in `updateCollider` (0x420f24, 0x42104c..0x42105c).
use super::*;
use crate::water::{Area, WaterAreas};

/// `PlayerExternalVelocity` constructor 0x440b84: how much of a new force enters the velocity
/// (`+0x68` on the ground, `+0x6c` in the air) and how much of the old velocity stays (`+0x58`,
/// `+0x5c`).
const SENSITIVITY: f32 = 0.275;
const GROUND_DRAG: f32 = 0.95;
const AIR_DRAG: f32 = 0.955;

/// `ExternalForceKeeper` (0x551e40): the componentwise extrema of the forces a frame's messages
/// brought, both starting from zero; the force is their sum (`calcForce` 0x551f64).
#[derive(Clone, Debug, Default)]
pub(super) struct ForceKeeper {
    min: Vec3,
    max: Vec3,
    /// `PlayerExternalVelocity::mExternalForce` (+0x4c): the force the last update took from them.
    applied: Vec3,
}

impl ForceKeeper {
    /// `receiveMsg` for `ByugoBlow`: the compare-and-select keeps the old extremum only when it
    /// is strictly beyond the new force.
    fn receive(&mut self, force: Vec3) {
        let lower = |old: f32, new: f32| if old < new { old } else { new };
        let upper = |old: f32, new: f32| if old > new { old } else { new };
        self.min = Vec3::new(
            lower(self.min.x, force.x),
            lower(self.min.y, force.y),
            lower(self.min.z, force.z),
        );
        self.max = Vec3::new(
            upper(self.max.x, force.x),
            upper(self.max.y, force.y),
            upper(self.max.z, force.z),
        );
    }

    fn force(&self) -> Vec3 {
        self.min + self.max
    }

    fn reset(&mut self) {
        self.min = Vec3::ZERO;
        self.max = Vec3::ZERO;
    }
}

/// The sensors of the upright 3D player that reach `PlayerExternalVelocity::receiveMsgPlayer`:
/// `PlayerActorHakoniwa::receiveMsg` 0x428810 answers false for the PlayerAttack sensors (Carry,
/// HipDropKnockDown: `isSensorPlayerAttack`, 0x428844). Name, offset up from the actor and radius
/// are the PlayerActorHakoniwa InitSensor entries.
const BLOWN_SENSORS: [(&str, f32, f32); 3] =
    [("Foot", 40., 40.), ("Body", 75., 50.), ("Head", 110., 40.)];

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
    /// The player's absolute stage position, the way the area and the wind sources read it.
    fn stage_position(&self, mesh: &crate::geometry::TriangleMesh) -> Vec3 {
        self.stage_translation
            .filter(|(p, _)| *p == self.pos)
            .map_or_else(|| mesh.to_game_vec(self.position_cm()), |(_, p)| p)
    }

    /// The ground flag `PlayerExternalVelocity::update` reads (`rs::isOnGround`, 0x440e20): the
    /// collider's contact of the last update.
    fn external_velocity_on_ground(&self) -> bool {
        self.is_on_ground_by(self.units_per_frame())
    }

    /// `PlayerExternalVelocity::reset` 0x441268.
    fn reset_external_velocity(&mut self) {
        self.external_keeper.reset();
        self.external_velocity = Vec3::ZERO;
        self.area_velocity = Vec3::ZERO;
    }

    /// The `AirCurrent` sensor phase of one frame: every current whose sphere overlaps one of the
    /// player's Foot/Body/Head spheres sends `ByugoBlow` (`attackSensor` 0x212e40), and the player's
    /// receiver (0x4289d8) feeds the force keeper. A dead or bound player resets the external
    /// velocity instead (0x4289e8..0x428a34; the Demo, Hack and Abyss nerves have no counterpart
    /// that could coincide with a current here).
    pub fn receive_air_currents(&mut self, mesh: &crate::geometry::TriangleMesh) {
        if mesh.air_currents.is_empty() {
            return;
        }
        let at = self.stage_position(mesh);
        let (up, _) = self.pose_axes();
        let mut sensors = Vec::new();
        let mut sent = Vec::new();
        for source in &mesh.air_currents {
            for (name, offset, radius) in BLOWN_SENSORS {
                let centre = at + up * offset;
                sensors.push(format!("{name}:{}", carry::hex_vec(centre)));
                let Some(force) = source.blow_at(centre, radius) else {
                    continue;
                };
                sent.push(format!("{}:{name}:{}", source.id, carry::hex_vec(force)));
                if self.is_dead() || self.bound_to.is_some() {
                    self.reset_external_velocity();
                } else {
                    self.external_keeper.receive(force);
                }
            }
        }
        self.wind_source = format!(
            "wind={};windpose={};windsensors={};windsent={}",
            mesh.air_currents
                .iter()
                .map(|c| c.diagnostic())
                .collect::<Vec<_>>()
                .join("|"),
            mesh.air_currents
                .iter()
                .map(|c| c.pose_diagnostic())
                .collect::<Vec<_>>()
                .join("|"),
            sensors.join("|"),
            sent.join("|")
        );
    }

    /// The `mExternalVelocity` half of `PlayerExternalVelocity::update`: the force the frame's
    /// messages collected enters the velocity, which keeps a drag share of itself.
    pub(super) fn update_external_velocity(&mut self, scene: &Course) {
        let sources = scene
            .mesh
            .as_ref()
            .is_some_and(|m| !m.air_currents.is_empty());
        if !sources
            && self.external_velocity == Vec3::ZERO
            && self.external_keeper.force() == Vec3::ZERO
        {
            return;
        }
        let drag = if self.external_velocity_on_ground() {
            GROUND_DRAG
        } else {
            AIR_DRAG
        };
        let force = self.external_keeper.force();
        self.external_keeper.reset();
        self.external_keeper.applied = force;
        self.external_velocity = self.external_velocity * drag + force * SENSITIVITY;
        if vector_lib::length(self.external_velocity) < 0.1 {
            self.external_velocity = Vec3::ZERO;
        }
        self.wind_update = format!(
            "windforce={};windvelocity={};windground={}",
            carry::hex_vec(force),
            carry::hex_vec(self.external_velocity),
            u8::from(drag == GROUND_DRAG)
        );
    }

    /// `mTotalVelocity` (0x440ff8..0x441028): the saved ground force, the external velocity and
    /// the area velocity added in that order.
    pub(super) fn total_external_velocity(&self) -> Vec3 {
        (self.ground_inertia.last + self.external_velocity) + self.area_velocity
    }

    pub(super) fn update_area_velocity(&mut self, scene: &Course) {
        self.area_source.clear();
        let Some(mesh) = &scene.mesh else { return };
        if mesh.force_areas.is_empty() {
            return;
        }
        let drag = if self.external_velocity_on_ground() {
            GROUND_DRAG
        } else {
            AIR_DRAG
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
    keeper_keeps_extrema_from_zero();
    sensors_feed_the_keeper_and_the_update_applies_it();
}

/// `ExternalForceKeeper::receiveMsg` (0x551e40): componentwise extrema starting from zero, the force is
/// their sum, so opposite forces of one frame add up and a negative-only frame keeps its minimum.
#[cfg_attr(test, test)]
fn keeper_keeps_extrema_from_zero() {
    let mut keeper = ForceKeeper::default();
    assert_eq!(keeper.force(), Vec3::ZERO);
    keeper.receive(Vec3::new(0., 3., 1.));
    keeper.receive(Vec3::new(0., 1., -2.));
    keeper.receive(Vec3::new(0., 2., -1.));
    assert_eq!(keeper.force(), Vec3::new(0., 3., -1.));
    keeper.reset();
    keeper.receive(Vec3::new(0., -4., 0.));
    assert_eq!(keeper.force(), Vec3::new(0., -4., 0.));
}

/// A player standing in a blowing current: the sphere test feeds the keeper from his three blown
/// sensors, the next update takes `v * drag + force * 0.275`, and the keeper starts empty again.
#[cfg_attr(test, test)]
fn sensors_feed_the_keeper_and_the_update_applies_it() {
    use crate::movement::air_current::AirCurrent;
    // A current at the origin pointing up (-90 degrees about X), blowing at full power (step 38 of Blow).
    let mut current = AirCurrent::new("t".into(), Vec3::ZERO, [-90., 0., 0.], 1., 30, -1, 0, false);
    for _ in 0..60 {
        current.tick();
    }
    let mut mesh = crate::geometry::TriangleMesh::default();
    mesh.air_currents.push(current);
    let mut player = Player {
        pos: Vec3::ZERO,
        ..Player::default()
    };
    player.receive_air_currents(&mesh);
    // Foot, Body and Head are on the axis, inside the taper: each sends the full 12.
    for sensor in ["Foot", "Body", "Head"] {
        assert!(
            player.wind_source.contains(&format!("t:{sensor}:")),
            "{}",
            player.wind_source
        );
    }
    let force = player.external_keeper.force();
    assert!((force.y - 12.).abs() < 1e-4 && force.x.abs() < 1e-4 && force.z.abs() < 1e-4);
    let course = Course {
        mesh: Some(std::sync::Arc::new(mesh)),
        ..Course::default()
    };
    player.update_external_velocity(&course);
    assert!((player.external_velocity.y - 12. * SENSITIVITY).abs() < 1e-4);
    assert_eq!(player.external_keeper.force(), Vec3::ZERO);
    assert!((player.external_keeper.applied.y - 12.).abs() < 1e-4);
    // Nothing more arrives: the velocity keeps its drag share (air or ground) of itself.
    let before = player.external_velocity.y;
    player.update_external_velocity(&course);
    let kept = player.external_velocity.y / before;
    assert!((kept - AIR_DRAG).abs() < 1e-6 || (kept - GROUND_DRAG).abs() < 1e-6);
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
