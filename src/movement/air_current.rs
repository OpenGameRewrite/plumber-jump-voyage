//! `AirCurrent` (1.0.0): the placed wind source whose attack sensor blows `ByugoBlow` messages
//! at the player (`AirCurrent::init` 0x212b90, `attackSensor` 0x212e40, force helper 0x212fe0,
//! `calcBlowPowerRate` 0x212f5c, nerves 0x213270..0x213b18).
//!
//! Supported: a source that does not move (a single key pose), is not driven by a stage switch
//! and is never clipped away (the evidence checks the recorded actor rows). The source is created
//! asleep with the stage and runs its nerve chain from the first update the replay executes
//! (`relax` updates included): Sleep, then Wait/BlowSign/BlowStart, Blow and, for a finite
//! `BlowFrame`, BlowEnd and Wait again. The receiving side is `ExternalForceKeeper` and the
//! player's `PlayerExternalVelocity` (`external_velocity.rs`).
use super::*;
use bevy::prelude::Quat;

/// `AirCurrent::init` 0x212b90: the blow length is 2000 x the placement's local Z scale.
const BASE_LENGTH: f32 = 2000.;
/// `AirCurrent::init`: the sensor's radius and its offset along the local front are 0.55 x length.
const SENSOR_LENGTH_RATE: f32 = 0.55;
/// Force helper 0x212fe0: the base magnitude of the force.
const BASE_FORCE: f32 = 12.;
/// Force helper 0x212fe0: below this local length the cone test applies ...
const CONE_LENGTH: f32 = 200.;
/// ... a point inside it must lie within `local z / 200 * 400` of the axis.
const CONE_RADIUS_PER_LENGTH: f32 = 400.;
/// Force helper 0x212fe0: the axial taper spans the last 400 of the length.
const AXIAL_TAPER: f32 = 400.;
/// Force helper 0x212fe0: the transverse taper between 250 and 400 off the axis.
const TRANSVERSE_TAPER_START: f32 = 250.;
const TRANSVERSE_TAPER_END: f32 = 400.;
/// `attackSensor` 0x212e40: the power and the force vector must exceed this to be sent.
const SEND_THRESHOLD: f32 = 0.001;
/// Steps of the BlowStart nerve (0x213550), the Blow power ramp and the BlowEnd falloff.
const BLOW_START_STEPS: i32 = 20;
const BLOW_RAMP_STEPS: i32 = 20;
const BLOW_END_STEPS: i32 = 40;
/// BlowSign (0x213484) lasts until this step.
const BLOW_SIGN_STEPS: i32 = 40;
/// BlowEnd (0x2139e8) returns to Wait at this step.
const BLOW_END_WAIT_STEPS: i32 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nerve {
    Sleep,
    Wait,
    BlowSign,
    BlowStart,
    Blow,
    BlowEnd,
}

impl Nerve {
    fn name(self, force_calc: bool) -> &'static str {
        match self {
            Nerve::Sleep => "Sleep",
            Nerve::Wait => "Wait",
            Nerve::BlowSign => "BlowSign",
            Nerve::BlowStart => "BlowStart",
            Nerve::Blow if force_calc => "BlowForceCalc",
            Nerve::Blow => "Blow",
            Nerve::BlowEnd => "BlowEnd",
        }
    }
}

/// One placed `AirCurrent`, in absolute stage coordinates.
#[derive(Clone, Debug)]
pub struct AirCurrent {
    pub id: String,
    translate: Vec3,
    quat: Quat,
    /// `AirCurrent+0x12c`: the length of the current.
    length: f32,
    /// `WaitFrame` (+0x144), `BlowFrame` (+0x148, negative: continuous) and `DelayFrame` (+0x14c).
    wait_frame: i32,
    blow_frame: i32,
    delay_frame: i32,
    /// `IsForceCalc` (+0x182) only selects the BlowForceCalc nerve, which runs the Blow executor.
    force_calc: bool,
    nerve: Nerve,
    /// `al::getNerveStep`: the step the next update executes (0 on a nerve's first update).
    step: i32,
}

