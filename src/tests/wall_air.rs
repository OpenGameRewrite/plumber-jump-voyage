use super::wall_air::{WallHistory, angle_on_plane, converge};
use super::*;
use crate::geometry::TriangleMesh;

pub(super) fn run() {
    helpers();
    wall_history();
    ledge_fall_waits_for_the_judge();
    fall_distance_counts_from_the_terminal_speed();
}

#[cfg_attr(test, test)]
fn helpers() {
    assert_eq!(converge(0., 0.78, 0.1), 0.1);
    assert_eq!(converge(0.75, 0.78, 0.1), 0.78);
    assert_eq!(converge(0.2, -0.3, 0.1), 0.1);
    // Seen along the vertical, a quarter turn of the horizontal normal.
    let angle = angle_on_plane(Vec3::X, Vec3::Z, Vec3::NEG_Y);
    assert!((angle - 90.).abs() < 1e-3, "{angle}");
    assert!(angle_on_plane(Vec3::X, Vec3::X, Vec3::Y) < 1e-3);
}

/// The history keeps the player off the wall he just left or jumped from.
#[cfg_attr(test, test)]
fn wall_history() {
    let wall = Vec3::Z;
    // Nothing recorded: any wall.
    assert!(WallHistory::default().enables(Vec3::ZERO, wall));
    // After a slide ended, the player has to fall by his own height first.
    let left = WallHistory {
        leave: Some((Vec3::new(0., 500., 0.), wall)),
        ..default()
    };
    assert!(!left.enables(Vec3::new(0., 400., 0.), wall));
    assert!(left.enables(Vec3::new(0., 330., 0.), wall));
    // After a jump off a wall, the same wall is out until the player has
    // fallen below the jump's height, further below the further he is to the
    // side of it.
    let jumped = WallHistory {
        jump: Some((Vec3::new(0., 500., 0.), wall)),
        ..default()
    };
    assert!(!jumped.enables(Vec3::new(0., 500., 100.), wall));
    assert!(jumped.enables(Vec3::new(0., -500., 100.), wall));
    assert!(!jumped.enables(Vec3::new(900., 500., 0.), wall));
    assert!(!jumped.enables(Vec3::new(900., -300., 0.), wall));
    assert!(jumped.enables(Vec3::new(900., -500., 0.), wall));
    // A wall facing the other way is ruled out only far away.
    assert!(jumped.enables(Vec3::new(0., 500., 600.), -wall));
    assert!(!jumped.enables(Vec3::new(2000., 600., 0.), -wall));
}

/// A rectangle of floor at height 0, in cm.
fn tile(x: [f32; 2], z: [f32; 2]) -> [[Vec3; 3]; 2] {
    let corner = |x, z| Vec3::new(x, 0., z);
    [
        [corner(x[0], z[0]), corner(x[0], z[1]), corner(x[1], z[0])],
        [corner(x[1], z[1]), corner(x[1], z[0]), corner(x[0], z[1])],
    ]
}

/// Running off a ledge: the ground nerve goes on for five steps without ground
/// (`PlayerJudgeSpeedCheckFall` wants more than four) and the fall is chosen
/// in the fifth; a jump pressed in those steps still takes off.
#[cfg_attr(test, test)]
fn ledge_fall_waits_for_the_judge() {
    let mesh = TriangleMesh::from_corners(&tile([-1000., 1000.], [-300., 3000.]));
    let scene = Course {
        mesh: Some(Arc::new(mesh)),
        ..default()
    };
    let run = Orders {
        direction: Vec2::NEG_Y,
        ..default()
    };
    let start = || Player::spawn_at(Vec3::ZERO, Vec3::NEG_Z);
    let mut p = start();
    let mut left = None;
    for step in 0..200 {
        p.step(&run, &scene);
        if left.is_none() && !p.grounded {
            left = Some(step);
        }
        if p.mode == Move::Fall {
            assert_eq!(Some(step - 5), left, "the fall is chosen in the fifth step");
            break;
        }
    }
    assert_eq!(p.mode, Move::Fall);
    // The same run with a jump pressed in the air still jumps.
    let mut p = start();
    let mut jumped = false;
    for _ in 0..200 {
        let jump = !p.grounded;
        p.step(
            &Orders {
                jump,
                jump_held: jump,
                ..run.clone()
            },
            &scene,
        );
        if p.mode == Move::Jump && jump {
            jumped = true;
            break;
        }
    }
    assert!(jumped, "a jump pressed just after the ledge takes off");
}

/// The long-fall judge measures the drop since the first step at the terminal
/// speed, and starts over on the ground or with a slower fall.
#[cfg_attr(test, test)]
fn fall_distance_counts_from_the_terminal_speed() {
    let mut fall = FallDistance::default();
    // Speeding up does not count.
    fall.update(1000., -20., false, false);
    assert_eq!(fall.distance, 0.);
    // The step that first reaches the terminal speed starts the count.
    fall.update(965., -35., false, false);
    assert_eq!(fall.distance, 0.);
    fall.update(930., -35., false, false);
    assert_eq!(fall.distance, 35.);
    // The step that lands still counts, the next one on the ground resets.
    fall.update(895., -35., true, true);
    assert_eq!(fall.distance, 70.);
    fall.update(895., -3., true, false);
    assert_eq!(fall, FallDistance::default());
}
