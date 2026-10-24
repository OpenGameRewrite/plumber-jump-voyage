//! The spine's swing while the cap is thrown (`PlayerJointControlCapThrow`): a turn of the first
//! spine joint about its own y axis that rises and falls over the throw.
//!
//! The spin-cap state asks for it in the first step of its ground and air nerves (`start`): an
//! angle, a kind of swing (0 to 3) and a side. The angle comes from the throw gesture
//! (`PlayerSpinCapAttack::setupAttackInfo`): it is nothing for a thrown button, so the swing only
//! shows with a left/right throw gesture, which the rewrite's input does not have yet and the
//! recordings do not contain; a controller asked for nothing turns nothing. What the callback
//! also does to the keeper's matrix (`+0xc0`, an offset and a z turn) has no reader in the
//! executable's dumped code and is left out, as is the swing's early end (`forceEnd`, when the
//! spin-cap state is cancelled).
//!
//! Each kind counts down its own number of steps: the turn `yaw * t` rises to 1 over the first
//! part, holds, and falls over the last (`update`); `t` is eased out.
use super::joint_math::{calc_rate01, ease_out, rotate_own_y, turn_scale_free};
use super::look_joint::Mat34;
use crate::skeleton_pose::Joint;

/// A kind of swing, as `start` takes it (its type 0 to 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Swing {
    /// Type 0: the turn is whole at once, holds, and falls over the last 20 steps of 30.
    Held,
    /// Type 1: it rises over 25 steps of 35 and falls over the last 10.
    Quick,
    /// Type 2: it rises over 25 steps of 45, holds for 10 and falls over the last 10.
    Long,
    /// Type 3: it rises over 40 steps of 50 and falls over the last 10; its angle limit is smaller.
    Wide,
}

impl Swing {
    /// The kind the spin-cap state asks for in the ground nerve, from the throw's count
    /// (`PlayerSpinCapAttack` field `+0x40`: 0 for the continuous throw, 1 and 2 for the first
    /// two of a row); nothing is asked for any other count.
    pub fn on_ground(count: u32) -> Option<Self> {
        match count {
            0 => Some(Self::Long),
            1 => Some(Self::Held),
            2 => Some(Self::Quick),
            _ => None,
        }
    }

    /// The same in the air nerve.
    pub fn in_air(count: u32) -> Option<Self> {
        match count {
            0 => Some(Self::Long),
            1 => Some(Self::Wide),
            2 => Some(Self::Quick),
            _ => None,
        }
    }

    /// The steps the swing counts down from (`+0x50`), the count its rise ends at (`+0x54`) and
    /// the length of its fall (`+0x58`).
    fn timing(self) -> (i32, i32, i32) {
        match self {
            Self::Held => (30, 30, 20),
            Self::Quick => (35, 10, 10),
            Self::Long => (45, 20, 10),
            Self::Wide => (50, 10, 10),
        }
    }

    /// The limit of the swing angle that turns the spine (degrees).
    fn angle_limit(self) -> f32 {
        match self {
            Self::Wide => 20.,
            _ => 30.,
        }
    }
}

/// What the spin-cap state asks (`PlayerJointParamCapThrow`, the keeper's block at `+0x50`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Request {
    swing: Swing,
    angle: f32,
}

/// The controller.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CapThrow {
    request: Option<Request>,
    /// The turn of the spine in degrees (`+0x38`).
    yaw: f32,
    /// Steps left (`+0x4c`; nothing while it is 0).
    count: i32,
    /// The count it started from, the count its rise ends at and the length of its fall.
    start_count: i32,
    rise_end: i32,
    fall: i32,
    /// The eased share of the turn (`+0x5c`).
    share: f32,
}

impl CapThrow {
    /// The spin-cap state's first step: ask for a swing of this kind and angle.
    pub fn ask(&mut self, swing: Swing, angle: f32) {
        self.request = Some(Request { swing, angle });
    }

    /// `PlayerJointControlKeeper::updateCapThrow`: take the request, then step the swing.
    pub fn update(&mut self) {
        if let Some(Request { swing, angle }) = self.request.take() {
            self.start(swing, angle);
        }
        if self.count == 0 {
            return;
        }
        // `al::converge(count, 0, 1)`.
        self.count -= 1;
        let count = self.count as f32;
        let raw = if self.count < self.rise_end {
            calc_rate01(count, 0., self.fall as f32)
        } else {
            (1. - calc_rate01(count, self.rise_end as f32, self.start_count as f32)).clamp(0., 1.)
        };
        self.share = ease_out(raw);
    }

    /// `start(angle, type, side)`: the spine's turn is the angle, reversed and limited.
    fn start(&mut self, swing: Swing, angle: f32) {
        let (start_count, rise_end, fall) = swing.timing();
        let limit = swing.angle_limit();
        self.yaw = -angle.clamp(-limit, limit);
        self.count = start_count;
        self.start_count = start_count;
        self.rise_end = rise_end;
        self.fall = fall;
    }

    /// `calcJointCallback`: turn `world`, the matrix of `joint` as the model has just computed it.
    pub fn calc(&self, joint: Joint, world: &mut Mat34) {
        if joint != Joint::Spine1 || self.count == 0 {
            return;
        }
        let degrees = self.yaw * self.share;
        turn_scale_free(world, |m| rotate_own_y(m, degrees));
    }
}

#[path = "../tests/cap_throw.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
