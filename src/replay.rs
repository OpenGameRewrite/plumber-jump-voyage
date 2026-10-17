//! Headless, deterministic replay of a scripted input run on flat ground,
//! writing a per-frame trace for comparison with the real game.
//!
//! ```text
//! plumber-jump-voyage --replay run.input --trace rewrite.csv
//!     [--camera game.csv] [--lockstep game.csv] [--random game.csv]
//! ```
//!
//! A script holds `start x y z yaw_deg` (game units, yaw 0 = +Z, 90 = +X;
//! `start spawn yaw_deg` is the game's default spawn),
//! an optional `camera yaw_deg`, optional `shot <frame>...` lines, and frame
//! spans `<frames> <lx> <ly> <buttons>` (stick up is +ly; buttons use Switch
//! names joined by `+`, or `-`). The player stands on a flat floor, idles for
//! ten frames, then takes one `Player::step` per scripted frame with orders
//! built exactly as the windowed game builds them from its default layout.
use crate::{
    effect_data::Library,
    input::{ACTIONS, Orders, Settings, bound, rotate},
    movement::{Course, Player, Solid},
    sound_table::Table,
    trace::{COLUMNS, FrameInfo, Recorder, TraceFile},
};
use bevy::prelude::*;

/// Frames the player idles after the teleport before frame 0.
pub const SETTLE_FRAMES: usize = 10;

/// Switch button names and the gamepad buttons the game's bindings use.
const BUTTONS: [(&str, &str); 16] = [
    ("A", "East"),
    ("B", "South"),
    ("X", "North"),
    ("Y", "West"),
    ("L", "LeftTrigger"),
    ("R", "RightTrigger"),
    ("ZL", "LeftTrigger2"),
    ("ZR", "RightTrigger2"),
    ("PLUS", "Start"),
    ("MINUS", "Select"),
    ("LS", "LeftThumb"),
    ("RS", "RightThumb"),
    ("UP", "DPadUp"),
    ("DOWN", "DPadDown"),
    ("LEFT", "DPadLeft"),
    ("RIGHT", "DPadRight"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub frames: u32,
    /// Left stick, +y up.
    pub stick: Vec2,
    pub buttons: Vec<&'static str>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script {
    /// Game units.
    pub start: Vec3,
    pub start_yaw_deg: f32,
    pub camera_yaw_deg: Option<f32>,
    pub spans: Vec<Span>,
    pub shots: Vec<u32>,
}
impl Script {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut script = Self::default();
        let mut has_start = false;
        for (index, raw) in text.lines().enumerate() {
            let at = |m: &str| format!("line {}: {m}", index + 1);
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let words: Vec<&str> = line.split_whitespace().collect();
            let numbers = |words: &[&str]| -> Result<Vec<f32>, String> {
                words
                    .iter()
                    .map(|w| {
                        w.parse::<f32>()
                            .map_err(|_| at(&format!("bad number '{w}'")))
                    })
                    .collect()
            };
            match words[0] {
                "start" if words.get(1) == Some(&"spawn") => {
                    // The game's default spawn: on a flat floor only the facing matters.
                    let n = numbers(&words[2..])?;
                    let [yaw] = n[..] else {
                        return Err(at("start spawn needs yaw_deg"));
                    };
                    script.start = Vec3::ZERO;
                    script.start_yaw_deg = yaw;
                    has_start = true;
                }
                "start" => {
                    let n = numbers(&words[1..])?;
                    let [x, y, z, yaw] = n[..] else {
                        return Err(at("start needs x y z yaw_deg"));
                    };
                    script.start = Vec3::new(x, y, z);
                    script.start_yaw_deg = yaw;
                    has_start = true;
                }
                // Header lines for the real game's probe: the stage it loads, the
                // outfit it dresses Mario in and whether its HUD shows. The
                // rewrite replays on its own flat floor in its own outfit.
                "stage" | "outfit" | "hud" => {}
                "camera" => {
                    let n = numbers(&words[1..])?;
                    let [yaw] = n[..] else {
                        return Err(at("camera needs yaw_deg"));
                    };
                    script.camera_yaw_deg = Some(yaw);
                }
                "shot" => {
                    for w in &words[1..] {
                        let frame = w.parse().map_err(|_| at(&format!("bad frame '{w}'")))?;
                        script.shots.push(frame);
                    }
                }
                _ => {
                    let [frames, lx, ly, buttons] = words[..] else {
                        return Err(at("expected '<frames> <lx> <ly> <buttons>'"));
                    };
                    let frames = frames
                        .parse()
                        .map_err(|_| at(&format!("bad frame count '{frames}'")))?;
                    let n = numbers(&[lx, ly])?;
                    script.spans.push(Span {
                        frames,
                        stick: Vec2::new(n[0], n[1]).clamp_length_max(1.),
                        buttons: parse_buttons(buttons).map_err(|e| at(&e))?,
                    });
                }
            }
        }
        if !has_start {
            return Err("the script has no 'start x y z yaw_deg' line".into());
        }
        Ok(script)
    }
    pub fn frame_count(&self) -> usize {
        self.spans.iter().map(|s| s.frames as usize).sum()
    }
    /// The span each frame belongs to.
    pub fn frames(&self) -> impl Iterator<Item = &Span> {
        self.spans
            .iter()
            .flat_map(|s| std::iter::repeat_n(s, s.frames as usize))
    }
}
fn parse_buttons(text: &str) -> Result<Vec<&'static str>, String> {
    if text == "-" {
        return Ok(Vec::new());
    }
    text.split('+')
        .map(|name| {
            BUTTONS
                .iter()
                .find(|(switch, _)| *switch == name)
                .map(|(switch, _)| *switch)
                .ok_or_else(|| format!("unknown button '{name}'"))
        })
        .collect()
}

