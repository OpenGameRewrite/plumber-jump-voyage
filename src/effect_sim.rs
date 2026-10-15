//! CPU particle simulation of the original effects, one step per 60 Hz frame.
//!
//! An `Instance` is one running effect: a tree of emitters that spawn
//! particles on the decoded schedule, move them with velocity, air
//! resistance and gravity, and describe them (size, colour, texture tile)
//! for the renderer. Data units are the original's: centimetres, frames.
//! Where the data does not say how a field combines with the others the
//! choice is marked "inferred".
use crate::effect_data::{
    AlphaChannel, ChannelMode, ColorChannel, Emitter, Follow, PatternKind, Resource, Shape, Volume,
    sample_keys,
};
use bevy::math::{EulerRot, Quat, Vec3};
use std::sync::Arc;

/// Metres per centimetre: the course is built in metres, the data in cm.
pub const METRES: f32 = 0.01;
const MAX_PARTICLES: usize = 512;
const INSET: f32 = 0.003;

/// Small deterministic generator (xorshift) so effects can be replayed.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }
    pub fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 32) as u32
    }
    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    /// Uniform in [-1, 1).
    pub fn signed(&mut self) -> f32 {
        self.unit() * 2. - 1.
    }
    pub fn direction(&mut self) -> Vec3 {
        let z = self.signed();
        let angle = self.unit() * std::f32::consts::TAU;
        let r = (1. - z * z).max(0.).sqrt();
        Vec3::new(r * angle.cos(), z, r * angle.sin())
    }
}

/// Where an effect sits. The translation is in metres, as in the course.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: f32,
}

impl Pose {
    pub fn new(position: Vec3, rotation: Quat) -> Self {
        Self {
            position,
            rotation,
            scale: 1.,
        }
    }
}

/// Emitters of one effect, flattened so children can refer to their parent.
pub struct Flat {
    pub emitters: Vec<Emitter>,
    pub parents: Vec<Option<usize>>,
}

impl Flat {
    pub fn new(resource: &Resource) -> Self {
        let mut flat = Flat {
            emitters: vec![],
            parents: vec![],
        };
        fn add(flat: &mut Flat, emitter: &Emitter, parent: Option<usize>) {
            let index = flat.emitters.len();
            let mut copy = emitter.clone();
            copy.children.clear();
            flat.emitters.push(copy);
            flat.parents.push(parent);
            for child in &emitter.children {
                add(flat, child, Some(index));
            }
        }
        for emitter in &resource.emitters {
            add(&mut flat, emitter, None);
        }
        flat
    }
}

#[derive(Clone)]
struct Particle {
    /// Centimetres in the emitter's follow space (see `Follow`).
    position: Vec3,
    /// Centimetres per frame, same space.
    velocity: Vec3,
    age: f32,
    life: f32,
    /// Size at birth, centimetres.
    size: Vec3,
    rotation: Vec3,
    spin: Vec3,
    random: [f32; 3],
    /// Orientation of the emitter's frame at birth (flat and mesh particles).
    frame: Quat,
    /// Bit per child emitter already started from this particle.
    children_started: u32,
}

#[derive(Default)]
struct EmitterRun {
    particles: Vec<Particle>,
    carry: f32,
    next_emission: u32,
}

/// One particle as the renderer needs it.
#[derive(Clone, Debug)]
pub struct Draw {
    pub emitter: usize,
    /// Metres, world space.
    pub position: Vec3,
    /// World direction of travel in centimetres per frame.
    pub velocity: Vec3,
    /// Metres.
    pub size: Vec3,
    pub rotation: Vec3,
    pub frame: Quat,
    /// Linear RGBA; colours above one glow.
    pub color: [f32; 4],
    /// The second colour the texture blends toward (see `Combiner`).
    pub color1: [f32; 3],
    /// u0, v0, u1, v1 of the texture tile.
    pub uv: [f32; 4],
}

