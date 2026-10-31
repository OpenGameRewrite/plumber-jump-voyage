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

/// The 8-bit joints in the file's order (parents before their children) with their world
/// matrices for a clip at a frame under the actor's matrix.
pub fn world_pose(actor: &Mat34, clip: &str, frame: f32) -> Option<Vec<(&'static str, Mat34)>> {
    let curves = Curves::installed_2d()?;
    let clip = curves.clip(clip)?;
    let mut worlds: Vec<Mat34> = Vec::with_capacity(curves.joint_count());
    let mut named = Vec::with_capacity(curves.joint_count());
    for joint in 0..curves.joint_count() {
        let local = local_matrix(&curves.pose(clip, joint, frame)?);
        let above = curves.parent(joint).map_or(*actor, |parent| worlds[parent]);
        worlds.push(mul34(&above, &local));
        named.push((curves.joint_name(joint)?, worlds[joint]));
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

/// How many frames the 8-bit skeleton's clip lasts (the archive's own count), e.g. the
/// one-frame `Jump` and `Fall` poses whose visibility actions run five.
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
pub fn clip_frames(clip: &str) -> Option<f32> {
    Some(Curves::installed_2d()?.clip(clip)?.frames())
}
