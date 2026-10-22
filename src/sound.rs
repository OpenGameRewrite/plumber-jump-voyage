//! Local effects are loaded only by the optional asset build.
//!
//! What plays and when comes from the original's own sound tables (written
//! by `tools/extract-sounds.py`): sounds attached to animation frames, to
//! events such as landing, and to the thrown cap, each with its volume,
//! pitch and speed curves.
use crate::{
    input::Settings,
    movement::{Fx, Player},
    sound_table::{AnimCursor, Cue, FxContext, Layer, Table, cap_action_name, fx_cues},
    ui::Menu,
};
use bevy::audio::{PlaybackMode, Volume};
use bevy::prelude::*;
use std::collections::BTreeMap;

#[derive(Resource, Default)]
struct Sounds {
    table: Table,
    handles: BTreeMap<String, Vec<Handle<AudioSource>>>,
}
/// A looping sound, tied to the animation or cap state that started it.
#[derive(Component)]
struct Looping(&'static str);
#[derive(Default)]
struct Tracker {
    cursor: AnimCursor,
    /// Where the cap's clip stands, and whether the cap was out last frame.
    cap_cursor: AnimCursor,
    had_cap: bool,
    boosts: u32,
    rng: u32,
    pending: Vec<(f32, String, bool, f32)>,
}
pub fn plugin(app: &mut App) {
    app.init_resource::<Sounds>()
        .add_systems(Startup, load)
        .add_systems(Update, play);
}
fn safe(path: &str) -> bool {
    path.starts_with("local/sounds/")
        && path.ends_with(".wav")
        && !path.contains("..")
        && !path.contains('\\')
        && !path.contains(':')
}
fn load(mut sounds: ResMut<Sounds>, server: Res<AssetServer>) {
    let Some(table) = Table::load() else {
        return;
    };
    for (name, resource) in &table.resources {
        let handles = resource
            .files
            .iter()
            .filter(|p| safe(p))
            .map(|p| server.load(p.clone()))
            .collect();
        sounds.handles.insert(name.clone(), handles);
    }
    sounds.table = table;
}
impl Tracker {
    fn rand(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }
}
/// Everything needed to start sounds in one frame.
struct Ctx<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    sounds: &'a Sounds,
    volume: f32,
    speed: f32,
    footwear: Option<&'a str>,
}
impl Ctx<'_, '_, '_> {
    /// Start one named sound of a group. Loops are tagged so they can stop.
    fn start(&mut self, cap: bool, name: &str, tag: Option<&'static str>, t: &mut Tracker) {
        self.start_with(cap, name, tag, false, t);
    }
    /// `held` loops the sound whatever the table says: its owner stops it.
    fn start_with(
        &mut self,
        cap: bool,
        name: &str,
        tag: Option<&'static str>,
        held: bool,
        t: &mut Tracker,
    ) {
        let group = self.sounds.table.group(cap);
        // Outfits with their own footwear use its foot sounds when present.
        let worn = self.footwear.and_then(|f| {
            let name = if f == "Metal" {
                name.replacen("Run", "RunMetal", 1)
            } else {
                format!("{f}{name}")
            };
            group.plays.get(&name)
        });
        let Some(play) = worn.or_else(|| group.plays.get(name)) else {
            return;
        };
        let layers: Vec<&Layer> = if play.random && !play.layers.is_empty() {
            let i = (t.rand() * play.layers.len() as f32) as usize % play.layers.len();
            vec![&play.layers[i]]
        } else {
            play.layers.iter().collect()
        };
        for layer in layers {
            let (Some(resource), Some(handles)) = (
                self.sounds.table.resources.get(&layer.resource),
                self.sounds.handles.get(&layer.resource),
            ) else {
                continue;
            };
            if handles.is_empty() {
                continue;
            }
            let handle =
                handles[(t.rand() * handles.len() as f32) as usize % handles.len()].clone();
            let (level, pitch) = layer.levels(resource, self.speed, || t.rand());
            let gain = self.volume * level;
            if layer.delay > 0 && tag.is_none() {
                t.pending
                    .push((layer.delay as f32 / 60., layer.resource.clone(), cap, gain));
                continue;
            }
            let looping = (play.looping || held) && tag.is_some();
            let mode = if looping {
                PlaybackMode::Loop
            } else {
                PlaybackMode::Despawn
            };
            let mut e = self.commands.spawn((
                AudioPlayer::new(handle),
                PlaybackSettings {
                    mode,
                    volume: Volume::Linear(gain),
                    speed: pitch,
                    ..default()
                },
            ));
            if let (true, Some(tag)) = (looping, tag) {
                e.insert(Looping(tag));
            }
        }
    }
    fn reaction(&mut self, cap: bool, name: &str, t: &mut Tracker) {
        let group = self.sounds.table.group(cap);
        for play in group.reaction_plays(name) {
            self.start(cap, &play, None, t);
        }
    }
    /// Sounds an action schedules between two frames of its clip.
    fn action(
        &mut self,
        cap: bool,
        name: &str,
        from: f32,
        to: f32,
        tag: &'static str,
        replay: bool,
        carried: &[String],
        t: &mut Tracker,
    ) {
        let group = self.sounds.table.group(cap);
        for play in group.triggered(name, from, to, replay) {
            if from < 0. && carried.contains(&play) {
                continue;
            }
            self.start(cap, &play, Some(tag), t);
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn play(
    mut commands: Commands,
    mut events: MessageReader<Fx>,
    sounds: Res<Sounds>,
    settings: Res<Settings>,
    wardrobe: Option<Res<crate::local_assets::Wardrobe>>,
    menu: Res<Menu>,
    time: Res<Time>,
    player: Option<Single<&Player>>,
    loops: Query<(Entity, &Looping)>,
    mut t: Local<Tracker>,
) {
    let fx: Vec<Fx> = events.read().copied().collect();
    let (Some(p), false) = (player, menu.open || settings.outfit < 3) else {
        for (e, _) in &loops {
            commands.entity(e).despawn();
        }
        return;
    };
    let outfit = wardrobe
        .as_ref()
        .and_then(|w| w.outfits.get(settings.outfit - 3))
        .and_then(|o| o.file.strip_prefix("local/"))
        .and_then(|f| f.strip_suffix(".glb"))
        .unwrap_or("");
    let mut ctx = Ctx {
        commands: &mut commands,
        sounds: &sounds,
        volume: settings.volume,
        speed: p.move_speed,
        footwear: sounds.table.footwear.get(outfit).map(String::as_str),
    };
    // Animation-driven sounds, with loops ending when their clip does.
    let advance = t.cursor.advance(
        p.anim.name,
        p.anim.serial,
        p.trigger_frame(),
        crate::local_assets::looping(p.anim.name),
    );
    // A level sound both clips list keeps sounding across the change.
    let carried = if advance.restarted {
        ctx.sounds
            .table
            .group(false)
            .continuing(advance.previous, p.anim.name)
    } else {
        Vec::new()
    };
    if advance.restarted && carried.is_empty() {
        for (e, tag) in &loops {
            if tag.0 == "anim" {
                ctx.commands.entity(e).despawn();
            }
        }
    }
    if let Some((from, to, with_cap)) = advance.scan {
        ctx.action(
            false,
            p.anim.name,
            from,
            to,
            "anim",
            advance.replay,
            &carried,
            &mut t,
        );
        if with_cap {
            ctx.action(true, p.anim.name, from, to, "anim", false, &[], &mut t);
        }
    }
    if advance.loop_start {
        ctx.action(false, p.anim.name, -1., 0., "anim", true, &[], &mut t);
    }
    // Event sounds.
    let context = FxContext { mode: p.mode };
    let mut held: Vec<&'static str> = Vec::new();
    for f in fx {
        let mut boosts = t.boosts;
        let cues = fx_cues(f, context, &mut boosts);
        t.boosts = boosts;
        for cue in cues {
            match cue {
                Cue::Play(name) => ctx.start(false, name, None, &mut t),
                Cue::Rated(name) => {
                    // The footsteps' curves follow the animation's rate.
                    let speed = std::mem::replace(&mut ctx.speed, p.anim.rate);
                    ctx.start(false, name, None, &mut t);
                    ctx.speed = speed;
                }
                Cue::Hold(name) => {
                    // A held sound's loop is tagged with its own name.
                    held.push(name);
                    if !loops.iter().any(|(_, tag)| tag.0 == name) {
                        ctx.start_with(false, name, Some(name), true, &mut t);
                    }
                }
                Cue::CapPlay(name) => ctx.start(true, name, None, &mut t),
                Cue::Reaction { cap, name } => ctx.reaction(cap, name, &mut t),
            }
        }
    }
    // A held sound ends the frame its cue stops coming.
    for (e, tag) in &loops {
        if !matches!(tag.0, "anim" | "cap") && !held.contains(&tag.0) {
            ctx.commands.entity(e).despawn();
        }
    }
    // The cap's own clip starts its sounds; its loops end with the clip or
    // with the cap.
    match &p.cap {
        Some(cap) => {
            let name = cap_action_name(cap.clip());
            let advance = t
                .cap_cursor
                .advance(name, cap.clip_serial(), cap.clip_frame(), false);
            if advance.restarted {
                for (e, tag) in &loops {
                    if tag.0 == "cap" {
                        ctx.commands.entity(e).despawn();
                    }
                }
            }
            if let Some((from, to, _)) = advance.scan {
                ctx.action(true, name, from, to, "cap", false, &[], &mut t);
            }
        }
        None => {
            if t.had_cap {
                for (e, tag) in &loops {
                    if tag.0 == "cap" {
                        ctx.commands.entity(e).despawn();
                    }
                }
            }
            t.cap_cursor = AnimCursor::default();
        }
    }
    t.had_cap = p.cap.is_some();
    // Delayed layers.
    let dt = time.delta_secs();
    let mut due = Vec::new();
    t.pending.retain_mut(|(wait, name, cap, gain)| {
        *wait -= dt;
        if *wait <= 0. {
            due.push((name.clone(), *cap, *gain));
        }
        *wait > 0.
    });
    for (name, _, gain) in due {
        if let Some(handle) = sounds.handles.get(&name).and_then(|h| h.first()) {
            ctx.commands.spawn((
                AudioPlayer::new(handle.clone()),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(gain)),
            ));
        }
    }
}
#[path = "tests/sound.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
