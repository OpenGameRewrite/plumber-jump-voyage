//! Per-frame trace of the player for fidelity comparison: one CSV row per
//! simulation step, in the layout the real-game probe also writes.
//!
//! Everything here is derived from the same state, tables and rules the
//! windowed game uses (animation blend, crossfade, sound and effect choices),
//! so the trace shows what the game would present without opening a window.
//! Columns the rewrite has no data for stay empty; comparison tools treat an
//! empty cell as "missing", never as a value.
use crate::{
    fx,
    local_assets::{Blend, Posed, looping},
    movement::{Cap, CapPhase, FPS, Fx, Move, Player, Snapshot, Throw, UNIT, move_weights},
    sound_table::{AnimCursor, CapWatch, Cue, FxContext, Table, fx_cues, is_voice},
};
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Column names, in file order.
pub const COLUMNS: [&str; 60] = [
    "frame",
    "px",
    "py",
    "pz",
    "vx",
    "vy",
    "vz",
    "fx",
    "fz",
    "ground",
    "state",
    "anim",
    "anim_frame",
    "anim_rate",
    "blend",
    "interp",
    "sub_anim",
    "sub_frame",
    "cam_yaw",
    "events",
    "game_frame",
    "in_stick",
    "in_buttons",
    "qx",
    "qy",
    "qz",
    "qw",
    "ux",
    "uy",
    "uz",
    "speed_h",
    "anim_layers",
    "anim_len",
    "anim_loop",
    "parts",
    "bones",
    "cap_state",
    "cap_px",
    "cap_py",
    "cap_pz",
    "cap_anim",
    "cam_px",
    "cam_py",
    "cam_pz",
    "cam_pitch",
    "effects_live",
    "sounds",
    "floor_normal",
    "state_step",
    "sub_step",
    "move_input",
    "pre_input",
    "counters",
    "triggers",
    "collision",
    "random",
    "model",
    "face",
    "cam_mode",
    "cam_fov",
];

