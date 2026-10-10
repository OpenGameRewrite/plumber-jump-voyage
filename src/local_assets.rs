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
}
fn one() -> f32 {
    1.
}
#[derive(Resource)]
pub struct Wardrobe {
    pub outfits: Vec<Outfit>,
    pub status: String,
}
#[cfg(feature = "local-assets")]
#[derive(Deserialize)]
struct Manifest {
    outfits: Vec<Outfit>,
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
        && outfit.cap_file.as_ref().is_none_or(|p| {
            p.starts_with("local/caps/")
                && p.ends_with(".glb")
                && !p.contains("..")
                && !p.contains('\\')
                && !p.contains(':')
        })
}
#[cfg(feature = "local-assets")]
#[derive(Component)]
struct Imported;
#[cfg(feature = "local-assets")]
#[derive(Component)]
struct ImportedCap;
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
#[cfg(feature = "local-assets")]
#[derive(Resource, Default)]
struct Loaded {
    selection: Option<usize>,
    gltf: Handle<bevy::gltf::Gltf>,
    graph: Handle<AnimationGraph>,
    nodes: std::collections::BTreeMap<String, AnimationNodeIndex>,
    last: String,
    ready: bool,
    model_visible: bool,
    cap_loaded: bool,
}
pub fn plugin(app: &mut App) {
    #[allow(unused_mut)]
    let mut wardrobe = Wardrobe {
        outfits: vec![],
        status: "ROMless | original explorer and scenery".into(),
    };
    #[cfg(feature = "local-assets")]
    match std::fs::read(asset_dir().join("local/manifest.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Manifest>(&b).ok())
    {
        Some(m) if m.outfits.iter().all(validate) => {
            wardrobe.outfits = m.outfits;
            wardrobe.status = format!("{} local outfits available", wardrobe.outfits.len());
        }
        _ => wardrobe.status = "No valid local asset pack. Original explorer available.".into(),
    }
    if let Some(mut settings) = app.world_mut().get_resource_mut::<crate::input::Settings>()
        && settings.outfit >= 3 + wardrobe.outfits.len()
    {
        settings.outfit = 0;
    }
    app.insert_resource(wardrobe).add_systems(Update, colors);
    #[cfg(feature = "local-assets")]
    app.init_resource::<Loaded>().add_systems(
        Update,
        (select, animation, parts)
            .chain()
            .after(crate::world::animate),
    );
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
#[cfg(feature = "local-assets")]
fn select(
    mut commands: Commands,
    settings: Res<crate::input::Settings>,
    mut wardrobe: ResMut<Wardrobe>,
    server: Res<AssetServer>,
    models: Res<Assets<bevy::gltf::Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut loaded: ResMut<Loaded>,
    player: Single<Entity, With<crate::movement::Player>>,
    roots: Query<Entity, With<Imported>>,
    mut original: Query<&mut Visibility, With<crate::world::Body>>,
) {
    if loaded.selection != Some(settings.outfit) {
        for e in &roots {
            commands.entity(e).despawn();
        }
        loaded.selection = Some(settings.outfit);
        loaded.nodes.clear();
        loaded.ready = false;
        loaded.model_visible = false;
        loaded.cap_loaded = false;
        loaded.last.clear();
        for mut v in &mut original {
            *v = Visibility::Visible;
        }
        if let Some(outfit) = settings
            .outfit
            .checked_sub(3)
            .and_then(|i| wardrobe.outfits.get(i))
        {
            loaded.gltf = server.load(outfit.file.clone());
            wardrobe.status = "Loading local model...".into();
        } else {
            loaded.gltf = Handle::default();
        }
    }
    if loaded.ready || settings.outfit < 3 {
        return;
    }
    if server.is_loaded_with_dependencies(&loaded.gltf) {
        let Some(gltf) = models.get(&loaded.gltf) else {
            return;
        };
        let Some(scene) = gltf
            .default_scene
            .clone()
            .or_else(|| gltf.scenes.first().cloned())
        else {
            wardrobe.status = "Local model has no scene. Using explorer.".into();
            loaded.ready = true;
            return;
        };
        let outfit = &wardrobe.outfits[settings.outfit - 3];
        let mut graph = AnimationGraph::new();
        for (name, clip) in &gltf.named_animations {
            let node = graph.add_clip(clip.clone(), 1., graph.root);
            loaded.nodes.insert(name.to_string(), node);
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
        commands.entity(*player).add_child(root);
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
            "Local outfit ready. Movement animation pack absent.".into()
        } else {
            "Local wardrobe ready.".into()
        };
    } else if matches!(
        server.load_state(&loaded.gltf),
        bevy::asset::LoadState::Failed(_)
    ) {
        wardrobe.status = "Local model failed to load. Using original explorer.".into();
        loaded.ready = true;
    }
}
#[cfg(feature = "local-assets")]
fn animation(
    mut commands: Commands,
    p: Single<&crate::movement::Player>,
    settings: Res<crate::input::Settings>,
    wardrobe: Res<Wardrobe>,
    mut loaded: ResMut<Loaded>,
    mut players: Query<(
        Entity,
        &mut AnimationPlayer,
        Option<&AnimationGraphHandle>,
        Option<&mut AnimationTransitions>,
    )>,
) {
    if !loaded.ready || loaded.nodes.is_empty() {
        return;
    }
    let Some(outfit) = settings
        .outfit
        .checked_sub(3)
        .and_then(|i| wardrobe.outfits.get(i))
    else {
        return;
    };
    let state = match p.mode {
        crate::movement::Move::Idle => "idle",
        crate::movement::Move::Walk => "walk",
        crate::movement::Move::Run => "run",
        crate::movement::Move::DoubleJump => "double_jump",
        crate::movement::Move::TripleJump => "triple_jump",
        crate::movement::Move::LongJump => "long_jump",
        crate::movement::Move::Backflip => "backflip",
        crate::movement::Move::Sideflip => "sideflip",
        crate::movement::Move::SpinJump => "spin_jump",
        crate::movement::Move::PoundJump => "pound_jump",
        crate::movement::Move::RollJump => "roll_jump",
        crate::movement::Move::WallSlide => "wall_slide",
        crate::movement::Move::WallJump => "wall_jump",
        crate::movement::Move::Ledge => "ledge",
        crate::movement::Move::Pole => "pole",
        crate::movement::Move::CapVault => "cap_vault",
        crate::movement::Move::CapBounce => "cap_bounce",
        crate::movement::Move::CatchJump => "catch_jump",
        crate::movement::Move::Roll => "roll",
        crate::movement::Move::Dive | crate::movement::Move::Slide => "dive",
        crate::movement::Move::Swim | crate::movement::Move::SwimDash => "swim",
        crate::movement::Move::GroundPound => "pound",
        crate::movement::Move::Crouch | crate::movement::Move::Crawl => "crouch",
        _ => "jump",
    };
    let state = if p.cap.as_ref().is_some_and(|c| c.age < 0.3)
        && p.grounded
        && matches!(
            p.mode,
            crate::movement::Move::Idle | crate::movement::Move::Run | crate::movement::Move::Walk
        ) {
        "throw"
    } else {
        state
    };
    let name = outfit
        .animations
        .get(state)
        .or_else(|| {
            outfit
                .animations
                .get(if p.grounded { "idle" } else { "jump" })
        })
        .cloned()
        .unwrap_or_else(|| state.into());
    let Some(node) = loaded.nodes.get(&name).copied() else {
        return;
    };
    let changed = loaded.last != name;
    for (e, mut player, graph, transitions) in &mut players {
        let mut fresh = AnimationTransitions::new();
        if changed || graph.is_none() {
            let animation = if let Some(mut transitions) = transitions {
                transitions.play(&mut player, node, std::time::Duration::from_millis(90))
            } else {
                fresh.play(&mut player, node, std::time::Duration::ZERO)
            };
            if matches!(
                state,
                "idle"
                    | "run"
                    | "walk"
                    | "swim"
                    | "roll"
                    | "pole"
                    | "ledge"
                    | "crouch"
                    | "wall_slide"
            ) {
                animation.repeat();
            }
        }
        if graph.is_none() {
            commands
                .entity(e)
                .insert((AnimationGraphHandle(loaded.graph.clone()), fresh));
        }
    }
    loaded.last = name;
}
#[cfg(feature = "local-assets")]
fn parts(
    p: Single<&crate::movement::Player>,
    settings: Res<crate::input::Settings>,
    loaded: Res<Loaded>,
    mut hats: Query<&mut Visibility, With<crate::world::Hat>>,
    mut meshes: Query<(&Name, &mut Visibility), (With<Mesh3d>, Without<crate::world::Hat>)>,
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
            transform.translation = cap.pos;
            transform.rotation = Quat::from_rotation_y(cap.age * 18.);
        }
    }
    for (name, mut visibility) in &mut meshes {
        let n = name.as_str();
        if n.starts_with("Eyelid")
            || n.starts_with("EyeClose")
            || n.starts_with("EyeHalf")
            || n.starts_with("Eyeball__EyeMT")
        {
            *visibility = Visibility::Hidden;
        } else if n.starts_with("Cap__") || n.starts_with("CapHair__") {
            *visibility = if p.cap.is_some() {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        } else if n.starts_with("Hair__") {
            *visibility = if p.cap.is_some() {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}
pub fn self_check() {
    let mut o = Outfit {
        name: "Local outfit".into(),
        file: "local/outfit.glb".into(),
        scale: 1.,
        yaw: 0.,
        animations: default(),
        cap_file: None,
    };
    assert!(validate(&o));
    for path in [
        "../file.glb",
        "local/../../x.glb",
        "C:/file.glb",
        "local/x.nsp",
    ] {
        o.file = path.into();
        assert!(!validate(&o));
    }
}
