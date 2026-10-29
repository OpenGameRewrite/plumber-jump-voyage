//! SeaWorldUtsuboCaveStage obj473: the MoveType0 rail swimmer and hip-drop receiver.
use super::{Move, Player, pose_math, swim::SwimNerve};
use bevy::prelude::{Quat, Vec3};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug)]
struct Bezier {
    coefficients: [[f32; 4]; 3],
    length: f32,
}

impl Bezier {
    fn new(points: [Vec3; 4]) -> Self {
        let mut curve = Self {
            coefficients: [[0.; 4]; 3],
            length: 0.,
        };
        for (axis, c) in curve.coefficients.iter_mut().enumerate() {
            let [a, b, c1, d] = points.map(|p| p[axis]);
            let (ab, bc, cd) = (b - a, c1 - b, d - c1);
            *c = [a, ab * 3., (bc - ab) * 3., (cd - bc) - (bc - ab)];
        }
        curve.length = curve.arc(1.);
        curve
    }
    fn velocity(&self, t: f32) -> Vec3 {
        let twice = t + t;
        let square = (t * 3.) * t;
        Vec3::from_array(
            self.coefficients
                .map(|[_, b, c, d]| (twice * c + b) + square * d),
        )
    }
    /// BezierCurve::calcLength (0xa7b514), including its half endpoint weights.
    fn arc(&self, t: f32) -> f32 {
        let speed = |t| pose_math::squared_length(self.velocity(t)).sqrt();
        let endpoints = (speed(0.) + speed(t)) * 0.5;
        let h = t * (1. / 20.);
        let (mut even, mut odd) = (0., 0.);
        for i in 1..=10 {
            odd += speed(h * ((i as f32 + i as f32) - 1.));
            if i != 10 {
                even += speed(h * (i as f32 + i as f32));
            }
        }
        let length = (h * (1. / 3.)) * ((endpoints + (even + even)) + odd * 4.);
        (length * 1024.).floor() * (1. / 1024.)
    }
    fn parameter(&self, distance: f32) -> f32 {
        let mut t = distance / self.length;
        let mut length = self.arc(t);
        for _ in 0..5 {
            if (distance - length).abs() <= 0.01 {
                break;
            }
            let speed = pose_math::squared_length(self.velocity(t))
                .sqrt()
                .max(0.001);
            t = (t + (distance - length) / speed).clamp(0., 1.);
            length = self.arc(t);
        }
        t
    }
    fn position(&self, t: f32) -> Vec3 {
        let (t2, t3) = (t * t, (t * t) * t);
        Vec3::from_array(
            self.coefficients
                .map(|[a, b, c, d]| ((b * t + a) + t2 * c) + t3 * d),
        )
    }
}

#[derive(Clone, Debug)]
pub struct Pukupuku {
    rail: [Bezier; 2],
    animation: Value,
    pub coord: f32,
    pub position: Vec3,
    pub body: Vec3,
    previous_body: Vec3,
    quat: Quat,
    pub phase: u8,
    pub step: u32,
    anim_frame: u32,
    first: bool,
}

fn vector(value: &Value) -> Result<Vec3, String> {
    let array = value
        .as_array()
        .filter(|a| a.len() == 3)
        .ok_or("puku vector")?;
    let mut v = Vec3::ZERO;
    for (i, item) in array.iter().enumerate() {
        v[i] = item.as_f64().ok_or("puku component")? as f32;
    }
    if !v.is_finite() {
        return Err("nonfinite Pukupuku vector".into());
    }
    Ok(v)
}

