use super::ledge::CatchNerve;
use super::*;
use crate::geometry::TriangleMesh;

pub(super) fn run() {
    easing_and_poses();
    catching_a_ledge();
    letting_go_of_a_ledge();
    climbing_fast_and_the_climb_jump();
    sliding_along_a_straight_ledge();
}

/// Two triangles of a rectangle (cm) facing `outward`.
fn quad(corners: [Vec3; 4], outward: Vec3) -> [[Vec3; 3]; 2] {
    let [a, b, c, d] = corners;
    if (b - a).cross(c - a).dot(outward) >= 0. {
        [[a, b, c], [a, c, d]]
    } else {
        [[a, d, c], [a, c, b]]
    }
}

/// A platform 3 m high filling x < 0, with a level floor around it: the wall
/// faces +x, the top is level at y = 300.
fn platform() -> Course {
    let far = 5000.;
    let mut faces: Vec<[Vec3; 3]> = Vec::new();
    let mut add = |corners: [Vec3; 4], outward: Vec3| faces.extend(quad(corners, outward));
    add(
        [
            Vec3::new(-far, 0., -far),
            Vec3::new(far, 0., -far),
            Vec3::new(far, 0., far),
            Vec3::new(-far, 0., far),
        ],
        Vec3::Y,
    );
    add(
        [
            Vec3::new(0., 0., -far),
            Vec3::new(0., 300., -far),
            Vec3::new(0., 300., far),
            Vec3::new(0., 0., far),
        ],
        Vec3::X,
    );
    add(
        [
            Vec3::new(-far, 300., -far),
            Vec3::new(0., 300., -far),
            Vec3::new(0., 300., far),
            Vec3::new(-far, 300., far),
        ],
        Vec3::Y,
    );
    Course {
        mesh: Some(Arc::new(TriangleMesh::from_corners(&faces))),
        ..default()
    }
}

fn toward_wall() -> Orders {
    Orders {
        direction: Vec2::new(-1., 0.),
        stick: Vec2::new(-1., 0.),
        ..default()
    }
}

/// A body that falls against the platform's wall just under its edge.
fn falling_at_the_wall() -> Player {
    Player {
        pos: Vec3::new(0.56, 2.2, 0.),
        grounded: false,
        mode: Move::Fall,
        v: Vec3::new(-4., -3., 0.),
        facing: Vec3::NEG_X,
        ..default()
    }
}

/// Steps until the hang starts; returns the player at the step that chose it.
fn caught(scene: &Course) -> Player {
    let mut p = falling_at_the_wall();
    for _ in 0..30 {
        p.step(&toward_wall(), scene);
        if p.mode == Move::Ledge {
            return p;
        }
    }
    panic!("the ledge was not caught: {:?} {:?}", p.mode, p.pos);
}

fn hang(p: &mut Player, scene: &Course, steps: usize, o: &Orders) {
    for _ in 0..steps {
        p.step(o, scene);
    }
}

#[cfg_attr(test, test)]
fn easing_and_poses() {
    // `al::easeOut`: 1.5 t - 0.5 t^3, flat at the end.
    let at = |t: f32| (-0.5 * t * t + 1.5) * t;
    assert_eq!(at(0.), 0.);
    assert_eq!(at(1.), 1.);
    assert!((at(0.1) - 0.1495).abs() < 1e-4);
    assert_eq!(CatchNerve::MoveLeft.name(), "WallCatch/MoveLeft");
}

/// The judge puts the body on the ledge point at once: 5 inside the wall's
/// face, on the top, facing the wall, with the catch clip started.
#[cfg_attr(test, test)]
fn catching_a_ledge() {
    let scene = platform();
    let p = caught(&scene);
    assert_eq!(p.nerve_name().name, "WallCatch/Start");
    assert_eq!(p.anim.name, "WallCatchStart");
    assert!((p.pos.y - 3.).abs() < 0.01, "on the top: {:?}", p.pos);
    assert!(
        p.pos.x < -0.03 && p.pos.x > -0.07,
        "5 inside the face: {:?}",
        p.pos
    );
    assert!(p.facing.dot(Vec3::NEG_X) > 0.999, "faces the wall");
    // The catch clip plays through (55 steps), then the wait.
    let mut p = p;
    hang(&mut p, &scene, 70, &default());
    assert_eq!(p.nerve_name().name, "WallCatch/Wait");
    assert!(
        p.pos.y > 2.99 && p.pos.y < 3.01,
        "the hang holds: {:?}",
        p.pos
    );
}

