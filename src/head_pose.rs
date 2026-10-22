//! The animated head: where the model's `Head` joint stands for a given blend of clips.
//!
//! The windowed game poses the skeleton in the renderer, but the headless trace has no
//! skeleton, and the head's look-at turns the matrix the animation leaves, so the trace
//! evaluates the head joint itself from the clips of the player's own installation (the
//! converter writes them into `assets/local/Mario.glb`, which this module reads the first time a
//! pose is asked for; nothing of it is kept in the repository). Nothing is returned when the
//! file or a needed clip is missing.
//!
//! The chain is the model's `nw4f_root`, `AllRoot`, `JointRoot`, `Spine1`, `Spine2` and `Head`
//! joints: each takes its translation and rotation from the clips (the bind pose where a clip has
//! no key for it), blended by the clips' weights (translations linearly, rotations as a
//! normalised weighted sum), and the head's matrix is the product down the chain. The clips are
//! sampled at 60 keys a second and linear between keys.
//!
//! Limits: only the animation. The original's joint controllers that run before the head's
//! (the swim and ground poses, the cap throw, the pole climb: `PlayerJointControlKeeper`) bend
//! the same joints, and a pose that leans on them (running down a slope, swimming) deviates by
//! what they add; the trace's `bones` column of the original shows where.
use crate::movement::look_joint::Mat34;
use bevy::prelude::Vec3;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// The model the trace poses.
const MODEL: &str = "local/Mario.glb";
/// The clips are sampled at this many keys a second.
const KEYS_PER_SECOND: f32 = 60.;
/// The joints from the root down to the head, as the converter names them.
const CHAIN_END: &str = "Armature_Head";
const CHAIN_ROOT: &str = "Armature_nw4f_root";

/// One clip playing: its name, the frame it shows and its share of the blend.
#[derive(Clone, Copy, Debug)]
pub struct PoseLayer<'a> {
    pub clip: &'a str,
    pub frame: f32,
    pub weight: f32,
}

#[derive(Deserialize)]
struct Document {
    nodes: Vec<NodeDef>,
    animations: Vec<AnimationDef>,
    accessors: Vec<AccessorDef>,
    #[serde(rename = "bufferViews")]
    buffer_views: Vec<ViewDef>,
}

#[derive(Deserialize)]
struct NodeDef {
    name: Option<String>,
    matrix: Option<[f32; 16]>,
    translation: Option<[f32; 3]>,
    rotation: Option<[f32; 4]>,
    children: Option<Vec<usize>>,
}

#[derive(Deserialize)]
struct AnimationDef {
    name: Option<String>,
    channels: Vec<ChannelDef>,
    samplers: Vec<SamplerDef>,
}

#[derive(Deserialize)]
struct ChannelDef {
    sampler: usize,
    target: TargetDef,
}

#[derive(Deserialize)]
struct TargetDef {
    node: Option<usize>,
    path: String,
}

#[derive(Deserialize)]
struct SamplerDef {
    input: usize,
    output: usize,
}

#[derive(Deserialize)]
struct AccessorDef {
    #[serde(rename = "bufferView")]
    buffer_view: usize,
    #[serde(rename = "byteOffset")]
    byte_offset: Option<usize>,
    count: usize,
}

#[derive(Deserialize)]
struct ViewDef {
    #[serde(rename = "byteOffset")]
    byte_offset: Option<usize>,
}

/// A joint's local pose: translation and rotation (x, y, z, w).
#[derive(Clone, Copy)]
struct Local {
    translation: [f64; 3],
    rotation: [f64; 4],
}

/// Keys of one channel.
struct Track<const N: usize> {
    times: Vec<f32>,
    values: Vec<[f32; N]>,
}

impl<const N: usize> Track<N> {
    /// The two keys around `time` and how far between them it is.
    fn around(&self, time: f32) -> Option<([f32; N], [f32; N], f32)> {
        let last = self.times.len().checked_sub(1)?;
        let next = self.times.partition_point(|key| *key <= time);
        if next == 0 {
            return Some((self.values[0], self.values[0], 0.));
        }
        if next > last {
            return Some((self.values[last], self.values[last], 0.));
        }
        let (from, to) = (self.times[next - 1], self.times[next]);
        Some((
            self.values[next - 1],
            self.values[next],
            (time - from) / (to - from),
        ))
    }
}