/// How a state is named in the original, and whether that name is a guess.
///
/// The names are the nerves of the original player actor where they are known
/// (`Wait`, `Run`, `Squat`, `Slope`, `Rolling`, `Jump`, `Fall`, `HipDrop`,
/// `HeadSliding`, `LongJump`, `WallAir`, `WallCatch`, `PoleClimb`, `Swim`,
/// `SpinCap`, `DamageFire`), with the sub-state after a slash. Sub-state names
/// the public decompilation does not show are this project's guesses; the
/// `guess` flag marks every entry that is not read from it.
pub struct Nerve {
    pub name: &'static str,
    pub guess: bool,
}
pub fn nerve(mode: Move, anim: &str) -> Nerve {
    let (name, guess) = match mode {
        Move::Idle => ("Wait", false),
        // Walking is the Run nerve at a low stick tilt.
        Move::Walk | Move::Run => ("Run", false),
        Move::Skid => ("Run/Brake", true),
        Move::Turn => ("Run/Turn", true),
        Move::Crouch => ("Squat/Wait", false),
        Move::Crawl => ("Squat/Walk", false),
        Move::Fall => ("Fall", false),
        Move::Jump => ("Jump", false),
        Move::DoubleJump => ("Jump/Jump2nd", true),
        Move::TripleJump => ("Jump/Jump3rd", true),
        Move::LongJump => ("LongJump/Jump", false),
        Move::Backflip => ("Jump/Backflip", true),
        Move::Sideflip => ("Jump/TurnJump", true),
        Move::SpinJump => ("Jump/SpinJump", true),
        Move::SpinDrop => ("Jump/SpinDrop", true),
        Move::GroundSpin => ("SpinCap/GroundSpin", true),
        Move::GroundPound if anim == "HipDropStart" => ("HipDrop/Start", false),
        Move::GroundPound => ("HipDrop/Fall", false),
        Move::PoundLand => ("HipDrop/Land", false),
        Move::PoundJump => ("Jump/HipDropJump", true),
        Move::Roll => ("Rolling", false),
        Move::RollEnd => ("Rolling/End", true),
        Move::RollJump => ("Jump/RollJump", true),
        Move::Dive => ("HeadSliding/Dive", false),
        Move::Slide => ("HeadSliding/Land", true),
        Move::SlopeSlide => ("Slope/Slide", false),
        Move::BurnJump => ("DamageFire/Jump", true),
        Move::BurnBounce => ("DamageFire/Bounce", true),
        Move::FireRun => ("DamageFire/Run", true),
        Move::WallSlide => ("WallAir/Slide", false),
        Move::WallJump => ("WallAir/Jump", false),
        Move::Ledge => ("WallCatch", false),
        Move::Pole => ("PoleClimb", false),
        Move::Swim => ("Swim", false),
        Move::SwimDash => ("Swim/Dash", true),
        Move::CapBounce => ("Jump/CapBounce", true),
        Move::CapVault => ("Jump/CapLeapFrog", true),
        Move::CatchJump => ("Jump/CapCatch", true),
        Move::Throw => ("SpinCap", false),
    };
    Nerve { name, guess }
}
/// Every state with the original name the trace gives it; names that are
/// guesses are marked with a question mark.
pub fn state_table() -> String {
    Move::ALL
        .into_iter()
        .map(|mode| {
            let n = nerve(mode, "HipDropStart");
            format!(
                "{:<16} {}{}\n",
                format!("{mode:?}"),
                n.name,
                if n.guess { " ?" } else { "" }
            )
        })
        .collect()
}
/// The state a recorded nerve name stands for (the reverse of `nerve`).
/// Walking and running share a nerve, so the speed picks between them; a name
/// without a known sub-state matches the first state of that nerve.
pub fn state_from_nerve(name: &str, anim: &str, speed_h: f32) -> Option<Move> {
    let exact: Vec<Move> = Move::ALL
        .into_iter()
        .filter(|m| nerve(*m, anim).name == name)
        .collect();
    let candidates = if exact.is_empty() {
        let family = name.split('/').next().unwrap_or(name);
        Move::ALL
            .into_iter()
            .filter(|m| nerve(*m, anim).name.split('/').next() == Some(family))
            .collect()
    } else {
        exact
    };
    match candidates.as_slice() {
        [] => None,
        [Move::Walk, Move::Run] => Some(if speed_h < 8. { Move::Walk } else { Move::Run }),
        [first, ..] => Some(*first),
    }
}

/// One clip contributing to the pose.
#[derive(Clone)]
struct Layer {
    name: &'static str,
    frame: f32,
    rate: f32,
    weight: f32,
}
/// What the animation blend holds for one frame.
#[derive(Clone, Default)]
struct Pose {
    layers: Vec<Layer>,
}
impl Posed for Pose {
    fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
}
impl Pose {
    /// The pose the controller asks for. The "Move" cycle blends Walk, Run,
    /// Dash and DashFast by speed, with the weights the renderer uses.
    fn of(p: &Player) -> Self {
        let a = p.anim;
        if a.name != "Move" {
            return Self {
                layers: vec![Layer {
                    name: a.name,
                    frame: a.frame,
                    rate: a.rate,
                    weight: 1.,
                }],
            };
        }
        let layers = ["Walk", "Run", "Dash", "DashFast"]
            .into_iter()
            .zip(move_weights(p.move_speed))
            .filter(|(_, w)| *w > 0.001)
            .map(|(name, weight)| Layer {
                name,
                frame: a.frame,
                rate: a.rate,
                weight,
            })
            .collect();
        Self { layers }
    }
    fn dominant(&self) -> &'static str {
        self.layers
            .iter()
            .fold(None::<&Layer>, |best, l| match best {
                Some(b) if b.weight >= l.weight => Some(b),
                _ => Some(l),
            })
            .map_or("", |l| l.name)
    }
}

