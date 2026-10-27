//! Scripted graphical checks feed the same Orders as real devices.
use crate::input::Orders;
use crate::movement::{Move, Player};
use bevy::prelude::*;

/// Where the toy course's hot plate stands (`world.rs`): centre x and half width.
const HOT_PLATE_X: f32 = 4.;
const HOT_PLATE_HALF_X: f32 = 1.5;

/// Progress of the two-burn walk, which follows the player's state rather
/// than the clock: a slow render stretches a burn in real time, so timed
/// legs would reverse the stick before the first burn is over.
#[derive(Default)]
pub struct LavaTwice {
    burns: u8,
    was_burning: bool,
    /// Real seconds spent standing after the last burn.
    settled: f32,
    /// Which side of the plate the last burn left Mario on (+1 or -1 along X).
    side: f32,
    walked_out: bool,
}

impl LavaTwice {
    /// The stick for this frame: step onto the plate, stand through the burn,
    /// walk away from the plate once Mario waits on safe ground, then back on.
    fn direction(&mut self, player: &Player, delta: f32, now: f32) -> Vec2 {
        let burning = matches!(
            player.mode,
            Move::BurnJump | Move::BurnBounce | Move::FireRun | Move::FireFall
        );
        if burning != self.was_burning {
            if burning {
                self.burns += 1;
                self.settled = 0.;
                self.walked_out = false;
            }
            info!(
                "smoke: burn {} {} at x={:.2} z={:.2}",
                self.burns,
                if burning { "starts" } else { "ends" },
                player.pos.x,
                player.pos.z
            );
            self.was_burning = burning;
        }
        if burning || now < 0.1 {
            return Vec2::ZERO;
        }
        match self.burns {
            0 => Vec2::X * 0.6,
            1 => {
                // The burn is over; give the ordinary Wait time to show.
                if player.grounded && player.mode == Move::Idle {
                    self.settled += delta;
                }
                if self.settled < 1.5 {
                    return Vec2::ZERO;
                }
                let side = (player.pos.x - HOT_PLATE_X).signum();
                if !self.walked_out {
                    self.side = side;
                    // Away from the plate until well clear of its edge.
                    if (player.pos.x - HOT_PLATE_X).abs() > HOT_PLATE_HALF_X + 1.5 {
                        self.walked_out = true;
                    }
                    return Vec2::X * side * 0.6;
                }
                // Back onto the plate for the second burn.
                Vec2::X * -self.side * 0.6
            }
            _ => Vec2::ZERO,
        }
    }
}

/// What the scripted checks remember between frames.
#[derive(Default)]
pub struct SmokeState {
    phase: u8,
    last: f32,
    lava_twice: LavaTwice,
}