/// What a clip does to the chain: for each joint of it, its translation and rotation tracks.
#[derive(Default)]
struct ClipTracks {
    translation: Vec<Option<Track<3>>>,
    rotation: Vec<Option<Track<4>>>,
}

pub struct HeadPose {
    document: Document,
    bytes: Vec<u8>,
    bin_start: usize,
    /// The chain's node indices, root first, and each joint's bind pose.
    chain: Vec<usize>,
    bind: Vec<Local>,
    clips: Mutex<HashMap<String, Option<std::sync::Arc<ClipTracks>>>>,
}

/// A quaternion's rotation matrix, row-major, in double precision.
fn rotation_matrix(q: [f64; 4]) -> [[f64; 3]; 3] {
    let norm = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    let [x, y, z, w] = q.map(|c| c / norm);
    [
        [
            1. - 2. * (y * y + z * z),
            2. * (x * y - z * w),
            2. * (x * z + y * w),
        ],
        [
            2. * (x * y + z * w),
            1. - 2. * (x * x + z * z),
            2. * (y * z - x * w),
        ],
        [
            2. * (x * z - y * w),
            2. * (y * z + x * w),
            1. - 2. * (x * x + y * y),
        ],
    ]
}

fn multiply(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.; 3]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[r][k] * b[k][c]).sum();
        }
    }
    out
}

