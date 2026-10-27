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
use crate::effect_data::Library;
use crate::movement::{Cap, Fx, Move, Player};
use crate::sound_table::{Cue, FxContext, fx_cues};
use std::collections::{HashMap, HashSet};

use crate::effect_life::EMPTY_RESOURCE;

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
    /// Started by a hit reaction Mario's code raised on Cappy (the catch): its effect keeper's.
    CapReaction,
    /// Started by a hit reaction Cappy's own update raised on itself (a wall hit, the throw, the
    /// trample): its effect keeper's, started after the frame's sample.
    CapOwnReaction,
    /// Kept running by the controller's own conditions.
    Code,
    /// Kept running by Cappy's own code through its effect keeper (the rolling throw's
    /// grounding effect): the same, owned by the cap.
    CapCode,
    /// Started by the controller's own call through the player's effect keeper (the splash of
    /// entering and leaving the water): logged without the model's owner mark, and it ends
    /// with its particles.
    Emitted,
}

impl Source {
    /// Whether the effect is started after the sample the probe takes in the player's update and
    /// before the same frame's particle calculation: Cappy's own update runs there, so its
    /// effects have already been calculated once when they are first listed. Everything the
    /// player's update starts is first listed one calculation younger.
    fn after_sample(self) -> bool {
        matches!(self, Source::Cap | Source::CapOwnReaction | Source::CapCode)
    }
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
    /// Raised by Cappy's own update (not by the player's code, as the catch is).
    pub by_cap_update: bool,
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
    /// Effects the controller starts and deletes within this very frame (the cap's resting
    /// effect on the surface in the first step of its return): the original logs both.
    pub flashes: &'a [&'a str],
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
    /// The effects of Cappy's table: when the controller keeps one running, the cap owns it.
    cap_owned: HashSet<String>,
    /// Entries that follow the actor: only these end when their clip does,
    /// the others (dust left behind) live out their own particles.
    follows: HashSet<Key>,
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
                if trigger.table == "Cap" {
                    triggers.cap_owned.insert(effect.clone());
                }
                // Keeping the emitter lets a one-shot emission finish after
                // the clip ends (the brake's sparks); an emitter that never
                // ends by itself still stops with the clip (the ground spin's
                // dust, recorded stopping as the next clip starts).
                let endless = trigger.resources.iter().any(|r| {
                    library
                        .timing(Some(r))
                        .is_some_and(|t| t.last_alive.is_none())
                });
                // An effect a clip starts at one of its frames (not for as long as the clip plays) and
                // that does not fade out when deleted lives out its particles after the clip
                // changes (recorded: the air throw's `SpinRight` through the pound that cancels it).
                let fades = trigger.has_flag("OneTimeFade");
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
                            keep: action.keep && !endless
                                || (action.start >= 0 && !fades && !endless),
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
            changes.push(Change::Stop { key });
        }
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
            let source = match (reaction.cap, reaction.by_cap_update) {
                (false, _) => Source::Reaction,
                (true, false) => Source::CapReaction,
                (true, true) => Source::CapOwnReaction,
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
                let source = if self.cap_owned.contains(*name) {
                    Source::CapCode
                } else {
                    Source::Code
                };
                self.running.insert(key.clone(), (source, false));
                changes.push(Change::Start {
                    key,
                    source,
                    once: false,
                });
            }
        }
        for name in input.flashes {
            if self.known.contains(*name) && !wanted.contains(name) {
                let key = Self::code_key(name);
                let source = if self.cap_owned.contains(*name) {
                    Source::CapCode
                } else {
                    Source::Code
                };
                changes.push(Change::Start {
                    key: key.clone(),
                    source,
                    once: false,
                });
                changes.push(Change::Stop { key });
            }
        }
        let dropped: Vec<Key> = self
            .running
            .iter()
            .filter(|(k, (s, _))| {
                (matches!(*s, Source::Code | Source::CapCode)
                    || RUN_DUST.contains(&k.effect.as_str()))
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
    // The effects of the cap skimming and resting on the water's surface (its own code).
    let (skimming, resting) = p.cap_water_effects();
    if skimming {
        wanted.push("FlyWaterSurface");
    }
    if resting {
        wanted.push("StayWaterSurfaceR");
    }
    // The rolling throw's grounding effect (the cap's own code starts it).
    if p.cap.as_ref().is_some_and(|cap| cap.grounding_effect().0) {
        wanted.push("FlyRollingGrounding");
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

/// The effects the controller starts and deletes in the same frame.
pub fn code_flashes(p: &Player) -> Vec<&'static str> {
    let mut flashes = vec![];
    if p.cap_water_flashes().1 {
        flashes.push("StayWaterSurfaceR");
    }
    if p.cap_water_flashes().0 {
        flashes.push("FlyWaterSurface");
    }
    flashes
}

/// The effects the controller emits by name in this frame: the water's splash.
pub fn emitted_by(fx: Fx) -> Vec<&'static str> {
    match fx {
        // `PlayerEffect::emitEffectWaterInOut`: the large splash when the entry set one of the
        // dive triggers (9, 15, 19, 29).
        Fx::WaterIn { strong: true } => vec!["WaterSurfaceInOutLarge"],
        Fx::WaterIn { strong: false } | Fx::WaterOut => vec!["WaterSurfaceInOut"],
        _ => vec![],
    }
}

/// The effects whose emit call the controller makes in this frame.
pub fn code_calls(p: &Player) -> Vec<&'static str> {
    let mut calls = vec![];
    if p.roll_effect_called() {
        calls.push("Rolling");
    }
    if p.cap.as_ref().is_some_and(|cap| cap.grounding_effect().1) {
        calls.push("FlyRollingGrounding");
    }
    calls
}