/// One running effect.
pub struct Instance {
    flat: Arc<Flat>,
    runs: Vec<EmitterRun>,
    pose: Pose,
    follow_translation: bool,
    follow_rotation: bool,
    fade_on_stop: bool,
    /// Centimetres per frame the effect moves in the world.
    velocity: Vec3,
    age: u32,
    stopped_for: Option<u32>,
    rng: Rng,
}

/// How an effect is attached and stopped (from the table's flags).
#[derive(Clone, Copy, Debug, Default)]
pub struct Attach {
    pub follow_translation: bool,
    pub follow_rotation: bool,
    pub fade_on_stop: bool,
}

fn euler(rotation: [f32; 3]) -> Quat {
    Quat::from_euler(EulerRot::XYZ, rotation[0], rotation[1], rotation[2])
}

fn v3(a: [f32; 3]) -> Vec3 {
    Vec3::from_array(a)
}

/// Point on `shape` for the `index`-th of `count` particles, with the
/// outward direction particles leave along. Volumes lie in the emitter's
/// XZ plane with Y up (inferred from the data's shapes).
fn sample_volume(shape: &Shape, index: u32, count: u32, rng: &mut Rng) -> (Vec3, Vec3) {
    let radius = Vec3::new(
        shape.radius[0] * safe_scale(shape.form_scale[0]),
        shape.radius[1] * safe_scale(shape.form_scale[1]),
        shape.radius[2] * safe_scale(shape.form_scale[2]),
    );
    let sweep = if shape.sweep_longitude > 0. {
        shape.sweep_longitude
    } else {
        std::f32::consts::TAU
    };
    let divided = index_fraction(index, count);
    match shape.volume {
        Volume::Point | Volume::Primitive => (Vec3::ZERO, rng.direction()),
        Volume::Circle | Volume::CircleDivided | Volume::CircleFill => {
            let angle = shape.sweep_start
                + if shape.volume == Volume::CircleDivided {
                    sweep * divided + rng.signed() * shape.surface_random
                } else {
                    rng.unit() * sweep
                };
            let reach = if shape.volume == Volume::CircleFill {
                rng.unit().sqrt()
            } else {
                1.
            };
            let dir = Vec3::new(angle.cos(), 0., angle.sin());
            (dir * radius * reach, dir)
        }
        Volume::Sphere | Volume::SphereDivided | Volume::SphereDivided64 | Volume::SphereFill => {
            let dir = if matches!(
                shape.volume,
                Volume::SphereDivided | Volume::SphereDivided64
            ) && count > 1
            {
                // Evenly spread points on the sphere (golden-angle spiral).
                let y = 1. - 2. * (index as f32 + 0.5) / count as f32;
                let r = (1. - y * y).max(0.).sqrt();
                let a = index as f32 * 2.399_963;
                Vec3::new(r * a.cos(), y, r * a.sin())
            } else {
                rng.direction()
            };
            let reach = if shape.volume == Volume::SphereFill {
                rng.unit().cbrt()
            } else {
                1.
            };
            (dir * radius * reach, dir)
        }
        Volume::Cylinder | Volume::CylinderFill => {
            let angle = shape.sweep_start + rng.unit() * sweep;
            let reach = if shape.volume == Volume::CylinderFill {
                rng.unit().sqrt()
            } else {
                1.
            };
            let dir = Vec3::new(angle.cos(), 0., angle.sin());
            (
                Vec3::new(
                    dir.x * radius.x * reach,
                    rng.signed() * radius.y,
                    dir.z * radius.z * reach,
                ),
                dir,
            )
        }
        Volume::Box | Volume::BoxFill => {
            let p = Vec3::new(rng.signed(), rng.signed(), rng.signed()) * radius;
            (p, p.normalize_or(Vec3::Y))
        }
        Volume::Line | Volume::LineDivided => {
            let along = if shape.volume == Volume::LineDivided {
                divided - 0.5
            } else {
                rng.unit() - 0.5
            };
            (Vec3::new(0., 0., along * shape.line_length), Vec3::Y)
        }
        Volume::Rectangle => (
            Vec3::new(rng.signed() * radius.x, 0., rng.signed() * radius.z),
            Vec3::Y,
        ),
    }
}

