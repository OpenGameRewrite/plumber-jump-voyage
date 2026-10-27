use crate::look;
use crate::movement::{Course, Fx, Move, Player, Pole, SPAWN, Solid};
use bevy::prelude::*;

#[derive(Resource)]
pub struct Orbit {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    /// Height the camera frames; it holds still through ordinary jumps.
    anchor: f32,
    focus: Vec3,
    recenter: f32,
    idle: f32,
    zoom: f32,
    fov: f32,
    /// A rendered replay sets the yaw itself; input and follow leave it be.
    pub locked: bool,
}
impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: 0.,
            pitch: 0.32,
            distance: 9.,
            anchor: SPAWN.y,
            focus: SPAWN + Vec3::Y * 1.2,
            recenter: 0.,
            idle: 0.,
            zoom: 1.,
            fov: 55_f32.to_radians(),
            locked: false,
        }
    }
}
/// Camera shake amount; effects add to it and it decays.
#[derive(Resource, Default)]
pub struct Shake(pub f32);
#[derive(Component)]
pub struct MainCamera;
#[derive(Component)]
struct PreviewCamera;
#[derive(Component)]
struct PreviewLight;
/// Visual root under the player: lean and squash for every outfit.
#[derive(Component)]
pub struct Pivot;
#[derive(Component)]
pub struct Body;
#[derive(Component)]
struct RecoveryBubble;
#[derive(Component)]
struct DeathWipe;
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
        .init_resource::<Shake>()
        .add_systems(Startup, (setup, setup_death_presentation).chain())
        .add_systems(PreUpdate, moving_platforms.after(crate::input::read))
        .add_systems(
            Update,
            (animate, collect, camera, preview, death_presentation).chain(),
        );
    if std::env::var_os("PJV_SHOT").is_some() {
        app.add_systems(Update, screenshot);
    }
}
fn material(m: &mut Assets<StandardMaterial>, rgb: [f32; 3]) -> Handle<StandardMaterial> {
    // The game-data build lights the course with the game's strong daylight;
    // the course's own colours were chosen for the dimmer ROMless light.
    let k = look::course_albedo();
    m.add(StandardMaterial {
        base_color: Color::srgb(rgb[0] * k, rgb[1] * k, rgb[2] * k),
        perceptual_roughness: 0.62,
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
/// Original gradient sky as a small cubemap: deep blue overhead, a pale
/// horizon with a soft sun glow, and a muted ground bounce below.
pub fn sky_cubemap(size: u32) -> Image {
    use bevy::render::render_resource::{
        Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
    };
    let sun = Vec3::new(10., 18., 8.).normalize();
    let mut data = Vec::with_capacity((size * size * 6 * 4) as usize);
    for face in 0..6 {
        for y in 0..size {
            for x in 0..size {
                let u = (x as f32 + 0.5) / size as f32 * 2. - 1.;
                let v = (y as f32 + 0.5) / size as f32 * 2. - 1.;
                let d = match face {
                    0 => Vec3::new(1., -v, -u),
                    1 => Vec3::new(-1., -v, u),
                    2 => Vec3::new(u, 1., v),
                    3 => Vec3::new(u, -1., -v),
                    4 => Vec3::new(u, -v, 1.),
                    _ => Vec3::new(-u, -v, -1.),
                }
                .normalize();
                let zenith = Vec3::new(0.22, 0.52, 0.93);
                let horizon = Vec3::new(0.80, 0.91, 0.98);
                let ground = Vec3::new(0.55, 0.60, 0.52);
                let mut c = if d.y >= 0. {
                    horizon.lerp(zenith, d.y.powf(0.55))
                } else {
                    horizon.lerp(ground, (-d.y * 3.).min(1.))
                };
                c += Vec3::new(1., 0.92, 0.75) * d.dot(sun).max(0.).powf(48.) * 0.6;
                for k in 0..3 {
                    data.push((c[k].clamp(0., 1.) * 255.) as u8);
                }
                data.push(255);
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image
}
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut course: ResMut<Course>,
    replay: Option<Res<crate::shots::Driver>>,
) {
    let cream = material(&mut materials, [0.93, 0.86, 0.68]);
    let white = material(&mut materials, [0.97, 0.96, 0.89]);
    let red = material(&mut materials, [0.86, 0.30, 0.20]);
    let teal = material(&mut materials, [0.10, 0.52, 0.53]);
    let dark = material(&mut materials, [0.06, 0.18, 0.23]);
    let gold = materials.add(StandardMaterial {
        base_color: Color::srgb(1., 0.72, 0.18),
        emissive: LinearRgba::rgb(0.9, 0.5, 0.05),
        metallic: 0.6,
        perceptual_roughness: 0.3,
        ..default()
    });
    let skin = material(&mut materials, [0.67, 0.87, 0.85]);
    let grass = material(&mut materials, [0.47, 0.76, 0.33]);
    commands.insert_resource(Palette(teal.clone()));
    let sky = images.add(sky_cubemap(128));
    let rig = if cfg!(feature = "local-assets") {
        look::game()
    } else {
        look::romless(replay.is_some())
    };
    let environment = rig
        .environment
        .map_or_else(|| sky.clone(), |image| images.add(image));
    commands.insert_resource(GlobalAmbientLight {
        color: rig.ambient,
        brightness: rig.ambient_brightness,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            illuminance: rig.sun_lux,
            color: rig.sun_color,
            shadow_maps_enabled: true,
            ..default()
        },
        bevy::light::CascadeShadowConfigBuilder {
            maximum_distance: 60.,
            first_cascade_far_bound: 12.,
            ..default()
        }
        .build(),
        bevy::camera::visibility::RenderLayers::from_layers(&[0, 1, look::SPRITE_SHADOW_LAYER]),
        Transform::from_translation(rig.sun_from).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let camera = commands
        .spawn((
            Camera3d::default(),
            MainCamera,
            IsDefaultUiCamera,
            bevy::camera::Hdr,
            Projection::Perspective(PerspectiveProjection {
                fov: 55_f32.to_radians(),
                ..default()
            }),
            rig.tonemapping,
            bevy::post_process::bloom::Bloom {
                intensity: rig.bloom,
                ..bevy::post_process::bloom::Bloom::NATURAL
            },
            bevy::light::Skybox {
                image: Some(sky.clone()),
                brightness: rig.skybox_brightness,
                ..default()
            },
            bevy::light::GeneratedEnvironmentMapLight {
                environment_map: environment,
                intensity: rig.environment_intensity,
                ..default()
            },
            bevy::pbr::DistanceFog {
                color: Color::srgba(0.80, 0.90, 0.98, 0.9),
                falloff: bevy::pbr::FogFalloff::Linear {
                    start: 45.,
                    end: 160.,
                },
                ..default()
            },
            Transform::from_xyz(0., 5., 14.).looking_at(Vec3::Y, Vec3::Y),
        ))
        .id();
    if let Some(curve) = rig.tone {
        commands.entity(camera).insert(curve);
    }
    if rig.hard_shadows {
        commands
            .entity(camera)
            .insert(bevy::light::ShadowFilteringMethod::Hardware2x2);
    }
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 2,
            is_active: false,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        // Same HDR target as the main camera; otherwise this uncleared
        // overlay draws into its own texture and smears old frames. The main
        // camera already tonemapped what lies underneath.
        bevy::camera::Hdr,
        bevy::core_pipeline::tonemapping::Tonemapping::None,
        bevy::camera::visibility::RenderLayers::layer(1),
        PreviewCamera,
        bevy::light::GeneratedEnvironmentMapLight {
            environment_map: sky,
            intensity: 1400.,
            ..default()
        },
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
    if replay.is_some() {
        // A rendered replay runs on the same flat floor as the headless one,
        // bare and dirt-coloured like the plateau the real game's reference
        // pictures were taken on, so only Mario and his effects draw the eye.
        *course = crate::replay::flat_floor();
        let [r, g, b] = look::replay_soil();
        // Dry earth: matte, with no sheen of the sky on it.
        let soil = materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            perceptual_roughness: 1.,
            reflectance: 0.,
            emissive: look::replay_soil_glow(),
            ..default()
        });
        block(
            &mut commands,
            &mut meshes,
            soil,
            Vec3::new(0., -0.5, 0.),
            Vec3::new(4000., 1., 4000.),
        );
        spawn_player(&mut commands, &mut meshes, teal, skin, dark, white, gold);
        return;
    }
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
                // Newell's normal stays valid when a ramp's end edge collapses
                // two corners of a side face into one point.
                let n = (0..4)
                    .map(|k| {
                        let a = Vec3::from_array(points[f[k]]);
                        let b = Vec3::from_array(points[f[(k + 1) % 4]]);
                        a.cross(b)
                    })
                    .sum::<Vec3>()
                    .normalize_or(Vec3::Y);
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
        grass,
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
    // A 35-degree slide slope: steep enough to crouch-slide down (26 degrees or more).
    add(
        Vec3::new(11., 1.4, 4.),
        Vec3::new(8., 2.8, 4.),
        cream.clone(),
        -0.7,
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
    // (The pool's water in centimetres: centre, half sizes, surface height and depth.)
    course.water = crate::water::WaterAreas::demo_box(900., 2500., 600., 500., 0., 400.);
    // Static Needle fixture: the box collider feeds the same floor code as a
    // loaded KCL triangle. The small cones show its hazardous footprint.
    let needle_center = Vec3::new(-4., 0., -6.);
    course
        .needles
        .push((needle_center, Vec3::new(1.5, 0., 1.25)));
    for x in -3..=3 {
        for z in -2..=2 {
            commands.spawn((
                Mesh3d(meshes.add(Cone {
                    radius: 0.14,
                    height: 0.35,
                })),
                MeshMaterial3d(dark.clone()),
                Transform::from_translation(
                    needle_center + Vec3::new(x as f32 * 0.4, 0.175, z as f32 * 0.4),
                ),
            ));
        }
    }
    // A lava plate: touching it burns, as hot ground does in the original.
    let lava = materials.add(StandardMaterial {
        base_color: Color::srgb(0.95, 0.32, 0.05),
        emissive: LinearRgba::rgb(6., 1.6, 0.2),
        ..default()
    });
    let lava_center = Vec3::new(4., 0.1, -1.);
    let lava_half = Vec3::new(1.5, 0.1, 1.25);
    course.solids.push(Solid {
        center: lava_center,
        half: lava_half,
        slope: 0.,
        moving: false,
        slippery: false,
    });
    course
        .hot
        .push((lava_center + Vec3::Y * lava_half.y, lava_half));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::from_size(lava_half * 2.))),
        MeshMaterial3d(lava),
        Transform::from_translation(lava_center),
    ));
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
        water.clone(),
        Vec3::new(9., -0.05, 25.),
        Vec3::new(13., 0.1, 11.),
    );
    // Shallow water past the lava: running into it puts a burn out.
    let puddle_center = Vec3::new(14.5, 0.02, -1.);
    let puddle_half = Vec3::new(1.6, 0.02, 1.3);
    course.puddles.push((puddle_center, puddle_half));
    block(
        &mut commands,
        &mut meshes,
        water.clone(),
        puddle_center,
        puddle_half * 2.,
    );
    for c in [Vec3::new(0., 0., -8.), Vec3::new(-17., 0., -11.)] {
        course.poles.push(Pole {
            center: c,
            height: 6.,
        });
        commands.spawn((
            Mesh3d(meshes.add(Cylinder::new(crate::movement::POLE_CIRCUMRADIUS * 0.01, 6.))),
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
    spawn_player(&mut commands, &mut meshes, teal, skin, dark, white, gold);
}
fn spawn_player(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    teal: Handle<StandardMaterial>,
    skin: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
    white: Handle<StandardMaterial>,
    gold: Handle<StandardMaterial>,
) {
    // PJV_SPAWN=x,y,z (debugging) moves the start and checkpoint.
    let spawn = std::env::var("PJV_SPAWN").ok().and_then(|s| {
        let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]))
    });
    let mut start = Player::default();
    if let Some(at) = spawn {
        start.checkpoint = at;
        start.reset();
    }
    start.enable_assist_recovery(std::env::var_os("PJV_ASSIST_RECOVERY").is_some());
    let player = commands
        .spawn((
            start,
            Transform::from_translation(SPAWN),
            Visibility::default(),
        ))
        .id();
    let pivot = commands
        .spawn((Pivot, Transform::default(), Visibility::default()))
        .id();
    commands.entity(player).add_child(pivot);
    let root = commands
        .spawn((Body, Transform::default(), Visibility::default()))
        .id();
    commands.entity(pivot).add_child(root);
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
        let e = block(commands, meshes, mat, base, size);
        commands.entity(e).insert(Part { kind, base });
        commands.entity(root).add_child(e);
    }
    // Single glowing visor instead of a familiar plumber face.
    let visor = block(
        commands,
        meshes,
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

fn setup_death_presentation(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    player: Single<Entity, With<Player>>,
) {
    // Presentation adapter: a translucent shell stands in for TractorBubble's child actor.
    let bubble = commands
        .spawn((
            RecoveryBubble,
            Mesh3d(meshes.add(Sphere::new(1.))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.35, 0.75, 1., 0.2),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 0.12,
                ..default()
            })),
            Transform::from_xyz(0., 0.8, 0.),
            Visibility::Hidden,
        ))
        .id();
    commands.entity(*player).add_child(bubble);
    commands.spawn((
        DeathWipe,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(Color::NONE),
        GlobalZIndex(100),
    ));
}

