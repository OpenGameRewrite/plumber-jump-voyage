//! The end of the cap's return: the tail of `HackCap::exeReturn` (`0x407434..0x40764c`), which
//! runs after the flight and the pose writes of every execution.
//!
//! 1. **The wall counter** (`+0x284`, an integer the return's first execution clears). The cap's
//!    collider is asked for a wall, else a ceiling, else the ground it touched in its last move
//!    (`rs::isCollidedWall`, `isCollidedCeiling`, `isCollidedGround`, each a deepest contact of
//!    zero or more; their normals are the struck face's, and the collider's ground normal). That
//!    normal's dot with the direction to the head (the target of this execution) below -sin 5
//!    degrees (`0x1880754`) means the cap is flying into the surface: the counter goes up by one.
//!    Anything else, including no contact at all (a zero normal) and no direction to the head,
//!    sets it back to zero. Without a stage there is nothing to touch.
//! 2. **The ends.** The return ends when the nerve's step is above 180 (`al::isGreaterStep` at
//!    `0x4074d4`, called with `0xb4` at `0x4074c8`; the step is the number of executions the nerve
//!    has finished, so the 182nd execution is the first to end it) or when the counter is above 45
//!    (`0x4074e0`).
//! 3. **What ending does** (`0x407570..0x407590`): `HackCap::hide(false)` (`0x3fa844`: the cap's
//!    collision, shadow and hit sensors off, its velocity zeroed, its model hidden, nerve
//!    `Hide`). The cap is not caught: nothing reached the player's sensors. The player's own
//!    movement then takes it back (`PlayerActorHakoniwa::movement`, first instruction
//!    `0x41dd6c`: `HackCap::isNoPutOnHide`) the way a catch does, see `Player::update_cap`.
//!
//! The binary has a third end, not ported: while the return flies to a captured target (flag
//! `+0x2a4`, set by `HackCap::receiveRequestTransferHack` at `0x3fc190`), the return ends by
//! `endHackThrowAndReturnHack` (`0x3fdca4`) when that target is less than 100 from the cap
//! (`0x187fcb8`, tested at `0x407568`). The rewrite has no capture transfer, so nothing sets
//! the flag; the CapHanger and switch lock-ons are a different feature (`prepareLockOn`,
//! `0x3fbe80`, which clears it).
//!
//! After the ends the nerve has one more block (`0x407594..0x407648`) for separate play, behind
//! the gate `Cap::throw_stay` documents (it never opens for a single player).
use super::pose_math::dot;
use bevy::prelude::Vec3;

/// `al::isGreaterStep(this, 180)` (`mov w1, #0xb4` at `0x4074c8`).
pub const LAST_STEP: u32 = 180;
/// The counter must exceed this to end the return (`cmp w8, #0x2d; b.gt` at `0x4074e0`).
pub const WALL_COUNTER_LIMIT: i32 = 45;
/// The dot of the surface normal and the direction to the head under which the cap is flying into
/// the surface: -sin 5 degrees (the literal at `0x1880754`).
pub const AGAINST_SURFACE: f32 = -0.087_155_74;

/// The counter after one more execution (`0x407478..0x4074c8`): up by one when the touched
/// surface faces against the direction to the head, otherwise back to zero. `normal` is zero when
/// nothing was touched; a direction of zero (no way to the head) also resets.
pub fn wall_counter(counter: i32, normal: Vec3, direction: Vec3) -> i32 {
    if dot(normal, direction) < AGAINST_SURFACE {
        counter + 1
    } else {
        0
    }
}

/// Whether this execution ends the return (`0x4074d4..0x4074e0`), by `HackCap::hide(false)`.
/// `step` is the nerve's step during the execution.
pub fn ends(step: u32, counter: i32) -> bool {
    step > LAST_STEP || counter > WALL_COUNTER_LIMIT
}
