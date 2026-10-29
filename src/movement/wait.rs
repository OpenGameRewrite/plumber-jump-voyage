//! The standing state's idle sequence (`PlayerStateWait`): `Wait`, then
//! `WaitRelaxStart`, `WaitRelax`, `WaitSnoozeStart`, `WaitSnooze`,
//! `WaitSleepStart` and `WaitSleep`.
//!
//! Written from the 1.0.0 executable's `exeWait`, `exeWaitRelaxStart`,
//! `exeWaitRelax`, `exeWaitSnoozeStart`, `exeWaitSnooze`, `exeWaitSleepStart`
//! and `exeWaitSleep` (the class has no decompiled source) and confirmed with
//! the recordings `idle-long` and `idle-sleep`.
//!
//! The sequence is a chain of sub-nerves of the wait state, each starting its
//! clip in its first step and choosing the next one:
//!
//! | nerve | clip | next one is chosen |
//! |---|---|---|
//! | `Wait` | `Wait` (loops) | when 180 steps have run |
//! | `WaitRelaxStart` | `WaitRelaxStart` | when the clip has ended |
//! | `WaitRelax` | `WaitRelax` (loops) | when 1200 steps have run |
//! | `WaitSnoozeStart` | `SnoozeStart` | when the clip has ended |
//! | `WaitSnooze` | `Snooze` (loops) | when 900 steps have run |
//! | `WaitSleepStart` | `SleepStart` | when the clip has ended |
//! | `WaitSleep` | `Sleep` (loops) | never |
//!
//! Nothing inside the sequence listens to the player. Every judge that ends
//! the wait state (a jump, the crouch, the cap, the stick) belongs to the
//! actor's own wait nerve (`PlayerActorHakoniwa::exeWait`) and is the same in
//! every sub-nerve (`isEnableCancelAction` is true in all of them); leaving
//! kills the state, and the next standing starts over in `Wait` with a fresh
//! step count. The sounds of the sequence (the sleeper's voices, the rustle of
//! the clothes) are the clips' own sound triggers, played by the sound tables.
//!
//! Not ported, because the rewrite has nothing they depend on: the clip
//! variants `BattleWait` and `DamageWait` (a boss fight, one point of life),
//! the stage's `PlayerAnimArea` and `SwitchPlayerAnim` areas, the requested
//! wait animation, the cap-man eyes that follow the clip, the sleeping music
//! (`MarioSleep` situation) and the bird that perches on the sleeper's nose.
use super::*;

/// `exeWait` stays in `Wait` while `al::isLessStep(180)` holds: the relax
/// starts in the nerve's 181st execution.
const RELAX_AFTER_STEPS: u32 = 180;
/// `exeWaitRelax` dozes off at this step (0x4b0).
const SNOOZE_AFTER_STEPS: u32 = 1200;
/// `exeWaitSnooze` falls asleep at this step (0x384).
const SLEEP_AFTER_STEPS: u32 = 900;

/// The sub-nerves of the wait state that stand on the spot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaitStage {
    Wait,
    RelaxStart,
    Relax,
    SnoozeStart,
    Snooze,
    SleepStart,
    Sleep,
}
impl WaitStage {
    /// The sub-nerve a clip belongs to; the landing and stand-up clips belong
    /// to no stage.
    fn of_clip(clip: &str) -> Option<Self> {
        Some(match clip {
            "Wait" => Self::Wait,
            "WaitRelaxStart" => Self::RelaxStart,
            "WaitRelax" => Self::Relax,
            "SnoozeStart" => Self::SnoozeStart,
            "Snooze" => Self::Snooze,
            "SleepStart" => Self::SleepStart,
            "Sleep" => Self::Sleep,
            _ => return None,
        })
    }
}

/// What the standing state keeps between steps.
#[derive(Clone, Debug, Default)]
pub(super) struct WaitControl {
    /// Steps the current sub-nerve has run (`al::NerveKeeper`'s step counter).
    /// It starts over whenever a sub-nerve is chosen; the new nerve's first
    /// step, which starts its clip, is step 0.
    pub(super) steps: u32,
    pub(super) area_clip: Option<&'static str>,
}

