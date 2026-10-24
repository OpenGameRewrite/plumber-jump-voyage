//! The original particle effects as installed by `tools/extract-effects.py`.
//!
//! `assets/local/effects/effects.json` holds the emitter parameters of every
//! effect Mario and Cappy use, the tables that say when each one starts, and
//! the few meshes particles are drawn with. This module only describes and
//! checks that data; `effect_sim` runs it.
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path, sync::Arc};

/// Largest values accepted from the file; a damaged or hostile file must not
/// be able to ask for unbounded work.
const MAX_FRAMES: i64 = 100_000;
const MAX_COUNT: f32 = 4_096.;
const MAX_KEYS: usize = 64;
const FORMAT_VERSION: u32 = 1;

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Volume {
    #[default]
    Point,
    Circle,
    CircleDivided,
    CircleFill,
    Sphere,
    SphereDivided,
    SphereDivided64,
    SphereFill,
    Cylinder,
    CylinderFill,
    Box,
    BoxFill,
    Line,
    LineDivided,
    Rectangle,
    Primitive,
}

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Blend {
    /// Alpha-tested, depth-writing: the crisp smoke puffs.
    Cutout,
    #[default]
    Alpha,
    Add,
}

/// Where particles live after they are emitted.
#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Follow {
    /// In the effect's own frame: they move and turn with it.
    All,
    /// In the world: they stay where they were born.
    #[default]
    World,
    /// They move with the effect but do not turn with it.
    Translate,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Emission {
    pub one_time: bool,
    pub start: u32,
    pub duration: u32,
    pub interval: u32,
    pub interval_random: u32,
    pub rate: f32,
    /// Percent of the rate that may be randomly removed per emission.
    pub rate_random: f32,
    /// Child emitters that start from the parent's particles (the data's "emitter particle"
    /// children): percent of the parent particle's life they begin at.
    pub child_timing: Option<f32>,
    /// The same field for every emitter. An ordinary child (`child_timing` none) emits from each
    /// parent particle once the particle has lived this percent of its life.
    pub timing: f32,
    pub position_random: f32,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Life {
    pub frames: i32,
    /// Percent of the life that may be randomly removed.
    pub random: i32,
    pub infinite: bool,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Shape {
    pub volume: Volume,
    pub radius: [f32; 3],
    pub form_scale: [f32; 3],
    pub sweep_longitude: f32,
    pub sweep_start: f32,
    pub caliber_ratio: f32,
    pub surface_random: f32,
    pub line_length: f32,
    pub divide_circle: i32,
    pub divide_circle_random: i32,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Velocity {
    pub all_direction: f32,
    pub direction: [f32; 3],
    pub direction_speed: f32,
    pub cone_degrees: f32,
    pub xz_diffusion: f32,
    pub diffusion: [f32; 3],
    /// Percent of the outward speed that may be randomly removed.
    pub random: f32,
    pub inherit: f32,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Forces {
    pub gravity_dir: [f32; 3],
    pub gravity: f32,
    pub world_gravity: bool,
    pub air_resistance: f32,
    pub momentum_random: f32,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Scale {
    pub base: [f32; 3],
    /// Percent of the size that may be randomly removed.
    pub random: [f32; 3],
    /// Multiplier keys over the normalised life: x, y, z, time.
    pub curve: Vec<[f32; 4]>,
    #[serde(rename = "loop")]
    pub looping: bool,
    pub add_velocity: f32,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Rotation {
    pub start: [f32; 3],
    pub start_random: [f32; 3],
    pub add: [f32; 3],
    pub add_random: [f32; 3],
    pub resistance: f32,
    pub reverse: [u8; 3],
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelMode {
    #[default]
    Constant,
    Keys,
    Random,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct ColorChannel {
    pub mode: ChannelMode,
    pub constant: [f32; 3],
    /// Red, green, blue, time.
    pub keys: Vec<[f32; 4]>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct AlphaChannel {
    pub mode: ChannelMode,
    pub constant: f32,
    /// Alpha, time.
    pub keys: Vec<[f32; 2]>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Color {
    pub color0: ColorChannel,
    pub alpha0: AlphaChannel,
    pub color1: ColorChannel,
    pub scale: f32,
    pub tint0: [f32; 4],
    pub tint1: [f32; 4],
    pub loop_color0: bool,
    pub loop_alpha0: bool,
    pub loop_frames: Vec<i32>,
    pub alpha_fade_time: i32,
    pub fade_in_time: i32,
    pub alpha_fade: u8,
    pub alpha_fade_in: u8,
    /// Whether the scale fades out over `alpha_fade_time` too (`is_scale_fade`).
    pub scale_fade: u8,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PatternKind {
    #[default]
    None,
    LifeFit,
    Clamp,
    Loop,
    Random,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Pattern {
    #[serde(rename = "type")]
    pub kind: PatternKind,
    pub count: i32,
    pub frames_per_tile: f32,
    pub random_range: f32,
    pub table: Option<Vec<i32>>,
    pub loop_random: u8,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct TextureSlot {
    pub name: String,
    pub grid: [u32; 2],
    pub pattern: Pattern,
    pub scroll: [f32; 2],
    pub scroll_start: [f32; 2],
    /// Whether the scroll, rotate and scale animations run.
    pub animated: [u8; 3],
}

/// How the texture and the two colours combine (inferred: 1 multiplies the
/// texture by colour 0, 2 blends from colour 0 to colour 1 by the texture).
#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Combiner {
    pub color_process: u8,
    pub alpha_process: u8,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Render {
    pub combiner: Combiner,
    pub blend: Blend,
    pub depth_write: bool,
    pub cull: u8,
    pub alpha_threshold: f32,
    pub billboard: u8,
    pub mesh: Option<String>,
    pub shader_type: String,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Transform {
    pub translate: [f32; 3],
    pub translate_random: [f32; 3],
    pub rotate: [f32; 3],
    pub rotate_random: [f32; 3],
}

/// A key animation of the emitter itself, over the emitter's age in frames.
#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Attachment {
    pub kind: String,
    pub enable: u8,
    #[serde(rename = "loop")]
    pub looping: u8,
    /// x, y, z, frame.
    pub keys: Vec<[f32; 4]>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Emitter {
    pub name: String,
    pub emission: Emission,
    pub life: Life,
    pub shape: Shape,
    pub velocity: Velocity,
    pub forces: Forces,
    pub scale: Scale,
    pub rotation: Rotation,
    pub color: Color,
    pub textures: Vec<Option<TextureSlot>>,
    pub render: Render,
    pub follow: Follow,
    pub transform: Transform,
    pub attachments: Vec<Attachment>,
    pub children: Vec<Emitter>,
}

impl Emitter {
    /// The key animation of the given kind, when present and enabled.
    pub fn attachment(&self, kind: &str) -> Option<&Attachment> {
        self.attachments
            .iter()
            .find(|a| a.kind == kind && a.enable != 0 && !a.keys.is_empty())
    }
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Resource {
    pub emitters: Vec<Emitter>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Mesh {
    pub position: Vec<[f32; 3]>,
    pub uv: Vec<[f32; 2]>,
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct ActionTrigger {
    pub name: String,
    /// Frame of the clip the effect starts on; -1 starts it with the clip.
    pub start: i32,
    /// Keep the effect running after the clip ends.
    pub keep: bool,
    /// Frame of the clip at which the effect is stopped (the table's `EndFrame`).
    pub end: Option<i32>,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct ResourceChoice {
    pub name: String,
    /// Floor material this resource is for; none means the default floor.
    pub material: Option<String>,
    pub joint: Option<String>,
    pub offset: Option<[f32; 3]>,
}

/// One way an effect can start, from the original's effect tables.
#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Trigger {
    pub table: String,
    pub actions: Vec<ActionTrigger>,
    pub reactions: Vec<String>,
    pub joint: Option<String>,
    pub offset: Option<[f32; 3]>,
    pub rotate: Option<[f32; 3]>,
    pub scale: Option<f32>,
    pub flags: Vec<String>,
    pub resources: Vec<ResourceChoice>,
}

impl Trigger {
    pub fn has_flag(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
    }
    /// The resource for a floor material: the entry's own variant, else the
    /// common one. An entry with neither is silent on that floor.
    pub fn resource_for(&self, material: Option<&str>) -> Option<&ResourceChoice> {
        material
            .and_then(|m| {
                self.resources
                    .iter()
                    .find(|r| r.material.as_deref() == Some(m))
            })
            .or_else(|| self.resources.iter().find(|r| r.material.is_none()))
    }
    /// The resource for the default floor (the first one without a material).
    pub fn default_resource(&self) -> Option<&ResourceChoice> {
        self.resources
            .iter()
            .find(|r| r.material.is_none())
            .or(self.resources.first())
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct File {
    version: u32,
    effects: BTreeMap<String, Vec<Trigger>>,
    resources: BTreeMap<String, Resource>,
    meshes: BTreeMap<String, Mesh>,
    textures: Vec<String>,
}

/// Everything loaded from `assets/local/effects`.
#[derive(Default)]
pub struct Library {
    pub effects: BTreeMap<String, Vec<Trigger>>,
    pub resources: BTreeMap<String, Arc<Resource>>,
    pub meshes: BTreeMap<String, Arc<Mesh>>,
    /// Textures with a PNG file installed.
    pub textures: Vec<String>,
}

fn finite3(v: &[f32; 3]) -> bool {
    v.iter().all(|x| x.is_finite())
}

fn valid_emitter(e: &Emitter, depth: usize) -> bool {
    let ok_keys = e.scale.curve.len() <= MAX_KEYS
        && e.color.color0.keys.len() <= MAX_KEYS
        && e.color.alpha0.keys.len() <= MAX_KEYS
        && e.attachments.iter().all(|a| a.keys.len() <= MAX_KEYS)
        && e.scale
            .curve
            .iter()
            .all(|k| k.iter().all(|x| x.is_finite()));
    let sane_numbers = finite3(&e.scale.base)
        && finite3(&e.shape.radius)
        && finite3(&e.velocity.direction)
        && finite3(&e.forces.gravity_dir)
        && e.emission.rate.is_finite()
        && e.emission.rate <= MAX_COUNT
        && e.shape.divide_circle <= MAX_COUNT as i32
        && i64::from(e.life.frames) <= MAX_FRAMES
        && u64::from(e.emission.duration) <= MAX_FRAMES as u64
        && u64::from(e.emission.interval) <= MAX_FRAMES as u64
        && e.render
            .mesh
            .as_deref()
            .is_none_or(|m| m.starts_with("0x") && m.len() < 20);
    let textures_ok = e.textures.iter().flatten().all(|t| {
        t.name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            && t.grid.iter().all(|g| *g <= 64)
    });
    depth < 8
        && ok_keys
        && sane_numbers
        && textures_ok
        && e.children.iter().all(|c| valid_emitter(c, depth + 1))
}

impl Library {
    /// Parse and check a library; returns None for data the game cannot trust.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let file: File = serde_json::from_slice(bytes).ok()?;
        if file.version != FORMAT_VERSION {
            return None;
        }
        let meshes_ok = file.meshes.values().all(|m| {
            m.position.len() < 100_000
                && m.position.iter().all(finite3)
                && m.triangles
                    .iter()
                    .all(|t| t.iter().all(|i| (*i as usize) < m.position.len()))
        });
        let resources_ok = file
            .resources
            .values()
            .all(|r| r.emitters.iter().all(|e| valid_emitter(e, 0)));
        if !meshes_ok || !resources_ok {
            return None;
        }
        Some(Self {
            effects: file.effects,
            resources: file
                .resources
                .into_iter()
                .map(|(k, v)| (k, Arc::new(v)))
                .collect(),
            meshes: file
                .meshes
                .into_iter()
                .map(|(k, v)| (k, Arc::new(v)))
                .collect(),
            textures: file.textures,
        })
    }
    pub fn load(directory: &Path) -> Option<Self> {
        Self::from_bytes(&std::fs::read(directory.join("effects.json")).ok()?)
    }
}

/// Piecewise-linear value of `keys` (value components, then time) at `t`.
pub fn sample_keys<const N: usize>(keys: &[[f32; N]], t: f32, default: [f32; N]) -> [f32; N] {
    let time = N - 1;
    let Some(first) = keys.first() else {
        return default;
    };
    if keys.len() == 1 || t <= first[time] {
        return *first;
    }
    for pair in keys.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if t <= b[time] {
            let span = b[time] - a[time];
            let k = if span > 1e-6 {
                (t - a[time]) / span
            } else {
                1.
            };
            let mut out = *a;
            for i in 0..time {
                out[i] = a[i] + (b[i] - a[i]) * k;
            }
            out[time] = t;
            return out;
        }
    }
    keys[keys.len() - 1]
}

pub fn self_check() {
    let keys = [[1., 0.], [0., 1.]];
    assert_eq!(sample_keys(&keys, 0.25, [9., 0.])[0], 0.75);
    assert_eq!(sample_keys(&keys, 2., [9., 0.])[0], 0.);
    assert_eq!(sample_keys::<2>(&[], 0.5, [9., 0.])[0], 9.);
    let json = br#"{"version":1,"effects":{"Land":[{"table":"Reaction","reactions":["Land"],
        "resources":[{"name":"Puff"},{"name":"PuffSnow","material":"Snow"}]}]},
        "resources":{"Puff":{"emitters":[{"name":"A","shape":{"volume":"circle_divided"},
        "render":{"blend":"cutout"},"follow":"all"}]}},"meshes":{},"textures":[]}"#;
    let library = Library::from_bytes(json).expect("a small library loads");
    let trigger = &library.effects["Land"][0];
    assert_eq!(trigger.default_resource().unwrap().name, "Puff");
    let emitter = &library.resources["Puff"].emitters[0];
    assert_eq!(emitter.shape.volume, Volume::CircleDivided);
    assert_eq!(emitter.render.blend, Blend::Cutout);
    assert_eq!(emitter.follow, Follow::All);
    assert!(Library::from_bytes(b"{\"version\":2}").is_none());
    assert!(Library::from_bytes(b"not json").is_none());
    let hostile = br#"{"version":1,"resources":{"X":{"emitters":[{"emission":{"rate":1e9}}]}}}"#;
    assert!(
        Library::from_bytes(hostile).is_none(),
        "huge emission rates are refused"
    );
    let broken_mesh =
        br#"{"version":1,"meshes":{"0x1":{"position":[[0,0,0]],"triangles":[[0,1,2]]}}}"#;
    assert!(
        Library::from_bytes(broken_mesh).is_none(),
        "mesh indices stay in range"
    );
}