impl Pukupuku {
    pub fn load(value: &Value) -> Result<Self, String> {
        if value["id"] != "obj473"
            || value["rail_id"] != "rail15"
            || value["move_type"] != 0
            || value["revive"] != true
        {
            return Err("unsupported Pukupuku placement".into());
        }
        let p = &value["rail"];
        if p.as_array().is_none_or(|p| p.len() != 2) {
            return Err("Pukupuku rail sections".into());
        }
        let animation = &value["animation"];
        let joints = animation["joints"]
            .as_array()
            .filter(|j| j.len() == 3)
            .ok_or("Pukupuku sensor joints")?;
        for (joint, name) in joints.iter().zip(["nw4f_root", "AllRoot", "JointRoot"]) {
            if joint["name"] != name
                || joint["bind"].as_array().is_none_or(|b| {
                    b.len() != 10
                        || b.iter()
                            .any(|v| v.as_f64().is_none_or(|n| !(n as f32).is_finite()))
                })
            {
                return Err("Pukupuku bind pose".into());
            }
        }
        for name in ["SwimWait", "PressDown"] {
            let clip = &animation["clips"][name];
            if clip["length"]
                .as_f64()
                .is_none_or(|n| n <= 0. || n.fract() != 0. || n > u32::MAX as f64)
            {
                return Err("Pukupuku clip length".into());
            }
            for record in clip["records"].as_array().ok_or("Pukupuku clip records")? {
                if record["quaternion"] != 0 || record["joint"].as_u64().is_none_or(|j| j >= 3) {
                    return Err("unsupported Pukupuku channel rotation".into());
                }
                for channel in record["channels"]
                    .as_array()
                    .filter(|c| c.len() == 10)
                    .ok_or("Pukupuku channels")?
                {
                    if let Some(v) = channel.as_f64()
                        && (v as f32).is_finite()
                    {
                        continue;
                    }
                    let keys = channel
                        .as_array()
                        .filter(|k| !k.is_empty())
                        .ok_or("Pukupuku channel keys")?;
                    let mut previous = f64::NEG_INFINITY;
                    for key in keys {
                        let k = key
                            .as_array()
                            .filter(|k| k.len() == 5)
                            .ok_or("Pukupuku key coefficients")?;
                        if k.iter()
                            .any(|v| v.as_f64().is_none_or(|n| !(n as f32).is_finite()))
                        {
                            return Err("nonfinite Pukupuku key".into());
                        }
                        let frame = k[0].as_f64().unwrap();
                        if frame <= previous {
                            return Err("unordered Pukupuku keys".into());
                        }
                        previous = frame;
                    }
                }
            }
        }
        let curve = |a: usize, b: usize| -> Result<Bezier, String> {
            Ok(Bezier::new([
                vector(&p[a]["position"])?,
                vector(&p[a]["out"])?,
                vector(&p[b]["in"])?,
                vector(&p[b]["position"])?,
            ]))
        };
        Ok(Self {
            rail: [curve(0, 1)?, curve(1, 0)?],
            animation: value["animation"].clone(),
            coord: 0.,
            position: Vec3::ZERO,
            body: Vec3::ZERO,
            previous_body: Vec3::ZERO,
            quat: Quat::IDENTITY,
            phase: 0,
            step: 0,
            anim_frame: 0,
            first: true,
        })
    }
    pub fn seed(&mut self, row: &HashMap<String, String>) -> Result<(), String> {
        let get = |k: &str| {
            row.get(k)
                .map(String::as_str)
                .ok_or_else(|| format!("puku seed {k}"))
        };
        if get("id")? != "obj473"
            || get("n_vt")? != "1ca58b8"
            || get("step")? != "1"
            || get("move_type")? != "0"
            || get("dead")? != "0"
            || get("clipped")? != "0"
            || get("anim")? != "SwimWait"
            || get("anim_frame")? != "0.0"
            || get("length_bits")?
                != format!(
                    "{:08x}",
                    (self.rail[0].length + self.rail[1].length).to_bits()
                )
        {
            return Err("unsupported Pukupuku initial phase".into());
        }
        self.coord = get("coord")?.parse().map_err(|_| "puku coord")?;
        if !self.coord.is_finite()
            || !(0. ..self.rail[0].length + self.rail[1].length).contains(&self.coord)
        {
            return Err("Pukupuku initial coordinate".into());
        }
        let q = get("quat_bits")?
            .split(',')
            .map(|s| u32::from_str_radix(s, 16).map(f32::from_bits))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "puku quaternion")?;
        if q.len() != 4 || q.iter().any(|v| !v.is_finite()) || q.iter().all(|v| *v == 0.) {
            return Err("puku quaternion size".into());
        }
        self.quat = Quat::from_slice(&q);
        self.step = 1;
        self.position = self.rail_pose().0;
        if self
            .position
            .to_array()
            .map(|v| format!("{:08x}", v.to_bits()))
            .join(",")
            != get("pos_bits")?
        {
            return Err("puku seed rail pose".into());
        }
        let centre = get("centre_bits")?
            .split(',')
            .map(|s| u32::from_str_radix(s, 16).map(f32::from_bits))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "puku initial sensor")?;
        if centre.len() != 3 || centre.iter().any(|v| !v.is_finite()) {
            return Err("puku sensor size".into());
        }
        self.body = Vec3::from_slice(&centre);
        self.previous_body = self.body;
        Ok(())
    }
    fn rail_pose(&self) -> (Vec3, Vec3) {
        let (curve, distance) = if self.coord <= self.rail[0].length {
            (&self.rail[0], self.coord)
        } else {
            (&self.rail[1], self.coord - self.rail[0].length)
        };
        let t = curve.parameter(distance);
        (
            curve.position(t),
            pose_math::try_normalize_or_zero(curve.velocity(t)).0,
        )
    }
    pub fn accepts_hip_drop(&self, foot: Vec3) -> bool {
        self.phase == 0 && pose_math::squared_length(foot - self.body) < (40. + 80.) * (40. + 80.)
    }
    pub fn preceding_body(&self) -> Vec3 {
        self.previous_body
    }
    pub fn tick(&mut self, hit: bool) {
        // The teleport sample precedes the first preroll movement; the source
        // snapshot is observed again when that first player update completes.
        if self.first {
            self.first = false;
            return;
        }
        self.previous_body = self.body;
        if hit {
            self.phase = 1;
            self.step = 1;
            self.anim_frame = 0;
        } else if self.phase == 0 {
            self.coord += 5.;
            let length = self.rail[0].length + self.rail[1].length;
            if self.coord >= length {
                self.coord -= length;
            }
            let (position, direction) = self.rail_pose();
            self.position = position;
            let degrees = (self.step as f32 / 60.).clamp(0., 1.) * 5.;
            self.quat = pose_math::turn_quat(
                self.quat,
                pose_math::rotate_by_quat(Vec3::Z, self.quat),
                direction,
                degrees * pose_math::DEGREES_TO_RADIANS,
            );
            self.step += 1;
            self.anim_frame = (self.anim_frame + 1)
                % self.animation["clips"]["SwimWait"]["length"]
                    .as_f64()
                    .unwrap() as u32;
        } else {
            self.step += 1;
            if self.phase == 1 {
                let length = self.animation["clips"]["PressDown"]["length"]
                    .as_f64()
                    .unwrap() as u32;
                self.anim_frame = (self.anim_frame + 1).min(length);
                if self.anim_frame == length {
                    self.phase = 2;
                    self.step = 0;
                }
            }
        }
        self.body = self.sensor_centre();
    }
    fn sensor_centre(&self) -> Vec3 {
        let clip = if self.phase == 0 {
            "SwimWait"
        } else {
            "PressDown"
        };
        let mut matrix =
            super::joint_math::rotation_rows(self.quat).map(|r| [r[0], r[1], r[2], 0.]);
        for (i, row) in matrix.iter_mut().enumerate() {
            row[3] = self.position[i];
        }
        for (joint, bind) in self.animation["joints"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let record = self.animation["clips"][clip]["records"]
                .as_array()
                .and_then(|rs| rs.iter().find(|r| r["joint"] == joint));
            let mut local = [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]];
            if let Some(record) = record {
                let channels = &record["channels"];
                let v: [f32; 10] =
                    std::array::from_fn(|i| channel(&channels[i], self.anim_frame as f32));
                let (sx, cx) = (pose_math::sinf(v[3]), pose_math::cosf(v[3]));
                let (sy, cy) = (pose_math::sinf(v[4]), pose_math::cosf(v[4]));
                let (sz, cz) = (pose_math::sinf(v[5]), pose_math::cosf(v[5]));
                local = [
                    [
                        cy * cz * v[0],
                        (sx * sy * cz - cx * sz) * v[1],
                        (cx * sy * cz + sx * sz) * v[2],
                        v[7],
                    ],
                    [
                        cy * sz * v[0],
                        (sx * sy * sz + cx * cz) * v[1],
                        (cx * sy * sz - sx * cz) * v[2],
                        v[8],
                    ],
                    [-sy * v[0], sx * cy * v[1], cx * cy * v[2], v[9]],
                ];
            } else if let Some(b) = bind["bind"].as_array() {
                let q = Quat::from_xyzw(
                    b[3].as_f64().unwrap() as f32,
                    b[4].as_f64().unwrap() as f32,
                    b[5].as_f64().unwrap() as f32,
                    b[6].as_f64().unwrap() as f32,
                );
                local = super::joint_math::rotation_rows(q).map(|r| [r[0], r[1], r[2], 0.]);
                for i in 0..3 {
                    local[i][3] = b[i].as_f64().unwrap() as f32;
                }
            }
            matrix = crate::skeleton_pose::mul34(&matrix, &local);
        }
        Vec3::new(
            matrix[0][1] * 15. + matrix[0][3],
            matrix[1][1] * 15. + matrix[1][3],
            matrix[2][1] * 15. + matrix[2][3],
        )
    }
    pub fn diagnostic(&self, hit: bool) -> String {
        format!(
            "{:08x}/{:08x}/{:08x}/{:08x}/{}/{}/{:08x}/{:08x}/{:08x}/{:08x}/{:08x}",
            self.coord.to_bits(),
            self.position.x.to_bits(),
            self.position.y.to_bits(),
            self.position.z.to_bits(),
            if hit { 1 } else { self.phase },
            if hit { -1 } else { self.step as i64 },
            self.body.x.to_bits(),
            self.body.y.to_bits(),
            self.body.z.to_bits(),
            self.quat.y.to_bits(),
            self.quat.w.to_bits()
        )
    }
}

