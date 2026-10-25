//! The animated skeleton: where the model's joints stand for a given blend of clips, before and
//! after the joint controllers have bent them.
//!
//! The windowed game poses the skeleton in the renderer, but the headless trace has no
//! skeleton, so the trace evaluates the joints itself from the clips of the player's own
//! installation (the converter writes them into `assets/local/Mario.glb`, which this module
//! reads the first time a pose is asked for; nothing of it is kept in the repository). Nothing
//! is returned when the file or a needed clip is missing.
//!
//! The joints are the ones the probe logs in `bones` (`nw4f_root` down to `HandR`): each takes
//! its translation and rotation from the clips (the bind pose where a clip has no key for it),
//! blended by the clips' weights (translations linearly, rotations as a normalised weighted
//! sum), and the clips are sampled at 60 keys a second, linear between keys.
//!
//! `world_pose` then walks the joints parents first, as the model's world calculation does: a
//! joint's world matrix is its parent's times its own, and the player's joint controllers
//! (`PlayerJointControlKeeper`) are given the matrix to bend before its children use it, in
//! the same order (`al::JointControllerKeeper::Exec`).
use crate::movement::look_joint::Mat34;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// The model the trace poses.
const MODEL: &str = "local/Mario.glb";
/// The clips are sampled at this many keys a second.
const KEYS_PER_SECOND: f32 = 60.;
/// The converter prefixes the joints' names with the armature's.
const JOINT_PREFIX: &str = "Armature_";

/// The joints the probe logs, in its order (the model's joint order: a joint comes after its
/// parent).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
// (The names are the model's own: `JointRoot` is a joint of the skeleton, not of this enum.)
#[allow(clippy::enum_variant_names)]
pub enum Joint {
    Nw4fRoot,
    AllRoot,
    JointRoot,
    Hip,
    LegL1,
    LegL2,
    FootL,
    ToeL,
    LegR1,
    LegR2,
    FootR,
    ToeR,
    Spine1,
    Spine2,
    Head,
    ShoulderL,
    ArmL1,
    ArmL1Sub,
    ArmL2,
    ArmL2Sub,
    HandL,
    ShoulderR,
    ArmR1,
    ArmR1Sub,
    ArmR2,
    ArmR2Sub,
    HandR,
}

pub const JOINT_COUNT: usize = 27;

impl Joint {
    pub const ALL: [Joint; JOINT_COUNT] = [
        Joint::Nw4fRoot,
        Joint::AllRoot,
        Joint::JointRoot,
        Joint::Hip,
        Joint::LegL1,
        Joint::LegL2,
        Joint::FootL,
        Joint::ToeL,
        Joint::LegR1,
        Joint::LegR2,
        Joint::FootR,
        Joint::ToeR,
        Joint::Spine1,
        Joint::Spine2,
        Joint::Head,
        Joint::ShoulderL,
        Joint::ArmL1,
        Joint::ArmL1Sub,
        Joint::ArmL2,
        Joint::ArmL2Sub,
        Joint::HandL,
        Joint::ShoulderR,
        Joint::ArmR1,
        Joint::ArmR1Sub,
        Joint::ArmR2,
        Joint::ArmR2Sub,
        Joint::HandR,
    ];

