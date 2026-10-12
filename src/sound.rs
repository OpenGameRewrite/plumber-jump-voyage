//! Local effects are loaded only by the optional asset build.
//!
//! What plays and when comes from the original's own sound tables (written
//! by `tools/extract-sounds.py`): sounds attached to animation frames, to
//! events such as landing, and to the thrown cap, each with its volume,
//! pitch and speed curves.
use crate::{
    input::Settings,
    movement::{Fx, Move, Player},
    ui::Menu,
};
use bevy::audio::{PlaybackMode, Volume};
use bevy::prelude::*;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Trigger {
    play: String,
    frame: f32,
    once: bool,
}
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Layer {
    resource: String,
    volume: Option<f32>,
    pitch: f32,
    random_pitch: f32,
    random_volume: f32,
    delay: u32,
    volume_curve: Option<[f32; 4]>,
    pitch_curve: Option<[f32; 4]>,
}
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Play {
    random: bool,
    #[serde(rename = "loop")]
    looping: bool,
    layers: Vec<Layer>,
}
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Group {
    actions: BTreeMap<String, Vec<Trigger>>,
    reactions: BTreeMap<String, Vec<String>>,
    plays: BTreeMap<String, Play>,
}
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Resource {
    files: Vec<String>,
    volume: f32,
    random_semitones: Option<[f32; 2]>,
}
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Table {
    mario: Group,
    cap: Group,
    footwear: BTreeMap<String, String>,
    resources: BTreeMap<String, Resource>,
}
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
    anim: (&'static str, u32),
    frame: f32,
    cap_out: bool,
    cap_returning: bool,
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
    let Some(table) =
        std::fs::read(crate::local_assets::asset_dir().join("local/sounds/player.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Table>(&b).ok())
    else {
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
fn curve(c: Option<[f32; 4]>, input: f32) -> f32 {
    c.map_or(1., |[in_min, in_max, out_min, out_max]| {
        let k = ((input - in_min) / (in_max - in_min).max(1e-6)).clamp(0., 1.);
        out_min + (out_max - out_min) * k
    })
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
        let group = if cap {
            &self.sounds.table.cap
        } else {
            &self.sounds.table.mario
        };
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
            let mut semitones = layer.pitch + (t.rand() * 2. - 1.) * layer.random_pitch;
            if let Some([low, high]) = resource.random_semitones {
                semitones += low + (high - low) * t.rand();
            }
            let pitch = 2f32.powf(semitones / 12.) * curve(layer.pitch_curve, self.speed);
            let gain = self.volume
                * resource.volume
                * layer.volume.unwrap_or(1.)
                * curve(layer.volume_curve, self.speed)
                * (1. - layer.random_volume * t.rand());
            if layer.delay > 0 && tag.is_none() {
                t.pending
                    .push((layer.delay as f32 / 60., layer.resource.clone(), cap, gain));
                continue;
            }
            let mode = if play.looping && tag.is_some() {
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
            if let (true, Some(tag)) = (play.looping, tag) {
                e.insert(Looping(tag));
            }
        }
    }
    fn reaction(&mut self, cap: bool, name: &str, t: &mut Tracker) {
        let group = if cap {
            &self.sounds.table.cap
        } else {
            &self.sounds.table.mario
        };
        for play in group.reactions.get(name).cloned().unwrap_or_default() {
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
        t: &mut Tracker,
    ) {
        let group = if cap {
            &self.sounds.table.cap
        } else {
            &self.sounds.table.mario
        };
        for trigger in group.actions.get(name).cloned().unwrap_or_default() {
            // Unset or negative start frames play as the clip begins.
            let at = trigger.frame.max(0.);
            let crossed = if from < 0. {
                at <= to
            } else {
                from < at && at <= to
            };
            if crossed {
                self.start(cap, &trigger.play, Some(tag), t);
            }
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
    let anim = (p.anim.name, p.anim.serial);
    let looped = crate::local_assets::looping(p.anim.name);
    if anim != t.anim {
        for (e, tag) in &loops {
            if tag.0 == "anim" {
                ctx.commands.entity(e).despawn();
            }
        }
        t.anim = anim;
        t.frame = -1.;
    }
    let frame = p.anim.frame;
    if frame >= t.frame {
        let from = t.frame;
        ctx.action(false, p.anim.name, from, frame, "anim", &mut t);
        ctx.action(true, p.anim.name, from, frame, "anim", &mut t);
    } else if looped {
        // A looping clip wrapped: replay its repeating sounds.
        ctx.action(false, p.anim.name, -1., frame, "anim", &mut t);
    }
    t.frame = frame;
    // Event sounds.
    for f in fx {
        match f {
            Fx::Step { left } => {
                let name = match (p.mode, left) {
                    (Move::Crawl, _) => continue,
                    (Move::Swim | Move::SwimDash, true) => "SwimRunL",
                    (Move::Swim | Move::SwimDash, false) => "SwimRunR",
                    (_, true) => "RunL",
                    (_, false) => "RunR",
                };
                ctx.start(false, name, None, &mut t);
            }
            Fx::Land { .. } => {
                let name = if p.mode == Move::Roll {
                    "RollLand"
                } else if p.move_speed > 3. {
                    "LandRun"
                } else {
                    "Land"
                };
                ctx.reaction(false, name, &mut t);
            }
            Fx::RollStart => t.boosts = 0,
            Fx::RollBoost { .. } => {
                t.boosts += 1;
                let name = if t.boosts == 1 {
                    "RollBoost"
                } else {
                    "RollBoostAgain"
                };
                ctx.reaction(false, name, &mut t);
            }
            Fx::Throw(kind) => {
                ctx.reaction(true, "Throw", &mut t);
                let flight = match kind {
                    crate::movement::Throw::Spin => "FlyingWaitSpiral",
                    crate::movement::Throw::Down if p.grounded => "FlyingWaitRolling",
                    _ => "FlyingWait",
                };
                ctx.action(true, flight, -1., 0., "cap", &mut t);
            }
            Fx::CapBounce => ctx.action(true, "Trample", -1., 60., "none", &mut t),
            Fx::Quenched => ctx.reaction(false, "FireQuench", &mut t),
            Fx::Burned(again) => {
                ctx.reaction(false, if again { "BurnBounce" } else { "BurnJump" }, &mut t)
            }
            _ => {}
        }
    }
    // The cap: return swing, catch, and its flight loop stopping.
    let out = p.cap.is_some();
    let returning = p.cap.as_ref().is_some_and(|c| c.returning);
    if returning && !t.cap_returning {
        ctx.reaction(true, "Return", &mut t);
    }
    if t.cap_out && !out {
        for (e, tag) in &loops {
            if tag.0 == "cap" {
                ctx.commands.entity(e).despawn();
            }
        }
        if p.events.is_empty() {
            ctx.reaction(true, "Catch", &mut t);
        }
    }
    t.cap_out = out;
    t.cap_returning = returning;
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
