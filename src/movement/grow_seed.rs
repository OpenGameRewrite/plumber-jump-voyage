//! `GrowFlowerSeed`: the placed seed that rests on the ground, is carried and thrown.
//!
//! The actor runs its nerve, then `control`, then its collider (`LiveActor::movement`). While it
//! waits, the state `GrowSeedStateWait` pulls it down, bounces it off what it hits and ties it to
//! the ground it comes to rest on. A hold ends in `GrowFlowerSeed::exeThrow`, the actor
//! falling into a death area revives after 300 steps.
//!
//! Ported: the throw (`GrowSeedUtil::throwFromPlayer`, the arrow check that moves the seed to the
//! player), flight and landing on static terrain with `ActorCollider`, resting (the ground
//! connector) and the death area. Not ported (never reached by the fixtures, each listed in the
//! port notes): water, quicksand and jump-pad floors, other actors' touch messages, moving
//! terrain, the seed's rolling visual and the random orientation of a revival.
use super::actor_collider::{ActorCollider, StrikeHit, Terrain};
use super::pose_math::{ActorPose, dot, squared_length, try_normalize_or_zero};
use super::*;

/// Original ObjectData/GrowFlowerSeed InitCollider: Radius=40, Y=40.
const COLLIDER_RADIUS: f32 = 40.;
/// Saved by initAfterPlacement (0x2913e8), restored by exeWait (0x29181c).
const COLLIDER_OFFSET: f32 = 40.;
/// `GrowSeedStateWait`'s parameters (data at 0x18bafb0): gravity, then horizontal and vertical
/// scale in the air and on the ground.
const GRAVITY: f32 = 1.8;
const AIR_SCALE: (f32, f32) = (1.0, 0.995);
const GROUND_SCALE: (f32, f32) = (0.85, 0.8);
/// A ground contact steeper than this does not count as ground.
const GROUND_ANGLE_DEGREES: f32 = 60.;
/// The rebound keeps this share of the speed and gives back this share of the normal speed.
const REBOUND: f32 = 0.7;
/// The seed rests when its horizontal speed is under this.
const REST_SPEED: f32 = 1.5;
/// Steps a landing counts as one contact for the hit reactions (`GrowSeedStateWait +0x50`).
const CONTACT_COUNTER: i32 = 10;
/// `exeRevival`: steps hidden before the seed returns to its placement.
const REVIVAL_STEPS: u32 = 300;
/// A seed is picked up once its pickup counter reached this (`GrowFlowerSeed::receiveMsg`).
pub(super) const PICKUP_AGE: u32 = 31;
/// The throw helper scales the body sensor's radius by this for its arrow check.
const ARROW_SENSOR_SCALE: f32 = 1.5;
const BODY_SENSOR_RADIUS: f32 = 50.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Nerve {
    Wait,
    Hold,
    Throw,
    Revival,
}

/// The two nerves of `GrowSeedStateWait` that matter on land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaitState {
    NoConnect,
    Connecting,
}

#[derive(Clone, Debug)]
pub struct Seed {
    pub id: String,
    pub position: Vec3,
    pub velocity: Vec3,
    /// `GrowFlowerSeed +0x17c`: control steps since the last throw (a pickup needs 31).
    pub(super) age: u32,
    nerve: Nerve,
    pending_nerve: Option<Nerve>,
    nerve_step: u32,
    state: WaitState,
    pending_state: Option<WaitState>,
    state_step: u32,
    /// The pose the actor was last given (a front-up quaternion after a throw).
    quat: Quat,
    /// The up axis of the actor's base matrix: it follows the pose at the end of each update.
    pose_up: Vec3,
    collider: ActorCollider,
    collide_off: bool,
    contact_counter: i32,
    /// The bounce happens on every second contact (`GrowSeedStateWait +0x62`).
    rebound_pending: bool,
    /// The collision part the seed came to rest on (the ground connector's host).
    connection: Option<usize>,
    thrown: bool,
    pub(super) revival_rng_unavailable: bool,
}

impl Seed {
    fn new(id: String, position: Vec3) -> Self {
        Self {
            id,
            position,
            velocity: Vec3::ZERO,
            age: 1,
            nerve: Nerve::Wait,
            pending_nerve: None,
            nerve_step: 0,
            state: WaitState::NoConnect,
            pending_state: None,
            state_step: 0,
            quat: Quat::IDENTITY,
            pose_up: Vec3::Y,
            collider: ActorCollider::new(COLLIDER_RADIUS, COLLIDER_OFFSET, position),
            collide_off: false,
            contact_counter: 0,
            rebound_pending: false,
            connection: None,
            thrown: false,
            revival_rng_unavailable: false,
        }
    }