/// The stick away from the wall lets go: the body moves out and down and
/// rises at 12; the fall ignores the stick for 8 steps.
#[cfg_attr(test, test)]
fn letting_go_of_a_ledge() {
    let scene = platform();
    let mut p = caught(&scene);
    hang(&mut p, &scene, 70, &default());
    let from = p.pos;
    let back = Orders {
        direction: Vec2::new(1., 0.),
        stick: Vec2::new(1., 0.),
        ..default()
    };
    p.step(&back, &scene);
    assert_eq!(p.mode, Move::Fall);
    assert_eq!(p.anim.name, "Fall");
    assert!(
        (p.pos.x - from.x - 0.65).abs() < 0.02 && (from.y - p.pos.y - 1.48).abs() < 0.01,
        "out 65, down 160 and the 12 it rises: {:?} from {:?}",
        p.pos,
        from
    );
    assert!((p.units_per_frame().y - 12.).abs() < 1e-3);
    // Eight steps with no push of the stick on the speed, then it takes hold.
    for _ in 0..8 {
        p.step(&back, &scene);
        assert!(p.units_per_frame().x.abs() < 1e-3);
    }
    p.step(&back, &scene);
    assert!(p.units_per_frame().x > 0.4);
}

/// A jump pressed after the catch clip climbs fast (`ClimbFast`, 34 steps); an
/// early one makes the climb jump instead.
#[cfg_attr(test, test)]
fn climbing_fast_and_the_climb_jump() {
    let scene = platform();
    let press = Orders {
        jump: true,
        jump_held: true,
        ..default()
    };
    let mut p = caught(&scene);
    hang(&mut p, &scene, 70, &default());
    p.step(&press, &scene);
    assert_eq!(p.nerve_name().name, "WallCatch/ClimbFast");
    hang(&mut p, &scene, 60, &default());
    assert!(p.grounded && p.mode == Move::Idle, "{:?}", p.mode);
    assert!(
        (p.pos.y - 3.).abs() < 0.01 && p.pos.x < 0.,
        "stands on the top: {:?}",
        p.pos
    );
    // Pressed within the first 30 steps of the hang (and after the 5th of the
    // catch): the climb jump, which goes up at 20 and toward the wall.
    let mut p = caught(&scene);
    hang(&mut p, &scene, 8, &default());
    p.step(&press, &scene);
    assert_eq!(p.nerve_name().name, "WallCatch/Jump");
    hang(&mut p, &scene, 2, &default());
    assert!(
        p.units_per_frame().y > 15. && p.units_per_frame().x < 0.,
        "{:?}",
        p.units_per_frame()
    );
}

/// The stick along a straight ledge slides the catch point 70 over 10 steps.
#[cfg_attr(test, test)]
fn sliding_along_a_straight_ledge() {
    let scene = platform();
    let mut p = caught(&scene);
    hang(&mut p, &scene, 70, &default());
    let from = p.pos;
    // The body's left is +z when it faces -x (its side axis is the front
    // crossed with gravity).
    let along = Orders {
        direction: Vec2::new(0., 1.),
        stick: Vec2::new(0., 1.),
        ..default()
    };
    p.step(&along, &scene);
    assert!(
        matches!(
            p.nerve_name().name,
            "WallCatch/MoveLeft" | "WallCatch/MoveRight"
        ),
        "{}",
        p.nerve_name().name
    );
    hang(&mut p, &scene, 11, &along);
    let moved = (p.pos - from).with_y(0.).length();
    assert!((moved - 0.7).abs() < 0.15, "70 along the ledge: {moved}");
    assert!((p.pos.y - 3.).abs() < 0.01);
    hang(&mut p, &scene, 40, &default());
    assert_eq!(p.nerve_name().name, "WallCatch/Wait");
}
