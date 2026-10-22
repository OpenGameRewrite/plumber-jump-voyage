//! Which of the original's effects start and stop each frame.
//!
//! The decision is a pure function of the previous frame's bookkeeping and
//! this frame's inputs (animation, hit reactions, cap animation, effects the
//! controller drives itself), so the windowed game and the headless trace
//! choose the same effects and the trace can log `effect:<name>` events.
//!
//! Semantics of the original's effect tables:
//! - an action trigger starts the effect when the named clip starts
//!   (start frame -1 or 0) or when the clip reaches frame `start`;
//! - when the clip changes, effects it started stop emitting unless the
//!   entry says to keep the emitter;
//! - a reaction trigger starts a one-shot effect;
//! - the controller itself keeps a few effects running (run dust, burning).
use crate::effect_data::{Library, ResourceChoice};
use crate::movement::{Cap, Fx, Move, Player};
use crate::sound_table::{Cue, FxContext, fx_cues};
use std::collections::{HashMap, HashSet};

/// Frames after which a one-shot effect with a never-ending emitter is stopped.
pub const ONE_SHOT_LIMIT: u32 = 120;
/// The resource the tables name for floors that raise nothing.
pub const EMPTY_RESOURCE: &str = "DummyEmitter";
/// Frames an effect keeps reporting itself emitting after its last particle's
/// life: the emitter starts a frame late and the check runs a frame behind
/// (recorded: a one-shot of 17 frames reads as live for 19).
const REPORT_LAG: u32 = 2;

/// The run state's dust: the run control picks which one the pose asks for
/// (`code_driven`), whoever started it. The turn clip's own entry for
/// `RunStart` (kept past the clip) does not take it out of the control's hands:
/// the dust of the next run clip replaces it.
const RUN_DUST: [&str; 4] = ["Run", "RunStart", "Dash", "DashFast"];

/// An effect entry: the effect's name and which of its table entries.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub effect: String,
    pub trigger: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Started by Mario's clip or a reaction.
    Mario,
    /// Started by Cappy's clip.
    Cap,
    /// Started by a hit reaction of the player actor itself (landings): the
    /// game logs these without the model's owner mark.
    Reaction,
    /// Started by a hit reaction raised on Cappy (the catch): its effect keeper's.
    CapReaction,
    /// Kept running by the controller's own conditions.
    Code,
    /// Started by the controller's own call through the player's effect keeper (the splash of
    /// entering and leaving the water): logged without the model's owner mark, and it ends
    /// with its particles.
    Emitted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// Start (or restart) the effect; `once` effects are never stopped by the tables.
    Start {
        key: Key,
        source: Source,
        once: bool,
    },
    Stop {
        key: Key,
    },
    /// An emit call for an effect that already runs: the original logs it,
    /// and nothing restarts.
    Again {
        key: Key,
    },
}

/// A hit reaction an actor raised: the effect tables listen to these by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Raised {
    /// Raised on Cappy rather than on the player.
    pub cap: bool,
    pub name: &'static str,
}

/// Inputs of one frame.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    pub anim: &'a str,
    pub serial: u32,
    pub frame: f32,
    pub cap_anim: Option<&'a str>,
    pub cap_serial: u32,
    /// The cap's clip frame, and whether its loop came back to the start.
    pub cap_frame: f32,
    pub cap_wrapped: bool,
    pub reactions: &'a [Raised],
    /// Effects the controller wants running this frame.
    pub code: &'a [&'a str],
    /// Effects the controller emits in this very frame through the player's effect keeper.
    pub emitted: &'a [&'a str],
    /// Effects whose emit call the controller makes in this very frame (the
    /// roll calls its effect every frame it is on the ground).
    pub calls: &'a [&'a str],
    /// The main clip looped back to its start in this frame: its entries fire
    /// again, as they do when a clip starts.
    pub wrapped: bool,
}

#[derive(Clone, Debug)]
struct ActionEntry {
    key: Key,
    start: i32,
    keep: bool,
    /// The clip frame that stops the effect (`EndFrame`).
    end: Option<i32>,
}