    fn set_nerve(&mut self, nerve: Nerve) {
        self.pending_nerve = Some(nerve);
    }

    fn set_state(&mut self, state: WaitState) {
        self.pending_state = Some(state);
    }

    /// Applies the nerve change asked for during the previous update.
    fn enter_pending_nerve(&mut self) {
        if let Some(nerve) = self.pending_nerve.take() {
            self.nerve = nerve;
            self.nerve_step = 0;
            if nerve == Nerve::Wait {
                // The state is started afresh with the nerve.
                self.state = WaitState::NoConnect;
                self.pending_state = None;
                self.state_step = 0;
            }
        }
    }

    /// `MapObjStatePlayerHold::exeHold`: the seed follows the carrying pose.
    pub(super) fn follow_hold(&mut self, pose: &look_joint::Mat34) {
        if self.nerve != Nerve::Hold {
            self.nerve = Nerve::Hold;
            self.pending_nerve = None;
            self.nerve_step = 0;
            self.velocity = Vec3::ZERO;
        }
        self.position = look_joint::translation(pose);
        self.pose_up = Vec3::new(pose[0][1], pose[1][1], pose[2][1]);
        self.nerve_step += 1;
        self.age += 1;
    }

    /// `GrowSeedStateRevival::appear` invalidates the hit sensors until the seed is back.
    pub(super) fn hit_sensors_valid(&self) -> bool {
        self.nerve != Nerve::Revival
    }

    pub(super) fn can_pick_up(&self) -> bool {
        self.nerve == Nerve::Wait && self.age >= PICKUP_AGE
    }

    fn update(&mut self, mesh: &crate::geometry::TriangleMesh) {
        self.enter_pending_nerve();
        match self.nerve {
            Nerve::Hold => {}
            Nerve::Throw => self.exe_throw(),
            Nerve::Wait => self.exe_wait(mesh),
            Nerve::Revival => self.exe_revival(),
        }
        self.nerve_step += 1;
        self.control(mesh);
        self.update_collider(mesh);
        // The model's pose, and with it the base matrix, is brought up to date last.
        self.pose_up = ActorPose::new(self.quat).up();
    }

    /// `GrowFlowerSeed::exeThrow`, a single step: the collision starts afresh and the seed waits.
    fn exe_throw(&mut self) {
        if self.nerve_step == 0 {
            self.collide_off = false;
            self.collider.invalidate(self.position, self.pose_up);
            self.age = 0;
        }
        self.set_nerve(Nerve::Wait);
    }

    /// `GrowFlowerSeed::exeWait` on land: the state decides what the seed does this step.
    fn exe_wait(&mut self, mesh: &crate::geometry::TriangleMesh) {
        if self.nerve_step == 0 {
            self.collider.set_offset_y(COLLIDER_OFFSET);
        }
        if let Some(state) = self.pending_state.take() {
            self.state = state;
            self.state_step = 0;
        }
        match self.state {
            WaitState::NoConnect => self.exe_no_connect(mesh),
            WaitState::Connecting => self.exe_connecting(mesh),
        }
        self.state_step += 1;
    }

    /// `GrowSeedStateWait::exeNoConnect`: bounce, come to rest, gravity and drag.
    fn exe_no_connect(&mut self, mesh: &crate::geometry::TriangleMesh) {
        if self.state_step == 0 {
            self.collide_off = false;
            self.collider.invalidate(self.position, self.pose_up);
            self.contact_counter = CONTACT_COUNTER;
        }
        let ground = self
            .collider
            .floor
            .is_present()
            .then_some(self.collider.floor.hit);
        let ground_normal = ground
            .map(|hit| hit.normal)
            .filter(|normal| angle_to_up_degrees(*normal) < GROUND_ANGLE_DEGREES);
        let wall = self
            .collider
            .wall
            .is_present()
            .then_some(self.collider.wall.hit);
        let touching = ground_normal.is_some() || wall.is_some();
        let contact_normal = ground_normal.or(wall.map(|hit| hit.normal));
        if ground_normal.is_some() {
            self.contact_counter = CONTACT_COUNTER;
            let horizontal = Vec3::new(self.velocity.x, 0., self.velocity.z);
            if squared_length(horizontal) < REST_SPEED * REST_SPEED {
                self.velocity = Vec3::ZERO;
                self.connection = mesh.triangles[self.collider.floor.hit.face as usize]
                    .local
                    .map(|local| local.part as usize);
                self.set_state(WaitState::Connecting);
            }
        }
        if touching {
            let first = !self.rebound_pending;
            self.rebound_pending = first;
            if let (true, Some(normal)) = (first, contact_normal) {
                let into = dot(normal, self.velocity);
                if into < 0. {
                    self.velocity -= normal * into;
                    self.velocity *= REBOUND;
                    self.velocity -= normal * into * REBOUND;
                }
            }
            self.contact_counter = CONTACT_COUNTER;
        }
        self.velocity.y -= GRAVITY;
        let (horizontal, vertical) = if ground_normal.is_some() {
            GROUND_SCALE
        } else {
            AIR_SCALE
        };
        self.velocity = Vec3::new(
            self.velocity.x * horizontal,
            self.velocity.y * vertical,
            self.velocity.z * horizontal,
        );
        self.contact_counter -= 1;
    }