/// A form scale of zero means the file did not set one.
fn safe_scale(value: f32) -> f32 {
    if value.abs() > 1e-6 { value } else { 1. }
}

fn index_fraction(index: u32, count: u32) -> f32 {
    if count > 1 {
        index as f32 / count as f32
    } else {
        0.
    }
}

/// `direction` turned randomly by up to `degrees` (inferred cone spread).
fn spread(direction: Vec3, degrees: f32, rng: &mut Rng) -> Vec3 {
    if degrees <= 0. {
        return direction;
    }
    let axis = direction.normalize_or(Vec3::Y);
    let tilt = rng.unit().sqrt() * degrees.to_radians();
    let turn = rng.unit() * std::f32::consts::TAU;
    let side = axis.any_orthonormal_vector();
    let rotated = Quat::from_axis_angle(axis, turn) * (Quat::from_axis_angle(side, tilt) * axis);
    rotated.normalize_or(axis) * direction.length()
}

/// Key animation value of an emitter at the effect's age, when it has one.
fn animated(emitter: &Emitter, kind: &str, age: u32) -> Option<Vec3> {
    let animation = emitter.attachment(kind)?;
    let last = animation.keys.last()?[3];
    let frame = if animation.looping != 0 && last > 0. {
        age as f32 % last
    } else {
        age as f32
    };
    let value = sample_keys(&animation.keys, frame, [0.; 4]);
    Some(Vec3::new(value[0], value[1], value[2]))
}

fn channel_time(looping: bool, loop_frames: i32, age: f32, life: f32) -> f32 {
    if looping {
        let period = if loop_frames > 0 {
            loop_frames as f32
        } else {
            life
        };
        if period.is_finite() && period > 0. {
            return (age % period) / period;
        }
    }
    if life.is_finite() && life > 0. {
        (age / life).clamp(0., 1.)
    } else {
        0.
    }
}

fn color_at(channel: &ColorChannel, t: f32, pick: f32) -> [f32; 3] {
    match channel.mode {
        ChannelMode::Keys => {
            let v = sample_keys(
                &channel.keys,
                t,
                [
                    channel.constant[0],
                    channel.constant[1],
                    channel.constant[2],
                    0.,
                ],
            );
            [v[0], v[1], v[2]]
        }
        ChannelMode::Random if !channel.keys.is_empty() => {
            let k = channel.keys[(pick * channel.keys.len() as f32) as usize % channel.keys.len()];
            [k[0], k[1], k[2]]
        }
        _ => channel.constant,
    }
}

fn alpha_at(channel: &AlphaChannel, t: f32, pick: f32) -> f32 {
    match channel.mode {
        ChannelMode::Keys => sample_keys(&channel.keys, t, [channel.constant, 0.])[0],
        ChannelMode::Random if !channel.keys.is_empty() => {
            channel.keys[(pick * channel.keys.len() as f32) as usize % channel.keys.len()][0]
        }
        _ => channel.constant,
    }
}

impl Instance {
    pub fn new(flat: Arc<Flat>, pose: Pose, attach: Attach, seed: u64) -> Self {
        let runs = flat
            .emitters
            .iter()
            .map(|_| EmitterRun::default())
            .collect();
        let mut instance = Self {
            runs,
            pose,
            follow_translation: attach.follow_translation,
            follow_rotation: attach.follow_rotation,
            fade_on_stop: attach.fade_on_stop,
            velocity: Vec3::ZERO,
            age: 0,
            stopped_for: None,
            rng: Rng::new(seed),
            flat,
        };
        for (run, emitter) in instance.runs.iter_mut().zip(&instance.flat.emitters) {
            run.next_emission = emitter.emission.start;
        }
        instance
    }