/// `sead::Quatf::setRPY` as `ActorPoseKeeperTQGSV::updatePoseRotate` 0x8eda54 runs it on the
/// placement's rotation in degrees, with the console's sine and cosine.
fn quat_from_placement_degrees(rotate: [f32; 3]) -> Quat {
    const DEGREES_TO_RADIANS: f32 = 0.017_453_292;
    let half = |degrees: f32| degrees * DEGREES_TO_RADIANS * 0.5;
    let (hx, hy, hz) = (half(rotate[0]), half(rotate[1]), half(rotate[2]));
    let (cx, cy, cz) = (
        console_libm::cosf(hx),
        console_libm::cosf(hy),
        console_libm::cosf(hz),
    );
    let (sx, sy, sz) = (
        console_libm::sinf(hx),
        console_libm::sinf(hy),
        console_libm::sinf(hz),
    );
    let w = cx * cy * cz + sx * sy * sz;
    let x = cz * cy * sx - cx * sz * sy;
    let y = cx * cz * sy + cy * sz * sx;
    let z = cx * cy * sz - cz * sy * sx;
    Quat::from_xyzw(x, y, z, w)
}

/// `al::calcQuatFront`: the third column of the rotation matrix of `q`, which is what
/// `al::calcFrontDir` reads from the actor's pose matrix.
fn quat_front(q: Quat) -> Vec3 {
    Vec3::new(
        2. * (q.x * q.z) + 2. * (q.w * q.y),
        2. * (q.y * q.z) - 2. * (q.w * q.x),
        1. - 2. * (q.x * q.x) - 2. * (q.y * q.y),
    )
}

/// `al::multVecInvQuat` (ActorPoseUtil.cpp): the point relative to the actor, in its local frame.
fn local_point(trans: Vec3, q: Quat, point: Vec3) -> Vec3 {
    let v = point - trans;
    let r = Quat::from_xyzw(
        -(q.y * v.z) + (q.z * v.y) + (q.w * v.x),
        (q.x * v.z) - (q.z * v.x) + (q.w * v.y),
        -(q.x * v.y) + (q.y * v.x) + (q.w * v.z),
        (q.x * v.x) + (q.y * v.y) + (q.z * v.z),
    );
    Vec3::new(
        (r.x * q.w) + (r.y * q.z) - (r.z * q.y) + (r.w * q.x),
        -(r.x * q.z) + (r.y * q.w) + (r.z * q.x) + (r.w * q.y),
        (r.x * q.y) - (r.y * q.x) + (r.z * q.w) + (r.w * q.z),
    )
}

/// `sead::Mathf::clamp` as `al::lerpValue` uses it: a value that is not above `high` (a NaN included)
/// is returned as it is.
fn clamp_rate(value: f32, low: f32, high: f32) -> f32 {
    if value < low {
        low
    } else if value.partial_cmp(&high) == Some(std::cmp::Ordering::Greater) {
        high
    } else {
        value
    }
}

/// `al::lerpValue(a, b, t, clampA, clampB)` (MathUtil.cpp): `a` mapped from [b, t] onto
/// [clampA, clampB], clamped.
fn lerp_between(a: f32, b: f32, t: f32, clamp_a: f32, clamp_b: f32) -> f32 {
    if (t - b).abs() < 0.001 {
        return if a <= b { clamp_a } else { clamp_b };
    }
    let rate = clamp_rate((a - b) / (t - b), 0., 1.);
    clamp_a * (1. - rate) + rate * clamp_b
}