/// The camera yaw for each frame, in the game's convention (degrees, 0 looks
/// along +Z, 90 along +X). A game trace's `cam_yaw` column wins; otherwise
/// the script's `camera` line; otherwise the start yaw.
pub struct CameraTrack {
    fixed: f32,
    per_frame: Vec<f32>,
    views: Vec<View>,
}
/// The real game's camera for one frame, in the rewrite's world (metres,
/// start at the origin; the game's axes and Bevy's are both right-handed
/// with +Y up, so only the unit changes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub position: Vec3,
    pub forward: Vec3,
    /// Vertical field of view, degrees.
    pub fov_deg: f32,
}
impl View {
    /// Frame `row` of a game trace: `cam_px/py/pz` (cm, relative to the start)
    /// and the view direction the probe derived from the game's own camera
    /// (`cam_real_yaw/pitch`, degrees; pitch below zero looks down), falling
    /// back to the locked `cam_yaw` and a level view. `None` when the row has
    /// no camera position.
    fn read(game: &TraceFile, row: usize) -> Option<Self> {
        let position = Vec3::new(
            game.number(row, "cam_px")?,
            game.number(row, "cam_py")?,
            game.number(row, "cam_pz")?,
        ) / 100.;
        let yaw = game
            .number(row, "cam_real_yaw")
            .or_else(|| game.number(row, "cam_yaw"))?
            .to_radians();
        let pitch = game
            .number(row, "cam_real_pitch")
            .unwrap_or(0.)
            .to_radians();
        Some(Self {
            position,
            forward: Vec3::new(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                yaw.cos() * pitch.cos(),
            ),
            fov_deg: game.number(row, "cam_fov").unwrap_or(40.),
        })
    }
}
impl CameraTrack {
    pub fn new(script: &Script, game: Option<&TraceFile>) -> Result<Self, String> {
        let fixed = script.camera_yaw_deg.unwrap_or(script.start_yaw_deg);
        let Some(game) = game else {
            return Ok(Self {
                fixed,
                per_frame: Vec::new(),
                views: Vec::new(),
            });
        };
        // Like the yaw: rows without a camera keep the last one, earlier rows
        // take the first. A trace without camera columns gives no views.
        let recorded: Vec<Option<View>> = (0..game.rows.len())
            .map(|row| View::read(game, row))
            .collect();
        let views = match recorded.iter().flatten().next().copied() {
            None => Vec::new(),
            Some(first) => {
                let mut last = first;
                recorded
                    .into_iter()
                    .map(|view| {
                        last = view.unwrap_or(last);
                        last
                    })
                    .collect()
            }
        };
        // Rows without a value keep the last one; rows before the first take it.
        let recorded: Vec<Option<f32>> = (0..game.rows.len())
            .map(|row| game.number(row, "cam_yaw"))
            .collect();
        let first = recorded
            .iter()
            .flatten()
            .next()
            .copied()
            .ok_or("the camera trace has no cam_yaw values")?;
        let mut last = first;
        let per_frame = recorded
            .into_iter()
            .map(|yaw| {
                last = yaw.unwrap_or(last);
                last
            })
            .collect();
        Ok(Self {
            fixed,
            per_frame,
            views,
        })
    }
    /// The game's camera at `frame` (the last one past the end of the trace),
    /// when `--camera` supplied a trace that records it.
    pub fn view(&self, frame: usize) -> Option<View> {
        self.views.get(frame).or(self.views.last()).copied()
    }
    pub fn yaw_deg(&self, frame: usize) -> f32 {
        self.per_frame
            .get(frame)
            .or(self.per_frame.last())
            .copied()
            .unwrap_or(self.fixed)
    }
}