    /// The definition of the emitter a Draw came from.
    pub fn emitter(&self, index: usize) -> &Emitter {
        &self.flat.emitters[index]
    }

    pub fn particle_count(&self) -> usize {
        self.runs.iter().map(|r| r.particles.len()).sum()
    }

    /// Stop emitting; live particles finish (or fade at once for effects
    /// that fade when stopped). Particles that would live forever end now.
    pub fn stop(&mut self) {
        if self.stopped_for.is_some() {
            return;
        }
        self.stopped_for = Some(0);
        for (run, emitter) in self.runs.iter_mut().zip(&self.flat.emitters) {
            if emitter.life.infinite {
                let grace = emitter.color.alpha_fade_time.max(10) as f32;
                for p in &mut run.particles {
                    p.life = p.age + grace;
                }
            }
        }
    }

    /// True when nothing is left to draw and nothing more will be emitted.
    pub fn finished(&self) -> bool {
        let emitting = self.stopped_for.is_none()
            && self.flat.emitters.iter().any(|e| {
                !e.emission.one_time || self.age < e.emission.start + e.emission.duration.max(1)
            });
        !emitting && self.runs.iter().all(|r| r.particles.is_empty())
    }

    /// Advance one frame. `pose` is where the effect's anchor is now;
    /// `emit` is false while emitters that need it (`NoEmitAtNoCollide`) must wait.
    pub fn step(&mut self, pose: Option<Pose>, emit: bool) {
        if let Some(next) = pose {
            let moved = next.position - self.pose.position;
            self.velocity = if self.follow_translation {
                moved / METRES
            } else {
                Vec3::ZERO
            };
            if self.follow_translation {
                self.pose.position = next.position;
            }
            if self.follow_rotation {
                self.pose.rotation = next.rotation;
            }
            self.pose.scale = next.scale;
        }
        let stopped = self.stopped_for.is_some();
        for index in 0..self.runs.len() {
            if self.flat.parents[index].is_none() && !stopped && emit {
                self.emit_roots(index);
            }
            self.advance_particles(index);
        }
        for index in 0..self.runs.len() {
            if let Some(parent) = self.flat.parents[index] {
                self.start_children(parent, index);
            }
        }
        self.age += 1;
        if let Some(frames) = &mut self.stopped_for {
            *frames += 1;
        }
    }

    fn emit_roots(&mut self, index: usize) {
        let flat = Arc::clone(&self.flat);
        let emitter = &flat.emitters[index];
        let emission = &emitter.emission;
        if self.age < emission.start
            || (emission.one_time && self.age >= emission.start + emission.duration.max(1))
            || self.age < self.runs[index].next_emission
        {
            return;
        }
        let gap = emission.interval.max(1)
            + if emission.interval_random > 0 {
                self.rng.next_u32() % (emission.interval_random + 1)
            } else {
                0
            };
        self.runs[index].next_emission = self.age + gap;
        let wanted = emission.rate * (1. - self.rng.unit() * emission.rate_random / 100.)
            + self.runs[index].carry;
        let whole = wanted.floor().max(0.);
        self.runs[index].carry = wanted - whole;
        let divided = matches!(
            emitter.shape.volume,
            Volume::CircleDivided
                | Volume::SphereDivided
                | Volume::SphereDivided64
                | Volume::LineDivided
        );
        let per_ring = if divided {
            (emitter.shape.divide_circle.max(1) as u32)
                + if emitter.shape.divide_circle_random > 0 {
                    self.rng.next_u32() % (emitter.shape.divide_circle_random as u32 + 1)
                } else {
                    0
                }
        } else {
            1
        };
        // A divided volume emits its whole ring for each unit of rate.
        for _ in 0..whole as u32 {
            for i in 0..per_ring {
                self.spawn(index, i, per_ring, None);
            }
        }
    }

