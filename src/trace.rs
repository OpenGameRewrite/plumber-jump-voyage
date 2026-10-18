//! Per-frame trace of the player for fidelity comparison: one CSV row per
//! simulation step, in the layout the real-game probe also writes.
//!
//! Everything here is derived from the same state, tables and rules the
//! windowed game uses (animation blend, crossfade, sound and effect choices),
//! so the trace shows what the game would present without opening a window.
//! Columns the rewrite has no data for stay empty; comparison tools treat an
//! empty cell as "missing", never as a value.
use crate::{
    effect_data::Library,
    effect_triggers::Tracker,
    fx,
    game_columns::{collision_cell, counters_cell, counters_from_game, triggers_cell},
    game_names::state_from_nerve,
    local_assets::{Blend, Posed, looping},
    movement::{Anim, FPS, Fx, Move, Player, RunHint, Snapshot, UNIT, clip},
    sound_table::{AnimCursor, Cue, FxContext, Table, cap_action_name, fx_cues, is_voice},
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

/// One clip contributing to the pose.
#[derive(Clone)]
struct Layer {
    name: &'static str,
    frame: f32,
    rate: f32,
    weight: f32,
}
/// Frames the main clip blends in over when a sub animation ends and restarts it.
const RESTART_BLEND_FRAMES: u32 = 5;
/// What the animation blend holds for one frame.
#[derive(Clone, Default)]
struct Pose {
    layers: Vec<Layer>,
    /// The slot the clips belong to: the clip itself, or Move for the cycle.
    slot: &'static str,
}
impl Posed for Pose {
    fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
}
/// The clip frame a trace row shows is the frame the pose displays in the
/// game's sample (after its animation update of the logic frame). The game
/// advances a clip at the start of its logic frame, then runs the state, so a
/// clip a state starts reads frame 0 in that frame and the next frame advances
/// it; the controller does the same, so its clip frame is the shown frame.
/// Animation-frame triggers fire one advance later (see `record`).
fn main_frame(a: &Anim) -> f32 {
    a.frame
}
impl Pose {
    /// The pose the controller asks for. The "Move" cycle blends Walk, Run,
    /// Dash and DashFast by speed, with the weights the renderer uses.
    fn of(p: &Player) -> Self {
        let a = p.anim;
        if a.name != "Move" {
            return Self {
                slot: a.name,
                layers: vec![Layer {
                    name: a.name,
                    // The braking, turning and held clips keep their last frame
                    // while the animator's counter runs one past it.
                    frame: match a.name {
                        "Brake" => a.frame.min(clip::BRAKE as f32),
                        "Turn" => a.frame.min(clip::TURN as f32),
                        "RollingEnd" => a.frame.min(clip::ROLLING_END as f32),
                        // Under a sub animation the model's own frame is shown.
                        name => p.model_frame().unwrap_or_else(|| {
                            clip::held_length(name)
                                .map_or(a.frame, |length| a.frame.min(length as f32))
                        }),
                    },
                    rate: a.rate,
                    weight: 1.,
                }],
            };
        }
        // Each clip of the slot plays the shared phase at its own length.
        let layers = p
            .run_cycle()
            .layers()
            .map(|l| Layer {
                name: l.name,
                frame: l.frame,
                rate: l.rate,
                weight: l.weight,
            })
            .collect();
        Self {
            layers,
            slot: "Move",
        }
    }
    /// The name the game gives this pose in the interp column.
    fn dominant(&self) -> &'static str {
        self.slot
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
    /// Where the cap's clip stands, for the sounds its frames start.
    cap_cursor: AnimCursor,
    boosts: u32,
    blend: Blend<Pose>,
    overlay: f32,
    /// Name of the latest sub clip (the probe keeps it after the clip ends).
    last_sub: &'static str,
    /// The state label and the state's step on the previous row, and the
    /// sub-nerve's steps there. The game's sub-nerve restarts whenever the
    /// state label changes (a sub-state of the running actor nerve) or the
    /// actor nerve starts over; at the start of a replay it counts with the
    /// state's own step.
    last_state: Option<(&'static str, u32)>,
    sub_steps: u32,
    /// The main clip of the previous row and the one before it, and the model's
    /// sub animation then: when a sub animation of another clip ends, the
    /// animator restarts the main clip with a blend from the clip before it
    /// (`PlayerAnimator::endSubAnim`), which `restart` counts through.
    main_clip: &'static str,
    stale_clip: &'static str,
    sub_clip: Option<&'static str>,
    restart: Option<(&'static str, u32)>,
    /// Burst effects still alive, with frames left.
    bursts: Vec<(&'static str, u32)>,
    /// The original's effects, when their library is installed; the
    /// procedural stand-ins are logged otherwise.
    effects: Option<Tracker>,
    /// The cap was thrown from the right hand (its clip names carry the side).
    right_hand: bool,
}
impl Recorder {
    /// `floor` is the material of the floor the run stands on (the recorded
    /// game's), which picks the variant of each effect.
    pub fn new(table: Option<Table>, library: Option<Library>, floor: Option<String>) -> Self {
        Self {
            table,
            right_hand: true,
            effects: library.map(|library| Tracker::new(library).on_floor(floor)),
            cursor: AnimCursor::default(),
            cap_cursor: AnimCursor::default(),
            boosts: 0,
            blend: Blend::default(),
            last_sub: "",
            last_state: None,
            sub_steps: 0,
            main_clip: "",
            stale_clip: "",
            sub_clip: None,
            restart: None,
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
        // The game marks a sound of Cappy's own audio keeper with `@cap`.
        events.push(format!(
            "{}:{name}{}",
            if is_voice(name) { "voice" } else { "se" },
            if cap { "@cap" } else { "" }
        ));
        if self.table.is_none() {
            return;
        }
        // The game logs one request per sound, however many layers answer it;
        // the mix shows the first layer, or neutral levels for a play that has
        // none (a material variant waits for the floor to name its sound).
        let (gain, pitch) = self
            .table
            .as_ref()
            .and_then(|table| {
                let layer = table.group(cap).plays.get(name)?.layers.first()?;
                let resource = table.resources.get(&layer.resource)?;
                // Random variation is left at its middle (volume at its loudest).
                let mut draws = [0.5, 0.5, 0.].into_iter();
                Some(layer.levels(resource, speed, || draws.next().unwrap_or(0.)))
            })
            .unwrap_or((1., 1.));
        mix.push(format!("{name}/{}/{}", num(gain), num(pitch)));
    }
    fn reaction(
        &self,
        cap: bool,
        name: &str,
        speed: f32,
        events: &mut Vec<String>,
        mix: &mut Vec<String>,
    ) {
        // The game marks a reaction raised on Cappy with `@cap` as well.
        events.push(format!("reaction:{name}{}", if cap { "@cap" } else { "" }));
        if let Some(table) = &self.table {
            for play in table.group(cap).reaction_plays(name) {
                self.play(cap, &play, speed, events, mix);
            }
        }
    }
    fn action(
        &self,
        (cap, name, from, to, replay): (bool, &str, f32, f32, bool),
        speed: f32,
        events: &mut Vec<String>,
        mix: &mut Vec<String>,
    ) {
        if let Some(table) = &self.table {
            for play in table.group(cap).triggered(name, from, to, replay) {
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

        // The pose this row shows, and the frame animation events fire on. The
        // game logs a sound or effect that belongs to clip frame t in the row
        // whose pose still shows t - 1 (SpinCapStart's frame 8 and 11 sounds
        // were logged at shown frames 7 and 10; frame-0 sounds in the first
        // row), which is the frame the controller holds after its step.
        let shown = if p.anim.name == "Move" {
            p.shown_frame()
        } else {
            main_frame(&p.anim)
        };
        let advance = self.cursor.advance(
            p.anim.name,
            p.anim.serial,
            p.trigger_frame(),
            looping(p.anim.name),
        );
        if let Some((from, to, with_cap)) = advance.scan {
            self.action(
                (false, p.anim.name, from, to, advance.replay),
                speed,
                &mut events,
                &mut mix,
            );
            if with_cap {
                self.action(
                    (true, p.anim.name, from, to, false),
                    speed,
                    &mut events,
                    &mut mix,
                );
            }
        }
        // The cap's own clip: what it starts when it begins (its flying loop).
        match &p.cap {
            Some(cap) => {
                let advance = self.cap_cursor.advance(
                    cap_action_name(cap.clip()),
                    cap.clip_serial(),
                    cap.clip_frame(),
                    false,
                );
                if let Some((from, to, _)) = advance.scan {
                    let name = cap_action_name(cap.clip());
                    self.action((true, name, from, to, false), speed, &mut events, &mut mix);
                }
            }
            None => self.cap_cursor = AnimCursor::default(),
        }
        // Movement events: sounds, then effects.
        let context = FxContext { mode: p.mode };
        let mut started = Vec::new();
        for f in &fxs {
            if matches!(f, Fx::Throw(_)) {
                self.right_hand = !p.anim.name.contains("Left");
            }
            for cue in fx_cues(*f, context, &mut self.boosts) {
                match cue {
                    Cue::Play(name) | Cue::Hold(name) => {
                        self.play(false, name, speed, &mut events, &mut mix)
                    }
                    Cue::CapPlay(name) => self.play(true, name, speed, &mut events, &mut mix),
                    Cue::Rated(name) => self.play(false, name, p.anim.rate, &mut events, &mut mix),
                    Cue::Reaction { cap, name } => {
                        self.reaction(cap, name, speed, &mut events, &mut mix)
                    }
                    Cue::Action {
                        cap,
                        name,
                        from,
                        to,
                        ..
                    } => self.action((cap, name, from, to, false), speed, &mut events, &mut mix),
                }
            }
            if self.effects.is_none() {
                started.extend(fx::burst_names(*f, p.flat_speed()));
            }
        }
        // Effects: the original's by their table names, or the stand-ins'.
        let live: BTreeSet<String> = if let Some(tracker) = &mut self.effects {
            let (names, alive) = tracker.step(p, &fxs, p.trigger_frame());
            events.extend(names.into_iter().map(|n| format!("effect:{n}")));
            alive.into_iter().collect()
        } else {
            for name in &started {
                events.push(format!("effect:{name}"));
            }
            // Bursts live for their lifetime, plus the state's trails.
            self.bursts.retain_mut(|(_, left)| {
                *left = left.saturating_sub(1);
                *left > 0
            });
            self.bursts
                .extend(started.iter().map(|n| (*n, fx::burst_frames(n))));
            self.bursts
                .iter()
                .map(|b| b.0)
                .chain(fx::trail_names(p))
                .map(String::from)
                .collect()
        };

        // Animation blend and crossfade, advanced by one frame.
        let dt = 1. / FPS;
        // The dive's landing clip covers the whole body while it plays: the
        // pose under it neither blends nor shows, and no upper overlay is meant.
        let covering = p.sub_anim.filter(|s| s.name == "RollingStandUp");
        if covering.is_none() {
            self.blend
                .update(p.anim.name, p.anim.serial, Pose::of(p), dt);
        }
        let pose = &self.blend.current;
        let kept = self.blend.kept();
        if let Some(sub) = p.sub_anim {
            self.last_sub = sub.name;
        } else if let Some((name, _)) = p.model_sub() {
            self.last_sub = name;
        }
        self.overlay = if p.sub_anim.is_some() && covering.is_none() {
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
        // The game lists the clips of the playing slot with their own weights;
        // the fade from the clip before it is the `interp` column.
        if let Some(sub) = covering {
            layers.push(layer_cell(sub.name, sub.frame, sub.rate, 1.));
        } else if let Some((name, frame)) = p.model_sub().filter(|(name, _)| *name != p.anim.name) {
            // A sub animation of another clip plays instead of the main one.
            layers.push(layer_cell(name, frame, 1., 1.));
        } else {
            for l in &pose.layers {
                layers.push(layer_cell(l.name, l.frame, l.rate, l.weight));
            }
        }
        if self.overlay > 0. {
            let frame = p.sub_anim.map_or(15., |s| sub_frame(&s)).min(15.9);
            layers.push(layer_cell(upper, frame, 1., self.overlay));
        }
        let blend = if pose.layers.len() > 1 && covering.is_none() {
            pose.layers
                .iter()
                .map(|l| format!("{}:{}", l.name, num(l.weight)))
                .collect::<Vec<_>>()
                .join("|")
        } else {
            String::new()
        };
        let mut interp = self
            .blend
            .old
            .last()
            .filter(|_| covering.is_none())
            .map(|(old, _, _)| {
                // A clip restarted over itself fades from the clip it first
                // followed: the interpolation source keeps its name.
                let from = match old.dominant() {
                    name if name == p.anim.name && !self.stale_clip.is_empty() => self.stale_clip,
                    name => name,
                };
                format!("{from}:{}", num(1. - kept))
            })
            .unwrap_or_default();
        // A sub animation of another clip just ended: the main clip restarts
        // blending in over `RESTART_BLEND_FRAMES` from the clip it followed.
        if self.main_clip != p.anim.name {
            self.stale_clip = self.main_clip;
            self.main_clip = p.anim.name;
        }
        let sub_now = p.model_sub().map(|(name, _)| name);
        // The main slot's own blend is not reported while a sub animation of
        // another clip plays over it.
        if sub_now.is_some_and(|name| name != p.anim.name) {
            interp.clear();
        }
        if let Some(ended) = self.sub_clip
            && sub_now.is_none()
            && ended != p.anim.name
            && !self.stale_clip.is_empty()
        {
            self.restart = Some((self.stale_clip, 0));
        }
        self.sub_clip = sub_now;
        if let Some((from, done)) = self.restart {
            let done = done + 1;
            self.restart = (done < RESTART_BLEND_FRAMES).then_some((from, done));
            if interp.is_empty() && done < RESTART_BLEND_FRAMES {
                interp = format!("{from}:{}", num(done as f32 / RESTART_BLEND_FRAMES as f32));
            }
        }
        // Clips that need the model's data (eye and hand parts) are not known here.
        let parts = p
            .sub_anim
            .filter(|_| covering.is_none())
            .map(|s| format!("upper:{upper}@{}", num(sub_frame(&s))))
            .unwrap_or_default();

        let v = p.units_per_frame();
        // A negative zero would put a half turn on the other side of the circle.
        let yaw = (p.facing.x + 0.).atan2(p.facing.z);
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
        cells.text(p.nerve_name().name);
        // The run cycle is one slot, `Move`, whose clips are the blend layers.
        cells.text(p.anim.name);
        cells.num(if p.counter_restarted { 0. } else { shown });
        cells.num(p.anim.rate);
        cells.text(&blend);
        cells.text(&interp);
        // The probe reads the animator's sub slot, which keeps the name of the
        // last sub clip after it ends and then shows the main clip's frame.
        // Sub clips are the rewrite's own overlays (`sub_anim`) or the game's
        // sub animation on the model that the state started (`model_sub`).
        cells.text(self.last_sub);
        cells.text(&match (p.sub_anim, p.model_sub()) {
            (Some(s), _) => num(sub_frame(&s)),
            (None, Some((_, frame))) => num(frame),
            (None, None) if self.last_sub.is_empty() => String::new(),
            (None, None) if p.anim.name == "Move" => num(p.run_cycle().phase),
            (None, None) => num(pose.layers.first().map_or(p.anim.frame, |l| l.frame)),
        });
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
        // Only the run cycle's length is known (its 60-frame phase).
        if p.anim.name == "Move" {
            cells.num(60.);
        } else {
            cells.empty();
        }
        cells.int(looping(p.anim.name) as i64 | (p.anim.name == "Move") as i64);
        cells.text(&parts);
        cells.empty(); // bones need the skeleton
        let cap = p.cap_row();
        cells.text(cap.as_ref().map_or("", |c| c.state));
        for axis in 0..3 {
            cells.text(
                &cap.as_ref()
                    .map_or(String::new(), |c| num(c.pos[axis] / UNIT)),
            );
        }
        cells.text(cap.as_ref().map_or("", |c| c.clip));
        for _ in 0..4 {
            cells.empty(); // camera position and pitch are the renderer's
        }
        cells.text(&live.into_iter().collect::<Vec<_>>().join("|"));
        cells.text(&mix.join("|"));
        cells.text(&p.floor_normal().map_or(String::new(), |n| {
            format!("{}/{}/{}", num(n.x), num(n.y), num(n.z))
        }));
        cells.int(p.state_frames() as i64);
        // The sub-nerve's own step counter (see `last_state`).
        let label = p.nerve_name().name;
        self.sub_steps = match self.last_state {
            None => p.state_frames(),
            Some((last, steps)) if last == label && steps < p.state_frames() => self.sub_steps + 1,
            Some(_) => 0,
        };
        self.last_state = Some((label, p.state_frames()));
        cells.int(self.sub_steps as i64);
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
        cells.text(&counters_cell(p));
        let names: Vec<String> = fxs.iter().map(|f| fx_name(*f)).collect();
        cells.text(&triggers_cell(&names));
        cells.text(&collision_cell(p));
        for _ in 0..5 {
            cells.empty(); // random, model, face, cam_mode, cam_fov
        }
        cells.finish()
    }
}

/// The frame a partial (upper-body) clip shows; it advances one frame per step
/// like the main clip.
fn sub_frame(sub: &Anim) -> f32 {
    sub.frame
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
    /// The floor material the recorded game stood on (its `collision` column's
    /// `gmat`), taken from the first row that names one.
    pub fn floor_material(&self) -> Option<String> {
        (0..self.rows.len()).find_map(|row| {
            self.cell(row, "collision")?
                .split(';')
                .find_map(|item| item.strip_prefix("gmat="))
                .map(str::to_string)
        })
    }
    /// What a row of the run state shows of its hidden state: the run cycle's
    /// pose and weights, the turn that moved the facing, the stick, how long the
    /// current nerve has been going and the nerve chosen in this row.
    fn run_hint(&self, row: usize) -> Option<RunHint> {
        let anim = self.cell(row, "anim")?;
        if !matches!(anim, "Move" | "Brake" | "Turn" | "TurnPoint") {
            return None;
        }
        let mut hint = RunHint {
            rate: self.number(row, "anim_rate").unwrap_or(1.),
            ..RunHint::default()
        };
        if anim == "Move" {
            let mut phase = None;
            for item in self.cell(row, "anim_layers")?.split('|') {
                // `Name@frame*rate=weight`
                let (name, rest) = item.split_once('@')?;
                let (frame, rest) = rest.split_once('*')?;
                let (_, weight) = rest.split_once('=')?;
                let slot = ["Walk", "Run", "Dash", "DashFast", "RunStart"]
                    .iter()
                    .position(|n| *n == name)?;
                hint.weights[slot] = weight.parse().ok()?;
                let length = [60., 17., 17., 14., 30.][slot];
                phase = phase.or(frame.parse::<f32>().ok().map(|f| f * 60. / length));
            }
            hint.phase = phase?;
        }
        // Rows of the same clip so far: the steps the nerve has taken. A run
        // that follows the turn is the run after turning round.
        let mut first = row;
        while first > 0 && self.cell(first - 1, "anim") == Some(anim) {
            first -= 1;
        }
        hint.steps = (row - first + 1) as u32;
        hint.after_turn =
            anim == "Move" && first > 0 && self.cell(first - 1, "anim") == Some("Turn");
        // The next row shows the clip a nerve chose in this one starting.
        let next = self.cell(row + 1, "anim");
        if next != Some(anim) && self.number(row + 1, "anim_frame") == Some(0.) {
            hint.chosen = match next {
                Some("Brake") => Some("Brake"),
                Some("Turn") => Some("Turn"),
                Some("TurnPoint") => Some("TurnPoint"),
                Some("Move") => Some("Move"),
                _ => None,
            };
        }
        let front = |r: usize| {
            Vec3::new(
                self.number(r, "fx").unwrap_or(0.),
                0.,
                self.number(r, "fz").unwrap_or(0.),
            )
        };
        if row > 0 {
            hint.turn_rate = front(row).angle_between(front(row - 1)).to_degrees();
        }
        // The turn power is a running average of how hard the facing swung.
        let mut power = 0.;
        for r in row.saturating_sub(40).max(1)..=row {
            let swing = front(r).angle_between(front(r - 1)).to_degrees();
            power += (crate::movement::turn_power(swing) - power) * 0.15;
        }
        hint.turn_power = power;
        let input = self.cell(row, "move_input")?;
        let mut parts = input.split('/').filter_map(|v| v.parse::<f32>().ok());
        hint.last_input = Vec3::new(parts.next()?, 0., parts.next()?);
        Some(hint)
    }
    /// The clip the next row starts (it differs from this row's and sits at
    /// its first frame): what a nerve chosen in this row plays.
    fn next_clip(&self, row: usize) -> Option<&'static str> {
        let now = self.cell(row, "anim")?;
        let next = self.cell(row + 1, "anim")?;
        (next != now && self.number(row + 1, "anim_frame") == Some(0.)).then(|| intern(next))
    }
    /// A recorded position of the cap in centimetres, when the row has one.
    fn cap_spot(&self, row: usize) -> Option<Vec3> {
        Some(Vec3::new(
            self.number(row, "cap_px")?,
            self.number(row, "cap_py")?,
            self.number(row, "cap_pz")?,
        ))
    }
    /// The cap in flight, as the recording shows it at a row: its nerve and
    /// steps, where it is and how fast it moves (the move of the frame). The
    /// arrival row, where the cap already sits on the player, is not flight.
    fn cap_moment(&self, row: usize) -> Option<crate::cap::Moment> {
        use crate::cap::{CapNerve, Moment};
        let flying = |row: usize| {
            let nerve = CapNerve::from_name(self.cell(row, "cap_state")?)?;
            let arrived = self
                .cell(row, "cap_anim")
                .is_some_and(|c| c.starts_with("CatchCap"));
            (!arrived).then_some(nerve)
        };
        let nerve = flying(row)?;
        let spot = self.cap_spot(row)?;
        let step = self.number(row, "cap_step").map_or(0, |s| s.max(0.) as u32);
        // The first row of a cap has no row before it: it left the throw point
        // (150 in front of the player, 70 up) at its flight speed.
        let velocity = |row: usize| -> Option<Vec3> {
            let spot = self.cap_spot(row)?;
            if row > 0 && flying(row - 1).is_some() {
                return Some(spot - self.cap_spot(row - 1)?);
            }
            let player = Vec3::new(
                self.number(row, "px")?,
                self.number(row, "py")?,
                self.number(row, "pz")?,
            );
            let front = Vec3::new(self.number(row, "fx")?, 0., self.number(row, "fz")?);
            Some(spot - (player + Vec3::Y * 70. + front * 150.))
        };
        let moving = velocity(row)?;
        // The flight speed is the one the throw cruised at.
        let mut speed = 0f32;
        let mut at = row;
        loop {
            if matches!(flying(at), Some(CapNerve::Throw | CapNerve::ThrowBrake))
                && let Some(v) = velocity(at)
            {
                speed = speed.max(v.length());
            }
            if at == 0 || flying(at - 1).is_none() {
                break;
            }
            at -= 1;
        }
        Some(Moment {
            nerve,
            step,
            spot,
            velocity: moving,
            speed: if speed > 0. { speed } else { 32. },
            return_speed: if nerve == CapNerve::Return {
                moving.length()
            } else {
                0.
            },
        })
    }
    /// The spin throw state at a row: whether the cap is still the spin attack
    /// and for how long, whether the throw frame just passed, and what kind of
    /// throw it is (the clip names it).
    fn throw_hint(&self, row: usize) -> Option<crate::movement::ThrowHint> {
        use crate::movement::{Throw, ThrowHint};
        if !self.cell(row, "state")?.starts_with("SpinCap/") {
            return None;
        }
        let spinning = |row: usize| self.cell(row, "cap_state") == Some("SpinAttack");
        let (kind, count, continuous) = match self.cell(row, "anim")? {
            "SpinCapStartDoubleUp" | "SpinCapAirStartDoubleUp" => (Throw::Up, 1, false),
            "SpinCapStartDoubleDown" | "SpinCapAirStartDoubleDown" => (Throw::Down, 1, false),
            name if name.contains("Double") => (Throw::Spin, 1, false),
            name if name.contains("Start2") => (Throw::Forward, 2, false),
            name if name.contains("Start3") => (Throw::Forward, 0, true),
            _ => (Throw::Forward, 1, false),
        };
        let mut age = 0;
        while age <= row && spinning(row - age) {
            age += 1;
        }
        let state_runs = self.cell(row, "cap_state").is_none();
        Some(ThrowHint {
            winding: age > 0 || state_runs,
            age: age as u32,
            launch: age > 0 && self.cell(row + 1, "cap_state") == Some("Throw"),
            kind,
            count,
            continuous,
        })
    }
    /// Frames since the cap arrived at the player, -1 on the arrival itself.
    fn catching(&self, row: usize) -> Option<i32> {
        let arrival = |row: usize| {
            self.cell(row, "cap_state") == Some("Return")
                && self
                    .cell(row, "cap_anim")
                    .is_some_and(|c| c.starts_with("CatchCap"))
        };
        if arrival(row) {
            return Some(-1);
        }
        let mut frames = -1;
        let mut at = row;
        while self.cell(at, "cap_state") == Some("Catch") {
            frames += 1;
            at = at.checked_sub(1)?;
        }
        (frames >= 0).then_some(frames)
    }
    /// The model's sub animation at a row: the last start or end the probe
    /// logged decides whether one plays.
    fn sub_hint(&self, row: usize) -> Option<(&'static str, f32, f32)> {
        let name = (0..=row).rev().find_map(|at| {
            let events = self.cell(at, "events")?;
            let marked = events
                .split(';')
                .rev()
                .find(|e| e.starts_with("subanim:") || e.starts_with("endsub:"))?;
            Some(marked.strip_prefix("subanim:").map(intern))
        })??;
        let length = if name.starts_with("SpinCap") {
            clip::throw(name) as f32
        } else {
            10.
        };
        Some((name, self.number(row, "sub_frame")?, length))
    }
    /// The injectable player state a row records, for lockstep stepping.
    pub fn snapshot(&self, row: usize) -> Snapshot {
        let n = |c| self.number(row, c).unwrap_or(0.);
        let anim_name = self.cell(row, "anim").unwrap_or("Wait");
        let speed = self
            .number(row, "speed_h")
            .unwrap_or_else(|| (n("vx") * n("vx") + n("vz") * n("vz")).sqrt());
        let spin_fall = self.cell(row, "state") == Some("SpinCap/Fall");
        // The spin cap state's fall is the rewrite's plain fall, named for the state.
        let mode = if spin_fall {
            Some(Move::Fall)
        } else {
            self.cell(row, "state")
                .and_then(|s| state_from_nerve(s, anim_name, speed))
        };
        let shown = n("anim_frame");
        let moving_cycle = ["Move", "Walk", "Run", "Dash", "DashFast"].contains(&anim_name)
            && matches!(mode, Some(Move::Walk | Move::Run));
        let anim = if anim_name == "Move" && moving_cycle {
            // The game's `Move` slot is the 60-frame cycle phase itself.
            Some(("Move", shown.rem_euclid(60.)))
        } else if moving_cycle {
            // A clip of the cycle: the phase runs over 60 frames whatever the length.
            self.number(row, "anim_len")
                .filter(|len| *len > 0.)
                .map(|len| ("Move", (shown / len * 60.).rem_euclid(60.)))
        } else {
            Some((intern(anim_name), shown))
        };
        // The game's counters under their names, or the rewrite's own.
        let mut counters = counters_from_game(self.cell(row, "counters").unwrap_or(""));
        for item in self.cell(row, "pre_input").unwrap_or("").split('|') {
            if let Some((name, value)) = item.split_once('=')
                && let Ok(value) = value.parse::<f64>()
            {
                counters.push((format!("pre_{name}"), value as i64));
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
            sub_steps: self.number(row, "sub_step").map(|s| s.max(0.) as u32),
            counters,
            run: self.run_hint(row),
            next_clip: self.next_clip(row),
            cap: self.cap_moment(row),
            throw: self.throw_hint(row),
            catching: self.catching(row),
            sub: self.sub_hint(row),
            roll_brake: self.cell(row, "state") == Some("Rolling/Brake"),
            spin_fall,
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
