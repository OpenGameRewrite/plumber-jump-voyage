//! The original's player sound tables and the pure rules that pick which
//! sounds start. The windowed audio (`sound.rs`) plays what these rules choose;
//! the headless trace (`trace.rs`) records the same choices by name.
use crate::movement::{Fx, Move, Throw};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Trigger {
    pub play: String,
    pub frame: f32,
    pub once: bool,
}
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Layer {
    pub resource: String,
    pub volume: Option<f32>,
    pub pitch: f32,
    pub random_pitch: f32,
    pub random_volume: f32,
    pub delay: u32,
    pub volume_curve: Option<[f32; 4]>,
    pub pitch_curve: Option<[f32; 4]>,
}
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Play {
    pub random: bool,
    #[serde(rename = "loop")]
    pub looping: bool,
    pub layers: Vec<Layer>,
}
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Group {
    /// Sounds attached to animation frames, by clip name.
    pub actions: BTreeMap<String, Vec<Trigger>>,
    /// Sounds a named reaction starts.
    pub reactions: BTreeMap<String, Vec<String>>,
    pub plays: BTreeMap<String, Play>,
}
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Resource {
    pub files: Vec<String>,
    pub volume: f32,
    pub random_semitones: Option<[f32; 2]>,
}
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Table {
    pub mario: Group,
    pub cap: Group,
    pub footwear: BTreeMap<String, String>,
    pub resources: BTreeMap<String, Resource>,
}
impl Table {
    /// The table written by `tools/extract-sounds.py`; absent without game data.
    pub fn load() -> Option<Self> {
        std::fs::read(crate::local_assets::asset_dir().join("local/sounds/player.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Self>(&b).ok())
    }
    pub fn group(&self, cap: bool) -> &Group {
        if cap { &self.cap } else { &self.mario }
    }
}
pub fn curve(c: Option<[f32; 4]>, input: f32) -> f32 {
    c.map_or(1., |[in_min, in_max, out_min, out_max]| {
        let k = ((input - in_min) / (in_max - in_min).max(1e-6)).clamp(0., 1.);
        out_min + (out_max - out_min) * k
    })
}
impl Layer {
    /// Gain (before the master volume) and playback pitch of one layer at a
    /// horizontal speed. `rand` supplies its random variation in 0 to 1, drawn
    /// in the order pitch, semitone range, volume.
    pub fn levels(
        &self,
        resource: &Resource,
        speed: f32,
        mut rand: impl FnMut() -> f32,
    ) -> (f32, f32) {
        let mut semitones = self.pitch + (rand() * 2. - 1.) * self.random_pitch;
        if let Some([low, high]) = resource.random_semitones {
            semitones += low + (high - low) * rand();
        }
        let pitch = 2f32.powf(semitones / 12.) * curve(self.pitch_curve, speed);
        let gain = resource.volume
            * self.volume.unwrap_or(1.)
            * curve(self.volume_curve, speed)
            * (1. - self.random_volume * rand());
        (gain, pitch)
    }
}
impl Group {
    /// The level sounds (`IsLevel`, the looping plays) a clip change carries
    /// on: both clips list them, so the sound that is playing keeps playing and
    /// the new clip's trigger does not start it again. The ground spin's charge
    /// sound goes on through the spin jump; the drop between ends it, and the
    /// jump restarting after the drop starts it anew.
    pub fn continuing(&self, previous: &str, next: &str) -> Vec<String> {
        let (Some(before), Some(after)) = (self.actions.get(previous), self.actions.get(next))
        else {
            return Vec::new();
        };
        after
            .iter()
            .filter(|t| self.plays.get(&t.play).is_some_and(|p| p.looping))
            .filter(|t| before.iter().any(|b| b.play == t.play))
            .map(|t| t.play.clone())
            .collect()
    }
    /// Plays a reaction starts.
    pub fn reaction_plays(&self, name: &str) -> Vec<String> {
        self.reactions.get(name).cloned().unwrap_or_default()
    }
    /// Plays an action schedules between two frames of its clip; a negative
    /// `from` means the clip is just starting. `replay` marks the scan after a
    /// looping clip wrapped: the triggers flagged one-time (`IsOneTime`) play
    /// once per start of the clip and stay silent on later passes.
    pub fn triggered(&self, action: &str, from: f32, to: f32, replay: bool) -> Vec<String> {
        let Some(triggers) = self.actions.get(action) else {
            return Vec::new();
        };
        triggers
            .iter()
            .filter(|trigger| !(replay && trigger.once))
            // A level sound that is playing goes on through the clip's next pass.
            .filter(|trigger| !(replay && self.plays.get(&trigger.play).is_some_and(|p| p.looping)))
            .filter(|trigger| {
                // Unset or negative start frames play as the clip begins.
                let at = trigger.frame.max(0.);
                if from < 0. {
                    at <= to
                } else {
                    from < at && at <= to
                }
            })
            .map(|trigger| trigger.play.clone())
            .collect()
    }
}
/// The original names voice plays with a lowercase `v` prefix ("vJump").
pub fn is_voice(play: &str) -> bool {
    let mut chars = play.chars();
    chars.next() == Some('v') && chars.next().is_some_and(char::is_uppercase)
}

/// One thing the sound system should do for an event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cue {
    /// Start a play of Mario's group directly.
    Play(&'static str),
    /// Start a play whose curves follow the animation's rate (the footsteps'
    /// "animation rate" parameter) instead of the walking speed.
    Rated(&'static str),
    /// Hold a play for as long as the cue keeps coming (the fall's wind).
    Hold(&'static str),
    /// Start a sound of Cappy's own audio keeper.
    CapPlay(&'static str),
    Reaction {
        cap: bool,
        name: &'static str,
    },
}
/// The player facts the event sounds depend on.
#[derive(Clone, Copy, Debug)]
pub struct FxContext {
    pub mode: Move,
}
/// The reaction a landing raises, by the state that takes over: the original
/// starts it from that state's first step. The crouch has none.
fn landing_reaction(mode: Move) -> Option<&'static str> {
    match mode {
        Move::Walk
        | Move::Run
        | Move::Jump
        | Move::DoubleJump
        | Move::TripleJump
        | Move::LongJump
        | Move::Backflip
        | Move::Sideflip
        | Move::PoundJump => Some("LandRun"),
        Move::SquatBrake | Move::Crouch | Move::Crawl => None,
        // Landing into the roll (`PlayerStateRolling::appear`) or from the
        // roll's jump (`exeJump`) raises the plain landing reaction; the roll's
        // own is `Fx::RollLand`.
        _ => Some("Land"),
    }
}
/// Sounds for one movement event.
pub fn fx_cues(fx: Fx, context: FxContext) -> Vec<Cue> {
    let reaction = |name| vec![Cue::Reaction { cap: false, name }];
    match fx {
        // The footfall, its material variant and the clothes' rustle.
        Fx::Step { left } => match (context.mode, left) {
            (Move::Crawl, _) => vec![],
            (_, true) => vec![
                Cue::Rated("RunL"),
                Cue::Rated("RunMatL"),
                Cue::Rated("ShakeNoiseL"),
            ],
            (_, false) => vec![
                Cue::Rated("RunR"),
                Cue::Rated("RunMatR"),
                Cue::Rated("ShakeNoiseR"),
            ],
        },
        Fx::FootLift { left } => vec![Cue::Rated(if left { "RunUpL" } else { "RunUpR" })],
        Fx::RollSpin => vec![Cue::Play("Roll")],
        Fx::FallWhoosh { .. } => vec![Cue::Hold("FallWhoosh_loop")],
        // The landing after a long fall raises only the high-fall reaction
        // (`PlayerStateWait::exeLandStiffen`); its sounds are the clip's.
        Fx::Land { hard: true } => reaction("LandFall"),
        Fx::Land { hard: false } => landing_reaction(context.mode).map_or(vec![], reaction),
        Fx::RollJumpLand => reaction("Land"),
        Fx::RollLand => reaction("RollLand"),
        Fx::PoundLand => reaction("HipDropLand"),
        Fx::RollStart => vec![],
        // `PlayerStateRolling::restartRolling` names the reaction by its restart counter:
        // `[1回目]` for the first boost of a roll, `[2回目以降]` after that (a roll that
        // began with a landing or a pound cancel starts with the counter at one).
        Fx::RollBoost { again } => reaction(if again { "RollBoostAgain" } else { "RollBoost" }),
        Fx::Throw(_) => vec![Cue::Reaction {
            cap: true,
            name: "Throw",
        }],
        // `HackCap::exeThrow` raises one voice reaction per throw in a row.
        Fx::ThrowVoice(count) => vec![Cue::Reaction {
            cap: true,
            name: match count {
                1 => "ThrowVoice1",
                2 => "ThrowVoice2",
                _ => "ThrowVoice3",
            },
        }],
        // `HackCap::exeThrowStay` starts a level sound on every step, and the
        // stay's end raises the reaction for the return from it.
        Fx::CapStay => vec![Cue::CapPlay("PgStayLv")],
        Fx::CapReturn => vec![Cue::Reaction {
            cap: true,
            name: "ReturnFromStay",
        }],
        // The collision reaction `HackCap` raises when it turns along a wall,
        // and the sound it starts with it.
        Fx::CapWallHit(_) => vec![
            Cue::Reaction {
                cap: true,
                name: "コリジョン衝突方向転換",
            },
            Cue::CapPlay("HitCol"),
        ],
        Fx::Catch => vec![Cue::Reaction {
            cap: true,
            name: "Catch",
        }],
        // `HackCap::exeTrample`'s first step raises its reaction (the trample
        // clip's own sounds follow its frames); the stay raises one when it
        // starts with the button held.
        Fx::CapBounce => vec![Cue::Reaction {
            cap: true,
            name: "Trample",
        }],
        Fx::CapStayStart => vec![Cue::Reaction {
            cap: true,
            name: "StayStart",
        }],
        Fx::Quenched => reaction("FireQuench"),
        // `PlayerStateSwim::tryReactionWaterIn` starts the entry sound itself; the exit and the
        // dive raise hit reactions (the exit's sound is in the reaction table).
        Fx::WaterIn { strong: true } => vec![Cue::Play("PlWaterIn")],
        Fx::WaterIn { strong: false } => vec![Cue::Play("PlWaterInMedSlow")],
        Fx::WaterOut => reaction("WaterOut"),
        Fx::WaterDive => reaction("WaterDive"),
        Fx::SwimHipDropLand => reaction("HipDropLand"),
        // The sound control's camera sounds: the camera dipping into the water, its ambience
        // while it and the player are under, and the swim stroke's bubbles (heard from under
        // the water; from above only the louder surfacing sound).
        Fx::CameraWater { entered: true } => vec![Cue::Play("CamWaterIn")],
        Fx::CameraWater { entered: false } => vec![Cue::Play("CamWaterOut")],
        Fx::CameraWaterHold => vec![Cue::Hold("CamWaterAmb")],
        Fx::SwimStrokeSound { camera_under: true } => {
            vec![Cue::Play("SwimStandBubble"), Cue::Play("SwimStand")]
        }
        Fx::SwimStrokeSound {
            camera_under: false,
        } => vec![Cue::Play("SwimStandBubbleOver")],
        Fx::Burned(again) => reaction(if again { "BurnBounce" } else { "BurnJump" }),
        _ => vec![],
    }
}

/// Which frames of the current clip passed since the last update.
#[derive(Default)]
pub struct AnimCursor {
    anim: (&'static str, u32),
    frame: f32,
    /// The clip has looped since it started.
    wrapped: bool,
}
/// What an update of the cursor asks the sound system to do.
pub struct AnimAdvance {
    /// The clip was restarted or replaced: its looping sounds end.
    pub restarted: bool,
    /// The clip it replaced (empty when it started the first time).
    pub previous: &'static str,
    /// Frames to look for triggers between, and whether the cap's sounds
    /// are searched too (a wrapped loop only replays Mario's).
    pub scan: Option<(f32, f32, bool)>,
    /// The clip has wrapped since it started: its one-time sounds are spent.
    pub replay: bool,
    /// A looping clip whose counter shows its length for a row before it
    /// wraps has reached that length: the loop's first frame sounds again
    /// in this row (the pole's turn clips).
    pub loop_start: bool,
}
impl AnimCursor {
    pub fn advance(
        &mut self,
        name: &'static str,
        serial: u32,
        frame: f32,
        looped: bool,
    ) -> AnimAdvance {
        let restarted = (name, serial) != self.anim;
        let previous = self.anim.0;
        if restarted {
            self.anim = (name, serial);
            self.frame = -1.;
            self.wrapped = false;
        }
        let scan = if frame >= self.frame {
            Some((self.frame, frame, true))
        } else if looped {
            // A looping clip wrapped: replay its repeating sounds. The loop's
            // first frame is the frame it just left (the frame counter goes
            // from the last frame to 1), so a sound on frame 0 is not heard
            // again (the ground spin's charge sound plays once).
            self.wrapped = true;
            Some((0., frame, false))
        } else {
            None
        };
        let loop_start = looped
            && crate::movement::clip::loop_slack(name) > 0.
            && crate::movement::clip::loop_length(name)
                .is_some_and(|length| frame >= length && self.frame < length);
        self.frame = frame;
        AnimAdvance {
            restarted,
            previous,
            scan,
            replay: self.wrapped || loop_start,
            loop_start,
        }
    }
}
/// The name the cap's sound table knows a clip of the cap by: the table does
/// not tell the hands apart (`FlyingWaitR` plays as `FlyingWait`).
pub fn cap_action_name(clip: &str) -> &str {
    match clip.strip_suffix(['L', 'R']) {
        Some(base) if base.starts_with("FlyingWait") => base,
        _ => clip,
    }
}
#[path = "tests/sound_table.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