    /// The joint's name in the model (and in the probe's `bones`).
    pub fn name(self) -> &'static str {
        const NAMES: [&str; JOINT_COUNT] = [
            "nw4f_root",
            "AllRoot",
            "JointRoot",
            "Hip",
            "LegL1",
            "LegL2",
            "FootL",
            "ToeL",
            "LegR1",
            "LegR2",
            "FootR",
            "ToeR",
            "Spine1",
            "Spine2",
            "Head",
            "ShoulderL",
            "ArmL1",
            "ArmL1Sub",
            "ArmL2",
            "ArmL2Sub",
            "HandL",
            "ShoulderR",
            "ArmR1",
            "ArmR1Sub",
            "ArmR2",
            "ArmR2Sub",
            "HandR",
        ];
        NAMES[self as usize]
    }

    /// The joint a node of the model is, by its name in the converted file.
    #[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
    pub fn from_node_name(name: &str) -> Option<Joint> {
        let short = name.strip_prefix(JOINT_PREFIX)?;
        Joint::ALL.into_iter().find(|joint| joint.name() == short)
    }

    /// The joint above this one in the skeleton (none for the root).
    pub fn parent(self) -> Option<Joint> {
        use Joint::*;
        Some(match self {
            Nw4fRoot => return None,
            AllRoot => Nw4fRoot,
            JointRoot => AllRoot,
            Hip | Spine1 => JointRoot,
            LegL1 | LegR1 => Hip,
            LegL2 => LegL1,
            FootL => LegL2,
            ToeL => FootL,
            LegR2 => LegR1,
            FootR => LegR2,
            ToeR => FootR,
            Spine2 => Spine1,
            Head | ShoulderL | ShoulderR => Spine2,
            ArmL1 => ShoulderL,
            ArmL1Sub | ArmL2 => ArmL1,
            ArmL2Sub | HandL => ArmL2,
            ArmR1 => ShoulderR,
            ArmR1Sub | ArmR2 => ArmR1,
            ArmR2Sub | HandR => ArmR2,
        })
    }
}

/// The joint the model's partial animation group hangs from: Mario's model adds `Spine1` and
/// everything below it to partial group 0 (`al::addPartialSklAnimPartsListRecursive`, in the
/// model's construction), so `startUpperBodyAnim` plays on those joints only.
const PARTIAL_GROUP_ROOT: Joint = Joint::Spine1;

impl Joint {
    /// Whether this is the joint the partial animation's group hangs from.
    #[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
    pub fn is_partial_group_root(self) -> bool {
        self == PARTIAL_GROUP_ROOT
    }
    /// Whether the partial animation's group (`PARTIAL_GROUP_ROOT` and its descendants) holds
    /// the joint.
    pub fn in_partial_group(self) -> bool {
        let mut joint = Some(self);
        while let Some(current) = joint {
            if current == PARTIAL_GROUP_ROOT {
                return true;
            }
            joint = current.parent();
        }
        false
    }
}

/// Which joints a layer of the pose speaks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// Every joint: a clip of the main blend nobody overlays.
    Whole,
    /// The joints outside the partial group: a main clip whose upper body a partial animation
    /// replaces.
    Lower,
    /// The partial group's joints: the partial animation itself.
    Upper,
}

impl Region {
    fn covers(self, joint: Joint) -> bool {
        match self {
            Region::Whole => true,
            Region::Lower => !joint.in_partial_group(),
            Region::Upper => joint.in_partial_group(),
        }
    }
}

/// Every joint's matrix: local ones from the clips, world ones from `world_pose`.
pub type Matrices = [Mat34; JOINT_COUNT];

/// What bends a joint's world matrix once it is computed (the model's joint controllers).
pub trait JointCalc {
    /// `world` is the joint's matrix, replaced by the bent one; `solved` holds the world
    /// matrices of the joints before this one (and may be written, as
    /// `alModelJointFunction::setDirectJointMtx` does for the legs).
    fn calc(&mut self, joint: Joint, world: &mut Mat34, solved: &mut Matrices);
}

/// No controllers: the skeleton as the animation leaves it.
pub struct Unbent;

impl JointCalc for Unbent {
    fn calc(&mut self, _: Joint, _: &mut Mat34, _: &mut Matrices) {}
}

pub const IDENTITY: Mat34 = [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]];

fn invert3(m: &Mat34) -> [[f32; 3]; 3] {
    let c = |r: usize, k: usize| m[r][k];
    let cof = |a: usize, b: usize| {
        let (r1, r2) = ((a + 1) % 3, (a + 2) % 3);
        let (c1, c2) = ((b + 1) % 3, (b + 2) % 3);
        c(r1, c1) * c(r2, c2) - c(r1, c2) * c(r2, c1)
    };
    let det = c(0, 0) * cof(0, 0) + c(0, 1) * cof(0, 1) + c(0, 2) * cof(0, 2);
    let mut out = [[0.; 3]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        for (k, cell) in row.iter_mut().enumerate() {
            *cell = cof(k, r) / det;
        }
    }
    out
}