pub fn apply(
    time: &Time<Real>,
    menu_open: bool,
    orders: &mut Orders,
    smoke: &mut SmokeState,
    player: Option<&Player>,
) {
    // Optional graphical check drives the same orders through the real controller.
    if !menu_open && std::env::var_os("PJV_SMOKE").is_some() {
        // PJV_SMOKE=<seconds> delays the script, e.g. until shaders are ready.
        let delay: f32 = std::env::var("PJV_SMOKE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.);
        let started = time.elapsed_secs() >= delay;
        let phase = ((time.elapsed_secs() - delay).max(0.) / 0.7) as u8;
        let edge = phase != smoke.phase;
        smoke.phase = phase;
        let script = std::env::var("PJV_SCRIPT").unwrap_or_default();
        if script == "needle" {
            let now = time.elapsed_secs() - delay;
            *orders = Orders {
                reset: started && now < 0.04,
                direction: if (0.1..1.0).contains(&now) {
                    Vec2::X * 0.6
                } else {
                    Vec2::ZERO
                },
                ..default()
            };
            return;
        }
        if script == "sand" || script == "sand-die" {
            // From PJV_SPAWN east of the sand patch: walk onto it and stop to sink. `sand` jumps
            // out while the body is still shallow; `sand-die` stays until the sink height.
            let now = time.elapsed_secs() - delay;
            let jumps = script == "sand";
            *orders = Orders {
                reset: started && now < 0.04,
                direction: if (0.1..0.7).contains(&now) {
                    Vec2::NEG_X * 0.7
                } else {
                    Vec2::ZERO
                },
                jump: jumps && (1.2..1.23).contains(&now),
                jump_held: jumps && (1.2..1.8).contains(&now),
                ..default()
            };
            return;
        }
        if script == "lava-twice"
            && let Some(player) = player
        {
            // From PJV_SPAWN west of the hot plate: burn, wait, walk out to the
            // safe side and back for a second burn, without resetting HP or stain.
            let now = time.elapsed_secs() - delay;
            *orders = Orders {
                reset: started && now < 0.04,
                direction: if started {
                    smoke.lava_twice.direction(player, time.delta_secs(), now)
                } else {
                    Vec2::ZERO
                },
                ..default()
            };
            return;
        }
        if script == "lava" {
            // From PJV_SPAWN: cross the hot plate.
            let now = time.elapsed_secs() - delay;
            *orders = Orders {
                reset: started && now < 0.04,
                direction: if (0.1..3.0).contains(&now) {
                    Vec2::X * 0.6
                } else {
                    Vec2::ZERO
                },
                ..default()
            };
            return;
        }
        if script == "fall" {
            // From PJV_SPAWN near the course's edge: run +X off it and let the death area,
            // the wipe and the respawn play out.
            let now = time.elapsed_secs() - delay;
            *orders = Orders {
                reset: started && now < 0.04,
                direction: if (0.1..3.0).contains(&now) {
                    Vec2::X
                } else {
                    Vec2::ZERO
                },
                ..default()
            };
            return;
        }
        if script == "soot" {
            // From PJV_SPAWN: run +X across the lava plate, stand while the
            // soot smoulders, then walk on into the puddle to wash it off.
            let now = time.elapsed_secs() - delay;
            *orders = Orders {
                reset: started && now < 0.04,
                direction: if (0.1..1.5).contains(&now) || (8.0..9.4).contains(&now) {
                    Vec2::X * 0.6
                } else {
                    Vec2::ZERO
                },
                ..default()
            };
            return;
        }
        if script == "slide" {
            // From PJV_SPAWN on a slope: settle, then hold crouch.
            let now = time.elapsed_secs() - delay;
            *orders = Orders {
                reset: started && now < 0.04,
                crouch: now >= 0.4,
                crouch_pressed: smoke.last < 0.4 && now >= 0.4,
                ..default()
            };
            smoke.last = now;
            return;
        }
        if script == "crouchwalk" {
            // Walk toward the camera, then crouch and keep holding it.
            let now = time.elapsed_secs() - delay;
            *orders = Orders {
                direction: if started && now < 1.6 {
                    Vec2::NEG_X * 0.4
                } else {
                    Vec2::ZERO
                },
                crouch: now >= 1.0,
                crouch_pressed: smoke.last < 1.0 && now >= 1.0,
                ..default()
            };
            smoke.last = now;
            return;
        }
        if script == "spin" || script == "spinjump" {
            // Circle the stick into a ground spin (it takes about a second:
            // the detector wants nearly two turns). "spin" throws while still
            // circling; "spinjump" jumps during the spin and drops from it.
            let now = time.elapsed_secs() - delay;
            let circling = started && now < 2.6;
            let c = Vec2::new((now * 14.).cos(), (now * 14.).sin());
            let crossed = |at: f32| smoke.last < at && now >= at;
            let jumping = script == "spinjump";
            *orders = Orders {
                direction: if circling { c } else { Vec2::ZERO },
                // The stick as the controller reports it: y down the screen.
                stick: if circling {
                    Vec2::new(c.x, -c.y)
                } else {
                    Vec2::ZERO
                },
                throw_pressed: !jumping && crossed(1.2),
                jump: jumping && crossed(1.5),
                jump_held: jumping && (1.5..1.7).contains(&now),
                crouch: jumping && now >= 2.1,
                crouch_pressed: jumping && crossed(2.1),
                ..default()
            };
            smoke.last = now;
            return;
        }
        if script == "dive" || script == "diveroll" {
            // Run, jump, pound, dive forward; "diveroll" keeps crouch held to land rolling.
            let now = time.elapsed_secs() - delay;
            let crossed = |at: f32| smoke.last < at && now >= at;
            *orders = Orders {
                direction: if started && now < 1.1 {
                    Vec2::Y
                } else {
                    Vec2::ZERO
                },
                jump: crossed(0.7),
                jump_held: (0.7..0.9).contains(&now),
                crouch_pressed: crossed(1.0),
                crouch: now >= 1.0 && (script == "diveroll" || now < 1.1),
                throw_pressed: crossed(1.08),
                ..default()
            };
            smoke.last = now;
            return;
        }
        if script == "walk" {
            // A slow walk, then a full run, for close inspection.
            *orders = Orders {
                direction: match phase {
                    _ if !started => Vec2::ZERO,
                    0..=3 => Vec2::Y * 0.3,
                    _ => Vec2::Y,
                },
                ..default()
            };
            return;
        }
        if script == "pole" || script == "ledge" {
            // From PJV_SPAWN: catch a pole and climb then circle it, or catch an
            // edge (forward is -Z), hang, then climb up.
            let pole = script == "pole";
            *orders = Orders {
                reset: started && time.elapsed_secs() - delay < 0.04,
                direction: match phase {
                    _ if !started => Vec2::ZERO,
                    0 if !pole && time.elapsed_secs() - delay < 0.25 => Vec2::NEG_Y,
                    1..=3 if pole => Vec2::NEG_Y,
                    4 | 5 if pole => Vec2::X,
                    4 if !pole => Vec2::NEG_Y,
                    _ => Vec2::ZERO,
                },
                ..default()
            };
            return;
        }
        *orders = Orders {
            // Across open ground: run, jump, throw mid-run, then roll back.
            direction: match phase {
                _ if !started => Vec2::ZERO,
                0..=2 => Vec2::X,
                3 | 4 => Vec2::NEG_X,
                _ => Vec2::ZERO,
            },
            jump: phase == 1 && edge,
            jump_held: phase == 1,
            throw_pressed: matches!(phase, 2 | 3 | 6) && edge,
            throw_held: matches!(phase, 2..=4 | 6..=9),
            crouch: phase == 3,
            reset: phase == 5 && edge,
            ..default()
        };
    }
}
