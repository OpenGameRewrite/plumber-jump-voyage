//! The original particle effects in the game-data build.
//!
//! `effect_triggers` decides which effect starts or stops from the player's
//! animation, hit reactions, cap clip and state; this module places each one
//! (on the player, a joint of the model, or Cappy), steps its particles with
//! `effect_sim` at 60 Hz, and hands them to `effect_render`. Without the
//! installed library nothing here runs and `fx.rs` keeps its procedural
//! stand-ins.
use crate::{
    effect_data::{Library, ResourceChoice, Trigger},
    effect_render::{Painter, ParticleMaterial, View},
    effect_sim::{Attach, Draw, Flat, Instance, Pose},
    effect_triggers::{
        Change, Frame, Key, ONE_SHOT_LIMIT, Triggers, cap_clip, code_driven, reactions_of,
    },
    fx::OriginalEffects,
    movement::{Course, Fx, Player},
    sound_table::FxContext,
    ui::Menu,
};
use bevy::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

/// Effects alive at once; the oldest end first beyond this.
const MAX_LIVE: usize = 96;
/// Resources that emit nothing.
const EMPTY_RESOURCE: &str = "DummyEmitter";
/// Jump dust stays where Mario took off. The tables let it follow him, but the
/// controller leaves the ground at once while the original's dust emitter runs
/// for six frames, which would stack puffs up in a column.
const STAYS_AT_TAKEOFF: [&str; 2] = ["Jump", "ActionJump"];

enum Anchor {
    Player,
    /// A joint of Mario's skeleton.
    Joint(String),
    Cappy,
}

/// Where an effect sits relative to its anchor.
struct Placement {
    anchor: Anchor,
    /// Centimetres, in the anchor's frame.
    offset: Vec3,
    extra_rotation: Quat,
    scale: f32,
    /// Turns with the actor when emitted (else world axes).
    emit_rotate: bool,
}

/// An effect instance and how it is attached.
struct Live {
    /// The table entry that keeps it going; none once it was told to stop.
    key: Option<Key>,
    instance: Instance,
    place: Placement,
    /// Does not emit while the player is in the air.
    needs_ground: bool,
    /// Frames until a never-ending one-shot effect is stopped.
    auto_stop: Option<u32>,
}

#[derive(Resource)]
pub struct Effects {
    library: Library,
    flat: HashMap<String, Arc<Flat>>,
    triggers: Triggers,
    live: Vec<Live>,
    boosts: u32,
    cap_serial: u32,
    right_hand: bool,
    last_cap: Vec3,
    seed: u64,
    joint_names: HashSet<String>,
    installed: HashSet<String>,
    /// Development aid, `PJV_EFFECTS=Name@frame,+Name@frame`: start these effects
    /// on those frames of play (a leading + keeps the effect running).
    debug_starts: Vec<(u32, String, bool)>,
    frames: u32,
}

/// The requests of `PJV_EFFECTS`.
fn debug_requests(text: &str) -> Vec<(u32, String, bool)> {
    text.split(',')
        .filter_map(|item| {
            let (name, frame) = item.trim().split_once('@')?;
            let keep = name.starts_with('+');
            Some((
                frame.parse().ok()?,
                name.trim_start_matches('+').to_string(),
                keep,
            ))
        })
        .collect()
}

impl Effects {
    fn new(library: Library) -> Self {
        let joint_names = library
            .effects
            .values()
            .flatten()
            .flat_map(|t| {
                t.joint
                    .iter()
                    .chain(t.resources.iter().filter_map(|r| r.joint.as_ref()))
            })
            .cloned()
            .collect();
        Self {
            triggers: Triggers::new(&library),
            flat: HashMap::new(),
            installed: library.textures.iter().cloned().collect(),
            library,
            live: Vec::new(),
            boosts: 0,
            cap_serial: 0,
            right_hand: true,
            last_cap: Vec3::ZERO,
            seed: 1,
            joint_names,
            debug_starts: std::env::var("PJV_EFFECTS")
                .map(|t| debug_requests(&t))
                .unwrap_or_default(),
            frames: 0,
        }
    }

    fn flat(&mut self, resource: &str) -> Option<Arc<Flat>> {
        if let Some(flat) = self.flat.get(resource) {
            return Some(flat.clone());
        }
        let flat = Arc::new(Flat::new(self.library.resources.get(resource)?));
        self.flat.insert(resource.to_string(), flat.clone());
        Some(flat)
    }
}

pub fn plugin(app: &mut App) {
    crate::effect_render::plugin(app);
    app.add_systems(Startup, load)
        .add_systems(FixedPostUpdate, tick.run_if(resource_exists::<Effects>))
        .add_systems(
            Update,
            draw.run_if(resource_exists::<Effects>)
                .after(crate::world::animate),
        );
}

fn load(mut commands: Commands) {
    let directory = crate::local_assets::asset_dir().join("local/effects");
    if let Some(library) = Library::load(&directory) {
        commands.insert_resource(Effects::new(library));
        commands.insert_resource(OriginalEffects);
    }
}

