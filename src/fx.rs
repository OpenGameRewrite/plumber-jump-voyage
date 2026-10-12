//! Original procedural feedback: dust, rings, sparkles, a drop shadow and
//! camera shake, driven by movement events.
use crate::movement::{Course, Fx, Move, Player, Throw};
use bevy::prelude::*;

/// One-off bursts requested by the course itself.
#[derive(Message, Clone, Copy)]
pub enum Burst {
    Spark(Vec3),
    Target(Vec3),
}
#[derive(Component)]
struct Particle {
    vel: Vec3,
    age: f32,
    life: f32,
    size: f32,
    gravity: f32,
    drag: f32,
    /// Grows from a small start, then shrinks away (dust); otherwise only shrinks.
    puff: bool,
    spin: f32,
}
#[derive(Component)]
struct Ring {
    age: f32,
    life: f32,
    radius: f32,
}
#[derive(Component)]
struct Shadow;
#[derive(Resource)]
struct Kit {
    ball: Handle<Mesh>,
    chip: Handle<Mesh>,
    ring: Handle<Mesh>,
    disc: Handle<Mesh>,
    dust: Handle<StandardMaterial>,
    spark: Handle<StandardMaterial>,
    white: Handle<StandardMaterial>,
    water: Handle<StandardMaterial>,
    wave: Handle<StandardMaterial>,
    smoke: Handle<StandardMaterial>,
    gold_wave: Handle<StandardMaterial>,
}
pub fn plugin(app: &mut App) {
    app.add_message::<Burst>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (react, trails, simulate, shadow).after(crate::world::animate),
        );
}
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let unlit = |m: &mut Assets<StandardMaterial>, c: Color, glow: f32, blend: bool| {
        m.add(StandardMaterial {
            base_color: c,
            emissive: c.to_linear() * glow,
            unlit: true,
            alpha_mode: if blend {
                AlphaMode::Blend
            } else {
                AlphaMode::Opaque
            },
            ..default()
        })
    };
    let kit = Kit {
        ball: meshes.add(Sphere::new(0.5).mesh().ico(1).unwrap()),
        chip: meshes.add(Cuboid::from_length(0.5)),
        ring: meshes.add(Torus::new(0.92, 1.)),
        disc: meshes.add(Circle::new(0.5)),
        dust: unlit(
            &mut materials,
            Color::linear_rgba(1.5, 1.45, 1.35, 0.7),
            0.,
            true,
        ),
        spark: unlit(&mut materials, Color::srgb(1., 0.8, 0.25), 6., false),
        white: unlit(&mut materials, Color::srgb(1., 1., 0.95), 4., false),
        water: unlit(&mut materials, Color::srgba(0.55, 0.85, 1., 0.8), 0.5, true),
        wave: unlit(&mut materials, Color::srgba(1., 1., 1., 0.55), 1., true),
        smoke: unlit(&mut materials, Color::srgba(0.25, 0.22, 0.2, 0.6), 0., true),
        gold_wave: unlit(&mut materials, Color::srgba(1., 0.85, 0.3, 0.7), 4., true),
    };
    commands.spawn((
        Shadow,
        Mesh3d(kit.disc.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.02, 0.05, 0.08, 0.38),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        bevy::light::NotShadowCaster,
    ));
    commands.insert_resource(kit);
}
struct Spawner<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    kit: &'a Kit,
    seed: &'a mut u32,
}
impl Spawner<'_, '_, '_> {
    fn rand(&mut self) -> f32 {
        *self.seed ^= *self.seed << 13;
        *self.seed ^= *self.seed >> 17;
        *self.seed ^= *self.seed << 5;
        (*self.seed % 10_000) as f32 / 10_000.
    }
    fn dir(&mut self) -> Vec3 {
        Vec3::new(self.rand() - 0.5, self.rand() - 0.5, self.rand() - 0.5).normalize_or(Vec3::Y)
    }
    #[allow(clippy::too_many_arguments)]
    fn particle(
        &mut self,
        at: Vec3,
        vel: Vec3,
        size: f32,
        life: f32,
        gravity: f32,
        drag: f32,
        kind: u8,
    ) {
        let kit = self.kit;
        let (mesh, mat, puff) = match kind {
            0 => (&kit.ball, &kit.dust, true),
            1 => (&kit.chip, &kit.spark, false),
            2 => (&kit.chip, &kit.white, false),
            4 => (&kit.ball, &kit.wave, false),
            5 => (&kit.ball, &kit.smoke, true),
            _ => (&kit.ball, &kit.water, false),
        };
        let spin = self.rand() * 12. - 6.;
        let life = life * (0.8 + self.rand() * 0.4);
        self.commands.spawn((
            Particle {
                vel,
                age: 0.,
                life,
                size,
                gravity,
                drag,
                puff,
                spin,
            },
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(at).with_scale(Vec3::splat(size * 0.3)),
            bevy::light::NotShadowCaster,
        ));
    }
    /// A ring of dust puffs sliding outward along the ground.
    fn dust_ring(&mut self, at: Vec3, count: usize, speed: f32, size: f32) {
        for i in 0..count {
            let a = i as f32 / count as f32 * std::f32::consts::TAU + self.rand() * 0.4;
            let d = Vec3::new(a.cos(), 0., a.sin());
            let vel = d * speed * (0.7 + self.rand() * 0.6) + Vec3::Y * 0.6;
            let size = size * (0.8 + self.rand() * 0.5);
            self.particle(at + d * 0.3 + Vec3::Y * 0.12, vel, size, 0.45, 0., 5., 0);
        }
    }
    fn sparks(&mut self, at: Vec3, count: usize, speed: f32, kind: u8) {
        for _ in 0..count {
            let d = self.dir();
            self.particle(
                at,
                d * speed + Vec3::Y * speed * 0.4,
                0.22,
                0.6,
                9.,
                2.5,
                kind,
            );
        }
    }
    /// The white sweep around the body when the cap is spun out.
    fn swoosh(&mut self, center: Vec3, facing: Vec3, clockwise: bool, up: bool) {
        let base = facing.x.atan2(facing.z);
        let turn = if clockwise { -1. } else { 1. };
        for i in 0..14 {
            let a = base + turn * (i as f32 / 14. * 5.2 - 1.);
            let (radial, tangent) = (
                Vec3::new(a.sin(), 0., a.cos()),
                Vec3::new(a.cos(), 0., -a.sin()) * turn,
            );
            let height = if up { i as f32 * 0.08 } else { 0. };
            self.particle(
                center + radial * 0.75 + Vec3::Y * height,
                tangent * 6. + radial,
                0.2 + 0.1 * (i as f32 / 14.),
                0.18 + i as f32 * 0.006,
                0.,
                3.,
                2,
            );
        }
    }
    fn ring(&mut self, at: Vec3, radius: f32, life: f32, gold: bool) {
        self.commands.spawn((
            Ring {
                age: 0.,
                life,
                radius,
            },
            Mesh3d(self.kit.ring.clone()),
            MeshMaterial3d(if gold {
                self.kit.gold_wave.clone()
            } else {
                self.kit.wave.clone()
            }),
            Transform::from_translation(at + Vec3::Y * 0.08).with_scale(Vec3::splat(0.1)),
            bevy::light::NotShadowCaster,
        ));
    }
}
fn react(
    mut commands: Commands,
    kit: Res<Kit>,
    p: Single<&Player>,
    mut fx: MessageReader<Fx>,
    mut bursts: MessageReader<Burst>,
    mut shake: ResMut<crate::world::Shake>,
    mut seed: Local<u32>,
) {
    if *seed == 0 {
        *seed = 0x9e37_79b9;
    }
    let mut s = Spawner {
        commands: &mut commands,
        kit: &kit,
        seed: &mut seed,
    };
    let feet = p.pos;
    let back = -p.facing;
    for f in fx.read() {
        match *f {
            Fx::Jump(kind) => {
                // The original puffs smoke on every ground jump clip except the
                // cap leapfrog and the roll jump; wall jumps burst off the wall.
                if matches!(kind, Move::CapBounce | Move::CapVault | Move::RollJump) {
                    continue;
                }
                if kind == Move::WallJump {
                    s.dust_ring(feet + Vec3::Y * 0.9 - p.facing * 0.4, 6, 2.5, 0.4);
                    continue;
                }
                let n = match kind {
                    Move::TripleJump | Move::PoundJump | Move::Backflip => 9,
                    Move::LongJump | Move::RollJump => 7,
                    _ => 5,
                };
                s.dust_ring(feet, n, 3., 0.4);
                if matches!(kind, Move::PoundJump | Move::TripleJump) {
                    s.ring(feet, 1.6, 0.35, false);
                }
            }
            Fx::Land { hard } => {
                s.dust_ring(
                    feet,
                    if hard { 12 } else { 6 },
                    if hard { 5. } else { 3. },
                    0.42,
                );
                if hard {
                    shake.0 = shake.0.max(0.45);
                }
            }
            Fx::Step { .. } if p.flat_speed() > 5. => {
                s.particle(
                    feet + back * 0.2 + Vec3::Y * 0.1,
                    back * 1.2 + Vec3::Y * 0.5,
                    0.4,
                    0.35,
                    0.,
                    4.,
                    0,
                );
            }
            Fx::Skid => s.dust_ring(feet + p.facing * 0.3, 6, 2.5, 0.5),
            Fx::TurnDust => {
                for _ in 0..5 {
                    let side = Vec3::new(s.rand() - 0.5, 0., s.rand() - 0.5) * 0.4;
                    s.particle(
                        feet + back * 0.3 + side + Vec3::Y * 0.1,
                        back * 3. + Vec3::Y,
                        0.5,
                        0.4,
                        0.,
                        4.,
                        0,
                    );
                }
            }
            Fx::PoundStart => s.sparks(feet + Vec3::Y * 0.9, 6, 3., 2),
            Fx::PoundLand => {
                s.dust_ring(feet, 16, 7., 0.75);
                s.ring(feet, 3.2, 0.4, false);
                s.sparks(feet + Vec3::Y * 0.3, 10, 5., 1);
                shake.0 = shake.0.max(0.65);
            }
            Fx::RollStart => s.dust_ring(feet, 6, 3.5, 0.4),
            Fx::RollBoost { perfect } => {
                for _ in 0..4 {
                    let side = Vec3::new(s.rand() - 0.5, 0., s.rand() - 0.5);
                    s.particle(
                        feet + side + Vec3::Y * 0.2,
                        back * 4. + Vec3::Y,
                        0.6,
                        0.4,
                        0.,
                        4.,
                        0,
                    );
                }
                if perfect {
                    s.ring(feet + Vec3::Y * 0.5, 1.8, 0.3, true);
                    s.sparks(feet + Vec3::Y * 0.6, 8, 4., 1);
                }
            }
            Fx::Dive => s.sparks(feet + Vec3::Y * 0.8, 4, 2., 2),
            Fx::Throw(kind) => {
                // The spin that flings the cap leaves a sweep around the body.
                let hand = feet + Vec3::Y * 0.9 + p.facing * 0.4;
                let clockwise = !p.anim.name.contains("Left");
                s.swoosh(
                    feet + Vec3::Y * 0.85,
                    p.facing,
                    clockwise,
                    kind == Throw::Up,
                );
                if kind == Throw::Spin {
                    s.swoosh(feet + Vec3::Y * 0.5, -p.facing, clockwise, false);
                }
                s.sparks(hand, 4, 2.5, 2);
            }
            Fx::Catch => s.sparks(feet + Vec3::Y * 1.6, 5, 1.8, 2),
            Fx::CapBounce => {
                s.ring(feet + Vec3::Y * 0.4, 1.4, 0.3, true);
                s.sparks(feet + Vec3::Y * 0.4, 10, 4., 1);
                shake.0 = shake.0.max(0.25);
            }
            Fx::Ledge => s.sparks(feet + Vec3::Y * 1.6 - back * 0.3, 4, 1.5, 2),
            Fx::Burned(_) => {
                // A burst of embers and smoke from the seat.
                s.sparks(feet + Vec3::Y * 0.5, 12, 4., 1);
                for _ in 0..6 {
                    let d = s.dir() * 0.3;
                    s.particle(
                        feet + Vec3::Y * 0.5 + d,
                        Vec3::Y * 2. + d,
                        0.6,
                        0.7,
                        -1.,
                        2.,
                        5,
                    );
                }
                shake.0 = shake.0.max(0.2);
            }
            Fx::Quenched => {
                // A puff of steam off the seat.
                for _ in 0..10 {
                    let d = s.dir() * 0.4;
                    s.particle(
                        feet + Vec3::Y * 0.5 + d,
                        Vec3::Y * 2.5 + d,
                        0.5,
                        0.8,
                        -1.,
                        2.,
                        0,
                    );
                }
            }
            Fx::Splash => {
                for _ in 0..16 {
                    let d = s.dir().with_y(0.).normalize_or(Vec3::X);
                    let up = 3. + s.rand() * 4.;
                    s.particle(
                        feet + Vec3::Y * 0.6,
                        d * 2.5 + Vec3::Y * up,
                        0.3,
                        0.8,
                        16.,
                        0.5,
                        3,
                    );
                }
                s.ring(feet + Vec3::Y * 0.6, 2., 0.5, false);
            }
            Fx::SwimStroke => {
                for _ in 0..5 {
                    let d = s.dir() * 0.4;
                    s.particle(
                        feet + Vec3::Y * 0.8 + d,
                        Vec3::Y * 1.5 + back,
                        0.18,
                        0.9,
                        -2.,
                        1.,
                        3,
                    );
                }
            }
            _ => {}
        }
    }
    for b in bursts.read() {
        match *b {
            Burst::Spark(at) => {
                s.sparks(at, 14, 5., 1);
                s.ring(at - Vec3::Y * 0.3, 1.2, 0.35, true);
            }
            Burst::Target(at) => {
                s.sparks(at, 16, 6., 2);
                s.ring(at, 2., 0.45, true);
            }
        }
    }
}
/// Continuous effects that follow the player and the cap.
fn trails(
    mut commands: Commands,
    kit: Res<Kit>,
    time: Res<Time>,
    menu: Res<crate::ui::Menu>,
    p: Single<&Player>,
    mut clock: Local<f32>,
    mut seed: Local<u32>,
) {
    if menu.open {
        return;
    }
    if *seed == 0 {
        *seed = 0x1234_5678;
    }
    *clock += time.delta_secs();
    if *clock < 1. / 40. {
        return;
    }
    *clock = 0.;
    let mut s = Spawner {
        commands: &mut commands,
        kit: &kit,
        seed: &mut seed,
    };
    let back = -p.facing;
    match p.mode {
        Move::SlopeSlide if p.grounded => {
            // Kicked out to either side at ground level, clear of the body
            // lying along the slide.
            let side = Vec3::Y.cross(p.facing) * if s.rand() < 0.5 { 0.3 } else { -0.3 };
            s.particle(
                p.pos + side + Vec3::Y * 0.05,
                side * 2. + Vec3::Y * 0.5 + back * 0.5,
                0.3,
                0.35,
                0.,
                4.,
                0,
            );
        }
        Move::Skid | Move::Slide if p.grounded => {
            s.particle(
                p.pos + p.facing * 0.3 + Vec3::Y * 0.1,
                Vec3::Y * 0.8 + back,
                0.5,
                0.4,
                0.,
                4.,
                0,
            );
        }
        Move::Roll => {
            // The rolling ball's blur, and its dust while on the ground.
            s.particle(p.pos + Vec3::Y * 0.45, p.vel * 0.3, 1.3, 0.1, 0., 0., 4);
            if p.grounded && p.flat_speed() > 3. {
                let side = Vec3::new(s.rand() - 0.5, 0., s.rand() - 0.5) * 0.5;
                s.particle(
                    p.pos + side + Vec3::Y * 0.1,
                    back * 2. + Vec3::Y * 0.6,
                    0.45,
                    0.35,
                    0.,
                    4.,
                    0,
                );
            }
        }
        Move::BurnJump | Move::BurnBounce | Move::FireRun => {
            // Smoke trailing from the seat, with the odd ember.
            let hip = p.pos + Vec3::Y * 0.55 - p.facing * 0.25;
            let d = s.dir() * 0.1;
            s.particle(hip + d, Vec3::Y * 1.2 - p.facing + d, 0.45, 0.6, -1., 2., 5);
            if s.rand() < 0.3 {
                let up = Vec3::Y * 2. + s.dir();
                s.particle(hip, up, 0.12, 0.4, 6., 1., 1);
            }
        }
        Move::SpinDrop | Move::GroundSpin => {
            let d = s.dir().with_y(0.).normalize_or(Vec3::X) * 0.6;
            s.particle(
                p.pos + Vec3::Y * 0.6 + d,
                Vec3::new(d.z, 0., -d.x) * 4.,
                0.16,
                0.2,
                0.,
                2.,
                2,
            );
        }
        Move::WallSlide => {
            s.particle(
                p.pos + Vec3::Y * 1.3 - p.wall * 0.3,
                Vec3::Y * 0.6,
                0.3,
                0.3,
                0.,
                4.,
                0,
            );
        }
        Move::TripleJump
        | Move::Backflip
        | Move::Sideflip
        | Move::SpinJump
        | Move::PoundJump
        | Move::CapVault
        | Move::CapBounce
            if !p.grounded && p.timer < 0.7 =>
        {
            let d = s.dir() * 0.3;
            s.particle(p.pos + Vec3::Y * 0.9 + d, Vec3::ZERO, 0.14, 0.4, 0., 0., 2);
        }
        Move::LongJump | Move::Dive | Move::RollJump if p.flat_speed() > 11. => {
            s.particle(p.pos + Vec3::Y * 0.8, Vec3::ZERO, 0.12, 0.25, 0., 0., 2);
        }
        _ => {}
    }
    if let Some(cap) = &p.cap {
        let d = s.dir() * 0.15;
        s.particle(cap.pos + d, Vec3::ZERO, 0.12, 0.25, 0., 0., 2);
    }
}
fn simulate(
    mut commands: Commands,
    time: Res<Time>,
    menu: Res<crate::ui::Menu>,
    mut particles: Query<(Entity, &mut Particle, &mut Transform), Without<Ring>>,
    mut rings: Query<(Entity, &mut Ring, &mut Transform), Without<Particle>>,
) {
    let dt = if menu.open {
        0.
    } else {
        time.delta_secs().min(0.05)
    };
    for (e, mut p, mut t) in &mut particles {
        p.age += dt;
        if p.age >= p.life {
            commands.entity(e).despawn();
            continue;
        }
        p.vel.y -= p.gravity * dt;
        let drag = (1. - p.drag * dt).max(0.);
        p.vel *= drag;
        t.translation += p.vel * dt;
        t.rotate_local_y(p.spin * dt);
        t.rotate_local_x(p.spin * 0.7 * dt);
        let k = p.age / p.life;
        let s = if p.puff {
            p.size * (0.4 + 0.9 * k.sqrt()) * (1. - k * k)
        } else {
            p.size * (1. - k)
        };
        t.scale = Vec3::splat(s.max(0.001));
    }
    for (e, mut r, mut t) in &mut rings {
        r.age += dt;
        if r.age >= r.life {
            commands.entity(e).despawn();
            continue;
        }
        let k = r.age / r.life;
        let grow = r.radius * (1. - (1. - k) * (1. - k));
        t.scale = Vec3::new(grow, 0.6 * (1. - k) + 0.02, grow);
    }
}
fn shadow(
    p: Single<&Player>,
    course: Res<Course>,
    mut shadow: Single<(&mut Transform, &mut Visibility), With<Shadow>>,
) {
    let Some(ground) = course.ground(p.pos + Vec3::Y * 0.05) else {
        *shadow.1 = Visibility::Hidden;
        return;
    };
    let height = p.pos.y - ground;
    if course.wet(Vec3::new(p.pos.x, ground + 0.1, p.pos.z)) && height > 0. {
        *shadow.1 = Visibility::Hidden;
        return;
    }
    *shadow.1 = Visibility::Visible;
    shadow.0.translation = Vec3::new(p.shown.x, ground + 0.03, p.shown.z);
    let size = (0.95 - height * 0.04).clamp(0.45, 0.95);
    shadow.0.scale = Vec3::splat(size);
}
pub fn self_check() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<crate::world::Shake>()
        .add_message::<Fx>()
        .add_message::<Burst>();
    let mut menu = crate::ui::Menu::default();
    menu.open = false;
    app.insert_resource(menu);
    app.add_systems(Startup, setup)
        .add_systems(Update, (react, simulate).chain());
    app.world_mut().spawn(Player::default());
    app.update();
    app.world_mut().write_message(Fx::PoundLand);
    app.update();
    let n = app
        .world_mut()
        .query::<&Particle>()
        .iter(app.world())
        .count();
    assert!(n > 10, "a pound kicks up dust and sparks");
    assert!(app.world().resource::<crate::world::Shake>().0 > 0.5);
}
