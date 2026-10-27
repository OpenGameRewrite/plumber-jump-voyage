use bevy::prelude::*;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct Outfit {
    pub name: String,
    pub file: String,
    #[serde(default = "one")]
    pub scale: f32,
    #[serde(default)]
    pub yaw: f32,
    #[serde(default)]
    #[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
    pub animations: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub cap_file: Option<String>,
    /// Another installed file whose clips this outfit plays (they are shared by
    /// bone path instead of being copied into every outfit).
    #[serde(default)]
    pub animation_file: Option<String>,
    /// Discrete mesh visibility from the original eye/head action sets.
    #[serde(default)]
    pub expressions: std::sync::Arc<std::collections::BTreeMap<String, Expression>>,
}
#[derive(Clone, Deserialize)]
pub struct Expression {
    pub looping: bool,
    pub tracks: std::collections::BTreeMap<String, Vec<bool>>,
    #[serde(default)]
    pub uv: std::collections::BTreeMap<String, Vec<[f32; 3]>>,
}
impl Outfit {
    #[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
    fn part_visibility(&self, clip: &str, seconds: f32, part: &str) -> Option<bool> {
        let expression = self
            .expressions
            .get(clip)
            .or_else(|| self.expressions.get("Wait"))?;
        let frames = expression.tracks.get(part)?;
        if frames.is_empty() {
            return None;
        }
        let frame = (seconds.max(0.) * 60.).floor() as usize;
        let frame = if expression.looping && frames.len() > 1 {
            frame % (frames.len() - 1)
        } else {
            frame.min(frames.len() - 1)
        };
        Some(frames[frame])
    }
    #[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
    fn pupil_uv(&self, clip: &str, seconds: f32, material: &str) -> Option<bevy::math::Affine2> {
        let expression = self
            .expressions
            .get(clip)
            .or_else(|| self.expressions.get("Wait"))?;
        let frames = expression.uv.get(material)?;
        if frames.is_empty() {
            return None;
        }
        let at = (seconds.max(0.) * 60.).min(usize::MAX as f32);
        let sample = |i: usize| {
            frames[if expression.looping && frames.len() > 1 {
                i % (frames.len() - 1)
            } else {
                i.min(frames.len() - 1)
            }]
        };
        let a = Vec3::from_array(sample(at.floor() as usize));
        let b = Vec3::from_array(sample((at.floor() as usize).saturating_add(1)));
        let srt = a.lerp(b, at.fract());
        // Maya's unit-scale texture matrix, with V flipped by the GLB exporter.
        let rotation = bevy::math::Mat2::from_angle(-srt.x);
        let offset = Vec2::splat(0.5) - rotation * (Vec2::splat(0.5) + Vec2::new(srt.y, -srt.z));
        Some(bevy::math::Affine2::from_mat2_translation(rotation, offset))
    }
}
fn one() -> f32 {
    1.
}
/// True while an original model is shown: its shadow then comes from the sun
/// alone, as in the game, and the procedural drop-shadow disc stays hidden.
#[derive(Resource, Default)]
pub struct CastsOwnShadow(pub bool);
pub const EXPLORERS: [&str; 3] = ["Terrace explorer", "Sunset explorer", "Midnight explorer"];
#[derive(Resource)]
pub struct Wardrobe {
    pub outfits: Vec<Outfit>,
    pub status: String,
}
impl Wardrobe {
    /// Display order: local outfits first when present, then the explorers.
    pub fn slot(&self, row: usize) -> usize {
        let n = self.outfits.len();
        if n == 0 {
            row
        } else if row < n {
            row + 3
        } else {
            row - n
        }
    }
    pub fn row(&self, slot: usize) -> usize {
        (0..self.outfits.len() + 3)
            .find(|r| self.slot(*r) == slot)
            .unwrap_or(0)
    }
    pub fn name(&self, slot: usize) -> String {
        if slot < 3 {
            format!("{}  (ROMless)", EXPLORERS[slot])
        } else {
            self.outfits
                .get(slot - 3)
                .map(|o| o.name.clone())
                .unwrap_or_else(|| "Unavailable outfit".into())
        }
    }
}
#[cfg(feature = "local-assets")]
#[derive(Deserialize)]
struct Manifest {
    outfits: Vec<Outfit>,
    #[serde(default)]
    expressions: std::collections::BTreeMap<String, Expression>,
}
pub fn validate(outfit: &Outfit) -> bool {
    let p = std::path::Path::new(&outfit.file);
    !outfit.name.trim().is_empty()
        && p.is_relative()
        && !p
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        && outfit.file.starts_with("local/")
        && outfit.file.ends_with(".glb")
        && outfit.scale.is_finite()
        && outfit.scale > 0.
        && outfit.scale < 100.
        && outfit.yaw.is_finite()
        && outfit
            .animation_file
            .as_ref()
            .is_none_or(|p| safe_local(p, "local/"))
        && outfit.cap_file.as_ref().is_none_or(|p| {
            p.starts_with("local/caps/")
                && p.ends_with(".glb")
                && !p.contains("..")
                && !p.contains('\\')
                && !p.contains(':')
        })
}
fn safe_local(p: &str, prefix: &str) -> bool {
    p.starts_with(prefix) && p.ends_with(".glb") && !p.contains("..") && !p.contains(['\\', ':'])
}
pub fn asset_dir() -> std::path::PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default().join("assets");
    if cwd.is_dir() {
        cwd
    } else {
        std::env::current_exe()
            .unwrap_or_default()
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("assets")
    }
}
pub fn plugin(app: &mut App) {
    #[allow(unused_mut)]
    let mut wardrobe = Wardrobe {
        outfits: vec![],
        status: "ROMless build: original explorer and scenery".into(),
    };
    #[cfg(feature = "local-assets")]
    match std::fs::read(asset_dir().join("local/manifest.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Manifest>(&b).ok())
    {
        Some(m) if !m.outfits.is_empty() && m.outfits.iter().all(validate) => {
            wardrobe.outfits = m.outfits;
            // All converted costumes share one action table. Keep it once in
            // the manifest and in memory; custom GLBs may supply an override.
            let expressions = std::sync::Arc::new(m.expressions);
            for outfit in &mut wardrobe.outfits {
                if outfit.expressions.is_empty() {
                    outfit.expressions = expressions.clone();
                }
            }
            wardrobe.status = format!(
                "Game-data build: {} original outfits installed",
                wardrobe.outfits.len()
            );
        }
        _ => {
            wardrobe.status =
                "Game-data build, but no outfits are installed. See the README.".into()
        }
    }
    if let Some(mut settings) = app.world_mut().get_resource_mut::<crate::input::Settings>() {
        // The game-data build starts in an original outfit unless the player
        // picked something else in this build before.
        if !wardrobe.outfits.is_empty() {
            settings.outfit = settings
                .local_outfit
                .filter(|o| *o < 3 + wardrobe.outfits.len())
                .unwrap_or(3);
        }
    }
    app.insert_resource(wardrobe)
        .init_resource::<CastsOwnShadow>()
        .add_systems(Update, colors);
    #[cfg(feature = "local-assets")]
    imported::plugin(app);
}
fn colors(
    settings: Res<crate::input::Settings>,
    palette: Res<crate::world::Palette>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if settings.is_changed()
        && let Some(mut mat) = materials.get_mut(&palette.0)
    {
        mat.base_color = match settings.outfit {
            1 => Color::srgb(0.86, 0.39, 0.17),
            2 => Color::srgb(0.25, 0.24, 0.47),
            _ => Color::srgb(0.10, 0.52, 0.53),
        };
    }
}
/// Frames the original blends over when a clip starts (its interpolation
/// table; anything unlisted starts at once). Jumps out of the run cycle use
/// the table's short "interp" entries.
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
pub fn interp_frames(name: &str, from: &str) -> f32 {
    match name {
        "Jump" if from == "Move" => 3.,
        "Move" | "RunStart" | "Walk" | "Run" | "Dash" | "Push" | "WallCatch" | "WallCatchEnd" => 5.,
        "Wait" | "Fall" | "WaitRelaxStart" => 6.,
        // PlayerAnimation/AnimInterp.byml.
        "DeadFall" | "WaitHot" | "TractorBubbleWait" => 10.,
        "TractorBubbleStart" | "TractorBubbleEnd" => 5.,
        "JumpBroad" | "JumpCapLeapFrog" | "SpinCap" | "SpinCapEnd" | "SpinCapWait"
        | "SquatWait" | "SwimLand" | "SwimStand" => 3.,
        "SpinJumpDownFall" => 4.,
        // The swim's clips (recorded `interp`): the surface strokes blend over fifteen steps,
        // the idle surface over ten, the idle on the bottom over twenty.
        "SwimWalk" => 7.,
        "SwimSurfaceNormal" => 15.,
        "SwimStandSurface" => 10.,
        "SwimStandWait" => 20.,
        // Recorded on the lake floor (`swim-lake-forward`): the stroke's clip hands over to the
        // forward swim over fourteen steps.
        "SwimStandMove" => 14.,
        "PoleWait" => 8.,
        // Recorded on the pole (`pole-climb`, `pole-top`): the turn round the
        // pole after the slide, and the handstand's start and wait.
        "PoleTurn" => 2.,
        "PoleHandStandStart" => 3.,
        "PoleHandStandWait" => 4.,
        _ => 0.,
    }
}
/// The skeleton clip an action plays when the action table (`ActionAnimCtrl`)
/// names another one: the spin jump's start and restart actions both play the
/// 8-frame `SpinJumpL` or `SpinJumpR`.
pub fn skeleton_clip(action: &str) -> &str {
    match action {
        "StartSpinJumpL" | "RestartSpinJumpL" => "SpinJumpL",
        "StartSpinJumpR" | "RestartSpinJumpR" => "SpinJumpR",
        // The action table's swim clip names; the skeleton file has the surface swim as one clip.
        "SwimSurfaceNormal" | "SwimSurfaceCarry" => "SwimSurface",
        other => other,
    }
}
/// A pose the animation blend can freeze and fade out.
pub trait Posed {
    fn is_empty(&self) -> bool;
    /// Multiply every clip's weight (the share of a mixture it keeps).
    fn scale(&mut self, share: f32);
    /// Add another pose's clips to this one.
    fn merge(&mut self, other: Self);
}
/// The playing request and the frozen poses it is blending away from. The
/// renderer blends clip poses; the headless trace blends clip names, so both
/// follow one set of crossfade rules.
#[derive(Default)]
pub struct Blend<P> {
    pub name: &'static str,
    pub serial: u32,
    pub current: P,
    /// Outgoing poses with their weight and fade per second.
    pub old: Vec<(P, f32, f32)>,
}
impl<P: Default + Posed> Blend<P> {
    /// Follow the requested clip: a new request freezes the pose it replaces
    /// and fades it over the clip's blend frames; then age the fades by `dt`.
    pub fn update(&mut self, name: &'static str, serial: u32, next: P, dt: f32) {
        if self.name != name || self.serial != serial {
            self.freeze(interp_frames(name, self.name));
            self.name = name;
            self.serial = serial;
        }
        self.current = next;
        for (_, weight, fade) in &mut self.old {
            *weight -= *fade * dt;
            // The fade is a whole number of steps: what is left of it under half a step is the
            // rounding of the steps before (seven steps of a seventh do not sum to one).
            if *weight < 0.5 * *fade * dt {
                *weight = 0.;
            }
        }
        self.old.retain(|o| o.1 > 0.);
    }
    /// Freeze the pose shown now and fade the next one in from it over `frames` (none: the
    /// next pose shows at once). The pose being left is what the blend shows now: the clip
    /// and the fading rest of the clips before it. The animator freezes all of it as one pose
    /// that fades over the new clip's frames (the trace's `interp` shows that one fade).
    pub fn freeze(&mut self, frames: f32) {
        let mut old = std::mem::take(&mut self.current);
        if frames > 0. && !old.is_empty() {
            let total: f32 = 1. - self.old.iter().map(|o| o.1).sum::<f32>();
            old.scale(total.max(0.));
            for (mut earlier, weight, _) in self.old.drain(..) {
                earlier.scale(weight);
                old.merge(earlier);
            }
            self.old.push((old, 1., 60. / frames));
        } else {
            self.old.clear();
        }
    }
    /// Share of the final pose still held by outgoing poses.
    pub fn kept(&self) -> f32 {
        self.old.iter().map(|o| o.1).sum::<f32>().min(1.)
    }
}
/// Clips that cycle; every other clip holds its last frame.
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
pub fn looping(name: &str) -> bool {
    matches!(
        name,
        "Wait"
            | "WaitHot"
            | "TractorBubbleWait"
            | "WaitRelax"
            | "Snooze"
            | "Sleep"
            | "Rolling"
            | "HeadSliding"
            | "SquatWait"
            | "SquatWalk"
            | "WallSlideL"
            | "WallCatch"
            | "WallCatchMoveL"
            | "WallCatchMoveR"
            | "PoleWait"
            | "PoleClimb"
            | "PoleFall"
            | "PoleTurn"
            | "PoleHandStandWait"
            | "PoleHandStandTurn"
            | "SwimStandSurface"
            | "SwimStandMove"
            | "SwimSurfaceNormal"
            | "SwimStandWait"
            | "SwimHeadSlidingIn"
            | "SwimWalk"
            | "SpinJumpLoop"
            | "Slide"
            | "FireRun"
            | "SpinGroundL"
            | "SpinGroundR"
            | "StartSpinJumpL"
            | "StartSpinJumpR"
            | "RestartSpinJumpL"
            | "RestartSpinJumpR"
            | "SpinJumpDownFall"
            | "SpinJumpDownFallL"
            | "SpinJumpDownFallR"
            | "SpinCapWait"
    )
}
/// Clip to use when an outfit lacks one (older conversions skipped some).
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
pub fn fallback(name: &str) -> &'static str {
    match name {
        "Dash" | "DashFast" | "RunStart" => "Run",
        "JumpReverse" | "JumpCapCatchCommon" | "WallCatchEndJump" | "JumpInterp" => "Jump",
        "LandJump3" | "LandTurn" | "LandStiffen" | "LandDownFall" | "LandRolling" => "Land",
        "TurnPoint" | "Turn" => "Brake",
        "SpinJumpStart" | "SpinJumpLoop" | "SpinJumpDownFall" => "SpinJump",
        "SpinJumpDownFallL" | "SpinJumpDownFallR" => "SpinJumpDownFall",
        "RollingStandUp" => "RollingEnd",
        "WallKeep" => "WallSlideL",
        "WallCatchEnd" => "WallCatch",
        "PoleCatch" | "PoleFall" => "PoleWait",
        "SwimFallIn" => "SwimWait",
        "SquatEnd" | "SquatStart" => "SquatWait",
        "SpinGroundL" | "SpinGroundR" => "SpinCapStart",
        "JumpBroad" => "Jump",
        n if n.starts_with("JumpBroad") => "JumpBroad",
        n if n.starts_with("SpinCap") && n != "SpinCapStart" => "SpinCapStart",
        _ => "Wait",
    }
}
#[cfg(feature = "local-assets")]
mod imported {
    use super::*;
    use crate::asset_contract::{self, Bone, Part as AssetPart, Stain, Surface, VisibilityRole};
    use crate::movement::ControllerState;
    use crate::movement::look_joint::Mat34;
    use crate::skeleton_pose::{JOINT_COUNT, Joint, Matrices};
    use crate::trace::SkeletonFeed;
    #[derive(Component)]
    struct Imported;
    #[derive(Component)]
    struct ImportedCap;
    #[derive(Resource, Default)]
    struct Loaded {
        selection: Option<usize>,
        gltf: Handle<bevy::gltf::Gltf>,
        /// The file the clips come from (the outfit itself, or a shared one).
        clip_file: Handle<bevy::gltf::Gltf>,
        graph: Handle<AnimationGraph>,
        /// Clip name to graph node and length in seconds.
        nodes: std::collections::BTreeMap<String, (AnimationNodeIndex, f32)>,
        /// The same clips split along the partial animation's group (`skeleton_pose::Region`):
        /// `lower` leaves the group's joints alone (a main clip under a partial animation),
        /// `upper` leaves every other joint alone (the partial animation itself).
        lower_nodes: std::collections::BTreeMap<String, (AnimationNodeIndex, f32)>,
        upper_nodes: std::collections::BTreeMap<String, (AnimationNodeIndex, f32)>,
        /// A partial animation was playing in the last frame drawn.
        partial_was: bool,
        ready: bool,
        model_visible: bool,
        cap_loaded: bool,
        finished: bool,
        /// Upper-body throw clips (ground, air), masked off the legs.
        overlay: Option<(AnimationNodeIndex, AnimationNodeIndex)>,
        overlay_weight: f32,
        masked: bool,
        blend: Blend,
        /// The clip file's animation target for each (bone, parent bone) name pair.
        targets: std::collections::HashMap<[String; 2], bevy::animation::AnimationTargetId>,
        retargeted: bool,
        /// Each model material's own colour, and the soot levels last applied.
        stain_base: std::collections::HashMap<AssetId<StandardMaterial>, (Color, f32, f32)>,
        stain_shown: Option<(f32, f32)>,
    }
    /// One pose source: weighted clips at fixed times.
    #[derive(Clone, Default)]
    struct Pose {
        clips: Vec<(AnimationNodeIndex, f32, f32)>,
    }
    impl Posed for Pose {
        fn is_empty(&self) -> bool {
            self.clips.is_empty()
        }
        fn scale(&mut self, share: f32) {
            for clip in &mut self.clips {
                clip.1 *= share;
            }
        }
        fn merge(&mut self, other: Self) {
            self.clips.extend(other.clips);
        }
    }
    type Blend = super::Blend<Pose>;
    /// Mask group for hips and legs; the throw overlay leaves them alone.
    const LEGS: u32 = 0;
    /// Mask groups for the partial animation's joints (`Spine1` and below) and the rest.
    const PARTIAL_GROUP: u32 = 1;
    const OUTSIDE_PARTIAL_GROUP: u32 = 2;
    /// Frames the pose with the partial animation fades over once it is cleared.
    const PARTIAL_CLEAR_FRAMES: f32 = 5.;
    #[path = "../../tests/imported_assets.rs"]
    mod checks;
    pub fn self_check() {
        checks::run();
    }
    pub fn plugin(app: &mut App) {
        // The joint controllers bend the joints the animation just posed, before the transforms
        // are propagated.
        app.add_systems(
            PostUpdate,
            pose_joints
                .after(bevy::app::AnimationSystems)
                .before(bevy::transform::TransformSystems::Propagate),
        );
        // `PJV_POSE_CHECK=1`: report how far the drawn skeleton is from the traced one.
        if std::env::var_os("PJV_POSE_CHECK").is_some() {
            app.init_resource::<PoseCheck>().add_systems(
                PostUpdate,
                compare_pose.after(bevy::transform::TransformSystems::Propagate),
            );
        }
        app.init_resource::<Loaded>().add_systems(
            Update,
            (
                select,
                bind_metadata,
                retarget,
                mask,
                animation,
                parts,
                finish,
                stain,
            )
                .chain()
                .after(crate::world::animate),
        );
    }
    /// What the joint controllers' pose system remembers between frames.
    #[derive(Default)]
    struct JointMemory {
        /// The joints written last frame: the transform the animation gave and the one written.
        written: Vec<(Entity, Transform, Transform)>,
        /// The step the controllers last advanced for, and their state from before it.
        step: Option<u32>,
        before: Option<ControllerState>,
        /// The traced skeleton's animation state, and the step it was last advanced for.
        feed: SkeletonFeed,
        fed: Option<u32>,
    }
    /// The most steps one drawn frame advances the skeleton's animation state by.
    const MAX_FED_STEPS: u32 = 4;
    /// The traced skeleton of the frame being drawn and the nodes that show it
    /// (`PJV_POSE_CHECK`).
    #[derive(Resource, Default)]
    struct PoseCheck {
        shown: Option<(Matrices, [Option<Entity>; JOINT_COUNT])>,
        frames: u32,
    }
    /// The drawn joints against the traced skeleton, each joint's place measured in the frame of
    /// `AllRoot` (so that the model's scale, yaw and the actor's place drop out). Prints the
    /// largest difference, in centimetres, every 30 frames.
    fn compare_pose(mut check: ResMut<PoseCheck>, globals: Query<&GlobalTransform>) {
        let Some((traced, nodes)) = check.shown else {
            return;
        };
        let root = Joint::AllRoot as usize;
        let Some(drawn_root) = nodes[root].and_then(|e| globals.get(e).ok()) else {
            return;
        };
        let drawn_inverse = drawn_root.affine().inverse();
        let traced_inverse = mat4(&traced[root]).inverse();
        let mut worst = (0f32, Joint::AllRoot);
        for joint in Joint::ALL {
            let Some(drawn) = nodes[joint as usize].and_then(|e| globals.get(e).ok()) else {
                continue;
            };
            let at = drawn_inverse.transform_point3a(drawn.translation().into());
            let row = &traced[joint as usize];
            let expected =
                traced_inverse.transform_point3(Vec3::new(row[0][3], row[1][3], row[2][3]));
            let gap = (Vec3::from(at) - expected).length();
            if gap > worst.0 {
                worst = (gap, joint);
            }
        }
        check.frames += 1;
        if check.frames % 30 == 1 {
            info!(
                "POSECHECK frame {}: drawn skeleton is {:.5} cm from the traced one at most ({})",
                check.frames,
                worst.0,
                worst.1.name()
            );
        }
    }
    /// A joint of the model as the animation leaves it: the node's entity, and the matrix of
    /// the nodes between it and its parent joint (the identity where it is the child itself).
    struct AnimatedJoint {
        entity: Entity,
        lead: Mat4,
    }
    fn mat34(matrix: Mat4) -> Mat34 {
        let row = |r: usize| {
            [
                matrix.col(0)[r],
                matrix.col(1)[r],
                matrix.col(2)[r],
                matrix.col(3)[r],
            ]
        };
        [row(0), row(1), row(2)]
    }
    fn mat4(rows: &Mat34) -> Mat4 {
        Mat4::from_cols_array_2d(&[
            [rows[0][0], rows[1][0], rows[2][0], 0.],
            [rows[0][1], rows[1][1], rows[2][1], 0.],
            [rows[0][2], rows[1][2], rows[2][2], 0.],
            [rows[0][3], rows[1][3], rows[2][3], 1.],
        ])
    }
    /// The body's skeleton, posed as the trace poses it. The 27 joints the trace records get
    /// their local matrices from the traced chain itself (`SkeletonFeed`: the original curves,
    /// the game's matrix-row blend and `ModelAnimInterp` fades, the sub and partial animations),
    /// go through the joint controllers (`Player::pose_skeleton`, the calculation the trace does
    /// without a renderer: the ground pose's lean, the sway of a landing, the legs planted, the
    /// head's look-at, the hip spring, the poles) and are written to the model's nodes before the
    /// transforms are propagated, so the body on screen is the `bones` column. The animation
    /// player's own blend still poses the joints the trace does not follow (fingers and face);
    /// without the installed curves it poses the body too, and only the joints the controllers
    /// moved are written. The controllers' state moves once a simulation step, however often
    /// the frame is drawn.
    fn pose_joints(
        fixed: Res<Time<Fixed>>,
        mut player: Single<&mut crate::movement::Player>,
        bones: Query<(Entity, &Bone)>,
        parents: Query<&ChildOf>,
        imported: Query<(), With<Imported>>,
        caps: Query<(), With<ImportedCap>>,
        mut transforms: Query<&mut Transform>,
        check: Option<ResMut<PoseCheck>>,
        mut memory: Local<JointMemory>,
    ) {
        // A clip with no channel for a joint leaves what was written last frame there.
        for (entity, animated, shown) in memory.written.drain(..) {
            if let Ok(mut transform) = transforms.get_mut(entity)
                && *transform == shown
            {
                *transform = animated;
            }
        }
        // The body's joints: the cap model carries joints of its own under the same names.
        let on_cap = |mut node: Entity| {
            while !imported.contains(node) {
                match parents.get(node) {
                    Ok(parent) => node = parent.parent(),
                    Err(_) => return false,
                }
            }
            caps.contains(node)
        };
        let mut nodes = [None; JOINT_COUNT];
        for (entity, bone) in &bones {
            if let Some(joint) = Joint::from_node_name(&bone.key[0])
                && !on_cap(entity)
            {
                nodes[joint as usize] = Some(entity);
            }
        }
        let mut joints = Vec::with_capacity(JOINT_COUNT);
        let mut locals = [crate::skeleton_pose::IDENTITY; JOINT_COUNT];
        for joint in Joint::ALL {
            let Some(entity) = nodes[joint as usize] else {
                return;
            };
            let above = joint.parent().and_then(|p| nodes[p as usize]);
            let Ok(own) = transforms.get(entity) else {
                return;
            };
            let mut lead = Mat4::IDENTITY;
            let mut matrix = own.to_matrix();
            let mut node = entity;
            loop {
                match parents.get(node) {
                    Ok(parent)
                        if Some(parent.parent()) != above
                            && !imported.contains(parent.parent()) =>
                    {
                        node = parent.parent();
                        let Ok(local) = transforms.get(node) else {
                            return;
                        };
                        lead = local.to_matrix() * lead;
                        matrix = local.to_matrix() * matrix;
                    }
                    _ => break,
                }
            }
            locals[joint as usize] = mat34(matrix);
            joints.push(AnimatedJoint { entity, lead });
        }
        let step = player.step_count();
        // The traced animation state moves once per simulation step.
        match memory.fed {
            None => memory.feed.advance(&player),
            Some(fed) => {
                for _ in 0..step.saturating_sub(fed).min(MAX_FED_STEPS) {
                    memory.feed.advance(&player);
                }
            }
        }
        memory.fed = Some(step);
        let traced = memory.feed.locals(&player, fixed.overstep_fraction());
        if let Some(traced) = &traced {
            locals = *traced;
        }
        let bent = match (memory.step == Some(step), memory.before.clone()) {
            (true, Some(mut before)) => player.pose_skeleton_from(&mut before, &locals),
            _ => {
                memory.step = Some(step);
                memory.before = Some(player.controllers());
                player.pose_skeleton(&locals)
            }
        };
        let actor = player.pose_matrix();
        let mut shown = crate::skeleton_pose::bent_locals(&actor, &locals, &bent);
        if traced.is_some() {
            // Every traced joint is written: those the controllers did not move keep the
            // traced local matrix.
            let moved = std::mem::take(&mut shown);
            shown = Joint::ALL
                .iter()
                .map(|&joint| {
                    let own = moved.iter().find(|(m, _)| *m == joint).map(|(_, l)| *l);
                    (joint, own.unwrap_or(locals[joint as usize]))
                })
                .collect();
        }
        for (joint, local) in shown {
            let AnimatedJoint { entity, lead } = &joints[joint as usize];
            let Ok(mut transform) = transforms.get_mut(*entity) else {
                continue;
            };
            let animated = *transform;
            *transform = Transform::from_matrix(lead.inverse() * mat4(&local));
            memory.written.push((*entity, animated, *transform));
        }
        if let Some(mut check) = check
            && traced.is_some()
        {
            check.shown = Some((bent, nodes));
        }
    }
    fn select(
        mut commands: Commands,
        settings: Res<crate::input::Settings>,
        mut wardrobe: ResMut<Wardrobe>,
        server: Res<AssetServer>,
        models: Res<Assets<bevy::gltf::Gltf>>,
        mut clips: ResMut<Assets<AnimationClip>>,
        gltf_nodes: Res<Assets<bevy::gltf::GltfNode>>,
        mut graphs: ResMut<Assets<AnimationGraph>>,
        mut loaded: ResMut<Loaded>,
        pivot: Single<Entity, With<crate::world::Pivot>>,
        roots: Query<Entity, With<Imported>>,
        mut original: Query<&mut Visibility, With<crate::world::Body>>,
    ) {
        if loaded.selection != Some(settings.outfit) {
            for e in &roots {
                commands.entity(e).despawn();
            }
            *loaded = Loaded {
                selection: Some(settings.outfit),
                ..default()
            };
            for mut v in &mut original {
                *v = Visibility::Visible;
            }
            if let Some(outfit) = settings
                .outfit
                .checked_sub(3)
                .and_then(|i| wardrobe.outfits.get(i))
            {
                loaded.gltf = server.load(outfit.file.clone());
                loaded.clip_file = server.load(
                    outfit
                        .animation_file
                        .clone()
                        .unwrap_or_else(|| outfit.file.clone()),
                );
                wardrobe.status = format!("Loading {}...", outfit.name);
            }
        }
        if loaded.ready || settings.outfit < 3 {
            return;
        }
        if server.is_loaded_with_dependencies(&loaded.gltf)
            && server.is_loaded_with_dependencies(&loaded.clip_file)
        {
            let (Some(gltf), Some(source)) =
                (models.get(&loaded.gltf), models.get(&loaded.clip_file))
            else {
                return;
            };
            let Some(scene) = gltf
                .default_scene
                .clone()
                .or_else(|| gltf.scenes.first().cloned())
            else {
                wardrobe.status = "This outfit has no model. Using the explorer.".into();
                loaded.ready = true;
                return;
            };
            let outfit = &wardrobe.outfits[settings.outfit - 3];
            let mut graph = AnimationGraph::new();
            let mut node_ids = std::collections::HashMap::new();
            loaded.targets = match target_ids(source, &gltf_nodes).and_then(|(ids, by_node)| {
                target_ids(gltf, &gltf_nodes)?;
                node_ids = by_node;
                Ok(ids)
            }) {
                Ok(ids) => ids,
                Err(error) => {
                    wardrobe.status =
                        format!("Invalid animation metadata: {error}. Using the explorer.");
                    error!("{}", wardrobe.status);
                    loaded.ready = true;
                    return;
                }
            };
            // Each clip with the original curves in place of the baked ones, where the installation
            // has them: the game evaluates those between frames.
            let named: std::collections::BTreeMap<String, Handle<AnimationClip>> = source
                .named_animations
                .iter()
                .map(|(name, clip)| {
                    let exact = exact_clip(name, clip, &node_ids, &mut clips);
                    (name.to_string(), exact.unwrap_or_else(|| clip.clone()))
                })
                .collect();
            for (name, clip) in &named {
                let node = graph.add_clip(clip.clone(), 1., graph.root);
                let length = clips.get(clip).map_or(1., |c| c.duration());
                loaded.nodes.insert(name.to_string(), (node, length));
                let mut split = |mask_group: u32| {
                    (
                        graph.add_clip_with_mask(clip.clone(), 1 << mask_group, 1., graph.root),
                        length,
                    )
                };
                loaded
                    .lower_nodes
                    .insert(name.to_string(), split(PARTIAL_GROUP));
                loaded
                    .upper_nodes
                    .insert(name.to_string(), split(OUTSIDE_PARTIAL_GROUP));
            }
            // Blending is a normalized weighted average, so a heavy overlay
            // node dominates the arms and torso while its mask keeps the
            // running legs from the locomotion clip underneath.
            let upper = |key: &str, default: &str| {
                let name = outfit.animations.get(key).map_or(default, String::as_str);
                named.get(name).cloned()
            };
            if let (Some(ground), Some(air)) = (
                upper("throw_upper", "ThrowCapParts"),
                upper("throw_upper_air", "ThrowCapJumpParts"),
            ) {
                loaded.overlay = Some((
                    graph.add_clip_with_mask(ground, 1 << LEGS, 12., graph.root),
                    graph.add_clip_with_mask(air, 1 << LEGS, 12., graph.root),
                ));
            }
            loaded.graph = graphs.add(graph);
            let root = commands
                .spawn((
                    WorldAssetRoot(scene),
                    Imported,
                    Transform::from_rotation(Quat::from_rotation_y(outfit.yaw))
                        .with_scale(Vec3::splat(outfit.scale)),
                    Visibility::default(),
                ))
                .id();
            commands.entity(*pivot).add_child(root);
            if let Some(file) = &outfit.cap_file {
                let cap_root = commands
                    .spawn((
                        Imported,
                        ImportedCap,
                        WorldAssetRoot(server.load(format!("{file}#Scene0"))),
                        Transform::from_scale(Vec3::splat(outfit.scale)),
                        Visibility::Hidden,
                    ))
                    .id();
                if asset_dir().join("local/caps/CapManHeroEyes.glb").is_file() {
                    let eyes = commands
                        .spawn((
                            WorldAssetRoot(server.load("local/caps/CapManHeroEyes.glb#Scene0")),
                            Transform::from_xyz(0., 15., 42.).with_scale(Vec3::splat(1.5)),
                        ))
                        .id();
                    commands.entity(cap_root).add_child(eyes);
                }
                loaded.cap_loaded = true;
            }
            loaded.model_visible = true;
            for mut v in &mut original {
                *v = Visibility::Hidden;
            }
            loaded.ready = true;
            wardrobe.status = if loaded.nodes.is_empty() {
                format!(
                    "Wearing {}. Its movement animations are missing.",
                    outfit.name
                )
            } else {
                format!("Wearing {} (original game data)", outfit.name)
            };
        } else if [&loaded.gltf, &loaded.clip_file]
            .iter()
            .any(|h| matches!(server.load_state(*h), bevy::asset::LoadState::Failed(_)))
        {
            wardrobe.status = "This outfit failed to load. Using the explorer.".into();
            loaded.ready = true;
        }
    }
    /// The animation targets of a file: by bone and parent name, and by node name.
    type TargetIds = (
        std::collections::HashMap<[String; 2], bevy::animation::AnimationTargetId>,
        std::collections::HashMap<String, bevy::animation::AnimationTargetId>,
    );
    /// Target ids the clip file's animations use, keyed by bone and parent name (and, in the
    /// second map, by the node's own name). The loader derives them from each bone's name path
    /// below the root.
    fn target_ids(
        source: &bevy::gltf::Gltf,
        nodes: &Assets<bevy::gltf::GltfNode>,
    ) -> Result<TargetIds, String> {
        let all: Vec<&bevy::gltf::GltfNode> =
            source.nodes.iter().filter_map(|h| nodes.get(h)).collect();
        let by_index: std::collections::HashMap<usize, &bevy::gltf::GltfNode> =
            all.iter().map(|n| (n.index, *n)).collect();
        let mut parent = std::collections::HashMap::new();
        for n in &all {
            for c in n.children.iter().filter_map(|h| nodes.get(h)) {
                parent.insert(c.index, n.index);
            }
        }
        let mut ids = std::collections::HashMap::new();
        let mut by_node = std::collections::HashMap::new();
        for n in &all {
            let mut path = vec![Name::new(n.name.clone())];
            let mut at = n.index;
            while let Some(p) = parent.get(&at) {
                path.push(Name::new(by_index[p].name.clone()));
                at = *p;
            }
            path.reverse();
            let parent_name = parent
                .get(&n.index)
                .map_or(String::new(), |p| by_index[p].name.clone());
            let spec = n
                .extras
                .as_ref()
                .map(|e| asset_contract::decode::<Bone>(&e.value, "pjv_bone"))
                .transpose()?
                .flatten()
                .unwrap_or_else(|| crate::legacy_assets::bone(&n.name, &parent_name));
            if spec.key[0].is_empty() {
                return Err("Empty bone identity".into());
            }
            let id = bevy::animation::AnimationTargetId::from_names(path.iter());
            by_node.insert(n.name.clone(), id);
            if ids.insert(spec.key.clone(), id).is_some() {
                return Err(format!("Ambiguous bone identity {:?}", spec.key));
            }
        }
        Ok((ids, by_node))
    }
    /// A copy of a clip whose joints follow the original curves (`clip_curves`) instead of the
    /// baked keys, or `None` when the installation has no curves for it. The curves are sampled
    /// at the playback time, so the pose between whole frames is the game's, not a line between
    /// two baked samples.
    fn exact_clip(
        name: &str,
        baked: &Handle<AnimationClip>,
        node_ids: &std::collections::HashMap<String, bevy::animation::AnimationTargetId>,
        clips: &mut Assets<AnimationClip>,
    ) -> Option<Handle<AnimationClip>> {
        use bevy::animation::{animated_field, animation_curves::AnimatableCurve};
        use bevy::curve::{FunctionCurve, Interval};
        let curves = crate::clip_curves::Curves::installed()?;
        let curve_clip = curves.clip(name)?;
        let mut clip = clips.get(baked)?.clone();
        let domain = Interval::new(0., clip.duration()).ok()?;
        for joint in 0..curves.joint_count() {
            let Some(target) = curves
                .joint_name(joint)
                .and_then(|joint_name| node_ids.get(&format!("Armature_{joint_name}")))
            else {
                continue;
            };
            if curves.sample(curve_clip, joint, 0.).is_none() {
                continue;
            }
            // (Every sample is asked for a whole joint at the clip's frame: 60 a second.)
            let sample = move |time: f32| {
                curves
                    .sample_for_file(
                        curve_clip,
                        joint,
                        time * crate::clip_curves::FRAMES_PER_SECOND,
                    )
                    .expect("the clip animates the joint")
            };
            if let Some(baked_curves) = clip.curves_for_target_mut(*target) {
                baked_curves.clear();
            }
            clip.add_curve_to_target(
                *target,
                AnimatableCurve::new(
                    animated_field!(Transform::translation),
                    FunctionCurve::new(domain, move |t| {
                        Vec3::from_array(sample(t).translation.map(|c| c as f32))
                    }),
                ),
            );
            clip.add_curve_to_target(
                *target,
                AnimatableCurve::new(
                    animated_field!(Transform::rotation),
                    FunctionCurve::new(domain, move |t| {
                        Quat::from_array(sample(t).rotation.map(|c| c as f32))
                    }),
                ),
            );
            clip.add_curve_to_target(
                *target,
                AnimatableCurve::new(
                    animated_field!(Transform::scale),
                    FunctionCurve::new(domain, move |t| {
                        Vec3::from_array(sample(t).scale.map(|c| c as f32))
                    }),
                ),
            );
        }
        Some(clips.add(clip))
    }
    fn bind_metadata(
        mut commands: Commands,
        settings: Res<crate::input::Settings>,
        wardrobe: Res<Wardrobe>,
        meshes: Query<
            (Entity, &Name, Option<&bevy::gltf::GltfExtras>),
            (With<Mesh3d>, Without<AssetPart>),
        >,
        bones: Query<
            (Entity, &Name, &ChildOf, Option<&bevy::gltf::GltfExtras>),
            (With<bevy::animation::AnimationTargetId>, Without<Bone>),
        >,
        names: Query<&Name>,
    ) {
        let legacy_metal = settings
            .outfit
            .checked_sub(3)
            .and_then(|i| wardrobe.outfits.get(i))
            .is_some_and(|o| o.file.contains("Metal"));
        for (e, name, extras) in &meshes {
            let spec = match extras
                .map(|x| asset_contract::decode::<AssetPart>(&x.value, "pjv_part"))
                .transpose()
            {
                Ok(Some(Some(spec))) => spec,
                Ok(_) => crate::legacy_assets::part(name.as_str(), legacy_metal),
                Err(error) => {
                    error!("Invalid mesh metadata: {error}");
                    AssetPart::default()
                }
            };
            commands.entity(e).insert(spec);
        }
        for (e, name, parent, extras) in &bones {
            let parent = names.get(parent.parent()).map_or("", Name::as_str);
            let spec = match extras
                .map(|x| asset_contract::decode::<Bone>(&x.value, "pjv_bone"))
                .transpose()
            {
                Ok(Some(Some(spec))) => spec,
                Ok(_) => crate::legacy_assets::bone(name.as_str(), parent),
                Err(error) => {
                    error!("Invalid bone metadata: {error}");
                    continue;
                }
            };
            commands.entity(e).insert(spec);
        }
    }
    fn retarget(
        mut loaded: ResMut<Loaded>,
        mut bones: Query<(&Bone, &mut bevy::animation::AnimationTargetId)>,
    ) {
        if !loaded.ready || loaded.retargeted || loaded.targets.is_empty() {
            return;
        }
        let mut found = false;
        for (bone, mut id) in &mut bones {
            if let Some(target) = loaded.targets.get(&bone.key) {
                *id = *target;
                found |= bone.hip;
            }
        }
        loaded.retargeted = found;
    }
    fn mask(
        mut loaded: ResMut<Loaded>,
        mut graphs: ResMut<Assets<AnimationGraph>>,
        targets: Query<(&bevy::animation::AnimationTargetId, &Bone)>,
    ) {
        if !loaded.ready || !loaded.retargeted || loaded.masked {
            return;
        }
        let Some(mut graph) = graphs.get_mut(&loaded.graph) else {
            return;
        };
        // Each joint's parent, to tell the joints of the partial animation's group (below
        // `Spine1`) from the rest.
        let parents: std::collections::HashMap<&str, &str> = targets
            .iter()
            .map(|(_, bone)| (bone.key[0].as_str(), bone.key[1].as_str()))
            .collect();
        let in_partial_group = |bone: &Bone| {
            let mut name = bone.key[0].as_str();
            // (The walk is bounded by the number of joints: a cycle never loops forever.)
            for _ in 0..=parents.len() {
                if Joint::from_node_name(name).is_some_and(Joint::is_partial_group_root) {
                    return true;
                }
                match parents.get(name) {
                    Some(parent) => name = parent,
                    None => return false,
                }
            }
            false
        };
        let mut found = false;
        for (id, bone) in &targets {
            if bone.leg {
                graph.add_target_to_mask_group(*id, LEGS);
                found |= bone.hip;
            }
            graph.add_target_to_mask_group(
                *id,
                if in_partial_group(bone) {
                    PARTIAL_GROUP
                } else {
                    OUTSIDE_PARTIAL_GROUP
                },
            );
        }
        loaded.masked = found;
    }
    /// Look up a clip among `nodes`, following the fallback chain for missing ones.
    fn clip_in(
        nodes: &std::collections::BTreeMap<String, (AnimationNodeIndex, f32)>,
        name: &str,
    ) -> Option<(AnimationNodeIndex, f32)> {
        let mut name = skeleton_clip(name);
        for _ in 0..4 {
            if let Some(found) = nodes.get(name) {
                return Some(*found);
            }
            name = fallback(name);
        }
        nodes.get("Wait").copied()
    }
    /// The name of the clip a graph node plays, whichever way the node is masked.
    fn clip_name(loaded: &Loaded, node: AnimationNodeIndex) -> Option<&str> {
        [&loaded.nodes, &loaded.lower_nodes, &loaded.upper_nodes]
            .into_iter()
            .flatten()
            .find(|(_, value)| value.0 == node)
            .map(|(name, _)| name.as_str())
    }
    /// The pose the controller asks for, with clip times in seconds. The run
    /// cycle blends Walk, Run, Dash and DashFast on one shared phase. While a
    /// partial animation plays, the main clips leave the joints it covers to it
    /// (`AnimPlayerSkl::calcSklAnim`) and the partial takes them with the
    /// weight of the whole blend.
    fn target(loaded: &Loaded, p: &crate::movement::Player, ahead: f32) -> Pose {
        let partial = p.partial_anim();
        let main = if partial.is_some() {
            &loaded.lower_nodes
        } else {
            &loaded.nodes
        };
        let anim = p.anim;
        let mut pose = if anim.name == "Move" {
            let cycle = p.run_cycle();
            let phase = ((cycle.phase + cycle.rate * ahead) / 60.).rem_euclid(1.);
            let clips = cycle
                .layers()
                .filter_map(|l| {
                    clip_in(main, l.name).map(|(node, len)| (node, l.weight, phase * len))
                })
                .collect();
            Pose { clips }
        } else if let Some((node, len)) = clip_in(main, anim.name) {
            let seconds = (anim.held() + anim.rate * ahead).max(0.) / 60.;
            let time = if looping(anim.name) {
                seconds.rem_euclid(len.max(1e-3))
            } else {
                // Hold just before the end: the player wraps a time equal to the
                // clip's length back to its first frame.
                seconds.min((len - 1e-4).max(0.))
            };
            Pose {
                clips: vec![(node, 1., time)],
            }
        } else {
            return Pose::default();
        };
        if let Some((name, frame)) = partial
            && let Some((node, len)) = loaded.upper_nodes.get(name)
        {
            let weight = pose.clips.iter().map(|clip| clip.1).sum();
            let seconds = ((frame + ahead) / 60.).min((len - 1e-4).max(0.));
            pose.clips.push((*node, weight, seconds));
        }
        pose
    }
    fn animation(
        mut commands: Commands,
        time: Res<Time>,
        fixed: Res<Time<Fixed>>,
        p: Single<&crate::movement::Player>,
        mut loaded: ResMut<Loaded>,
        mut players: Query<(Entity, &mut AnimationPlayer, Option<&AnimationGraphHandle>)>,
    ) {
        if !loaded.ready || loaded.nodes.is_empty() {
            return;
        }
        // Render between fixed steps without waiting for the next one.
        let ahead = fixed.overstep_fraction();
        let next = target(&loaded, &p, ahead);
        let dt = time.delta_secs();
        // The partial animation's end clears it with a five step interpolation
        // (`PlayerAnimator::clearUpperBodyAnim`): the pose shown fades into the clip alone.
        let partial_now = p.partial_anim().is_some();
        let cleared = std::mem::replace(&mut loaded.partial_was, partial_now) && !partial_now;
        let blend = &mut loaded.blend;
        if cleared && blend.name == p.anim.name && blend.serial == p.anim.serial {
            blend.freeze(PARTIAL_CLEAR_FRAMES);
        }
        blend.update(p.anim.name, p.anim.serial, next, dt);
        // Final weights: outgoing poses keep their share, the rest is current.
        let kept = blend.kept();
        let mut weights: std::collections::BTreeMap<AnimationNodeIndex, (f32, f32)> = default();
        for (pose, share) in blend
            .old
            .iter()
            .map(|o| (&o.0, o.1))
            .chain([(&blend.current, 1. - kept)])
        {
            for &(node, w, t) in &pose.clips {
                let entry = weights.entry(node).or_insert((0., t));
                entry.0 += w * share;
                entry.1 = t;
            }
        }
        // Throwing while running: the upper-body clip over the legs.
        let sub = p.sub_anim;
        let fade = dt / 0.05;
        loaded.overlay_weight = if sub.is_some() {
            (loaded.overlay_weight + fade).min(1.)
        } else {
            (loaded.overlay_weight - fade).max(0.)
        };
        let overlay = loaded.overlay.map(|(ground, air)| {
            if p.grounded {
                (ground, air)
            } else {
                (air, ground)
            }
        });
        for (e, mut player, graph) in &mut players {
            let playing: Vec<AnimationNodeIndex> =
                player.playing_animations().map(|(n, _)| *n).collect();
            for node in playing {
                let is_overlay = overlay.is_some_and(|(on, off)| node == on || node == off);
                if !weights.contains_key(&node) && !is_overlay {
                    player.stop(node);
                }
            }
            for (&node, &(weight, seconds)) in &weights {
                let active = player.play(node);
                active.set_weight(weight).set_speed(0.).seek_to(seconds);
                active.repeat();
            }
            if let Some((on, off)) = overlay {
                player.stop(off);
                if loaded.overlay_weight == 0. {
                    player.stop(on);
                } else {
                    let frame = sub.map_or(15., |s| s.frame + ahead).min(15.9);
                    player
                        .play(on)
                        .set_weight(loaded.overlay_weight)
                        .set_speed(0.)
                        .seek_to(frame / 60.);
                }
            }
            if graph.is_none() {
                commands
                    .entity(e)
                    .insert(AnimationGraphHandle(loaded.graph.clone()));
            }
        }
    }
    fn parts(
        time: Res<Time>,
        fixed: Res<Time<Fixed>>,
        p: Single<&crate::movement::Player>,
        settings: Res<crate::input::Settings>,
        loaded: Res<Loaded>,
        wardrobe: Res<Wardrobe>,
        mut own_shadow: ResMut<CastsOwnShadow>,
        mut hats: Query<&mut Visibility, With<crate::world::Hat>>,
        mut meshes: Query<
            (&AssetPart, &mut Visibility),
            (With<Mesh3d>, Without<crate::world::Hat>),
        >,
        eye_materials: Query<(&AssetPart, &MeshMaterial3d<StandardMaterial>)>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        mut caps: Query<
            (&mut Transform, &mut Visibility),
            (
                With<ImportedCap>,
                Without<Mesh3d>,
                Without<crate::world::Hat>,
            ),
        >,
    ) {
        let imported = settings.outfit >= 3 && loaded.model_visible;
        if own_shadow.0 != imported {
            own_shadow.0 = imported;
        }
        for mut hat in &mut hats {
            *hat = if imported && (p.cap.is_none() || loaded.cap_loaded) {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        }
        for (mut transform, mut visibility) in &mut caps {
            *visibility = if p.cap.is_some() {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            if let Some(cap) = &p.cap {
                transform.translation = p.shown_cap.unwrap_or(cap.pos);
                transform.rotation = Quat::from_rotation_y(cap.age * 18.);
            }
        }
        // Blink for a few frames every few seconds; eyes stay shut in a pound.
        let t = time.elapsed_secs();
        let blink =
            (t % 3.7) < 0.11 || (t % 11.3) < 0.09 || p.mode == crate::movement::Move::PoundLand;
        let outfit = settings
            .outfit
            .checked_sub(3)
            .and_then(|i| wardrobe.outfits.get(i));
        // Discrete visibility follows the dominant playing clip's actual sample
        // time (including locomotion phase and the upper-body throw overlay).
        let sample = if let Some(a) = p.sub_anim.filter(|_| loaded.overlay.is_some()) {
            let (key, default) = if p.grounded {
                ("throw_upper", "ThrowCapParts")
            } else {
                ("throw_upper_air", "ThrowCapJumpParts")
            };
            let name = outfit
                .and_then(|o| o.animations.get(key))
                .map_or(default, String::as_str);
            Some((name, (a.frame + fixed.overstep_fraction()) / 60.))
        } else {
            loaded
                .blend
                .current
                .clips
                .iter()
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .and_then(|(node, _, seconds)| {
                    clip_name(&loaded, *node).map(|name| (name, *seconds))
                })
        };
        let part_show = |part: &str| {
            sample.and_then(|(clip, seconds)| outfit?.part_visibility(clip, seconds, part))
        };
        let legacy = outfit.is_none_or(|o| o.expressions.is_empty());
        let closed_left = part_show("EyelidCloseLeft").unwrap_or(legacy && blink);
        let closed_right = part_show("EyelidCloseRight").unwrap_or(legacy && blink);
        for (part, handle) in &eye_materials {
            if let Some(material) = part.pupil.as_deref()
                && let Some(mut m) = materials.get_mut(&handle.0)
            {
                let uv = sample
                    .and_then(|(clip, seconds)| outfit?.pupil_uv(clip, seconds, material))
                    .unwrap_or(bevy::math::Affine2::IDENTITY);
                if m.uv_transform != uv {
                    m.uv_transform = uv;
                }
            }
        }
        for (part, mut visibility) in &mut meshes {
            let show = match part.role {
                VisibilityRole::Unchanged => None,
                VisibilityRole::Hidden => Some(false),
                VisibilityRole::Visible => Some(true),
                VisibilityRole::ClosedLeft => Some(closed_left),
                VisibilityRole::ClosedRight => Some(closed_right),
                VisibilityRole::PupilLeft => Some(!closed_left),
                VisibilityRole::PupilRight => Some(!closed_right),
                VisibilityRole::Track => {
                    Some(part.track.as_deref().and_then(part_show).unwrap_or(false))
                }
                VisibilityRole::WornCap => Some(
                    p.cap.is_none() && part.track.as_deref().and_then(part_show).unwrap_or(true),
                ),
                VisibilityRole::UncappedHair => Some(p.cap.is_some()),
            };
            if let Some(show) = show {
                visibility.set_if_neq(if show {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        }
    }
    /// Materials the export cannot express: the metal suit is shaded entirely
    /// by reflections in the original, and untextured glass is see-through.
    fn finish(
        mut commands: Commands,
        mut loaded: ResMut<Loaded>,
        meshes: Query<(Entity, &Name, &AssetPart, &MeshMaterial3d<StandardMaterial>)>,
        mut materials: ResMut<Assets<StandardMaterial>>,
    ) {
        if !loaded.model_visible || loaded.finished {
            return;
        }
        let mut seen = false;
        for (entity, name, part, handle) in &meshes {
            if !part.exported {
                continue;
            }
            seen = true;
            if !casts_ground_shadow(name) {
                commands.entity(entity).insert(bevy::light::NotShadowCaster);
            }
            let Some(mut m) = materials.get_mut(&handle.0) else {
                continue;
            };
            finish_material(part, &mut m);
        }
        loaded.finished = seen;
    }
    /// The game keeps Mario's shadow on the ground to an ellipsoid around his
    /// body (`ShadowMaskArray` in his actor data: 110 cm wide, 100 cm tall,
    /// centred 50 cm up), so what his head and cap would throw far across the
    /// ground never shows. Only body and hands cast the sun's shadow here.
    fn casts_ground_shadow(name: &Name) -> bool {
        const HEAD: [&str; 6] = ["Cap", "Hair", "Eye", "face", "mario_", "Face"];
        !HEAD.iter().any(|prefix| name.starts_with(prefix))
    }
    fn finish_material(part: &AssetPart, m: &mut StandardMaterial) {
        // Capes, coats and skirts are single sheets the original draws from
        // both sides; the export loses that flag.
        m.double_sided = true;
        m.cull_mode = None;
        if part.surface == Surface::Glass {
            // Visors and goggles are tinted see-through glass in the original (its
            // glass shader is not exported); the face stays visible behind them.
            m.base_color_texture = None;
            m.base_color = Color::srgba(0.27, 0.27, 0.29, 0.35);
            m.alpha_mode = AlphaMode::Blend;
            m.perceptual_roughness = 0.05;
            m.reflectance = 0.9;
            return;
        }
        if part.surface == Surface::Hair && !part.metal {
            // Matte hair: the grey sheen of the sky on it would wash the brown out.
            m.reflectance = 0.3;
        }
        // The metal head reuses the 64 cap's hair texture, but the game's
        // reflection shader overrides that colour along with the rest.
        if m.base_color_texture.is_some() && !(part.metal && part.surface == Surface::Hair) {
            return;
        }
        if part.metal {
            m.base_color_texture = None;
            m.base_color = Color::srgb(0.86, 0.88, 0.92);
            m.metallic = 1.;
            m.perceptual_roughness = 0.12;
        }
    }
    /// Soot strength as the original's stain blend: an ease-out of the level
    /// scaled to 1.8, which saturates before the stain is at full strength.
    pub fn soot_blend(level: f32) -> f32 {
        // 0x466804: original endpoint/easing; clamping adapts it to StandardMaterial.
        (1.8 * crate::movement::joint_math::ease_out(level)).min(1.)
    }
    /// Soot on the original model. The game blends a near-black soot colour in
    /// through a stain pattern its shader masks; the export has neither, so
    /// every body material is darkened evenly and turned matte (its sheen
    /// would otherwise keep it bright), stopping short of black. The eyes
    /// stay clear and the worn cap follows its own level.
    fn stain(
        p: Single<&crate::movement::Player>,
        mut loaded: ResMut<Loaded>,
        roots: Query<Entity, (With<Imported>, Without<ImportedCap>)>,
        children: Query<&Children>,
        meshes: Query<(&AssetPart, &MeshMaterial3d<StandardMaterial>)>,
        mut materials: ResMut<Assets<StandardMaterial>>,
    ) {
        let soot = p.soot;
        if !loaded.finished || loaded.stain_shown == Some((soot.body, soot.cap)) {
            return;
        }
        loaded.stain_shown = Some((soot.body, soot.cap));
        for root in &roots {
            for e in children.iter_descendants(root) {
                let Ok((part, handle)) = meshes.get(e) else {
                    continue;
                };
                let level = match part.stain {
                    Stain::None => continue,
                    Stain::Cap => soot.cap,
                    Stain::Body => soot.body,
                };
                let Some(base) = loaded.stain_base.get(&handle.id()).copied().or_else(|| {
                    let m = materials.get(&handle.0)?;
                    Some((m.base_color, m.reflectance, m.perceptual_roughness))
                }) else {
                    continue;
                };
                loaded.stain_base.insert(handle.id(), base);
                let (color, reflectance, roughness) = base;
                let k = soot_blend(level);
                let c = color.to_linear();
                let soot_color = 0.01 * k;
                let Some(mut m) = materials.get_mut(&handle.0) else {
                    continue;
                };
                m.base_color = Color::LinearRgba(LinearRgba::new(
                    c.red * (1. - k) + soot_color,
                    c.green * (1. - k) + soot_color,
                    c.blue * (1. - k) + soot_color,
                    c.alpha,
                ));
                m.reflectance = reflectance * (1. - k);
                m.perceptual_roughness = roughness + (1. - roughness) * k;
            }
        }
    }
}
#[path = "tests/local_assets.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
