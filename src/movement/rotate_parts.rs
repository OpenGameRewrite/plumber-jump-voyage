//! Autonomous RotateMapParts turning about its local X or Y axis (`RotateAxis` 0/1) from an
//! identity placement; the Z axis, assist and switch timelines are unsupported.
use super::{pose_math, *};
use crate::prism::{PartFrame, transform_point};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct RotatePart {
    pub id: String,
    pub part: usize,
    /// `RotateAxis` of the placement: 0 turns about local X, 1 about local Y (a turntable).
    pub axis: u8,
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
    /// `al::rotateQuatLocalDirDegree` (`MathUtil.cpp`): the rotation about the quaternion's own
    /// axis is multiplied in front and the product normalized. A placement that starts at the
    /// identity keeps only the axis component and `w`, so the other products drop out; the
    /// operation order of the stored quaternion is the one of the original, not an accumulated
    /// angle.
    pub fn tick(&mut self) {
        self.previous = self.frame();
        let half = ((self.speed / 100.) * pose_math::DEGREES_TO_RADIANS) * 0.5;
        let mut s = pose_math::sinf(half);
        let c = pose_math::cosf(half);
        let q = self.quat;
        let (lane, unit) = match self.axis {
            0 => (q.x, Vec3::X),
            _ => (q.y, Vec3::Y),
        };
        let own_axis = pose_math::rotate_by_quat(unit, q);
        s *= if self.axis == 0 {
            own_axis.x
        } else {
            own_axis.y
        };
        let w = c * q.w - s * lane;
        let turned = s * q.w + c * lane;
        let length = (w * w + turned * turned).sqrt();
        let inv = if length > 0. { 1. / length } else { 1. };
        self.quat = if self.axis == 0 {
            Quat::from_xyzw(inv * turned, 0., 0., inv * w)
        } else {
            Quat::from_xyzw(0., inv * turned, 0., inv * w)
        };
        self.executions += 1;
    }
    pub fn seed(&mut self, row: &HashMap<String, String>) -> Result<(), String> {
        let get = |k: &str| row.get(k).map(String::as_str).ok_or("rotor seed field");
        if get("id")? != self.id
            || get("axis")? != self.axis.to_string()
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
        // The first recorded sample follows a collision-part synchronisation: the part's
        // previous matrix is its current one (later samples carry the last current matrix).
        self.previous = self.frame();
        // The probe reads the quaternion back through `al::calcQuat` (the matrix), as `diagnostic`
        // does, not the stored one: they can differ in the last bit.
        let read_back = pose_math::to_quat(pose_math::quat_to_axes(self.quat));
        if get("quat_bits")?
            != read_back
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
    local_y_source_turns_a_point_about_the_vertical();
    standing_front_turns_with_a_turntable();
}
#[cfg_attr(test, test)]
fn local_x_source_carries_a_point() {
    let mut r = RotatePart {
        id: "test".into(),
        part: 0,
        axis: 0,
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

/// A turntable (`RotateAxis` 1, speed -80 as in Lava Clock Ex obj312): the first execution
/// reads back as the recorded quaternion, and the surface carries a point sideways.
#[cfg_attr(test, test)]
fn local_y_source_turns_a_point_about_the_vertical() {
    let position = Vec3::new(4850., 1050., -2650.);
    let mut r = RotatePart {
        id: "test".into(),
        part: 0,
        axis: 1,
        position,
        speed: -80.,
        executions: 0,
        quat: Quat::IDENTITY,
        previous: PartFrame::new([1., 0., 0., 4850., 0., 1., 0., 1050., 0., 0., 1., -2650.])
            .unwrap(),
    };
    r.tick();
    let read_back = pose_math::to_quat(pose_math::quat_to_axes(r.quat));
    assert_eq!(read_back.y.to_bits(), 0xbbe4c30f);
    assert_eq!(read_back.w.to_bits(), 0x3f7ffe67);
    assert_eq!((read_back.x, read_back.z), (0., 0.));
    let carried = r.force_at(position + Vec3::X * 300.);
    assert!(carried.z > 4. && carried.y == 0.);
    assert_eq!(r.force_at(position), Vec3::ZERO);
}

/// Standing on the turntable (`rs::waitGround` -> `al::followRotateFrontAxisUp`): the front is
/// carried through the part's previous and current matrices, so one -0.8 degree execution turns
/// a front along +Z by sin(-0.8 degrees) toward -X (the recorded run starts at -0.0140).
#[cfg_attr(test, test)]
fn standing_front_turns_with_a_turntable() {
    let mut r = RotatePart {
        id: "test".into(),
        part: 0,
        axis: 1,
        position: Vec3::new(4850., 1050., -2650.),
        speed: -80.,
        executions: 0,
        quat: Quat::IDENTITY,
        previous: PartFrame::new([1., 0., 0., 4850., 0., 1., 0., 1050., 0., 0., 1., -2650.])
            .unwrap(),
    };
    r.tick();
    let standing = pose_math::ActorPose::new(Quat::IDENTITY);
    let turned = standing.follow_part(&r.previous.inverse, &r.frame().base);
    let front = pose_math::ActorPose::new(turned).front();
    assert!((front.x + 0.013_962_18).abs() < 1e-6 && front.y == 0. && front.z > 0.999);
}
