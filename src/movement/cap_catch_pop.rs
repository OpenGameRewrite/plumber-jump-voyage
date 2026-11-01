//! The pop after a cap caught in the air: `PlayerJudgeCapCatchPop::judge` (0x458c84) asked by
//! `tryChangeNerveFromAir`, the actor nerve `exeCapCatchPop` (0x4244a4) and the state it runs,
//! `PlayerStateCapCatchPop::exePop` (0x4680d8).
use super::*;

impl Player {
    /// The judge: not the 2D model (`IPlayerModelChanger` slot 3), inside the catch window
    /// (`PlayerCounterAfterCapCatch::isCapCatch`), a pop still unspent since the last landing
    /// (`PlayerCapActionHistory`'s byte at `+0x39`), no ground contact, the cap back on the head
    /// (`rs::isEquipCapCatched` is `HackCap::isCatched`: the cap's catch or hide nerve with its
    /// put-on flag) and a jump press this frame (`PlayerInput::isTriggerJump`).
    pub(super) fn judge_cap_catch_pop(&self, o: &Orders) -> bool {
        !self.model_2d()
            && self.catch_window > 0
            && self.cap_pop_possible
            && !self.is_collided_ground()
            && self.is_equip_cap_catched()
            && o.jump
    }

    /// `HackCap::isCatched` (0x3fe6b8): Catch or Hide AND byte +0x2a3. `CapNerve`
    /// in cap.rs represents only flight nerves, so a live `cap` never qualifies. Catch and
    /// Hide live in the player's catch counters / absent cap; winding and a queued launch
    /// instead mean SpinAttack / ThrowStart, even when no flight actor exists yet.
    fn is_equip_cap_catched(&self) -> bool {
        let catch_or_hide = self.cap_put_on // forcePutOn immediately selects Hide (0x3fce58).
            || self.cap.is_none() && !self.winding && self.launch.is_none();
        catch_or_hide && self.cap_catched
    }

    /// The actor takes the `CapCatchPop` nerve; its first step comes next frame like any
    /// other nerve's.
    pub(super) fn start_cap_catch_pop(&mut self) {
        self.change(Move::CapCatchPop, "JumpCapCatchCommon", |p| {
            // `exeCapCatchPop` first step (0x4244bc): the pop is spent until the history clears.
            p.cap_pop_possible = false;
            // `exePop` first step (0x4680f8..0x4681d0): end the sub animation, start
            // `JumpCapCatchAir` (`PlayerAnimation/ActionAnimCtrl.byml` aliases it to the
            // `JumpCapCatchCommon` clip, which `change` starts), restart the cap's catch with
            // `JumpCapCatch`, and set up the air control.
            p.sub.active = false;
            p.cap_catch_restart = true;
            p.air
                .setup(&mut p.v, p.facing, &cap_catch_pop_takeoff(p.constants));
            p.gravity = p.constants.cap_catch_pop_gravity;
            p.air_for = Some(Move::CapCatchPop);
        });
    }

    /// `exePop` after its first step: the air control, then the state ends when the clip has
    /// ended and the nerve has run 30 steps; the actor then chooses its ground-or-air nerve
    /// again (`setNerveOnGround`). A landing is the generic landing step's.
    pub(super) fn cap_catch_pop_step(&mut self, o: &Orders) {
        let steer = air_move::Steering {
            front: t::JUMP_ACCEL_FRONT,
            back: t::JUMP_ACCEL_BACK,
            turn: t::JUMP_ACCEL_TURN,
        };
        let push = self.push();
        self.air.update(&mut self.v, push, o.jump_held, &steer);
        let clip_ended = self.anim_done(clip::JUMP_CAP_CATCH_COMMON);
        if clip_ended && self.state_frames() >= CAP_CATCH_POP_MIN_STEPS {
            self.go(Move::Fall, "Fall");
        }
    }
}

/// `exePop` hands the air control the spin air's speed limit and base (`getSpinAirSpeedMax`),
/// `CapCatchPopPower` and `CapCatchPopGravity`, no extension and no input lock: the argument
/// order of 0x4681cc is that of the spin's own call (0x482f58).
fn cap_catch_pop_takeoff(constants: &PlayerConstants) -> air_move::Takeoff {
    air_move::Takeoff {
        speed_limit: t::SPIN_AIR_SPEED_MAX,
        base_speed: t::SPIN_AIR_SPEED_MAX,
        extend_frames: 0,
        power: t::CAP_CATCH_POP_POWER,
        gravity: constants.cap_catch_pop_gravity,
        input_lock: 0,
        speed_window: None,
        fall_max: t::FALL_MAX,
    }
}

/// `exePop`'s end test: `!al::isLessStep(this, 30)` (0x4681f0..0x4681fc).
const CAP_CATCH_POP_MIN_STEPS: u32 = 30;
