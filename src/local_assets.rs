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
    app.insert_resource(wardrobe).add_systems(Update, colors);
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
/// Animation state for the current movement, as a manifest key.
/// Returns (key, loops, playback speed, blend seconds).
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
pub fn pose(p: &crate::movement::Player) -> (&'static str, bool, f32, f32) {
    use crate::movement::{Move, Throw};
    let speed = p.flat_speed();
    let throwing = p.cap.as_ref().filter(|c| c.age < 0.35 && !c.returning);
    if let Some(cap) = throwing {
        if cap.kind == Throw::Spin {
            return ("spin_throw", false, 1., 0.06);
        }
        if p.grounded && p.mode == Move::Idle {
            return ("throw", false, 1., 0.06);
        }
        if !p.grounded && matches!(p.mode, Move::Jump | Move::Fall | Move::DoubleJump) {
            return ("throw_air", false, 1., 0.06);
        }
    }
    match p.mode {
        Move::Idle if p.since_land < 0.3 && p.hard_land => ("land_hard", false, 1., 0.05),
        Move::Idle if p.since_land < 0.2 => ("land", false, 1., 0.05),
        Move::Idle => ("idle", true, 1., 0.22),
        Move::Walk => ("walk", true, (speed / 2.6).clamp(0.5, 1.6), 0.15),
        Move::Run => ("run", true, (speed / 8.).clamp(0.75, 1.5), 0.12),
        Move::Skid => ("brake", false, 1., 0.06),
        Move::Crouch => ("crouch", true, 1., 0.1),
        Move::Crawl => ("crawl", true, (speed / 1.6).clamp(0.5, 1.5), 0.1),
        Move::Fall => ("fall", true, 1., 0.2),
        Move::Jump | Move::CapBounce if p.vel.y < -2. && p.timer > 0.5 => ("fall", true, 1., 0.25),
        Move::Jump => ("jump", false, 1., 0.05),
        Move::DoubleJump => ("double_jump", false, 1., 0.05),
        Move::TripleJump => ("triple_jump", false, 1., 0.05),
        Move::LongJump => ("long_jump", false, 1., 0.05),
        Move::Backflip => ("backflip", false, 1., 0.05),
        Move::Sideflip => ("sideflip", false, 1., 0.05),
        Move::SpinJump => ("spin_jump", true, 1., 0.05),
        Move::GroundPound if p.timer < 0.28 => ("pound_start", false, 1., 0.04),
        Move::GroundPound => ("pound", true, 1., 0.05),
        Move::PoundLand => ("pound_land", false, 1., 0.03),
        Move::PoundJump => ("pound_jump", false, 1., 0.05),
        Move::Roll => ("roll", true, (speed / 13.).clamp(0.7, 1.8), 0.06),
        Move::RollEnd => ("roll_end", false, 1., 0.08),
        Move::RollJump => ("roll_jump", false, 1., 0.05),
        Move::Dive => ("dive", false, 1., 0.05),
        Move::Slide => ("slide", true, 1., 0.1),
        Move::WallSlide => ("wall_slide", true, 1., 0.08),
        Move::WallJump => ("wall_jump", false, 1., 0.04),
        Move::Ledge if p.timer < 0.25 => ("ledge_start", false, 1., 0.05),
        Move::Ledge => ("ledge", true, 1., 0.1),
        Move::Pole => ("pole", true, 1., 0.1),
        Move::Swim if speed > 1. => ("swim_move", true, 1., 0.2),
        Move::Swim => ("swim", true, 1., 0.2),
        Move::SwimDash => ("swim_dash", false, 1., 0.08),
        Move::CapBounce => ("cap_bounce", false, 1., 0.04),
        Move::CapVault => ("cap_vault", false, 1., 0.04),
    }
}
/// Key to fall back on when an outfit's manifest lacks a clip.
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
fn fallback(key: &str) -> &'static str {
    match key {
        "land" | "land_hard" | "brake" | "crouch" => "idle",
        "fall" | "throw_air" | "pound_jump" | "roll_jump" | "spin_jump" | "cap_bounce" => "jump",
        "pound_start" | "pound_land" => "pound",
        "roll_end" => "crouch",
        "ledge_start" => "ledge",
        "swim_move" | "swim_dash" => "swim",
        "crawl" => "crouch",
        "spin_throw" => "throw",
        _ => "idle",
    }
}
#[cfg(feature = "local-assets")]
mod imported {
    use super::*;
    #[derive(Component)]
    struct Imported;
    #[derive(Component)]
    struct ImportedCap;
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
        finished: bool,
        /// Upper-body throw clips (ground, air), masked off the legs.
        overlay: Option<(AnimationNodeIndex, AnimationNodeIndex)>,
        overlay_weight: f32,
        masked: bool,
    }
    /// Mask group for hips and legs; the throw overlay leaves them alone.
    const LEGS: u32 = 0;
    fn legs(name: &str) -> bool {
        [
            "Hip",
            "Leg",
            "Foot",
            "Toe",
            "JointRoot",
            "AllRoot",
            "nw4f_root",
        ]
        .iter()
        .any(|n| name.contains(n))
    }
    pub fn plugin(app: &mut App) {
        app.init_resource::<Loaded>().add_systems(
            Update,
            (select, mask, animation, parts, finish)
                .chain()
                .after(crate::world::animate),
        );
    }
    fn select(
        mut commands: Commands,
        settings: Res<crate::input::Settings>,
        mut wardrobe: ResMut<Wardrobe>,
        server: Res<AssetServer>,
        models: Res<Assets<bevy::gltf::Gltf>>,
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
                wardrobe.status = format!("Loading {}...", outfit.name);
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
                wardrobe.status = "This outfit has no model. Using the explorer.".into();
                loaded.ready = true;
                return;
            };
            let outfit = &wardrobe.outfits[settings.outfit - 3];
            let mut graph = AnimationGraph::new();
            for (name, clip) in &gltf.named_animations {
                let node = graph.add_clip(clip.clone(), 1., graph.root);
                loaded.nodes.insert(name.to_string(), node);
            }
            // Blending is a normalized weighted average, so a heavy overlay
            // node dominates the arms and torso while its mask keeps the
            // running legs from the locomotion clip underneath.
            let upper = |key: &str, default: &str| {
                let name = outfit.animations.get(key).map_or(default, String::as_str);
                gltf.named_animations.get(name).cloned()
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
        } else if matches!(
            server.load_state(&loaded.gltf),
            bevy::asset::LoadState::Failed(_)
        ) {
            wardrobe.status = "This outfit failed to load. Using the explorer.".into();
            loaded.ready = true;
        }
    }
    /// Bone entities exist only after the scene spawns; tag the legs then.
    fn mask(
        mut loaded: ResMut<Loaded>,
        mut graphs: ResMut<Assets<AnimationGraph>>,
        targets: Query<(&bevy::animation::AnimationTargetId, &Name)>,
    ) {
        if !loaded.ready || loaded.masked || loaded.overlay.is_none() {
            return;
        }
        let Some(mut graph) = graphs.get_mut(&loaded.graph) else {
            return;
        };
        let mut found = false;
        for (id, name) in &targets {
            if legs(name.as_str()) {
                graph.add_target_to_mask_group(*id, LEGS);
                found |= name.as_str().ends_with("Hip");
            }
        }
        loaded.masked = found;
    }
    fn animation(
        mut commands: Commands,
        time: Res<Time>,
        p: Single<&crate::movement::Player>,
        settings: Res<crate::input::Settings>,
        wardrobe: Res<Wardrobe>,
        mut loaded: ResMut<Loaded>,
        mut fx: MessageReader<crate::movement::Fx>,
        mut players: Query<(
            Entity,
            &mut AnimationPlayer,
            Option<&AnimationGraphHandle>,
            Option<&mut AnimationTransitions>,
        )>,
    ) {
        use crate::movement::{Fx, Move, Throw};
        // A repeated move (two cap jumps, a second roll boost) restarts its clip.
        let mut restarts = 0;
        let mut thrown = false;
        for f in fx.read() {
            match f {
                Fx::Jump(_) | Fx::RollBoost { .. } => restarts += 1,
                Fx::Throw(kind) => thrown |= *kind != Throw::Spin,
                _ => {}
            }
        }
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
        let (mut key, repeat, speed, blend) = pose(&p);
        let mut name = outfit.animations.get(key);
        for _ in 0..3 {
            if name.is_some_and(|n| loaded.nodes.contains_key(n)) {
                break;
            }
            key = fallback(key);
            name = outfit.animations.get(key);
        }
        let Some(name) = name.cloned() else {
            return;
        };
        let Some(node) = loaded.nodes.get(&name).copied() else {
            return;
        };
        let restart = restarts > 0 && key != "roll";
        let changed = loaded.last != name || restart;
        // Throwing while moving: the upper-body clip over whatever the legs do.
        let full_body = matches!(key, "throw" | "throw_air" | "spin_throw");
        let throwing = !full_body
            && p.cap
                .as_ref()
                .is_some_and(|c| c.age < 0.4 && !c.returning && c.kind != Throw::Spin)
            && !matches!(
                p.mode,
                Move::Roll
                    | Move::Dive
                    | Move::Slide
                    | Move::GroundPound
                    | Move::Swim
                    | Move::SwimDash
            );
        let fade = time.delta_secs() / if throwing { 0.06 } else { 0.15 };
        loaded.overlay_weight = if throwing {
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
        for (e, mut player, graph, transitions) in &mut players {
            let mut fresh = AnimationTransitions::new();
            if changed || graph.is_none() {
                let fade = std::time::Duration::from_secs_f32(blend);
                let animation = if let Some(mut transitions) = transitions {
                    transitions.play(&mut player, node, fade)
                } else {
                    fresh.play(&mut player, node, std::time::Duration::ZERO)
                };
                if repeat {
                    animation.repeat();
                }
                animation.replay();
            }
            if let Some(active) = player.animation_mut(node) {
                active.set_speed(speed);
            }
            if let Some((on, off)) = overlay {
                if thrown && throwing {
                    player.stop(off);
                    player.start(on);
                }
                if loaded.overlay_weight == 0. {
                    player.stop(on);
                    player.stop(off);
                } else if let Some(active) = player.animation_mut(on) {
                    active.set_weight(loaded.overlay_weight);
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
    fn parts(
        time: Res<Time>,
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
        // Blink for a few frames every few seconds; eyes stay shut in a pound.
        let t = time.elapsed_secs();
        let blink =
            (t % 3.7) < 0.11 || (t % 11.3) < 0.09 || p.mode == crate::movement::Move::PoundLand;
        for (name, mut visibility) in &mut meshes {
            let n = name.as_str();
            let show = if n.starts_with("EyelidClose") {
                Some(blink)
            } else if n.starts_with("Eyeball__EyePupil") {
                Some(!blink)
            } else if n.starts_with("Eyelid")
                || n.starts_with("EyeHalf")
                || n.starts_with("Eyeball__EyeMT")
            {
                Some(false)
            } else if n.starts_with("Cap__") || n.starts_with("CapHair__") {
                Some(p.cap.is_none())
            } else if n.starts_with("Hair__") {
                Some(p.cap.is_some())
            } else {
                None
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
        settings: Res<crate::input::Settings>,
        wardrobe: Res<Wardrobe>,
        mut loaded: ResMut<Loaded>,
        meshes: Query<(&Name, &MeshMaterial3d<StandardMaterial>)>,
        mut materials: ResMut<Assets<StandardMaterial>>,
    ) {
        if !loaded.model_visible || loaded.finished {
            return;
        }
        let Some(outfit) = settings
            .outfit
            .checked_sub(3)
            .and_then(|i| wardrobe.outfits.get(i))
        else {
            return;
        };
        let metal = outfit.file.contains("Metal");
        let mut seen = false;
        for (name, handle) in &meshes {
            if !name.as_str().contains("__") {
                continue;
            }
            seen = true;
            let Some(mut m) = materials.get_mut(&handle.0) else {
                continue;
            };
            // Capes, coats and skirts are single sheets the original draws from
            // both sides; the export loses that flag.
            m.double_sided = true;
            m.cull_mode = None;
            if m.base_color_texture.is_some() {
                continue;
            }
            if name.as_str().contains("Glass") {
                m.base_color = Color::srgba(0.75, 0.9, 1., 0.3);
                m.alpha_mode = AlphaMode::Blend;
                m.perceptual_roughness = 0.05;
            } else if metal {
                m.base_color = Color::srgb(0.86, 0.88, 0.92);
                m.metallic = 1.;
                m.perceptual_roughness = 0.12;
            }
        }
        loaded.finished = seen;
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
    o.file = "local/outfit.glb".into();
    let w = Wardrobe {
        outfits: vec![o.clone(), o],
        status: String::new(),
    };
    assert_eq!(w.slot(0), 3, "original outfits are listed first");
    assert_eq!(w.slot(2), 0);
    assert_eq!(w.row(0), 2);
    let empty = Wardrobe {
        outfits: vec![],
        status: String::new(),
    };
    assert_eq!(empty.slot(1), 1);
    // Every movement state maps to a clip key with a fallback chain ending in idle.
    let mut p = crate::movement::Player::default();
    let (key, ..) = pose(&p);
    assert_eq!(key, "idle");
    p.mode = crate::movement::Move::RollEnd;
    assert_eq!(fallback(fallback(pose(&p).0)), "idle");
}