/// What the replay fed the player this frame.
pub struct FrameInfo {
    pub frame: i64,
    pub game_frame: i64,
    /// The stick as scripted (up is positive).
    pub stick: Vec2,
    pub held: Vec<&'static str>,
    pub pressed: Vec<&'static str>,
    pub camera_yaw_deg: f32,
    /// World-space move direction derived from stick and camera.
    pub direction: Vec2,
}

/// Turns the player after each step into a row of cells.
pub struct Recorder {
    table: Option<Table>,
    cursor: AnimCursor,
    cap_watch: CapWatch,
    boosts: u32,
    blend: Blend<Pose>,
    overlay: f32,
    /// Burst effects still alive, with frames left.
    bursts: Vec<(&'static str, u32)>,
}
impl Recorder {
    pub fn new(table: Option<Table>) -> Self {
        Self {
            table,
            cursor: AnimCursor::default(),
            cap_watch: CapWatch::default(),
            boosts: 0,
            blend: Blend::default(),
            overlay: 0.,
            bursts: Vec::new(),
        }
    }

    /// One sound starting: the event name and, with a sound table, its mix.
    fn play(
        &self,
        cap: bool,
        name: &str,
        speed: f32,
        events: &mut Vec<String>,
        mix: &mut Vec<String>,
    ) {
        events.push(format!(
            "{}:{name}",
            if is_voice(name) { "voice" } else { "se" }
        ));
        let Some(table) = &self.table else {
            return;
        };
        let group = table.group(cap);
        let Some(play) = group.plays.get(name) else {
            return;
        };
        // A random play picks one layer; the trace shows the first one.
        let layers = if play.random {
            &play.layers[..play.layers.len().min(1)]
        } else {
            &play.layers[..]
        };
        for layer in layers {
            let Some(resource) = table.resources.get(&layer.resource) else {
                continue;
            };
            // Random variation is left at its middle (volume at its loudest).
            let mut draws = [0.5, 0.5, 0.].into_iter();
            let (gain, pitch) = layer.levels(resource, speed, || draws.next().unwrap_or(0.));
            mix.push(format!("{name}/{}/{}", num(gain), num(pitch)));
        }
    }
    fn reaction(
        &self,
        cap: bool,
        name: &str,
        speed: f32,
        events: &mut Vec<String>,
        mix: &mut Vec<String>,
    ) {
        events.push(format!("reaction:{name}"));
        if let Some(table) = &self.table {
            for play in table.group(cap).reaction_plays(name) {
                self.play(cap, &play, speed, events, mix);
            }
        }
    }
    fn action(
        &self,
        (cap, name, from, to): (bool, &str, f32, f32),
        speed: f32,
        events: &mut Vec<String>,
        mix: &mut Vec<String>,
    ) {
        if let Some(table) = &self.table {
            for play in table.group(cap).triggered(name, from, to) {
                self.play(cap, &play, speed, events, mix);
            }
        }
    }