#[derive(Default)]
pub struct Triggers {
    mario: HashMap<String, Vec<ActionEntry>>,
    cap: HashMap<String, Vec<ActionEntry>>,
    reactions: HashMap<String, Vec<Key>>,
    /// The player's own entries that no reaction starts: the controller emits them by name.
    emitted: HashMap<String, Key>,
    /// Effects currently running because of a table entry, with whether to keep them.
    running: HashMap<Key, (Source, bool)>,
    /// Entries of the current clip waiting for their start frame.
    waiting: Vec<(Source, ActionEntry)>,
    last_mario: Option<(String, u32)>,
    last_cap: Option<(String, u32)>,
    /// Names of all effects in the library; controller requests for others are ignored.
    known: HashSet<String>,
    /// Entries that follow the actor: only these end when their clip does,
    /// the others (dust left behind) live out their own particles.
    follows: HashSet<Key>,
    /// Entries running that their clip started with itself (start frame -1),
    /// that follow the actor and are not kept.
    clip_bound: HashSet<Key>,
    /// Entries with an end frame that run in the clip now playing: the effect
    /// stops when the clip reaches the frame.
    ending: Vec<(Source, Key, i32)>,
}

impl Triggers {
    pub fn new(library: &Library) -> Self {
        let mut triggers = Self {
            known: library.effects.keys().cloned().collect(),
            ..Self::default()
        };
        for (effect, entries) in &library.effects {
            for (index, trigger) in entries.iter().enumerate() {
                let key = Key {
                    effect: effect.clone(),
                    trigger: index,
                };
                if trigger.has_flag("FollowTrans") || trigger.has_flag("FollowRotate") {
                    triggers.follows.insert(key.clone());
                }
                // Keeping the emitter lets a one-shot emission finish after
                // the clip ends (the brake's sparks); an emitter that never
                // ends by itself still stops with the clip (the ground spin's
                // dust, recorded stopping as the next clip starts).
                let endless = trigger.resources.iter().any(|r| {
                    library
                        .timing(Some(r))
                        .is_some_and(|t| t.emitting.is_none())
                });
                for action in &trigger.actions {
                    let table = if trigger.table == "Cap" {
                        &mut triggers.cap
                    } else {
                        &mut triggers.mario
                    };
                    table
                        .entry(action.name.clone())
                        .or_default()
                        .push(ActionEntry {
                            key: key.clone(),
                            start: action.start,
                            keep: action.keep && !endless,
                            end: action.end,
                        });
                }
                if trigger.table == "Reaction" && trigger.reactions.is_empty() {
                    triggers
                        .emitted
                        .entry(effect.clone())
                        .or_insert(key.clone());
                }
                for reaction in &trigger.reactions {
                    triggers
                        .reactions
                        .entry(reaction.clone())
                        .or_default()
                        .push(key.clone());
                }
            }
        }
        triggers
    }

    /// The table entry the controller starts an effect by (the first one).
    fn code_key(effect: &str) -> Key {
        Key {
            effect: effect.to_string(),
            trigger: 0,
        }
    }

    fn start_action(&mut self, changes: &mut Vec<Change>, source: Source, entry: &ActionEntry) {
        if entry.start < 0 && !entry.keep && self.follows.contains(&entry.key) {
            self.clip_bound.insert(entry.key.clone());
        } else {
            self.clip_bound.remove(&entry.key);
        }
        if self
            .running
            .insert(entry.key.clone(), (source, entry.keep))
            .is_some()
        {
            changes.push(Change::Stop {
                key: entry.key.clone(),
            });
        }
        changes.push(Change::Start {
            key: entry.key.clone(),
            source,
            once: false,
        });
        self.ending.retain(|(_, key, _)| *key != entry.key);
        if let Some(end) = entry.end {
            self.ending.push((source, entry.key.clone(), end));
        }
    }

