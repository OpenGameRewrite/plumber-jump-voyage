//! `al::ConveyerMapParts` (1.0.0, `ConveyerMapParts::exeMove` 0x912cb4, `ConveyerStep::setTransByCoord`
//! 0x9130bc, `ConveyerKeyKeeper::calcPosAndQuat` 0xa69f98): a chain of identical collision steps that
//! slide along the `KeyMoveNext` key chain of the placement. The host is not a floor type: every step is
//! an ordinary moving collision part, so Mario is carried by the generic moving-part contact of the
//! collider. Supported: unrotated keys on a straight line along the local Z axis (`MoveAxis` 2), linear
//! interpolation, no stage switch and no ride-only speed (`IsRideOnlyMove`).
use super::*;
use crate::movement::rotate_parts::RotatePart;
use std::collections::HashMap;

/// One entry of `ConveyerKeyKeeper`: the key's signed distance along the move direction, the
/// cumulative travelled distance up to it and its offset from the line.
#[derive(Clone, Debug)]
struct ConveyorKey {
    distance: f32,
    total: f32,
    vertical: Vec3,
}

#[derive(Clone, Debug)]
pub struct ConveyorHost {
    pub id: String,
    /// Indices into `TriangleMesh::rotate_parts` of the step parts, in step order.
    pub steps: Vec<usize>,
    speed: f32,
    interval: f32,
    /// `mMaxCoord`: `PartsInterval * stepCount`, the length at which the step coordinates wrap.
    max_coord: f32,
    trans: Vec3,
    direction: Vec3,
    keys: Vec<ConveyorKey>,
    /// `mTotalMoveDistance`.
    total: f32,
    /// `mOffsetCoord`.
    offset: f32,
    /// Where each step actor stood on the previous host update. A step's collision matrix is
    /// synchronised with its actor one update later (the recorded matrices of every sample are the
    /// actor positions of the update before), so the part follows these.
    lagged: Vec<Vec3>,
    /// Whether each step's collision was valid on the previous update. The collision part is only
    /// synchronised with its actor while it is valid (`invalidateCollisionParts` freezes both of its
    /// matrices), so a hidden step keeps the last pair it had.
    valid: Vec<bool>,
}

/// `al::wrapValue(value, max)`: `fmodf(value + max, max) + 0`.
fn wrap_value(value: f32, max: f32) -> f32 {
    ((value + max) % max) + 0.0
}

impl ConveyorHost {
    /// `ConveyerKeyKeeper::init` and `ConveyerMapParts::init`. `keys` are the placement and its
    /// `KeyMoveNext` chain; `direction` is the placement's local move axis.
    pub fn new(
        id: String,
        steps: Vec<usize>,
        speed: f32,
        interval: f32,
        start_rate: f32,
        keys: &[Vec3],
    ) -> Self {
        let trans = keys[0];
        let direction = Vec3::Z;
        let mut converted: Vec<ConveyorKey> = keys
            .iter()
            .map(|key| {
                let relative = *key - trans;
                let distance =
                    relative.x * direction.x + relative.y * direction.y + relative.z * direction.z;
                // al::verticalizeVec: setScaleAdd(-dot, vertical, vec)
                let dot = distance;
                let vertical = direction * -dot + relative;
                ConveyorKey {
                    distance,
                    total: 0.0,
                    vertical,
                }
            })
            .collect();
        let mut total = 0.0f32;
        for i in 1..converted.len() {
            let span = (converted[i].distance - converted[i - 1].distance).abs();
            total += span;
            converted[i].total = total;
        }
        let interval = interval.max(10.0);
        let count = (total / interval) as usize + 1;
        debug_assert_eq!(count, steps.len());
        let max_coord = interval * count as f32;
        Self {
            id,
            steps,
            speed,
            interval,
            max_coord,
            trans,
            direction,
            keys: converted,
            total,
            offset: wrap_value(interval * start_rate, interval),
            lagged: Vec::new(),
            valid: Vec::new(),
        }
    }

    /// `ConveyerKeyKeeper::calcPosAndQuat` (position only: the supported keys are unrotated).
    fn position_at(&self, coord: f32) -> Vec3 {
        let last = self.keys.last().expect("conveyor keys");
        if coord <= 0.0 {
            return self.trans;
        }
        if coord >= self.total {
            return last.vertical + self.direction * last.distance + self.trans;
        }
        let index = self
            .keys
            .iter()
            .position(|k| k.total > coord)
            .unwrap_or(0)
            .max(1);
        let (previous, key) = (&self.keys[index - 1], &self.keys[index]);
        let span = key.total - previous.total;
        let t = if span.abs() < 0.001 {
            0.0
        } else {
            (coord - previous.total) / span
        };
        // easeByType(t, 0) is t; lerpVec and lerpValue (clamped t) as in MathUtil.cpp
        let vertical = previous.vertical + (key.vertical - previous.vertical) * t;
        let clamped = t.clamp(0.0, 1.0);
        let distance = previous.distance * (1.0 - clamped) + clamped * key.distance;
        self.direction * distance + self.trans + vertical
    }

    /// Where step `index` stands for `offset`, and whether its collision exists
    /// (`ConveyerStep::setTransByCoord`: beyond the total distance the part is invalidated).
    fn step_pose(&self, offset: f32, index: usize) -> (Vec3, bool) {
        let coord = self.step_coord(offset, index);
        (self.position_at(coord), coord <= self.total)
    }