fn channel(value: &Value, frame: f32) -> f32 {
    if let Some(v) = value.as_f64() {
        return v as f32;
    }
    let keys = value.as_array().unwrap();
    let at = keys
        .partition_point(|k| k[0].as_f64().unwrap() as f32 <= frame)
        .saturating_sub(1);
    let k = &keys[at];
    let c: [f32; 4] = std::array::from_fn(|i| k[i + 1].as_f64().unwrap() as f32);
    if at + 1 == keys.len() || frame <= k[0].as_f64().unwrap() as f32 {
        return c[0];
    }
    let start = k[0].as_f64().unwrap() as f32;
    let t = (frame - start) / (keys[at + 1][0].as_f64().unwrap() as f32 - start);
    ((c[3] * t + c[2]) * t + c[1]) * t + c[0]
}

impl Player {
    pub(super) fn puku_sensor_phase(&mut self, scene: &super::Course) {
        self.puku_hit = false;
        self.puku_source.clear();
        let Some(mesh) = &scene.mesh else {
            return;
        };
        let Some(source) = &mesh.pukupuku else {
            return;
        };
        if self.mode == Move::Swim
            && matches!(self.swim.nerve, SwimNerve::HipDropLoop | SwimNerve::Dive)
        {
            let foot = mesh.to_game_vec(self.position_cm()) + Vec3::Y * 40.;
            // receiveMsg rejects ObjHipDropReflect; fallback ObjHipDrop on
            // EnemyBody enters Trample and attackSensor sets only trigger3.
            if source.accepts_hip_drop(foot) {
                self.swim.attack.hip_drop_hit = true;
                self.puku_hit = true;
                self.events.push(super::Fx::PoundHit);
            }
        }
        self.puku_source = source.diagnostic(self.puku_hit);
    }
}