/// The flat course a replay runs on: a floor whose top is at height zero.
/// Nothing else exists, so the start position only matters relative to it.
pub fn flat_floor() -> Course {
    Course {
        solids: vec![Solid {
            center: Vec3::new(0., -0.5, 0.),
            half: Vec3::new(2000., 0.5, 2000.),
            slope: 0.,
            moving: false,
            slippery: false,
        }],
        ..default()
    }
}
/// The player's orders for one frame, built as the windowed game builds them.
pub fn orders_for(
    span: &Span,
    previous: &[&str],
    camera_yaw_deg: f32,
    bindings: &[[String; 2]],
) -> (Orders, FrameInfo) {
    let mut held = [false; ACTIONS.len()];
    let mut edge = [false; ACTIONS.len()];
    for (i, binding) in bindings.iter().enumerate() {
        for (switch, pad) in BUTTONS {
            if span.buttons.contains(&switch) && bound(&binding[1], pad) {
                held[i] = true;
                edge[i] |= !previous.contains(&switch);
            }
        }
    }
    // The game's camera looks along `yaw`; the rewrite's orbit yaw is the
    // opposite way round (stick up walks along minus-sine, minus-cosine).
    let camera_yaw = camera_yaw_deg.to_radians() + std::f32::consts::PI;
    let stick = Vec2::new(span.stick.x, -span.stick.y).clamp_length_max(1.);
    let mut orders = Orders {
        stick,
        direction: rotate(stick, camera_yaw),
        camera_yaw,
        ..default()
    };
    orders.apply_actions(&held, &edge);
    let pressed = span
        .buttons
        .iter()
        .copied()
        .filter(|b| !previous.contains(b))
        .collect();
    let info = FrameInfo {
        frame: 0,
        game_frame: 0,
        stick: span.stick,
        held: span.buttons.clone(),
        pressed,
        camera_yaw_deg,
        direction: orders.direction,
    };
    (orders, info)
}

pub struct Options<'a> {
    pub camera: Option<&'a TraceFile>,
    /// Run in lockstep with this game trace: each step starts from the
    /// game's previous row.
    pub lockstep: Option<&'a TraceFile>,
    pub table: Option<Table>,
    /// The original's particle effects, for their names in the trace.
    pub library: Option<Library>,
}
/// Result of a replay: the CSV text and the sound table's presence.
pub struct Trace {
    pub rows: Vec<Vec<String>>,
}
impl Trace {
    pub fn to_csv(&self) -> String {
        let mut text = COLUMNS.join(",");
        for row in &self.rows {
            text.push('\n');
            text.push_str(&row.join(","));
        }
        text.push('\n');
        text
    }
}

pub fn replay(script: &Script, options: Options) -> Result<Trace, String> {
    let camera = CameraTrack::new(script, options.camera)?;
    let bindings = Settings::default().bindings;
    let course = flat_floor();
    let start_yaw = script.start_yaw_deg.to_radians();
    let mut player = Player::spawn_at(Vec3::ZERO, Vec3::new(start_yaw.sin(), 0., start_yaw.cos()));
    let floor = options
        .camera
        .or(options.lockstep)
        .and_then(TraceFile::floor_material);
    let mut recorder = Recorder::new(options.table, options.library, floor);
    let settle = Span {
        frames: 0,
        stick: Vec2::ZERO,
        buttons: Vec::new(),
    };
    // Teleported and idle; the recorder watches so its animation state is
    // warm when frame 0 starts, but nothing is written.
    for i in 0..SETTLE_FRAMES {
        let (orders, mut info) = orders_for(&settle, &[], camera.yaw_deg(0), &bindings);
        player.step(&orders, &course);
        info.game_frame = i as i64;
        recorder.record(&info, &mut player);
    }
    let mut rows = Vec::with_capacity(script.frame_count());
    let mut previous: Vec<&str> = Vec::new();
    let spans: Vec<&Span> = script.frames().collect();
    for (frame, span) in spans.iter().copied().enumerate() {
        if let Some(game) = options.lockstep
            && frame > 0
            && frame - 1 < game.rows.len()
        {
            let last = frame - 1;
            let just_chosen = game
                .number(last, "sub_step")
                .or(game.number(last, "state_step"))
                == Some(0.);
            if !just_chosen {
                player.inject(&game.snapshot(last));
            } else if last > 0 {
                // A row at step 0 shows a state that was only chosen: its first
                // step, with the clip, the impulse and the sounds, is still to
                // come, and a snapshot cannot hold that. Choose it again from
                // the row before, with that frame's input.
                player.inject(&game.snapshot(last - 1));
                let before: &[&str] = if last > 1 {
                    &spans[last - 1].buttons
                } else {
                    &[]
                };
                let (orders, _) = orders_for(spans[last], before, camera.yaw_deg(last), &bindings);
                player.step(&orders, &course);
                player.events.clear();
            }
        }
        let (orders, mut info) = orders_for(span, &previous, camera.yaw_deg(frame), &bindings);
        player.step(&orders, &course);
        info.frame = frame as i64;
        info.game_frame = (SETTLE_FRAMES + frame) as i64;
        rows.push(recorder.record(&info, &mut player));
        previous = span.buttons.clone();
    }
    Ok(Trace { rows })
}