/// `a` times `b`, both affine (a rotation or scale part and a translation column).
pub fn mul34(a: &Mat34, b: &Mat34) -> Mat34 {
    let mut out = [[0.; 4]; 3];
    for r in 0..3 {
        for c in 0..3 {
            out[r][c] = (a[r][0] * b[0][c] + a[r][1] * b[1][c]) + a[r][2] * b[2][c];
        }
        out[r][3] = ((a[r][0] * b[0][3] + a[r][1] * b[1][3]) + a[r][2] * b[2][3]) + a[r][3];
    }
    out
}

/// The lengths of a matrix's three columns: the scale a joint carries.
fn column_lengths(m: &Mat34) -> [f32; 3] {
    [0, 1, 2].map(|c| ((m[0][c] * m[0][c] + m[1][c] * m[1][c]) + m[2][c] * m[2][c]).sqrt())
}

impl Joint {
    /// Whether the joint's bone has the skeleton's segment scale compensation (the
    /// `SegmentScaleCompensate` bone flag, 0x800000, of `Mario.bfres`; the skeleton's scaling
    /// mode is Maya). Every bone has it except the three roots, which take what is above them.
    fn compensates_segment_scale(self) -> bool {
        !matches!(self, Joint::Nw4fRoot | Joint::AllRoot | Joint::JointRoot)
    }

    /// The scale of the joint above that this joint's rule works with, from the parent's true
    /// scale `above`: the parent's for a compensating joint, none for a root.
    fn compensated_scale(self, above: [f32; 3]) -> [f32; 3] {
        if self.compensates_segment_scale() {
            above
        } else {
            [1.; 3]
        }
    }
}

/// A joint's world matrix from its parent's `above` and its own `local` one, with the scale
/// rule of the model's skeleton (Maya's segment scale compensation).
///
/// A compensating joint is placed as `above * T * inverse(parent scale) * R * S`: the parent's
/// own scale (`parent_scale`, its local one, not what the joints above it add) carries the
/// joint's offset farther or nearer, but neither shears nor scales the joint's own frame, so
/// a stretched upper arm moves the elbow out along with it while the forearm keeps its
/// proportions. The roots have no compensation: they inherit the scale above them as a plain
/// product does. A compensating joint's `local` columns carry the converted scale (the file
/// stores its scale divided by its parent's, glTF fashion), which times `parent_scale` is the
/// true one; `parent_scale` is 1 for a root, whose scale is stored as it is.
fn place(above: &Mat34, local: &Mat34, parent_scale: [f32; 3]) -> Mat34 {
    if parent_scale == [1.; 3] || parent_scale.contains(&0.) {
        return mul34(above, local);
    }
    let mut free = *above;
    let mut carried_local = *local;
    for c in 0..3 {
        for r in 0..3 {
            free[r][c] = above[r][c] / parent_scale[c];
            carried_local[r][c] = local[r][c] * parent_scale[c];
        }
    }
    let rotated = mul34(&free, &carried_local);
    // The offset is the plain product's: it takes the parent's scale.
    let mut out = mul34(above, local);
    for r in 0..3 {
        out[r][..3].copy_from_slice(&rotated[r][..3]);
    }
    out
}