    /// Begin child emitters from the parent particles that reached their timing.
    fn start_children(&mut self, parent: usize, child: usize) {
        let flat = Arc::clone(&self.flat);
        let emitter = &flat.emitters[child];
        let Some(timing) = emitter.emission.child_timing else {
            return;
        };
        let bit = 1u32 << (child.min(31) as u32);
        let ready: Vec<usize> = self.runs[parent]
            .particles
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                p.children_started & bit == 0 && p.age >= p.life.min(1e9) * timing / 100.
            })
            .map(|(i, _)| i)
            .collect();
        for i in ready {
            self.runs[parent].particles[i].children_started |= bit;
            let origin = (
                self.runs[parent].particles[i].position,
                self.runs[parent].particles[i].velocity,
            );
            let count = emitter.emission.rate.round().max(1.) as u32;
            for k in 0..count {
                self.spawn(child, k, count, Some(origin));
            }
        }
    }

    fn spawn(&mut self, index: usize, k: u32, count: u32, parent: Option<(Vec3, Vec3)>) {
        if self.runs[index].particles.len() >= MAX_PARTICLES {
            return;
        }
        let flat = Arc::clone(&self.flat);
        let emitter = &flat.emitters[index];
        let rng = &mut self.rng;
        let age = self.age;
        // Key animations of the emitter override the base values (inferred
        // from the keys starting at the base value).
        let base = animated(emitter, "EASL", age).unwrap_or(v3(emitter.scale.base));
        let life_frames =
            animated(emitter, "EAPL", age).map_or(emitter.life.frames as f32, |l| l.x);
        let shift = animated(emitter, "EATR", age).unwrap_or(Vec3::ZERO);
        let (local, outward) = sample_volume(&emitter.shape, k, count, rng);
        let jitter = rng.direction() * rng.unit() * emitter.emission.position_random;
        let wobble = Vec3::new(rng.signed(), rng.signed(), rng.signed())
            * v3(emitter.transform.translate_random);
        let turn = euler([
            emitter.transform.rotate[0] + rng.signed() * emitter.transform.rotate_random[0],
            emitter.transform.rotate[1] + rng.signed() * emitter.transform.rotate_random[1],
            emitter.transform.rotate[2] + rng.signed() * emitter.transform.rotate_random[2],
        ]);
        let start = turn * (local + jitter) + v3(emitter.transform.translate) + wobble + shift;
        let v = &emitter.velocity;
        let speed = v.all_direction * (1. - rng.unit() * v.random / 100.);
        let aimed = spread(
            v3(v.direction).normalize_or(Vec3::Y) * v.direction_speed,
            v.cone_degrees,
            rng,
        );
        let loose = Vec3::new(rng.signed(), rng.signed(), rng.signed()) * v3(v.diffusion);
        let mut velocity = turn * (outward * speed + aimed + loose);
        velocity *= 1. + rng.signed() * emitter.forces.momentum_random;
        let pose = self.pose;
        let (position, velocity, frame) = match emitter.follow {
            Follow::World => (
                pose.position / METRES + pose.rotation * (start * pose.scale),
                pose.rotation * (velocity * pose.scale) + self.velocity * v.inherit,
                pose.rotation * turn,
            ),
            Follow::Translate => (
                pose.rotation * (start * pose.scale),
                pose.rotation * (velocity * pose.scale),
                pose.rotation * turn,
            ),
            Follow::All => (start * pose.scale, velocity * pose.scale, turn),
        };
        let (position, velocity) = match parent {
            Some((origin, _)) => (position + origin, velocity),
            None => (position, velocity),
        };
        let mut life = life_frames * (1. - rng.unit() * emitter.life.random as f32 / 100.);
        if emitter.life.infinite {
            life = f32::INFINITY;
        }
        let shrink = 1. - rng.unit() * emitter.scale.random[0] / 100.;
        let r = &emitter.rotation;
        let sign = |axis: usize, rng: &mut Rng| {
            if r.reverse[axis] != 0 && rng.unit() < 0.5 {
                -1.
            } else {
                1.
            }
        };
        let rotation = Vec3::new(
            r.start[0] + rng.unit() * r.start_random[0],
            r.start[1] + rng.unit() * r.start_random[1],
            r.start[2] + rng.unit() * r.start_random[2],
        );
        let spin = Vec3::new(
            (r.add[0] + rng.signed() * r.add_random[0]) * sign(0, rng),
            (r.add[1] + rng.signed() * r.add_random[1]) * sign(1, rng),
            (r.add[2] + rng.signed() * r.add_random[2]) * sign(2, rng),
        );
        let particle = Particle {
            position,
            velocity,
            age: 0.,
            life: life.max(1.),
            size: base * shrink,
            rotation,
            spin,
            random: [rng.unit(), rng.unit(), rng.unit()],
            frame,
            children_started: 0,
        };
        self.runs[index].particles.push(particle);
    }

    fn advance_particles(&mut self, index: usize) {
        let flat = Arc::clone(&self.flat);
        let emitter = &flat.emitters[index];
        let forces = &emitter.forces;
        let air = if forces.air_resistance > 0. {
            forces.air_resistance
        } else {
            1.
        };
        let mut gravity = v3(forces.gravity_dir).normalize_or_zero() * forces.gravity;
        if emitter.follow == Follow::All && forces.world_gravity {
            gravity = self.pose.rotation.inverse() * gravity;
        } else if emitter.follow != Follow::All && !forces.world_gravity {
            gravity = self.pose.rotation * gravity;
        }
        let drag = emitter.rotation.resistance;
        self.runs[index].particles.retain_mut(|p| {
            p.age += 1.;
            p.position += p.velocity;
            p.velocity = p.velocity * air + gravity;
            p.rotation += p.spin;
            if drag > 0. && drag < 1. {
                p.spin *= drag;
            }
            p.age <= p.life
        });
    }

    /// Describe every live particle for drawing.
    pub fn draws(&self, out: &mut Vec<Draw>) {
        let pose = self.pose;
        let fade = match self.stopped_for {
            Some(frames) if self.fade_on_stop => {
                let span = self
                    .flat
                    .emitters
                    .iter()
                    .map(|e| e.color.alpha_fade_time)
                    .max()
                    .unwrap_or(0)
                    .max(10);
                (1. - frames as f32 / span as f32).max(0.)
            }
            _ => 1.,
        };
        for (index, (emitter, run)) in self.flat.emitters.iter().zip(&self.runs).enumerate() {
            let tint0 = tint(emitter.color.tint0);
            let tint1 = tint(emitter.color.tint1);
            let (tint, tint_alpha) = (Vec3::new(tint0[0], tint0[1], tint0[2]), tint0[3]);
            let tint1 = Vec3::new(tint1[0], tint1[1], tint1[2]);
            let slot = emitter.textures.first().and_then(|t| t.as_ref());
            let boost = animated(emitter, "EAA0", self.age).map_or(1., |a| a.x);
            for p in &run.particles {
                let t = channel_time(
                    emitter.color.loop_color0,
                    emitter.color.loop_frames.first().copied().unwrap_or(0),
                    p.age,
                    p.life,
                );
                let ta = channel_time(
                    emitter.color.loop_alpha0,
                    emitter.color.loop_frames.get(1).copied().unwrap_or(0),
                    p.age,
                    p.life,
                );
                let mut rgb =
                    Vec3::from_array(color_at(&emitter.color.color0, t, p.random[1])) * tint;
                let glow = if emitter.color.scale > 0. {
                    emitter.color.scale
                } else {
                    1.
                };
                rgb *= glow;
                let rgb1 = Vec3::from_array(color_at(&emitter.color.color1, t, p.random[2]))
                    * tint1
                    * glow;
                let mut alpha =
                    alpha_at(&emitter.color.alpha0, ta, p.random[1]) * tint_alpha * fade;
                if emitter.color.alpha_fade != 0
                    && emitter.color.alpha_fade_time > 0
                    && p.life.is_finite()
                {
                    alpha *=
                        ((p.life - p.age) / emitter.color.alpha_fade_time as f32).clamp(0., 1.);
                }
                if emitter.color.alpha_fade_in != 0 && emitter.color.fade_in_time > 0 {
                    alpha *= (p.age / emitter.color.fade_in_time as f32).clamp(0., 1.);
                }
                alpha *= if emitter.attachment("EAA0").is_some() {
                    boost.clamp(0., 4.)
                } else {
                    1.
                };
                let curve_t = channel_time(
                    emitter.scale.looping,
                    emitter.color.loop_frames.get(4).copied().unwrap_or(0),
                    p.age,
                    p.life,
                );
                let key = sample_keys(&emitter.scale.curve, curve_t, [1., 1., 1., 0.]);
                let size = p.size * Vec3::new(key[0], key[1], key[2]) * pose.scale * METRES;
                let (position, velocity, frame) = match emitter.follow {
                    Follow::World => (p.position, p.velocity, p.frame),
                    Follow::Translate => (p.position + pose.position / METRES, p.velocity, p.frame),
                    Follow::All => (
                        pose.position / METRES + pose.rotation * p.position,
                        pose.rotation * p.velocity,
                        pose.rotation * p.frame,
                    ),
                };
                out.push(Draw {
                    emitter: index,
                    position: position * METRES,
                    velocity,
                    size,
                    rotation: p.rotation,
                    frame,
                    color: [rgb.x, rgb.y, rgb.z, alpha.clamp(0., 1.)],
                    color1: rgb1.to_array(),
                    uv: tile_uv(slot, p),
                });
            }
        }
    }
}

