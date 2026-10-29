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
    local_assets::{Blend, Posed, interp_frames, looping},
    movement::{
        Anim, FPS, Fx, Move, Player, RunHint, Snapshot, UNIT, clip,
        look_at::{LookHint, LookSensor},
        look_joint, plane,
    },
    skeleton_pose::{Joint, Matrices, PoseLayer, Region, SkeletonPose},
    sound_table::{AnimCursor, Cue, FxContext, Table, cap_action_name, fx_cues, is_voice},
};
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Column names, in file order.
pub const COLUMNS: [&str; 66] = [
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
    // The head: where the animation puts it (cm, replay frame) and how far the look-at has
    // turned it (degrees), neither a probe column; the world rotation the look-at leaves it
    // with; and the look-at's request, keeper and sensors.
    "head_pos",
    "head_turn",
    "head_q",
    "look_target",
    "look_keeper",
    "look_sensors",
];

/// One clip contributing to the pose.
#[derive(Clone)]
struct Layer {
    name: &'static str,
    frame: f32,
    rate: f32,
    weight: f32,
    /// A partial animation replaces this clip on the upper body (`Pose::partial`).
    covered: bool,
    /// Layers of one stage play together; a pose that fades into another is a stage of its own
    /// (a frozen pose made of several fades keeps one stage for each).
    stage: usize,
}
/// Frames the main clip blends in over when a sub animation ends and restarts it.
const RESTART_BLEND_FRAMES: u32 = 5;
/// What the animation blend holds for one frame.
#[derive(Clone, Default)]
struct Pose {
    layers: Vec<Layer>,
    /// The partial animation (`partial0`) playing over the clips, with the share of the pose it
    /// speaks for: it replaces the clips on the upper body instead of joining their blend.
    partial: Vec<Layer>,
    /// The slot the clips belong to: the clip itself, or Move for the cycle.
    slot: &'static str,
}
impl Pose {
    /// How many stages the pose's layers are spread over.
    fn stages(&self) -> usize {
        self.layers
            .iter()
            .chain(&self.partial)
            .map(|l| l.stage + 1)
            .max()
            .unwrap_or(0)
    }
}
impl Posed for Pose {
    fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
    fn scale(&mut self, share: f32) {
        for layer in self.layers.iter_mut().chain(&mut self.partial) {
            layer.weight *= share;
        }
    }
    fn merge(&mut self, other: Self) {
        // The merged pose was a fade of its own: its stages stay apart from ours.
        let shift = self.stages();
        let shifted = |l: Layer| Layer {
            stage: l.stage + shift,
            ..l
        };
        self.layers.extend(other.layers.into_iter().map(shifted));
        self.partial.extend(other.partial.into_iter().map(shifted));
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
        let mut pose = Self::main_clips(p);
        if let Some((name, frame)) = p.partial_anim() {
            pose.play_partial(name, frame);
        }
        pose
    }
    /// The partial animation takes over the upper body of the clips playing now.
    fn play_partial(&mut self, name: &'static str, frame: f32) {
        let weight = self.layers.iter().map(|l| l.weight).sum();
        for layer in &mut self.layers {
            layer.covered = true;
        }
        self.partial.push(Layer {
            name,
            frame,
            rate: 1.,
            weight,
            covered: false,
            stage: 0,
        });
    }
    /// The clips of the main slot.
    fn main_clips(p: &Player) -> Self {
        let a = p.anim;
        if let Some(weight) = p.sand_blend() {
            let frame = (a.frame + p.anim_rate_change()) % 80.;
            return Self {
                slot: a.name,
                partial: Vec::new(),
                layers: [
                    ("SandWaitNormal", 80., 1. - weight),
                    ("SandWaitStrong", 30., weight),
                ]
                .into_iter()
                .map(|(name, length, weight)| Layer {
                    name,
                    frame: frame * (length / 80.),
                    rate: a.rate * (length / 80.),
                    weight,
                    covered: false,
                    stage: 0,
                })
                .filter(|l| l.weight > 0.)
                .collect(),
            };
        }
        if a.name != "Move" {
            return Self {
                slot: a.name,
                partial: Vec::new(),
                layers: vec![Layer {
                    name: crate::local_assets::skeleton_clip(a.name),
                    // The braking, turning and held clips keep their last frame
                    // while the animator's counter runs one past it.
                    frame: match a.name {
                        "Brake" => a.frame.min(clip::BRAKE as f32),
                        "Turn" => a.frame.min(clip::TURN as f32),
                        "RollingEnd" => a.frame.min(clip::ROLLING_END as f32),
                        // Under a sub animation the model's own frame is shown.
                        name => p.model_frame().unwrap_or_else(|| {
                            // (The model is one rate change ahead of the counter.)
                            let frame = a.frame + p.anim_rate_change();
                            clip::held_length(name).map_or(frame, |length| frame.min(length as f32))
                                - clip::loop_length(name)
                                    .filter(|length| frame >= *length)
                                    .unwrap_or(0.)
                        }),
                    },
                    rate: a.rate,
                    weight: 1.,
                    covered: false,
                    stage: 0,
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
                covered: false,
                stage: 0,
            })
            .collect();
        Self {
            layers,
            partial: Vec::new(),
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
    /// Where the cap's spin attack clip stands while the cap is not out: it plays the clip the
    /// player plays, at the player's shown frame.
    attack_cursor: AnimCursor,
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
    /// The previous row showed a nerve the actor had chosen after the state's update.
    chosen_late: bool,
    /// The main clip of the previous row and the one before it, and the model's
    /// sub animation then: when a sub animation of another clip ends, the
    /// animator restarts the main clip with a blend from the clip before it
    /// (`PlayerAnimator::endSubAnim`), which `restart` counts through.
    main_clip: &'static str,
    stale_clip: &'static str,
    sub_clip: Option<&'static str>,
    /// The model's sub animation ended while the catch's upper-body clip goes on (a forced run
    /// started from a catch): the main clip's own blend is reported again.
    upper_outlives_sub: bool,
    restart: Option<(&'static str, u32, u32)>,
    /// The sub animation on screen at the previous row (the clip, its shown frame, and whether
    /// it covered the whole body): when it ends, the main clip fades in from that pose.
    shown_sub: Option<(Layer, bool)>,
    /// The blend of clips on screen at the previous row (outgoing poses at their shares
    /// included), when no sub animation covered it: when a partial animation ends, the main
    /// clip fades in from that pose (`clearUpperBodyAnim` interpolates from the shown pose).
    shown_blend: Option<Pose>,
    /// Burst effects still alive, with frames left.
    bursts: Vec<(&'static str, u32)>,
    /// The original's effects, when their library is installed; the
    /// procedural stand-ins are logged otherwise.
    effects: Option<Tracker>,
    /// The cap was thrown from the right hand (its clip names carry the side).
    right_hand: bool,
    /// The stage the replay runs in: its sensors and its frame (the probe logs stage
    /// coordinates).
    stage: Option<std::sync::Arc<crate::geometry::TriangleMesh>>,
    /// Legacy captures lack the first row's interpolation label. Explicit preroll
    /// scripts carry the sampled animation history across the row-zero boundary.
    first_row: bool,
    /// The velocity, front, up and quaternion of every recorded frame as f32 bit patterns
    /// (the format of the probe's `bits_*` columns), settle frames included: one line per
    /// frame, `frame,vel,front,up,quat,position` (the position as start-relative centimetres). A debugging aid (`--bits`), not a compared column.
    pub bits: Vec<String>,
}
impl Recorder {
    /// `floor` is the material of the floor the run stands on (the recorded
    /// game's), which picks the variant of each effect.
    pub fn new(table: Option<Table>, library: Option<Library>, floor: Option<String>) -> Self {
        Self {
            table,
            right_hand: true,
            stage: None,
            effects: library.map(|library| Tracker::new(library).on_floor(floor)),
            cursor: AnimCursor::default(),
            cap_cursor: AnimCursor::default(),
            attack_cursor: AnimCursor::default(),
            blend: Blend::default(),
            last_sub: "",
            last_state: None,
            sub_steps: 0,
            chosen_late: false,
            main_clip: "",
            stale_clip: "",
            sub_clip: None,
            upper_outlives_sub: false,
            restart: None,
            shown_sub: None,
            shown_blend: None,
            first_row: false,
            bits: Vec::new(),
            overlay: 0.,
            bursts: Vec::new(),
        }
    }

    /// The new actor starts Wait twice. Its interpolation is between two identical new Wait
    /// poses, as long as the Wait entry of the interpolation table; the capture converter names
    /// it after the last sampled clip (before loading). Keep that diagnostic label without
    /// blending the dead actor.
    pub fn scene_stale_clip(&self) -> &'static str {
        self.stale_clip
    }

    pub fn scene_started(&mut self, previous_stale: &'static str, previous_clip: &'static str) {
        self.main_clip = "Wait";
        self.stale_clip = previous_stale;
        self.restart = Some((
            previous_clip,
            0,
            interp_frames("Wait", previous_clip) as u32,
        ));
    }

    /// The frames recorded so far only warmed the animation state up: the next row is the first
    /// one written.
    pub fn start_rows(&mut self, carry_interpolation: bool) {
        self.first_row = !carry_interpolation;
    }

    /// The stage the replay runs in, for the head look-at's sensors and coordinates.
    pub fn with_stage(
        mut self,
        stage: Option<std::sync::Arc<crate::geometry::TriangleMesh>>,
    ) -> Self {
        self.stage = stage;
        self
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
    /// The sounds a clip schedules between two frames. `previous` is the clip
    /// it replaced when it has just started: a level sound both clips list is
    /// already sounding and is not requested again.
    fn action(
        &self,
        (cap, name, from, to, replay): (bool, &str, f32, f32, bool),
        previous: &str,
        speed: f32,
        events: &mut Vec<String>,
        mix: &mut Vec<String>,
    ) {
        if let Some(table) = &self.table {
            let group = table.group(cap);
            let carried = group.continuing(previous, name);
            for play in group.triggered(name, from, to, replay) {
                if from < 0. && carried.contains(&play) {
                    continue;
                }
                self.play(cap, &play, speed, events, mix);
            }
        }
    }

    /// The clips and shares the skeleton's pose is made of (the renderer's weights: outgoing
    /// poses keep their share, the current one the rest).
    ///
    /// `ahead` is the share of a step the frame being drawn is past the last step (the windowed
    /// game draws between steps); the traced rows are at 0.
    fn pose_layers(
        &self,
        p: &Player,
        covering: Option<Anim>,
        kept: f32,
        ahead: f32,
    ) -> Vec<PoseLayer<'static>> {
        let mut playing: Vec<PoseLayer> = Vec::new();
        if let Some(sub) = covering {
            playing.push(PoseLayer {
                clip: sub.name,
                frame: sub.frame + sub.rate * ahead,
                weight: 1.,
                region: Region::Whole,
                group: 0,
            });
        } else if let Some((name, frame)) = p.model_sub().filter(|(name, _)| *name != p.anim.name) {
            playing.push(PoseLayer {
                clip: name,
                frame: frame + ahead,
                weight: 1.,
                region: Region::Whole,
                group: 0,
            });
        } else {
            // Each stage of each pose is a group: the fade interpolates between them, oldest
            // first. A frozen pose lists its stages newest first (what was playing when it was
            // frozen, then the fades it had absorbed).
            let mut groups = 0;
            for (old, share, _) in &self.blend.old {
                Self::push_pose(&mut playing, old, (*share, ahead), groups, true);
                groups += old.stages();
            }
            Self::push_pose(
                &mut playing,
                &self.blend.current,
                (1. - kept, ahead),
                groups,
                false,
            );
        }
        playing
    }

    /// Add a pose's clips at `share` of the blend: the clips under a partial animation speak
    /// for the lower body only, the partial for the upper body.
    fn push_pose(
        playing: &mut Vec<PoseLayer<'static>>,
        pose: &Pose,
        (share, ahead): (f32, f32),
        first_group: usize,
        newest_stage_first: bool,
    ) {
        let group = |l: &Layer| {
            first_group
                + if newest_stage_first {
                    pose.stages() - 1 - l.stage
                } else {
                    l.stage
                }
        };
        for l in &pose.layers {
            playing.push(PoseLayer {
                clip: l.name,
                frame: l.frame + l.rate * ahead,
                weight: l.weight * share,
                region: if l.covered {
                    Region::Lower
                } else {
                    Region::Whole
                },
                group: group(l),
            });
        }
        for l in &pose.partial {
            playing.push(PoseLayer {
                clip: l.name,
                frame: l.frame + l.rate * ahead,
                weight: l.weight * share,
                region: Region::Upper,
                group: group(l),
            });
        }
    }

    /// The skeleton as the model's calculation leaves it this row, controllers included; `None`
    /// without the installed model.
    fn posed_skeleton(
        &self,
        p: &mut Player,
        covering: Option<Anim>,
        kept: f32,
    ) -> Option<Matrices> {
        let playing = self.pose_layers(p, covering, kept, 0.);
        let locals = SkeletonPose::installed()?.locals(&playing)?;
        Some(p.pose_skeleton(&locals))
    }

    /// The `bones` cell: every joint's position (cm, replay frame) as the probe prints them,
    /// and the actor itself.
    fn bones_cell(skeleton: Option<&Matrices>, p: &Player) -> String {
        let Some(skeleton) = skeleton else {
            return String::new();
        };
        let at = |x: f32, y: f32, z: f32| format!("{}/{}/{}", num(x), num(y), num(z));
        let mut items: Vec<String> = Joint::ALL
            .iter()
            .map(|joint| {
                let m = &skeleton[*joint as usize];
                format!("{}:{}", joint.name(), at(m[0][3], m[1][3], m[2][3]))
            })
            .collect();
        let feet = p.pos / UNIT;
        items.push(format!("Mario:{}", at(feet.x, feet.y, feet.z)));
        items.join("|")
    }

    /// The head's cells: the animated head joint with the look-at turned on it, and what the
    /// look-at asked for. Without the model (no installed game data) the head columns stay empty
    /// but the keeper's and sensors' do not.
    fn head_cells(&mut self, p: &mut Player, cells: &mut Cells, skeleton: Option<&Matrices>) {
        match skeleton.map(|joints| joints[Joint::Head as usize]) {
            Some(matrix) => {
                let at = look_joint::translation(&matrix);
                cells.text(&format!("{}/{}/{}", num(at.x), num(at.y), num(at.z)));
                cells.num(p.look.joint.turn_degrees());
                let q = look_joint::quat_of_rotation(&matrix).normalize();
                let q = if q.w < 0. { -q } else { q };
                cells.text(&format!(
                    "{}/{}/{}/{}",
                    num(q.x),
                    num(q.y),
                    num(q.z),
                    num(q.w)
                ));
            }
            None => {
                cells.empty();
                cells.empty();
                cells.empty();
            }
        }
        let static_sensors: &[LookSensor] =
            self.stage.as_ref().map_or(&[], |mesh| &mesh.look_sensors);
        let sensors = p.rocket_look_sensors(static_sensors, self.stage.as_deref());
        cells.text(
            &p.look
                .requested_sensor()
                .and_then(|i| sensors.get(i))
                .map_or(String::new(), |s| {
                    format!(
                        "{}/{}/{}",
                        num(s.stage_position.x),
                        num(s.stage_position.y),
                        num(s.stage_position.z)
                    )
                }),
        );
        cells.text(&p.look.keeper_cell());
        cells.text(&p.look.sensors_cell(&sensors));
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
                advance.previous,
                speed,
                &mut events,
                &mut mix,
            );
            if with_cap {
                // The cap's spin attack plays the same clip on the cap's own model, whose frame
                // is the one the player shows: its sounds come in the row that shows their
                // frame, not one before (the third throw's voice at frame 1).
                let attack = self
                    .attack_cursor
                    .advance(p.anim.name, p.anim.serial, shown, false);
                if let Some((from, to, _)) = attack.scan {
                    self.action(
                        (true, p.anim.name, from, to, false),
                        "",
                        speed,
                        &mut events,
                        &mut mix,
                    );
                }
            }
        }
        if advance.loop_start {
            self.action(
                (false, p.anim.name, -1., 0., true),
                "",
                speed,
                &mut events,
                &mut mix,
            );
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
                    self.action(
                        (true, name, from, to, false),
                        "",
                        speed,
                        &mut events,
                        &mut mix,
                    );
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
            for cue in fx_cues(*f, context) {
                match cue {
                    Cue::Play(name) | Cue::Hold(name) => {
                        self.play(false, name, speed, &mut events, &mut mix)
                    }
                    Cue::CapPlay(name) => self.play(true, name, speed, &mut events, &mut mix),
                    Cue::Rated(name) => self.play(false, name, p.anim.rate, &mut events, &mut mix),
                    Cue::Reaction { cap, name } => {
                        self.reaction(cap, name, speed, &mut events, &mut mix)
                    }
                }
            }
            if self.effects.is_none() {
                started.extend(fx::burst_names(*f, p.flat_speed()));
            }
        }
        // Effects: the original's by their table names, or the stand-ins'.
        let live: BTreeSet<String> = if let Some(tracker) = &mut self.effects {
            let (names, alive) = tracker.step(p, &fxs, p.effect_frame());
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
        let PoseStep {
            covering,
            kept,
            mut interp,
        } = self.advance_pose(p);
        let pose = &self.blend.current;
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
        // Clips that need the model's data (eye and hand parts) are not known here.
        let parts = p
            .sub_anim
            .filter(|_| covering.is_none())
            .map(|s| format!("upper:{upper}@{}", num(sub_frame(&s))))
            .unwrap_or_default();

        let v = p.units_per_frame();
        // The probe reads `al::calcQuat`: `toQuat` over the actor's rotation matrix, whose
        // largest-of-four selection decides the sign (a quarter turn about the vertical axis is
        // a tie of its scalar and vertical-axis branches, so its result follows the rounding of
        // the matrix). An upright pose is the one that faces the way the player does.
        let mut q = plane::ActorPose::new(plane::quat_up_front(Vec3::Y, p.facing)).calc_quat();
        // A pose tilted onto a slope (a run up it) shows its own up and front.
        let (pose_up, pose_front) = match p.pose_read_back() {
            Some(pose) => {
                q = pose;
                p.pose_axes()
            }
            None => (Vec3::Y, p.facing),
        };
        let words = |values: &[f32]| {
            values
                .iter()
                .map(|v| format!("0x{:08x}", v.to_bits()))
                .collect::<Vec<_>>()
                .join("/")
        };
        let cm = p.position_cm();
        self.bits.push(format!(
            "{},{},{},{},{},{}",
            info.frame,
            words(&[v.x, v.y, v.z]),
            words(&[pose_front.x, pose_front.y, pose_front.z]),
            words(&[pose_up.x, pose_up.y, pose_up.z]),
            words(&[q.x, q.y, q.z, q.w]),
            words(&[cm.x, cm.y, cm.z])
        ));
        let mut cells = Cells::default();
        cells.int(info.frame);
        for axis in [p.pos.x, p.pos.y, p.pos.z] {
            cells.num(axis / UNIT);
        }
        for axis in [v.x, v.y, v.z] {
            cells.num(axis);
        }
        cells.num(pose_front.x);
        cells.num(pose_front.z);
        cells.int(p.grounded as i64);
        cells.text(p.nerve_name().name);
        // The run cycle is one slot, `Move`, whose clips are the blend layers.
        cells.text(p.anim.name);
        cells.num(if p.counter_restarted { 0. } else { shown });
        cells.num(p.anim.rate);
        cells.text(&blend);
        if std::mem::take(&mut self.first_row) {
            interp.clear();
        }
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
        for c in [q.x, q.y, q.z, q.w, pose_up.x, pose_up.y, pose_up.z] {
            cells.num(c);
        }
        // The horizontal speed of the velocity the row shows, which on a tilted
        // floor includes the horizontal part of the pull into it.
        cells.num(v.x.hypot(v.z));
        cells.text(&layers.join("|"));
        // Only the run cycle's length is known (its 60-frame phase).
        if p.anim.name == "Move" {
            cells.num(60.);
        } else {
            cells.empty();
        }
        cells.int(looping(p.anim.name) as i64 | (p.anim.name == "Move") as i64);
        cells.text(&parts);
        let skeleton = self.posed_skeleton(p, covering, kept);
        cells.text(&Self::bones_cell(skeleton.as_ref(), p));
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
        cells.int(if p.death.abyss_pending_for_trace() {
            -1
        } else {
            p.state_frames() as i64
        });
        // The sub-nerve's own step counter (see `last_state`).
        let label = p.nerve_name().name;
        self.sub_steps = match self.last_state {
            None => p.state_frames(),
            Some((last, steps))
                if last == label && steps < p.state_frames() && !p.sub_restarted() =>
            {
                self.sub_steps + 1
            }
            // A hit sensor restarted the nerve and its first step ran already.
            Some(_) if p.restarted_by_sensor() => p.state_frames(),
            Some(_) => 0,
        };
        if let Some(steps) = p.fire_sub_steps() {
            self.sub_steps = steps;
        }
        self.last_state = Some((label, p.state_frames()));
        // A nerve the actor chose after the state's update (the swim's cap throw) shows -1 until
        // the state applies it, and its first step then counts 1 (the keeper applied it and
        // counted the step in the same update).
        let chosen_late = p.nerve_chosen_late() && !p.death.abyss_pending_for_trace();
        if p.mode == Move::Dead && p.crushed() {
            cells.empty();
        } else if chosen_late {
            cells.int(-1);
        } else if std::mem::take(&mut self.chosen_late) {
            self.sub_steps = 1;
            cells.int(1);
        } else {
            cells.int(self.sub_steps as i64);
        }
        self.chosen_late = chosen_late;
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
        self.head_cells(p, &mut cells, skeleton.as_ref());
        cells.finish()
    }
}

/// The animation state behind the skeleton the windowed game draws: the same crossfade, sub
/// animations and partial animation the trace records, stepped once per simulation step, so
/// that the pose on screen is the pose of the traced `bones` column.
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
pub struct SkeletonFeed {
    recorder: Recorder,
    step: Option<(Option<Anim>, f32)>,
}
impl Default for SkeletonFeed {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
impl SkeletonFeed {
    pub fn new() -> Self {
        Self {
            recorder: Recorder::new(None, None, None),
            step: None,
        }
    }

    /// The player took a simulation step.
    pub fn advance(&mut self, p: &Player) {
        let PoseStep { covering, kept, .. } = self.recorder.advance_pose(p);
        self.step = Some((covering, kept));
    }

    /// Every joint's matrix relative to its parent, as the animation leaves it `ahead` of the
    /// last step (a share of a step); `None` before the first step or without the installed
    /// model and its curves.
    pub fn locals(&self, p: &Player, ahead: f32) -> Option<Matrices> {
        let (covering, kept) = self.step?;
        let playing = self.recorder.pose_layers(p, covering, kept, ahead);
        SkeletonPose::installed()?.locals(&playing)
    }
}

/// What one step of the animation state hands the rest of the row.
struct PoseStep {
    /// The dive's landing clip, when it covers the whole body.
    covering: Option<Anim>,
    /// Share of the pose still held by outgoing poses.
    kept: f32,
    /// The `interp` column: the fade from the clip before.
    interp: String,
}

impl Recorder {
    /// The animation blend and crossfade, advanced by one frame: the state behind the pose the
    /// row shows. The windowed game steps its own recorder through this too, so that the
    /// skeleton it poses is the traced one.
    fn advance_pose(&mut self, p: &Player) -> PoseStep {
        let dt = 1. / FPS;
        // The dive's landing clip covers the whole body while it plays: the
        // pose under it neither blends nor shows, and no upper overlay is meant.
        let covering = p.sub_anim.filter(|s| s.name == "RollingStandUp");
        // The sub animation on screen in this row (a model sub animation of another clip, or
        // the cover), for the fade that follows its end.
        let shown_now = covering
            .map(|s| (s.name, s.frame, s.rate, true))
            .or_else(|| {
                p.model_sub()
                    .filter(|(name, _)| *name != p.anim.name)
                    .map(|(name, frame)| (name, frame, 1., false))
            })
            .map(|(name, frame, rate, covers)| {
                (
                    Layer {
                        name,
                        frame,
                        rate,
                        weight: 1.,
                        covered: false,
                        stage: 0,
                    },
                    covers,
                )
            });
        let blend_shows = shown_now.is_none();
        let shown_before = std::mem::replace(&mut self.shown_sub, shown_now);
        if covering.is_none() {
            self.blend
                .update(p.anim.name, p.anim.serial, Pose::of(p), dt);
            if p.interpolation_cleared() {
                self.blend.old.clear();
            }
            // The cover just ended: the main clip fades in over the pose the cover left on
            // screen, not over the pose held under it (`PlayerAnimator` restarts the main
            // slot at `endSubAnim`; the recorded `interp` still names the main clip's
            // predecessor). The fade keeps its label (`slot`) and share.
            if let Some((shown, true)) = shown_before.clone() {
                for (old, _, _) in &mut self.blend.old {
                    let held: f32 = old.layers.iter().map(|l| l.weight).sum();
                    old.layers = vec![Layer {
                        weight: held,
                        ..shown.clone()
                    }];
                }
            }
        }
        let mut kept = self.blend.kept();
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
            if let Some((from, 0, _)) = self.restart {
                // A new scene's Wait can immediately select its area animation.
                // The probe keeps the prior interpolation label when the clip
                // name survived the reload, without blending the old skeleton.
                let from = if from == p.anim.name && !self.stale_clip.is_empty() {
                    self.stale_clip
                } else {
                    from
                };
                self.restart = Some((from, 0, interp_frames(p.anim.name, from) as u32));
                self.stale_clip = from;
            } else {
                self.restart = None;
                self.stale_clip = self.main_clip;
            }
            self.main_clip = p.anim.name;
        }
        // The animation that played over the main clip: the model's sub
        // animation, or the upper-body one of a catch on the move.
        let sub_now = p.model_sub().map(|(name, _)| name).or(p.upper_catch());
        if p.ended_model_sub().is_some() && p.upper_catch().is_some() {
            self.upper_outlives_sub = true;
        } else if p.upper_catch().is_none() {
            self.upper_outlives_sub = false;
        }
        // The main slot's own blend is not reported while a sub animation of
        // another clip plays over it.
        let own_blend_shown = self.upper_outlives_sub && p.model_sub().is_none();
        if sub_now.is_some_and(|name| name != p.anim.name) && !own_blend_shown {
            interp.clear();
        }
        if let Some(ended) = self.sub_clip
            && (sub_now.is_none() || p.ended_model_sub().is_some())
            && !p.upper_cleared()
            && ended != p.anim.name
            && !self.stale_clip.is_empty()
        {
            // The animator restarts the main clip (`endSubAnim`): the fade the
            // clip had begun is replaced by the restart's own, from the pose the sub
            // animation left on screen (named, as the game records it, after the clip
            // before it).
            interp.clear();
            // (A partial animation's end left no sub animation on screen: the pose it fades
            // from is the blend of the row before, partial included.)
            let slot = self.stale_clip;
            let frozen = shown_before
                .map(|(shown, _)| Pose {
                    slot,
                    layers: vec![shown],
                    partial: Vec::new(),
                })
                .or_else(|| self.shown_blend.clone().map(|flat| Pose { slot, ..flat }));
            let restart_frames = if p.ended_model_sub().is_some() {
                interp_frames(p.anim.name, ended) as u32
            } else {
                RESTART_BLEND_FRAMES
            };
            let share = 1. - 1. / restart_frames as f32;
            let fade = FPS / restart_frames as f32;
            self.blend.old = frozen
                .map(|pose| vec![(pose, share, fade)])
                .unwrap_or_default();
            kept = self.blend.kept();
            self.restart = Some((self.stale_clip, 0, restart_frames));
        }
        self.sub_clip = sub_now;
        // What the blend shows in this row, for the fade that follows a partial animation.
        self.shown_blend = blend_shows.then(|| {
            let mut flat = Pose::default();
            for (old, share, _) in &self.blend.old {
                let mut held = old.clone();
                held.scale(*share);
                flat.merge(held);
            }
            let mut current = self.blend.current.clone();
            current.scale(1. - kept);
            flat.merge(current);
            flat
        });
        if let Some((from, done, length)) = self.restart {
            let done = done + 1;
            self.restart = (done < length).then_some((from, done, length));
            if interp.is_empty() && done < length {
                interp = format!("{from}:{}", num(done as f32 / length as f32));
            }
        }
        PoseStep {
            covering,
            kept,
            interp,
        }
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
    pub metadata: serde_json::Value,
    pub grabceil_seed: Option<serde_json::Value>,
    pub geyser_seed: Option<serde_json::Value>,
    pub rocket_seed: Option<serde_json::Value>,
    pub press_seed: Vec<HashMap<String, String>>,
}
impl TraceFile {
    pub fn read(path: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let mut trace = Self::parse(&text)?;
        let meta = std::path::Path::new(path).with_extension("meta.json");
        if meta.exists() {
            trace.metadata = serde_json::from_str(
                &std::fs::read_to_string(&meta).map_err(|e| format!("{}: {e}", meta.display()))?,
            )
            .map_err(|e| format!("{}: {e}", meta.display()))?;
        }
        let seed = std::path::Path::new(path).with_extension("geyser_seed.json");
        if seed.exists() {
            trace.geyser_seed = Some(
                serde_json::from_str(
                    &std::fs::read_to_string(&seed)
                        .map_err(|e| format!("{}: {e}", seed.display()))?,
                )
                .map_err(|e| e.to_string())?,
            );
        }
        let seed = std::path::Path::new(path).with_extension("grabceil_seed.json");
        if seed.exists() {
            trace.grabceil_seed = Some(
                serde_json::from_str(&std::fs::read_to_string(&seed).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?,
            );
        }
        let seed = std::path::Path::new(path).with_extension("rocket_seed.json");
        if seed.exists() {
            trace.rocket_seed = Some(
                serde_json::from_str(&std::fs::read_to_string(seed).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?,
            );
        }
        let side = std::path::Path::new(path).with_extension("press_parts.csv");
        if side.exists() {
            let text = std::fs::read_to_string(side).map_err(|e| e.to_string())?;
            let mut lines = text.lines();
            let header: Vec<_> = lines.next().ok_or("part CSV header")?.split(',').collect();
            let mut first = None;
            for line in lines {
                let mut quoted = false;
                let mut cell = String::new();
                let mut cells = Vec::new();
                for c in line.chars() {
                    if c == '"' {
                        quoted = !quoted;
                    } else if c == ',' && !quoted {
                        cells.push(std::mem::take(&mut cell));
                    } else {
                        cell.push(c);
                    }
                }
                cells.push(cell);
                let row: HashMap<String, String> = header
                    .iter()
                    .zip(cells)
                    .map(|(k, v)| (k.to_string(), v))
                    .collect();
                let frame = row.get("frame").ok_or("part frame")?.clone();
                if first.as_ref().is_some_and(|f| f != &frame) {
                    break;
                }
                first = Some(frame);
                trace.press_seed.push(row);
            }
        }
        Ok(trace)
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
        Ok(Self {
            rows,
            metadata: serde_json::Value::Null,
            geyser_seed: None,
            rocket_seed: None,
            press_seed: Vec::new(),
            grabceil_seed: None,
        })
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
    /// The quicksand's depth after the step a row records (`PlayerSandSinkAffect::update`),
    /// rebuilt from the rows of the episode: the affect runs after the state's step on the
    /// contacts and position the previous row ends with, so the depth follows from the floor
    /// the previous row stood on and how far it sat below it. GUESS: the surface is flat under
    /// the body (the last `SandSink` floor height stands for the probe's hit), and the episode
    /// starts where no earlier row is in the sand's states, the slope slide or the air.
    fn sand_depth(&self, row: usize) -> Option<f32> {
        let collision = |at: usize| {
            self.cell(at, "collision")
                .unwrap_or("")
                .split(';')
                .filter_map(|s| s.split_once('='))
                .collect::<BTreeMap<_, _>>()
        };
        let on_sand = |at: usize| {
            self.cell(at, "ground") == Some("1") && collision(at).get("gcode") == Some(&"SandSink")
        };
        let in_episode = |at: usize| {
            let state = self.cell(at, "state").unwrap_or("");
            self.cell(at, "ground") != Some("1")
                || state.starts_with("SandSink")
                || state.starts_with("Slope")
                || on_sand(at)
        };
        if !in_episode(row) {
            return None;
        }
        let mut start = row;
        while start > 0 && in_episode(start - 1) {
            start -= 1;
        }
        let (mut depth, mut safe, mut surface) = (0.0_f32, true, None);
        for at in start.max(1)..=row {
            let before = at - 1;
            let y = self.number(before, "py").unwrap_or(0.);
            if on_sand(before) {
                let floor = self.number(before, "floor_y").unwrap_or(0.);
                surface = Some(floor);
                safe = false;
                let measured = (floor - y).max(0.).min(depth);
                let state = self.cell(at, "state").unwrap_or("");
                let vy = self.number(at, "vy").unwrap_or(0.);
                let sinking = state.starts_with("SandSink") && !state.ends_with("Jump") || vy <= 0.;
                depth = (measured + if sinking { 200. / 240. } else { -vy }).max(0.);
            } else if !safe && depth > 0. {
                depth = match surface {
                    Some(floor) if floor >= y && floor <= y + depth => {
                        (floor - y).max(0.).min(depth)
                    }
                    _ => {
                        safe = true;
                        0.
                    }
                };
            } else {
                safe = depth > 0.;
                depth = 0.;
            }
            if depth.abs() <= 0.001 {
                safe = true;
            }
        }
        (depth > 0.).then_some(depth)
    }
    /// `mSlopeForceCounter` of a recorded slope slide: the steps in a row, before this row, that
    /// the body stood on ground flatter than `SlopeSlideAngleEnd`.
    fn slope_flat_steps(&self, row: usize) -> u32 {
        // The slide's step reads the contacts the previous row ended with.
        let mut steps = 0;
        let Some(mut at) = row.checked_sub(1) else {
            return 0;
        };
        loop {
            let flat = self
                .cell(at, "state")
                .is_some_and(|s| s.starts_with("Slope"))
                && self.cell(at, "ground") == Some("1")
                && self
                    .cell(at, "floor_normal")
                    .and_then(vector_cell)
                    .is_some_and(crate::movement::Player::is_slide_end_normal);
            if !flat {
                return steps;
            }
            steps += 1;
            match at.checked_sub(1) {
                Some(previous) => at = previous,
                None => return steps,
            }
        }
    }
    fn sand_hint(&self, row: usize) -> Option<crate::movement::SandHint> {
        let depth = self.sand_depth(row)?;
        let mut hint = crate::movement::SandHint {
            depth,
            ..Default::default()
        };
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
        let mut power = 0.;
        for r in row.saturating_sub(40).max(1)..=row {
            let swing = front(r).angle_between(front(r - 1)).to_degrees();
            power += (crate::movement::turn_power(swing) - power) * 0.15;
        }
        hint.turn_power = power;
        if let Some(input) = self.cell(row, "move_input") {
            let mut parts = input.split('/').filter_map(|v| v.parse::<f32>().ok());
            if let (Some(x), Some(z)) = (parts.next(), parts.next()) {
                hint.last_input = Vec3::new(x, 0., z);
            }
        }
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
            launch: age > 0
                && matches!(
                    self.cell(row + 1, "cap_state"),
                    Some("Throw" | "ThrowSpiral" | "ThrowRolling")
                ),
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
    /// What the row records of the look-at (see `LookHint`).
    pub fn look_hint(&self, row: usize) -> Option<LookHint> {
        let keeper = self.cell(row, "look_keeper")?;
        let counter = keeper
            .split(';')
            .find_map(|kv| kv.strip_prefix("count=")?.parse().ok());
        let head = self
            .cell(row, "bones")
            .and_then(|bones| bones.split('|').find_map(|b| b.strip_prefix("Head:")))
            .and_then(vector_cell);
        let n = |c| self.number(row, c).unwrap_or(0.);
        Some(LookHint {
            feet: Vec3::new(n("px"), n("py"), n("pz")),
            head,
            head_rotation: self.cell(row, "head_q").and_then(|text| {
                let mut axes = text.split('/').map(|v| v.parse::<f32>());
                Some(Quat::from_xyzw(
                    axes.next()?.ok()?,
                    axes.next()?.ok()?,
                    axes.next()?.ok()?,
                    axes.next()?.ok()?,
                ))
            }),
            counter,
            target_stage: self.cell(row, "look_target").and_then(vector_cell),
        })
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
        // The steps the ground states have gone on without ground under them
        // (`PlayerJudgeSpeedCheckFall`): the airborne rows before this one.
        let mut lost = 0;
        while lost < 5 && lost < row && self.cell(row - 1 - lost, "ground") == Some("0") {
            lost += 1;
        }
        counters.push(("ground_lost".to_string(), lost as i64));
        let collision = |at| {
            self.cell(at, "collision")
                .unwrap_or("")
                .split(';')
                .filter_map(|s| s.split_once('='))
                .collect::<BTreeMap<_, _>>()
        };
        let current = collision(row);
        let floor_contact = if current.get("ground") == Some(&"1") {
            current.get("gcode").map(|code| {
                let mut steps = 1;
                while steps <= row && collision(row - steps).get("ground") == Some(&"1") {
                    steps += 1;
                }
                let normal = current
                    .get("gn")
                    .and_then(|s| vector_cell(s))
                    .unwrap_or(Vec3::Y);
                (
                    crate::geometry::CollisionCodes {
                        floor: (*code).into(),
                        material: current.get("gmat").copied().unwrap_or("").into(),
                        floor_index: current
                            .get("gcodeidx")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0),
                        ..Default::default()
                    },
                    normal,
                    steps as u32,
                )
            })
        } else {
            None
        };
        let pose = self
            .cell(row, "bits_quat")
            .and_then(|text| {
                let words = text
                    .split('/')
                    .map(|w| {
                        u32::from_str_radix(w.trim_start_matches("0x"), 16)
                            .ok()
                            .map(f32::from_bits)
                    })
                    .collect::<Option<Vec<_>>>()?;
                let [x, y, z, w] = words[..] else {
                    return None;
                };
                Some(Quat::from_xyzw(x, y, z, w))
            })
            .or_else(|| {
                ["qx", "qy", "qz", "qw"]
                    .iter()
                    .all(|c| self.number(row, c).is_some())
                    .then(|| Quat::from_xyzw(n("qx"), n("qy"), n("qz"), n("qw")))
            });
        Snapshot {
            mode,
            geyser_collider_from: (mode == Some(Move::SandGeyser) && row > 0).then(|| {
                Vec3::new(
                    self.number(row - 1, "px").unwrap_or(0.),
                    self.number(row - 1, "py").unwrap_or(0.),
                    self.number(row - 1, "pz").unwrap_or(0.),
                )
            }),
            pose,
            floor_contact,
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
            sand: self.sand_hint(row),
            slope_flat_steps: self.slope_flat_steps(row),
        }
    }
}
fn vector_cell(text: &str) -> Option<Vec3> {
    let mut axes = text.split('/').map(|v| v.parse::<f32>());
    Some(Vec3::new(
        axes.next()?.ok()?,
        axes.next()?.ok()?,
        axes.next()?.ok()?,
    ))
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
