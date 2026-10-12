use super::*;

pub(super) fn run() {
    bone_masks_and_stain();
    material_roles();
    renamed_scene_keeps_roles_and_targets();
}

#[cfg_attr(test, test)]
fn bone_masks_and_stain() {
    assert!(
        crate::legacy_assets::bone("Armature_Hip", "").leg
            && crate::legacy_assets::bone("Armature_JointRoot", "").leg
    );
    assert!(!crate::legacy_assets::bone("Armature_Eye_JointRoot", "").leg);
    assert!(!crate::legacy_assets::bone("Armature_Face_AllRoot", "").leg);
    assert!(!crate::legacy_assets::bone("Armature_Head_MarioHead", "").leg);
    assert_eq!(soot_blend(0.), 0.);
    assert_eq!(soot_blend(1.), 1.);
    assert!(soot_blend(0.25) > 0.6, "a fading stain still shows");
}

#[cfg_attr(test, test)]
fn material_roles() {
    let mut m = StandardMaterial::default();
    let glass = AssetPart {
        exported: true,
        surface: Surface::Glass,
        ..default()
    };
    finish_material(&glass, &mut m);
    assert_eq!(m.alpha_mode, AlphaMode::Blend);
    assert!(m.double_sided && m.cull_mode.is_none());
    let hair = AssetPart {
        exported: true,
        metal: true,
        surface: Surface::Hair,
        ..default()
    };
    m.base_color_texture = Some(Handle::default());
    finish_material(&hair, &mut m);
    assert!(m.base_color_texture.is_none());
    assert_eq!(m.metallic, 1.);
}

#[cfg_attr(test, test)]
fn renamed_scene_keeps_roles_and_targets() {
    let mut world = World::new();
    world.insert_resource(crate::input::Settings::default());
    world.insert_resource(Wardrobe {
        outfits: vec![],
        status: String::new(),
    });
    let parent = world.spawn(Name::new("renamed parent")).id();
    let id = bevy::animation::AnimationTargetId::from_names(
        [Name::new("source"), Name::new("hip")].iter(),
    );
    let entity = world.spawn((Name::new("unrecognizable"), ChildOf(parent), id,
        bevy::gltf::GltfExtras { value:r#"{"pjv_bone":{"version":1,"key":["stable hip","stable parent"],"leg":true,"hip":true}}"#.into() })).id();
    let mesh = world.spawn((Name::new("unrecognizable mesh"), Mesh3d(Handle::default()),
        bevy::gltf::GltfExtras { value:r#"{"pjv_part":{"version":1,"role":"worn_cap","track":"Cap","pupil":null,"surface":"hair","stain":"cap","exported":true,"metal":true}}"#.into() })).id();
    let bind = world.register_system(bind_metadata);
    world.run_system(bind).unwrap();
    assert_eq!(
        world.get::<AssetPart>(mesh).unwrap().role,
        VisibilityRole::WornCap
    );
    assert_eq!(world.get::<AssetPart>(mesh).unwrap().surface, Surface::Hair);
    let target = bevy::animation::AnimationTargetId::from_names(
        [Name::new("clip root"), Name::new("clip hip")].iter(),
    );
    let mut loaded = Loaded {
        ready: true,
        ..default()
    };
    loaded
        .targets
        .insert(["stable hip".into(), "stable parent".into()], target);
    world.insert_resource(loaded);
    let retarget_system = world.register_system(retarget);
    world.run_system(retarget_system).unwrap();
    assert_eq!(
        *world
            .get::<bevy::animation::AnimationTargetId>(entity)
            .unwrap(),
        target
    );
    *world.get_mut::<Name>(entity).unwrap() = Name::new("renamed again");
    world.resource_mut::<Loaded>().retargeted = false;
    world.run_system(retarget_system).unwrap();
    assert_eq!(
        *world
            .get::<bevy::animation::AnimationTargetId>(entity)
            .unwrap(),
        target
    );
}
