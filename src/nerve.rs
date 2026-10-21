//! Frame bookkeeping of the original's nerve machine, so the controller changes
//! state on the same frames as the real game.
//!
//! The game's player runs one nerve per frame (`al::NerveKeeper::update`). A
//! nerve that decides to change state only *names* the next nerve: that frame's
//! row already reads the new state with its step counter at 0, but the clip,
//! position and velocity are still the old state's. The new nerve's first
//! execution (`isFirstStep`: start the clip, apply the impulse, play the
//! sounds) happens the next frame. A landing has one more frame: the contact is
//! flagged in the frame the ground is reached, the state that handles it
//! (`Wait/Land`, a run, the next jump) is chosen the frame after, and that
//! state's first step is one frame later still.
//!
//! The controller models this with one deferred piece of work per state change
//! ([`Nerve::choose`]), run at the start of the next step ([`Nerve::start`]),
//! and a flag for the ground contact ([`Nerve::touchdown`]).
use crate::movement::Player;
use std::fmt;

/// What a state does on its first step: impulses, events, counters.
type Work = Box<dyn FnOnce(&mut Player) + Send + Sync>;

pub struct Nerve {
    /// The first-step work of the state chosen last step.
    work: Option<Work>,
    /// The clip the chosen state, or a state's own sub-phase, starts on the
    /// next step. Until then the old clip keeps playing.
    clip: Option<&'static str>,
    /// Steps the actor nerve has executed (the trace's `state_step`). A new
    /// actor nerve restarts it; a change of sub-state inside the same nerve
    /// (walking to skidding, landing to standing) keeps counting.
    pub step: u32,
    /// The actor nerve those steps belong to, by the name the game logs.
    pub actor: &'static str,
    /// A state was chosen in the step in progress: it has not run yet, so
    /// this step does not count for it and the old state does no more.
    pub deferred: bool,
    /// The chosen state is a new actor nerve (a restart of the step counter,
    /// even when the new nerve has the old one's name).
    pub renewed: bool,
    /// The ground was reached last step; the next step decides what happens.
    pub touchdown: bool,
}
impl Default for Nerve {
    fn default() -> Self {
        Self {
            work: None,
            clip: None,
            step: 0,
            // The player starts standing.
            actor: "Wait",
            deferred: false,
            renewed: false,
            touchdown: false,
        }
    }
}
impl fmt::Debug for Nerve {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Nerve")
            .field("work", &self.work.is_some())
            .field("clip", &self.clip)
            .field("step", &self.step)
            .field("actor", &self.actor)
            .field("deferred", &self.deferred)
            .field("renewed", &self.renewed)
            .field("touchdown", &self.touchdown)
            .finish()
    }
}
impl Nerve {
    /// Name the next state, a new actor nerve or a sub-state of the running
    /// one. Its clip and work wait for the next step.
    pub fn choose(
        &mut self,
        renew: bool,
        clip: &'static str,
        work: impl FnOnce(&mut Player) + Send + Sync + 'static,
    ) {
        self.queue(clip, work);
        self.deferred = true;
        self.renewed = renew;
    }
    /// A state chosen whose own code starts its clip and does its first-step work (the swim's
    /// nerves decide that themselves): the choice alone counts.
    pub fn choose_silently(&mut self, renew: bool) {
        self.clip = None;
        self.work = None;
        self.deferred = true;
        self.renewed = renew;
    }
    /// Queue a clip and first-step work without leaving the running nerve.
    pub fn queue(
        &mut self,
        clip: &'static str,
        work: impl FnOnce(&mut Player) + Send + Sync + 'static,
    ) {
        self.clip = Some(clip);
        self.work = Some(Box::new(work));
    }
    /// The chosen sub-state starts no clip of its own: the one playing goes on.
    pub fn keep_clip(&mut self) {
        self.clip = None;
    }
    /// The state chosen belongs to the running actor nerve even though it was
    /// chosen like a new one (a sub-nerve that sets up like a state): its step
    /// counter keeps counting.
    pub fn keep_actor_steps(&mut self) {
        self.renewed = false;
    }
    /// Ask for a clip on the next step.
    pub fn queue_clip(&mut self, clip: &'static str) {
        self.clip = Some(clip);
    }
    /// The clip the state will be playing once the queued one starts; the
    /// state's name in the trace follows it.
    pub fn pending_clip(&self) -> Option<&'static str> {
        self.clip
    }
    /// Take what the chosen state starts with, at the start of its first step.
    pub fn start(&mut self) -> (Option<&'static str>, Option<Work>) {
        (self.clip.take(), self.work.take())
    }
    /// Forget everything pending (a teleport or a reset).
    pub fn clear(&mut self) {
        self.work = None;
        self.clip = None;
        self.deferred = false;
        self.renewed = false;
        self.touchdown = false;
    }
}
