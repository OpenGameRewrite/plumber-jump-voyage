//! The 8-bit Mario on screen.
//!
//! While the dimension keeper is 2D the game shows `Mario2D` in place of the body: a flat figure
//! of voxels (one mesh per sprite, each skinned to its own `MarioDot00..08` joint) that the
//! actor's pose carries directly, with no lean, squash or turning ease. The clips move the
//! joints (the pipe clips slide the figure along the mouth) and their visibility tracks show
//! one sprite at a time. Everything is read from the installed `Mario2D.glb` and
//! `Mario2D.curves.bin` (`tools/convert-outfits.py --model-2d`); without them the 3D body stays.
//!
//! The model is its own tree, apart from the body's: the 3D pose systems look for joints under
//! their own roots (`local_assets::imported`) and never see these.
use crate::{model_2d, movement::Player, trace, world::Pivot};
use bevy::{camera::visibility::NoFrustumCulling, image::ImageSampler, prelude::*};
use std::collections::HashMap;

const MODEL: &str = "local/Mario2D.glb";
/// The model is in centimetres, the world in metres.
const UNIT: f32 = 0.01;
/// The material's `uniform0_mul_color` in the original (BodyMT): the sprite colours are lifted by
/// this factor over the texture's.
const COLOR_GAIN: f32 = 1.3;

#[derive(Component)]
struct Model2dRoot;

/// The loaded model and what was found in its spawned tree.
#[derive(Resource, Default)]
struct Model2d {
    gltf: Handle<bevy::gltf::Gltf>,
    spawned: Option<Entity>,
    /// Joint entities by name (`Armature_` prefix removed).
    joints: HashMap<String, Entity>,
    /// Sprite meshes with the joint each is skinned to.
    parts: Vec<(Entity, String)>,
    ready: bool,
    /// The last of the model's visibility actions that was shown: an action the model lacks
    /// (a 3D-only one) keeps the sprite that was on screen.
    action: Option<&'static str>,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Model2d>()
        .add_systems(Startup, load)
        .add_systems(
            Update,
            (spawn, find_nodes, drive)
                .chain()
                .after(crate::world::animate),
        );
}

fn load(mut model: ResMut<Model2d>, server: Res<AssetServer>) {
    if crate::local_assets::asset_dir().join(MODEL).is_file() {
        model.gltf = server.load(MODEL);
    }
}

fn spawn(
    mut commands: Commands,
    mut model: ResMut<Model2d>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<bevy::gltf::Gltf>>,
) {
    if model.spawned.is_some() || !server.is_loaded_with_dependencies(&model.gltf) {
        return;
    }
    let Some(scene) = gltfs.get(&model.gltf).and_then(|gltf| {
        gltf.default_scene
            .clone()
            .or_else(|| gltf.scenes.first().cloned())
    }) else {
        return;
    };
    let root = commands
        .spawn((
            WorldAssetRoot(scene),
            Model2dRoot,
            Transform::from_scale(Vec3::splat(UNIT)),
            Visibility::Hidden,
        ))
        .id();
    model.spawned = Some(root);
}

/// Finds the model's joints and sprite meshes once its scene has spawned, and gives its material
/// the 8-bit look.
fn find_nodes(
    mut commands: Commands,
    mut model: ResMut<Model2d>,
    names: Query<(Entity, &Name, Option<&MeshMaterial3d<StandardMaterial>>)>,
    parents: Query<&ChildOf>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(root) = model.spawned.filter(|_| !model.ready) else {
        return;
    };
    let under_root = |mut node: Entity| {
        while let Ok(parent) = parents.get(node) {
            node = parent.parent();
            if node == root {
                return true;
            }
        }
        false
    };
    let mut joints = HashMap::new();
    let mut parts = Vec::new();
    for (entity, name, material) in &names {
        if !under_root(entity) {
            continue;
        }
        if let Some(joint) = name.strip_prefix("Armature_") {
            joints.insert(joint.to_string(), entity);
        } else if let Some((joint, _)) = name.split_once("__") {
            parts.push((entity, joint.to_string()));
            // The joints move far from the mesh's bind pose (the pipe clips), which the bounds
            // the loader computed do not cover.
            commands.entity(entity).insert(NoFrustumCulling);
            if let Some(mut material) = material.and_then(|m| materials.get_mut(&m.0)) {
                finish_material(&mut material, &mut images);
            }
        }
    }
    // The scene spawns over a frame or two: wait until every joint is there.
    if joints.len() < 11 || parts.is_empty() {
        return;
    }
    model.joints = joints;
    model.parts = parts;
    model.ready = true;
}

