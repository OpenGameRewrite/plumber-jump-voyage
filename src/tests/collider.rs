use super::*;

pub(super) fn run() {
    a_carry_keeps_the_orientation_for_the_velocity_sweep();
    standing_is_held_by_the_arrows();
    translating_ground_carries_descent_and_supports_landing();
    rotational_jump_displacement_uses_the_actor_point_without_normal_projection();
    upright_fall_needs_to_go_below_the_floor();
    crouched_fall_touches_on_arrival();
    crouched_hop_still_reports_the_floor();
    a_wall_stops_the_sphere_at_its_radius();
    low_step_is_walked_over();
    ceiling_stops_a_jump();
    exact_arrival_depth_is_just_under_the_margin();
    a_reset_body_skips_the_hair_touch_at_the_start_of_its_move();
    the_bounding_radius_and_the_growth_of_a_reset_collider();
    the_exes_sweep_poses_agree_bit_for_bit();
    the_collided_queries_need_a_depth_of_zero_or_more();
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

/// `rs::isCollidedGround` and `rs::isCollidedWallFace` also ask the deepest contact's depth
/// (`+0x70`, `+0x80`) to be at least zero; a contact kept a hair short of zero (a disk's
/// `isNearZero` tolerance) is not a collision for them although the collider holds it.
#[cfg_attr(test, test)]
fn the_collided_queries_need_a_depth_of_zero_or_more() {
    let mut c = PlayerCollider::new(Form::Normal);
    fall(&mut c, &flat(), 0., -3.);
    assert!(c.collided_ground());
    c.best[0] = -0.0005;
    assert!(c.grounded() && !c.collided_ground());
    c.invalidate(0.);
    assert!(!c.grounded() && !c.collided_ground());

    let mut list = vec![];
    list.extend(square(0., 1000.));
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
    for t in &mut mesh.triangles[2..] {
        if t.normal.x > 0. {
            t.normal = -t.normal;
        }
    }
    let mut c = PlayerCollider::new(Form::Normal);
    c.collide(&mesh, Vec3::new(0., 0., 0.), Vec3::new(80., -3., 0.), 0.);
    assert!(c.collided_wall_face().is_some_and(|w| w.normal.x < -0.99));
    c.best[1] = -0.0005;
    assert!(c.wall.is_some() && c.collided_wall_face().is_none());
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

/// The poses a sweep checks and the world shapes at them, from the executable's own
/// `SpherePoseInterpolator`, `findCollidePos` and `calcWorldShapeInfo` run in a local interpreter
/// (the fidelity folder's `collider_pose_oracle.py`): `PJV_POSE_SWEEP_CASES` names the file.
#[cfg_attr(test, test)]
fn the_exes_sweep_poses_agree_bit_for_bit() {
    use serde_json::Value;
    let Some(path) = std::env::var_os("PJV_POSE_SWEEP_CASES") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("the cases file");
    let cases: Value = serde_json::from_str(&text).expect("json");
    let v3 = |v: &Value| -> Vec3 {
        let a = v.as_array().expect("a vector");
        Vec3::new(
            a[0].as_f64().unwrap() as f32,
            a[1].as_f64().unwrap() as f32,
            a[2].as_f64().unwrap() as f32,
        )
    };
    let num = |v: &Value| v.as_f64().expect("a number") as f32;
    let quat = |v: &Value| {
        let a = v.as_array().expect("a quaternion");
        Quat::from_xyzw(num(&a[0]), num(&a[1]), num(&a[2]), num(&a[3]))
    };
    let mut poses = 0;
    for case in cases.as_array().expect("cases") {
        let start = v3(&case["start"]);
        let tilt = Tilt {
            from: quat(&case["from"]),
            to: quat(&case["to"]),
        };
        let base = Pose::new(start, 0., 0., Some(tilt));
        let mut sweep = Sweep::new(
            start,
            v3(&case["end"]),
            num(&case["step"]),
            num(&case["r0"]),
            num(&case["r1"]),
        );
        if case["skip"].as_bool().unwrap() {
            sweep.advance();
        }
        let expect = &case["expect"];
        let mut index = 0;
        while !sweep.finished() && index < 40 {
            let want = &expect["poses"][index];
            let pose = base.along(&sweep);
            let m: Vec<f32> = want["m"].as_array().unwrap().iter().map(&num).collect();
            let what = format!("pose {index} of {case}");
            let rows = [0, 1, 2, 4, 5, 6, 8, 9, 10];
            for (k, row) in rows.into_iter().enumerate() {
                assert_eq!(pose.rotation[k], m[row], "matrix {k} of {what}");
            }
            assert_eq!(pose.pos, Vec3::new(m[3], m[7], m[11]), "position of {what}");
            let scale = sweep.shape_scale(num(&case["own"]));
            assert_eq!(scale, num(&want["scale"]), "scale of {what}");
            for (shape, expected) in case["spheres"]
                .as_array()
                .unwrap()
                .iter()
                .zip(want["spheres"].as_array().unwrap())
            {
                let (center, radius) =
                    pose.world_sphere(v3(&shape["center"]), num(&shape["radius"]), scale);
                assert_eq!(center, v3(&expected["center"]), "sphere centre of {what}");
                assert_eq!(radius, num(&expected["radius"]), "sphere radius of {what}");
                assert_eq!(
                    pose.rotate(Vec3::Y),
                    v3(&expected["axis"]),
                    "sphere axis of {what}"
                );
            }
            for (shape, expected) in case["arrows"]
                .as_array()
                .unwrap()
                .iter()
                .zip(want["arrows"].as_array().unwrap())
            {
                let (from, vector) =
                    pose.world_arrow(v3(&shape["start"]), v3(&shape["vector"]), scale);
                assert_eq!(from, v3(&expected["start"]), "arrow start of {what}");
                assert_eq!(vector, v3(&expected["vec"]), "arrow vector of {what}");
            }
            sweep.advance();
            index += 1;
            poses += 1;
        }
        assert_eq!(
            index,
            (expect["count"].as_u64().unwrap() as usize).min(40),
            "pose count of {case}"
        );
    }
    eprintln!("sweep poses checked against the executable: {poses}");
}

/// The radius-change carry reaches the requested orientation before the
/// frame's translation starts (moveCollide's quaternion is an in/out argument).
#[cfg_attr(test, test)]
fn a_carry_keeps_the_orientation_for_the_velocity_sweep() {
    let mesh = TriangleMesh::from_corners(&[]);
    let mut collider = PlayerCollider::default();
    collider.collide(&mesh, Vec3::ZERO, Vec3::ZERO, 0.);
    collider.set_form(Form::Mini);
    let from = Quat::IDENTITY;
    let to = Quat::from_rotation_x(0.7);
    let mut pose = Pose::new(Vec3::ZERO, 0., 0., Some(Tilt { from, to }));
    let mut at = Vec3::ZERO;
    let mut radius = 160.;
    assert!(!collider.move_collide(
        &mesh,
        &mut pose,
        &mut at,
        (&mut radius, 110.),
        Vec3::ZERO,
        Vec3::ZERO,
        35.,
        false,
    ));
    assert_eq!(pose.tilt.unwrap().from, normalize_quat(to));
    let mut moving = Sweep::new(at, Vec3::NEG_Z * 20., 5., radius, radius);
    moving.advance();
    assert_eq!(
        pose.along(&moving).rotation,
        rotation_of(normalize_quat(to))
    );
}

/// A translating raw KCL top preserves inward carry, outward collision push,
/// departure asymmetry and landing, without making its Press code a death.
#[cfg_attr(test, test)]
fn translating_ground_carries_descent_and_supports_landing() {
    use crate::geometry::LocalPrism;
    use crate::movement::key_move_parts::KeyMovePart;
    use crate::prism::{PartFrame, Prism};
    let frame = PartFrame::new([1., 0., 0., 0., 0., 1., 0., 1000., 0., 0., 1., 0.]).unwrap();
    let diagonal = std::f32::consts::FRAC_1_SQRT_2;
    let prism = |edges| Prism {
        length: 200.,
        origin: [-100., 0., -100.],
        face: [0., 1., 0.],
        edges,
        thickness: 300.,
    };
    let prisms = [
        prism([[diagonal, 0., -diagonal], [-1., 0., 0.], [0., 0., 1.]]),
        prism([[0., 0., -1.], [-diagonal, 0., diagonal], [1., 0., 0.]]),
    ];
    let corners = prisms.map(|p| p.world_triangle(&frame).corners.map(Vec3::from_array));
    let mut mesh = TriangleMesh::from_corners(&corners);
    mesh.parts.push(frame);
    mesh.codes[0].floor = "Press".into();
    mesh.codes[0].floor_index = 19;
    for (triangle, prism) in mesh.triangles.iter_mut().zip(prisms) {
        triangle.local = Some(LocalPrism { part: 0, prism });
    }
    mesh.key_move_parts.push(KeyMovePart {
        id: "test".into(),
        part: 0,
        keys: [Vec3::Y * 1000., Vec3::Y * 1100.],
        wait: [1; 2],
        duration: [20; 2],
        moving: true,
        key: 0,
        step: 0,
        position: Vec3::Y * 1000.,
        motion: Vec3::ZERO,
    });
    let mut collider = PlayerCollider::default();
    let mut pos = collider
        .collide(&mesh, Vec3::Y * 1000., Vec3::NEG_Y * 3., 0.)
        .pos;
    let update = |mesh: &mut TriangleMesh, y, delta| {
        mesh.key_move_parts[0].position.y = y;
        mesh.key_move_parts[0].motion.y = delta;
        mesh.update_key_move_collision();
    };
    update(&mut mesh, 995., -5.);
    pos = collider.collide(&mesh, pos, Vec3::NEG_Y * 3., 0.).pos;
    assert_eq!(pos.y, 995.);
    assert!(collider.grounded());
    update(&mut mesh, 1000., 5.);
    pos = collider.collide(&mesh, pos, Vec3::NEG_Y * 3., 0.).pos;
    assert_eq!(pos.y, 1000.);
    assert!(collider.grounded());
    update(&mut mesh, 1005., 5.);
    pos = collider.collide(&mesh, pos, Vec3::Y * 17., 0.).pos;
    assert_eq!(pos.y, 1017.);
    // Land on the now moving top, then depart with inward normal carry.
    pos = collider.collide(&mesh, pos, Vec3::NEG_Y * 20., 0.).pos;
    assert_eq!(pos.y, 1005.);
    assert!(collider.grounded());
    update(&mut mesh, 1000., -5.);
    pos = collider.collide(&mesh, pos, Vec3::Y * 17., 0.).pos;
    assert_eq!(pos.y, 1017.);
    assert!(!collider.pressed_ground);
}

#[cfg_attr(test, test)]
fn rotational_jump_displacement_uses_the_actor_point_without_normal_projection() {
    use crate::geometry::LocalPrism;
    use crate::movement::rotate_parts::RotatePart;
    use crate::prism::{PartFrame, Prism};
    let previous = PartFrame::new([1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0.]).unwrap();
    let mut mesh = flat();
    mesh.parts.push(previous.clone());
    mesh.triangles[0].local = Some(LocalPrism {
        part: 0,
        prism: Prism {
            length: 1.,
            origin: [0.; 3],
            face: [0., 1., 0.],
            edges: [[0.; 3]; 3],
            thickness: 1.,
        },
    });
    mesh.rotate_parts.push(RotatePart {
        id: "test".into(),
        part: 0,
        position: Vec3::ZERO,
        speed: 50.,
        executions: 1,
        quat: Quat::from_xyzw(1., 0., 0., 0.),
        previous,
    });
    let collider = PlayerCollider {
        floor: Some(Contact {
            normal: Vec3::NEG_Y,
            position: Vec3::ZERO,
            triangle: 0,
            location: Location::Face,
        }),
        ..PlayerCollider::default()
    };
    let actor = Vec3::new(0., 100., 25.);
    assert_eq!(collider.moving_ground_force(&mesh), Vec3::ZERO);
    assert_eq!(
        collider.moving_ground_displacement_at(&mesh, actor),
        Vec3::new(0., -200., -50.)
    );
    assert_eq!(
        collider.moving_ground_force_at(&mesh, actor),
        Vec3::new(0., 0., -50.)
    );
}