/// The world matrices of all joints for the local ones in `locals` under the actor's matrix
/// `actor`, walking the joints parents first and giving `calc` each matrix before its children
/// are computed from it.
pub fn world_pose(actor: &Mat34, locals: &Matrices, calc: &mut impl JointCalc) -> Matrices {
    let mut worlds = [IDENTITY; JOINT_COUNT];
    let mut scales = [[1.; 3]; JOINT_COUNT];
    for joint in Joint::ALL {
        let above = joint.parent().map_or(*actor, |p| worlds[p as usize]);
        let parent_scale =
            joint.compensated_scale(joint.parent().map_or([1.; 3], |p| scales[p as usize]));
        let local = &locals[joint as usize];
        let own = column_lengths(local);
        scales[joint as usize] = [0, 1, 2].map(|c| own[c] * parent_scale[c]);
        let mut world = place(&above, local, parent_scale);
        calc.calc(joint, &mut world, &mut worlds);
        worlds[joint as usize] = world;
    }
    worlds
}

/// The inverse of an affine matrix.
fn invert34(m: &Mat34) -> Mat34 {
    let inverse = invert3(m);
    let mut out = [[0.; 4]; 3];
    for r in 0..3 {
        for c in 0..3 {
            out[r][c] = inverse[r][c];
        }
        out[r][3] =
            -((inverse[r][0] * m[0][3] + inverse[r][1] * m[1][3]) + inverse[r][2] * m[2][3]);
    }
    out
}

/// The local matrices a skeleton the controllers bent needs to show that pose when the
/// renderer composes the joints itself: only the joints a controller moved (the legs' own
/// matrices included) or that the rig places differently from a plain composition (a parent
/// with a scale of its own, see `place`) change; the others keep their animated local matrix
/// and follow their bent parents. `bent` is what `world_pose` returned for `locals`.
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
pub fn bent_locals(actor: &Mat34, locals: &Matrices, bent: &Matrices) -> Vec<(Joint, Mat34)> {
    let mut moved = Vec::new();
    for joint in Joint::ALL {
        let above = joint.parent().map_or(*actor, |p| bent[p as usize]);
        // What the renderer's own composition makes of the animated local matrix.
        let expected = mul34(&above, &locals[joint as usize]);
        let same = expected
            .iter()
            .flatten()
            .zip(bent[joint as usize].iter().flatten())
            .all(|(a, b)| (a - b).abs() <= COMPOSITION_TOLERANCE * (1. + b.abs()));
        if !same {
            moved.push((joint, mul34(&invert34(&above), &bent[joint as usize])));
        }
    }
    moved
}

/// How far (relative, f32 rounding of two ways to the same product) the rig's placement of a
/// joint may be from the renderer's plain composition before the joint is written back.
const COMPOSITION_TOLERANCE: f32 = 1e-5;

/// One clip playing: its name, the frame it shows and its share of the blend.
///
/// A partial animation (`partial0`) does not join the blend of the joints it covers, it
/// replaces it (`AnimPlayerSkl::calcSklAnim` clears the blender's result for the group's
/// joints before blending the partial in with weight 1). So the main clips under one are
/// `Lower` layers, the partial a `Upper` one, and a joint is the weighted blend of the layers
/// that cover it. Layers faded out together (a frozen pose) keep their regions: each joint
/// normalises over the weights of the layers that cover it.
#[derive(Clone, Copy, Debug)]
pub struct PoseLayer<'a> {
    pub clip: &'a str,
    pub frame: f32,
    pub weight: f32,
    pub region: Region,
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
    scale: Option<[f32; 3]>,
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

/// A joint's local pose: translation, rotation (x, y, z, w) and scale.
#[derive(Clone, Copy)]
struct Local {
    translation: [f64; 3],
    rotation: [f64; 4],
    scale: [f64; 3],
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
    scale: Vec<Option<Track<3>>>,
}

