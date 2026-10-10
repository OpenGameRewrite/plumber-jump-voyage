use crate::movement::{Course, Move, Player, Pole, SPAWN, Solid};
use bevy::prelude::*;

#[derive(Resource)]
pub struct Orbit {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}
impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: 0.,
            pitch: 0.30,
            distance: 9.,
        }
    }
}
#[derive(Component)]
struct MainCamera;
#[derive(Component)]
struct PreviewCamera;
#[derive(Component)]
struct PreviewLight;
#[derive(Component)]
pub struct Body;
#[derive(Component)]
pub struct Part {
    pub kind: u8,
    pub base: Vec3,
}
#[derive(Component)]
pub struct Hat;
#[derive(Component)]
struct Platform {
    index: usize,
    home: Vec3,
    phase: f32,
}
#[derive(Component)]
struct Gem {
    home: Vec3,
    id: usize,
}
#[derive(Component)]
struct Target {
    home: Vec3,
    id: usize,
}
#[derive(Component)]
struct Checkpoint;
#[derive(Resource, Default)]
pub struct Progress {
    pub gems: Vec<usize>,
    pub targets: Vec<usize>,
    pub time: f32,
    pub started: bool,
    pub finished: bool,
    pub message: String,
    pub message_time: f32,
}
#[derive(Resource)]
pub struct Palette(pub Handle<StandardMaterial>);

