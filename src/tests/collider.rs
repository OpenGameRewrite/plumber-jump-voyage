use super::*;

pub(super) fn run() {
    standing_is_held_by_the_arrows();
    upright_fall_needs_to_go_below_the_floor();
    crouched_fall_touches_on_arrival();
    crouched_hop_still_reports_the_floor();
    a_wall_stops_the_sphere_at_its_radius();
    low_step_is_walked_over();
    ceiling_stops_a_jump();
    exact_arrival_depth_is_just_under_the_margin();
}

fn square(y: f32, half: f32) -> [[Vec3; 3]; 2] {
    [
        [
            Vec3::new(-half, y, -half),
            Vec3::new(-half, y, half),
            Vec3::new(half, y, -half),
        ],
        [
            Vec3::new(half, y, half),
            Vec3::new(half, y, -half),
            Vec3::new(-half, y, half),
        ],
    ]
}
fn flat() -> TriangleMesh {
    TriangleMesh::from_corners(&square(0., 1000.))
}
fn fall(collider: &mut PlayerCollider, mesh: &TriangleMesh, y: f32, v: f32) -> (Vec3, bool) {
    let out = collider.collide(mesh, Vec3::new(0., y, 0.), Vec3::new(0., v, 0.), 0.);
    (out.pos, collider.grounded())
}

/// Resting on a floor with the game's pull of 3: the arrows go in 3 and push
/// the feet back to the surface every frame.
#[cfg_attr(test, test)]
fn standing_is_held_by_the_arrows() {
    let mesh = flat();
    let mut c = PlayerCollider::new(Form::Normal);
    let (pos, grounded) = fall(&mut c, &mesh, 0., -3.);
    assert_eq!(pos.y, 0.);
    assert!(grounded);
    assert_eq!(c.ground_normal, Vec3::Y);
    assert_eq!(c.floor.as_ref().map(|f| f.triangle < 2), Some(true));
}

/// Backflip, side somersault and triple jump land a row after the feet first
/// reach the surface: a fall that ends exactly on it does not touch, the
/// next one goes in.
#[cfg_attr(test, test)]
fn upright_fall_needs_to_go_below_the_floor() {
    let mesh = flat();
    let mut c = PlayerCollider::new(Form::Normal);
    let (pos, grounded) = fall(&mut c, &mesh, 31., -31.);
    assert_eq!((pos.y, grounded), (0., false));
    let (pos, grounded) = fall(&mut c, &mesh, pos.y, -32.);
    assert_eq!((pos.y, grounded), (0., true));
    // A fall from higher up, going in by 17.5 at once, ends on the surface.
    let (pos, grounded) = fall(&mut c, &mesh, 10.5, -28.);
    assert_eq!((pos.y, grounded), (0., true));
}

/// The roll's hop lands exactly on the floor and is already touching it: the
/// crouched body's sphere reaches the feet.
#[cfg_attr(test, test)]
fn crouched_fall_touches_on_arrival() {
    let mesh = flat();
    let mut c = PlayerCollider::new(Form::Mini);
    let (pos, grounded) = fall(&mut c, &mesh, 10.5, -10.5);
    assert_eq!((pos.y, grounded), (0., true));
    // An upright body does not.
    let mut up = PlayerCollider::new(Form::Normal);
    assert!(!fall(&mut up, &mesh, 10.5, -10.5).1);
}

/// The hop that starts a roll rises from the floor and the collider still
/// reports it that frame (the trace's `ground` 1 in the rising row).
#[cfg_attr(test, test)]
fn crouched_hop_still_reports_the_floor() {
    let mesh = flat();
    let mut mini = PlayerCollider::new(Form::Mini);
    let (pos, grounded) = fall(&mut mini, &mesh, 0., 10.5);
    assert_eq!((pos.y, grounded), (10.5, true));
    // An upright jump leaves it at once.
    let mut up = PlayerCollider::new(Form::Normal);
    assert_eq!(fall(&mut up, &mesh, 0., 17.), (Vec3::new(0., 17., 0.), false));
}