pub struct SkeletonPose {
    document: Document,
    bytes: Vec<u8>,
    bin_start: usize,
    /// The glTF node of each joint (in `Joint` order) and each joint's bind pose.
    nodes: Vec<usize>,
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

impl SkeletonPose {
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
        let nodes: Vec<usize> = Joint::ALL
            .iter()
            .map(|joint| find(&format!("{JOINT_PREFIX}{}", joint.name())))
            .collect::<Option<_>>()?;
        let bind = nodes
            .iter()
            .map(|&i| {
                let node = &document.nodes[i];
                if let Some(m) = node.matrix {
                    let m = m.map(f64::from);
                    // Column-major: the first three columns are the axes, the fourth the offset.
                    let columns = [[m[0], m[1], m[2]], [m[4], m[5], m[6]], [m[8], m[9], m[10]]];
                    let length = |c: [f64; 3]| (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
                    Local {
                        translation: [m[12], m[13], m[14]],
                        rotation: matrix_quaternion(columns),
                        scale: columns.map(length),
                    }
                } else {
                    Local {
                        translation: node.translation.unwrap_or([0.; 3]).map(f64::from),
                        rotation: node.rotation.unwrap_or([0., 0., 0., 1.]).map(f64::from),
                        scale: node.scale.unwrap_or([1.; 3]).map(f64::from),
                    }
                }
            })
            .collect();
        Some(Self {
            document,
            bytes,
            bin_start,
            nodes,
            bind,
            clips: Mutex::new(HashMap::new()),
        })
    }

    /// The installation's model, read once; `None` without game data.
    pub fn installed() -> Option<&'static SkeletonPose> {
        static MODEL_POSE: OnceLock<Option<SkeletonPose>> = OnceLock::new();
        MODEL_POSE.get_or_init(Self::open).as_ref()
    }

