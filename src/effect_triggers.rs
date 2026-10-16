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
use crate::effect_data::{Library, Trigger};
use crate::movement::{Cap, CapPhase, Fx, Move, Player, Throw};
use crate::sound_table::{Cue, FxContext, fx_cues};
use std::collections::{HashMap, HashSet};

/// Frames after which a one-shot effect with a never-ending emitter is stopped.
pub const ONE_SHOT_LIMIT: u32 = 120;

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
    /// Kept running by the controller's own conditions.
    Code,
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
}

/// Inputs of one frame.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    pub anim: &'a str,
    pub serial: u32,
    pub frame: f32,
    pub cap_anim: Option<&'a str>,
    pub cap_serial: u32,
    pub reactions: &'a [&'a str],
    /// Effects the controller wants running this frame.
    pub code: &'a [&'a str],
}

#[derive(Clone, Debug)]
struct ActionEntry {
    key: Key,
    start: i32,
    keep: bool,
}

#[derive(Default)]
pub struct Triggers {
    mario: HashMap<String, Vec<ActionEntry>>,
    cap: HashMap<String, Vec<ActionEntry>>,
    reactions: HashMap<String, Vec<Key>>,
    /// Effects currently running because of a table entry, with whether to keep them.
    running: HashMap<Key, (Source, bool)>,
    /// Entries of the current clip waiting for their start frame.
    waiting: Vec<(Source, ActionEntry)>,
    last_mario: Option<(String, u32)>,
    last_cap: Option<(String, u32)>,
    /// Names of all effects in the library; controller requests for others are ignored.
    known: HashSet<String>,
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
                            keep: action.keep,
                        });
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
    }

    fn follow_clip(
        &mut self,
        changes: &mut Vec<Change>,
        source: Source,
        clip: Option<(&str, u32)>,
        frame: f32,
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
            let ended: Vec<Key> = self
                .running
                .iter()
                .filter(|(_, (s, keep))| *s == source && !keep)
                .map(|(k, _)| k.clone())
                .collect();
            for key in ended {
                self.running.remove(&key);
                changes.push(Change::Stop { key });
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
        // Entries that wait for a frame of the clip.
        let (due, still): (Vec<_>, Vec<_>) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|(s, e)| *s == source && frame >= e.start as f32);
        self.waiting = still;
        for (s, entry) in due {
            self.start_action(changes, s, &entry);
        }
    }

    pub fn evaluate(&mut self, input: &Frame) -> Vec<Change> {
        let mut changes = vec![];
        self.follow_clip(
            &mut changes,
            Source::Mario,
            Some((input.anim, input.serial)),
            input.frame,
        );
        let cap_clip = input.cap_anim.map(|name| (name, input.cap_serial));
        self.follow_clip(&mut changes, Source::Cap, cap_clip, 0.);
        for reaction in input.reactions {
            for key in self.reactions.get(*reaction).into_iter().flatten() {
                changes.push(Change::Start {
                    key: key.clone(),
                    source: Source::Mario,
                    once: true,
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
            if !self.running.contains_key(&key) {
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
            .filter(|(k, (s, _))| *s == Source::Code && !wanted.contains(&k.effect.as_str()))
            .map(|(k, _)| k.clone())
            .collect();
        for key in dropped {
            self.running.remove(&key);
            changes.push(Change::Stop { key });
        }
        changes
    }
}

/// Effects the controller keeps alive by its own conditions (the tables list
/// no clip for them). Speeds are in original units per frame.
pub fn code_driven(p: &Player) -> Vec<&'static str> {
    let mut wanted = vec![];
    let running = p.grounded && p.anim.name == "Move";
    if running && p.move_speed >= 20. {
        wanted.push("DashFast");
    } else if running && p.move_speed >= 15. {
        wanted.push("Dash");
    } else if running && p.move_speed >= 8. {
        wanted.push("Run");
    }
    if p.mode == Move::Roll && p.grounded {
        wanted.push("Rolling");
    }
    if matches!(p.mode, Move::BurnJump | Move::BurnBounce | Move::FireRun) || p.soot.smoking {
        wanted.push("StateFire");
    }
    wanted
}

/// Reaction names the effect tables listen to for one movement event: the
/// ones the sound tables use (shared through `fx_cues`) and a few more.
pub fn reactions_of(fx: Fx, context: FxContext, boosts: &mut u32) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = fx_cues(fx, context, boosts)
        .into_iter()
        .filter_map(|cue| match cue {
            Cue::Reaction { name, .. } => Some(name),
            _ => None,
        })
        .collect();
    match fx {
        Fx::Land { hard: true } => names.push("LandFall"),
        Fx::PoundLand => names.push("HipDropLand"),
        Fx::Quenched => names.push("SootWash"),
        Fx::Splash => names.push("WaterDive"),
        Fx::Catch => names.push("Catch"),
        _ => {}
    }
    names
}

/// The clip name Cappy's effect table knows for the thrown cap.
pub fn cap_clip(cap: &Cap, right_hand: bool) -> &'static str {
    match (cap.phase, cap.kind) {
        (CapPhase::Stay, _) => {
            if right_hand {
                "StayR"
            } else {
                "StayL"
            }
        }
        (_, Throw::Spin) => "FlyingWaitSpiral",
        (_, Throw::Down) => "FlyingWaitRolling",
        _ => {
            if right_hand {
                "FlyingWaitR"
            } else {
                "FlyingWaitL"
            }
        }
    }
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
    /// Timing of the effect an entry starts with (its default-floor resource).
    pub fn timing(&self, trigger: &Trigger) -> Timing {
        let mut timing = Timing {
            emitting: Some(0),
            linger: 0,
        };
        let Some(resource) = trigger
            .default_resource()
            .and_then(|r| self.resources.get(&r.name))
        else {
            return timing;
        };
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
        timing
    }
}

/// An effect the tracker keeps alive: its table entry, the actor whose effect
/// keeper owns it, frames of life left (none: until stopped) and how long its
/// particles outlive the emission.
struct Live {
    key: Key,
    owner: &'static str,
    left: Option<u32>,
    linger: u32,
}
/// The real game's name for the actor whose effect keeper runs an effect: its
/// model for Mario's own and the controller's, its cap for Cappy's.
fn owner(source: Source) -> &'static str {
    match source {
        Source::Cap => "cap",
        Source::Mario | Source::Code => "model",
    }
}
/// Follows the effects a run starts and how long each stays alive, without
/// drawing anything: what the headless trace records as `effect:` events and
/// `effects_live`.
pub struct Tracker {
    triggers: Triggers,
    library: Library,
    boosts: u32,
    cap_serial: u32,
    right_hand: bool,
    /// Effect name, entry, and frames of life left (none: until stopped).
    live: Vec<Live>,
}