    fn follow_clip(
        &mut self,
        changes: &mut Vec<Change>,
        source: Source,
        clip: Option<(&str, u32)>,
        frame: f32,
        wrapped: bool,
    ) {
        let last = if source == Source::Mario {
            &mut self.last_mario
        } else {
            &mut self.last_cap
        };
        let now = clip.map(|(name, serial)| (name.to_string(), serial));
        let changed = *last != now;
        *last = now;
        if changed {
            self.waiting.retain(|(s, _)| *s != source);
            self.ending.retain(|(s, _, _)| *s != source);
            let ended: Vec<Key> = self
                .running
                .iter()
                .filter(|(_, (s, keep))| *s == source && !keep)
                .map(|(k, _)| k.clone())
                .collect();
            for key in ended {
                self.running.remove(&key);
                self.clip_bound.remove(&key);
                if self.follows.contains(&key) {
                    changes.push(Change::Stop { key });
                }
            }
            let Some((name, _)) = clip else {
                return;
            };
            let table = if source == Source::Mario {
                &self.mario
            } else {
                &self.cap
            };
            for entry in table.get(name).cloned().unwrap_or_default() {
                if entry.start <= 0 {
                    self.start_action(changes, source, &entry);
                } else {
                    self.waiting.push((source, entry));
                }
            }
        }
        if wrapped && !changed {
            let Some((name, _)) = clip else {
                return;
            };
            let table = if source == Source::Mario {
                &self.mario
            } else {
                &self.cap
            };
            for entry in table.get(name).cloned().unwrap_or_default() {
                // Start frame 0 is passed again by every loop; -1 means the
                // clip's start only (the ground spin's dust starts once, the
                // spin jump's wind starts every loop).
                match entry.start {
                    0 => self.start_action(changes, source, &entry),
                    start if start > 0 => self.waiting.push((source, entry)),
                    _ => {}
                }
            }
        }
        // The loop has just restarted: its frame counts from the start again (a counter that
        // shows the clip's length for a row before it wraps has not wrapped yet).
        let frame = if wrapped && !changed { 0. } else { frame };
        // Entries that wait for a frame of the clip.
        let (due, still): (Vec<_>, Vec<_>) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|(s, e)| *s == source && frame >= e.start as f32);
        self.waiting = still;
        for (s, entry) in due {
            self.start_action(changes, s, &entry);
        }
        // Entries whose end frame the clip has reached.
        let (over, running): (Vec<_>, Vec<_>) = std::mem::take(&mut self.ending)
            .into_iter()
            .partition(|(s, _, end)| *s == source && frame >= *end as f32);
        self.ending = running;
        for (_, key, _) in over {
            self.running.remove(&key);
            self.clip_bound.remove(&key);
            changes.push(Change::Stop { key });
        }
    }

    /// The effect the clip started with itself and that follows the actor: it
    /// is alive until the clip ends and has no life of its own (recorded: the
    /// jump's dust is still there after the hundred frames of a spin jump,
    /// longer than its particles live).
    fn lives_with_clip(&self, key: &Key) -> bool {
        self.clip_bound.contains(key)
    }

    pub fn evaluate(&mut self, input: &Frame) -> Vec<Change> {
        let mut changes = vec![];
        self.follow_clip(
            &mut changes,
            Source::Mario,
            Some((input.anim, input.serial)),
            input.frame,
            input.wrapped,
        );
        let cap_clip = input.cap_anim.map(|name| (name, input.cap_serial));
        self.follow_clip(
            &mut changes,
            Source::Cap,
            cap_clip,
            input.cap_frame,
            input.cap_wrapped,
        );
        for reaction in input.reactions {
            let source = if reaction.cap {
                Source::CapReaction
            } else {
                Source::Reaction
            };
            for key in self.reactions.get(reaction.name).into_iter().flatten() {
                changes.push(Change::Start {
                    key: key.clone(),
                    source,
                    once: true,
                });
            }
        }
        for name in input.emitted {
            if let Some(key) = self.emitted.get(*name) {
                changes.push(Change::Start {
                    key: key.clone(),
                    source: Source::Emitted,
                    once: false,
                });
            }
        }
        // Controller-driven effects start when wanted and stop when not.
        let wanted: Vec<&str> = input
            .code
            .iter()
            .copied()
            .filter(|n| self.known.contains(*n))
            .collect();
        for name in &wanted {
            let key = Self::code_key(name);
            if self.running.contains_key(&key) {
                if input.calls.contains(name) {
                    changes.push(Change::Again { key });
                }
            } else {
                self.running.insert(key.clone(), (Source::Code, false));
                changes.push(Change::Start {
                    key,
                    source: Source::Code,
                    once: false,
                });
            }
        }
        let dropped: Vec<Key> = self
            .running
            .iter()
            .filter(|(k, (s, _))| {
                (*s == Source::Code || RUN_DUST.contains(&k.effect.as_str()))
                    && !wanted.contains(&k.effect.as_str())
            })
            .map(|(k, _)| k.clone())
            .collect();
        for key in dropped {
            self.running.remove(&key);
            // The step that ended the effect made its emit call first.
            if input.calls.contains(&key.effect.as_str()) {
                changes.push(Change::Again { key: key.clone() });
            }
            changes.push(Change::Stop { key });
        }
        changes
    }
}