impl AirCurrent {
    /// `AirCurrent::init` for a placement: `translate` in absolute stage centimetres, `rotate` in
    /// degrees, `scale_z` the placement's local Z scale and the three frame arguments.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        translate: Vec3,
        rotate: [f32; 3],
        scale_z: f32,
        wait_frame: i32,
        blow_frame: i32,
        delay_frame: i32,
        force_calc: bool,
    ) -> Self {
        Self {
            id,
            translate,
            quat: quat_from_placement_degrees(rotate),
            length: scale_z * BASE_LENGTH,
            wait_frame,
            blow_frame,
            delay_frame,
            force_calc,
            nerve: Nerve::Sleep,
            step: 0,
        }
    }

    /// One `NerveKeeper::update` (NerveKeeper.cpp): the nerve executes with its current step, the
    /// step advances, and a nerve chosen meanwhile starts at step 0 within the same update.
    pub fn tick(&mut self) {
        let next = self.execute();
        self.step += 1;
        if let Some(next) = next {
            self.nerve = next;
            self.step = 0;
        }
    }

    fn continuous(&self) -> bool {
        self.blow_frame < 0
    }

    /// The nerve executors (`exeSleep` 0x213270, `exeWait` 0x213304, `exeBlowSign` 0x213484,
    /// `exeBlowStart` 0x213550, `exeBlow` 0x21365c, `exeBlowEnd` 0x2139e8). Clipping changes
    /// (`validateClipping`) are not modelled: the evidence requires an unclipped source.
    fn execute(&self) -> Option<Nerve> {
        match self.nerve {
            Nerve::Sleep => (self.step >= self.delay_frame).then(|| self.start_nerve()),
            Nerve::Wait => (self.step >= self.wait_frame).then(|| self.start_nerve()),
            Nerve::BlowSign => (self.step >= BLOW_SIGN_STEPS).then_some(Nerve::BlowStart),
            Nerve::BlowStart => (self.step >= BLOW_START_STEPS).then_some(Nerve::Blow),
            Nerve::Blow => {
                (!self.continuous() && self.step >= self.blow_frame).then_some(Nerve::BlowEnd)
            }
            Nerve::BlowEnd => (self.step >= BLOW_END_WAIT_STEPS).then_some(Nerve::Wait),
        }
    }

    /// The nerve Sleep and Wait choose once their wait is over: a finite current announces
    /// itself (BlowSign), a continuous one starts at once (0x2132a8..0x2132c0, 0x213378..0x2133b4).
    fn start_nerve(&self) -> Nerve {
        if self.continuous() {
            Nerve::BlowStart
        } else {
            Nerve::BlowSign
        }
    }

    /// The blow power: `AirCurrent::attackSensor` 0x212e40 / `calcBlowPowerRate` 0x212f5c read it
    /// from the nerve's step after the update.
    fn power(&self) -> f32 {
        match self.nerve {
            Nerve::Blow => clamp_rate(self.step as f32 / BLOW_RAMP_STEPS as f32, 0., 1.),
            Nerve::BlowEnd => {
                let rate = clamp_rate(self.step as f32 / BLOW_END_STEPS as f32, 0., 1.);
                1. * (1. - rate) + rate * 0.
            }
            _ => 0.,
        }
    }

    fn front(&self) -> Vec3 {
        quat_front(self.quat)
    }

    /// The attack sensor (`AirCurrent::init`): a sphere of radius 0.55 x length centred 0.55 x
    /// length ahead of the origin along the front.
    fn sensor(&self) -> (Vec3, f32) {
        let reach = self.length * SENSOR_LENGTH_RATE;
        (
            self.translate + local_to_world(self.quat, Vec3::new(0., 0., reach)),
            reach,
        )
    }

    /// The force helper 0x212fe0: the force at a world point (a receiving sensor's centre).
    fn force_at(&self, point: Vec3, power: f32) -> Vec3 {
        let local = local_point(self.translate, self.quat, point);
        let front = self.front();
        if local.z < CONE_LENGTH {
            let cone = local.z / CONE_LENGTH * CONE_RADIUS_PER_LENGTH;
            if cone * cone < local.x * local.x + local.y * local.y {
                return Vec3::ZERO;
            }
        }
        let axial = lerp_between(local.z, self.length - AXIAL_TAPER, self.length, 1., 0.);
        // Compare and select: the larger magnitude, the first on a tie.
        let (side, up) = (
            if local.x > 0. { local.x } else { -local.x },
            if local.y > 0. { local.y } else { -local.y },
        );
        let across = if side < up { up } else { side };
        let transverse = lerp_between(across, TRANSVERSE_TAPER_START, TRANSVERSE_TAPER_END, 1., 0.);
        let scaled = |component: f32| {
            let blown = transverse * (axial * (component * BASE_FORCE));
            blown * power
        };
        Vec3::new(scaled(front.x), scaled(front.y), scaled(front.z))
    }

    /// `AirCurrent::attackSensor` against one receiving sensor: the `ByugoBlow` force it sends,
    /// when the spheres overlap and the power and the force pass the sending thresholds.
    pub fn blow_at(&self, sensor_centre: Vec3, sensor_radius: f32) -> Option<Vec3> {
        let power = self.power();
        if vector_lib::is_near_zero_value(power, SEND_THRESHOLD) {
            return None;
        }
        let (centre, radius) = self.sensor();
        let reach = radius + sensor_radius;
        if pose_math::squared_length(sensor_centre - centre) > reach * reach {
            return None;
        }
        let force = self.force_at(sensor_centre, power);
        (!vector_lib::is_near_zero(force, SEND_THRESHOLD)).then_some(force)
    }

    /// The pose the source evaluates its force in: translation and rotation as f32 bits.
    pub fn pose_diagnostic(&self) -> String {
        let bits = |values: &[f32]| {
            values
                .iter()
                .map(|v| format!("{:08x}", v.to_bits()))
                .collect::<Vec<_>>()
                .join("/")
        };
        format!(
            "{}:{}:{}",
            self.id,
            bits(&self.translate.to_array()),
            bits(&self.quat.to_array())
        )
    }

    pub fn diagnostic(&self) -> String {
        format!(
            "{}/{}/{}/{:08x}",
            self.id,
            self.nerve.name(self.force_calc),
            self.step,
            self.power().to_bits()
        )
    }
}

