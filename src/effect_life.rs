//! How long an original effect stays alive: the rule behind the probe's `effects_live` column.
//!
//! An effect is listed live while its emitter set still has an emitter that survived the frame's
//! calculation (`al::isEffectEmitting` reads the set's emitter counter, which
//! `nn::vfx::EmitterSet::Calculate` rebuilds every frame from the emitters whose
//! `EmitterCalculator::Calculate` answered "alive"). The sample that sees the counter at zero is
//! the first one without the effect; the set itself is freed on the next frame.
//!
//! The emitter's answer is a plain function of its age (frames since its set was created, one per
//! calculation, read before this frame's increment), not of how many particles are left:
//! - a one-time emitter is alive while `age <= start + duration + life` (the data's base life; the
//!   randomised lives only shorten particles), plus the fade time when the emitter fades its alpha
//!   or scale out (`is_alpha_fade` / `is_scale_fade`, `alpha_fade_time`);
//! - an emitter that never stops emitting, or whose particles never die, is always alive;
//! - an ordinary child emitter (a child entry of the resource that emits from the parent's
//!   particles; the set creates it with the parent) is alive while its parent is, the parent's end
//!   being seen one frame late, and afterwards until `start + e + life`, where `e` is the child's
//!   age when it last emitted. It emits once from each parent particle when that particle's age
//!   reaches `timing` percent of its life (a particle's age is 1 in the calculation that creates
//!   it), so `e` follows the parent's last emission and the longest particle life;
//! - a child that starts from the parent's particles (`child_timing`) is created with the parent's
//!   particle and runs its own one-time rule from there (inferred: not seen in any recording).
//!
//! Limits: particle lives are drawn from the effect system's global random generator, which every
//! effect of the game advances, so the draw cannot be reproduced; the longest life is assumed
//! (exact whenever the last emission has a particle that long, which is likely for a handful of
//! particles). Stripe plugins add an extension to a one-time emitter's end that is not applied
//! (only demo effects use them).
use crate::effect_data::{Emitter, Library, ResourceChoice};

/// The resource the tables name for floors that raise nothing.
pub const EMPTY_RESOURCE: &str = "DummyEmitter";

/// How long an effect emits and how long its emitter set lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// Frames until every emitter has finished emitting; none while one never does.
    pub emitting: Option<u32>,
    /// The last set age at which an emitter is still alive; none while one never ends.
    pub last_alive: Option<u32>,
}

/// Emission frames of a one-time emitter: the first, the last, in set age.
fn emission_window(emitter: &Emitter) -> (u32, u32) {
    let emission = &emitter.emission;
    // The emitter fires every `interval` frames (the data's field is the gap plus one, and a gap
    // of zero or one fires every frame) from its start up to its duration.
    let period = emission.interval.saturating_sub(1).max(1);
    let fired = emission.duration.saturating_sub(1) / period;
    (emission.start, emission.start + fired * period)
}

/// The fade time added to a one-time emitter's end.
fn fade_frames(emitter: &Emitter) -> u32 {
    let color = &emitter.color;
    if color.alpha_fade != 0 || color.scale_fade != 0 {
        color.alpha_fade_time.max(0) as u32
    } else {
        0
    }
}

/// The last age at which the emitter alone is alive; none when it never ends.
fn own_end(emitter: &Emitter) -> Option<u32> {
    if !emitter.emission.one_time || emitter.life.infinite {
        return None;
    }
    let emission = &emitter.emission;
    Some(
        emission.start
            + emission.duration
            + emitter.life.frames.max(0) as u32
            + fade_frames(emitter),
    )
}

/// The age at which a parent's particle makes an ordinary child emit, counted in calculations
/// since the particle was created (the particle is one calculation old when it is created).
/// None when the particle dies before reaching the child's timing.
fn trigger_age(parent_life: f32, timing: f32) -> Option<u32> {
    // The game multiplies the life by the percent divided by one hundred, in single precision.
    let at = parent_life * (timing / 100.);
    let age = at.ceil().max(1.) as u32;
    (((age - 1) as f32) < parent_life).then_some(age)
}

/// The last age at which `child` is alive, given its parent and the parent's own end.
fn child_end(parent: &Emitter, parent_end: u32, child: &Emitter) -> Option<u32> {
    let life = child.life.frames.max(0) as f32;
    if let Some(timing) = child.emission.child_timing {
        // Created with the parent's last particle, then its own one-time rule from the delay
        // the timing percent of the parent's life gives.
        let (_, last) = emission_window(parent);
        let delay = (parent.life.frames.max(0) as f32 * (timing / 100.)).floor() as u32;
        if !child.emission.one_time || child.life.infinite {
            return None;
        }
        return Some(
            last + delay
                + child.emission.duration
                + child.life.frames.max(0) as u32
                + fade_frames(child),
        );
    }
    let (_, last) = emission_window(parent);
    let emitted = trigger_age(parent.life.frames.max(0) as f32, child.emission.timing)
        .map_or(0, |age| last + age - 1);
    let after_parent = child.emission.start as f32 + emitted as f32 + life;
    Some((parent_end + 1).max(after_parent as u32))
}