    fn floats<const N: usize>(&self, accessor: usize) -> Option<Vec<[f32; N]>> {
        let a = self.document.accessors.get(accessor)?;
        let view = self.document.buffer_views.get(a.buffer_view)?;
        let start = self.bin_start + view.byte_offset.unwrap_or(0) + a.byte_offset.unwrap_or(0);
        let raw = self.bytes.get(start..start + a.count * N * 4)?;
        let numbers: Vec<f32> = raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| f32::from_le_bytes(*bytes))
            .collect();
        Some(numbers.as_chunks::<N>().0.to_vec())
    }

    fn track<const N: usize>(
        &self,
        animation: &AnimationDef,
        channel: &ChannelDef,
    ) -> Option<Track<N>> {
        let sampler = animation.samplers.get(channel.sampler)?;
        let times = self
            .floats::<1>(sampler.input)?
            .into_iter()
            .map(|t| t[0])
            .collect();
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
            translation: (0..self.nodes.len()).map(|_| None).collect(),
            rotation: (0..self.nodes.len()).map(|_| None).collect(),
            scale: (0..self.nodes.len()).map(|_| None).collect(),
        };
        for channel in &animation.channels {
            let Some(slot) = channel
                .target
                .node
                .and_then(|n| self.nodes.iter().position(|c| *c == n))
            else {
                continue;
            };
            match channel.target.path.as_str() {
                "translation" => tracks.translation[slot] = self.track::<3>(animation, channel),
                "rotation" => tracks.rotation[slot] = self.track::<4>(animation, channel),
                "scale" => tracks.scale[slot] = self.track::<3>(animation, channel),
                _ => {}
            }
        }
        Some(tracks)
    }

    /// Every joint's matrix relative to its parent (cm), for the blend of `layers`; `None` when
    /// no layer's clip is in the model. Each joint blends the layers whose region covers it,
    /// by their weights.
    pub fn locals(&self, layers: &[PoseLayer]) -> Option<Matrices> {
        let playing: Vec<(std::sync::Arc<ClipTracks>, &PoseLayer)> = layers
            .iter()
            .filter(|l| l.weight > 0.)
            .filter_map(|l| Some((self.clip(l.clip)?, l)))
            .collect();
        if playing.is_empty() {
            return None;
        }
        // The scale each joint truly has in each clip: the file stores a joint's scale divided
        // by its parent's (see `place`), so it is multiplied back down the chain. The game
        // blends the true scales, so the blend is made of those and converted afterwards.
        let compensated = |joint: Joint, scales: &[[f64; 3]]| match joint
            .parent()
            .filter(|_| joint.compensates_segment_scale())
        {
            Some(parent) => scales[parent as usize],
            None => [1.; 3],
        };
        let true_scales: Vec<[[f64; 3]; JOINT_COUNT]> = playing
            .iter()
            .map(|(clip, layer)| {
                let time = layer.frame / KEYS_PER_SECOND;
                let mut scales = [[1.; 3]; JOINT_COUNT];
                for (slot, joint) in Joint::ALL.iter().enumerate() {
                    scales[slot] = match clip.scale[slot].as_ref().and_then(|t| t.around(time)) {
                        Some((a, b, k)) => {
                            let above = compensated(*joint, &scales);
                            std::array::from_fn(|i| {
                                (f64::from(a[i])
                                    + (f64::from(b[i]) - f64::from(a[i])) * f64::from(k))
                                    * above[i]
                            })
                        }
                        None => self.bind[slot].scale,
                    };
                }
                scales
            })
            .collect();
        // The layers each joint blends, with their shares of its total weight.
        let shares: Vec<Vec<(usize, f64)>> = Joint::ALL
            .iter()
            .map(|joint| {
                let covering: Vec<(usize, f64)> = playing
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, layer))| layer.region.covers(*joint))
                    .map(|(i, (_, layer))| (i, f64::from(layer.weight)))
                    .collect();
                let total: f64 = covering.iter().map(|(_, w)| w).sum();
                covering.into_iter().map(|(i, w)| (i, w / total)).collect()
            })
            .collect();
        let blended_scales: Vec<[f64; 3]> = (0..JOINT_COUNT)
            .map(|slot| {
                let mut blend = [0.; 3];
                for (i, share) in &shares[slot] {
                    for k in 0..3 {
                        blend[k] += share * true_scales[*i][slot][k];
                    }
                }
                if shares[slot].is_empty() {
                    self.bind[slot].scale
                } else {
                    blend
                }
            })
            .collect();
        let mut out = [IDENTITY; JOINT_COUNT];
        for (slot, bind) in self.bind.iter().enumerate() {
            let mut translation = [0.; 3];
            let mut blended = [0.; 4];
            for (i, share) in &shares[slot] {
                let (clip, layer) = &playing[*i];
                let time = layer.frame / KEYS_PER_SECOND;
                let t = clip.translation[slot]
                    .as_ref()
                    .and_then(|track| track.around(time))
                    .map_or(bind.translation, |(a, b, k)| {
                        std::array::from_fn(|i| {
                            f64::from(a[i]) + (f64::from(b[i]) - f64::from(a[i])) * f64::from(k)
                        })
                    });
                for k in 0..3 {
                    translation[k] += share * t[k];
                }
                let q = clip.rotation[slot]
                    .as_ref()
                    .and_then(|track| track.around(time))
                    .map_or(bind.rotation, |(a, b, k)| {
                        slerp(a.map(f64::from), b.map(f64::from), f64::from(k))
                    });
                // Rotations are summed on one hemisphere.
                let reference = if blended == [0.; 4] { q } else { blended };
                let sign = if dot4(q, reference) < 0. { -1. } else { 1. };
                for k in 0..4 {
                    blended[k] += share * sign * q[k];
                }
            }
            if shares[slot].is_empty() {
                blended = bind.rotation;
                translation = bind.translation;
            }
            // Back to the file's convention: the blended scale over the blended parent's.
            let above = compensated(Joint::ALL[slot], &blended_scales);
            let scale: [f64; 3] = std::array::from_fn(|i| {
                if above[i] == 0. {
                    blended_scales[slot][i]
                } else {
                    blended_scales[slot][i] / above[i]
                }
            });
            let rotation = rotation_matrix(blended);
            for r in 0..3 {
                for c in 0..3 {
                    out[slot][r][c] = (rotation[r][c] * scale[c]) as f32;
                }
                out[slot][r][3] = translation[r] as f32;
            }
        }
        Some(out)
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

#[path = "tests/skeleton_pose.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