fn death_presentation(
    player: Single<&Player>,
    mut bubble: Single<&mut Visibility, With<RecoveryBubble>>,
    mut wipe: Single<&mut BackgroundColor, With<DeathWipe>>,
) {
    **bubble = if player.recovery_bubble() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    wipe.0 = Color::srgba(0., 0., 0., player.death.wipe_alpha());
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
fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}
/// Presentation state that the simulation does not need.
#[derive(Default)]
pub struct Visual {
    yaw: Option<f32>,
    bank: f32,
    pitch: f32,
    squash: f32,
    squash_vel: f32,
    speed: f32,
}
pub fn animate(
    time: Res<Time>,
    mut fx: MessageReader<Fx>,
    mut vis: Local<Visual>,
    p: Single<(&Player, &mut Transform, &mut Visibility)>,
    mut pivot: Single<&mut Transform, (With<Pivot>, Without<Player>, Without<Body>)>,
    mut body: Single<
        &mut Transform,
        (
            With<Body>,
            Without<Player>,
            Without<Pivot>,
            Without<Part>,
            Without<Hat>,
        ),
    >,
    mut parts: Query<
        (&Part, &mut Transform),
        (Without<Player>, Without<Body>, Without<Pivot>, Without<Hat>),
    >,
    mut hat: Single<
        (&mut Transform, &mut Visibility),
        (
            With<Hat>,
            Without<Player>,
            Without<Body>,
            Without<Pivot>,
            Without<Part>,
        ),
    >,
) {
    let dt = time.delta_secs().clamp(1e-4, 0.05);
    let (p, mut t, mut visibility) = p.into_inner();
    *visibility = if p.damage_hidden() {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    for f in fx.read() {
        match f {
            Fx::Jump(_) | Fx::CapBounce => vis.squash_vel += 3.2,
            Fx::Land { hard } => vis.squash_vel -= if *hard { 5.5 } else { 3. },
            Fx::RollLand => vis.squash_vel -= 3.,
            Fx::PoundLand => vis.squash = -0.28,
            Fx::RollBoost { .. } => vis.squash_vel -= 2.,
            _ => {}
        }
    }
    // Damped spring: a quick squash on landing, a stretch on take-off.
    vis.squash_vel += (-180. * vis.squash - 14. * vis.squash_vel) * dt;
    vis.squash = (vis.squash + vis.squash_vel * dt).clamp(-0.3, 0.3);
    t.translation = p.shown;
    let target = (-p.facing.x).atan2(-p.facing.z);
    let yaw = vis.yaw.unwrap_or(target);
    let snap = matches!(
        p.mode,
        Move::WallJump | Move::Ledge | Move::Pole | Move::Sideflip
    );
    let step = wrap(target - yaw) * if snap { 1. } else { 1. - (-dt * 22.).exp() };
    let next = yaw + step;
    let turn_rate = step / dt;
    vis.yaw = Some(next);
    t.rotation = Quat::from_rotation_y(next);
    let speed = p.flat_speed();
    let accel = (speed - vis.speed) / dt;
    vis.speed = speed;
    let leaning = p.grounded && matches!(p.mode, Move::Walk | Move::Run | Move::Roll);
    let bank = if leaning {
        (turn_rate * speed * 0.006).clamp(-0.32, 0.32)
    } else {
        0.
    };
    let pitch = if leaning && p.mode != Move::Roll {
        (-accel * 0.006).clamp(-0.12, 0.18)
    } else {
        0.
    };
    let blend = 1. - (-dt * 10.).exp();
    vis.bank += (bank - vis.bank) * blend;
    vis.pitch += (pitch - vis.pitch) * blend;
    pivot.rotation = Quat::from_rotation_z(vis.bank) * Quat::from_rotation_x(-vis.pitch);
    pivot.scale = Vec3::new(
        1. - vis.squash * 0.5,
        1. + vis.squash,
        1. - vis.squash * 0.5,
    );
    let phase = time.elapsed_secs() * speed * 1.3;
    body.rotation = match p.mode {
        Move::Roll => Quat::from_rotation_x(-p.distance * 2.5),
        Move::Dive => Quat::from_rotation_x(-1.3),
        Move::SlopeSlide => Quat::from_rotation_x(0.5),
        Move::Backflip => Quat::from_rotation_x((p.timer * 7.).min(std::f32::consts::TAU)),
        Move::Sideflip => Quat::from_rotation_z((p.timer * 7.).min(std::f32::consts::TAU)),
        Move::GroundPound if p.timer < 0.28 => {
            Quat::from_rotation_x(-p.timer / 0.28 * std::f32::consts::TAU)
        }
        Move::TripleJump | Move::SpinJump => Quat::from_rotation_y(p.timer * 12.),
        Move::LongJump | Move::RollJump => Quat::from_rotation_x(-0.6),
        _ => Quat::IDENTITY,
    };
    // The actor sits on a held edge or pole; the explorer hangs below/behind it.
    body.translation = match p.mode {
        Move::SquatBrake | Move::Crouch | Move::Crawl | Move::Roll | Move::PoundLand => {
            Vec3::Y * -0.45
        }
        Move::Ledge if p.anim.name != "WallCatchEnd" => Vec3::new(0., -1.45, 0.25),
        Move::Pole => Vec3::new(0., 0., 0.3),
        _ => Vec3::ZERO,
    };
    body.scale = if matches!(
        p.mode,
        Move::SquatBrake | Move::Crouch | Move::Crawl | Move::PoundLand
    ) {
        Vec3::new(1., 0.7, 1.)
    } else if p.mode == Move::Roll {
        Vec3::splat(0.75)
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
        hat.0.translation = p.shown_cap.unwrap_or(cap.pos);
        hat.0.rotation = Quat::from_rotation_y(time.elapsed_secs() * 18.);
        hat.0.scale = Vec3::splat(1.2);
    } else {
        hat.0.translation = p.shown + Vec3::Y * 1.75;
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
    mut bursts: MessageWriter<crate::fx::Burst>,
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
        t.rotation = Quat::from_rotation_y(time.elapsed_secs() * 2.);
        if progress.gems.contains(&g.id) {
            *v = Visibility::Hidden;
            continue;
        }
        if (p.pos + Vec3::Y * 0.8).distance(t.translation) < 1.2 {
            progress.gems.push(g.id);
            *v = Visibility::Hidden;
            progress.message = "Sun spark found!".into();
            progress.message_time = 2.;
            bursts.write(crate::fx::Burst::Spark(t.translation));
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
            bursts.write(crate::fx::Burst::Target(t.home));
        }
    }
    if p.pos.distance(Vec3::new(-41., 2.5, -8.)) < 3. && p.checkpoint != Vec3::new(-41., 2.5, -8.) {
        p.checkpoint = Vec3::new(-41., 2.5, -8.);
        progress.message = "Checkpoint!".into();
        progress.message_time = 1.5;
        bursts.write(crate::fx::Burst::Target(p.pos + Vec3::Y));
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
    mut shake: ResMut<Shake>,
    mut cam: Single<(&mut Transform, &mut Projection), (With<MainCamera>, Without<Player>)>,
    course: Res<Course>,
    mut seen_restart: Local<u32>,
) {
    let dt = time.delta_secs().clamp(1e-4, 0.05);
    if *seen_restart != p.death.generation {
        // A new life starts with the camera at the player (the original restarts its camera
        // with the scene), not easing up from where the fall ended.
        *seen_restart = p.death.generation;
        orbit.anchor = p.pos.y;
        orbit.focus = p.pos + Vec3::Y * 1.25;
    }
    let behind = (-p.facing.x).atan2(-p.facing.z);
    let speed = p.flat_speed();
    if !menu.open && !orbit.locked {
        orbit.yaw -= orders.look.x;
        orbit.pitch = (orbit.pitch + orders.look.y).clamp(-0.35, 1.15);
        if orders.look.length_squared() > 1e-7 {
            orbit.idle = 0.;
            orbit.recenter = 0.;
        } else {
            orbit.idle += dt;
        }
        if orders.recenter {
            orbit.recenter = 0.35;
        }
        if orbit.recenter > 0. {
            orbit.recenter -= dt;
            let k = 1. - (-dt * 16.).exp();
            orbit.yaw += wrap(behind - orbit.yaw) * k;
            orbit.pitch += (0.32 - orbit.pitch) * k;
        } else if orbit.idle > 1. && p.grounded && speed > 2. {
            // Gentle follow while running sideways; never swing toward the camera.
            let d = wrap(behind - orbit.yaw);
            if d.abs() < 2.2 {
                orbit.yaw += d * dt * 0.5 * (speed / crate::movement::RUN).min(1.);
            }
        }
    }
    // Ground-anchored framing: ordinary jumps keep the horizon steady; long
    // falls and big climbs are followed.
    let settled = p.grounded
        || matches!(
            p.mode,
            Move::Ledge | Move::Pole | Move::Swim | Move::WallSlide
        );
    let target = if settled || p.pos.y < orbit.anchor - 0.3 {
        p.pos.y
    } else if p.pos.y > orbit.anchor + 3.2 {
        p.pos.y - 3.2
    } else {
        orbit.anchor
    };
    let rate = if p.pos.y < orbit.anchor { 9. } else { 5. };
    orbit.anchor += (target - orbit.anchor) * (1. - (-dt * rate).exp());
    let ahead = (p.vel.with_y(0.) * 0.12).clamp_length_max(1.5);
    let focus_target = Vec3::new(p.shown.x, orbit.anchor + 1.25, p.shown.z) + ahead;
    orbit.focus = orbit.focus.lerp(focus_target, 1. - (-dt * 9.).exp());
    if orbit.focus.distance(focus_target) > 12. {
        orbit.focus = focus_target;
    }
    let focus = orbit.focus;
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
    // Pull in instantly when blocked, ease back out when clear.
    orbit.zoom = if distance < orbit.zoom {
        distance
    } else {
        orbit.zoom + (distance - orbit.zoom) * (1. - (-dt * 4.).exp())
    };
    let fast = ((speed - crate::movement::RUN) / 10.).clamp(0., 1.);
    let fov = (55. + 9. * fast).to_radians();
    orbit.fov += (fov - orbit.fov) * (1. - (-dt * 4.).exp());
    if let Projection::Perspective(proj) = cam.1.as_mut() {
        proj.fov = orbit.fov;
    }
    shake.0 = (shake.0 - dt * 2.2).max(0.);
    let t = time.elapsed_secs() * 47.;
    let jitter = Vec3::new(t.sin(), (t * 1.31).cos(), (t * 0.77).sin()) * shake.0 * shake.0 * 0.35;
    cam.0.translation = focus + offset * orbit.zoom + jitter;
    cam.0.look_at(focus + jitter * 0.5, Vec3::Y);
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
/// Development aid: `PJV_SHOT=out.png PJV_SHOT_AT=1.2,2.5` saves numbered
/// screenshots at those seconds, then exits.
fn screenshot(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut taken: Local<usize>,
    mut exit: MessageWriter<AppExit>,
) {
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
    let times: Vec<f32> = std::env::var("PJV_SHOT_AT")
        .unwrap_or_else(|_| "5".into())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let path = std::env::var("PJV_SHOT").unwrap_or_default();
    if let Some(at) = times.get(*taken) {
        if time.elapsed_secs() > *at {
            let file = if times.len() > 1 {
                path.replace(".png", &format!("-{}.png", *taken))
            } else {
                path
            };
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(file));
            *taken += 1;
        }
    } else if time.elapsed_secs() > times.last().copied().unwrap_or(0.) + 2. {
        exit.write(AppExit::Success);
    }
}