    /// `GrowSeedStateWait::exeConnecting`: tied to the ground, the seed no longer collides.
    fn exe_connecting(&mut self, mesh: &crate::geometry::TriangleMesh) {
        if self.state_step == 0 {
            self.collide_off = true;
        }
        self.velocity = Vec3::ZERO;
        // The connector carries the seed with its host: out of the part's frame and back in.
        // For terrain that stands still this is the identity up to single-precision noise.
        if let Some(part) = self.connection {
            let frame = &mesh.parts[part];
            let local = crate::prism::transform_point(&frame.inverse, self.position.to_array());
            self.position = Vec3::from_array(crate::prism::transform_point(&frame.base, local));
        }
    }

    /// Where the seed's body sensor is: its offset follows the actor's pose.
    pub(super) fn body_sensor(&self, offset: f32) -> Vec3 {
        if self.nerve == Nerve::Hold {
            self.position + Vec3::Y * offset
        } else {
            self.position + self.pose_up * offset
        }
    }

    /// `GrowFlowerSeed::exeRevival`: hidden for 300 steps, then back at the placement.
    fn exe_revival(&mut self) {
        if self.nerve_step == 0 {
            self.velocity = Vec3::ZERO;
            self.collide_off = true;
        }
        if self.nerve_step >= REVIVAL_STEPS {
            // The next state requires the actor's independently initialized random source.
            // Keep the unsupported revival visible instead of inventing an orientation.
            self.revival_rng_unavailable = true;
        }
    }

    pub(super) fn throw_diagnostic(&self) -> String {
        if !self.thrown {
            return String::new();
        }
        let name = |nerve| match nerve {
            Nerve::Wait => "Wait",
            Nerve::Hold => "Hold",
            Nerve::Throw => "Throw",
            Nerve::Revival => "Revival",
        };
        format!(
            ";seed_velocity={};seed_quat={};seed_nerve={}/{};seed_collide_off={}",
            super::carry::hex_vec(self.velocity),
            // The probe observes al::calcQuat: matrix -> quaternion, not getQuatPtr.
            ActorPose::new(self.quat)
                .calc_quat()
                .to_array()
                .map(|v| format!("{:08x}", v.to_bits()))
                .join("/"),
            name(self.pending_nerve.unwrap_or(self.nerve)),
            if self.pending_nerve.is_some() {
                0
            } else {
                self.nerve_step
            },
            u8::from(self.collide_off),
        )
    }

    /// `GrowFlowerSeed::control`: the death area sends the seed to revival.
    fn control(&mut self, mesh: &crate::geometry::TriangleMesh) {
        let in_death_area = mesh.death.volume_at(self.position).is_some();
        if in_death_area && !matches!(self.nerve, Nerve::Hold | Nerve::Revival) {
            self.set_nerve(Nerve::Revival);
        } else {
            self.age += 1;
        }
    }

    /// `GrowFlowerSeed::updateCollider`: no collision while it is held.
    fn update_collider(&mut self, mesh: &crate::geometry::TriangleMesh) {
        if self.nerve == Nerve::Hold {
            return;
        }
        if self.collide_off {
            self.position += self.velocity;
            self.collider.invalidate(self.position, self.pose_up);
        } else {
            self.position +=
                self.collider
                    .collide(mesh, self.position, self.pose_up, self.velocity);
        }
    }
}

fn angle_to_up_degrees(normal: Vec3) -> f32 {
    normal.dot(Vec3::Y).clamp(-1., 1.).acos().to_degrees()
}