/// The sprite colours come from a tiny palette texture, so a texel must show as itself: point
/// sampling (a filter would blend neighbouring palette entries along the sprite edges). The
/// material is matte and takes the original's colour gain. GUESS: the original sampler's filter
/// was not read from the archive; point sampling is the only one that keeps a palette intact.
fn finish_material(material: &mut StandardMaterial, images: &mut Assets<Image>) {
    material.base_color = Color::linear_rgb(COLOR_GAIN, COLOR_GAIN, COLOR_GAIN);
    material.perceptual_roughness = 1.;
    material.metallic = 0.;
    material.reflectance = 0.;
    material.alpha_mode = AlphaMode::Opaque;
    if let Some(mut image) = material
        .base_color_texture
        .as_ref()
        .and_then(|texture| images.get_mut(texture))
    {
        image.sampler = ImageSampler::nearest();
    }
}

/// Puts the figure where the actor is, poses its joints, shows the sprite the action's
/// visibility track names, and hides the body while the 8-bit model is on.
fn drive(
    time: Res<Time<Fixed>>,
    mut model: ResMut<Model2d>,
    player: Single<&Player>,
    mut roots: Query<(&mut Transform, &mut Visibility), With<Model2dRoot>>,
    mut pivot: Single<&mut Visibility, (With<Pivot>, Without<Model2dRoot>)>,
    mut transforms: Query<&mut Transform, Without<Model2dRoot>>,
    mut visibilities: Query<&mut Visibility, (Without<Model2dRoot>, Without<Pivot>)>,
) {
    let on = model.ready && player.model_2d();
    let Some((mut place, mut visible)) = model.spawned.and_then(|e| roots.get_mut(e).ok()) else {
        return;
    };
    // The body is hidden whenever the 8-bit one is (it is under the pivot; Cappy is not).
    pivot.set_if_neq(if on {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    });
    let shown = on && !player.damage_hidden() && !player.puppet_hidden();
    visible.set_if_neq(if shown {
        Visibility::Visible
    } else {
        Visibility::Hidden
    });
    if !on {
        return;
    }
    // The actor's pose, exactly: the model's own axes are the pose's (side, up, front), so
    // facing flips as the actor's pose does.
    let m = player.pose_matrix();
    let axis = |c: usize| Vec3::new(m[0][c], m[1][c], m[2][c]);
    let (side, up, front) = (axis(0), axis(1), axis(2));
    let scale = Vec3::new(side.length(), up.length(), front.length());
    place.translation = player.shown;
    place.rotation = Quat::from_mat3(&Mat3::from_cols(
        side / scale.x,
        up / scale.y,
        front / scale.z,
    ));
    place.scale = scale * UNIT;
    // The skeleton: the clip's curves at the frame the animator shows.
    let ahead = time.overstep_fraction();
    if let Some((clip, frame)) = trace::clip_2d(&player, ahead)
        && let Some(locals) = model_2d::locals(clip, frame)
    {
        for (name, sample) in locals {
            let Some(mut transform) = model
                .joints
                .get(name)
                .and_then(|e| transforms.get_mut(*e).ok())
            else {
                continue;
            };
            transform.translation = Vec3::from_array(sample.translation.map(|c| c as f32));
            transform.rotation = Quat::from_array(sample.rotation.map(|c| c as f32));
            transform.scale = Vec3::from_array(sample.scale.map(|c| c as f32));
        }
    }
    // The sprite: the visibility action's track at the animator's counter.
    let action = if model_2d::action_length(player.anim.name).is_some() {
        Some(player.anim.name)
    } else {
        model.action
    };
    let Some(action) = action else {
        return;
    };
    model.action = Some(action);
    for (entity, joint) in &model.parts {
        let show = model_2d::joint_visible(action, player.anim.frame.max(0.) as u32, joint)
            .unwrap_or(false);
        if let Ok(mut visibility) = visibilities.get_mut(*entity) {
            visibility.set_if_neq(if show {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
}
