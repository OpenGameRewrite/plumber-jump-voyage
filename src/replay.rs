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
    geometry::TriangleMesh,
    input::{ACTIONS, Orders, Settings, bound, rotate},
    movement::{
        Course, Player, Solid,
        move_input::{ViewRows, locked_view},
    },
    sound_table::Table,
    trace::{COLUMNS, FrameInfo, Recorder, TraceFile},
};
use bevy::prelude::*;
use std::sync::Arc;

/// Frames the player idles after the teleport before frame 0.
pub const SETTLE_FRAMES: usize = 10;
/// Height above the floor (metres) from which the placed player counts as in the air: the
/// collider's arrows reach 40 cm below the feet.
const AIR_START_HEIGHT: f32 = 0.4;

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
    /// The stage the real game loaded (`stage <Name> [scenario]`), when named.
    pub stage: Option<String>,
    /// Its scenario number, when the script gives one.
    pub scenario: Option<u32>,
    /// Explicit preroll also opts into carrying interpolation across frame zero.
    /// Absent in legacy scripts, whose ten-frame trace convention stays unchanged.
    pub preroll: Option<u32>,
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
                // The real game's probe loads this stage; a replay that has the
                // stage's geometry (`--geometry-dir`) runs in it.
                "stage" => {
                    script.stage = words.get(1).map(|w| w.to_string());
                    script.scenario = words.get(2).and_then(|w| w.parse().ok());
                }
                // The outfit the probe dresses Mario in and whether its HUD
                // shows: the rewrite replays in its own outfit.
                "outfit" | "hud" => {}
                "camera" => {
                    let n = numbers(&words[1..])?;
                    let [yaw] = n[..] else {
                        return Err(at("camera needs yaw_deg"));
                    };
                    script.camera_yaw_deg = Some(yaw);
                }
                "preroll" => {
                    let [_, frames] = words[..] else {
                        return Err(at("preroll needs one nonnegative frame count"));
                    };
                    script.preroll = Some(frames.parse().map_err(|_| {
                        at("preroll needs a nonnegative 32-bit integer frame count")
                    })?);
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
                        stick: Vec2::new(n[0], n[1]),
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
    pub fn preroll_frames(&self) -> usize {
        self.preroll.map_or(SETTLE_FRAMES, |frames| frames as usize)
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
    /// How far the real stick direction is turned from the one `per_frame`
    /// gives, per frame (see `aim_correction`).
    aim: Vec<f32>,
    /// The length the game gave the stick's world direction (a camera tilted
    /// up or down shortens it), per frame.
    gain: Vec<f32>,
    views: Vec<View>,
    /// The exact view matrix the stick is laid on the world with, per frame: the recorded
    /// `bits_cam_view` where the trace has it, else the locked camera's matrix (the probe pins
    /// the view to the script's `camera` yaw, `move_input::locked_view`).
    view_rows: Vec<Option<ViewRows>>,
    locked_rows: Option<ViewRows>,
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
/// How far (degrees) the direction the game's camera really gave the stick in
/// `row` is turned from the one `yaw_deg` gives: the `cam_yaw` column is the
/// locked orbit angle, but the game turns the stick by the line from its camera
/// to the player, which swings while the player moves. The probe logs the
/// world direction it got (`move_input`), so the stick is turned by that.
/// Zero when the row has no stick or no such columns.
fn aim_correction(game: &TraceFile, row: usize, yaw_deg: f32) -> (f32, f32) {
    let pair = |column: &str| -> Option<Vec2> {
        let mut parts = game
            .cell(row, column)?
            .split('/')
            .map(|p| p.parse::<f32>().ok());
        Some(Vec2::new(parts.next()??, parts.next()??))
    };
    let (Some(stick), Some(aim)) = (pair("in_stick"), pair("move_input")) else {
        return (0., 1.);
    };
    if stick.length() < 0.05 || aim.length() < 0.05 {
        return (0., 1.);
    }
    // The game's third number is the length of the world direction: the
    // stick's own length, less what the camera's tilt takes off it.
    let length = game
        .cell(row, "move_input")
        .and_then(|cell| cell.split('/').nth(2))
        .and_then(|part| part.parse::<f32>().ok())
        .filter(|length| *length > 0.)
        .unwrap_or(1.);
    let gain = length / stick.length().min(1.);
    // The same construction as `orders_for`.
    let ours = -rotate(Vec2::new(stick.x, -stick.y), yaw_deg.to_radians());
    let turn = aim.x.atan2(aim.y) - ours.x.atan2(ours.y);
    // The angle is a direction, so fold it into the half turn either way.
    let degrees = (turn + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU)
        .to_degrees()
        - 180.;
    (degrees, gain)
}
/// The 3x4 view matrix the probe logged in bit patterns (`bits_cam_view`), as its three rows.
fn view_rows_of(game: &TraceFile, row: usize) -> Option<ViewRows> {
    let words: Vec<f32> = game
        .cell(row, "bits_cam_view")?
        .split('/')
        .map(|w| {
            u32::from_str_radix(w.trim_start_matches("0x"), 16)
                .ok()
                .map(f32::from_bits)
        })
        .collect::<Option<_>>()?;
    let [a, b, c, _, d, e, f, _, g, h, i, _] = words[..] else {
        return None;
    };
    Some([Vec3::new(a, b, c), Vec3::new(d, e, f), Vec3::new(g, h, i)])
}
impl CameraTrack {
    pub fn new(script: &Script, game: Option<&TraceFile>) -> Result<Self, String> {
        let fixed = script.camera_yaw_deg.unwrap_or(script.start_yaw_deg);
        let locked_rows = script.camera_yaw_deg.map(locked_view);
        let Some(game) = game else {
            return Ok(Self {
                fixed,
                per_frame: Vec::new(),
                aim: Vec::new(),
                gain: Vec::new(),
                views: Vec::new(),
                view_rows: Vec::new(),
                locked_rows,
            });
        };
        let view_rows = (0..game.rows.len())
            .map(|row| view_rows_of(game, row))
            .collect();
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
        let per_frame: Vec<f32> = recorded
            .into_iter()
            .map(|yaw| {
                last = yaw.unwrap_or(last);
                last
            })
            .collect();
        let (aim, gain) = per_frame
            .iter()
            .enumerate()
            .map(|(row, yaw)| aim_correction(game, row, *yaw))
            .unzip();
        Ok(Self {
            fixed,
            per_frame,
            aim,
            gain,
            views,
            view_rows,
            locked_rows,
        })
    }
    /// The view matrix of `frame` for the stick (the last recorded one past the end of the
    /// trace), when the run records it or its camera is locked.
    pub fn view_rows(&self, frame: usize) -> Option<ViewRows> {
        self.view_rows
            .get(frame)
            .or(self.view_rows.last())
            .copied()
            .flatten()
            .or(self.locked_rows)
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
    /// The turn (degrees) the stick's world direction gets on top of the
    /// camera yaw, from the game's logged direction; zero without one.
    pub fn aim_deg(&self, frame: usize) -> f32 {
        self.aim
            .get(frame)
            .or(self.aim.last())
            .copied()
            .unwrap_or(0.)
    }
    /// The factor the game's world direction has over the stick's length.
    pub fn aim_gain(&self, frame: usize) -> f32 {
        self.gain
            .get(frame)
            .or(self.gain.last())
            .copied()
            .unwrap_or(1.)
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
            floor_code: "Ground",
        }],
        ..default()
    }
}
/// The player's orders for one frame, built as the windowed game builds them.
pub fn orders_for(
    span: &Span,
    previous: &[&str],
    camera_yaw_deg: f32,
    aim_deg: f32,
    aim_gain: f32,
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
        // A half turn of the orbit yaw is a negation; spelling it so keeps a
        // stick along an axis exactly on it (`sin(PI)` is not zero in `f32`).
        direction: -rotate(stick, (camera_yaw_deg + aim_deg).to_radians()) * aim_gain,
        forward: -rotate(Vec2::NEG_Y, (camera_yaw_deg + aim_deg).to_radians()),
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
    /// The real stage's collision for the player to run in, instead of the
    /// flat floor.
    pub geometry: Option<Arc<TriangleMesh>>,
}
/// Result of a replay: the CSV text and the sound table's presence.
pub struct Trace {
    pub rows: Vec<Vec<String>>,
    /// `Recorder::bits` of the whole run, settle frames first (numbered -10..-1).
    pub bits: Vec<String>,
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

/// The probe's gcode prints Ground for Skate because it only tests selected
/// codes. Keep recorded contact/normal state, but recover its code from the
/// actual exported floor at the injected position.
fn snapshot_with_geometry(
    game: &TraceFile,
    row: usize,
    course: &Course,
) -> crate::movement::Snapshot {
    let mut snapshot = game.snapshot(row);
    if let Some((codes, _, _)) = &mut snapshot.floor_contact
        && let Some(mesh) = &course.mesh
    {
        let p = snapshot.pos / crate::movement::UNIT;
        if let Some((_, exported)) = mesh.floor_below_with_codes(p.x, p.z, p.y + 40., 0.3) {
            *codes = exported.clone();
        }
    }
    snapshot
}

/// Static InitClipping spheres use FrustumRadar's four side planes. The camera
/// uses CameraPoseUpdater's original 16:9 aspect. Near/far/group clipping and
/// arbitrary camera roll remain outside these recorded fixtures' coverage.
pub(crate) fn update_static_clipping(player: &mut Player, course: &Course, view: Option<View>) {
    let Some(mesh) = &course.mesh else { return };
    let mut clipped = Vec::new();
    let mut invalid = Vec::new();
    if let Some(view) = view {
        let forward = view.forward;
        let side = forward.cross(Vec3::Y).normalize();
        let up = side.cross(forward);
        let slope = (view.fov_deg.to_radians() * 0.5).tan();
        let radius_limit = (slope * slope + 1.).sqrt() * 300.;
        for (host, center, radius) in &mesh.look_clipping {
            let delta = mesh.relative_vec(*center) - view.position * 100.;
            let dot = |a: Vec3, b: Vec3| (a.x * b.x + a.y * b.y) + a.z * b.z;
            let limit = (slope * slope + 1.).sqrt() * radius;
            let horizontal = slope * (16. / 9.);
            let side_limit = (horizontal * horizontal + 1.).sqrt() * radius;
            if dot(delta, up).abs() > dot(delta, forward) * slope + limit
                || dot(delta, side).abs() > dot(delta, forward) * horizontal + side_limit
            {
                invalid.extend(
                    mesh.look_sensors
                        .iter()
                        .enumerate()
                        .filter_map(|(i, s)| (&s.host == host).then_some(i)),
                );
            }
        }
        for (i, fire) in mesh.candle_fire.iter().enumerate() {
            let center = mesh.relative_vec(*fire - Vec3::Y * 200.);
            let delta = center - view.position * 100.;
            let dot = |a: Vec3, b: Vec3| (a.x * b.x + a.y * b.y) + a.z * b.z;
            if dot(delta, up).abs() > dot(delta, forward) * slope + radius_limit {
                clipped.push(i);
                invalid.extend(
                    mesh.look_sensors
                        .iter()
                        .enumerate()
                        .filter_map(|(j, sensor)| {
                            (sensor.stage_position == *fire + Vec3::Y * 10.).then_some(j)
                        }),
                );
            }
        }
    }
    player.set_clipped_candles(clipped);
    player.look.set_invalid_sensors(invalid);
}

fn tick_geysers(course: &mut Course, player: &Player) {
    if let Some(mesh) = &mut course.mesh
        && (!mesh.geysers.is_empty() || !mesh.grab_ceil_bars.is_empty())
    {
        let mesh = Arc::make_mut(mesh);
        mesh.tick_geysers();
        let feet = mesh.to_game_vec(player.position_cm());
        for bar in &mut mesh.grab_ceil_bars {
            bar.tick(
                feet,
                player.attached_grab_ceil_bar() == Some(bar.id.as_str()),
            );
        }
        let count = mesh.grab_ceil_bars.len();
        let first = mesh.look_sensors.len() - count;
        for i in 0..count {
            let centre = mesh.grab_ceil_bars[i].centre;
            let relative = mesh.relative_point(centre.to_array());
            mesh.look_sensors[first + i].stage_position = centre;
            mesh.look_sensors[first + i].position = relative;
        }
    }
}

pub fn replay(script: &Script, options: Options) -> Result<Trace, String> {
    let camera = CameraTrack::new(script, options.camera)?;
    let bindings = Settings::default().bindings;
    let mut course = match &options.geometry {
        Some(mesh) => Course {
            mesh: Some(mesh.clone()),
            ..default()
        },
        None => flat_floor(),
    };
    if let Some(mesh) = &mut course.mesh {
        let mesh = Arc::make_mut(mesh);
        if let Some(seed) = options
            .camera
            .or(options.lockstep)
            .and_then(|g| g.grabceil_seed.as_ref())
        {
            if seed["stage"].as_str() != script.stage.as_deref()
                || seed["scenario"].as_u64() != script.scenario.map(u64::from)
                || seed["frame"].as_i64() != Some(-(script.preroll_frames() as i64))
            {
                return Err("grabceil seed stage/scenario/preroll mismatch".into());
            }
            for bar in &mut mesh.grab_ceil_bars {
                let row = seed["sources"]
                    .as_array()
                    .and_then(|rows| rows.iter().find(|r| r["id"] == bar.id))
                    .ok_or("missing bar seed")?;
                if row["phase"] != "Wait" || row["dead"] != "0" {
                    return Err("unsupported bar seed".into());
                }
                bar.step = row["step"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
                    .ok_or("bar step")?;
                bar.active = row["system"] == "1";
                let mut centre = [0.; 3];
                for (i, axis) in ["x", "y", "z"].into_iter().enumerate() {
                    centre[i] = row[format!("centre_{axis}")]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .ok_or("bar centre")?;
                }
                bar.centre = Vec3::from_array(centre);
            }
        }
        if let Some(seed) = options
            .camera
            .or(options.lockstep)
            .and_then(|g| g.geyser_seed.as_ref())
        {
            if seed["stage"].as_str() != script.stage.as_deref()
                || seed["scenario"].as_u64() != script.scenario.map(u64::from)
                || seed["frame"].as_i64() != Some(-(script.preroll_frames() as i64))
            {
                return Err("geyser seed stage/scenario/preroll mismatch".into());
            }
            for source in &mesh.geysers {
                let row = seed["sources"]
                    .as_array()
                    .and_then(|rows| rows.iter().find(|r| r["id"] == source.id))
                    .ok_or("missing geyser source seed")?;
                if row["phase"] != "Wait"
                    || row["step"] != "0"
                    || row["dead"] != "0"
                    || row["clipped"] != "0"
                {
                    return Err("unsupported geyser seed".into());
                }
            }
        } else {
            mesh.geysers.clear();
        }
    }
    let mut player = Player::spawn_facing(Vec3::ZERO, script.start_yaw_deg);
    player.prepare_recorded_teleport();
    let recorded = options.camera.or(options.lockstep);
    if let (Some(game), Some(mesh)) = (recorded, &options.geometry) {
        let spawn = game.metadata["spawn_abs"].as_array();
        let selected_id = game.metadata["restart_point_id"].as_str();
        let point = mesh.restart_points.iter().find(|point| {
            if let Some(id) = selected_id {
                return point["id"].as_str() == Some(id);
            }
            let position = point["position"].as_array();
            point["kind"].as_str() == Some("stage")
                && spawn
                    .zip(position)
                    .is_some_and(|(a, b)| a[0] == b[0] && a[2] == b[2])
        });
        if let Some(point) = point {
            let p = point["position"]
                .as_array()
                .ok_or("restart position missing")?;
            let absolute = Vec3::new(
                p[0].as_f64().ok_or("restart x")? as f32,
                p[1].as_f64().ok_or("restart y")? as f32,
                p[2].as_f64().ok_or("restart z")? as f32,
            );
            let yaw = point["rotate"][1]
                .as_f64()
                .ok_or("restart rotation missing")? as f32;
            player.death.restart_pose = Some((mesh.relative_vec(absolute) * 0.01, yaw));
        }
        player.death.external_reload = true;
    }
    // Over no floor within reach of the collider's arrows (40 below the feet): the player is
    // in the air.
    if course
        .ground(player.pos)
        .is_none_or(|floor| player.pos.y - floor > AIR_START_HEIGHT)
    {
        player.start_in_air();
    }
    let floor = options
        .camera
        .or(options.lockstep)
        .and_then(TraceFile::floor_material);
    let mut recorder = Recorder::new(
        options.table.clone(),
        options.library.clone(),
        floor.clone(),
    )
    .with_stage(options.geometry.clone());
    let settle = Span {
        frames: 0,
        stick: Vec2::ZERO,
        buttons: Vec::new(),
    };
    // Teleported and idle; the recorder watches so its animation state is
    // warm when frame 0 starts, but nothing is written.
    let preroll = script.preroll_frames();
    for i in 0..preroll {
        let (mut orders, mut info) = orders_for(
            &settle,
            &[],
            camera.yaw_deg(0),
            camera.aim_deg(0),
            camera.aim_gain(0),
            &bindings,
        );
        orders.view = camera.view_rows(0);
        update_static_clipping(&mut player, &course, camera.view(0));
        player.step(&orders, &course);
        info.game_frame = i as i64;
        info.frame = i as i64 - preroll as i64;
        recorder.record(&info, &mut player);
        tick_geysers(&mut course, &player);
    }
    recorder.start_rows(script.preroll.is_some());
    let mut rows = Vec::with_capacity(script.frame_count());
    let mut previous: Vec<&str> = Vec::new();
    let spans: Vec<&Span> = script.frames().collect();
    for (frame, span) in spans.iter().copied().enumerate() {
        if let Some(game) = options.lockstep
            && frame > 0
            && frame - 1 < game.rows.len()
        {
            let last = frame - 1;
            // tryChangeAreaAnim starts its clip immediately before silently
            // choosing AreaAnimWait; replaying the preceding row starts it twice.
            let area_started = game.cell(last, "state") == Some("Wait/AreaAnimWait")
                && game.number(last, "anim_frame") == Some(0.);
            let just_chosen = !area_started
                && game
                    .number(last, "sub_step")
                    .or(game.number(last, "state_step"))
                    == Some(0.);
            let grab_ceil_transition = game
                .cell(last, "state")
                .is_some_and(|state| state.starts_with("GrabCeil/"))
                || just_chosen
                    && last > 0
                    && game
                        .cell(last - 1, "state")
                        .is_some_and(|state| state.starts_with("GrabCeil/"));
            if grab_ceil_transition {
                player.inject_grab_ceil(&snapshot_with_geometry(game, last, &course));
                if let Some(hint) = game.look_hint(last) {
                    player.look.restore(&hint, course.look_sensors());
                }
            } else if !just_chosen {
                player.inject(&snapshot_with_geometry(game, last, &course));
                if let Some(hint) = game.look_hint(last) {
                    player.look.restore(&hint, course.look_sensors());
                }
            } else if last > 0 {
                // A row at step 0 shows a state that was only chosen: its first
                // step, with the clip, the impulse and the sounds, is still to
                // come, and a snapshot cannot hold that. Choose it again from
                // the row before, with that frame's input.
                player.inject(&snapshot_with_geometry(game, last - 1, &course));
                if let Some(hint) = game.look_hint(last - 1) {
                    player.look.restore(&hint, course.look_sensors());
                }
                let before: &[&str] = if last > 1 {
                    &spans[last - 1].buttons
                } else {
                    &[]
                };
                let (mut orders, _) = orders_for(
                    spans[last],
                    before,
                    camera.yaw_deg(last),
                    camera.aim_deg(last),
                    camera.aim_gain(last),
                    &bindings,
                );
                orders.view = camera.view_rows(last);
                let spin = player.spin_input();
                update_static_clipping(&mut player, &course, camera.view(last));
                player.step(&orders, &course);
                player.set_spin_input(spin);
                player.events.clear();
            }
        }
        // The probe stops sampling Mario across scene teardown/loading. A boot-clock gap
        // is an external scene boundary, not extra scripted player updates or an HP sample.
        if frame > 0
            && player.is_dead()
            && let Some(game) = recorded
        {
            let previous_frame = game.number(frame - 1, "game_frame");
            let current_frame = game.number(frame, "game_frame");
            if previous_frame
                .zip(current_frame)
                .is_some_and(|(a, b)| b > a + 1.)
            {
                if !player.death.ready() {
                    return Err(format!("scene reload at {frame} before death-wipe judge"));
                }
                if player.death.restart_pose.is_none() {
                    return Err("scene reload requires exported restart_points and recorded spawn_abs/restart_point_id".into());
                }
                let previous_clip = player.anim.name;
                player.restart_after_death(&course);
                recorder = Recorder::new(
                    options.table.clone(),
                    options.library.clone(),
                    floor.clone(),
                )
                .with_stage(options.geometry.clone());
                recorder.scene_started(previous_clip);
            }
        }
        let (mut orders, mut info) = orders_for(
            span,
            &previous,
            camera.yaw_deg(frame),
            camera.aim_deg(frame),
            camera.aim_gain(frame),
            &bindings,
        );
        orders.view = camera.view_rows(frame);
        // The game's camera (when the trace records it) is what its sound control hears.
        player.set_camera_position(camera.view(frame).map(|view| view.position * 100.));
        // Scene RNG is external to a player-only replay. Feed raw getU32 results
        // at the original getRandom(int,int) call site; the state chooses the clip.
        player.set_damage_random(
            recorded
                .and_then(|game| game.cell(frame, "random"))
                .into_iter()
                .flat_map(|cell| cell.split('|'))
                .filter_map(|item| item.strip_prefix("_ZN2al9getRandomEii+0x24="))
                .filter_map(|hex| u32::from_str_radix(hex, 16).ok()),
        );
        update_static_clipping(&mut player, &course, camera.view(frame));
        let axis = course.mesh.as_ref().and_then(|mesh| {
            mesh.snap_move
                .iter()
                .find(|area| {
                    !player.grounded
                        && area
                            .volume_at(mesh.to_game_vec(player.position_cm()))
                            .is_some()
                })
                .map(|_| Vec3::X)
        });
        if let Some(axis) = axis {
            let input = {
                let v = Vec3::new(info.direction.x, 0., info.direction.y);
                v - axis * v.dot(axis)
            };
            info.direction = Vec2::new(input.x, input.z);
        }
        player.step(&orders, &course);
        info.frame = frame as i64;
        info.game_frame = (preroll + frame) as i64;
        if !course
            .mesh
            .as_ref()
            .is_none_or(|mesh| mesh.grab_ceil_bars.is_empty())
        {
            recorder = recorder.with_stage(course.mesh.clone());
        }
        rows.push(recorder.record(&info, &mut player));
        tick_geysers(&mut course, &player);
        previous = span.buttons.clone();
    }
    Ok(Trace {
        rows,
        bits: std::mem::take(&mut recorder.bits),
    })
}

const USAGE: &str = "usage: plumber-jump-voyage --replay <file.input> --trace <out.csv> \
[--camera <game.csv>] [--lockstep <game.csv>] [--random <game.csv>] [--geometry <stage.json> | --geometry-dir <dir>]
   or: plumber-jump-voyage --replay <file.input> --shots <dir> [--camera <game.csv>] [--geometry-dir <dir>]
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
/// The stage collision a replay runs in, when it is asked for: `--geometry`
/// names an export file; `--geometry-dir` names a folder of exports and picks
/// the one for the stage and scenario the script names, else the ones the
/// game's `game.meta.json` beside the `--camera` or `--lockstep` trace records.
/// A stage's scenarios place different objects, so an export is per scenario:
/// `<stage>.s<scenario>.json` is preferred, and `<stage>.json` serves only when
/// its own `scenario` field is the requested one (or no scenario is known).
/// The script's `start` is the origin of the traces and of the loaded
/// triangles.
fn load_geometry(
    args: &[String],
    script: &Script,
    trace_paths: &[Option<&str>],
) -> Result<Option<Arc<TriangleMesh>>, String> {
    let path = if let Some(file) = flag(args, "--geometry")? {
        std::path::PathBuf::from(file)
    } else if let Some(dir) = flag(args, "--geometry-dir")? {
        let meta = trace_paths.iter().flatten().find_map(|trace| {
            let meta = std::path::Path::new(trace).with_file_name("game.meta.json");
            let text = std::fs::read_to_string(meta).ok()?;
            serde_json::from_str::<serde_json::Value>(&text).ok()
        });
        let named = script
            .stage
            .clone()
            .or_else(|| Some(meta.as_ref()?["stage"].as_str()?.to_string()));
        let scenario = script
            .scenario
            .or_else(|| meta.as_ref()?["scenario_no"].as_u64().map(|n| n as u32));
        let Some(stage) = named else {
            eprintln!("--geometry-dir: the replay names no stage, so it runs on the flat floor");
            return Ok(None);
        };
        match find_export(std::path::Path::new(dir), &stage, scenario) {
            Some(file) => file,
            None => {
                eprintln!(
                    "--geometry-dir: no export of {stage} scenario {scenario:?} (flat floor)"
                );
                return Ok(None);
            }
        }
    } else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mesh = TriangleMesh::from_json(&text, script.start)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    eprintln!(
        "geometry: {} ({} triangles, origin {:.1} {:.1} {:.1})",
        path.display(),
        mesh.triangles.len(),
        script.start.x,
        script.start.y,
        script.start.z
    );
    Ok(Some(Arc::new(mesh)))
}
/// The export of `stage` for `scenario` in `dir` (see `load_geometry`).
fn find_export(
    dir: &std::path::Path,
    stage: &str,
    scenario: Option<u32>,
) -> Option<std::path::PathBuf> {
    if let Some(n) = scenario {
        let specific = dir.join(format!("{stage}.s{n}.json"));
        if specific.exists() {
            return Some(specific);
        }
    }
    let general = dir.join(format!("{stage}.json"));
    let Some(wanted) = scenario else {
        return general.exists().then_some(general);
    };
    let text = std::fs::read_to_string(&general).ok()?;
    let root: serde_json::Value = serde_json::from_str(&text).ok()?;
    // An export without a scenario field predates the per-scenario names.
    let matches = root["scenario"]
        .as_u64()
        .is_none_or(|have| have == u64::from(wanted));
    matches.then_some(general)
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
    let geometry = load_geometry(
        args,
        &script,
        &[flag(args, "--camera")?, flag(args, "--lockstep")?],
    )?;
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
                geometry: geometry.clone(),
            },
        )?;
        if let Some(dir) = std::path::Path::new(trace_path).parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(trace_path, trace.to_csv()).map_err(|e| format!("{trace_path}: {e}"))?;
        if args.iter().any(|a| a == "--bits") {
            let name = format!("{trace_path}.bits");
            std::fs::write(
                &name,
                trace.bits.join(
                    "
",
                ) + "
",
            )
            .map_err(|e| format!("{name}: {e}"))?;
        }
        println!(
            "{} frames (start height {:.1} cm) -> {trace_path}",
            trace.rows.len(),
            script.start.y
        );
    }
    shots_dir
        .map(|dir| {
            let camera = CameraTrack::new(&script, camera.as_ref())?;
            crate::shots::Driver::new(&script, camera, dir.into(), geometry)
        })
        .transpose()
}
#[path = "tests/replay.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