/// Effects the controller keeps alive by its own conditions (the tables list
/// no clip for them). Speeds are in original units per frame.
pub fn code_driven(p: &Player) -> Vec<&'static str> {
    let mut wanted = vec![];
    // The run cycle's control picks the dust for the pose it plays.
    // The dust starts with the run state's first step and lasts as long as the
    // state, through its brake and turn and through the steps a run goes on
    // in the air before it falls (recorded: it stops with the state).
    if matches!(p.mode, Move::Walk | Move::Run | Move::Skid | Move::Turn)
        && matches!(p.anim.name, "Move" | "Brake" | "Turn" | "TurnPoint")
    {
        wanted.extend(p.run_cycle().effect);
    } else if let Some(effect) = p.run_start_effect() {
        // The run state's first step started its dust and chose the jump.
        wanted.push(effect);
    }
    if p.roll_effect_on() {
        wanted.push("Rolling");
    }
    if matches!(p.mode, Move::BurnJump | Move::BurnBounce | Move::FireRun) || p.soot.smoking {
        wanted.push("StateFire");
    }
    // The drops running off the player after he left the water.
    if p.is_dripping() {
        wanted.push("StateWet");
    }
    wanted
}

/// The effects the controller emits by name in this frame: the water's splash.
pub fn emitted_by(fx: Fx) -> Vec<&'static str> {
    match fx {
        Fx::WaterIn { .. } | Fx::WaterOut => vec!["WaterSurfaceInOut"],
        _ => vec![],
    }
}

/// The effects whose emit call the controller makes in this frame.
pub fn code_calls(p: &Player) -> Vec<&'static str> {
    let mut calls = vec![];
    if p.roll_effect_called() {
        calls.push("Rolling");
    }
    calls
}

/// Reaction names the effect tables listen to for one movement event: the
/// ones the sound tables use (shared through `fx_cues`) and a few more.
pub fn reactions_of(fx: Fx, context: FxContext, boosts: &mut u32) -> Vec<Raised> {
    let mut raised: Vec<Raised> = fx_cues(fx, context, boosts)
        .into_iter()
        .filter_map(|cue| match cue {
            Cue::Reaction { cap, name } => Some(Raised { cap, name }),
            _ => None,
        })
        .collect();
    let on_player = |name| Raised { cap: false, name };
    if let Fx::Quenched = fx {
        raised.push(on_player("SootWash"));
    }
    raised
}

/// The clip name Cappy's effect table knows for the thrown cap: the clip the
/// cap's nerve is playing.
pub fn cap_clip(cap: &Cap, _right_hand: bool) -> &'static str {
    cap.clip()
}

/// How long an effect emits and how long its particles outlive that.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// Frames until every emitter has finished; none while one never does.
    pub emitting: Option<u32>,
    /// Longest particle life in frames.
    pub linger: u32,
}

