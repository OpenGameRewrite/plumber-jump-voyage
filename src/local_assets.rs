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
/// Frames the original blends over when a clip starts (its interpolation
/// table; anything unlisted starts at once). Jumps out of the run cycle use
/// the table's short "interp" entries.
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
pub fn interp_frames(name: &str, from: &str) -> f32 {
    match name {
        "Jump" | "JumpReverse" if from == "Move" => 3.,
        "Move" | "RunStart" | "Walk" | "Run" | "Dash" | "Push" | "WallCatch" | "WallCatchEnd" => 5.,
        "Wait" | "Fall" => 6.,
        "JumpBroad" | "JumpCapLeapFrog" | "SpinCap" | "SpinCapEnd" | "SpinCapWait"
        | "SquatWait" | "SwimWait" | "SwimLand" | "SwimStand" => 3.,
        "SpinJumpDownFall" => 4.,
        "SwimWalk" => 7.,
        "PoleWait" => 8.,
        _ => 0.,
    }
}
/// Clips that cycle; every other clip holds its last frame.
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
pub fn looping(name: &str) -> bool {
    matches!(
        name,
        "Wait"
            | "Fall"
            | "Rolling"
            | "HeadSliding"
            | "HipDrop"
            | "SquatWait"
            | "SquatWalk"
            | "WallSlideL"
            | "WallCatch"
            | "PoleWait"
            | "PoleClimb"
            | "PoleFall"
            | "SwimWait"
            | "SwimWalk"
            | "SpinJumpLoop"
            | "SpinJumpDownFall"
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
    #[derive(Component)]
    struct Imported;
    #[derive(Component)]
    struct ImportedCap;
    #[derive(Resource, Default)]
    struct Loaded {
        selection: Option<usize>,
        gltf: Handle<bevy::gltf::Gltf>,
        graph: Handle<AnimationGraph>,
        /// Clip name to graph node and length in seconds.
        nodes: std::collections::BTreeMap<String, (AnimationNodeIndex, f32)>,
        ready: bool,
        model_visible: bool,
        cap_loaded: bool,
        finished: bool,
        /// Upper-body throw clips (ground, air), masked off the legs.
        overlay: Option<(AnimationNodeIndex, AnimationNodeIndex)>,
        overlay_weight: f32,
        masked: bool,
        blend: Blend,
    }
    /// One pose source: weighted clips at fixed times.
    #[derive(Clone, Default)]
    struct Pose {
        clips: Vec<(AnimationNodeIndex, f32, f32)>,
    }
    /// The playing request and the frozen poses it is blending away from.
    #[derive(Default)]
    struct Blend {
        name: &'static str,
        serial: u32,
        current: Pose,
        /// Outgoing poses with their weight and fade per second.
        old: Vec<(Pose, f32, f32)>,
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
        clips: Res<Assets<AnimationClip>>,
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
                let length = clips.get(clip).map_or(1., |c| c.duration());
                loaded.nodes.insert(name.to_string(), (node, length));
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
    /// Look up a clip, following the fallback chain for missing ones.
    fn clip(loaded: &Loaded, mut name: &str) -> Option<(AnimationNodeIndex, f32)> {
        for _ in 0..4 {
            if let Some(found) = loaded.nodes.get(name) {
                return Some(*found);
            }
            name = fallback(name);
        }
        loaded.nodes.get("Wait").copied()
    }
    /// The pose the controller asks for, with clip times in seconds. The run
    /// cycle blends Walk, Run, Dash and DashFast on one shared phase.
    fn target(loaded: &Loaded, p: &crate::movement::Player, ahead: f32) -> Pose {
        let anim = p.anim;
        if anim.name == "Move" {
            let phase = ((anim.frame + anim.rate * ahead) / 60.).rem_euclid(1.);
            let weights = crate::movement::move_weights(p.move_speed);
            let clips = ["Walk", "Run", "Dash", "DashFast"]
                .iter()
                .zip(weights)
                .filter(|(_, w)| *w > 0.001)
                .filter_map(|(n, w)| clip(loaded, n).map(|(node, len)| (node, w, phase * len)))
                .collect();
            return Pose { clips };
        }
        let Some((node, len)) = clip(loaded, anim.name) else {
            return Pose::default();
        };
        let seconds = (anim.frame + anim.rate * ahead).max(0.) / 60.;
        let time = if looping(anim.name) {
            seconds.rem_euclid(len.max(1e-3))
        } else {
            seconds.min(len)
        };
        Pose {
            clips: vec![(node, 1., time)],
        }
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
        let blend = &mut loaded.blend;
        if blend.name != p.anim.name || blend.serial != p.anim.serial {
            let frames = interp_frames(p.anim.name, blend.name);
            let old = std::mem::take(&mut blend.current);
            if frames > 0. && !old.clips.is_empty() {
                // The outgoing pose freezes and fades over the clip's blend frames.
                let total: f32 = 1. - blend.old.iter().map(|o| o.1).sum::<f32>();
                blend.old.push((old, total.max(0.), 60. / frames));
            } else {
                blend.old.clear();
            }
            blend.name = p.anim.name;
            blend.serial = p.anim.serial;
        }
        blend.current = next;
        let dt = time.delta_secs();
        for (_, weight, fade) in &mut blend.old {
            *weight -= *fade * dt;
        }
        blend.old.retain(|o| o.1 > 0.);
        // Final weights: outgoing poses keep their share, the rest is current.
        let kept: f32 = blend.old.iter().map(|o| o.1).sum::<f32>().min(1.);
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
                    let frame = sub.map_or(15., |s| s.frame + ahead);
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
                transform.translation = p.shown_cap.unwrap_or(cap.pos);
                transform.rotation = Quat::from_rotation_y(cap.age * 18.);
            }
        }
        // Blink for a few frames every few seconds; eyes stay shut in a pound.
        let t = time.elapsed_secs();
        let blink =
            (t % 3.7) < 0.11 || (t % 11.3) < 0.09 || p.mode == crate::movement::Move::PoundLand;
        for (name, mut visibility) in &mut meshes {
            let n = name.as_str();
            // The thrown cap's eye model holds every lid state; it flies wide-eyed.
            let show = if n.starts_with("EyeClose") || n.starts_with("EyeHalf") {
                Some(false)
            } else if n.starts_with("EyeOpen") {
                Some(true)
            } else if n.starts_with("EyelidClose") {
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
            if name.as_str().contains("Glass") {
                // Visors and goggles are tinted see-through glass in the original (its
                // glass shader is not exported); the face stays visible behind them.
                m.base_color_texture = None;
                m.base_color = Color::srgba(0.27, 0.27, 0.29, 0.35);
                m.alpha_mode = AlphaMode::Blend;
                m.perceptual_roughness = 0.05;
                m.reflectance = 0.9;
                continue;
            }
            if m.base_color_texture.is_some() {
                continue;
            }
            if metal {
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
    // Missing clips fall back along a chain that ends in the idle loop.
    for name in [
        "DashFast",
        "SpinCapStart3Left",
        "JumpBroad3",
        "LandJump3",
        "Anything",
    ] {
        let mut n = name;
        for _ in 0..4 {
            n = fallback(n);
        }
        assert_eq!(fallback(n), "Wait", "{name}");
    }
    assert_eq!(interp_frames("Jump", "Move"), 3.);
    assert_eq!(interp_frames("Jump", "Wait"), 0.);
    assert!(looping("Wait") && !looping("Jump"));
}