impl Terrain for crate::geometry::TriangleMesh {
    fn strike_sphere(&self, centre: Vec3, radius: f32) -> Vec<StrikeHit> {
        crate::collider::sphere_strike_hits_game(self, centre, radius)
            .into_iter()
            .map(|(face, hit)| StrikeHit {
                face,
                normal: self.triangles[face as usize].normal,
                depth: hit.depth,
                position: hit.position,
                centre,
                at_face: hit.is_face(),
            })
            .collect()
    }
}

impl Player {
    /// Only placement/init state enters the replay. No carry observation file is loaded.
    pub fn init_flower_seed(&mut self, mesh: &crate::geometry::TriangleMesh) {
        self.carry.origin = mesh.to_game_vec(Vec3::ZERO);
        self.carry.stone = mesh.kick_stones.first().map(super::kick_stone::Stone::new);
        if let Some(source) = mesh
            .carry_sources
            .iter()
            .find(|s| s.class == "GrowFlowerSeed" && s.active)
        {
            self.carry.seed = Some(Seed::new(source.id.clone(), source.position));
        }
    }

    /// One update of the seed actor, in the original's order for the frame.
    pub(super) fn update_flower_seed(&mut self, scene: &Course) {
        let Some(mesh) = scene.mesh.as_deref() else {
            return;
        };
        let held_pose = self.carry.held.then_some(self.carry.hold).flatten();
        let Some(seed) = &mut self.carry.seed else {
            return;
        };
        if self.carry.held {
            if let Some(pose) = held_pose {
                seed.follow_hold(&pose);
            }
            return;
        }
        seed.update(mesh);
    }

    /// `GrowFlowerSeed::receiveMsg` for PlayerRelease: the pose turns to the player's front, the
    /// throw helper (0x291934) aims it, and the seed starts its throw.
    pub(super) fn release_flower_seed(&mut self, scene: &Course) {
        let (_, front) = self.pose_axes();
        let flat_front = try_normalize_or_zero(Vec3::new(front.x, 0., front.z)).0;
        let direction = pose_math::turn_vec_to_vec_degree(flat_front, Vec3::Y, 70.);
        let player_velocity = self.units_per_frame().with_y(0.);
        // `throwFromPlayer` normalises the turned direction and `setVelocityToDirection`
        // normalises it again before it scales it by the speed.
        let aimed = try_normalize_or_zero(try_normalize_or_zero(direction).0).0;
        let velocity = aimed * 28. + player_velocity;
        let player_position = self.position_cm();
        let Some(seed) = &mut self.carry.seed else {
            return;
        };
        seed.quat = pose_math::quat_front_up(front, Vec3::Y);
        seed.thrown = true;
        seed.velocity = velocity;
        if let Some(mesh) = scene.mesh.as_deref() {
            // The helper tests an arrow behind the seed (as long as the distance of its
            // outer point from the world origin) and moves the seed to the player on a hit.
            let direction = try_normalize_or_zero(velocity).0;
            let reach = ARROW_SENSOR_SCALE * BODY_SENSOR_RADIUS;
            let length = squared_length(seed.position + direction * reach).sqrt();
            let arrow = direction * length;
            if crate::collider::first_strike(
                mesh,
                mesh.relative_vec(seed.position - arrow),
                arrow,
                crate::collider::FaceFilter::Any,
            )
            .is_some()
            {
                seed.position = mesh.to_game_vec(player_position);
            }
        }
        seed.collide_off = false;
        seed.collider.invalidate(seed.position, seed.pose_up);
        seed.set_nerve(Nerve::Throw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revival_cannot_be_picked_up_or_fake_its_unrecorded_random_orientation() {
        let mut seed = Seed::new("test".into(), Vec3::ZERO);
        seed.age = PICKUP_AGE;
        assert!(seed.can_pick_up());
        seed.nerve = Nerve::Revival;
        seed.velocity = Vec3::Y;
        seed.exe_revival();
        assert_eq!(seed.velocity, Vec3::ZERO);
        assert!(seed.collide_off);
        assert!(!seed.can_pick_up());
        seed.nerve_step = REVIVAL_STEPS - 1;
        seed.exe_revival();
        assert!(!seed.revival_rng_unavailable);
        seed.nerve_step += 1;
        seed.exe_revival();
        assert!(seed.revival_rng_unavailable);
        assert_eq!(seed.nerve, Nerve::Revival);
        assert!(seed.pending_nerve.is_none());
    }
}