/// Approximate joint heights for models without the skeleton (the explorer).
fn fallback_joint(name: &str, p: &Player) -> Vec3 {
    let side = Vec3::Y.cross(p.facing).normalize_or_zero();
    p.pos
        + match name {
            "Hip" | "Spine1" => Vec3::Y * 0.9,
            "Head" => Vec3::Y * 1.5,
            "HandL" => Vec3::Y * 1.0 + side * 0.45,
            "HandR" => Vec3::Y * 1.0 - side * 0.45,
            "SpinCap" => Vec3::Y * 0.5,
            _ => Vec3::ZERO,
        }
}

/// Where an effect's anchor is now, in metres, with its orientation.
fn pose_of(place: &Placement, p: &Player, joints: &HashMap<String, Vec3>, cap: Vec3) -> Pose {
    let yaw = Quat::from_rotation_y(p.facing.x.atan2(p.facing.z));
    let base = match &place.anchor {
        Anchor::Player => p.pos,
        Anchor::Joint(name) => joints
            .get(name)
            .copied()
            .unwrap_or_else(|| fallback_joint(name, p)),
        Anchor::Cappy => cap,
    };
    let frame = if place.emit_rotate {
        yaw
    } else {
        Quat::IDENTITY
    };
    Pose {
        position: base + frame * (place.offset * 0.01 * place.scale),
        rotation: frame * place.extra_rotation,
        scale: place.scale,
    }
}

fn degrees(v: [f32; 3]) -> Quat {
    Quat::from_euler(
        EulerRot::XYZ,
        v[0].to_radians(),
        v[1].to_radians(),
        v[2].to_radians(),
    )
}

/// The resource variant for the floor under the player (only puddles so far).
fn choose<'a>(trigger: &'a Trigger, material: Option<&str>) -> Option<&'a ResourceChoice> {
    material
        .and_then(|m| {
            trigger
                .resources
                .iter()
                .find(|r| r.material.as_deref() == Some(m))
        })
        .or_else(|| trigger.default_resource())
}

impl Effects {
    fn start(
        &mut self,
        key: Key,
        once: bool,
        p: &Player,
        joints: &HashMap<String, Vec3>,
        material: Option<&str>,
    ) {
        let Some(trigger) = self
            .library
            .effects
            .get(&key.effect)
            .and_then(|t| t.get(key.trigger))
            .cloned()
        else {
            return;
        };
        let Some(choice) = choose(&trigger, material) else {
            return;
        };
        if choice.name == EMPTY_RESOURCE {
            return;
        }
        let Some(flat) = self.flat(&choice.name) else {
            return;
        };
        let joint = choice.joint.clone().or(trigger.joint.clone());
        let anchor = if trigger.table == "Cap" {
            Anchor::Cappy
        } else if let Some(name) = joint {
            Anchor::Joint(name)
        } else {
            Anchor::Player
        };
        let offset = choice.offset.or(trigger.offset).unwrap_or([0.; 3]);
        let attach = Attach {
            follow_translation: trigger.has_flag("FollowTrans")
                && !STAYS_AT_TAKEOFF.contains(&key.effect.as_str()),
            follow_rotation: trigger.has_flag("FollowRotate"),
            fade_on_stop: trigger.has_flag("OneTimeFade"),
        };
        let timing = self.library.timing(&trigger);
        self.seed = self.seed.wrapping_add(1);
        let place = Placement {
            anchor,
            offset: Vec3::from_array(offset),
            extra_rotation: trigger.rotate.map_or(Quat::IDENTITY, degrees),
            scale: trigger.scale.unwrap_or(1.),
            emit_rotate: trigger.has_flag("EmitRotate"),
        };
        let pose = pose_of(&place, p, joints, self.last_cap);
        let live = Live {
            key: Some(key.clone()),
            instance: Instance::new(flat, pose, attach, self.seed),
            place,
            needs_ground: trigger.has_flag("NoEmitAtNoCollide"),
            auto_stop: (once && timing.emitting.is_none()).then_some(ONE_SHOT_LIMIT),
        };
        self.live.push(live);
        if self.live.len() > MAX_LIVE {
            self.live.remove(0);
        }
    }

