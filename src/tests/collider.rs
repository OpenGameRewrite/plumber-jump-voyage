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
    a_reset_body_skips_the_hair_touch_at_the_start_of_its_move();
    the_bounding_radius_and_the_growth_of_a_reset_collider();
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
    assert_eq!(
        fall(&mut up, &mesh, 0., 17.),
        (Vec3::new(0., 17., 0.), false)
    );
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
    let hit = arrow_hit(
        &t,
        &t.triangles[0],
        Vec3::new(0., 40., 0.),
        Vec3::new(0., -60., 0.),
    )
    .expect("the arrow crosses the floor");
    assert!(hit.depth < 20. && hit.depth > 19.999);
    let hit = arrow_hit(
        &t,
        &t.triangles[0],
        Vec3::new(0., 39.5, 0.),
        Vec3::new(0., -60., 0.),
    )
    .unwrap();
    assert!(hit.depth >= 20.);
}

/// A wall `x = 100` (normal -x) and a second one that leans away from the first by under a
/// degree and starts 0.02 further out, so a body that touches the first by a hair is clear of the
/// second.
fn two_walls() -> TriangleMesh {
    let facing_minus_x = |a: Vec3, b: Vec3, c: Vec3| {
        if (b - a).cross(c - a).x > 0. {
            [a, c, b]
        } else {
            [a, b, c]
        }
    };
    let mut list = vec![];
    let (p, q) = (500., 4000.);
    let (a, b, c, d) = (
        Vec3::new(100., -p, -p),
        Vec3::new(100., p, -p),
        Vec3::new(100., -p, p),
        Vec3::new(100., p, p),
    );
    list.push(facing_minus_x(a, b, c));
    list.push(facing_minus_x(d, c, b));
    let angle = 0.8f32.to_radians();
    let along = Vec3::new(angle.sin(), angle.cos(), 0.);
    let origin = Vec3::new(100.02, 65., 0.);
    let corner = |u: f32, z: f32| origin + along * u + Vec3::Z * z;
    list.push(facing_minus_x(corner(-q, -q), corner(q, -q), corner(-q, q)));
    list.push(facing_minus_x(corner(q, q), corner(-q, q), corner(q, -q)));
    TriangleMesh::from_corners(&list)
}

/// After `rs::resetCollisionExpandCheck` the collider grows the shapes back in the next
/// `collide` and the frame's own move skips its first pose: a body that touches a wall by a hair
/// is not pushed at that pose but runs into the wall and out of it, which lets a second, leaning
/// wall push it along its slope (the wall slide's start in the real game does exactly this).
#[cfg_attr(test, test)]
fn a_reset_body_skips_the_hair_touch_at_the_start_of_its_move() {
    let mesh = two_walls();
    let start = Vec3::new(45.0001, 0., 0.);
    let into_wall = Vec3::new(15., 0., 0.);
    let mut plain = PlayerCollider::new(Form::Normal);
    let plain_out = plain.collide(&mesh, start, into_wall, 0.);
    assert!(plain_out.pos.y.abs() < 1e-3, "{:?}", plain_out.pos);
    assert!((plain_out.pos.x - 45.).abs() < 0.03, "{:?}", plain_out.pos);

    let mut reset = PlayerCollider::new(Form::Normal);
    reset.reset_expand_check(0.);
    let out = reset.collide(&mesh, start, into_wall, 0.);
    assert!(out.pos.y > 0.1, "{:?}", out.pos);
    assert!((out.pos.x - 45.).abs() < 0.03, "{:?}", out.pos);
    // The growth is a one-off: the collider has the full radius again afterwards.
    assert_eq!(reset.last_radius, reset.keeper.bounding_radius);
}

/// `CollisionShapeKeeper::updateShape`: the sphere around the player's shapes, and the scale a
/// reset starts the shapes at.
#[cfg_attr(test, test)]
fn the_bounding_radius_and_the_growth_of_a_reset_collider() {
    let keeper = Keeper::new(Form::Normal);
    assert!(
        (keeper.bounding_radius - 95.532).abs() < 0.01,
        "{}",
        keeper.bounding_radius
    );
    assert_eq!(Keeper::cap().bounding_radius, 60.);
    // From a radius of 20 to the full one, in steps of 29.7 of (length + growth).
    let sweep = Sweep::new(Vec3::ZERO, Vec3::ZERO, 29.7, 20., keeper.bounding_radius);
    assert!((sweep.step - 29.7 / (keeper.bounding_radius - 20.)).abs() < 1e-6);
    assert!((sweep.shape_scale(1.) - 20. / keeper.bounding_radius).abs() < 1e-6);
    let mut sweep = sweep;
    while !sweep.finished() {
        sweep.advance();
    }
    assert_eq!(sweep.shape_scale(1.), 1.);
}