/// The quaternion of a rotation matrix whose columns may carry a uniform scale.
fn matrix_quaternion(columns: [[f64; 3]; 3]) -> [f64; 4] {
    let scale: Vec<f64> = columns
        .iter()
        .map(|c| (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt())
        .collect();
    let m = |row: usize, column: usize| columns[column][row] / scale[column];
    let trace = m(0, 0) + m(1, 1) + m(2, 2);
    if trace > 0. {
        let s = (trace + 1.).sqrt() * 2.;
        [
            (m(2, 1) - m(1, 2)) / s,
            (m(0, 2) - m(2, 0)) / s,
            (m(1, 0) - m(0, 1)) / s,
            s / 4.,
        ]
    } else if m(0, 0) > m(1, 1) && m(0, 0) > m(2, 2) {
        let s = (1. + m(0, 0) - m(1, 1) - m(2, 2)).sqrt() * 2.;
        [
            s / 4.,
            (m(0, 1) + m(1, 0)) / s,
            (m(0, 2) + m(2, 0)) / s,
            (m(2, 1) - m(1, 2)) / s,
        ]
    } else if m(1, 1) > m(2, 2) {
        let s = (1. + m(1, 1) - m(0, 0) - m(2, 2)).sqrt() * 2.;
        [
            (m(0, 1) + m(1, 0)) / s,
            s / 4.,
            (m(1, 2) + m(2, 1)) / s,
            (m(0, 2) - m(2, 0)) / s,
        ]
    } else {
        let s = (1. + m(2, 2) - m(0, 0) - m(1, 1)).sqrt() * 2.;
        [
            (m(0, 2) + m(2, 0)) / s,
            (m(1, 2) + m(2, 1)) / s,
            s / 4.,
            (m(1, 0) - m(0, 1)) / s,
        ]
    }
}

impl HeadPose {
    fn open() -> Option<Self> {
        let path = crate::local_assets::asset_dir().join(MODEL);
        let bytes = std::fs::read(path).ok()?;
        if bytes.len() < 20 || &bytes[0..4] != b"glTF" {
            return None;
        }
        let le = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let json_length = le(12);
        let document: Document = serde_json::from_slice(bytes.get(20..20 + json_length)?).ok()?;
        let bin_start = 20 + json_length + 8;
        let find = |name: &str| {
            document
                .nodes
                .iter()
                .position(|n| n.name.as_deref() == Some(name))
        };
        let mut parent = vec![None; document.nodes.len()];
        for (index, node) in document.nodes.iter().enumerate() {
            for child in node.children.iter().flatten() {
                parent[*child] = Some(index);
            }
        }
        let mut chain = vec![find(CHAIN_END)?];
        while let Some(up) = parent[*chain.last()?] {
            chain.push(up);
            if document.nodes[up].name.as_deref() == Some(CHAIN_ROOT) {
                break;
            }
        }
        chain.reverse();
        let bind = chain
            .iter()
            .map(|&i| {
                let node = &document.nodes[i];
                if let Some(m) = node.matrix {
                    let m = m.map(f64::from);
                    // Column-major: the first three columns are the axes, the fourth the offset.
                    let columns = [[m[0], m[1], m[2]], [m[4], m[5], m[6]], [m[8], m[9], m[10]]];
                    Local {
                        translation: [m[12], m[13], m[14]],
                        rotation: matrix_quaternion(columns),
                    }
                } else {
                    Local {
                        translation: node.translation.unwrap_or([0.; 3]).map(f64::from),
                        rotation: node.rotation.unwrap_or([0., 0., 0., 1.]).map(f64::from),
                    }
                }
            })
            .collect();
        Some(Self {
            document,
            bytes,
            bin_start,
            chain,
            bind,
            clips: Mutex::new(HashMap::new()),
        })
    }

    /// The installation's model, read once; `None` without game data.
    pub fn installed() -> Option<&'static HeadPose> {
        static MODEL_POSE: OnceLock<Option<HeadPose>> = OnceLock::new();
        MODEL_POSE.get_or_init(Self::open).as_ref()
    }

    fn floats<const N: usize>(&self, accessor: usize) -> Option<Vec<[f32; N]>> {
        let a = self.document.accessors.get(accessor)?;
        let view = self.document.buffer_views.get(a.buffer_view)?;
        let start = self.bin_start + view.byte_offset.unwrap_or(0) + a.byte_offset.unwrap_or(0);
        let raw = self.bytes.get(start..start + a.count * N * 4)?;
        Some(
            raw.chunks_exact(N * 4)
                .map(|chunk| {
                    let mut value = [0.; N];
                    for (slot, bytes) in value.iter_mut().zip(chunk.chunks_exact(4)) {
                        *slot = f32::from_le_bytes(bytes.try_into().unwrap());
                    }
                    value
                })
                .collect(),
        )
    }

    fn track<const N: usize>(&self, animation: &AnimationDef, channel: &ChannelDef) -> Option<Track<N>> {
        let sampler = animation.samplers.get(channel.sampler)?;
        let times = self.floats::<1>(sampler.input)?.into_iter().map(|t| t[0]).collect();
        Some(Track {
            times,
            values: self.floats::<N>(sampler.output)?,
        })
    }

    fn clip(&self, name: &str) -> Option<std::sync::Arc<ClipTracks>> {
        let mut cache = self.clips.lock().ok()?;
        if let Some(found) = cache.get(name) {
            return found.clone();
        }
        let tracks = self.load_clip(name).map(std::sync::Arc::new);
        cache.insert(name.to_string(), tracks.clone());
        tracks
    }

    fn load_clip(&self, name: &str) -> Option<ClipTracks> {
        let animation = self
            .document
            .animations
            .iter()
            .find(|a| a.name.as_deref() == Some(name))?;
        let mut tracks = ClipTracks {
            translation: (0..self.chain.len()).map(|_| None).collect(),
            rotation: (0..self.chain.len()).map(|_| None).collect(),
        };
        for channel in &animation.channels {
            let Some(slot) = channel
                .target
                .node
                .and_then(|n| self.chain.iter().position(|c| *c == n))
            else {
                continue;
            };
            match channel.target.path.as_str() {
                "translation" => tracks.translation[slot] = self.track::<3>(animation, channel),
                "rotation" => tracks.rotation[slot] = self.track::<4>(animation, channel),
                _ => {}
            }
        }
        Some(tracks)
    }

    /// The head's matrix in the frame of the player's root joint (cm), for the blend of `layers`;
    /// `None` when no layer's clip is in the model.
    pub fn head_in_actor_frame(&self, layers: &[PoseLayer]) -> Option<Mat34> {
        let playing: Vec<(std::sync::Arc<ClipTracks>, f32, f32)> = layers
            .iter()
            .filter(|l| l.weight > 0.)
            .filter_map(|l| Some((self.clip(l.clip)?, l.frame, l.weight)))
            .collect();
        let total: f64 = playing.iter().map(|(_, _, w)| f64::from(*w)).sum();
        if playing.is_empty() || total <= 0. {
            return None;
        }
        let mut rotation = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        let mut position = [0.; 3];
        for (slot, bind) in self.bind.iter().enumerate() {
            let mut translation = [0.; 3];
            let mut blended = [0.; 4];
            for (clip, frame, weight) in &playing {
                let share = f64::from(*weight) / total;
                let time = frame / KEYS_PER_SECOND;
                let t = clip.translation[slot]
                    .as_ref()
                    .and_then(|track| track.around(time))
                    .map_or(bind.translation, |(a, b, k)| {
                        std::array::from_fn(|i| {
                            f64::from(a[i]) + (f64::from(b[i]) - f64::from(a[i])) * f64::from(k)
                        })
                    });
                for i in 0..3 {
                    translation[i] += share * t[i];
                }
                let q = clip.rotation[slot]
                    .as_ref()
                    .and_then(|track| track.around(time))
                    .map_or(bind.rotation, |(a, b, k)| slerp(a.map(f64::from), b.map(f64::from), f64::from(k)));
                // Rotations are summed on one hemisphere.
                let reference = if blended == [0.; 4] { q } else { blended };
                let sign = if dot4(q, reference) < 0. { -1. } else { 1. };
                for i in 0..4 {
                    blended[i] += share * sign * q[i];
                }
            }
            for r in 0..3 {
                position[r] += (0..3).map(|c| rotation[r][c] * translation[c]).sum::<f64>();
            }
            rotation = multiply(rotation, rotation_matrix(blended));
        }
        Some([
            [
                rotation[0][0] as f32,
                rotation[0][1] as f32,
                rotation[0][2] as f32,
                position[0] as f32,
            ],
            [
                rotation[1][0] as f32,
                rotation[1][1] as f32,
                rotation[1][2] as f32,
                position[1] as f32,
            ],
            [
                rotation[2][0] as f32,
                rotation[2][1] as f32,
                rotation[2][2] as f32,
                position[2] as f32,
            ],
        ])
    }
}