const USAGE: &str = "usage: plumber-jump-voyage --replay <file.input> --trace <out.csv> \
[--camera <game.csv>] [--lockstep <game.csv>] [--random <game.csv>]
   or: plumber-jump-voyage --replay <file.input> --shots <dir> [--camera <game.csv>]
(--replay --states lists the state names the trace uses)";

fn flag<'a>(args: &'a [String], name: &str) -> Result<Option<&'a str>, String> {
    match args.iter().position(|a| a == name) {
        None => Ok(None),
        Some(i) => args
            .get(i + 1)
            .map(|v| Some(v.as_str()))
            .ok_or_else(|| format!("{name} needs a value\n{USAGE}")),
    }
}
/// Entry point for `--replay`: writes the trace when `--trace` is given, and
/// returns the driver of a rendered replay when `--shots` asks for pictures.
pub fn run_cli(args: &[String]) -> Result<Option<crate::shots::Driver>, String> {
    if args.iter().any(|a| a == "--states") {
        print!("{}", crate::game_names::state_table());
        return Ok(None);
    }
    let script_path = flag(args, "--replay")?.ok_or(USAGE)?;
    let trace_path = flag(args, "--trace")?;
    let shots_dir = flag(args, "--shots")?;
    if trace_path.is_none() && shots_dir.is_none() {
        return Err(format!("--trace or --shots is required\n{USAGE}"));
    }
    let script = Script::parse(
        &std::fs::read_to_string(script_path).map_err(|e| format!("{script_path}: {e}"))?,
    )
    .map_err(|e| format!("{script_path}: {e}"))?;
    let camera = flag(args, "--camera")?.map(TraceFile::read).transpose()?;
    let lockstep = flag(args, "--lockstep")?.map(TraceFile::read).transpose()?;
    // The rewrite's gameplay uses no random numbers; the game's logged
    // choices are read so a bad file is reported, but nothing changes.
    if let Some(path) = flag(args, "--random")? {
        let game = TraceFile::read(path)?;
        let logged = (0..game.rows.len())
            .filter(|row| game.cell(*row, "random").is_some())
            .count();
        eprintln!(
            "--random: {logged} frames log random choices; the rewrite's simulation \
             has no random choices to replace, so the replay is unchanged"
        );
    }
    if let Some(trace_path) = trace_path {
        let table = Table::load();
        if table.is_none() {
            eprintln!(
                "no local sound table (assets/local/sounds/player.json): animation-trigger \
                 sounds and reaction sounds are left out of events and sounds"
            );
        }
        let library = Library::load(&crate::local_assets::asset_dir().join("local/effects"));
        if library.is_none() {
            eprintln!(
                "no local effect library (assets/local/effects): effect events use the stand-in effects' names"
            );
        }
        let trace = replay(
            &script,
            Options {
                camera: camera.as_ref(),
                lockstep: lockstep.as_ref(),
                table,
                library,
            },
        )?;
        if let Some(dir) = std::path::Path::new(trace_path).parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(trace_path, trace.to_csv()).map_err(|e| format!("{trace_path}: {e}"))?;
        println!(
            "{} frames (start height {:.1} cm) -> {trace_path}",
            trace.rows.len(),
            script.start.y
        );
    }
    shots_dir
        .map(|dir| {
            let camera = CameraTrack::new(&script, camera.as_ref())?;
            crate::shots::Driver::new(&script, camera, dir.into())
        })
        .transpose()
}
#[path = "tests/replay.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