pub fn plugin(app: &mut App) {
    app.init_resource::<Orbit>()
        .init_resource::<Progress>()
        .add_systems(Startup, setup)
        .add_systems(PreUpdate, moving_platforms.after(crate::input::read))
        .add_systems(Update, (animate, collect, camera, preview).chain());
    if std::env::var_os("PJV_SHOT").is_some() {
        app.add_systems(Update, screenshot);
    }
}
fn material(m: &mut Assets<StandardMaterial>, rgb: [f32; 3]) -> Handle<StandardMaterial> {
    m.add(StandardMaterial {
        base_color: Color::srgb(rgb[0], rgb[1], rgb[2]),
        perceptual_roughness: 0.72,
        ..default()
    })
}
fn block(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mat: Handle<StandardMaterial>,
    center: Vec3,
    size: Vec3,
) -> Entity {
    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::from_size(size))),
            MeshMaterial3d(mat),
            Transform::from_translation(center),
        ))
        .id()
}
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut course: ResMut<Course>,
) {
    let cream = material(&mut materials, [0.88, 0.83, 0.69]);
    let white = material(&mut materials, [0.97, 0.96, 0.89]);
    let red = material(&mut materials, [0.81, 0.31, 0.20]);
    let teal = material(&mut materials, [0.10, 0.52, 0.53]);
    let dark = material(&mut materials, [0.06, 0.18, 0.23]);
    let gold = material(&mut materials, [1., 0.68, 0.15]);
    let skin = material(&mut materials, [0.67, 0.87, 0.85]);
    commands.insert_resource(Palette(teal.clone()));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.78, 0.88, 1.),
        brightness: 600.,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            illuminance: 16000.,
            shadow_maps_enabled: true,
            ..default()
        },
        bevy::camera::visibility::RenderLayers::from_layers(&[0, 1]),
        Transform::from_xyz(10., 18., 8.).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Camera3d::default(),
        MainCamera,
        IsDefaultUiCamera,
        Transform::from_xyz(0., 5., 14.).looking_at(Vec3::Y, Vec3::Y),
    ));
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 2,
            is_active: false,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        bevy::camera::visibility::RenderLayers::layer(1),
        PreviewCamera,
        Transform::default(),
    ));
    commands.spawn((
        PointLight {
            intensity: 800_000.,
            range: 8.,
            ..default()
        },
        bevy::camera::visibility::RenderLayers::layer(1),
        PreviewLight,
        Transform::default(),
    ));
    let mut add = |c: Vec3,
                   size: Vec3,
                   mat: Handle<StandardMaterial>,
                   slope: f32,
                   moving: bool,
                   slippery: bool| {
        let i = course.solids.len();
        course.solids.push(Solid {
            center: c,
            half: size * 0.5,
            slope,
            moving,
            slippery,
        });
        let entity = if slope == 0. {
            block(&mut commands, &mut meshes, mat, c, size)
        } else {
            // Ramp render is a wedge: geometry and analytic top use the same equation.
            let h = size * 0.5;
            let low = h.y - h.x * slope;
            let high = h.y + h.x * slope;
            let points = [
                [-h.x, -h.y, -h.z],
                [h.x, -h.y, -h.z],
                [h.x, high, -h.z],
                [-h.x, low, -h.z],
                [-h.x, -h.y, h.z],
                [h.x, -h.y, h.z],
                [h.x, high, h.z],
                [-h.x, low, h.z],
            ];
            let faces = [
                [0, 3, 2, 1],
                [4, 5, 6, 7],
                [0, 4, 7, 3],
                [1, 2, 6, 5],
                [3, 7, 6, 2],
                [0, 1, 5, 4],
            ];
            let mut positions = Vec::new();
            let mut normals = Vec::new();
            let mut uv = Vec::new();
            for f in faces {
                let a = Vec3::from_array(points[f[0]]);
                let b = Vec3::from_array(points[f[1]]);
                let c = Vec3::from_array(points[f[2]]);
                let n = (b - a).cross(c - a).normalize();
                for k in [0, 1, 2, 0, 2, 3] {
                    positions.push(points[f[k]]);
                    normals.push(n.to_array());
                    uv.push([0., 0.]);
                }
            }
            let mesh = Mesh::new(
                bevy::mesh::PrimitiveTopology::TriangleList,
                bevy::asset::RenderAssetUsages::default(),
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv);
            commands
                .spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(mat),
                    Transform::from_translation(c),
                ))
                .id()
        };
        if moving {
            commands.entity(entity).insert(Platform {
                index: i,
                home: c,
                phase: i as f32,
            });
        }
    };
    add(
        Vec3::new(0., -1., 0.),
        Vec3::new(42., 2., 38.),
        cream.clone(),
        0.,
        false,
        false,
    );
    // Jump stairway, split wall corridor, ramps, long-jump islands and moving pads.
    for i in 0..5 {
        add(
            Vec3::new(-10., i as f32 * 0.6 + 0.25, -3. - i as f32 * 3.),
            Vec3::new(4., 0.5, 2.2),
            if i % 2 == 0 {
                teal.clone()
            } else {
                white.clone()
            },
            0.,
            false,
            false,
        );
    }
    add(
        Vec3::new(8., 2., -7.),
        Vec3::new(1., 4., 8.),
        red.clone(),
        0.,
        false,
        false,
    );
    add(
        Vec3::new(11., 3., -7.),
        Vec3::new(1., 6., 8.),
        red.clone(),
        0.,
        false,
        false,
    );
    add(
        Vec3::new(15., 3.3, -11.),
        Vec3::new(6., 0.6, 3.),
        white.clone(),
        0.,
        false,
        false,
    );
    add(
        Vec3::new(-8., 1.5, 8.),
        Vec3::new(8., 3., 5.),
        teal.clone(),
        0.375,
        false,
        false,
    );
    add(
        Vec3::new(-15., 0.5, 12.),
        Vec3::new(4., 1., 5.),
        white.clone(),
        0.,
        false,
        true,
    );
    for i in 0..4 {
        add(
            Vec3::new(-4. + i as f32 * 6., i as f32 * 1.4, -24. - i as f32 * 5.),
            Vec3::new(4., 1.5, 4.),
            if i % 2 == 0 {
                red.clone()
            } else {
                teal.clone()
            },
            0.,
            false,
            false,
        );
    }
    for i in 0..3 {
        add(
            Vec3::new(-24. - i as f32 * 5., 1. + i as f32 * 0.7, -8.),
            Vec3::new(3., 0.7, 3.),
            gold.clone(),
            0.,
            true,
            false,
        );
    }
    add(
        Vec3::new(-41., 2., -8.),
        Vec3::new(7., 1., 8.),
        white.clone(),
        0.,
        false,
        false,
    );
    add(
        Vec3::new(0., 4.8, -8.),
        Vec3::new(5., 0.8, 4.),
        red.clone(),
        0.,
        false,
        false,
    );
    // Shallow low tunnel tests crawl clearance; raised underside is authoritative.
    add(
        Vec3::new(1., 1.3, 12.),
        Vec3::new(8., 0.4, 4.),
        red.clone(),
        0.,
        false,
        false,
    );
    // Pool uses its own floor below the main terrace, outside the terrace bounds.
    add(
        Vec3::new(9., -4., 25.),
        Vec3::new(13., 1., 11.),
        cream.clone(),
        0.,
        false,
        false,
    );
    course.water = Some((Vec3::new(9., 0., 25.), Vec3::new(6., 4., 5.)));
    for (c, s) in [
        (Vec3::new(2., -1.5, 25.), Vec3::new(1., 5., 12.)),
        (Vec3::new(16., -1.5, 25.), Vec3::new(1., 5., 12.)),
        (Vec3::new(9., -1.5, 31.), Vec3::new(15., 5., 1.)),
    ] {
        course.solids.push(Solid {
            center: c,
            half: s * 0.5,
            slope: 0.,
            moving: false,
            slippery: false,
        });
        block(&mut commands, &mut meshes, white.clone(), c, s);
    }
    let water = materials.add(StandardMaterial {
        base_color: Color::srgba(0.10, 0.61, 0.80, 0.55),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.2,
        ..default()
    });
    block(
        &mut commands,
        &mut meshes,
        water,
        Vec3::new(9., -0.05, 25.),
        Vec3::new(13., 0.1, 11.),
    );
    for c in [Vec3::new(0., 0., -8.), Vec3::new(-17., 0., -11.)] {
        course.poles.push(Pole {
            center: c,
            height: 6.,
        });
        commands.spawn((
            Mesh3d(meshes.add(Cylinder::new(0.18, 6.))),
            MeshMaterial3d(dark.clone()),
            Transform::from_translation(c + Vec3::Y * 3.),
        ));
        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(0.35))),
            MeshMaterial3d(gold.clone()),
            Transform::from_translation(c + Vec3::Y * 6.),
        ));
    }
    // Architectural edging and simple trees are original procedural scenery.
    for i in 0..16 {
        let a = i as f32 * 0.4;
        let at = Vec3::new(a.cos() * 18., 0., a.sin() * 16.);
        commands.spawn((
            Mesh3d(meshes.add(Cylinder::new(0.18, 2.2))),
            MeshMaterial3d(cream.clone()),
            Transform::from_translation(at + Vec3::Y * 1.1),
        ));
        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(1.2))),
            MeshMaterial3d(teal.clone()),
            Transform::from_translation(at + Vec3::Y * 2.8),
        ));
    }
    for i in 0..12 {
        let at = Vec3::new(-55. + i as f32 * 10., 14. + (i % 3) as f32 * 4., -55.);
        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(3.))),
            MeshMaterial3d(white.clone()),
            Transform::from_translation(at).with_scale(Vec3::new(2.5, 0.35, 1.)),
        ));
    }
    let gems = [
        Vec3::new(0., 1., -3.),
        Vec3::new(-10., 2.4, -9.),
        Vec3::new(-10., 4., -15.),
        Vec3::new(9.5, 6., -7.),
        Vec3::new(15., 5., -11.),
        Vec3::new(-7., 5., 8.),
        Vec3::new(0., 6.5, -8.),
        Vec3::new(-4., 2., -24.),
        Vec3::new(2., 3.4, -29.),
        Vec3::new(8., 4.8, -34.),
        Vec3::new(14., 6.2, -39.),
        Vec3::new(-41., 4., -8.),
        Vec3::new(9., -1.5, 26.),
        Vec3::new(-15., 2., 12.),
        Vec3::new(3., 1., 13.),
    ];
    for (id, home) in gems.into_iter().enumerate() {
        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(0.24))),
            MeshMaterial3d(gold.clone()),
            Transform::from_translation(home),
            Gem { home, id },
        ));
    }
    for (id, home) in [
        Vec3::new(-4., 2., -5.),
        Vec3::new(4., 6., -8.),
        Vec3::new(-13., 0.65, 3.),
    ]
    .into_iter()
    .enumerate()
    {
        course.targets.push(home);
        commands.spawn((
            Mesh3d(meshes.add(Torus::new(0.25, 0.7))),
            MeshMaterial3d(red.clone()),
            Transform::from_translation(home)
                .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            Target { home, id },
        ));
    }
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(1.2, 0.08))),
        MeshMaterial3d(gold.clone()),
        Transform::from_xyz(14., 4.8, -39.),
        Checkpoint,
    ));
    let player = commands
        .spawn((
            Player::default(),
            Transform::from_translation(SPAWN),
            Visibility::default(),
        ))
        .id();
    let root = commands
        .spawn((Body, Transform::default(), Visibility::default()))
        .id();
    commands.entity(player).add_child(root);
    let pieces = [
        (
            0,
            Vec3::new(0., 0.93, 0.),
            Vec3::new(0.66, 0.68, 0.42),
            teal.clone(),
        ),
        (1, Vec3::new(0., 1.43, 0.), Vec3::new(0.55, 0.48, 0.5), skin),
        (
            2,
            Vec3::new(-0.19, 0.34, 0.),
            Vec3::new(0.22, 0.60, 0.25),
            dark.clone(),
        ),
        (
            3,
            Vec3::new(0.19, 0.34, 0.),
            Vec3::new(0.22, 0.60, 0.25),
            dark.clone(),
        ),
        (
            4,
            Vec3::new(-0.44, 0.9, 0.),
            Vec3::new(0.2, 0.55, 0.25),
            white.clone(),
        ),
        (
            5,
            Vec3::new(0.44, 0.9, 0.),
            Vec3::new(0.2, 0.55, 0.25),
            white.clone(),
        ),
        (
            6,
            Vec3::new(0., 1.04, 0.3),
            Vec3::new(0.5, 0.55, 0.25),
            gold.clone(),
        ),
    ];
    for (kind, base, size, mat) in pieces {
        let e = block(&mut commands, &mut meshes, mat, base, size);
        commands.entity(e).insert(Part { kind, base });
        commands.entity(root).add_child(e);
    }
    // Single glowing visor instead of a familiar plumber face.
    let visor = block(
        &mut commands,
        &mut meshes,
        dark,
        Vec3::new(0., 1.45, -0.26),
        Vec3::new(0.42, 0.14, 0.04),
    );
    commands.entity(root).add_child(visor);
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(0.4, 0.14))),
        MeshMaterial3d(teal),
        Hat,
        bevy::camera::visibility::RenderLayers::from_layers(&[0, 1]),
        Transform::from_xyz(0., 1.75, 5.),
    ));
}
fn moving_platforms(
    time: Res<Time<Real>>,
    menu: Res<crate::ui::Menu>,
    mut ticks: Local<f32>,
    mut platforms: Query<(&Platform, &mut Transform)>,
    mut scene: ResMut<Course>,
    mut players: Query<&mut Player>,
) {
    if menu.open {
        return;
    }
    *ticks += time.delta_secs().min(0.05);
    for (p, mut t) in &mut platforms {
        let next = p.home + Vec3::Y * (*ticks * 0.9 + p.phase).sin() * 0.6;
        let delta = next - t.translation;
        let b = &mut scene.solids[p.index];
        debug_assert!(b.moving);
        for mut actor in &mut players {
            if actor.grounded
                && (actor.pos.x - b.center.x).abs() < b.half.x + 0.3
                && (actor.pos.z - b.center.z).abs() < b.half.z + 0.3
                && (actor.pos.y - b.top(actor.pos.x)).abs() < 0.1
            {
                actor.pos += delta;
            }
        }
        b.center = next;
        t.translation = next;
    }
}
pub fn animate(
    time: Res<Time>,
    p: Single<(&Player, &mut Transform)>,
    mut body: Single<&mut Transform, (With<Body>, Without<Player>, Without<Part>, Without<Hat>)>,
    mut parts: Query<(&Part, &mut Transform), (Without<Player>, Without<Body>, Without<Hat>)>,
    mut hat: Single<
        (&mut Transform, &mut Visibility),
        (With<Hat>, Without<Player>, Without<Body>, Without<Part>),
    >,
) {
    let (p, mut t) = p.into_inner();
    t.translation = p.pos;
    t.rotation = Quat::from_rotation_y((-p.facing.x).atan2(-p.facing.z));
    let speed = p.vel.with_y(0.).length();
    let phase = time.elapsed_secs() * speed * 1.3;
    body.rotation = match p.mode {
        Move::Roll => Quat::from_rotation_x(-p.distance * 2.5),
        Move::Dive | Move::Slide => Quat::from_rotation_x(-1.3),
        Move::Backflip => Quat::from_rotation_x(p.timer * 7.),
        Move::Sideflip => Quat::from_rotation_z(p.timer * 7.),
        Move::TripleJump | Move::SpinJump => Quat::from_rotation_y(p.timer * 10.),
        _ => Quat::IDENTITY,
    };
    body.translation = Vec3::Y
        * if matches!(p.mode, Move::Crouch | Move::Crawl | Move::Roll) {
            -0.45
        } else {
            0.
        };
    body.scale = if matches!(p.mode, Move::Crouch | Move::Crawl) {
        Vec3::new(1., 0.7, 1.)
    } else {
        Vec3::ONE
    };
    for (part, mut t) in &mut parts {
        t.translation = part.base;
        let limb = matches!(part.kind, 2..=5);
        if limb {
            let sign = if part.kind % 2 == 0 { 1. } else { -1. };
            let angle = if p.grounded {
                phase.sin() * 0.6 * sign * (speed / 4.).min(1.)
            } else if part.kind >= 4 {
                -2.0
            } else {
                0.2 * sign
            };
            t.rotation = Quat::from_rotation_x(angle);
        }
    }
    *hat.1 = Visibility::Visible;
    if let Some(cap) = &p.cap {
        hat.0.translation = cap.pos;
        hat.0.rotation = Quat::from_rotation_y(time.elapsed_secs() * 18.);
        hat.0.scale = Vec3::splat(1.2);
    } else {
        hat.0.translation = p.pos + Vec3::Y * 1.75;
        hat.0.rotation = t.rotation;
        hat.0.scale = Vec3::ONE;
    }
}
fn collect(
    time: Res<Time<Real>>,
    menu: Res<crate::ui::Menu>,
    mut progress: ResMut<Progress>,
    mut p: Single<&mut Player>,
    mut gems: Query<(&Gem, &mut Transform, &mut Visibility)>,
    mut targets: Query<(&Target, &mut MeshMaterial3d<StandardMaterial>)>,
    palette: Res<Palette>,
    mut settings: ResMut<crate::input::Settings>,
) {
    if menu.open {
        return;
    }
    let dt = time.delta_secs().min(0.05);
    progress.message_time = (progress.message_time - dt).max(0.);
    if p.distance > 0.1 {
        progress.started = true;
    }
    if progress.started && !progress.finished {
        progress.time += dt;
    }
    for (g, mut t, mut v) in &mut gems {
        t.translation.y = g.home.y + (time.elapsed_secs() * 2. + g.id as f32).sin() * 0.16;
        t.rotation = Quat::from_rotation_y(time.elapsed_secs());
        if progress.gems.contains(&g.id) {
            *v = Visibility::Hidden;
            continue;
        }
        if (p.pos + Vec3::Y * 0.8).distance(t.translation) < 1.2 {
            progress.gems.push(g.id);
            *v = Visibility::Hidden;
            progress.message = "Sun spark found!".into();
            progress.message_time = 2.;
        }
    }
    for (t, mut mat) in &mut targets {
        if progress.targets.contains(&t.id) {
            continue;
        }
        if p.cap.as_ref().is_some_and(|c| c.pos.distance(t.home) < 1.) {
            progress.targets.push(t.id);
            mat.0 = palette.0.clone();
            progress.message = "Cap target activated!".into();
            progress.message_time = 2.;
        }
    }
    if p.pos.distance(Vec3::new(-41., 2.5, -8.)) < 3. {
        p.checkpoint = Vec3::new(-41., 2.5, -8.);
    }
    if progress.gems.len() == 15 && progress.targets.len() == 3 && !progress.finished {
        progress.finished = true;
        progress.message = format!("Course complete! {:.1} seconds", progress.time);
        progress.message_time = 999.;
        let best = (progress.time * 1000.) as u32;
        if settings.best == 0 || best < settings.best {
            settings.best = best;
            if let Err(e) = settings.save() {
                warn!("Could not save best time: {e}");
            }
        }
    }
}
fn camera(
    time: Res<Time<Real>>,
    p: Single<&Player>,
    menu: Res<crate::ui::Menu>,
    orders: Res<crate::input::Orders>,
    mut orbit: ResMut<Orbit>,
    mut cam: Single<&mut Transform, (With<MainCamera>, Without<Player>)>,
    course: Res<Course>,
) {
    if !menu.open {
        orbit.yaw -= orders.look.x;
        orbit.pitch = (orbit.pitch + orders.look.y).clamp(-0.3, 1.1);
        if orders.recenter {
            orbit.yaw = (-p.facing.x).atan2(-p.facing.z);
        }
    }
    let focus = p.pos + Vec3::Y * 1.1;
    let offset = Vec3::new(
        orbit.yaw.sin() * orbit.pitch.cos(),
        orbit.pitch.sin(),
        orbit.yaw.cos() * orbit.pitch.cos(),
    ) * orbit.distance;
    let mut distance: f32 = 1.;
    for b in &course.solids {
        let lo = b.center - b.half - Vec3::splat(0.2);
        let hi = b.center + b.half + Vec3::splat(0.2);
        let mut near = 0_f32;
        let mut far = 1_f32;
        for axis in 0..3 {
            if offset[axis].abs() < 0.0001 {
                if focus[axis] < lo[axis] || focus[axis] > hi[axis] {
                    far = -1.;
                    break;
                }
            } else {
                let a = (lo[axis] - focus[axis]) / offset[axis];
                let z = (hi[axis] - focus[axis]) / offset[axis];
                near = near.max(a.min(z));
                far = far.min(a.max(z));
            }
        }
        if near <= far && near > 0. {
            distance = distance.min((near - 0.04).max(0.08));
        }
    }
    let target = focus + offset * distance;
    cam.translation = cam
        .translation
        .lerp(target, 1. - (-time.delta_secs() * 12.).exp());
    cam.look_at(focus, Vec3::Y);
}
fn preview(
    mut commands: Commands,
    menu: Res<crate::ui::Menu>,
    p: Single<(Entity, &Player)>,
    window: Single<&Window>,
    mut camera: Single<(&mut Camera, &mut Transform), (With<PreviewCamera>, Without<PreviewLight>)>,
    mut light: Single<
        (&mut PointLight, &mut Transform),
        (With<PreviewLight>, Without<PreviewCamera>),
    >,
    meshes: Query<
        Entity,
        (
            With<Mesh3d>,
            Without<bevy::camera::visibility::RenderLayers>,
        ),
    >,
    parents: Query<&ChildOf>,
) {
    // Reuse the same character for the menu portrait, with a separate camera layer.
    for e in &meshes {
        let mut at = e;
        for _ in 0..64 {
            if at == p.0 {
                commands
                    .entity(e)
                    .insert(bevy::camera::visibility::RenderLayers::from_layers(&[0, 1]));
                break;
            }
            let Ok(parent) = parents.get(at) else {
                break;
            };
            at = parent.parent();
        }
    }
    camera.0.is_active = menu.open;
    light.0.intensity = if menu.open { 800_000. } else { 0. };
    light.1.translation = p.1.pos + p.1.facing * 3. + Vec3::Y * 3.;
    camera.0.viewport = Some(bevy::camera::Viewport {
        physical_position: UVec2::new((window.physical_width() as f32 * 0.56) as u32, 0),
        physical_size: UVec2::new(
            (window.physical_width() as f32 * 0.44) as u32,
            (window.physical_height() as f32 * 0.92) as u32,
        ),
        ..default()
    });
    camera.1.translation = p.1.pos + p.1.facing * 3.2 + Vec3::Y * 1.5;
    camera.1.look_at(p.1.pos + Vec3::Y * 0.95, Vec3::Y);
}
fn screenshot(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut phase: Local<u8>,
    mut exit: MessageWriter<AppExit>,
) {
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
    if *phase == 0 && time.elapsed_secs() > 5. {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(std::env::var("PJV_SHOT").unwrap()));
        *phase = 1;
    }
    if *phase == 1 && time.elapsed_secs() > 7. {
        exit.write(AppExit::Success);
        *phase = 2;
    }
}
