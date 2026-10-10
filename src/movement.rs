//! Original movement approximation. Metres, seconds, Y up; no copied game code.
//! Tuning is by feel against the original's behaviour, not measured values.
use crate::input::Orders;
use bevy::prelude::*;

pub const RADIUS: f32 = 0.35;
pub const HEIGHT: f32 = 1.65;
pub const SPAWN: Vec3 = Vec3::new(0.0, 0.0, 5.0);

/// Full-tilt running speed.
pub const RUN: f32 = 9.;
const RISE_GRAVITY: f32 = 27.;
const FIXED_GRAVITY: f32 = 31.;
const CUT_GRAVITY: f32 = 72.;
const FALL_GRAVITY: f32 = 40.;
const TERMINAL: f32 = 30.;
const ROLL_MAX: f32 = 23.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Move {
    #[default]
    Idle,
    Walk,
    Run,
    Skid,
    Crouch,
    Crawl,
    Fall,
    Jump,
    DoubleJump,
    TripleJump,
    LongJump,
    Backflip,
    Sideflip,
    SpinJump,
    GroundPound,
    PoundLand,
    PoundJump,
    Roll,
    RollEnd,
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
}
impl Move {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Ready",
            Self::Walk => "Walk",
            Self::Run => "Run",
            Self::Skid => "Skid",
            Self::Crouch => "Crouch",
            Self::Crawl => "Crawl",
            Self::Fall => "Fall",
            Self::Jump => "Jump",
            Self::DoubleJump => "Double jump",
            Self::TripleJump => "Triple jump",
            Self::LongJump => "Long jump",
            Self::Backflip => "Backflip",
            Self::Sideflip => "Side somersault",
            Self::SpinJump => "Spin jump",
            Self::GroundPound => "Ground pound",
            Self::PoundLand => "Ground pound",
            Self::PoundJump => "Ground pound jump",
            Self::Roll => "Roll",
            Self::RollEnd => "Roll",
            Self::RollJump => "Roll jump",
            Self::Dive => "Dive",
            Self::Slide => "Belly slide",
            Self::WallSlide => "Wall slide",
            Self::WallJump => "Wall jump",
            Self::Ledge => "Ledge hang",
            Self::Pole => "Pole climb",
            Self::Swim => "Swim",
            Self::SwimDash => "Swim dash",
            Self::CapBounce => "Cap jump",
            Self::CapVault => "Cap vault",
        }
    }
    pub fn airborne_momentum(self) -> bool {
        matches!(
            self,
            Self::LongJump | Self::RollJump | Self::Dive | Self::CapVault
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Throw {
    Forward,
    Up,
    Down,
    Spin,
}
/// Moments the presentation layer reacts to (particles, sound, camera).
#[derive(Clone, Copy, Debug, PartialEq, Message)]
pub enum Fx {
    Jump(Move),
    Land { hard: bool },
    Step,
    Skid,
    PoundStart,
    PoundLand,
    RollStart,
    RollBoost { perfect: bool },
    Dive,
    Throw(Throw),
    Catch,
    CapBounce,
    WallSlide,
    Ledge,
    Pole,
    Splash,
    SwimStroke,
}
#[derive(Clone, Debug)]
pub struct Cap {
    pub pos: Vec3,
    pub origin: Vec3,
    pub dir: Vec3,
    pub reach: f32,
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
    pub chain: u8,
    pub since_land: f32,
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
    pub cap_bounced: bool,
    pub dive_used: bool,
    pub air_throw: bool,
    pub last_boost: f32,
    pub control_lock: f32,
    pub hard_land: bool,
    pub peak: f32,
    pub slope: f32,
    pub slick: bool,
    pub stride: f32,
    pub hitstop: f32,
    pub events: Vec<Fx>,
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
            chain: 0,
            since_land: 5.,
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
            cap_bounced: false,
            dive_used: false,
            air_throw: false,
            last_boost: 9.,
            control_lock: 0.,
            hard_land: false,
            peak: SPAWN.y,
            slope: 0.,
            slick: false,
            stride: 0.,
            hitstop: 0.,
            events: Vec::new(),
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
#[derive(Resource, Default, Clone)]
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
    /// Highest walkable surface at or below `pos`, for shadows and the camera.
    pub fn ground(&self, pos: Vec3) -> Option<f32> {
        self.solids
            .iter()
            .filter(|b| {
                (pos.x - b.center.x).abs() < b.half.x && (pos.z - b.center.z).abs() < b.half.z
            })
            .map(|b| b.top(pos.x))
            .filter(|top| *top <= pos.y + 0.05)
            .max_by(f32::total_cmp)
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Course>()
        .add_message::<Fx>()
        .add_systems(FixedUpdate, simulate);
}
fn simulate(
    mut players: Query<&mut Player>,
    mut orders: ResMut<Orders>,
    course: Res<Course>,
    time: Res<Time<Fixed>>,
    menu: Res<crate::ui::Menu>,
    mut fx: MessageWriter<Fx>,
) {
    if menu.open {
        orders.clear_edges();
        return;
    }
    for mut p in &mut players {
        p.step(&orders, &course, time.delta_secs());
        fx.write_batch(p.events.drain(..));
    }
    orders.clear_edges();
}
/// Rotate a flat direction toward another by at most `max` radians.
fn turn(from: Vec3, to: Vec3, max: f32) -> Vec3 {
    let a = from.x.atan2(from.z);
    let b = to.x.atan2(to.z);
    let d = (b - a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    let y = a + d.clamp(-max, max);
    Vec3::new(y.sin(), 0., y.cos())
}
fn approach(value: f32, target: f32, step: f32) -> f32 {
    if value < target {
        (value + step).min(target)
    } else {
        (value - step).max(target)
    }
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
        self.events.push(Fx::Jump(kind));
    }
    fn set(&mut self, mode: Move) {
        if self.mode != mode {
            self.mode = mode;
            self.timer = 0.;
        }
    }
    pub fn reset(&mut self) {
        let checkpoint = self.checkpoint;
        *self = Self::default();
        self.pos = checkpoint;
        self.peak = checkpoint.y;
        self.checkpoint = checkpoint;
    }
    pub fn flat_speed(&self) -> f32 {
        self.vel.with_y(0.).length()
    }
    pub fn step(&mut self, o: &Orders, scene: &Course, dt: f32) {
        if !dt.is_finite() || dt <= 0. || dt > 0.05 {
            return;
        }
        if self.hitstop > 0. {
            self.hitstop -= dt;
            return;
        }
        let mut guarded = o.clone();
        let low_ceiling = scene.solids.iter().any(|b| {
            let bottom = b.center.y - b.half.y;
            bottom > self.pos.y + 0.2
                && bottom < self.pos.y + HEIGHT
                && (self.pos.x - b.center.x).abs() < b.half.x + RADIUS
                && (self.pos.z - b.center.z).abs() < b.half.z + RADIUS
        });
        if self.grounded && low_ceiling && self.mode != Move::Roll {
            guarded.crouch = true;
            guarded.jump = false;
            self.jump_buffer = 0.;
        }
        let o = &guarded;
        self.timer += dt;
        self.since_land += dt;
        self.last_boost += dt;
        self.spin = (self.spin - dt).max(0.);
        self.control_lock = (self.control_lock - dt).max(0.);
        self.jump_buffer = (self.jump_buffer - dt).max(0.);
        if o.jump {
            self.jump_buffer = 0.12;
        }
        self.coyote = if self.grounded {
            0.1
        } else {
            (self.coyote - dt).max(0.)
        };
        let wish = Vec3::new(o.direction.x, 0., o.direction.y).clamp_length_max(1.);
        // Circling the stick arms a spin, as in the original's spin jump/throw.
        if wish.length_squared() > 0.25 && self.last_stick.length_squared() > 0.25 {
            let angle = self.last_stick.angle_to(o.direction);
            if angle.abs() < 1.2
                && (self.spin_turn == 0. || angle.signum() == self.spin_turn.signum())
            {
                self.spin_turn += angle;
            } else {
                self.spin_turn = 0.;
            }
            if self.spin_turn.abs() > 5.5 {
                self.spin = 0.5;
                self.spin_turn = 0.;
            }
        } else {
            self.spin_turn = 0.;
        }
        self.last_stick = o.direction;
        if o.reset {
            self.reset();
            return;
        }
        let was_grounded = self.grounded;

        if self.mode == Move::Ledge {
            self.vel = Vec3::ZERO;
            let toward = wish.dot(-self.wall);
            if o.crouch_pressed || toward < -0.4 {
                self.set(Move::Fall);
                self.pos += self.wall * 0.25;
                self.vel.y = -2.;
            } else if self.timer > 0.15 && (o.jump || toward > 0.5) {
                // Climb: hop onto the ledge top.
                self.pos = self.ledge - self.wall * 0.7;
                self.launch(Move::Jump, 6.5, Some(2.));
            }
            self.update_cap(o, scene, dt);
            return;
        }
        if self.mode == Move::Pole {
            if o.jump {
                self.facing = wish.try_normalize().unwrap_or(-self.facing);
                self.pos += self.facing * 0.5;
                self.launch(Move::Backflip, 13., Some(6.));
            } else if o.crouch_pressed {
                self.set(Move::Fall);
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
            if !matches!(self.mode, Move::Pole | Move::Backflip | Move::GroundPound)
                && !self.grounded
                && (self.pos - pole.center).with_y(0.).length() < 0.65
                && (pole.center.y..pole.center.y + pole.height).contains(&self.pos.y)
            {
                self.set(Move::Pole);
                self.ledge = Vec3::new(pole.center.x, pole.center.y, pole.height);
                self.facing = (self.pos - pole.center)
                    .with_y(0.)
                    .try_normalize()
                    .map_or(self.facing, |d| -d);
                self.pos.x = pole.center.x;
                self.pos.z = pole.center.z;
                self.vel = Vec3::ZERO;
                self.events.push(Fx::Pole);
                self.update_cap(o, scene, dt);
                return;
            }
        }
        let wet = scene.wet(self.pos + Vec3::Y * 0.6);
        if wet {
            self.swim(o, wish, dt);
        } else {
            if matches!(self.mode, Move::Swim | Move::SwimDash) {
                self.set(Move::Fall);
            }
            self.actions(o, wish);
            if self.grounded {
                self.ground_move(o, wish, dt);
            } else {
                self.air_move(o, wish, dt);
            }
        }
        self.update_cap(o, scene, dt);
        self.cap_contact();
        self.collide(o, scene, dt);
        // Stay glued to ramps and steps down instead of hopping off each frame.
        if !self.grounded
            && was_grounded
            && self.vel.y <= 0.
            && let Some(ground) = scene.ground(self.pos)
            && self.pos.y - ground < 0.45
        {
            self.pos.y = ground;
            self.grounded = true;
        }
        if self.grounded && !was_grounded {
            self.land(o);
        } else if !self.grounded && was_grounded && self.vel.y <= 0. {
            // Walked off an edge: coyote time still allows a jump.
            if matches!(
                self.mode,
                Move::Idle | Move::Walk | Move::Run | Move::Skid | Move::Crouch | Move::Crawl
            ) {
                self.set(Move::Fall);
            }
        }
        if self.grounded {
            self.peak = self.pos.y;
            self.stride += self.flat_speed() * dt;
            let stride = if self.flat_speed() > 5. { 1.15 } else { 0.75 };
            if matches!(self.mode, Move::Walk | Move::Run | Move::Crawl) && self.stride > stride {
                self.stride = 0.;
                self.events.push(Fx::Step);
            }
        } else {
            self.peak = self.peak.max(self.pos.y);
        }
        if self.pos.y < -18. || !self.pos.is_finite() {
            self.reset();
        }
    }
    /// Discrete moves started by button presses, before continuous movement.
    fn actions(&mut self, o: &Orders, wish: Vec3) {
        let speed = self.flat_speed();
        // Dive: crouch + throw in the air, or throw during a ground pound.
        if !self.grounded
            && o.throw_pressed
            && !self.dive_used
            && (o.crouch || self.mode == Move::GroundPound)
            && !matches!(self.mode, Move::Dive | Move::WallSlide | Move::Roll)
        {
            if let Some(w) = wish.try_normalize() {
                self.facing = w;
            }
            let s = (speed.max(9.) + 3.).min(16.);
            self.mode = Move::Dive;
            self.timer = 0.;
            self.vel = self.facing * s + Vec3::Y * 5.5;
            self.dive_used = true;
            self.events.push(Fx::Dive);
            return;
        }
        if !self.grounded
            && o.crouch_pressed
            && !matches!(
                self.mode,
                Move::Dive
                    | Move::GroundPound
                    | Move::WallSlide
                    | Move::Ledge
                    | Move::Pole
                    | Move::Roll
            )
        {
            self.set(Move::GroundPound);
            self.vel = Vec3::ZERO;
            self.events.push(Fx::PoundStart);
            return;
        }
        if self.grounded && o.throw_pressed {
            if self.mode == Move::PoundLand && o.crouch {
                self.start_roll(17.);
                return;
            }
            if self.mode == Move::Roll && !o.jump {
                self.roll_boost();
                return;
            }
            if o.crouch && !matches!(self.mode, Move::Roll | Move::Slide | Move::Skid) {
                self.start_roll(speed.max(13.));
                return;
            }
        }
        if self.jump_buffer <= 0. {
            return;
        }
        if self.mode == Move::WallSlide && self.wall.length_squared() > 0.5 {
            self.facing = self.wall;
            self.launch(Move::WallJump, 13., Some(7.5));
            self.control_lock = 0.22;
            self.dive_used = false;
            return;
        }
        if !(self.grounded || self.coyote > 0.) {
            return;
        }
        match self.mode {
            Move::Roll if o.throw_pressed => {
                // Roll cancel: stand up at speed, ready to jump onward.
                self.set(Move::Run);
                self.jump_buffer = 0.;
                let s = speed.min(14.);
                self.vel = self.facing * s;
            }
            Move::Roll | Move::RollEnd => {
                self.launch(Move::RollJump, 9.5, Some(speed.max(12.)));
            }
            Move::PoundLand if self.timer < 0.3 => self.launch(Move::PoundJump, 17., Some(0.)),
            _ if self.spin > 0. => {
                self.spin = 0.;
                self.launch(Move::SpinJump, 13., None);
            }
            Move::Skid => {
                if let Some(w) = wish.try_normalize() {
                    self.facing = w;
                }
                self.launch(Move::Sideflip, 15., Some(3.));
            }
            Move::Slide => self.launch(Move::Jump, 8.5, Some(speed * 0.5)),
            _ if o.crouch => {
                if speed > 3. {
                    self.launch(Move::LongJump, 9., Some(speed.max(14.)));
                } else {
                    self.launch(Move::Backflip, 15.5, Some(-3.5));
                }
            }
            _ => {
                self.chain = if self.since_land < 0.2 && speed > 3. && self.chain < 3 {
                    if self.chain == 2 && speed < 6. {
                        2
                    } else {
                        self.chain + 1
                    }
                } else {
                    1
                };
                let (kind, v) = match self.chain {
                    2 => (Move::DoubleJump, 12.5),
                    3 => (Move::TripleJump, 15.),
                    _ => (Move::Jump, 11.),
                };
                self.launch(kind, v, None);
            }
        }
    }
    fn start_roll(&mut self, speed: f32) {
        self.set(Move::Roll);
        self.last_boost = 0.;
        if let Some(w) = Vec3::new(self.last_stick.x, 0., self.last_stick.y).try_normalize() {
            self.facing = w;
        }
        self.vel = self.facing * speed.min(ROLL_MAX);
        self.events.push(Fx::RollStart);
    }
    fn roll_boost(&mut self) {
        if self.last_boost < 0.22 {
            return;
        }
        // Pressing on the roll's rhythm earns a stronger push.
        let perfect = (0.32..0.62).contains(&self.last_boost);
        let s = (self.flat_speed() + if perfect { 5.5 } else { 3. }).min(ROLL_MAX);
        self.vel = self.facing * s + Vec3::Y * 3.2;
        self.grounded = false;
        self.last_boost = 0.;
        self.timer = 0.;
        self.events.push(Fx::RollBoost { perfect });
    }
    fn ground_move(&mut self, o: &Orders, wish: Vec3, dt: f32) {
        let speed = self.flat_speed();
        let tilt = wish.length();
        match self.mode {
            Move::Skid => {
                let s = (speed - 38. * dt).max(0.);
                self.vel = self.vel.with_y(0.).normalize_or_zero() * s;
                if s < 0.5 || self.timer > 0.32 {
                    if let Some(w) = wish.try_normalize() {
                        self.facing = w;
                    }
                    self.vel = self.facing * s.min(2.);
                    self.set(Move::Run);
                }
            }
            Move::PoundLand => {
                self.vel = Vec3::ZERO;
                if self.timer > 0.3 {
                    self.set(if o.crouch { Move::Crouch } else { Move::Idle });
                }
            }
            Move::Slide => {
                let s = (speed - 9. * dt).max(0.);
                if let Some(w) = wish.try_normalize() {
                    self.facing = turn(self.facing, w, 1.5 * dt);
                }
                self.vel = self.facing * s;
                if s < 1. {
                    self.set(Move::Idle);
                }
            }
            Move::Roll => {
                if let Some(w) = wish.try_normalize() {
                    self.facing = turn(self.facing, w, 2.4 * dt);
                }
                // Downhill speeds the roll up, uphill slows and can reverse it.
                let downhill = -self.slope * self.facing.x;
                let mut s = speed + downhill * 26. * dt;
                if o.crouch {
                    s -= if self.slick { 1. } else { 6.5 } * dt;
                } else {
                    s -= 30. * dt;
                }
                if s < 0. {
                    self.facing = -self.facing;
                    s = -s;
                }
                s = s.min(ROLL_MAX);
                self.vel = self.facing * s;
                if !o.crouch && s <= RUN {
                    self.set(if tilt > 0.1 { Move::Run } else { Move::RollEnd });
                } else if o.crouch && s < 5. && downhill <= 0.05 {
                    self.set(Move::RollEnd);
                }
            }
            Move::RollEnd => {
                let s = (speed - 25. * dt).max(0.);
                self.vel = self.facing * s;
                if self.timer > 0.3 {
                    self.set(if o.crouch { Move::Crouch } else { Move::Idle });
                }
            }
            _ => {
                let crouching = o.crouch;
                if !crouching
                    && speed > 6.
                    && tilt > 0.5
                    && wish.normalize().dot(self.facing) < -0.55
                {
                    self.set(Move::Skid);
                    self.events.push(Fx::Skid);
                    return;
                }
                if let Some(w) = wish.try_normalize() {
                    let rate = if crouching {
                        6.
                    } else if speed < 3. {
                        22.
                    } else {
                        14. - 6. * (speed / RUN).min(1.)
                    };
                    self.facing = turn(self.facing, w, rate * dt);
                }
                let max = if crouching { 2.2 } else { RUN } * tilt;
                let accel = if max > speed {
                    if speed < 3. { 40. } else { 22. }
                } else if tilt < 0.05 {
                    if crouching { 14. } else { 45. }
                } else {
                    18.
                };
                let s = approach(speed, max, accel * dt);
                self.vel = self.facing * s;
                if !matches!(self.mode, Move::PoundLand) {
                    let next = if crouching {
                        if s > 0.3 { Move::Crawl } else { Move::Crouch }
                    } else if s < 0.2 && tilt < 0.05 {
                        Move::Idle
                    } else if s < 4.5 {
                        Move::Walk
                    } else {
                        Move::Run
                    };
                    self.set(next);
                }
            }
        }
    }
    fn air_move(&mut self, o: &Orders, wish: Vec3, dt: f32) {
        let mut flat = self.vel.with_y(0.);
        let tilt = wish.length();
        match self.mode {
            Move::GroundPound => {
                // A brief somersault hang, then a straight drop.
                self.vel = if self.timer < 0.28 {
                    Vec3::Y * 0.6
                } else {
                    Vec3::NEG_Y * 32.
                };
                return;
            }
            Move::WallSlide => {
                if o.crouch_pressed {
                    self.set(Move::Fall);
                    self.pos += self.wall * 0.15;
                }
                // Keep pressing into the wall so contact is detected each step.
                self.vel = -self.wall * 1.5 + Vec3::Y * (self.vel.y - 18. * dt).max(-3.5);
                return;
            }
            Move::Roll => {}
            m if m.airborne_momentum() => {
                if let Some(w) = wish.try_normalize() {
                    let d = flat.normalize_or(self.facing);
                    if w.dot(d) < -0.3 {
                        flat *= 1. - 1.6 * dt;
                    } else {
                        flat = turn(d, w, 1.7 * dt) * flat.length();
                    }
                }
                self.facing = flat.normalize_or(self.facing);
            }
            _ if self.control_lock > 0. => {}
            _ => {
                let speed = flat.length();
                if tilt > 0.05 {
                    let target = wish * speed.max(RUN * tilt);
                    flat = flat.move_towards(target, 17. * dt);
                    if !matches!(self.mode, Move::Backflip | Move::Sideflip | Move::WallJump) {
                        self.facing = turn(self.facing, wish.normalize(), 7. * dt);
                    }
                } else {
                    flat = flat.move_towards(Vec3::ZERO, 5. * dt);
                }
            }
        }
        self.vel.x = flat.x;
        self.vel.z = flat.z;
        let variable = matches!(
            self.mode,
            Move::Jump | Move::DoubleJump | Move::TripleJump | Move::WallJump
        );
        let gravity = if self.vel.y > 0. {
            if !variable {
                FIXED_GRAVITY
            } else if o.jump_held {
                RISE_GRAVITY
            } else {
                CUT_GRAVITY
            }
        } else {
            FALL_GRAVITY
        };
        let terminal = if self.mode == Move::SpinJump && self.timer > 0.3 {
            // The spin jump drifts down slowly while turning.
            -5.
        } else {
            -TERMINAL
        };
        self.vel.y = (self.vel.y - gravity * dt).max(terminal.min(self.vel.y));
    }
    fn swim(&mut self, o: &Orders, wish: Vec3, dt: f32) {
        if !matches!(self.mode, Move::Swim | Move::SwimDash) {
            self.events.push(Fx::Splash);
            self.vel.y *= 0.3;
            self.set(Move::Swim);
        }
        if o.throw_pressed {
            self.set(Move::SwimDash);
            self.vel = self.facing * 9. + Vec3::Y * self.vel.y;
            self.events.push(Fx::SwimStroke);
        } else if self.mode == Move::SwimDash && self.timer > 0.5 {
            self.set(Move::Swim);
        }
        if let Some(w) = wish.try_normalize() {
            self.facing = turn(self.facing, w, 5. * dt);
        }
        let flat = self.vel.with_y(0.).move_towards(
            wish * if self.mode == Move::SwimDash { 9. } else { 4.5 },
            (if self.mode == Move::SwimDash { 4. } else { 10. }) * dt,
        );
        self.vel.x = flat.x;
        self.vel.z = flat.z;
        let target = if o.jump_held {
            3.5
        } else if o.crouch {
            -4.
        } else {
            -0.6
        };
        self.vel.y += (target - self.vel.y) * (2.5 * dt).min(1.);
        if o.jump {
            self.vel.y = 7.;
            self.events.push(Fx::SwimStroke);
        }
    }
    fn land(&mut self, o: &Orders) {
        let fall = self.peak - self.pos.y;
        let speed = self.flat_speed();
        self.since_land = 0.;
        self.cap_bounced = false;
        self.dive_used = false;
        self.air_throw = false;
        self.control_lock = 0.;
        self.hard_land = fall > 7. && !matches!(self.mode, Move::GroundPound | Move::Dive);
        match self.mode {
            Move::GroundPound => {
                self.set(Move::PoundLand);
                self.hitstop = 0.05;
                self.events.push(Fx::PoundLand);
                return;
            }
            Move::Dive | Move::LongJump | Move::RollJump if o.crouch && speed > 6. => {
                self.set(Move::Roll);
                self.last_boost = 0.;
                self.events.push(Fx::RollStart);
            }
            Move::Dive => self.set(Move::Slide),
            Move::Roll => {}
            _ => {
                if self.hard_land {
                    self.vel *= 0.25;
                }
                self.set(if o.crouch {
                    Move::Crouch
                } else if self.flat_speed() > 4.5 {
                    Move::Run
                } else if self.flat_speed() > 0.2 {
                    Move::Walk
                } else {
                    Move::Idle
                });
            }
        }
        self.events.push(Fx::Land {
            hard: self.hard_land,
        });
    }
    fn cap_contact(&mut self) {
        let Some(cap) = &self.cap else {
            return;
        };
        if self.cap_bounced
            || cap.age < 0.12
            || cap.returning
            || cap.kind == Throw::Up
            || matches!(
                self.mode,
                Move::GroundPound | Move::Ledge | Move::Pole | Move::Swim | Move::SwimDash
            )
            || (self.pos + Vec3::Y * 0.7).distance(cap.pos) > 1.0
        {
            return;
        }
        let vault = self.grounded || self.mode == Move::Roll;
        if !vault && self.vel.y > 3. {
            return;
        }
        let keep = self.flat_speed();
        self.launch(
            if vault {
                Move::CapVault
            } else {
                Move::CapBounce
            },
            12.5,
            Some(if vault { keep.max(11.) } else { keep.min(10.) }),
        );
        self.cap_bounced = true;
        self.dive_used = false;
        self.air_throw = false;
        self.hitstop = 0.035;
        self.cap = None;
        self.events.push(Fx::CapBounce);
    }
    fn collide(&mut self, o: &Orders, scene: &Course, dt: f32) {
        let height = if self.grounded && o.crouch
            || matches!(self.mode, Move::Roll | Move::Slide | Move::Dive)
        {
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
                        self.slope = if (self.pos.x - b.center.x).abs() < b.half.x {
                            b.slope
                        } else {
                            0.
                        };
                        self.slick = b.slippery;
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
                    let into = -self.vel[axis] * sign;
                    self.vel[axis] = 0.;
                    if self.mode == Move::Roll && into > 8. {
                        // Rolling into a wall knocks the roll back out.
                        self.facing = (self.facing - self.wall * 2. * self.facing.dot(self.wall))
                            .normalize_or(self.wall);
                        self.vel = self.facing * 4.;
                        self.set(Move::RollEnd);
                    }
                    if self.vel.y < 0.
                        && !self.grounded
                        && !matches!(
                            self.mode,
                            Move::Dive
                                | Move::GroundPound
                                | Move::LongJump
                                | Move::RollJump
                                | Move::Roll
                        )
                    {
                        if self.pos.y + HEIGHT > top
                            && self.pos.y + HEIGHT < top + 0.45
                            && o.direction.length() > 0.1
                        {
                            self.set(Move::Ledge);
                            self.ledge = Vec3::new(self.pos.x, top, self.pos.z);
                            self.facing = -self.wall;
                            self.vel = Vec3::ZERO;
                            self.events.push(Fx::Ledge);
                        } else if self.mode != Move::WallSlide && into > 0.5 {
                            self.set(Move::WallSlide);
                            self.facing = self.wall;
                            self.vel.y = self.vel.y.max(-3.5);
                            self.events.push(Fx::WallSlide);
                        }
                    }
                }
            }
        }
        if self.mode == Move::WallSlide && (self.wall == Vec3::ZERO || self.grounded) {
            self.set(Move::Fall);
        }
        self.distance += (self.pos - old).with_y(0.).length();
    }
    fn update_cap(&mut self, o: &Orders, scene: &Course, dt: f32) {
        let free = !matches!(
            self.mode,
            Move::Ledge | Move::Pole | Move::Swim | Move::SwimDash
        );
        let plain =
            o.throw_pressed && !o.crouch && !matches!(self.mode, Move::Roll | Move::GroundPound);
        if free
            && self.cap.is_none()
            && (plain
                || o.up_throw
                || o.down_throw
                || o.spin_throw
                || self.mode == Move::PoundLand && o.throw_pressed && !o.crouch)
        {
            let kind = if o.up_throw {
                Throw::Up
            } else if o.down_throw || self.mode == Move::PoundLand {
                Throw::Down
            } else if o.spin_throw || self.spin > 0. {
                Throw::Spin
            } else {
                Throw::Forward
            };
            let aim = Vec3::new(o.direction.x, 0., o.direction.y)
                .try_normalize()
                .unwrap_or(self.facing);
            if kind == Throw::Forward && self.grounded {
                self.facing = aim;
            }
            let (dir, reach) = match kind {
                Throw::Up => (Vec3::Y, 4.5),
                Throw::Down if self.grounded => (self.facing, 3.5),
                Throw::Down => ((self.facing * 0.6 - Vec3::Y).normalize(), 3.5),
                _ => (self.facing, 5.2),
            };
            let origin = self.pos + Vec3::Y * 0.9;
            self.cap = Some(Cap {
                pos: origin,
                origin,
                dir,
                reach,
                age: 0.,
                returning: false,
                kind,
            });
            self.spin = 0.;
            self.events.push(Fx::Throw(kind));
            if !self.grounded && !self.air_throw && !matches!(self.mode, Move::Dive) {
                // One small hang per airtime, like the original's midair throw.
                self.vel.y = self.vel.y.max(if kind == Throw::Spin { 4. } else { 2.5 });
                self.air_throw = true;
            }
        }
        let Some(cap) = &mut self.cap else {
            return;
        };
        cap.age += dt;
        let hand = self.pos + Vec3::Y * 0.9;
        if o.homing && !cap.returning && cap.kind != Throw::Spin {
            let target = scene
                .targets
                .iter()
                .filter(|t| t.distance(cap.pos) < 9.)
                .min_by(|a, b| {
                    a.distance_squared(cap.pos)
                        .total_cmp(&b.distance_squared(cap.pos))
                });
            let dir = target
                .map(|t| (*t - cap.pos).normalize_or_zero())
                .or_else(|| Vec3::new(o.direction.x, 0., o.direction.y).try_normalize())
                .unwrap_or(cap.dir);
            cap.reach = target.map_or(4.5, |t| t.distance(cap.pos));
            cap.origin = cap.pos;
            cap.dir = dir;
            cap.age = 0.;
        }
        let hold = if cap.kind == Throw::Spin { 0.65 } else { 0.32 };
        if cap.age > 1.8 || cap.age > hold && !o.throw_held {
            cap.returning = true;
        }
        if cap.returning {
            let speed = 16. + cap.age * 30.;
            cap.pos +=
                (hand - cap.pos).normalize_or_zero() * (speed * dt).min(cap.pos.distance(hand));
        } else if cap.kind == Throw::Spin {
            let radius = 2.6 * (cap.age / 0.12).min(1.);
            cap.pos = hand + Vec3::new((cap.age * 15.).cos(), 0., (cap.age * 15.).sin()) * radius;
        } else {
            // Ease out to full reach, then hover with a gentle bob.
            let t = (cap.age / 0.24).min(1.);
            let reach = cap.reach * (1. - (1. - t) * (1. - t));
            cap.pos = cap.origin
                + cap.dir * reach
                + Vec3::Y * ((cap.age - 0.24).max(0.) * 7.).sin() * 0.08;
        }
        if cap.returning && cap.pos.distance(hand) < 0.6 {
            self.cap = None;
            self.events.push(Fx::Catch);
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
    let fwd = |extra: Orders| Orders {
        direction: Vec2::NEG_Y,
        ..extra
    };
    let run_up = |p: &mut Player, scene: &Course| {
        for _ in 0..90 {
            p.step(&fwd(default()), scene, dt);
        }
    };
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
    for _ in 0..200 {
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
    assert!(
        high > 2. && p.grounded && p.pos.y.abs() < 0.001,
        "high {high}"
    );
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
    for _ in 0..200 {
        short.step(&Orders::default(), &floor, dt);
        low = low.max(short.pos.y);
    }
    assert!(low < high * 0.7, "releasing jump early gives a short hop");
    // Chain actual landings rather than setting jump counters directly.
    p = Player::default();
    run_up(&mut p, &floor);
    assert_eq!(p.mode, Move::Run);
    assert!((p.flat_speed() - RUN).abs() < 0.1);
    for expected in [Move::Jump, Move::DoubleJump, Move::TripleJump] {
        p.step(
            &fwd(Orders {
                jump: true,
                jump_held: true,
                ..default()
            }),
            &floor,
            dt,
        );
        assert_eq!(p.mode, expected);
        for _ in 0..300 {
            p.step(
                &fwd(Orders {
                    jump_held: true,
                    ..default()
                }),
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
                direction: Vec2::NEG_Y * 0.3,
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
    ] {
        p = Player::default();
        for _ in 0..30 {
            p.step(&o, &floor, dt);
        }
        assert_eq!(p.mode, expected);
    }
    // Circling the stick then jumping gives a spin jump.
    p = Player::default();
    for i in 0..40 {
        let a = i as f32 * 0.35;
        p.step(
            &Orders {
                direction: Vec2::new(a.cos(), a.sin()),
                ..default()
            },
            &floor,
            dt,
        );
    }
    p.step(
        &Orders {
            jump: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::SpinJump);
    // Reversing at speed skids; jumping out of a skid is a side somersault.
    p = Player::default();
    run_up(&mut p, &floor);
    p.step(
        &Orders {
            direction: Vec2::Y,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::Skid);
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
    assert!(p.vel.z > 0., "side somersault goes the new way");
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
    run_up(&mut p, &floor);
    p.step(
        &fwd(Orders {
            crouch: true,
            jump: true,
            ..default()
        }),
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::LongJump);
    assert!(p.flat_speed() >= 14.);
    // Roll, boost on the rhythm, then the roll runs out on flat ground.
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
    assert!(p.cap.is_none(), "roll input does not throw the cap");
    for _ in 0..50 {
        p.step(
            &Orders {
                crouch: true,
                ..default()
            },
            &floor,
            dt,
        );
    }
    let speed = p.flat_speed();
    p.step(
        &Orders {
            crouch: true,
            throw_pressed: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert!(p.flat_speed() > speed + 4., "rhythm boost");
    let mut ended = false;
    for _ in 0..1200 {
        p.step(
            &Orders {
                crouch: true,
                ..default()
            },
            &floor,
            dt,
        );
        if p.mode == Move::Crouch {
            ended = true;
            break;
        }
    }
    assert!(ended, "a roll on flat ground slows back into a crouch");
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
    for _ in 0..20 {
        p.step(&Orders::default(), &floor, dt);
    }
    assert_ne!(p.mode, Move::Roll, "releasing crouch stops the roll");
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
    p.step(
        &Orders {
            crouch: true,
            throw_pressed: true,
            ..default()
        },
        &floor,
        dt,
    );
    p.step(
        &Orders {
            jump: true,
            throw_pressed: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::Run, "roll cancel stands up at speed");
    assert!(p.flat_speed() > RUN);
    // Dive, then belly slide; diving while holding crouch lands into a roll.
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
    for _ in 0..200 {
        p.step(&Orders::default(), &floor, dt);
        if p.grounded {
            break;
        }
    }
    assert_eq!(p.mode, Move::Slide);
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
    for _ in 0..200 {
        p.step(
            &Orders {
                crouch: true,
                ..default()
            },
            &floor,
            dt,
        );
        if p.grounded {
            break;
        }
    }
    assert_eq!(p.mode, Move::Roll);
    // Ground pound: hang, drop, impact; jump on impact or throw into a dive.
    p = Player::default();
    p.pos.y = 5.;
    p.grounded = false;
    p.step(
        &Orders {
            crouch_pressed: true,
            crouch: true,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::GroundPound);
    p.step(&Orders::default(), &floor, dt);
    assert!(p.pos.y > 4.99, "pound starts with a hang");
    for _ in 0..200 {
        p.step(&Orders::default(), &floor, dt);
        if p.grounded {
            break;
        }
    }
    assert_eq!(p.mode, Move::PoundLand);
    for _ in 0..10 {
        p.step(&Orders::default(), &floor, dt);
    }
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
    p.pos.y = 5.;
    p.grounded = false;
    p.step(
        &Orders {
            crouch_pressed: true,
            crouch: true,
            ..default()
        },
        &floor,
        dt,
    );
    p.step(
        &Orders {
            throw_pressed: true,
            direction: Vec2::X,
            ..default()
        },
        &floor,
        dt,
    );
    assert_eq!(p.mode, Move::Dive);
    assert!(p.vel.x > 9.);
    // Cap jump: dive onto a held cap; vault a cap from the ground.
    p = Player::default();
    p.pos.y = 3.;
    p.grounded = false;
    p.cap = Some(Cap {
        pos: p.pos + Vec3::Y * 0.7,
        origin: p.pos + Vec3::Y * 0.7,
        dir: Vec3::NEG_Z,
        reach: 0.,
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
    assert!(p.cap_bounced && !p.dive_used);
    p = Player::default();
    p.cap = Some(Cap {
        pos: p.pos + Vec3::Y * 0.7,
        origin: p.pos + Vec3::Y * 0.7,
        dir: Vec3::NEG_Z,
        reach: 0.,
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
    for kind in [Throw::Forward, Throw::Up, Throw::Down, Throw::Spin] {
        p = Player::default();
        let o = Orders {
            throw_pressed: kind == Throw::Forward,
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
    let mut wall = floor.clone();
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
    p.step(&fwd(default()), &wall, dt);
    assert_eq!(p.mode, Move::WallSlide);
    for _ in 0..10 {
        p.step(&Orders::default(), &wall, dt);
    }
    assert_eq!(p.mode, Move::WallSlide, "wall slide holds contact");
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
    p.step(&fwd(default()), &wall, dt);
    assert_eq!(p.mode, Move::Ledge);
    for _ in 0..30 {
        p.step(&Orders::default(), &wall, dt);
    }
    p.step(
        &Orders {
            jump: true,
            ..default()
        },
        &wall,
        dt,
    );
    assert!(p.pos.y >= 4. && p.pos.z < -1.7);

    let mut water = floor.clone();
    water.water = Some((Vec3::Y * 5., Vec3::splat(100.)));
    p = Player::default();
    for _ in 0..20 {
        p.step(
            &Orders {
                jump_held: true,
                ..default()
            },
            &water,
            dt,
        );
    }
    assert_eq!(p.mode, Move::Swim);
    assert!(p.vel.y > 0.);
    p.step(
        &fwd(Orders {
            throw_pressed: true,
            throw_held: true,
            ..default()
        }),
        &water,
        dt,
    );
    assert_eq!(p.mode, Move::SwimDash);
    assert!(p.vel.z < 0.);
    let mut pole = floor.clone();
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
    p.step(&fwd(default()), &pole, dt);
    assert!(p.pos.y > y);
    p.step(
        &fwd(Orders {
            jump: true,
            ..default()
        }),
        &pole,
        dt,
    );
    assert_eq!(p.mode, Move::Backflip);
    let mut tunnel = floor.clone();
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
    let mut targets = floor.clone();
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
        p.step(&fwd(default()), &wall, dt);
    }
    assert!(p.pos.z >= -1.36);
    let mut events = 0;
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
        events += p.events.drain(..).count();
        assert!(p.pos.is_finite() && p.vel.is_finite());
    }
    assert!(events > 0);
}
