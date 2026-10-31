//! The 8-bit Mario (`Mario2D`): its own skeleton and how its clips pose it.
//!
//! The model is a separate rig from the 27-joint body: `AllRoot`, `MarioDot` and the nine
//! `MarioDot00..08` joints under it, each part a flat sprite of voxels skinned to its own
//! joint. Its bind pose is the identity everywhere, and `PlayerAnimation2D`'s clips move the
//! joints only where a clip needs to (the pipe clips slide the whole figure along the mouth);
//! which sprite is on screen is a separate channel, the clips' visibility tracks. There is no
//! blending: the archive carries no `AnimInterp` resource, so a clip change is a cut.
//!
//! The joints and curves come from `Mario2D.curves.bin` (docs/asset-format.md, "Animation
//! curves"); without the file nothing here is available and callers keep the figure at the actor.
use crate::clip_curves::{Curves, Sample};
use crate::movement::look_joint::Mat34;
use crate::skeleton_pose::{IDENTITY, mul34, rotation_matrix};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The model next to its curves; its `extras.expressions` are the original visibility actions.
const MODEL: &str = "local/Mario2D.glb";
/// The joint whose visibility hides or shows the whole figure: the part joints hang under it.
const FIGURE_JOINT: &str = "MarioDot";

/// The 8-bit joints in the file's order (parents before their children) with their pose
/// relative to their parent for a clip at a frame.
pub fn locals(clip: &str, frame: f32) -> Option<Vec<(&'static str, Sample)>> {
    let curves = Curves::installed_2d()?;
    let clip = curves.clip(clip)?;
    (0..curves.joint_count())
        .map(|joint| Some((curves.joint_name(joint)?, curves.pose(clip, joint, frame)?)))
        .collect()
}

/// The joints' world matrices, in the file's order, for a clip at a frame under the actor's
/// matrix.
pub fn world_pose(actor: &Mat34, clip: &str, frame: f32) -> Option<Vec<(&'static str, Mat34)>> {
    let curves = Curves::installed_2d()?;
    let mut worlds: Vec<Mat34> = Vec::with_capacity(curves.joint_count());
    let mut named = Vec::with_capacity(curves.joint_count());
    for (joint, (name, sample)) in locals(clip, frame)?.into_iter().enumerate() {
        let above = curves.parent(joint).map_or(*actor, |parent| worlds[parent]);
        worlds.push(mul34(&above, &local_matrix(&sample)));
        named.push((name, worlds[joint]));
    }
    Some(named)
}

/// A joint's matrix relative to its parent: scale, then rotation, then translation.
fn local_matrix(sample: &Sample) -> Mat34 {
    let rotation = rotation_matrix(sample.rotation);
    let mut matrix = IDENTITY;
    for row in 0..3 {
        for column in 0..3 {
            matrix[row][column] = (rotation[row][column] * sample.scale[column]) as f32;
        }
        matrix[row][3] = sample.translation[row] as f32;
    }
    matrix
}

/// One visibility action of PlayerAnimation2D (what the converter writes as an expression): per
/// joint, whether it shows on each frame, a sample for every frame from 0 to the action's length.
#[derive(Deserialize)]
struct Action {
    looping: bool,
    tracks: BTreeMap<String, Vec<bool>>,
}

#[derive(Deserialize)]
struct Extras {
    expressions: BTreeMap<String, Action>,
}

#[derive(Deserialize)]
struct Document {
    extras: Extras,
}

fn actions() -> Option<&'static BTreeMap<String, Action>> {
    static ACTIONS: OnceLock<Option<BTreeMap<String, Action>>> = OnceLock::new();
    ACTIONS
        .get_or_init(|| {
            let bytes = std::fs::read(crate::local_assets::asset_dir().join(MODEL)).ok()?;
            let length = u32::from_le_bytes(bytes.get(12..16)?.try_into().ok()?) as usize;
            let document: Document = serde_json::from_slice(bytes.get(20..20 + length)?).ok()?;
            Some(document.extras.expressions)
        })
        .as_ref()
}

/// The length of an action in frames, the animator's counter, and whether it loops there. The
/// visibility tracks carry a sample per frame including the last, so the length is one less.
pub fn action_length(action: &str) -> Option<(u32, bool)> {
    let action = actions()?.get(action)?;
    let samples = action.tracks.values().map(Vec::len).max()?;
    Some((u32::try_from(samples.checked_sub(1)?).ok()?, action.looping))
}

/// Whether a part joint shows on a frame of an action: both the figure joint above all parts and
/// the part's own track must be on. A looping action wraps; a held one keeps its last sample.
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
pub fn joint_visible(action: &str, frame: u32, joint: &str) -> Option<bool> {
    let action = actions()?.get(action)?;
    let shown = |track: &str| {
        let samples = action.tracks.get(track)?;
        let last = samples.len().checked_sub(1)?;
        let frame = frame as usize;
        let at = if action.looping && last > 0 {
            frame % last
        } else {
            frame.min(last)
        };
        Some(samples[at])
    };
    Some(shown(FIGURE_JOINT)? && shown(joint)?)
}

/// How many frames the 8-bit skeleton's clip lasts (the archive's own count), e.g. the
/// one-frame `Jump` and `Fall` poses whose visibility actions run five.
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
pub fn clip_frames(clip: &str) -> Option<f32> {
    Some(Curves::installed_2d()?.clip(clip)?.frames())
}