/// The end of an emitter's subtree: the emitter, then its children and theirs.
fn subtree_end(emitter: &Emitter, own: u32) -> Option<u32> {
    let mut end = own;
    for child in &emitter.children {
        let child_own = child_end(emitter, own, child)?;
        end = end.max(subtree_end(child, child_own)?);
    }
    Some(end)
}

fn visit_emitting(emitters: &[Emitter], emitting: &mut Option<u32>) {
    for e in emitters {
        if e.emission.one_time {
            let end = e.emission.start + e.emission.duration.max(1);
            *emitting = emitting.map(|t| t.max(end));
        } else {
            *emitting = None;
        }
        visit_emitting(&e.children, emitting);
    }
}

impl Library {
    /// Timing of the effect a resource choice starts; none when the choice
    /// emits nothing (an entry without a resource for the floor, or the empty
    /// dummy the tables name for floors that raise no dust).
    pub fn timing(&self, choice: Option<&ResourceChoice>) -> Option<Timing> {
        let resource = choice
            .filter(|c| c.name != EMPTY_RESOURCE)
            .and_then(|c| self.resources.get(&c.name))?;
        let mut emitting = Some(0);
        visit_emitting(&resource.emitters, &mut emitting);
        let last_alive = resource.emitters.iter().try_fold(0, |latest, root| {
            let end = own_end(root).and_then(|own| subtree_end(root, own))?;
            Some(latest.max(end))
        });
        Some(Timing {
            emitting,
            last_alive,
        })
    }
}

pub fn self_check() {
    // Resources written as the data holds them; the comments give the recorded spans (rows an
    // effect is listed live) they explain: the span is `last alive + 2` for an effect emitted
    // before the frame's calculation.
    let json = br#"{"version":1,"effects":{},"resources":{
        "Spin":{"emitters":[{"emission":{"one_time":true,"duration":1,"interval":1},"life":{"frames":16}}]},
        "Wind":{"emitters":[{"emission":{"one_time":true,"duration":1,"interval":1},"life":{"frames":16},
            "color":{"alpha_fade":1,"alpha_fade_time":4}}]},
        "Smoke":{"emitters":[{"emission":{"one_time":true,"duration":1,"interval":10},"life":{"frames":35,"random":70},
            "color":{"scale_fade":1,"alpha_fade_time":30}}]},
        "Catch":{"emitters":[{"emission":{"one_time":true,"duration":1,"interval":1},"life":{"frames":6}},
            {"emission":{"one_time":true,"start":2,"duration":1,"interval":1},"life":{"frames":8}}]},
        "Trail":{"emitters":[{"emission":{"one_time":false,"duration":1},"life":{"frames":8}}]},
        "Forever":{"emitters":[{"emission":{"one_time":true,"duration":1},"life":{"frames":8,"infinite":true}}]},
        "Bubbles":{"emitters":[{"emission":{"one_time":true,"duration":5,"interval":1},"life":{"frames":35,"random":30},
            "children":[{"emission":{"one_time":true,"duration":1,"interval":1,"timing":80},"life":{"frames":40,"random":50}}]}]}
        }}"#;
    let library = Library::from_bytes(json).expect("test library");
    let alive = |name: &str| {
        library
            .timing(Some(&ResourceChoice {
                name: name.into(),
                ..ResourceChoice::default()
            }))
            .expect("a resource")
            .last_alive
    };
    // The ground spin's blur: 19 rows. 0 + 1 + 16 = 17, and 17 + 2 = 19.
    assert_eq!(alive("Spin"), Some(17));
    // The air spin's wind: 23 rows. Its alpha fades over 4 frames after its 17 (the "one time
    // fade" that looked like four extra frames).
    assert_eq!(alive("Wind"), Some(21));
    // The landing's smoke on a carpet: 68 rows. Its scale fades over 30 frames after its 36.
    assert_eq!(alive("Smoke"), Some(66));
    // The cap catch: 13 rows, from the second emitter (2 + 1 + 8 = 11).
    assert_eq!(alive("Catch"), Some(11));
    assert_eq!(
        alive("Trail"),
        None,
        "an emitter that never stops emitting never ends"
    );
    assert_eq!(alive("Forever"), None, "particles that never die never end");
    // The water landing: 73 rows. The parent emits on frames 0 to 4 and its longest particle
    // (35) reaches 80% at its 28th calculation, so the child's last emission is at age 4 + 27 =
    // 31 and it lives 40 frames more.
    assert_eq!(alive("Bubbles"), Some(71));
    assert_eq!(trigger_age(35., 80.), Some(28));
    assert_eq!(trigger_age(10., 100.), Some(10));
    assert_eq!(trigger_age(10., 0.), Some(1));
}