    fn stop(&mut self, key: &Key) {
        for live in &mut self.live {
            if live.key.as_ref() == Some(key) {
                live.key = None;
                live.instance.stop();
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn tick(
    mut effects: ResMut<Effects>,
    p: Single<&Player>,
    course: Res<Course>,
    menu: Res<Menu>,
    mut events: MessageReader<Fx>,
    names: Query<(&Name, &GlobalTransform)>,
) {
    let fx: Vec<Fx> = events.read().copied().collect();
    if menu.open {
        return;
    }
    let effects = &mut *effects;
    let context = FxContext {
        mode: p.mode,
        move_speed: p.move_speed,
        grounded: p.grounded,
    };
    let mut reactions = vec![];
    for f in &fx {
        reactions.extend(reactions_of(*f, context, &mut effects.boosts));
        if matches!(f, Fx::Throw(_)) {
            effects.cap_serial += 1;
            effects.right_hand = !p.anim.name.contains("Left");
        }
    }
    if let Some(cap) = &p.cap {
        effects.last_cap = p.shown_cap.unwrap_or(cap.pos);
    }
    let cap_anim = p.cap.as_ref().map(|c| cap_clip(c, effects.right_hand));
    let code = code_driven(&p);
    let changes = effects.triggers.evaluate(&Frame {
        anim: p.anim.name,
        serial: p.anim.serial,
        frame: p.anim.held(),
        cap_anim,
        cap_serial: effects.cap_serial,
        reactions: &reactions,
        code: &code,
    });
    // Joint positions of the model, when it has them.
    let mut joints = HashMap::new();
    if !effects.joint_names.is_empty() {
        for (name, transform) in &names {
            if let Some(joint) = name.as_str().strip_prefix("Armature_")
                && effects.joint_names.contains(joint)
            {
                joints.insert(joint.to_string(), transform.translation());
            }
        }
    }
    let material = course.puddle(p.pos).then_some("Puddle");
    let now = effects.frames;
    for (_, name, keep) in effects
        .debug_starts
        .clone()
        .into_iter()
        .filter(|r| r.0 == now)
    {
        let key = Key {
            effect: name,
            trigger: 0,
        };
        effects.start(key, !keep, &p, &joints, material);
    }
    effects.frames += 1;
    for change in changes {
        match change {
            Change::Start { key, once, .. } => effects.start(key, once, &p, &joints, material),
            Change::Stop { key } => effects.stop(&key),
        }
    }
    let cap = effects.last_cap;
    for live in &mut effects.live {
        let pose = pose_of(&live.place, &p, &joints, cap);
        live.instance
            .step(Some(pose), !live.needs_ground || p.grounded);
        if let Some(left) = &mut live.auto_stop {
            *left = left.saturating_sub(1);
            if *left == 0 {
                live.instance.stop();
                live.auto_stop = None;
                live.key = None;
            }
        }
    }
    effects.live.retain(|l| !l.instance.finished());
}

#[allow(clippy::too_many_arguments)]
fn draw(
    effects: Res<Effects>,
    camera: Single<&GlobalTransform, With<crate::world::MainCamera>>,
    mut painter: ResMut<Painter>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ParticleMaterial>>,
    mut images: ResMut<Assets<Image>>,
    server: Res<AssetServer>,
) {
    let view = View {
        position: camera.translation(),
        right: camera.right().as_vec3(),
        up: camera.up().as_vec3(),
    };
    let mut buffer: Vec<Draw> = Vec::new();
    let mut draws: Vec<(Draw, &crate::effect_data::Emitter)> = Vec::new();
    for live in &effects.live {
        buffer.clear();
        live.instance.draws(&mut buffer);
        draws.extend(buffer.drain(..).map(|d| {
            let emitter = live.instance.emitter(d.emitter);
            (d, emitter)
        }));
    }
    let meshes_data: HashMap<String, Arc<crate::effect_data::Mesh>> = effects
        .library
        .meshes
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    painter.paint(
        &draws,
        &view,
        &meshes_data,
        &effects.installed,
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut images,
        &server,
    );
}

pub fn self_check() {
    crate::effect_render::self_check();
    let json = br#"{"version":1,"effects":{"Puff":[{"table":"Mario","actions":[{"name":"Jump","start":-1}],
        "flags":["FollowTrans"],"offset":[0,100,0],"resources":[{"name":"A"},{"name":"B","material":"Puddle"}]}]},
        "resources":{"A":{"emitters":[{"emission":{"one_time":true,"duration":2,"interval":1,"rate":1},
        "life":{"frames":5},"scale":{"base":[50,50,50]}}]},"B":{"emitters":[]}}}"#;
    let library = Library::from_bytes(json).expect("test library");
    let mut effects = Effects::new(library);
    let mut p = Player::default();
    p.pos = Vec3::new(1., 2., 3.);
    effects.start(
        Key {
            effect: "Puff".into(),
            trigger: 0,
        },
        false,
        &p,
        &HashMap::new(),
        None,
    );
    assert_eq!(effects.live.len(), 1);
    let pose = pose_of(&effects.live[0].place, &p, &HashMap::new(), Vec3::ZERO);
    assert!(
        (pose.position - Vec3::new(1., 3., 3.)).length() < 1e-5,
        "the table's offset is centimetres above the feet"
    );
    for _ in 0..12 {
        effects.live[0].instance.step(Some(pose), true);
    }
    assert!(effects.live[0].instance.finished());
    let key = Key {
        effect: "Puff".into(),
        trigger: 0,
    };
    effects.start(key.clone(), false, &p, &HashMap::new(), Some("Puddle"));
    assert_eq!(effects.live.len(), 2);
    effects.stop(&key);
    assert!(effects.live.iter().all(|l| l.key.is_none()));
    assert!(
        choose(&effects.library.effects["Puff"][0], Some("Puddle"))
            .unwrap()
            .material
            .is_some()
    );
    assert_eq!(
        choose(&effects.library.effects["Puff"][0], Some("Snow"))
            .unwrap()
            .name,
        "A"
    );
}
