//! How wet the player is: `PlayerWetControl`, which starts the dripping effect (`StateWet`) once
//! the player has been out of the water for a while.
//!
//! Read from the 1.0.0 executable and checked against the `swim-lake-*` recordings (the drops
//! start on the step the player leaves the surface, 20 steps after leaving from under it, and
//! not at all when he is back in the water before that).
//!
//! The swim nerve records every step it keeps swimming: at the surface (`recordWaterSurface`)
//! or under it (`recordInWater`). The control updates at the end of every step.
use super::vector_lib as lib;

/// What the wetness counts down from, in steps (`recordInWater`'s 0x1e0).
const WET_STEPS: i32 = 480;
/// Steps the drops wait after the player was under the water (`recordInWater`'s 0x14).
const DRIP_WAIT: i32 = 20;
/// How fast the surface refills the wetness each step.
const SURFACE_REFILL: i32 = 10;
/// The drops start while the wetness is at least this (the first step after it was full).
const FRESH_WETNESS: i32 = 479;
/// Steps a deleted drip effect still counts as emitting, which keeps a new one from starting:
/// the longest life of its `WaterDrop` particles (`PlayerWetState`, 40 steps) plus the two the
/// effect system reports an emitter after its last particle (recorded: the drops were not
/// started again 12 and 40 steps after they were deleted).
const DROPS_LINGER: i32 = 42;

#[derive(Clone, Copy, Debug)]
pub(super) struct WetControl {
    /// Steps left before the drops may start, counted down while it is not negative
    /// (`this+0x24`); `-1` while nothing is pending.
    drip_wait: i32,
    /// The wetness, counted down every step it is not negative (`this+0x28`).
    wetness: i32,
    /// The player swam at the surface in the step that just ran (`this+0x30`).
    swam_at_surface: bool,
    /// The drops (the `StateWet` effect) are emitting.
    pub(super) dripping: bool,
    /// Steps the deleted drops still count as emitting.
    linger: i32,
}
impl Default for WetControl {
    /// `PlayerWetControl::reset`.
    fn default() -> Self {
        Self {
            drip_wait: -1,
            wetness: -1,
            swam_at_surface: false,
            dripping: false,
            linger: 0,
        }
    }
}
impl WetControl {
    /// `recordInWater`: the player is under the water.
    pub(super) fn record_in_water(&mut self) {
        self.drip_wait = DRIP_WAIT;
        self.delete_drops();
        self.wetness = WET_STEPS;
    }

    /// `PlayerEffect::tryDeleteWetEffect`: the drops stop emitting, their particles live on.
    fn delete_drops(&mut self) {
        if std::mem::take(&mut self.dripping) {
            self.linger = DROPS_LINGER;
        }
    }

    /// `recordWaterSurface`: the player swims at the surface, which wets him more slowly.
    pub(super) fn record_water_surface(&mut self) {
        self.swam_at_surface = true;
        self.delete_drops();
        self.wetness = lib::converge(self.wetness, WET_STEPS, SURFACE_REFILL);
    }

    /// `update`, at the end of the actor's step (the player is never in a wet area here).
    pub(super) fn update(&mut self) {
        let emitting = self.dripping || self.linger > 0;
        self.linger = (self.linger - 1).max(0);
        if self.drip_wait >= 0 {
            self.swam_at_surface = false;
            self.drip_wait -= 1;
            return;
        }
        if self.wetness >= FRESH_WETNESS && !self.swam_at_surface && !emitting {
            self.dripping = true;
        }
        self.swam_at_surface = false;
        if self.wetness >= 0 {
            self.wetness -= 1;
        }
    }
}
