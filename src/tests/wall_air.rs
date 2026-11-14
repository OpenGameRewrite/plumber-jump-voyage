use super::wall_air::{WallHistory, angle_on_plane, converge};
use super::*;
use crate::geometry::TriangleMesh;

pub(super) fn run() {
    helpers();
    wall_history();
    ledge_fall_waits_for_the_judge();
    a_wall_slide_on_a_stage_holds_one_more_step_after_the_wall_is_gone();
    fall_distance_counts_from_the_terminal_speed();
    lockstep_wall_keep_completion();
}

#[cfg_attr(test, test)]
fn lockstep_wall_keep_completion() {
    // exeKeep chooses Slide after isAnimEnd (0x476210..0x476228).
    // The completed Keep clip is still visible in that row.
    for (keep, slide) in [
        ("WallKeep", "WallSlideR"),
        ("WallKeepReverse", "WallSlideL"),
    ] {
        let mut player = Player::default();
        player.inject(&Snapshot {
            mode: Some(Move::WallSlide),
            state_frames: Some(27),
            sub_steps: Some(27),
            anim: Some((keep, 26.)),
            ..Default::default()
        });
        assert_eq!(player.phase_clip(), slide);
    }
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

/// A wall quad in the plane z = `z` (cm), both windings so either side is a wall.
fn wall(x: [f32; 2], y: [f32; 2], z: f32) -> Vec<[[Vec3; 3]; 2]> {
    let corner = |x, y| Vec3::new(x, y, z);
    let front = [
        [corner(x[0], y[0]), corner(x[1], y[0]), corner(x[0], y[1])],
        [corner(x[1], y[1]), corner(x[0], y[1]), corner(x[1], y[0])],
    ];
    let back = front.map(|[a, b, c]| [a, c, b]);
    vec![front, back]
}

/// `PlayerStateNormalWallSlide::followNormal` reads the contact the last move left, so on a stage
/// the slide holds for the step in which the wall is gone and ends in the next one (the game's
/// force-waterfall-airpass row 126 is still WallAir/Slide with no wall contact, row 127 Fall).
#[cfg_attr(test, test)]
fn a_wall_slide_on_a_stage_holds_one_more_step_after_the_wall_is_gone() {
    let quads: Vec<_> = wall([-1000., 1000.], [-3000., 400.], -200.)
        .into_iter()
        .flatten()
        .collect();
    let mesh = TriangleMesh::from_corners(&quads);
    let scene = Course {
        mesh: Some(Arc::new(mesh)),
        ..default()
    };
    let mut p = Player {
        pos: Vec3::new(0., 3., -1.2),
        grounded: false,
        v: Vec3::new(0., -2., -8.),
        mode: Move::Fall,
        facing: Vec3::NEG_Z,
        ..default()
    };
    let mut slid = false;
    let mut lost_at = None;
    for step in 0..2000 {
        p.step(&Orders::default(), &scene);
        slid |= p.mode == Move::WallSlide;
        if slid && lost_at.is_none() && p.touched.wall_contact.is_none() {
            lost_at = Some(step);
            assert_eq!(
                p.mode,
                Move::WallSlide,
                "the slide holds in the step the wall is gone"
            );
        } else if lost_at.is_some() {
            assert_eq!(p.mode, Move::Fall, "and ends in the next one");
            return;
        }
    }
    panic!("the slide never lost the wall (slid: {slid})");
}