impl Library {
    /// Timing of the effect a resource choice starts; none when the choice
    /// emits nothing (an entry without a resource for the floor, or the empty
    /// dummy the tables name for floors that raise no dust).
    pub fn timing(&self, choice: Option<&ResourceChoice>) -> Option<Timing> {
        let mut timing = Timing {
            emitting: Some(0),
            linger: 0,
        };
        let resource = choice
            .filter(|c| c.name != EMPTY_RESOURCE)
            .and_then(|c| self.resources.get(&c.name))?;
        fn visit(timing: &mut Timing, emitters: &[crate::effect_data::Emitter]) {
            for e in emitters {
                if e.emission.one_time {
                    let end = e.emission.start + e.emission.duration.max(1);
                    timing.emitting = timing.emitting.map(|t| t.max(end));
                } else {
                    timing.emitting = None;
                }
                let life = if e.life.infinite {
                    30
                } else {
                    e.life.frames.max(0) as u32
                };
                timing.linger = timing.linger.max(life);
                visit(timing, &e.children);
            }
        }
        visit(&mut timing, &resource.emitters);
        Some(timing)
    }
}

/// An effect the tracker keeps alive: its table entry, the actor whose effect
/// keeper owns it and the frames of life left (none: until stopped).
struct Live {
    key: Key,
    owner: &'static str,
    left: Option<u32>,
    /// Started in the water, with the variant the material code's water prefix picks: the
    /// effect ends when the prefix goes (`al::Effect::tryUpdateMaterial` replaces the emitter).
    water_bound: bool,
}
/// The material the water prefix picks effect variants by.
const WATER_MATERIAL: &str = "Water";
/// Effect resources whose whole span (emission, particles and the report lag) was recorded:
/// the particles of their nested child emitters depend on the effect system's random draws,
/// so the table's lives cannot give it (the hip drop landing in water, three recordings,
/// 73 steps each).
const MEASURED_SPANS: [(&str, u32); 1] = [("PlayerHipDropLandWater", 73)];
fn measured_span(resource: &str) -> Option<u32> {
    MEASURED_SPANS
        .iter()
        .find(|(name, _)| *name == resource)
        .map(|(_, span)| *span)
}
/// The real game's name for the actor whose effect keeper runs an effect: its
/// model for Mario's own and the controller's, its cap for Cappy's.
fn owner(source: Source) -> &'static str {
    match source {
        Source::Cap | Source::CapReaction => "cap",
        Source::Mario | Source::Code => "model",
        Source::Reaction | Source::Emitted => "player",
    }
}
/// Follows the effects a run starts and how long each stays alive, without
/// drawing anything: what the headless trace records as `effect:` events and
/// `effects_live`.
pub struct Tracker {
    triggers: Triggers,
    library: Library,
    boosts: u32,
    right_hand: bool,
    /// Effect name, entry, and frames of life left (none: until stopped).
    live: Vec<Live>,
    /// The material of the floor the effects start on (their resource variant).
    floor: Option<String>,
    /// Whether the player's root was in water at the end of the last two steps, newest first:
    /// the effect keeper learns the material code a step late, and the effect that depends on
    /// it ends a step after that.
    water: [bool; 2],
}

impl Tracker {
    pub fn new(library: Library) -> Self {
        Self {
            triggers: Triggers::new(&library),
            library,
            boosts: 0,
            right_hand: true,
            live: Vec::new(),
            floor: None,
            water: [false; 2],
        }
    }
    /// Effects start on this floor material (the course's, when it has one).
    pub fn on_floor(mut self, material: Option<String>) -> Self {
        self.floor = material;
        self
    }

