//! The `counters`, `collision` and `triggers` trace cells in the real game's
//! vocabulary, so the comparison can match them key by key.
//!
//! The game logs `key=value` items (`|` between counters, `;` between the
//! others). The rewrite writes the keys it has an equivalent for under the
//! game's names, then its own internal counters under its own names; the
//! comparison treats a key only one side has as not comparable.
//!
//! Mappings (the game's key, then what the rewrite writes):
//! * `cjump=count/timer`: the game counts a landing from a jump (count after
//!   the jump just landed, one to two) and times the window for the next jump
//!   (timer reaches the window and clears). The rewrite writes
//!   `(jump_count + 1) % 3` and `jump_timer + 1` while it stands on the ground
//!   inside the window, else `0/0`. The count is stale after a landing from a
//!   plain fall, which the rewrite does not tell apart.
//! * `clong=count/timer`: the same shape for the long jump window; provisional,
//!   no long jump has been recorded in the real game yet.
//! * `quickturn=frames`: the quick-turn jump frames left (`quick_turn`).
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
use crate::movement::{CONTINUOUS_JUMP_FRAMES, CONTINUOUS_LONG_JUMP_FRAMES, Player};

fn counter(counters: &[(&'static str, i64)], name: &str) -> i64 {
    counters
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(0, |(_, v)| *v)
}
/// A game-style `count/timer` pair: the timer only means something while the
/// count is above zero.
fn window(open: bool, count: i64, timer: i64) -> String {
    if open && count > 0 {
        format!("{count}/{timer}")
    } else {
        "0/0".into()
    }
}

/// Internal counters the rewrite keeps under its own names (the ones already
/// folded into a game key are left out).
const FOLDED: [&str; 3] = ["pre_jump", "quick_turn", "cap_jump_possible"];

pub fn counters_cell(p: &Player) -> String {
    let counters = p.counters();
    let get = |name| counter(&counters, name);
    let mut items = vec![
        format!(
            "cjump={}",
            window(
                p.grounded && get("jump_timer") + 1 < CONTINUOUS_JUMP_FRAMES as i64,
                (get("jump_count") + 1) % 3,
                get("jump_timer") + 1
            )
        ),
        format!(
            "clong={}",
            window(
                p.grounded && get("long_jump_timer") + 1 < CONTINUOUS_LONG_JUMP_FRAMES as i64,
                get("long_jump_count"),
                get("long_jump_timer") + 1
            )
        ),
        format!("quickturn={}", get("quick_turn")),
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
            "quickturn" => out.push(("quick_turn".into(), first)),
            "capjump" => out.push(("cap_jump_possible".into(), first)),
            "cjump" if first > 0 => {
                out.push(("jump_count".into(), (first + 2) % 3));
                out.push(("jump_timer".into(), second.unwrap_or(1) - 1));
            }
            "cjump" => out.push(("jump_timer".into(), CONTINUOUS_JUMP_FRAMES as i64)),
            "clong" if first > 0 => {
                out.push(("long_jump_count".into(), first));
                out.push(("long_jump_timer".into(), second.unwrap_or(1) - 1));
            }
            "clong" => out.push(("long_jump_timer".into(), CONTINUOUS_LONG_JUMP_FRAMES as i64)),
            _ => out.push((key.to_string(), first)),
        }
    }
    out
}

/// The contact flags, in the game's `collision` format.
pub fn collision_cell(p: &Player) -> String {
    let mut items = vec![
        format!("ground={}", p.grounded as i32),
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
