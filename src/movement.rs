//! Original movement approximation. Metres, seconds, Y up; no copied game code.
use crate::input::Orders;
use bevy::prelude::*;

pub const RADIUS: f32 = 0.35;
pub const HEIGHT: f32 = 1.65;
pub const SPAWN: Vec3 = Vec3::new(0.0, 0.0, 5.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Move {
    #[default]
    Idle,
    Walk,
    Run,
    Crouch,
    Crawl,
    Jump,
    DoubleJump,
    TripleJump,
    LongJump,
    Backflip,
    Sideflip,
    SpinJump,
    GroundPound,
    PoundJump,
    Roll,
    RollJump,
    Dive,
    Slide,
    WallSlide,
    WallJump,
    Ledge,
    Pole,
    Swim,
    SwimDash,
    CapBounce,
    CapVault,
    CatchJump,
}
impl Move {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Ready",
            Self::Walk => "Walk",
            Self::Run => "Run",
            Self::Crouch => "Crouch",
            Self::Crawl => "Crawl",
            Self::Jump => "Jump",
            Self::DoubleJump => "Double jump",
            Self::TripleJump => "Triple jump",
            Self::LongJump => "Long jump",
            Self::Backflip => "Backflip",
            Self::Sideflip => "Side somersault",
            Self::SpinJump => "Spin jump",
            Self::GroundPound => "Ground pound",
            Self::PoundJump => "Ground pound jump",
            Self::Roll => "Roll",
            Self::RollJump => "Roll jump",
            Self::Dive => "Dive",
            Self::Slide => "Belly slide",
            Self::WallSlide => "Wall slide",
            Self::WallJump => "Wall jump",
            Self::Ledge => "Ledge hang",
            Self::Pole => "Pole climb",
            Self::Swim => "Swim",
            Self::SwimDash => "Swim dash",
            Self::CapBounce => "Cap bounce",
            Self::CapVault => "Cap vault",
            Self::CatchJump => "Catch jump",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Throw {
    Forward,
    Up,
    Down,
    Spin,
}
#[derive(Clone, Debug)]
pub struct Cap {
    pub pos: Vec3,
    pub dir: Vec3,
    pub age: f32,
    pub returning: bool,
    pub kind: Throw,
}
#[derive(Component, Debug)]
pub struct Player {
    pub pos: Vec3,
    pub vel: Vec3,
    pub facing: Vec3,
    pub mode: Move,
    pub grounded: bool,
    pub cap: Option<Cap>,
    pub bounced: bool,
    pub chain: u8,
    pub since_land: f32,
    pub pound_window: f32,
    pub catch_window: f32,
    pub catch_used: bool,
    pub coyote: f32,
    pub jump_buffer: f32,
    pub wall: Vec3,
    pub ledge: Vec3,
    pub timer: f32,
    pub spin: f32,
    pub spin_turn: f32,
    pub last_stick: Vec2,
    pub distance: f32,
    pub checkpoint: Vec3,
}
impl Default for Player {
    fn default() -> Self {
        Self {
            pos: SPAWN,
            vel: Vec3::ZERO,
            facing: Vec3::NEG_Z,
            mode: Move::Idle,
            grounded: true,
            cap: None,
            bounced: false,
            chain: 0,
            since_land: 5.,
            pound_window: 0.,
            catch_window: 0.,
            catch_used: false,
            coyote: 0.1,
            jump_buffer: 0.,
            wall: Vec3::ZERO,
            ledge: Vec3::ZERO,
            timer: 0.,
            spin: 0.,
            spin_turn: 0.,
            last_stick: Vec2::ZERO,
            distance: 0.,
            checkpoint: SPAWN,
        }
    }
}
#[derive(Clone)]
pub struct Solid {
    pub center: Vec3,
    pub half: Vec3,
    pub slope: f32,
    pub moving: bool,
    pub slippery: bool,
}
impl Solid {
    pub fn top(&self, x: f32) -> f32 {
        self.center.y + self.half.y + (x - self.center.x) * self.slope
    }
}
#[derive(Clone)]
pub struct Pole {
    pub center: Vec3,
    pub height: f32,
}
#[derive(Resource, Default)]
pub struct Course {
    pub solids: Vec<Solid>,
    pub poles: Vec<Pole>,
    pub water: Option<(Vec3, Vec3)>,
    pub targets: Vec<Vec3>,
}
impl Course {
    pub fn wet(&self, pos: Vec3) -> bool {
        self.water.is_some_and(|(c, h)| {
            (pos.x - c.x).abs() < h.x && (pos.z - c.z).abs() < h.z && pos.y < c.y
        })
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Course>()
        .add_systems(FixedUpdate, simulate);
}
fn simulate(
    mut players: Query<&mut Player>,
    mut orders: ResMut<Orders>,
    course: Res<Course>,
    time: Res<Time<Fixed>>,
    menu: Res<crate::ui::Menu>,
) {
    if menu.open {
        orders.clear_edges();
        return;
    }
    for mut p in &mut players {
        p.step(&orders, &course, time.delta_secs());
    }
    orders.clear_edges();
}
impl Player {
    fn launch(&mut self, kind: Move, height: f32, speed: Option<f32>) {
        self.mode = kind;
        self.vel.y = height;
        if let Some(s) = speed {
            self.vel.x = self.facing.x * s;
            self.vel.z = self.facing.z * s;
        }
        self.grounded = false;
        self.coyote = 0.;
        self.timer = 0.;
        self.jump_buffer = 0.;
    }
    pub fn reset(&mut self) {
        let checkpoint = self.checkpoint;
        *self = Self::default();
        self.pos = checkpoint;
        self.checkpoint = checkpoint;
    }
    pub fn step(&mut self, o: &Orders, scene: &Course, dt: f32) {
        if !dt.is_finite() || dt <= 0. || dt > 0.05 {
            return;
        }
        let mut guarded = o.clone();
        if self.grounded
            && scene.solids.iter().any(|b| {
                let bottom = b.center.y - b.half.y;
                bottom > self.pos.y + 0.2
                    && bottom < self.pos.y + HEIGHT
                    && (self.pos.x - b.center.x).abs() < b.half.x + RADIUS
                    && (self.pos.z - b.center.z).abs() < b.half.z + RADIUS
            })
        {
            guarded.crouch = true;
            guarded.jump = false;
            self.jump_buffer = 0.;
        }
        let o = &guarded;
        self.timer += dt;
        self.since_land += dt;
        self.pound_window = (self.pound_window - dt).max(0.);
        self.catch_window = (self.catch_window - dt).max(0.);
        self.spin = (self.spin - dt).max(0.);
        self.jump_buffer = (self.jump_buffer - dt).max(0.);
        if o.jump {
            self.jump_buffer = 0.12;
        }
        if self.grounded {
            self.coyote = 0.09;
        } else {
            self.coyote = (self.coyote - dt).max(0.);
        }
        let wish = Vec3::new(o.direction.x, 0., o.direction.y).clamp_length_max(1.);
        if wish.length_squared() > 0.04 && self.last_stick.length_squared() > 0.04 {
            let angle = self.last_stick.angle_to(o.direction);
            if angle.abs() < 1.8 {
                self.spin_turn += angle;
            } else {
                self.spin_turn = 0.;
            }
            if self.spin_turn.abs() > 4.8 {
                self.spin = 0.6;
                self.spin_turn = 0.;
            }
        } else {
            self.spin_turn = 0.;
        }
        self.last_stick = o.direction;
        if o.spin {
            self.spin = 0.6;
        }
        let reversing = wish.length_squared() > 0.1
            && self.vel.with_y(0.).length() > 5.
            && wish.dot(self.facing) < -0.6;
        if wish.length_squared() > 0.01
            && !matches!(
                self.mode,
                Move::Roll | Move::Dive | Move::GroundPound | Move::Ledge | Move::Pole
            )
        {
            self.facing = wish.normalize();
        }
        if o.reset {
            self.reset();
            return;
        }

        if self.mode == Move::Ledge {
            self.vel = Vec3::ZERO;
            if o.crouch_pressed {
                self.mode = Move::Jump;
                self.pos += self.wall * 0.2;
                self.vel.y = -2.;
            } else if o.jump || o.direction.length() > 0.1 {
                self.pos = self.ledge - self.wall * 0.7;
                self.launch(Move::Jump, 7., None);
            }
            self.update_cap(o, scene, dt);
            return;
        }
        if self.mode == Move::Pole {
            if o.jump {
                self.facing = wish.try_normalize().unwrap_or(self.facing);
                self.launch(Move::Backflip, 11., Some(6.));
            } else if o.crouch_pressed {
                self.mode = Move::Jump;
                self.vel.y = -3.;
                self.pos += self.facing * 0.7;
            } else {
                self.pos.y = (self.pos.y + o.direction.y * -4. * dt)
                    .clamp(self.ledge.y, self.ledge.y + self.ledge.z);
                self.vel = Vec3::ZERO;
                self.update_cap(o, scene, dt);
                return;
            }
        }
        for pole in &scene.poles {
            if self.mode != Move::Pole
                && self.mode != Move::Backflip
                && self.mode != Move::GroundPound
                && !self.grounded
                && (self.pos - pole.center).with_y(0.).length() < 0.65
                && (pole.center.y..pole.center.y + pole.height).contains(&self.pos.y)
            {
                self.mode = Move::Pole;
                self.ledge = Vec3::new(pole.center.x, pole.center.y, pole.height);
                self.pos.x = pole.center.x;
                self.pos.z = pole.center.z;
                self.vel = Vec3::ZERO;
            }
        }
        let wet = scene.wet(self.pos + Vec3::Y * 0.6);
        if wet {
            self.mode = if o.throw_pressed {
                Move::SwimDash
            } else {
                Move::Swim
            };
            self.vel = self
                .vel
                .lerp(wish * if o.throw_held { 9. } else { 4. }, (dt * 3.).min(1.));
            self.vel.y += ((if o.jump_held {
                12.
            } else if o.crouch {
                -10.
            } else {
                2.
            }) - self.vel.y * 2.)
                * dt;
            if o.jump {
                self.vel.y = 8.;
            }
        } else {
            if matches!(self.mode, Move::Swim | Move::SwimDash) {
                self.mode = Move::Jump;
            }
            if self.jump_buffer > 0. {
                if self.catch_window > 0. && !self.catch_used && !self.grounded {
                    self.launch(Move::CatchJump, 10., None);
                    self.catch_used = true;
                } else if self.mode == Move::WallSlide && self.wall.length_squared() > 0.5 {
                    self.facing = self.wall;
                    self.launch(Move::WallJump, 11., Some(7.));
                    self.bounced = false;
                } else if self.grounded || self.coyote > 0. {
                    let speed = self.vel.with_y(0.).length();
                    if self.pound_window > 0. {
                        self.launch(Move::PoundJump, 16., None);
                        self.pound_window = 0.;
                    } else if self.catch_window > 0. {
                        self.launch(Move::CatchJump, 12., None);
                    } else if self.mode == Move::Roll {
                        self.launch(Move::RollJump, 8., Some(speed.max(12.)));
                    } else if self.spin > 0. {
                        self.launch(Move::SpinJump, 13., None);
                    } else if o.crouch {
                        if speed > 2. {
                            self.launch(Move::LongJump, 7., Some(12.));
                        } else {
                            self.launch(Move::Backflip, 14., Some(-2.));
                        }
                    } else if reversing {
                        self.launch(Move::Sideflip, 14., None);
                    } else {
                        self.chain = if self.since_land < 0.28 && speed > 2. {
                            self.chain % 3 + 1
                        } else {
                            1
                        };
                        let (kind, v) = match self.chain {
                            2 => (Move::DoubleJump, 11.5),
                            3 => (Move::TripleJump, 14.),
                            _ => (Move::Jump, 9.5),
                        };
                        self.launch(kind, v, None);
                    }
                }
            }
            if !self.grounded
                && o.crouch_pressed
                && !matches!(self.mode, Move::Dive | Move::GroundPound)
            {
                self.mode = Move::GroundPound;
                self.timer = 0.;
                self.vel = Vec3::ZERO;
            }
            if o.throw_pressed && o.crouch {
                if self.grounded {
                    self.mode = Move::Roll;
                    self.timer = 0.;
                    let s = self.vel.with_y(0.).length().max(13.);
                    self.vel = self.facing * s;
                } else if self.mode != Move::Dive {
                    self.launch(Move::Dive, 3.5, Some(13.));
                }
            }
            if self.mode == Move::Roll {
                if o.throw_pressed && !o.crouch {
                    self.mode = Move::Run;
                    self.vel *= 0.8;
                } else if o.boost {
                    self.vel += self.facing * 3.;
                } else if !o.crouch && self.timer > 0.2 {
                    self.mode = Move::Run;
                }
                let target = wish.try_normalize().unwrap_or(self.facing);
                self.facing = self.facing.lerp(target, dt * 2.).normalize_or_zero();
                let s = (self.vel.with_y(0.).length() - dt * 1.2).clamp(4., 24.);
                self.vel.x = self.facing.x * s;
                self.vel.z = self.facing.z * s;
            } else if self.mode == Move::GroundPound {
                self.vel = if self.timer < 0.12 {
                    Vec3::ZERO
                } else {
                    Vec3::NEG_Y * 28.
                };
            } else if !matches!(self.mode, Move::Dive | Move::Slide | Move::Pole) {
                let speed = if o.crouch && self.grounded {
                    1.8
                } else if o.walk {
                    3.
                } else {
                    8.
                };
                let accel = if self.grounded {
                    if wish.length_squared() > 0.01 {
                        32.
                    } else {
                        24.
                    }
                } else {
                    9.
                };
                let mut flat = self.vel.with_y(0.);
                let target = wish * speed;
                let change = (target - flat).clamp_length_max(accel * dt);
                // Preserve long-jump momentum in air; steering adds lateral authority.
                if self.grounded
                    || !matches!(self.mode, Move::LongJump | Move::RollJump | Move::CapVault)
                {
                    flat += change;
                } else {
                    flat += wish * dt * 3.;
                }
                self.vel.x = flat.x;
                self.vel.z = flat.z;
                if self.grounded {
                    self.mode = if o.crouch {
                        if wish.length_squared() > 0.01 {
                            Move::Crawl
                        } else {
                            Move::Crouch
                        }
                    } else if flat.length() < 0.1 {
                        Move::Idle
                    } else if speed < 4. {
                        Move::Walk
                    } else {
                        Move::Run
                    };
                }
            }
            if self.mode == Move::Slide {
                self.vel.x *= 1. - dt * 1.5;
                self.vel.z *= 1. - dt * 1.5;
                if self.vel.with_y(0.).length() < 2. || o.jump {
                    self.mode = Move::Run;
                }
            }
            if self.mode != Move::GroundPound {
                let gravity = if self.vel.y > 0. && o.jump_held {
                    24.
                } else {
                    36.
                };
                self.vel.y -= gravity * dt;
            }
        }
        self.update_cap(o, scene, dt);
        if let Some(cap) = &self.cap
            && !self.bounced
            && cap.age > 0.12
            && !cap.returning
            && cap.kind != Throw::Up
            && self.pos.distance(cap.pos - Vec3::Y * 0.35) < 0.9
            && self.vel.y <= 4.
        {
            let vault = self.grounded || self.mode == Move::Roll;
            self.launch(
                if vault {
                    Move::CapVault
                } else {
                    Move::CapBounce
                },
                12.,
                Some(if vault { 14. } else { 8. }),
            );
            self.bounced = true;
            self.cap = None;
        }
        let height = if self.grounded && o.crouch {
            0.9
        } else {
            HEIGHT
        };
        let old = self.pos;
        self.grounded = false;
        self.wall = Vec3::ZERO;
        // Small fixed steps bound travel to less than the collider radius.
        for axis in [0, 2, 1] {
            self.pos[axis] += self.vel[axis] * dt;
            for b in &scene.solids {
                let lo = b.center - b.half;
                let hi = b.center + b.half;
                let horizontal = self.pos.x + RADIUS > lo.x
                    && self.pos.x - RADIUS < hi.x
                    && self.pos.z + RADIUS > lo.z
                    && self.pos.z - RADIUS < hi.z;
                if !horizontal {
                    continue;
                }
                let top = b.top(self.pos.x.clamp(lo.x, hi.x));
                if axis == 1 {
                    if self.vel.y <= 0.
                        && old.y >= b.top(old.x.clamp(lo.x, hi.x)) - 0.15
                        && self.pos.y <= top
                        && self.pos.y + height > lo.y
                    {
                        self.pos.y = top;
                        self.vel.y = 0.;
                        self.grounded = true;
                    } else if self.vel.y > 0.
                        && old.y + height <= lo.y + 0.1
                        && self.pos.y + height >= lo.y
                    {
                        self.pos.y = lo.y - height;
                        self.vel.y = 0.;
                    }
                } else if self.pos.y + 0.12 < top && self.pos.y + height > lo.y + 0.05 {
                    // Short steps and ramps are traversable while grounded.
                    if old.y >= top - 0.32
                        && self.vel.y <= 0.
                        && !matches!(self.mode, Move::GroundPound | Move::Dive)
                    {
                        self.pos.y = top;
                        self.vel.y = 0.;
                        self.grounded = true;
                        continue;
                    }
                    let sign = if old[axis] < b.center[axis] { -1. } else { 1. };
                    self.pos[axis] = b.center[axis] + sign * (b.half[axis] + RADIUS);
                    self.wall = Vec3::ZERO;
                    self.wall[axis] = sign;
                    self.vel[axis] = 0.;
                    if self.vel.y < 0.
                        && !matches!(
                            self.mode,
                            Move::Dive | Move::GroundPound | Move::LongJump | Move::RollJump
                        )
                    {
                        if self.pos.y + HEIGHT > top
                            && self.pos.y + HEIGHT < top + 0.35
                            && o.direction.length() > 0.1
                        {
                            self.mode = Move::Ledge;
                            self.ledge = Vec3::new(self.pos.x, top, self.pos.z);
                            self.vel = Vec3::ZERO;
                        } else {
                            self.mode = Move::WallSlide;
                            self.vel.y = self.vel.y.max(-2.5);
                        }
                    }
                }
            }
        }
        self.distance += (self.pos - old).with_y(0.).length();
        if self.grounded {
            if old.y > self.pos.y + 0.005
                || self.since_land > 0.3 && self.vel.y == 0. && self.coyote == 0.
            {
                self.since_land = 0.;
                self.bounced = false;
                self.catch_used = false;
                if self.mode == Move::GroundPound {
                    self.pound_window = 0.25;
                    self.mode = Move::Idle;
                }
                if self.mode == Move::Dive {
                    self.mode = Move::Slide;
                }
            }
            if self.mode == Move::Roll {
                for b in &scene.solids {
                    if (self.pos.x - b.center.x).abs() < b.half.x
                        && (self.pos.z - b.center.z).abs() < b.half.z
                        && (self.pos.y - b.top(self.pos.x)).abs() < 0.1
                    {
                        self.vel.x -= b.slope * 24. * dt;
                        if b.slippery {
                            self.vel += self.facing * dt * 2.;
                        }
                    }
                }
            }
        }
        if self.pos.y < -18. || !self.pos.is_finite() {
            self.reset();
        }
    }
    fn update_cap(&mut self, o: &Orders, scene: &Course, dt: f32) {
        if (o.throw_pressed && !o.crouch || o.up_throw || o.down_throw || o.spin_throw)
            && self.cap.is_none()
        {
            let kind = if o.up_throw {
                Throw::Up
            } else if o.down_throw {
                Throw::Down
            } else if o.spin_throw || self.spin > 0. {
                Throw::Spin
            } else {
                Throw::Forward
            };
            let dir = match kind {
                Throw::Up => Vec3::Y,
                Throw::Down => (self.facing - Vec3::Y).normalize(),
                _ => self.facing,
            };
            self.cap = Some(Cap {
                pos: self.pos + Vec3::Y * 1.0 + dir * 0.5,
                dir,
                age: 0.,
                returning: false,
                kind,
            });
            if !self.grounded && self.mode != Move::GroundPound && self.mode != Move::Dive {
                self.vel.y = self.vel.y.max(1.8);
            }
        }
        if let Some(cap) = &mut self.cap {
            cap.age += dt;
            let hand = self.pos + Vec3::Y;
            if o.homing {
                let target = scene
                    .targets
                    .iter()
                    .filter(|t| t.distance(cap.pos) < 8.)
                    .min_by(|a, b| {
                        a.distance_squared(cap.pos)
                            .total_cmp(&b.distance_squared(cap.pos))
                    });
                cap.dir = target
                    .map(|t| (*t - cap.pos).normalize_or_zero())
                    .unwrap_or_else(|| {
                        Vec3::new(o.direction.x, 0.4, o.direction.y)
                            .try_normalize()
                            .unwrap_or(cap.dir)
                    });
                cap.age = 0.;
            }
            if cap.age > 2.5 || cap.age > 0.35 && !o.throw_held {
                cap.returning = true;
            }
            if cap.returning {
                cap.pos += (hand - cap.pos).normalize_or_zero() * 24. * dt;
            } else if cap.kind == Throw::Spin {
                cap.pos = hand + Vec3::new((cap.age * 12.).cos(), 0., (cap.age * 12.).sin()) * 2.8;
            } else if cap.age < 0.28 {
                cap.pos += cap.dir * 18. * dt;
            } else {
                cap.pos += Vec3::Y * (cap.age * 6.).sin() * dt * 0.3;
            }
            if cap.returning && cap.pos.distance(hand) < 0.5 {
                self.cap = None;
                self.catch_window = 0.15;
            }
        }
    }
}

pub fn self_check() {
    let floor = Course {
        solids: vec![Solid {
            center: Vec3::new(0., -0.5, 0.),
            half: Vec3::new(100., 0.5, 100.),
            slope: 0.,
            moving: false,
            slippery: false,
        }],
        ..default()
    };
    let dt = 1. / 120.;
    let mut p = Player::default();
    p.step(
        &Orders {
            jump: true,
            jump_held: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::Jump);
    assert!(p.vel.y > 9.);
    let mut high = p.pos.y;
    for _ in 0..150 {
        p.step(
            &Orders {
                jump_held: true,
                ..default()
            },
            &floor,
            dt,
        );
        high = high.max(p.pos.y);
    }
    assert!(high > 1.7 && p.grounded && p.pos.y.abs() < 0.001);
    let mut short = Player::default();
    short.step(
        &Orders {
            jump: true,
            ..default()
        },
        &floor,
        dt,
    );
    let mut low = 0_f32;
    for _ in 0..150 {
        short.step(&Orders::default(), &floor, dt);
        low = low.max(short.pos.y);
    }
    assert!(low < high);
    // Chain actual landings rather than setting jump counters directly.
    p = Player::default();
    for expected in [Move::Jump, Move::DoubleJump, Move::TripleJump] {
        for _ in 0..40 {
            p.step(
                &Orders {
                    direction: Vec2::NEG_Y,
                    ..default()
                },
                &floor,
                dt,
            );
            if p.vel.with_y(0.).length() > 3. {
                break;
            }
        }
        p.step(
            &Orders {
                direction: Vec2::NEG_Y,
                jump: true,
                jump_held: true,
                ..default()
            },
            &floor,
            dt,
        );
        assert_eq!(p.mode, expected);
        for _ in 0..240 {
            p.step(
                &Orders {
                    direction: Vec2::NEG_Y,
                    jump_held: true,
                    ..default()
                },
                &floor,
                dt,
            );
            if p.grounded {
                break;
            }
        }
        assert!(p.grounded);
    }
    for (o, expected) in [
        (
            Orders {
                direction: Vec2::NEG_Y,
                walk: true,
                ..default()
            },
            Move::Walk,
        ),
        (
            Orders {
                crouch: true,
                ..default()
            },
            Move::Crouch,
        ),
        (
            Orders {
                direction: Vec2::NEG_Y,
                crouch: true,
                ..default()
            },
            Move::Crawl,
        ),
        (
            Orders {
                spin: true,
                jump: true,
                ..default()
            },
            Move::SpinJump,
        ),
    ] {
        p = Player::default();
        p.step(&o, &floor, dt);
        assert_eq!(p.mode, expected);
    }
    p = Player::default();
    p.vel = Vec3::NEG_Z * 7.;
    p.step(
        &Orders {
            direction: Vec2::Y,
            jump: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::Sideflip);
    p = Player::default();
    p.step(
        &Orders {
            crouch: true,
            jump: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::Backflip);
    p = Player::default();
    p.vel = Vec3::NEG_Z * 6.;
    p.step(
        &Orders {
            crouch: true,
            jump: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::LongJump);
    p = Player::default();
    p.step(
        &Orders {
            crouch: true,
            throw_pressed: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::Roll);
    let speed = p.vel.length();
    p.step(
        &Orders {
            crouch: true,
            boost: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert!(p.vel.length() > speed + 2.);
    p.step(
        &Orders {
            crouch: true,
            jump: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::RollJump);
    p = Player::default();
    p.pos.y = 5.;
    p.grounded = false;
    p.step(
        &Orders {
            crouch: true,
            throw_pressed: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::Dive);
    p.step(
        &Orders {
            crouch_pressed: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::Dive);
    p = Player::default();
    p.pos.y = 5.;
    p.grounded = false;
    p.step(
        &Orders {
            crouch_pressed: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::GroundPound);
    for _ in 0..60 {
        p.step(&Orders::default(), &floor, dt);
        if p.grounded {
            break;
        }
    }
    assert!(p.pound_window > 0.);
    p.step(
        &Orders {
            jump: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::PoundJump);
    p = Player::default();
    p.pos.y = 3.;
    p.grounded = false;
    p.cap = Some(Cap {
        pos: p.pos + Vec3::Y * 0.35,
        dir: Vec3::NEG_Z,
        age: 0.3,
        returning: false,
        kind: Throw::Forward,
    });
    p.step(
        &Orders {
            throw_held: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::CapBounce);
    assert!(p.bounced);
    p = Player::default();
    p.cap = Some(Cap {
        pos: p.pos + Vec3::Y * 0.35,
        dir: Vec3::NEG_Z,
        age: 0.3,
        returning: false,
        kind: Throw::Forward,
    });
    p.step(
        &Orders {
            throw_held: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::CapVault);
    p = Player::default();
    p.pos.y = 4.;
    p.grounded = false;
    p.coyote = 0.;
    p.catch_window = 0.1;
    p.step(
        &Orders {
            jump: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::CatchJump);
    assert!(p.catch_used);
    let rising = p.vel.y;
    p.catch_window = 0.1;
    p.step(
        &Orders {
            jump: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert!(
        p.vel.y < rising,
        "catch boost can only happen once before landing"
    );
    for kind in [Throw::Forward, Throw::Up, Throw::Down, Throw::Spin] {
        p = Player::default();
        let o = Orders {
            throw_pressed: true,
            throw_held: true,
            up_throw: kind == Throw::Up,
            down_throw: kind == Throw::Down,
            spin_throw: kind == Throw::Spin,
            ..default()
        };
        p.step(&o, &floor, dt);
        assert_eq!(p.cap.as_ref().unwrap().kind, kind);
        for _ in 0..500 {
            p.step(&Orders::default(), &floor, dt);
        }
        assert!(p.cap.is_none());
    }
    let mut wall = floor.clone_scene();
    wall.solids.push(Solid {
        center: Vec3::new(0., 2., -2.),
        half: Vec3::new(3., 2., 0.3),
        slope: 0.,
        moving: false,
        slippery: false,
    });
    p = Player::default();
    p.pos = Vec3::new(0., 1., -1.35);
    p.grounded = false;
    p.coyote = 0.;
    p.vel = Vec3::new(0., -2., -4.);
    p.step(
        &Orders {
            direction: Vec2::NEG_Y,
            ..default()
        },
        &wall,
        dt,
    );
    assert_eq!(p.mode, Move::WallSlide);
    p.step(
        &Orders {
            jump: true,
            ..default()
        },
        &wall,
        dt,
    );
    assert_eq!(p.mode, Move::WallJump);
    assert!(p.vel.z > 0. && p.vel.y > 10.);
    p.pos = Vec3::new(0., 2.5, -1.35);
    p.vel = Vec3::new(0., -2., -4.);
    p.step(
        &Orders {
            direction: Vec2::NEG_Y,
            ..default()
        },
        &wall,
        dt,
    );
    assert_eq!(p.mode, Move::Ledge);
    p.step(
        &Orders {
            jump: true,
            ..default()
        },
        &wall,
        dt,
    );
    assert!(p.pos.y >= 4. && p.pos.z < -1.7);

    let mut water = floor.clone_scene();
    water.water = Some((Vec3::Y * 5., Vec3::splat(100.)));
    p = Player::default();
    p.step(
        &Orders {
            jump_held: true,
            ..default()
        },
        &water,
        dt,
    );
    assert_eq!(p.mode, Move::Swim);
    assert!(p.vel.y > 0.);
    p.step(
        &Orders {
            throw_pressed: true,
            throw_held: true,
            direction: Vec2::NEG_Y,
            ..default()
        },
        &water,
        dt,
    );
    assert_eq!(p.mode, Move::SwimDash);
    assert!(p.vel.z < 0.);
    let mut pole = floor.clone_scene();
    pole.poles.push(Pole {
        center: Vec3::ZERO,
        height: 8.,
    });
    p = Player::default();
    p.pos = Vec3::Y * 2.;
    p.grounded = false;
    p.coyote = 0.;
    p.step(&Orders::default(), &pole, dt);
    assert_eq!(p.mode, Move::Pole);
    let y = p.pos.y;
    p.step(
        &Orders {
            direction: Vec2::NEG_Y,
            ..default()
        },
        &pole,
        dt,
    );
    assert!(p.pos.y > y);
    p.step(
        &Orders {
            jump: true,
            direction: Vec2::NEG_Y,
            ..default()
        },
        &pole,
        dt,
    );
    assert_eq!(p.mode, Move::Backflip);
    let mut tunnel = floor.clone_scene();
    tunnel.solids.push(Solid {
        center: Vec3::new(0., 1.5, 5.),
        half: Vec3::new(2., 0.3, 2.),
        slope: 0.,
        moving: false,
        slippery: false,
    });
    p = Player::default();
    p.step(&Orders::default(), &tunnel, dt);
    assert_eq!(p.mode, Move::Crouch);
    p.step(
        &Orders {
            jump: true,
            ..default()
        },
        &tunnel,
        dt,
    );
    assert!(
        p.grounded && p.pos.y == 0.,
        "a blocked backflip must not expand through the ceiling"
    );
    let mut targets = floor.clone_scene();
    p = Player::default();
    targets.targets.push(p.pos + Vec3::new(4., 1., 0.));
    p.step(
        &Orders {
            throw_pressed: true,
            throw_held: true,
            ..default()
        },
        &targets,
        dt,
    );
    p.step(
        &Orders {
            homing: true,
            throw_held: true,
            ..default()
        },
        &targets,
        dt,
    );
    assert!(p.cap.as_ref().unwrap().dir.x > 0.9);
    p = Player::default();
    for _ in 0..300 {
        p.step(
            &Orders {
                direction: Vec2::NEG_Y,
                ..default()
            },
            &wall,
            dt,
        );
    }
    assert!(p.pos.z >= -1.36);
    for i in 0..10_000 {
        let o = Orders {
            direction: Vec2::new((i as f32 * 0.3).sin(), (i as f32 * 0.2).cos()),
            jump: i % 79 == 0,
            jump_held: i % 79 < 25,
            crouch: i % 173 < 8,
            crouch_pressed: i % 173 == 0,
            throw_pressed: i % 107 == 0,
            throw_held: i % 107 < 40,
            ..default()
        };
        p.step(&o, &wall, dt);
        assert!(p.pos.is_finite() && p.vel.is_finite());
    }
}
impl Course {
    fn clone_scene(&self) -> Self {
        Self {
            solids: self.solids.clone(),
            poles: self.poles.clone(),
            water: self.water,
            targets: self.targets.clone(),
        }
    }
}