/// An emitter tint; a missing one (all zero) is white.
fn tint(value: [f32; 4]) -> [f32; 4] {
    if value == [0.; 4] { [1.; 4] } else { value }
}

/// Texture rectangle of the particle's flip-book tile (u0, v0, u1, v1).
fn tile_uv(slot: Option<&crate::effect_data::TextureSlot>, p: &Particle) -> [f32; 4] {
    let Some(slot) = slot else {
        return [0., 0., 1., 1.];
    };
    let (cols, rows) = (slot.grid[0].max(1), slot.grid[1].max(1));
    let pattern = &slot.pattern;
    let count = pattern.count.max(1) as f32;
    let per_tile = pattern.frames_per_tile.max(1e-3);
    let step = match pattern.kind {
        PatternKind::None => 0.,
        PatternKind::Loop => {
            let offset = if pattern.loop_random != 0 {
                p.random[0] * count
            } else {
                0.
            };
            ((p.age / per_tile + offset) % count).floor()
        }
        PatternKind::Clamp => (p.age / per_tile).floor().min(count - 1.),
        PatternKind::LifeFit => {
            let t = if p.life.is_finite() {
                p.age / p.life
            } else {
                0.
            };
            (t * count).floor().min(count - 1.)
        }
        PatternKind::Random => {
            let range = if pattern.random_range >= 1. {
                pattern.random_range
            } else {
                count
            };
            (p.random[0] * range).floor()
        }
    };
    let tile = match &pattern.table {
        Some(table) if !table.is_empty() => table[step as usize % table.len()].max(0) as u32,
        _ => step as u32,
    };
    let (col, row) = (tile % cols, (tile / cols) % rows);
    let (w, h) = (1. / cols as f32, 1. / rows as f32);
    [
        col as f32 * w + INSET * w,
        row as f32 * h + INSET * h,
        (col + 1) as f32 * w - INSET * w,
        (row + 1) as f32 * h - INSET * h,
    ]
}