fn dot4(a: [f64; 4], b: [f64; 4]) -> f64 {
    (0..4).map(|i| a[i] * b[i]).sum()
}

/// Spherical blend of two unit quaternions, the shorter way round.
fn slerp(a: [f64; 4], mut b: [f64; 4], t: f64) -> [f64; 4] {
    let mut d = dot4(a, b);
    if d < 0. {
        b = b.map(|c| -c);
        d = -d;
    }
    if d > 0.9995 {
        let mixed: [f64; 4] = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
        let n = dot4(mixed, mixed).sqrt();
        return mixed.map(|c| c / n);
    }
    let theta = d.acos();
    let (wa, wb) = (((1. - t) * theta).sin(), (t * theta).sin());
    let sine = theta.sin();
    std::array::from_fn(|i| (wa * a[i] + wb * b[i]) / sine)
}

/// The head's world matrix: the actor's pose (a rotation matrix with the position `feet`, cm)
/// applied to the head's matrix in the actor's frame.
pub fn world_matrix(actor: &Mat34, local: &Mat34) -> Mat34 {
    let mut out = [[0.; 4]; 3];
    for r in 0..3 {
        for c in 0..3 {
            out[r][c] = (0..3).map(|k| actor[r][k] * local[k][c]).sum();
        }
        out[r][3] = (0..3).map(|k| actor[r][k] * local[k][3]).sum::<f32>() + actor[r][3];
    }
    out
}

/// The matrix's translation.
pub fn translation(m: &Mat34) -> Vec3 {
    Vec3::new(m[0][3], m[1][3], m[2][3])
}
