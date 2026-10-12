//! Scripted graphical checks feed the same Orders as real devices.
use crate::input::Orders;
use bevy::prelude::*;

pub fn apply(
    time: &Time<Real>,
    menu_open: bool,
    orders: &mut Orders,
    smoke_phase: &mut u8,
    smoke_last: &mut f32,
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
        let edge = phase != *smoke_phase;
        *smoke_phase = phase;
        let script = std::env::var("PJV_SCRIPT").unwrap_or_default();
        if script == "lava" {
            // From PJV_SPAWN: run +X onto the lava plate and keep going, then let go.
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
                crouch_pressed: *smoke_last < 0.4 && now >= 0.4,
                ..default()
            };
            *smoke_last = now;
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
                crouch_pressed: *smoke_last < 1.0 && now >= 1.0,
                ..default()
            };
            *smoke_last = now;
            return;
        }
        if script == "spin" {
            // Circle the stick into a ground spin, then throw while still circling.
            let now = time.elapsed_secs() - delay;
            *orders = Orders {
                direction: if started && now < 2.6 {
                    Vec2::new((now * 14.).cos(), (now * 14.).sin())
                } else {
                    Vec2::ZERO
                },
                throw_pressed: *smoke_last < 1.2 && now >= 1.2,
                ..default()
            };
            *smoke_last = now;
            return;
        }
        if script == "dive" || script == "diveroll" {
            // Run, jump, pound, dive forward; "diveroll" keeps crouch held to land rolling.
            let now = time.elapsed_secs() - delay;
            let crossed = |at: f32| *smoke_last < at && now >= at;
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
            *smoke_last = now;
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