/// The local offset `offset` of an actor with rotation `q`, in the world (`calcTransLocalOffset`
/// without the translation).
fn local_to_world(q: Quat, offset: Vec3) -> Vec3 {
    pose_math::rotate_by_quat(offset, q)
}

pub(super) fn self_check() {
    blows_along_the_front_with_a_ramp();
    tapers_and_cone();
    nerve_chain_continuous();
}

#[cfg_attr(test, test)]
fn blows_along_the_front_with_a_ramp() {
    // A current standing at the origin, rotated -90 degrees about X: the front is +Y.
    let mut source = AirCurrent::new("t".into(), Vec3::ZERO, [-90., 0., 0.], 1., 30, -1, 0, false);
    assert!(source.front().y > 0.99999);
    assert!(source.blow_at(Vec3::new(0., 300., 0.), 50.).is_none());
    for _ in 0..22 {
        source.tick();
    }
    assert_eq!((source.nerve, source.step), (Nerve::Blow, 0));
    assert!(source.blow_at(Vec3::new(0., 300., 0.), 50.).is_none());
    source.tick();
    let force = source
        .blow_at(Vec3::new(0., 300., 0.), 50.)
        .expect("ramp starts at step 1");
    assert!((force.y - 12. * 0.05).abs() < 1e-5);
}

#[cfg_attr(test, test)]
fn tapers_and_cone() {
    let mut source = AirCurrent::new("t".into(), Vec3::ZERO, [-90., 0., 0.], 1., 30, -1, 0, false);
    for _ in 0..60 {
        source.tick();
    }
    let at = |x: f32, y: f32| source.blow_at(Vec3::new(x, y, 0.), 10.);
    // Full power on the axis, nothing beyond the last 400 of the length plus a sensor radius.
    assert!((at(0., 500.).unwrap().y - 12.).abs() < 1e-4);
    assert!(at(0., 1990.).is_some());
    assert!(at(0., 2200.).is_none());
    // The cone near the origin: a point 50 up and 150 across lies outside it.
    assert!(at(150., 50.).is_none());
    // Across the transverse taper (250..400) the force falls off to zero.
    let half = at(325., 1000.).unwrap().y;
    assert!(half > 5.9 && half < 6.1);
}

#[cfg_attr(test, test)]
fn nerve_chain_continuous() {
    let mut source = AirCurrent::new("t".into(), Vec3::ZERO, [-90., 0., 0.], 1., 30, -1, 0, false);
    source.tick();
    assert_eq!((source.nerve, source.step), (Nerve::BlowStart, 0));
    for _ in 0..21 {
        source.tick();
    }
    assert_eq!((source.nerve, source.step), (Nerve::Blow, 0));
    // A finite current ends and starts over: Sleep, BlowSign (40), BlowStart, Blow, BlowEnd, Wait.
    let mut finite = AirCurrent::new("f".into(), Vec3::ZERO, [0.; 3], 1., 10, 5, 0, false);
    finite.tick();
    assert_eq!((finite.nerve, finite.step), (Nerve::BlowSign, 0));
    for _ in 0..41 + 21 + 6 {
        finite.tick();
    }
    assert_eq!((finite.nerve, finite.step), (Nerve::BlowEnd, 0));
    for _ in 0..21 {
        finite.tick();
    }
    assert_eq!((finite.nerve, finite.step), (Nerve::Wait, 0));
}