/// A wall facing -x at x = 100 stops the body spheres (radius 55) with the
/// feet at 45.
#[cfg_attr(test, test)]
fn a_wall_stops_the_sphere_at_its_radius() {
    let mut list = vec![];
    list.extend(square(0., 1000.));
    // The wall: its normal is -x.
    list.push([
        Vec3::new(100., -500., -500.),
        Vec3::new(100., 500., -500.),
        Vec3::new(100., -500., 500.),
    ]);
    list.push([
        Vec3::new(100., 500., 500.),
        Vec3::new(100., -500., 500.),
        Vec3::new(100., 500., -500.),
    ]);
    let mut mesh = TriangleMesh::from_corners(&list);
    // Make sure the wall faces the player.
    for t in &mut mesh.triangles[2..] {
        if t.normal.x > 0. {
            t.normal = -t.normal;
        }
    }
    let mut c = PlayerCollider::new(Form::Normal);
    let out = c.collide(&mesh, Vec3::new(0., 0., 0.), Vec3::new(80., -3., 0.), 0.);
    assert!((out.pos.x - 45.).abs() < 1e-3, "{:?}", out.pos);
    // The hit is on a step of the sweep, 2 down: the push back up is exact
    // to single precision only.
    assert!(out.pos.y.abs() < 1e-4, "{:?}", out.pos);
    assert!(c.grounded());
    let wall = c.wall.expect("wall contact");
    assert!(wall.normal.x < -0.99);
}

/// A step a few cm high does not block the run: the spheres' flank touches
/// it only above the small-step limit.
#[cfg_attr(test, test)]
fn low_step_is_walked_over() {
    let mut list = vec![];
    list.extend(square(0., 1000.));
    // A raised floor 8 cm high from x = 50 on, with its riser at x = 50.
    list.push([
        Vec3::new(50., 8., -500.),
        Vec3::new(50., 8., 500.),
        Vec3::new(500., 8., -500.),
    ]);
    list.push([
        Vec3::new(500., 8., 500.),
        Vec3::new(500., 8., -500.),
        Vec3::new(50., 8., 500.),
    ]);
    let mesh = TriangleMesh::from_corners(&list);
    let mut c = PlayerCollider::new(Form::Normal);
    let mut pos = Vec3::ZERO;
    for _ in 0..12 {
        pos = c.collide(&mesh, pos, Vec3::new(14., -3., 0.), 0.).pos;
    }
    assert!(pos.x > 150., "stopped at {pos:?}");
    assert!((pos.y - 8.).abs() < 1e-3, "{pos:?}");
}

/// A ceiling 170 up stops the head sphere (top at 160 plus 55 below its
/// centre): the feet cannot rise past 10.
#[cfg_attr(test, test)]
fn ceiling_stops_a_jump() {
    let mut list = vec![];
    list.extend(square(0., 1000.));
    let mut roof = square(215., 1000.);
    for tri in &mut roof {
        tri.swap(1, 2);
    }
    list.extend(roof);
    let mesh = TriangleMesh::from_corners(&list);
    let mut c = PlayerCollider::new(Form::Normal);
    let out = c.collide(&mesh, Vec3::new(0., 40., 0.), Vec3::new(0., 40., 0.), 0.);
    // The head sphere's top is 160 above the feet.
    assert!(out.pos.y < 56., "{:?}", out.pos);
    assert!(c.ceiling.is_some());
}

/// The reason an exact arrival only supports: the arrow's depth is rounded
/// to just under its margin.
#[cfg_attr(test, test)]
fn exact_arrival_depth_is_just_under_the_margin() {
    let t = TriangleMesh::from_corners(&square(0., 100.));
    let hit = arrow_hit(&t.triangles[0], Vec3::new(0., 40., 0.), Vec3::new(0., -60., 0.))
        .expect("the arrow crosses the floor");
    assert!(hit.depth < 20. && hit.depth > 19.999);
    let hit = arrow_hit(&t.triangles[0], Vec3::new(0., 39.5, 0.), Vec3::new(0., -60., 0.)).unwrap();
    assert!(hit.depth >= 20.);
}