    /// `ConveyerStep::setTransByCoord`: `wrapValue(PartsInterval * i + offset, mMaxCoord)`.
    fn step_coord(&self, offset: f32, index: usize) -> f32 {
        wrap_value(self.interval * index as f32 + offset, self.max_coord)
    }

    /// `ConveyerMapParts::exeMove`: the offset advances, every step actor is placed by its
    /// coordinate, and the collision parts take the positions of the update before.
    pub fn tick(&mut self, parts: &mut [RotatePart], disabled: &mut Vec<usize>) {
        let previous_offset = self.offset;
        self.offset = wrap_value(self.offset + 1.0 * self.speed, self.max_coord);
        for (i, step) in self.steps.iter().enumerate() {
            let (position, exists) = self.step_pose(self.offset, i);
            if self.valid[i] {
                parts[*step].drive(self.lagged[i]);
            }
            // `ConveyerStep::setTransByCoord` 0x9130bc: a step whose coordinate went back (the
            // wrap of a forward belt) calls `resetPosition`, whose `resetAllCollisionMtx`
            // 0x8d7ee8 makes both collision matrices the new pose.
            if self.step_coord(self.offset, i) < self.step_coord(previous_offset, i) {
                parts[*step].reset(position);
            }
            self.lagged[i] = position;
            self.valid[i] = exists;
            if !exists {
                disabled.push(parts[*step].part);
            }
        }
    }

    /// Only the independently observed initial offset is an input to replay; the placement
    /// arguments are checked against what the recorded host reports. The first sample's message
    /// buffer also holds what the host received while the probe placed the player (before the
    /// replayed window), so its dropped count is not a seed condition: the only message the host
    /// acts on is `isMsgFloorTouch` (`ConveyerMapParts::receiveMsg` 0x912b94), which feeds the ride
    /// counters that `exeMove` 0x912cb4 reads only with `IsRideOnlyMove` (required to be 0 here).
    pub fn seed(
        &mut self,
        row: &HashMap<String, String>,
        parts: &mut [RotatePart],
        disabled: &mut Vec<usize>,
    ) -> Result<(), String> {
        let get = |k: &str| row.get(k).map(String::as_str).ok_or("conveyor seed field");
        let bits = |v: f32| format!("{:08x}", v.to_bits());
        if get("id")? != self.id
            || get("phase")? != "Move"
            || get("speed_bits")? != bits(self.speed)
            || get("interval_bits")? != bits(self.interval)
            || get("max_coord_bits")? != bits(self.max_coord)
            || get("key_total_bits")? != bits(self.total)
            || get("steps")? != self.steps.len().to_string()
            || get("keys")? != self.keys.len().to_string()
            || [
                "dead",
                "clipped",
                "SwitchStart_valid",
                "ride_only",
                "registry_dropped",
            ]
            .iter()
            .any(|k| get(k) != Ok("0"))
        {
            return Err("unsupported conveyor seed".into());
        }
        let offset = u32::from_str_radix(get("offset_bits")?, 16).map_err(|_| "conveyor offset")?;
        self.offset = f32::from_bits(offset);
        if !self.offset.is_finite() || self.offset < 0.0 || self.offset >= self.max_coord {
            return Err("conveyor offset out of range".into());
        }
        // The recorded sample holds the actor poses of this update and the collision matrices of the
        // two updates before (current and previous).
        let last = wrap_value(self.offset - self.speed, self.max_coord);
        let before = wrap_value(last - self.speed, self.max_coord);
        self.lagged.clear();
        self.valid.clear();
        for (i, step) in self.steps.iter().enumerate() {
            let (now, exists) = self.step_pose(self.offset, i);
            parts[*step].place(self.step_pose(before, i).0, self.step_pose(last, i).0);
            self.lagged.push(now);
            self.valid.push(exists);
            if !exists {
                disabled.push(parts[*step].part);
            }
        }
        Ok(())
    }

    pub fn diagnostic(&self) -> String {
        format!("{}/Move/{:08x}", self.id, self.offset.to_bits())
    }
}

pub(super) fn self_check() {
    keys_place_steps_end_to_end();
}

/// Pyramid 001 (obj243 to obj244): five 800 long steps on a 3600 long line; a step past the
/// line's end loses its collision until it wraps to the start.
#[cfg_attr(test, test)]
fn keys_place_steps_end_to_end() {
    let keys = [
        Vec3::new(2350., 1450., -14100.),
        Vec3::new(2350., 1450., -17700.),
    ];
    let host = ConveyorHost::new("test".into(), vec![0, 1, 2, 3, 4], 2., 800., 0., &keys);
    assert_eq!((host.total, host.max_coord), (3600., 4000.));
    let (first, exists) = host.step_pose(0., 0);
    assert_eq!((first.z, exists), (-14100., true));
    assert_eq!(host.step_pose(0., 2).0.z, -15700.);
    // `setTransByCoord` hides a step only past the total distance (`newCoord > total`).
    let (edge, exists) = host.step_pose(2000., 2);
    assert_eq!((edge.z, exists), (-17700., true));
    let (hidden, exists) = host.step_pose(2100., 2);
    assert!(!exists);
    assert_eq!(hidden.z, -17700.);
    // A step wraps from the end of the belt to its start at `PartsInterval * count`.
    assert_eq!(
        (host.step_coord(798., 4), host.step_coord(802., 4)),
        (3998., 2.)
    );
}
