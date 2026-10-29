//! Autonomous local-X RotateMapParts; assist/switch timelines are unsupported.
use super::{pose_math, *};
use crate::prism::{PartFrame, transform_point};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct RotatePart {
    pub id: String,
    pub part: usize,
    pub position: Vec3,
    pub speed: f32,
    pub executions: u32,
    pub quat: Quat,
    pub previous: PartFrame,
}

impl RotatePart {
    pub fn frame(&self) -> PartFrame {
        let [x, y, z] = pose_math::quat_to_axes(self.quat);
        PartFrame::new([
            x.x,
            y.x,
            z.x,
            self.position.x,
            x.y,
            y.y,
            z.y,
            self.position.y,
            x.z,
            y.z,
            z.z,
            self.position.z,
        ])
        .expect("rotating part matrix")
    }
    pub fn force_at(&self, p: Vec3) -> Vec3 {
        if self.executions == 0 {
            return Vec3::ZERO;
        }
        let local = transform_point(&self.previous.inverse, p.to_array());
        Vec3::from_array(transform_point(&self.frame().base, local)) - p
    }
    pub fn tick(&mut self) {
        self.previous = self.frame();
        let half = ((self.speed / 100.) * pose_math::DEGREES_TO_RADIANS) * 0.5;
        let mut s = pose_math::sinf(half);
        let c = pose_math::cosf(half);
        // Local X stays world X for this focused initial orientation. Keep
        // sead's product and normalization order rather than accumulating angles.
        let q = self.quat;
        s *= pose_math::rotate_by_quat(Vec3::X, q).x;
        let w = c * q.w - s * q.x;
        let x = s * q.w + c * q.x;
        let length = (w * w + x * x).sqrt();
        let inv = if length > 0. { 1. / length } else { 1. };
        self.quat = Quat::from_xyzw(inv * x, 0., 0., inv * w);
        self.executions += 1;
    }
    pub fn seed(&mut self, row: &HashMap<String, String>) -> Result<(), String> {
        let get = |k: &str| row.get(k).map(String::as_str).ok_or("rotor seed field");
        if get("id")? != self.id
            || get("axis")? != "0"
            || get("phase")? != "Rotate"
            || get("speed_bits")? != format!("{:08x}", self.speed.to_bits())
            || [
                "dead",
                "clipped",
                "assist_timer",
                "SwitchStart_valid",
                "SwitchAppear_valid",
                "registry_dropped",
                "message_dropped",
            ]
            .iter()
            .any(|k| get(k) != Ok("0"))
            || get("init_quat_bits")? != "00000000,00000000,00000000,3f800000"
            || get("reset_quat_bits")? != get("init_quat_bits")?
        {
            return Err("unsupported rotor seed".into());
        }
        let initial = self
            .position
            .to_array()
            .map(|v| format!("{:08x}", v.to_bits()))
            .join(",");
        if get("pos_bits")? != initial || get("init_pos_bits")? != initial {
            return Err("rotor placement seed".into());
        }
        let count: u32 = get("executions")?.parse().map_err(|_| "rotor executions")?;
        let step: i32 = get("step")?.parse().map_err(|_| "rotor step")?;
        if count > 720 || !(step == count as i32 || step == -1 && count == 0) {
            return Err("unsupported rotor phase".into());
        }
        self.quat = Quat::IDENTITY;
        self.executions = 0;
        self.previous = self.frame();
        for _ in 0..count {
            self.tick();
        }
        if get("quat_bits")?
            != self
                .quat
                .to_array()
                .map(|v| format!("{:08x}", v.to_bits()))
                .join(",")
        {
            return Err("rotor phase quaternion".into());
        }
        Ok(())
    }
    pub fn diagnostic(&self) -> String {
        let read_back = pose_math::to_quat(pose_math::quat_to_axes(self.quat));
        format!(
            "{}/Rotate/{}/{}",
            self.id,
            self.executions,
            read_back
                .to_array()
                .map(|v| format!("{:08x}", v.to_bits()))
                .join("/")
        )
    }
}

pub(super) fn self_check() {
    local_x_source_carries_a_point();
}
#[cfg_attr(test, test)]
fn local_x_source_carries_a_point() {
    let mut r = RotatePart {
        id: "test".into(),
        part: 0,
        position: Vec3::new(100., 200., 300.),
        speed: 50.,
        executions: 0,
        quat: Quat::IDENTITY,
        previous: PartFrame::new([1., 0., 0., 100., 0., 1., 0., 200., 0., 0., 1., 300.]).unwrap(),
    };
    r.tick();
    assert_eq!(r.executions, 1);
    assert_eq!(
        pose_math::to_quat(pose_math::quat_to_axes(r.quat))
            .x
            .to_bits(),
        0x3b8efa19
    );
    assert_eq!(r.force_at(r.position), Vec3::ZERO);
    assert!(r.force_at(r.position + Vec3::Z * 700.).y < 0.);
}