    /// Row of cells (in `COLUMNS` order) for the state after a step. The
    /// player's pending events are consumed.
    pub fn record(&mut self, info: &FrameInfo, p: &mut Player) -> Vec<String> {
        let fxs: Vec<Fx> = std::mem::take(&mut p.events);
        let speed = p.move_speed;
        let mut events = Vec::new();
        let mut mix = Vec::new();

        // Sounds attached to animation frames.
        let advance = self.cursor.advance(
            p.anim.name,
            p.anim.serial,
            p.anim.frame,
            looping(p.anim.name),
        );
        if let Some((from, to, with_cap)) = advance.scan {
            self.action((false, p.anim.name, from, to), speed, &mut events, &mut mix);
            if with_cap {
                self.action((true, p.anim.name, from, to), speed, &mut events, &mut mix);
            }
        }
        // Movement events: sounds, then effects.
        let context = FxContext {
            mode: p.mode,
            move_speed: speed,
            grounded: p.grounded,
        };
        let mut started = Vec::new();
        for f in &fxs {
            for cue in fx_cues(*f, context, &mut self.boosts) {
                match cue {
                    Cue::Play(name) => self.play(false, name, speed, &mut events, &mut mix),
                    Cue::Reaction { cap, name } => {
                        self.reaction(cap, name, speed, &mut events, &mut mix)
                    }
                    Cue::Action {
                        cap,
                        name,
                        from,
                        to,
                        ..
                    } => self.action((cap, name, from, to), speed, &mut events, &mut mix),
                }
            }
            started.extend(fx::burst_names(*f, p.flat_speed()));
        }
        // The cap's return swing and catch.
        let change = self
            .cap_watch
            .update(p.cap.is_some(), p.cap.as_ref().is_some_and(|c| c.returning));
        if change.started_return {
            self.reaction(true, "Return", speed, &mut events, &mut mix);
        }
        if change.ended {
            self.reaction(true, "Catch", speed, &mut events, &mut mix);
        }
        for name in &started {
            events.push(format!("effect:{name}"));
        }
        // Effects alive: bursts for their lifetime, plus the state's trails.
        self.bursts.retain_mut(|(_, left)| {
            *left = left.saturating_sub(1);
            *left > 0
        });
        self.bursts
            .extend(started.iter().map(|n| (*n, fx::burst_frames(n))));
        let live: BTreeSet<&str> = self
            .bursts
            .iter()
            .map(|b| b.0)
            .chain(fx::trail_names(p))
            .collect();

        // Animation blend and crossfade, advanced by one frame.
        let dt = 1. / FPS;
        self.blend
            .update(p.anim.name, p.anim.serial, Pose::of(p), dt);
        let pose = &self.blend.current;
        let kept = self.blend.kept();
        self.overlay = if p.sub_anim.is_some() {
            (self.overlay + dt / 0.05).min(1.)
        } else {
            (self.overlay - dt / 0.05).max(0.)
        };
        let upper = if p.grounded {
            "ThrowCapParts"
        } else {
            "ThrowCapJumpParts"
        };
        let mut layers: Vec<String> = Vec::new();
        for (old, share, _) in &self.blend.old {
            for l in &old.layers {
                layers.push(layer_cell(l.name, l.frame, 0., l.weight * share));
            }
        }
        for l in &pose.layers {
            layers.push(layer_cell(l.name, l.frame, l.rate, l.weight * (1. - kept)));
        }
        if self.overlay > 0. {
            let frame = p.sub_anim.map_or(15., |s| s.frame).min(15.9);
            layers.push(layer_cell(upper, frame, 1., self.overlay));
        }
        let blend = if pose.layers.len() > 1 {
            pose.layers
                .iter()
                .map(|l| format!("{}:{}", l.name, num(l.weight)))
                .collect::<Vec<_>>()
                .join("|")
        } else {
            String::new()
        };
        let interp = self
            .blend
            .old
            .last()
            .map(|(old, _, _)| format!("{}:{}", old.dominant(), num(1. - kept)))
            .unwrap_or_default();
        // Clips that need the model's data (eye and hand parts) are not known here.
        let parts = p
            .sub_anim
            .map(|s| format!("upper:{upper}@{}", num(s.frame)))
            .unwrap_or_default();

        let v = p.units_per_frame();
        let yaw = p.facing.x.atan2(p.facing.z);
        let q = Quat::from_rotation_y(yaw);
        let mut cells = Cells::default();
        cells.int(info.frame);
        for axis in [p.pos.x, p.pos.y, p.pos.z] {
            cells.num(axis / UNIT);
        }
        for axis in [v.x, v.y, v.z] {
            cells.num(axis);
        }
        cells.num(p.facing.x);
        cells.num(p.facing.z);
        cells.int(p.grounded as i64);
        cells.text(nerve(p.mode, p.anim.name).name);
        cells.text(if p.anim.name == "Move" {
            pose.dominant()
        } else {
            p.anim.name
        });
        cells.num(p.anim.frame);
        cells.num(p.anim.rate);
        cells.text(&blend);
        cells.text(&interp);
        cells.text(p.sub_anim.map_or("", |s| s.name));
        cells.text(&p.sub_anim.map_or(String::new(), |s| num(s.frame)));
        cells.num(info.camera_yaw_deg);
        cells.text(&events.join(";"));
        // Extended columns.
        cells.int(info.game_frame);
        cells.text(&format!("{}/{}", num(info.stick.x), num(info.stick.y)));
        cells.text(&buttons_cell(&info.held, &info.pressed));
        for c in [q.x, q.y, q.z, q.w, 0., 1., 0.] {
            cells.num(c);
        }
        cells.num(speed);
        cells.text(&layers.join("|"));
        cells.empty(); // anim_len needs the clip data
        cells.int(looping(p.anim.name) as i64 | (p.anim.name == "Move") as i64);
        cells.text(&parts);
        cells.empty(); // bones need the skeleton
        let cap = p.cap.as_ref();
        cells.text(cap.map_or("", cap_state));
        for axis in 0..3 {
            cells.text(&cap.map_or(String::new(), |c| num(c.pos[axis] / UNIT)));
        }
        cells.text(cap.map_or("", cap_anim));
        for _ in 0..4 {
            cells.empty(); // camera position and pitch are the renderer's
        }
        cells.text(&live.into_iter().collect::<Vec<_>>().join("|"));
        cells.text(&mix.join("|"));
        cells.text(&if p.grounded {
            "0.000000/1.000000/0.000000".to_string()
        } else {
            String::new()
        });
        cells.int(p.state_frames() as i64);
        cells.empty(); // sub_step: the rewrite keeps one counter per state
        cells.text(&format!(
            "{}/{}/{}",
            num(info.direction.x),
            num(info.direction.y),
            num(info.direction.length())
        ));
        let counters = p.counters();
        let pending: Vec<String> = counters
            .iter()
            .filter(|(name, value)| name.starts_with("pre_") && *value > 0)
            .map(|(name, value)| format!("{}={value}", name.trim_start_matches("pre_")))
            .collect();
        cells.text(&pending.join("|"));
        let gating: Vec<String> = counters
            .iter()
            .filter(|(name, _)| !name.starts_with("pre_"))
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        cells.text(&gating.join("|"));
        cells.text(
            &fxs.iter()
                .map(|f| fx_name(*f))
                .collect::<Vec<_>>()
                .join("|"),
        );
        let mut contact = vec![format!("ground={}", p.grounded as i32)];
        if p.wall != Vec3::ZERO {
            contact.push(format!(
                "wall={}/{}/{}",
                num(p.wall.x),
                num(p.wall.y),
                num(p.wall.z)
            ));
        }
        cells.text(&contact.join("|"));
        for _ in 0..5 {
            cells.empty(); // random, model, face, cam_mode, cam_fov
        }
        cells.finish()
    }
}

fn layer_cell(name: &str, frame: f32, rate: f32, weight: f32) -> String {
    format!("{name}@{}*{}={}", num(frame), num(rate), num(weight))
}
fn buttons_cell(held: &[&str], pressed: &[&str]) -> String {
    held.iter()
        .map(|b| {
            if pressed.contains(b) {
                format!("!{b}")
            } else {
                b.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}
/// A thrown cap's state, in the original cap actor's terms (guesses).
fn cap_state(cap: &Cap) -> &'static str {
    match cap.phase {
        CapPhase::Out => "Fly",
        CapPhase::Stay => "Stay",
        CapPhase::Return => "Return",
    }
}
fn cap_anim(cap: &Cap) -> &'static str {
    match cap.kind {
        Throw::Spin => "FlyingWaitSpiral",
        Throw::Down => "FlyingWaitRolling",
        _ => "FlyingWait",
    }
}
fn fx_name(f: Fx) -> String {
    format!("{f:?}")
        .chars()
        .take_while(|c| c.is_alphanumeric())
        .collect()
}
/// Fixed-point text; negative zero prints as zero so equal runs diff cleanly.
pub fn num(value: f32) -> String {
    let text = format!("{value:.6}");
    if text == "-0.000000" {
        "0.000000".into()
    } else {
        text
    }
}
/// Cells in column order.
#[derive(Default)]
struct Cells(Vec<String>);
impl Cells {
    fn num(&mut self, value: f32) {
        self.0.push(num(value));
    }
    fn int(&mut self, value: i64) {
        self.0.push(value.to_string());
    }
    fn text(&mut self, value: &str) {
        // Commas separate cells; no name or number here contains one.
        self.0.push(value.replace(',', ";"));
    }
    fn empty(&mut self) {
        self.0.push(String::new());
    }
    fn finish(self) -> Vec<String> {
        debug_assert_eq!(self.0.len(), COLUMNS.len());
        self.0
    }
}

/// A trace file read back (the real game's, for the camera and lockstep).
pub struct TraceFile {
    pub rows: Vec<HashMap<String, String>>,
}
impl TraceFile {
    pub fn read(path: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        Self::parse(&text)
    }
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut lines = text.lines().filter(|l| !l.trim().is_empty());
        let header: Vec<&str> = lines
            .next()
            .ok_or("trace file is empty")?
            .split(',')
            .map(str::trim)
            .collect();
        let rows = lines
            .map(|line| {
                header
                    .iter()
                    .zip(line.split(','))
                    .map(|(k, v)| (k.to_string(), v.trim().to_string()))
                    .collect()
            })
            .collect();
        Ok(Self { rows })
    }
    /// A non-empty cell.
    pub fn cell(&self, row: usize, column: &str) -> Option<&str> {
        self.rows
            .get(row)?
            .get(column)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }
    pub fn number(&self, row: usize, column: &str) -> Option<f32> {
        self.cell(row, column)?.parse().ok()
    }
    /// The injectable player state a row records, for lockstep stepping.
    pub fn snapshot(&self, row: usize) -> Snapshot {
        let n = |c| self.number(row, c).unwrap_or(0.);
        let anim_name = self.cell(row, "anim").unwrap_or("Wait");
        let speed = self
            .number(row, "speed_h")
            .unwrap_or_else(|| (n("vx") * n("vx") + n("vz") * n("vz")).sqrt());
        let mode = self
            .cell(row, "state")
            .and_then(|s| state_from_nerve(s, anim_name, speed));
        let moving_cycle = ["Walk", "Run", "Dash", "DashFast"].contains(&anim_name)
            && matches!(mode, Some(Move::Walk | Move::Run));
        let anim = if moving_cycle {
            // The cycle phase runs over 60 frames whatever the clip's length.
            self.number(row, "anim_len")
                .filter(|len| *len > 0.)
                .map(|len| ("Move", n("anim_frame") / len * 60.))
        } else {
            Some((intern(anim_name), n("anim_frame")))
        };
        let mut counters = Vec::new();
        for column in ["counters", "pre_input"] {
            for item in self.cell(row, column).unwrap_or("").split('|') {
                if let Some((name, value)) = item.split_once('=')
                    && let Ok(value) = value.parse::<f64>()
                {
                    let name = if column == "pre_input" {
                        format!("pre_{name}")
                    } else {
                        name.to_string()
                    };
                    counters.push((name, value as i64));
                }
            }
        }
        Snapshot {
            mode,
            pos: Vec3::new(n("px"), n("py"), n("pz")) * UNIT,
            v: Vec3::new(n("vx"), n("vy"), n("vz")),
            facing: Vec3::new(n("fx"), 0., n("fz")),
            grounded: self.cell(row, "ground") == Some("1"),
            anim,
            state_frames: self.number(row, "state_step").map(|s| s.max(0.) as u32),
            counters,
        }
    }
}
/// A clip name as a `'static` string; each distinct name is kept once.
fn intern(name: &str) -> &'static str {
    use std::sync::{Mutex, OnceLock};
    static NAMES: OnceLock<Mutex<BTreeMap<String, &'static str>>> = OnceLock::new();
    let mut names = NAMES.get_or_init(Default::default).lock().unwrap();
    names
        .entry(name.to_string())
        .or_insert_with(|| Box::leak(name.to_string().into_boxed_str()))
}
#[path = "tests/trace.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