#[cfg(test)]
mod checks {
    use super::*;
    #[test]
    fn original_rail_length_and_inverse() {
        let c = Bezier::new([
            Vec3::new(0., 550., -450.),
            Vec3::new(600., 550., -450.),
            Vec3::new(600., 550., 450.),
            Vec3::new(0., 550., 450.),
        ]);
        assert_eq!(c.length, 1397.2412);
        assert_eq!(c.position(c.parameter(0.)), Vec3::new(0., 550., -450.));
        assert_eq!(c.position(c.parameter(c.length)), Vec3::new(0., 550., 450.));
    }

    #[test]
    fn accepted_hip_drop_stops_the_rail_until_revive() {
        let mut source = Pukupuku::load(&serde_json::json!({
            "id":"obj473", "rail_id":"rail15", "move_type":0,"revive":true,
            "rail":[{"position":[0,550,-450],"in":[-600,550,-450],"out":[600,550,-450]},
                {"position":[0,550,450],"in":[600,550,450],"out":[-600,550,450]}],
            "animation":{"joints":[
                {"name":"nw4f_root","bind":[0,0,0,0,0,0,1,1,1,1]},
                {"name":"AllRoot","bind":[0,0,0,0,0,0,1,1,1,1]},
                {"name":"JointRoot","bind":[0,0,0,0,0,0,1,1,1,1]}],
                "clips":{"SwimWait":{"length":360,"records":[]},"PressDown":{"length":40,"records":[]}}}
        }))
        .unwrap();
        source.first = false;
        source.body = Vec3::ZERO;
        assert!(source.accepts_hip_drop(Vec3::Y * 119.));
        assert!(!source.accepts_hip_drop(Vec3::Y * 120.));
        source.tick(true);
        assert_eq!((source.phase, source.step, source.anim_frame), (1, 1, 0));
        assert!(!source.accepts_hip_drop(Vec3::ZERO));
        for _ in 0..39 {
            source.tick(false);
        }
        assert_eq!((source.phase, source.step, source.anim_frame), (1, 40, 39));
        source.tick(false);
        assert_eq!((source.phase, source.step, source.anim_frame), (2, 0, 40));
        source.tick(false);
        assert_eq!((source.phase, source.step, source.coord), (2, 1, 0.));
    }
}
