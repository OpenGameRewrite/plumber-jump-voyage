//! `PlayerCounterForceRun` (0x43ea08 setup, 0x43ea14 update): the forced-run counter the
//! RocketFlower equipment feeds, and what the player's judges and controllers read from it.
//!
//! The counter holds the frames the player still runs by itself and the speed it runs at.
//! `PlayerActorHakoniwa::movement` asks the equipment user every frame and, while it holds
//! equipment, sets the counter to the equipment's frame count (2) and its speed (38) before
//! the state choice; the counter converges toward zero by one at the end of the same
//! movement. So 2 is a refresh window, not the duration: the equipment (`rocket_flower.rs`)
//! decides how long the run lasts.
use super::*;

/// What `RocketFlowerEquipWatcher` hands `tryGetEquipmentForceDashInfo` (0x2fe360 literals):
/// the frames of the refresh window and the forced speed.
pub const EQUIPMENT_FRAMES: i32 = 2;
pub const EQUIPMENT_SPEED: f32 = 38.;

/// `PlayerCounterForceRun`: frames left and the forced speed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ForceRun {
    pub counter: i32,
    pub speed: f32,
}
impl ForceRun {
    /// `isForceRun`: the counter is positive.
    pub fn is_active(&self) -> bool {
        self.counter > 0
    }
    /// `setupForceRun`: both values are stored, whatever the old counter was.
    pub fn setup(&mut self, frames: i32, speed: f32) {
        self.counter = frames;
        self.speed = speed;
    }
    /// `update`: `al::converge(counter, 0, 1)`. The speed stays.
    pub fn update(&mut self) {
        self.counter = if self.counter > 0 {
            self.counter - 1
        } else if self.counter < 0 {
            self.counter + 1
        } else {
            0
        };
    }
}

impl Player {
    /// The game's `forcerun=<counter>/<speed>` counter text.
    pub fn force_run_cell(&self) -> String {
        format!(
            "forcerun={}/{:.6}",
            self.force_run.counter, self.force_run.speed
        )
    }

    /// `PlayerActorHakoniwa::movement` (0x41df64..0x41df94), before the state choice: while the
    /// equipment user holds RocketFlower equipment the counter is set again; the hit reaction
    /// "強制走り開始" starts when it was not already running.
    pub(super) fn query_equipment_force_run(&mut self) {
        if !self.equipment_force_run() {
            return;
        }
        if !self.force_run.is_active() {
            self.events.push(Fx::ForceRunStart);
        }
        self.force_run.setup(EQUIPMENT_FRAMES, EQUIPMENT_SPEED);
    }

    /// The end of `PlayerActorHakoniwa::movement` (0x41f088).
    pub(super) fn update_force_run_counter(&mut self) {
        self.force_run.update();
    }
}

pub(super) fn self_check() {
    checks::run();
}

mod checks {
    use super::*;

    pub(super) fn run() {
        the_counter_converges_and_keeps_its_speed();
        equipment_refreshes_the_counter_and_starts_the_reaction_once();
    }

    #[cfg(test)]
    #[test]
    fn port_checks() {
        run();
    }

    fn the_counter_converges_and_keeps_its_speed() {
        let mut counter = ForceRun::default();
        assert!(!counter.is_active());
        counter.setup(2, 38.);
        assert!(counter.is_active());
        counter.update();
        assert_eq!((counter.counter, counter.speed), (1, 38.));
        counter.update();
        counter.update();
        assert_eq!((counter.counter, counter.speed), (0, 38.));
        assert!(!counter.is_active());
        // A negative counter (a setter this port does not use) converges up to zero.
        counter.setup(-2, 12.);
        counter.update();
        assert_eq!(counter.counter, -1);
    }

    fn equipment_refreshes_the_counter_and_starts_the_reaction_once() {
        let mut player = Player::default();
        player.query_equipment_force_run();
        assert!(!player.force_run.is_active(), "no equipment, no forced run");
        player.rocket.dash = true;
        player.query_equipment_force_run();
        assert_eq!(player.force_run.counter, EQUIPMENT_FRAMES);
        assert_eq!(player.force_run.speed, EQUIPMENT_SPEED);
        assert_eq!(player.events.len(), 1);
        // The late decrement takes the window to 1; the next query sets it again and the
        // reaction does not repeat while the counter is positive.
        player.update_force_run_counter();
        assert_eq!(player.force_run.counter, EQUIPMENT_FRAMES - 1);
        player.query_equipment_force_run();
        assert_eq!(player.force_run.counter, EQUIPMENT_FRAMES);
        assert_eq!(player.events.len(), 1);
        // Without the equipment the window runs out in two updates.
        player.rocket.dash = false;
        player.update_force_run_counter();
        player.update_force_run_counter();
        assert!(!player.force_run.is_active());
    }
}