impl Player {
    pub(super) fn land_nerve(&self) -> bool {
        matches!(
            self.clip(),
            "Land" | "LandTurn" | "LandRolling" | "LandJump3"
        ) && self.ground_normal_or_up() != Vec3::Y
    }

    /// `PlayerStateWait::exeLand` / `rs::landGroundPoseAndSnap` (0x5658c4):
    /// align the body to the support before resolving movement, then add the
    /// distance to its plane to this step's ground pull, capped at FallSpeedMax.
    pub(super) fn land_ground(&mut self, scene: &Course) {
        let normal = self.ground_normal_or_up();
        if self.frame == 0 {
            self.slerp_pose_up(normal, 1., t::WAIT_POSE_DEGREE_MAX);
        }
        let snap = if self.frame == 0 && self.is_collided_ground() {
            let at = scene.in_water_frame(self.position_cm());
            pose_math::dot(at - self.collider.ground_position(), normal).clamp(0., t::FALL_MAX)
        } else {
            0.
        };
        self.swim_wait_ground(
            t::GRAVITY + snap,
            t::FALL_MAX,
            t::SLERP_QUAT_RATE_WAIT,
            t::WAIT_POSE_DEGREE_MAX,
        );
    }

    /// A standing clip starts: that is the first step of its sub-nerve.
    pub(super) fn restart_wait_steps(&mut self, clip: &str) {
        if WaitStage::of_clip(clip).is_some() {
            self.wait.steps = 0;
        }
    }
    /// Count one execution of the standing state and return the steps run
    /// before it, which is what the nerve's tests read.
    pub(super) fn count_wait_step(&mut self) -> u32 {
        let step = self.wait.steps;
        self.wait.steps += 1;
        step
    }
    /// Steps the sub-nerve has run, for a snapshot of a recorded row.
    pub(super) fn set_wait_steps(&mut self, steps: u32) {
        self.wait.steps = steps;
    }

    /// The decisions of the standing sub-nerve once the player stands still:
    /// the landing hands over to `Wait`, and each stage of the idle sequence
    /// chooses the next one.
    pub(super) fn wait_nerve(&mut self, step: u32) {
        if self.damage.hp == 1 {
            // PlayerStateWait::exeWait selects DamageWait while one HP remains.
            if self.clip() != "DamageWait" {
                self.start_clip("DamageWait");
            }
            return;
        }
        if matches!(self.clip(), "WaitHot" | "WaitCold" | "WaitVeryCold") {
            return;
        }
        if self.clip() == "Wait"
            && let Some(clip) = self.wait.area_clip
            && (step >= RELAX_AFTER_STEPS || clip != "WaitHot")
        {
            // tryChangeAreaAnim starts its clip immediately, then chooses AreaAnimWait.
            self.start_clip(clip);
            self.wait.steps = 0;
            self.sub_restarted = true;
            self.nerve.choose_silently(false);
            return;
        }
        let Some(stage) = WaitStage::of_clip(self.clip()) else {
            // A landing or stand-up clip is still playing; `Wait` is chosen
            // when it ends.
            if !self.standing_clip_playing() {
                self.play("Wait");
            }
            return;
        };
        match stage {
            WaitStage::Wait if step >= RELAX_AFTER_STEPS => self.play("WaitRelaxStart"),
            WaitStage::RelaxStart if self.anim_done(clip::WAIT_RELAX_START) => {
                self.play("WaitRelax")
            }
            // `exeWaitRelax` also asks that the player is not under the water
            // surface (`rs::isUnderWaterSurface`, 30 above the feet); a
            // standing player is never in the water.
            WaitStage::Relax if step >= SNOOZE_AFTER_STEPS => self.play("SnoozeStart"),
            WaitStage::SnoozeStart if self.anim_done(clip::SNOOZE_START) => self.play("Snooze"),
            WaitStage::Snooze if step >= SLEEP_AFTER_STEPS => self.play("SleepStart"),
            WaitStage::SleepStart if self.anim_done(clip::SLEEP_START) => self.play("Sleep"),
            _ => {}
        }
    }
}
