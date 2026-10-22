//! The pose of Cappy's `AllRoot` joint over its animation clips.
//!
//! The thrown cap faces the way its model's `AllRoot` joint does when the player
//! stands on it (`HackCap::exeTrample` reads `al::calcJointFrontDir` of that joint),
//! and the joint is moved by the clip the cap plays (`StayR` while it hovers).
//! The clips are part of the player's own installation: the converter writes them
//! into `assets/local/caps/MarioCap.glb`, and this module reads that file the first
//! time a pose is asked for, without a renderer. Nothing of it is kept in the
//! repository. When the file or a clip is missing the joint stays at its bind pose.
//!
//! Only the joint's rotation and scale are read; its translation does not turn
//! its axes. Keys are linear in time (the clips are sampled at 60 frames a second)
//! and a rotation between two keys is a spherical blend, which is not claimed to
//! be bit for bit what the game's sampler produces (the clips the thrown cap
//! plays have constant rotations, which are exact).
use bevy::prelude::{Quat, Vec3};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

/// The cap model, under the asset directory.
const CAP_MODEL: &str = "local/caps/MarioCap.glb";
/// The joint the original reads, as the converter names it.
const JOINT: &str = "Armature_AllRoot";
/// The clips are sampled at this many frames a second.
const FRAMES_PER_SECOND: f32 = 60.;

/// Keys of one channel: their times in frames and their values.
#[derive(Default)]
struct Track<T> {
    frames: Vec<f32>,
    values: Vec<T>,
    /// The blend between keys is a hold (the file says `STEP`).
    hold: bool,
}

impl<T: Copy> Track<T> {
    /// The two keys around `frame` and how far between them it is.
    fn around(&self, frame: f32) -> Option<(T, T, f32)> {
        let last = self.values.len().checked_sub(1)?;
        let next = self.frames.iter().position(|&key| key > frame);
        let Some(next) = next else {
            return Some((self.values[last], self.values[last], 0.));
        };
        if next == 0 {
            return Some((self.values[0], self.values[0], 0.));
        }
        let (from, to) = (self.frames[next - 1], self.frames[next]);
        let share = if self.hold {
            0.
        } else {
            (frame - from) / (to - from)
        };
        Some((self.values[next - 1], self.values[next], share))
    }
}

/// What a clip does to the joint.
#[derive(Default)]
struct JointClip {
    rotation: Track<Quat>,
    scale: Track<Vec3>,
}

static CLIPS: OnceLock<HashMap<String, JointClip>> = OnceLock::new();

/// The front axis (third column) of the `AllRoot` joint's matrix in the model's frame, with the
/// joint's scale in it, at `frame` of the clip `clip`: what `al::calcJointFrontDir` takes before
/// it normalises. The bind pose (along +Z, unit scale) where the clip is not known.
pub fn all_root_front(clip: &str, frame: f32) -> Vec3 {
    let clips = CLIPS.get_or_init(|| {
        let path = crate::local_assets::asset_dir().join(CAP_MODEL);
        std::fs::read(path)
            .ok()
            .and_then(|bytes| read_clips(&bytes))
            .unwrap_or_default()
    });
    clips.get(clip).map_or(Vec3::Z, |clip| clip.front_at(frame))
}

impl JointClip {
    /// The joint's front axis, scale included, at `frame`.
    fn front_at(&self, frame: f32) -> Vec3 {
        let rotation = self
            .rotation
            .around(frame)
            .map_or(Quat::IDENTITY, |(from, to, share)| from.slerp(to, share));
        let scale = self
            .scale
            .around(frame)
            .map_or(Vec3::ONE, |(from, to, share)| from.lerp(to, share));
        rotation * Vec3::new(0., 0., scale.z)
    }
}

/// The `AllRoot` channels of every clip of a binary glTF file.
fn read_clips(bytes: &[u8]) -> Option<HashMap<String, JointClip>> {
    let (json, binary) = chunks(bytes)?;
    let document: Value = serde_json::from_slice(json).ok()?;
    let joint = document["nodes"]
        .as_array()?
        .iter()
        .position(|node| node["name"] == JOINT)?;
    let mut clips = HashMap::new();
    for animation in document["animations"].as_array()? {
        // A clip that cannot be read is left out; the joint then keeps its bind pose.
        if let (Some(name), Some(clip)) = (
            animation["name"].as_str(),
            read_clip(&document, binary, animation, joint),
        ) {
            clips.insert(name.to_string(), clip);
        }
    }
    Some(clips)
}

/// The channels of one animation that move the joint at node index `joint`.
fn read_clip(
    document: &Value,
    binary: &[u8],
    animation: &Value,
    joint: usize,
) -> Option<JointClip> {
    let mut clip = JointClip::default();
    for channel in animation["channels"].as_array()? {
        if channel["target"]["node"].as_u64()? as usize != joint {
            continue;
        }
        let sampler = &animation["samplers"][channel["sampler"].as_u64()? as usize];
        let frames: Vec<f32> = floats(document, binary, sampler["input"].as_u64()?, 1)?
            .into_iter()
            .map(|seconds| seconds * FRAMES_PER_SECOND)
            .collect();
        let hold = sampler["interpolation"] == "STEP";
        let output = sampler["output"].as_u64()?;
        match channel["target"]["path"].as_str()? {
            "rotation" => {
                let values = floats(document, binary, output, 4)?;
                clip.rotation = Track {
                    frames,
                    values: values
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|q| Quat::from_xyzw(q[0], q[1], q[2], q[3]))
                        .collect(),
                    hold,
                };
            }
            "scale" => {
                let values = floats(document, binary, output, 3)?;
                clip.scale = Track {
                    frames,
                    values: values
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .map(|s| Vec3::new(s[0], s[1], s[2]))
                        .collect(),
                    hold,
                };
            }
            _ => {}
        }
    }
    Some(clip)
}

/// The JSON and binary chunks of a binary glTF file.
fn chunks(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    const JSON: u32 = 0x4E4F_534A;
    const BINARY: u32 = 0x004E_4942;
    if bytes.get(..4)? != b"glTF" {
        return None;
    }
    let word = |at: usize| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let (mut json, mut binary) = (None, None);
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let length = word(at)? as usize;
        let body = bytes.get(at + 8..at + 8 + length)?;
        match word(at + 4)? {
            JSON => json = Some(body),
            BINARY => binary = Some(body),
            _ => {}
        }
        at += 8 + length;
    }
    Some((json?, binary?))
}

/// The values of a float accessor, `width` numbers per element.
fn floats(document: &Value, binary: &[u8], accessor: u64, width: usize) -> Option<Vec<f32>> {
    const FLOAT: u64 = 5126;
    let accessor = &document["accessors"][accessor as usize];
    if accessor["componentType"].as_u64()? != FLOAT {
        return None;
    }
    let view = &document["bufferViews"][accessor["bufferView"].as_u64()? as usize];
    // (Interleaved views are not read.)
    if view["byteStride"]
        .as_u64()
        .is_some_and(|s| s as usize != width * 4)
    {
        return None;
    }
    let start =
        view["byteOffset"].as_u64().unwrap_or(0) + accessor["byteOffset"].as_u64().unwrap_or(0);
    let count = accessor["count"].as_u64()? as usize * width;
    let body = binary.get(start as usize..start as usize + count * 4)?;
    Some(
        body.as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect(),
    )
}

#[path = "tests/cap_joint.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