    /// One frame after the player stepped; `events` are the frame's movement
    /// events. Returns names of effects started and of effects alive.
    ///
    /// `frame` is the main clip's frame to trigger on. The caller says which one
    /// (the frame the clip displays in that frame's sample, or the one the
    /// controller holds after its step) so the trace can follow the game.
    pub fn step(&mut self, p: &Player, events: &[Fx], frame: f32) -> (Vec<String>, Vec<String>) {
        let context = FxContext { mode: p.mode };
        let mut reactions = vec![];
        let mut emitted = vec![];
        for fx in events {
            reactions.extend(reactions_of(*fx, context, &mut self.boosts));
            emitted.extend(emitted_by(*fx));
            if matches!(fx, Fx::Throw(_)) {
                self.right_hand = !p.anim.name.contains("Left");
            }
        }
        let cap_anim = p.cap.as_ref().map(|c| cap_clip(c, self.right_hand));
        let code = code_driven(p);
        let changes = self.triggers.evaluate(&Frame {
            anim: p.anim.name,
            serial: p.anim.serial,
            frame,
            cap_anim,
            cap_serial: p.cap.as_ref().map_or(0, Cap::clip_serial),
            cap_frame: p.cap.as_ref().map_or(0., Cap::clip_frame),
            cap_wrapped: p.cap.as_ref().is_some_and(Cap::clip_wrapped),
            reactions: &reactions,
            code: &code,
            emitted: &emitted,
            calls: &code_calls(p),
            wrapped: p.clip_wrapped,
        });
        let mut started = vec![];
        for change in changes {
            match change {
                Change::Start { key, source, once } => {
                    let owner = owner(source);
                    if matches!(source, Source::Reaction | Source::Emitted) {
                        started.push(key.effect.clone());
                    } else {
                        started.push(format!("{}@{owner}", key.effect));
                    }
                    self.live.retain(|l| l.key != key);
                    // The variant is picked by the material code the keeper held in this
                    // step: the water prefix wins over the floor.
                    let trigger = self
                        .library
                        .effects
                        .get(&key.effect)
                        .and_then(|t| t.get(key.trigger));
                    let in_water = self.water[0];
                    let material = if in_water {
                        Some(WATER_MATERIAL)
                    } else {
                        self.floor.as_deref()
                    };
                    let choice = trigger.and_then(|t| t.resource_for(material));
                    let water_variant = in_water
                        && trigger.is_some_and(|t| {
                            t.resources
                                .iter()
                                .any(|r| r.material.as_deref() == Some(WATER_MATERIAL))
                        });
                    // The empty dummy the water prefix picks keeps the effect alive without
                    // emitting anything (recorded: the jump out of the water).
                    if water_variant && choice.is_some_and(|c| c.name == EMPTY_RESOURCE) {
                        self.live.push(Live {
                            key,
                            owner,
                            left: None,
                            water_bound: true,
                        });
                        continue;
                    }
                    // An entry with no resource for this floor starts nothing
                    // that emits, so it never shows among the live effects.
                    let Some(timing) = choice.and_then(|c| self.library.timing(Some(c))) else {
                        continue;
                    };
                    let emitting = match timing.emitting {
                        Some(frames) => Some(frames),
                        None if once => Some(ONE_SHOT_LIMIT),
                        None => None,
                    };
                    // The effects a hit reaction starts (`Land`, `HipDropLand`)
                    // never leave the player's list in any recording of up to
                    // 190 frames; only starting the same effect again renews
                    // them. The others end when their emitters and particles do.
                    let measured = choice.and_then(|c| measured_span(&c.name));
                    let left = match source {
                        _ if measured.is_some() => measured,
                        Source::Reaction => None,
                        _ if self.triggers.lives_with_clip(&key) => None,
                        _ => emitting.map(|e| e + timing.linger + REPORT_LAG),
                    };
                    // An emitter that never finishes is swapped for the water's when the
                    // material code changes, which ends the effect.
                    let water_bound = water_variant && timing.emitting.is_none();
                    self.live.push(Live {
                        key,
                        owner,
                        left,
                        water_bound,
                    });
                }
                // A stopped effect no longer emits at once; the particles it
                // left only fade.
                Change::Stop { key } => self.live.retain(|l| l.key != key),
                Change::Again { key } => started.push(format!("{}@model", key.effect)),
            }
        }
        // The dust a one-step run state started is stopped with the state, in
        // the same frame: it is logged as started but never listed as live.
        if let Some(effect) = p.run_start_effect()
            && !matches!(p.mode, Move::Walk | Move::Run | Move::Skid | Move::Turn)
        {
            self.live.retain(|l| l.key.effect != effect);
        }
        // The effect that depends on the water prefix ends two steps after the player's root
        // left the water.
        if !self.water[1] {
            self.live.retain(|l| !l.water_bound);
        }
        self.water = [p.material_water(), self.water[0]];
        self.live.retain_mut(|live| match &mut live.left {
            Some(0) => false,
            Some(frames) => {
                *frames -= 1;
                true
            }
            None => true,
        });
        let mut alive: Vec<String> = self
            .live
            .iter()
            .map(|l| format!("{}:{}", l.owner, l.key.effect))
            .collect();
        alive.sort();
        alive.dedup();
        (started, alive)
    }
}

