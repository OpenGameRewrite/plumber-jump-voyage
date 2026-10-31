//! The pipes a stage places, drawn where the stage's collision export has them.
//!
//! The pipe actors' models are scenery of the original game: an ordinary pipe linked to a 2D
//! mouth is the `DokanChange2D` model, and the 2D mouth the flat `Dokan2D` sprite (two mirrored
//! variants, of which the first is drawn). They are placed with the actors' final matrices (the
//! 2D mouth's is the one the area snap gave it), and are static: the pipe actor's own
//! animation is not played. Installed with `tools/convert-outfits.py --object`; without the
//! models nothing is drawn.
use crate::movement::{Course, dokan::Dokan};
use bevy::prelude::*;
use std::sync::Arc;

/// The model is in centimetres, the world in metres.
const UNIT: f32 = 0.01;

#[derive(Component)]
struct PipeModel;

pub fn plugin(app: &mut App) {
    app.add_systems(Update, (place_pipes, hide_mirrored_variant));
}

/// The second variant of the 2D mouth's sprite is the first one mirrored, in the same place.
fn hide_mirrored_variant(mut parts: Query<(&Name, &mut Visibility), Added<Name>>) {
    for (name, mut visibility) in &mut parts {
        if name.as_str() == "Dokan2D01__BodyMT" {
            *visibility = Visibility::Hidden;
        }
    }
}

/// The installed model of a pipe, by the kind of pipe it is.
fn model_of(pipe: &Dokan) -> &'static str {
    if pipe.is_2d {
        "local/objects/Dokan2D.glb"
    } else {
        "local/objects/DokanChange2D.glb"
    }
}

/// The pipes of the course's stage, once per loaded stage.
fn place_pipes(
    mut commands: Commands,
    server: Res<AssetServer>,
    course: Res<Course>,
    mut placed_for: Local<Option<usize>>,
    old: Query<Entity, With<PipeModel>>,
) {
    let Some(mesh) = &course.mesh else {
        return;
    };
    let identity = Arc::as_ptr(mesh) as usize;
    if *placed_for == Some(identity) {
        return;
    }
    *placed_for = Some(identity);
    for entity in &old {
        commands.entity(entity).despawn();
    }
    for pipe in &mesh.dokans {
        let model = model_of(pipe);
        if !crate::local_assets::asset_dir().join(model).is_file() {
            continue;
        }
        let m = pipe.matrix;
        let axis = |c: usize| Vec3::new(m[c], m[4 + c], m[8 + c]);
        let (side, up, front) = (axis(0), axis(1), axis(2));
        let rotation = Quat::from_mat3(&Mat3::from_cols(
            side.normalize(),
            up.normalize(),
            front.normalize(),
        ));
        commands.spawn((
            PipeModel,
            WorldAssetRoot(server.load(format!("{model}#Scene0"))),
            Transform {
                // The export's matrices are stage coordinates; the replay's world starts at the
                // stage's start position.
                translation: mesh.relative_vec(axis(3)) * UNIT,
                rotation,
                scale: Vec3::splat(UNIT),
            },
        ));
    }
}