pub fn self_check() {
    use crate::effect_data::{Emission, Life, Scale};
    // The decoded schedule: one-time emission for `duration` frames, an
    // emission every `interval`, a fractional rate carried between them.
    let emitter = Emitter {
        emission: Emission {
            one_time: true,
            duration: 6,
            interval: 2,
            rate: 1.5,
            ..Emission::default()
        },
        life: Life {
            frames: 10,
            ..Default::default()
        },
        scale: Scale {
            base: [50.; 3],
            ..Default::default()
        },
        ..Default::default()
    };
    let flat = Arc::new(Flat::new(&Resource {
        emitters: vec![emitter],
    }));
    let mut instance = Instance::new(
        flat.clone(),
        Pose::new(Vec3::ZERO, Quat::IDENTITY),
        Attach::default(),
        7,
    );
    let mut born = vec![];
    for _ in 0..8 {
        let before = instance.particle_count();
        instance.step(None, true);
        born.push(instance.particle_count() as i32 - before as i32);
    }
    assert_eq!(
        born.iter().sum::<i32>(),
        4,
        "1.5 per emission, three emissions: 1 + 2 + 1"
    );
    assert_eq!(born[0], 1, "the first emission is on the first frame");
    assert!(!instance.finished());
    for _ in 0..12 {
        instance.step(None, true);
    }
    assert!(instance.finished(), "particles die with their life");
    // Stopping ends emission of a looping emitter at once.
    let looping = Emitter {
        emission: Emission {
            interval: 1,
            rate: 1.,
            ..Emission::default()
        },
        life: Life {
            frames: 5,
            ..Default::default()
        },
        ..Default::default()
    };
    let flat = Arc::new(Flat::new(&Resource {
        emitters: vec![looping],
    }));
    let mut instance = Instance::new(
        flat,
        Pose::new(Vec3::ZERO, Quat::IDENTITY),
        Attach::default(),
        3,
    );
    for _ in 0..20 {
        instance.step(None, true);
    }
    assert!(instance.particle_count() >= 4 && !instance.finished());
    instance.stop();
    for _ in 0..8 {
        instance.step(None, true);
    }
    assert!(instance.finished(), "a stopped effect drains");
    let (_, dir) = sample_volume(
        &Shape {
            volume: Volume::Circle,
            radius: [10.; 3],
            form_scale: [1.; 3],
            ..Default::default()
        },
        0,
        1,
        &mut Rng::new(1),
    );
    assert!(
        dir.y.abs() < 1e-6 && (dir.length() - 1.).abs() < 1e-4,
        "rings leave horizontally"
    );
    assert!((channel_time(false, 0, 5., 10.) - 0.5).abs() < 1e-6);
    assert!((channel_time(true, 4, 5., 10.) - 0.25).abs() < 1e-6);
    let slot = crate::effect_data::TextureSlot {
        grid: [4, 2],
        pattern: crate::effect_data::Pattern {
            kind: PatternKind::Clamp,
            count: 8,
            frames_per_tile: 2.,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut p = Particle {
        position: Vec3::ZERO,
        velocity: Vec3::ZERO,
        age: 5.,
        life: 20.,
        size: Vec3::ONE,
        rotation: Vec3::ZERO,
        spin: Vec3::ZERO,
        random: [0.; 3],
        frame: Quat::IDENTITY,
        children_started: 0,
    };
    let uv = tile_uv(Some(&slot), &p);
    assert!(
        uv[0] > 0.5 && uv[0] < 0.51 && uv[1] < 0.01,
        "tile 2 of a 4x2 sheet is on the top row"
    );
    p.age = 100.;
    let uv = tile_uv(Some(&slot), &p);
    assert!(uv[0] > 0.75 && uv[1] > 0.5, "clamped to the last tile");
}