/// Reaction names the effect tables listen to for one movement event: the
/// ones the sound tables use (shared through `fx_cues`) and a few more.
pub fn reactions_of(fx: Fx, context: FxContext) -> Vec<Raised> {
    // Cappy's reactions come from its own update, except the catch (the player's code raises it).
    let by_cap_update = !matches!(fx, Fx::Catch);
    let mut raised: Vec<Raised> = fx_cues(fx, context)
        .into_iter()
        .filter_map(|cue| match cue {
            Cue::Reaction { cap, name } => Some(Raised {
                cap,
                by_cap_update: cap && by_cap_update,
                name,
            }),
            _ => None,
        })
        .collect();
    let on_player = |name| Raised {
        cap: false,
        by_cap_update: false,
        name,
    };
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

/// An effect the tracker keeps alive: its table entry, the actor whose effect
/// keeper owns it and how old its emitter set is.
struct Live {
    key: Key,
    owner: &'static str,
    /// Frames of the emitter set's calculation so far (`nn::vfx` emitter age).
    age: u32,
    /// The last age at which an emitter of the set is alive (`effect_life`); none: until
    /// stopped.
    last_alive: Option<u32>,
    /// Started in the water, with the variant the material code's water prefix picks: the
    /// effect ends when the prefix goes (`al::Effect::tryUpdateMaterial` replaces the emitter).
    water_bound: bool,
}
/// The material the water prefix picks effect variants by.
const WATER_MATERIAL: &str = "Water";
/// The real game's name for the actor whose effect keeper runs an effect: its
/// model for Mario's own and the controller's, its cap for Cappy's.
fn owner(source: Source) -> &'static str {
    match source {
        Source::Cap | Source::CapReaction | Source::CapOwnReaction | Source::CapCode => "cap",
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
    right_hand: bool,
    /// The effects whose emitter sets are alive.
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
        // The particle calculation that follows the previous sample runs before this frame's
        // update: a set with no emitter left alive in it is gone from this sample on.
        self.live.retain_mut(|live| {
            if live.last_alive.is_some_and(|last| live.age > last) {
                return false;
            }
            live.age += 1;
            true
        });
        let context = FxContext { mode: p.mode };
        let mut reactions = vec![];
        let mut emitted = vec![];
        for fx in events {
            reactions.extend(reactions_of(*fx, context));
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
            flashes: &code_flashes(p),
            wrapped: p.clip_wrapped,
        });
        let mut started = vec![];
        for change in changes {
            match change {
                Change::Start { key, source, .. } => {
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
                    // The first calculation of an effect started after the sample has already
                    // run when it is first listed (`Source::after_sample`).
                    let age = u32::from(source.after_sample());
                    // The empty dummy the water prefix picks keeps the effect alive without
                    // emitting anything (recorded: the jump out of the water).
                    if water_variant && choice.is_some_and(|c| c.name == EMPTY_RESOURCE) {
                        self.live.push(Live {
                            key,
                            owner,
                            age,
                            last_alive: None,
                            water_bound: true,
                        });
                        continue;
                    }
                    // An entry with no resource for this floor starts nothing
                    // that emits, so it never shows among the live effects.
                    let Some(timing) = choice.and_then(|c| self.library.timing(Some(c))) else {
                        continue;
                    };
                    // An emitter that never finishes is swapped for the water's when the
                    // material code changes, which ends the effect.
                    let water_bound = water_variant && timing.emitting.is_none();
                    self.live.push(Live {
                        key,
                        owner,
                        age,
                        last_alive: timing.last_alive,
                        water_bound,
                    });
                }
                // A stopped effect no longer emits at once; the particles it
                // left only fade.
                Change::Stop { key } => self.live.retain(|l| l.key != key),
                Change::Again { key } => {
                    let owner = self
                        .live
                        .iter()
                        .find(|live| live.key == key)
                        .map_or("model", |live| live.owner);
                    started.push(format!("{}@{owner}", key.effect));
                }
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
        // The effect keeper holds the material of the last floor the player stood on.
        if let Some(codes) = p.touched.floor.as_ref().filter(|c| !c.material.is_empty()) {
            self.floor = Some(codes.material.clone());
        }
        // `HackCap::forcePutOn` kills every emitter of the cap at once; the stop
        // of the table entries is logged by the next frame's clip change.
        if p.cap_put_on() {
            self.live.retain(|l| l.owner != "cap");
        }
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
        flashes: &[],
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
            by_cap_update: false,
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
    let names = |raised: Vec<Raised>| raised.iter().map(|r| r.name).collect::<Vec<_>>();
    assert_eq!(
        names(reactions_of(Fx::Land { hard: true }, context)),
        ["LandFall"]
    );
    assert_eq!(names(reactions_of(Fx::PoundLand, context)), ["HipDropLand"]);
    // The catch is a reaction of the cap.
    assert_eq!(
        reactions_of(Fx::Catch, context),
        [Raised {
            cap: true,
            by_cap_update: false,
            name: "Catch"
        }]
    );
    // A hit reaction's effect is listed while its emitter set lives: the table's emitter fires
    // for 3 frames and its particles live 10, so the set is alive through age 13 and the
    // landing's row is followed by 14 more rows (`last alive + 2` in all).
    let mut tracker = Tracker::new(library);
    let mut player = Player::default();
    let (started, alive) = tracker.step(&player, &[Fx::Land { hard: false }], 0.);
    // A landing reaction's effect is logged without the model's owner mark.
    assert_eq!(started, ["Thud"]);
    assert_eq!(alive, ["player:Thud"]);
    player.anim.serial = 1;
    let mut rows = 1;
    while !tracker.step(&player, &[], 0.).1.is_empty() {
        rows += 1;
        assert!(rows < 100, "the effect ends with its emitters");
    }
    assert_eq!(rows, 15);
    // The same reaction again starts it anew.
    let (started, alive) = tracker.step(&player, &[Fx::Land { hard: false }], 0.);
    assert_eq!(
        (started, alive),
        (vec!["Thud".to_string()], vec!["player:Thud".to_string()])
    );
    // Cappy's own update runs after the player's sample; the player's code does not.
    assert!(Source::Cap.after_sample() && Source::CapOwnReaction.after_sample());
    assert!(!Source::Mario.after_sample() && !Source::CapReaction.after_sample());
}