impl Tracker {
    pub fn new(library: Library) -> Self {
        Self {
            triggers: Triggers::new(&library),
            library,
            boosts: 0,
            cap_serial: 0,
            right_hand: true,
            live: Vec::new(),
        }
    }

    /// One frame after the player stepped; `events` are the frame's movement
    /// events. Returns names of effects started and of effects alive.
    ///
    /// `frame` is the main clip's frame to trigger on. The caller says which one
    /// (the frame the clip displays in that frame's sample, or the one the
    /// controller holds after its step) so the trace can follow the game.
    pub fn step(&mut self, p: &Player, events: &[Fx], frame: f32) -> (Vec<String>, Vec<String>) {
        let context = FxContext {
            mode: p.mode,
            move_speed: p.move_speed,
            grounded: p.grounded,
        };
        let mut reactions = vec![];
        for fx in events {
            reactions.extend(reactions_of(*fx, context, &mut self.boosts));
            if matches!(fx, Fx::Throw(_)) {
                self.cap_serial += 1;
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
            cap_serial: self.cap_serial,
            reactions: &reactions,
            code: &code,
        });
        let mut started = vec![];
        for change in changes {
            match change {
                Change::Start { key, source, once } => {
                    let timing = self
                        .library
                        .effects
                        .get(&key.effect)
                        .and_then(|t| t.get(key.trigger))
                        .map(|t| self.library.timing(t))
                        .unwrap_or(Timing {
                            emitting: Some(0),
                            linger: 0,
                        });
                    let emitting = match timing.emitting {
                        Some(frames) => Some(frames),
                        None if once => Some(ONE_SHOT_LIMIT),
                        None => None,
                    };
                    let owner = owner(source);
                    started.push(format!("{}@{owner}", key.effect));
                    self.live.retain(|l| l.key != key);
                    self.live.push(Live {
                        key,
                        owner,
                        left: emitting.map(|e| e + timing.linger),
                        linger: timing.linger,
                    });
                }
                Change::Stop { key } => {
                    for live in &mut self.live {
                        if live.key == key {
                            let linger = live.linger;
                            live.left = Some(live.left.map_or(linger, |l| l.min(linger)));
                        }
                    }
                }
            }
        }
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
            "resources":[{"name":"A"}]}],
        "Late":[{"table":"Mario","actions":[{"name":"Jump","start":4,"keep":false}]}],
        "Stay":[{"table":"Mario","actions":[{"name":"Jump","start":-1,"keep":true}]}],
        "Thud":[{"table":"Reaction","reactions":["Land"],"resources":[{"name":"A"}]}],
        "Trail":[{"table":"Cap","actions":[{"name":"FlyingWaitR","start":-1}]}],
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
        reactions: &[],
        code: &[],
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
    assert!(ended.contains(&Change::Stop {
        key: effect("Late")
    }));
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
        reactions: &["Land"],
        ..frame("Jump", 5, 1.)
    });
    assert_eq!(
        hit,
        [Change::Start {
            key: effect("Thud"),
            source: Source::Mario,
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
    let mut p = Player::default();
    p.anim.name = "Move";
    p.move_speed = 16.;
    assert_eq!(code_driven(&p), ["Dash"]);
    p.move_speed = 9.;
    assert_eq!(code_driven(&p), ["Run"]);
    p.grounded = false;
    assert!(code_driven(&p).is_empty());
    // Reaction names share the sound rules and add the effect-only ones.
    let context = FxContext {
        mode: Move::Run,
        move_speed: 10.,
        grounded: true,
    };
    let mut boosts = 0;
    assert_eq!(
        reactions_of(Fx::Land { hard: true }, context, &mut boosts),
        ["LandRun", "LandFall"]
    );
    assert_eq!(
        reactions_of(Fx::PoundLand, context, &mut boosts),
        ["HipDropLand"]
    );
    // The tracker keeps one-shot effects alive for their emission plus their particles' life.
    let mut tracker = Tracker::new(library);
    let mut player = Player::default();
    let (started, alive) = tracker.step(&player, &[Fx::Land { hard: false }], 0.);
    assert_eq!(started, ["Thud@model"]);
    assert_eq!(alive, ["model:Thud"]);
    player.anim.serial = 1;
    let mut last = alive.len();
    for _ in 0..20 {
        last = tracker.step(&player, &[], 0.).1.len();
    }
    assert_eq!(last, 0, "3 frames of emission and 10 of life are over");
}
