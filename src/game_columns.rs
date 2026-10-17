//! The `counters`, `collision` and `triggers` trace cells in the real game's
//! vocabulary, so the comparison can match them key by key.
//!
//! The game logs `key=value` items (`|` between counters, `;` between the
//! others). The rewrite writes the keys it has an equivalent for under the
//! game's names, then its own internal counters under its own names; the
//! comparison treats a key only one side has as not comparable.
//!
//! Mappings (the game's key, then what the rewrite writes):
//! * `cjump=count/timer`: the game counts a jump when it lands (the count after
//!   the jump just landed is one, then two, then wraps to zero) and times the
//!   window for the next jump (the timer runs to the window and clears). A jump
//!   that continues the combo flies with the count and the timer as they were
//!   at take-off. The rewrite keeps the same two numbers (`jump_count`,
//!   `jump_timer`) with the same rules, so the cell is written as they are.
//! * `clong=count/timer`: the count goes up as a long jump starts and the timer
//!   waits at zero until the landing, then runs a 15-frame window; the rewrite
//!   keeps `long_jump_count` and `long_jump_timer` the same way.
//! * `quickturn` is not written: the game kept it at zero through a full reverse
//!   and a side somersault, so the rewrite's own `quick_turn` window (set when a
//!   skid starts) is not the same counter.
//! * `capjump=0|1`: whether a cap bounce is still possible (`cap_jump_possible`).
//! * `prejump=frames`: the buffered jump press (`pre_jump`).
//! * `collision`: `ground`, `wall`, the ground normal `gn` when standing and the
//!   wall normal `wn` on a wall. No ceiling flag and no material names exist in
//!   the rewrite, so `ceiling` and `gmat` are left out.
//! * `triggers`: the game's keys are raw flag words of its player trigger
//!   object, and the rewrite has no such words. It writes `fx=` (its own
//!   movement event names) which the game has no counterpart for.
//!
//! The game keys without a rewrite counterpart (`upperpunch`, `forcerun`,
//! `limitheight`, `wallhist`) are not written.
use crate::movement::Player;

fn counter(counters: &[(&'static str, i64)], name: &str) -> i64 {
    counters
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(0, |(_, v)| *v)
}
/// Internal counters the rewrite keeps under its own names (the ones already
/// folded into a game key are left out).
const FOLDED: [&str; 2] = ["pre_jump", "cap_jump_possible"];

/// The game's `cjump` pair: the combo's count and timer exactly as the
/// original's `PlayerContinuousJump` keeps them (the count goes up when a jump
/// ends on the ground, the timer runs while standing or running, and any other
/// state clears both).
fn combo_jump(get: impl Fn(&str) -> i64) -> String {
    format!("{}/{}", get("jump_count"), get("jump_timer"))
}
/// The game's `clong` pair, from `PlayerContinuousLongJump`: the count goes up
/// as the long jump starts and its timer waits at zero until the landing.
fn combo_long_jump(get: impl Fn(&str) -> i64) -> String {
    format!("{}/{}", get("long_jump_count"), get("long_jump_timer"))
}
pub fn counters_cell(p: &Player) -> String {
    let counters = p.counters();
    let get = |name: &str| counter(&counters, name);
    let mut items = vec![
        format!("cjump={}", combo_jump(get)),
        format!("clong={}", combo_long_jump(get)),
        format!("capjump={}", get("cap_jump_possible")),
        format!("prejump={}", get("pre_jump")),
    ];
    items.extend(
        counters
            .iter()
            .filter(|(name, _)| !FOLDED.contains(name) && !name.starts_with("pre_"))
            .map(|(name, value)| format!("{name}={value}")),
    );
    items.join("|")
}

/// The rewrite's counters (by `Player::counters` names) that a game
/// `counters` cell stands for; keys with no equivalent are left out. Items
/// already in the rewrite's own names pass through, so a trace of the rewrite
/// can be fed back.
pub fn counters_from_game(cell: &str) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    for item in cell.split('|') {
        let Some((key, value)) = item.split_once('=') else {
            continue;
        };
        let number = |text: &str| text.parse::<f64>().ok().map(|v| v as i64);
        // `count/timer` pairs: the first and the second number.
        let (first, second) = match value.split_once('/') {
            Some((a, b)) => (number(a), number(b)),
            None => (number(value), None),
        };
        let Some(first) = first else {
            continue;
        };
        match key {
            "prejump" => out.push(("pre_jump".into(), first)),
            "capjump" => out.push(("cap_jump_possible".into(), first)),
            "cjump" => {
                out.push(("jump_count".into(), first));
                out.push(("jump_timer".into(), second.unwrap_or(0)));
            }
            "clong" => {
                out.push(("long_jump_count".into(), first));
                out.push(("long_jump_timer".into(), second.unwrap_or(0)));
            }
            _ => out.push((key.to_string(), first)),
        }
    }
    out
}

/// The contact flags, in the game's `collision` format.
pub fn collision_cell(p: &Player) -> String {
    let mut items = vec![
        format!("ground={}", p.floor_normal().is_some() as i32),
        format!("wall={}", (p.wall != bevy::prelude::Vec3::ZERO) as i32),
    ];
    if let Some(n) = p.floor_normal() {
        items.push(format!("gn={}", vector(n)));
    }
    if p.wall != bevy::prelude::Vec3::ZERO {
        items.push(format!("wn={}", vector(p.wall)));
    }
    items.join(";")
}
/// The rewrite's own movement events, in the `triggers` format.
pub fn triggers_cell(events: &[String]) -> String {
    if events.is_empty() {
        String::new()
    } else {
        format!("fx={}", events.join("+"))
    }
}
fn vector(v: bevy::prelude::Vec3) -> String {
    use crate::trace::num;
    format!("{}/{}/{}", num(v.x), num(v.y), num(v.z))
}

#[path = "tests/game_columns.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
