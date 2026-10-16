use super::*;
use bevy::prelude::Vec3;

pub(super) fn run() {
    jump_combo_counters();
    reading_back();
    contact_and_triggers();
}

fn player() -> Player {
    Player::spawn_at(Vec3::ZERO, Vec3::Z)
}
fn item(cell: &str, key: &str) -> String {
    cell.split('|')
        .find_map(|i| i.strip_prefix(&format!("{key}=")))
        .unwrap_or("<missing>")
        .to_string()
}

#[cfg_attr(test, test)]
fn jump_combo_counters() {
    let mut p = player();
    let cell = counters_cell(&p);
    assert_eq!(item(&cell, "cjump"), "0/0");
    assert_eq!(item(&cell, "capjump"), "1");
    assert_eq!(item(&cell, "prejump"), "0");
    assert_eq!(item(&cell, "quickturn"), "0");
    // Landed from a first jump (count 0 in the rewrite): the game counts one,
    // with a timer that starts at one on the landing frame.
    p.set_counter("jump_count", 0);
    p.set_counter("jump_timer", 0);
    assert_eq!(item(&counters_cell(&p), "cjump"), "1/1");
    p.set_counter("jump_count", 1);
    p.set_counter("jump_timer", 4);
    assert_eq!(item(&counters_cell(&p), "cjump"), "2/5");
    // The window closes when the timer reaches its length.
    p.set_counter("jump_timer", 9);
    assert_eq!(item(&counters_cell(&p), "cjump"), "0/0");
    // Internal counters keep their own names after the game's.
    p.set_counter("coyote", 2);
    assert_eq!(item(&counters_cell(&p), "coyote"), "2");
    assert_eq!(item(&counters_cell(&p), "pre_jump"), "<missing>");
}

#[cfg_attr(test, test)]
fn reading_back() {
    let back = counters_from_game(
        "cjump=2/5|clong=0/10|quickturn=7|capjump=0|prejump=3|upperpunch=2147483647",
    );
    let has = |name: &str, value: i64| back.contains(&(name.to_string(), value));
    assert!(has("jump_count", 1) && has("jump_timer", 4));
    assert!(has("long_jump_timer", 15));
    assert!(has("quick_turn", 7) && has("cap_jump_possible", 0) && has("pre_jump", 3));
    // A closed window reads back as a timer past the window.
    assert!(counters_from_game("cjump=0/10").contains(&("jump_timer".to_string(), 10)));
    // Numbers that are not integers or pairs are skipped.
    assert!(counters_from_game("forcerun=0/0.000000|junk").len() <= 1);
}

#[cfg_attr(test, test)]
fn contact_and_triggers() {
    let mut p = player();
    assert_eq!(
        collision_cell(&p),
        "ground=1;wall=0;gn=0.000000/1.000000/0.000000"
    );
    p.grounded = false;
    assert_eq!(collision_cell(&p), "ground=0;wall=0");
    p.wall = Vec3::X;
    assert_eq!(
        collision_cell(&p),
        "ground=0;wall=1;wn=1.000000/0.000000/0.000000"
    );
    assert_eq!(triggers_cell(&[]), "");
    assert_eq!(
        triggers_cell(&["Jump".to_string(), "Land".to_string()]),
        "fx=Jump+Land"
    );
}