pub fn self_check() {
    let json = br#"{"version":1,"effects":{
        "Puff":[{"table":"Mario","actions":[{"name":"Jump","start":-1,"keep":false}],
            "flags":["FollowTrans"],"resources":[{"name":"A"}]}],
        "Late":[{"table":"Mario","actions":[{"name":"Jump","start":4,"keep":false}]}],
        "Stay":[{"table":"Mario","actions":[{"name":"Jump","start":-1,"keep":true}]}],
        "Thud":[{"table":"Reaction","reactions":["Land"],"resources":[{"name":"A"}]}],
        "Trail":[{"table":"Cap","actions":[{"name":"FlyingWaitR","start":-1}],
            "flags":["FollowTrans"]}],
        "Run":[{"table":"Mario","resources":[{"name":"A"}]}]},
        "resources":{"A":{"emitters":[{"emission":{"one_time":true,"duration":3},
            "life":{"frames":10}}]}}}"#;
    let library = Library::from_bytes(json).expect("test library");
    let mut triggers = Triggers::new(&library);
    let effect = |name: &str| Key {
        effect: name.into(),
        trigger: 0,
    };
    let frame = |anim: &'static str, serial: u32, at: f32| Frame {
        anim,
        serial,
        frame: at,
        cap_anim: None,
        cap_serial: 0,
        cap_frame: 0.,
        cap_wrapped: false,
        reactions: &[],
        code: &[],
        emitted: &[],
        calls: &[],
        wrapped: false,
    };
    assert!(triggers.evaluate(&frame("Wait", 1, 0.)).is_empty());
    let started = triggers.evaluate(&frame("Jump", 2, 0.));
    assert!(started.contains(&Change::Start {
        key: effect("Puff"),
        source: Source::Mario,
        once: false
    }));
    assert!(
        !started
            .iter()
            .any(|c| matches!(c, Change::Start { key, .. } if key.effect == "Late"))
    );
    assert!(
        triggers.evaluate(&frame("Jump", 2, 2.)).is_empty(),
        "nothing repeats while the clip plays"
    );
    let later = triggers.evaluate(&frame("Jump", 2, 4.));
    assert_eq!(later.len(), 1, "a start frame fires once, on reaching it");
    assert!(matches!(&later[0], Change::Start { key, .. } if key.effect == "Late"));
    let ended = triggers.evaluate(&frame("Fall", 3, 0.));
    assert!(ended.contains(&Change::Stop {
        key: effect("Puff")
    }));
    assert!(
        !ended.contains(&Change::Stop {
            key: effect("Late")
        }),
        "effects that do not follow the actor live out their particles"
    );
    assert!(
        !ended.contains(&Change::Stop {
            key: effect("Stay")
        }),
        "kept emitters outlive the clip"
    );
    // A restart of the same clip restarts its effects.
    triggers.evaluate(&frame("Jump", 4, 0.));
    let again = triggers.evaluate(&frame("Jump", 5, 0.));
    assert!(again.contains(&Change::Stop {
        key: effect("Puff")
    }));
    assert!(
        again
            .iter()
            .any(|c| matches!(c, Change::Start { key, .. } if key.effect == "Puff"))
    );
    // Reactions start one-shot effects.
    let hit = triggers.evaluate(&Frame {
        reactions: &[Raised {
            cap: false,
            name: "Land",
        }],
        ..frame("Jump", 5, 1.)
    });
    assert_eq!(
        hit,
        [Change::Start {
            key: effect("Thud"),
            source: Source::Reaction,
            once: true
        }]
    );
    // The cap's clip drives its own table.
    let thrown = triggers.evaluate(&Frame {
        cap_anim: Some("FlyingWaitR"),
        cap_serial: 1,
        ..frame("Jump", 5, 2.)
    });
    assert!(thrown.iter().any(
        |c| matches!(c, Change::Start { key, source: Source::Cap, .. } if key.effect == "Trail")
    ));
    let caught = triggers.evaluate(&frame("Jump", 5, 3.));
    assert!(caught.contains(&Change::Stop {
        key: effect("Trail")
    }));
    // Controller-driven effects follow a condition.
    let run = triggers.evaluate(&Frame {
        code: &["Run"],
        ..frame("Move", 6, 0.)
    });
    assert!(run.iter().any(
        |c| matches!(c, Change::Start { key, source: Source::Code, .. } if key.effect == "Run")
    ));
    assert!(
        triggers
            .evaluate(&Frame {
                code: &["Run"],
                ..frame("Move", 6, 1.)
            })
            .is_empty()
    );
    assert_eq!(
        triggers.evaluate(&frame("Move", 6, 2.)),
        [Change::Stop { key: effect("Run") }]
    );
    // A full-stick run: the run start's dust, then the run's.
    let floor = crate::replay::flat_floor();
    let forward = crate::input::Orders {
        direction: bevy::math::Vec2::NEG_Y,
        ..Default::default()
    };
    let mut p = Player::default();
    for _ in 0..4 {
        p.step(&forward, &floor);
    }
    assert_eq!(code_driven(&p), ["RunStart"]);
    for _ in 0..45 {
        p.step(&forward, &floor);
    }
    assert_eq!(code_driven(&p), ["Run"]);
    // The dust outlives the ground under a run that has not yet fallen.
    p.grounded = false;
    assert_eq!(code_driven(&p), ["Run"]);
    p.mode = Move::Fall;
    assert!(code_driven(&p).is_empty());
    // Reaction names share the sound rules and add the effect-only ones.
    let context = FxContext { mode: Move::Run };
    let mut boosts = 0;
    let names = |raised: Vec<Raised>| raised.iter().map(|r| r.name).collect::<Vec<_>>();
    assert_eq!(
        names(reactions_of(Fx::Land { hard: true }, context, &mut boosts)),
        ["LandFall"]
    );
    assert_eq!(
        names(reactions_of(Fx::PoundLand, context, &mut boosts)),
        ["HipDropLand"]
    );
    // The catch is a reaction of the cap.
    assert_eq!(
        reactions_of(Fx::Catch, context, &mut boosts),
        [Raised {
            cap: true,
            name: "Catch"
        }]
    );
    // A hit reaction's effect stays among the player's live effects; only the
    // same reaction starting it again renews it (no recording shows it end).
    let mut tracker = Tracker::new(library);
    let mut player = Player::default();
    let (started, alive) = tracker.step(&player, &[Fx::Land { hard: false }], 0.);
    // A landing reaction's effect is logged without the model's owner mark.
    assert_eq!(started, ["Thud"]);
    assert_eq!(alive, ["player:Thud"]);
    player.anim.serial = 1;
    let mut last = alive.len();
    for _ in 0..20 {
        last = tracker.step(&player, &[], 0.).1.len();
    }
    assert_eq!(
        last, 1,
        "a reaction's effect outlives its emission and life"
    );
    let (started, alive) = tracker.step(&player, &[Fx::Land { hard: false }], 0.);
    assert_eq!(
        (started, alive),
        (vec!["Thud".to_string()], vec!["player:Thud".to_string()])
    );
}
