//! Placed `KillerLauncher` actors and the `Killer`s (Bullet Bills) they fire.
//!
//! Mario's head looks at the nearest enemy in reach, so a Killer flying by is a look target
//! (`look_at.rs`). The recordings that need it (the Sand lift runs) never touch a Killer; what
//! they need is where it is and when it exists. Both follow from the placements:
//!
//! * A `SwitchKeepOnArea` placed with a `SwitchAreaOn` link keeps its stage switch on while the
//!   player is inside the area (`al::SwitchKeepOnAreaGroup::update` 0x807b04). Several areas
//!   may drive one switch: the group writes it its final value, on while any of them holds the
//!   player, so a switch never flickers inside one update.
//! * A launcher with a `SwitchStart` link starts stopped. The switch turning on starts it
//!   (`KillerLauncher::startLaunch` 0x148138), the switch turning off ends it (`endLaunch`
//!   0x14819c). A started launcher waits `DelayFrame` steps (`exeDelay` 0x148338), stands by
//!   (`exeStandBy` 0x1483a8: it picks a dead Killer of its group of four, puts it in front of the
//!   barrel and waits 60 steps), and launches it (`exeLaunch` 0x14851c: `Killer::launch` 0x1466dc
//!   with `AliveFrame`). It then waits `WaitFrame` frames, counted from the start of the stand-by,
//!   for the next one (`exeWait` 0x148388; `control` 0x1482ec counts the frames).
//! * A launched Killer accelerates along its front and damps its speed (`exeLaunch` 0x1467e0),
//!   26 steps in the nerve, then flies (`exeFly` 0x1468e8): every step it turns its front toward
//!   the nearest player along the horizontal plane (`al::turnToTarget`, 2.4 degrees, less when
//!   it is closer than 1100 cm), accelerates and damps again. `Killer::control` 0x1460c8 counts
//!   the alive frames down from `AliveFrame` and explodes at zero.
//!
//! The frame order is the executable's (`al::ExecuteDirector`, Update table 0x1ddfad8, entries
//! in order): the hit sensors (6), the player's movement (30, which includes the head's look-at),
//! the enemies (37: the launchers, then the Killers [I]), the stage-switch watchers (65), and
//! after the whole table `al::SwitchAreaDirector::update` (0x4d1854), which samples the player's
//! position and writes the switches. A watcher reports a switch only as the difference to its
//! last sample (`StageSwitchWatcher::update` 0xa27ec8), so the area's write reaches a launcher
//! in the next update. `Killers::update_after_player` is that tail of the frame.
//!
//! A Killer's hit sensor holds the position its actor had when entry 6 ran, so the head's
//! look-at (inside entry 30) and the probe's sample (Mario's model animation, entry 31) see the
//! Killers where the previous frame's enemy update left them (`Killers::update_sensors`).
//! The probe places the player after his movement (`hook_movement`, `afterMovement`), so the
//! enemies, watchers and areas of that frame already see the placement and a replay's first
//! recorded frame is the next one (`Player::run_placement_frame_tail`).
//!
//! Not ported: the Killer's contact with the player, with the stage's collision and with other
//! actors (its collider, `attackSensor` 0x14588c, `receiveMsg` 0x145b5c and the explosion
//! nerves). A Killer that hits something explodes in the game; the replays that use this module
//! have none that does. The explosion at the end of the alive count removes the Killer at once.
//! Only the plain Killer is ported (not `KillerLauncherMagnum`, a stage-switch kill link or a
//! launcher that starts by itself: those start with the camera clipping).
//!
//! `IsNoCap` (read at 0x147ed8, copied to Killer +0x137 at 0x147f90) is modelled as true, which
//! the exporter checks. What it controls is not ported: the Killer's cap setup (0x145704) and
//! the effects it starts (0x146800 in `exeLaunch`, 0x14703c). The flight does not read it.
use super::{pose_math, vector_lib};
use crate::water;
use bevy::prelude::Vec3;

/// Group of Killers one launcher owns (`KillerLauncher::init` 0x147e5c: `LiveActorGroup` of 4).
const GROUP_SIZE: usize = 4;
/// The Killer's start in the launcher's frame (`exeStandBy` 0x148460, the plain Killer's offset).
const BARREL_OFFSET: Vec3 = Vec3::new(0., 75., 55.);
/// Steps of the launcher's stand-by before the launch (`exeStandBy` 0x1484dc: 0x3c).
const STAND_BY_STEPS: u32 = 60;
/// The Killer's flight constants, plain Killer: the front acceleration, the speed damping, the
/// turn per step in degrees and the distance under which the turn weakens
/// (0x1886940, 0x1886948, 0x1886950 and 0x1886958, first entries).
const ACCELERATION: f32 = 0.63;
const DAMPING: f32 = 0.95;
const TURN_DEGREES: f32 = 2.4;
const TURN_DISTANCE: f32 = 1100.;
/// `Killer::exeLaunch` hands over to `exeFly` once the nerve's step reaches this: 25 for the plain
/// Killer (0x146860), 40 for the Magnum one (0x146864), selected at 0x146868 by the kind byte at
/// Killer +0x136. Only the plain kind (0) is ported, as with the flight constants below.
const LAUNCH_STEPS: u32 = 25;
/// The hit sensor a look-at candidate is (`Killer.szs` `InitSensor.byml` "Body": EnemyBody).
const BODY_RADIUS: f32 = 80.;

/// One `KillerLauncher` placement.
#[derive(Clone, Debug)]
pub struct LauncherSource {
    /// The placement's pose (`al::tryGetMatrixTR`), row-major 3x4, absolute stage cm.
    pub matrix: [f32; 12],
    /// `WaitFrame`, `AliveFrame` and `DelayFrame` (0x118, 0x11c and 0x120).
    pub wait_frame: i32,
    pub alive_frame: i32,
    pub delay_frame: i32,
    /// The stage switch the `SwitchStart` link names.
    pub switch: String,
}

/// One `SwitchKeepOnArea` placement and the stage switch its `SwitchAreaOn` link names.
#[derive(Clone, Debug)]
pub struct SwitchAreaSource {
    pub area: water::Area,
    pub switch: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum KillerNerve {
    /// `Appear` and `StandBy`: the Killer sits in front of the barrel.
    Waiting,
    Launch,
    Fly,
}

/// A Killer's position as a hit sensor of the head's look-at.
#[derive(Clone, Copy, Debug)]
pub struct KillerSensor {
    pub position: Vec3,
    pub alive: bool,
}

pub const SENSOR_RADIUS: f32 = BODY_RADIUS;

#[derive(Clone, Debug)]
struct Killer {
    dead: bool,
    nerve: KillerNerve,
    pending: Option<KillerNerve>,
    step: u32,
    position: Vec3,
    front: Vec3,
    velocity: Vec3,
    /// The alive count (`+0x11c`), in frames.
    count: f32,
}

impl Killer {
    fn new() -> Self {
        Self {
            dead: true,
            nerve: KillerNerve::Waiting,
            pending: None,
            step: 0,
            position: Vec3::ZERO,
            front: Vec3::Z,
            velocity: Vec3::ZERO,
            count: 0.,
        }
    }

    /// `Killer::standByAppear` (0x146624): alive in front of the barrel, facing along it.
    fn stand_by(&mut self, position: Vec3, front: Vec3) {
        self.dead = false;
        self.position = position;
        self.front = front;
        self.pending = Some(KillerNerve::Waiting);
    }

    /// `Killer::launch` (0x1466dc).
    fn launch(&mut self, alive_frame: i32) {
        self.count = alive_frame as f32;
        self.pending = Some(KillerNerve::Launch);
    }

    /// The front the pose quaternion gives (`al::calcFrontDir`) after the front was turned and
    /// put back in it (`al::makeQuatUpFront`).
    fn front_of_pose(front: Vec3) -> Vec3 {
        pose_math::quat_to_axes(pose_math::quat_up_front(Vec3::Y, front))[2]
    }

    /// `al::addVelocityToFront` then `al::scaleVelocity`.
    fn accelerate(&mut self) {
        let front = self.front;
        self.velocity = Vec3::new(
            front.x * ACCELERATION + self.velocity.x,
            front.y * ACCELERATION + self.velocity.y,
            front.z * ACCELERATION + self.velocity.z,
        ) * DAMPING;
    }

    /// `Killer::exeFly`'s turn: `al::turnToTarget` toward the nearest player, more slowly when
    /// that one is near.
    fn turn_to(&mut self, target: Vec3) {
        let to = target - self.position;
        let distance = to.length();
        let mut degrees = TURN_DEGREES;
        if distance < TURN_DISTANCE {
            degrees *= distance / TURN_DISTANCE;
        }
        let (direction, found) = pose_math::try_normalize_or_zero(to);
        if !found {
            return;
        }
        let gravity = Vec3::new(0., -1., 0.);
        if let Some((turned, _)) = vector_lib::turn_vec_to_vec_cos_on_plane(
            self.front,
            direction,
            gravity,
            vector_lib::cos_of_degrees(degrees),
        ) {
            self.front = Self::front_of_pose(turned);
        }
    }

    /// `LiveActor::movement` of the Killer: the nerve, `control` (the alive count) and the move.
    /// `target` is the player's position, `None` in a 2D area (`rs::isPlayer2D`) or without one.
    fn update(&mut self, target: Option<Vec3>) {
        if self.dead {
            return;
        }
        if let Some(nerve) = self.pending.take() {
            self.nerve = nerve;
            self.step = 0;
        }
        match self.nerve {
            KillerNerve::Waiting => {}
            KillerNerve::Launch => {
                self.accelerate();
                if self.step >= LAUNCH_STEPS {
                    self.pending = Some(KillerNerve::Fly);
                }
            }
            KillerNerve::Fly => {
                if let Some(target) = target {
                    self.turn_to(target);
                }
                self.accelerate();
            }
        }
        if self.nerve != KillerNerve::Waiting {
            self.count -= 1.;
            if self.count <= 0. {
                self.dead = true;
                return;
            }
        }
        // `LiveActor::updateCollider` without a hit.
        self.position += self.velocity;
        self.step += 1;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum LauncherNerve {
    Wait,
    Delay,
    Stop,
    StandBy,
    Launch,
}

#[derive(Clone, Debug)]
struct Launcher {
    source: LauncherSource,
    nerve: LauncherNerve,
    pending: Option<LauncherNerve>,
    step: u32,
    /// `+0x124`, the frames left of the wait.
    counter: i32,
    /// `+0x128`, the index of the last Killer picked.
    killer_index: i32,
    /// `+0x12c`: the switch went off during the stand-by, stop after the launch.
    stop_after_launch: bool,
    /// `+0x110`: the Killer standing by.
    stand_by: Option<usize>,
    /// The switch value this launcher's `StageSwitchWatcher` sampled at its last delivery
    /// (`StageSwitchWatcher::update` 0xa27ec8 calls the functor only on a difference).
    watched_on: bool,
    killers: [Killer; GROUP_SIZE],
}

impl Launcher {
    fn new(source: LauncherSource) -> Self {
        Self {
            source,
            // `KillerLauncher::init` sets Delay for a `DelayFrame` above zero and Stop right after,
            // the launcher having a `SwitchStart` link.
            nerve: LauncherNerve::Stop,
            pending: None,
            step: 0,
            counter: 0,
            killer_index: 0,
            stop_after_launch: false,
            stand_by: None,
            watched_on: false,
            killers: std::array::from_fn(|_| Killer::new()),
        }
    }

    fn set_nerve(&mut self, nerve: LauncherNerve) {
        self.pending = Some(nerve);
    }

    /// The nerve the actor is in for `al::isNerve`: a nerve chosen this frame counts at once.
    fn is_nerve(&self, nerve: LauncherNerve) -> bool {
        self.pending.unwrap_or(self.nerve) == nerve
    }

    /// `KillerLauncher::startLaunch` (0x148138).
    fn start_launch(&mut self) {
        self.stop_after_launch = false;
        if self.is_nerve(LauncherNerve::Stop) {
            self.set_nerve(if self.source.delay_frame < 0 {
                LauncherNerve::Wait
            } else {
                LauncherNerve::Delay
            });
        }
    }

    /// `KillerLauncher::endLaunch` (0x14819c).
    fn end_launch(&mut self) {
        if self.is_nerve(LauncherNerve::StandBy) {
            self.stop_after_launch = true;
        } else {
            self.set_nerve(LauncherNerve::Stop);
        }
    }

    /// The point in front of the barrel, in the launcher's frame (`al::calcTransLocalOffset`).
    fn barrel(&self) -> Vec3 {
        let m = &self.source.matrix;
        let o = BARREL_OFFSET;
        Vec3::new(
            m[3] + ((m[0] * o.x + m[1] * o.y) + m[2] * o.z),
            m[7] + ((m[4] * o.x + m[5] * o.y) + m[6] * o.z),
            m[11] + ((m[8] * o.x + m[9] * o.y) + m[10] * o.z),
        )
    }

    fn front(&self) -> Vec3 {
        let m = &self.source.matrix;
        Vec3::new(m[2], m[6], m[10])
    }

    /// `KillerLauncher::exeStandBy`'s first step: the last dead Killer the scan finds, starting
    /// after the last one picked (the loop keeps going after a find and re-reads the index).
    fn pick_killer(&mut self) {
        self.stand_by = None;
        let size = GROUP_SIZE as i32;
        for offset in 0..size {
            let index = (self.killer_index + offset + 1).rem_euclid(size);
            if self.killers[index as usize].dead {
                self.stand_by = Some(index as usize);
                self.killer_index = index;
            }
        }
    }

    /// `LiveActor::movement` of the launcher: its nerve, then `control` counts the wait down.
    fn update(&mut self) {
        if let Some(nerve) = self.pending.take() {
            self.nerve = nerve;
            self.step = 0;
        }
        match self.nerve {
            LauncherNerve::Stop => {}
            LauncherNerve::Delay => {
                if self.step >= self.source.delay_frame.max(0) as u32 && self.counter <= 0 {
                    self.set_nerve(LauncherNerve::StandBy);
                }
            }
            LauncherNerve::Wait => {
                if self.counter <= 0 {
                    self.set_nerve(LauncherNerve::StandBy);
                }
            }
            LauncherNerve::StandBy => {
                if self.step == 0 {
                    self.counter = self.source.wait_frame;
                    self.pick_killer();
                    let Some(index) = self.stand_by else {
                        self.set_nerve(LauncherNerve::Wait);
                        self.counter -= 1;
                        self.step += 1;
                        return;
                    };
                    let (position, front) = (self.barrel(), self.front());
                    self.killers[index].stand_by(position, front);
                }
                if self.step >= STAND_BY_STEPS {
                    self.set_nerve(LauncherNerve::Launch);
                }
            }
            LauncherNerve::Launch => {
                if let Some(index) = self.stand_by.take() {
                    self.killers[index].launch(self.source.alive_frame);
                }
                if self.stop_after_launch {
                    self.stop_after_launch = false;
                    self.set_nerve(LauncherNerve::Stop);
                } else {
                    self.set_nerve(LauncherNerve::Wait);
                }
            }
        }
        self.counter -= 1;
        self.step += 1;
    }
}

/// Every launcher of the stage, the switches they listen to and the areas that drive them.
#[derive(Clone, Debug, Default)]
pub struct Killers {
    launchers: Vec<Launcher>,
    areas: Vec<SwitchAreaSource>,
    /// The stage switches that are on.
    switches_on: Vec<String>,
    /// The Killers' hit sensors as `al::HitSensorDirector::update` left them at the start of the
    /// frame (Update entry 6). A sensor holds where its actor stood then, so what runs before the
    /// next update (the head's look-at, the probe's sample) sees the Killers where the previous
    /// frame's enemy update left them.
    sensors: Vec<KillerSensor>,
}

impl Killers {
    pub fn new(launchers: &[LauncherSource], areas: &[SwitchAreaSource]) -> Self {
        Self {
            launchers: launchers.iter().cloned().map(Launcher::new).collect(),
            areas: areas.to_vec(),
            switches_on: Vec::new(),
            sensors: vec![
                KillerSensor {
                    position: Vec3::ZERO,
                    alive: false,
                };
                launchers.len() * GROUP_SIZE
            ],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.launchers.is_empty()
    }

    /// Everything of a frame that runs after the player's own movement (Update entry 30), in the
    /// executable's order: the enemies (entry 37), the stage-switch watchers (entry 65), then
    /// the switch areas' sampling. `feet` is the player's position after its movement, which the
    /// Killers chase and the areas test; `chase` is false in a 2D area.
    pub fn update_after_player(&mut self, feet: Vec3, chase: bool) {
        self.update_enemies(chase.then_some(feet));
        self.deliver_switch_edges();
        self.sample_switch_areas(feet);
    }

    /// The enemies' part of a frame (`al::ExecuteDirector` Update entry 37, `敵`): the launchers'
    /// movement, then the Killers' [I: the launcher registers its actor group first, 0x147e84
    /// against the Killers' 0x147f78, and `ExecutorListActorExecuteBase` runs the groups in
    /// registration order, 0x897330/0x897458; the Sand lift recordings agree: the Killers
    /// first fails all three]. A Killer launched by its launcher flies in the same frame.
    /// `player` is where the player stands after its own movement (entry 30 runs
    /// first), `None` in a 2D area or without a player.
    fn update_enemies(&mut self, player: Option<Vec3>) {
        for launcher in &mut self.launchers {
            launcher.update();
        }
        for launcher in &mut self.launchers {
            for killer in &mut launcher.killers {
                killer.update(player);
            }
        }
    }

    /// `al::StageSwitchDirector`'s watchers (Update entry 65, after the enemies): each launcher
    /// is told its switch turned on or off when the value differs from the one the watcher
    /// sampled last (`StageSwitchWatcher::update` 0xa27ec8).
    fn deliver_switch_edges(&mut self) {
        for launcher in &mut self.launchers {
            let on = self.switches_on.contains(&launcher.source.switch);
            if on == launcher.watched_on {
                continue;
            }
            launcher.watched_on = on;
            if on {
                launcher.start_launch();
            } else {
                launcher.end_launch();
            }
        }
    }

    /// `al::SwitchAreaDirector::update` (0x4d1854), after the whole execute table:
    /// `SwitchKeepOnAreaGroup::update` (0x807b04) samples where the player stands now and writes
    /// each switch its final value, on while any of its areas holds the player. The switch is a
    /// plain stage-switch bit (0xa27114/0xa2713c); the watchers only see it at their next
    /// delivery, and only as the difference to their last sample.
    fn sample_switch_areas(&mut self, player: Vec3) {
        let mut switches: Vec<String> = Vec::new();
        for area in &self.areas {
            if !switches.contains(&area.switch) {
                switches.push(area.switch.clone());
            }
        }
        for switch in switches {
            let on = self
                .areas
                .iter()
                .any(|area| area.switch == switch && area.area.contains(player));
            self.write_switch(&switch, on);
        }
    }

    /// `StageSwitchDirector::onSwitch`/`offSwitch`: the switch's bit, nothing else.
    fn write_switch(&mut self, switch: &str, on: bool) {
        self.switches_on.retain(|name| name != switch);
        if on {
            self.switches_on.push(switch.to_owned());
        }
    }

    /// `al::HitSensorDirector::update` (Update entry 6): every sensor takes its actor's position.
    pub fn update_sensors(&mut self) {
        self.sensors = self
            .launchers
            .iter()
            .flat_map(|launcher| &launcher.killers)
            .map(|killer| KillerSensor {
                position: killer.position,
                alive: !killer.dead,
            })
            .collect();
    }

    /// The hit sensors of the Killers as the last update left them, in a fixed order (a dead
    /// Killer's has `alive` false).
    pub fn sensors(&self) -> &[KillerSensor] {
        &self.sensors
    }

    /// The number of sensors `sensors` returns.
    pub fn sensor_count(&self) -> usize {
        self.launchers.len() * GROUP_SIZE
    }
}

pub fn self_check() {
    launcher_timeline();
    shared_switch_keeps_the_launcher_running();
    killer_flight_settles_to_the_damped_speed();
}

fn test_launcher(switch: &str, delay_frame: i32) -> LauncherSource {
    LauncherSource {
        // Rotated 90 degrees about Y: the barrel faces +X.
        matrix: [0., 0., 1., 0., 0., 1., 0., 0., -1., 0., 0., 0.],
        wait_frame: 300,
        alive_frame: 600,
        delay_frame,
        switch: switch.into(),
    }
}

/// A `SwitchKeepOnArea` cube whose floor centre is at `(x, 0, 0)`.
fn test_area(x: f32, switch: &str) -> SwitchAreaSource {
    SwitchAreaSource {
        area: water::Area {
            shape: water::Shape::CubeBase,
            matrix: [1., 0., 0., x, 0., 1., 0., 0., 0., 0., 1., 0.],
            scale: Vec3::ONE,
            priority: -1,
            ignore: false,
        },
        switch: switch.into(),
    }
}

/// A launcher whose area holds the player from the frame he is placed in launches its Killer in
/// frame 64 after the placement and the Killer flies from frame 64 too: the area samples at the
/// end of the placement frame (frame -1, of which only the tail runs), the watcher delivers the
/// edge in frame 0 (after the launcher's movement), the launcher's `Delay` of one frame ends in
/// frame 2, its stand-by lasts 60 steps from frame 3 and its `Launch` nerve runs in frame 64,
/// where the Killer, moving after it, starts flying.
#[cfg_attr(test, test)]
fn launcher_timeline() {
    let area = test_area(0., "obj2");
    let mut killers = Killers::new(&[test_launcher("obj2", 1)], &[area]);
    let player = Vec3::new(0., 10., 0.);
    killers.update_after_player(player, true);
    let mut flying_from = None;
    for frame in 0..80 {
        killers.update_sensors();
        killers.update_after_player(player, true);
        if flying_from.is_none()
            && killers.launchers[0]
                .killers
                .iter()
                .any(|killer| killer.velocity.length() > 0.)
        {
            flying_from = Some(frame);
        }
    }
    assert_eq!(flying_from, Some(64));
    // It started in front of the barrel (rotated 90 degrees about Y: along +X).
    killers.update_sensors();
    let launched = killers
        .sensors()
        .iter()
        .find(|sensor| sensor.alive)
        .unwrap();
    assert!(launched.position.x > 55.);
}

/// Two areas drive one switch (the Sand stage's `obj168`: `obj891` and `obj932`) and the player
/// stays only in the second. `SwitchKeepOnAreaGroup::update` writes the switch its final value,
/// so the first area's "off" never reaches the watcher: the launcher stands by and launches
/// rather than restarting its delay every frame.
#[cfg_attr(test, test)]
fn shared_switch_keeps_the_launcher_running() {
    let areas = [test_area(0., "obj890"), test_area(5000., "obj890")];
    let mut killers = Killers::new(&[test_launcher("obj890", 1)], &areas);
    let player = Vec3::new(5000., 10., 0.);
    let mut reached_stand_by = false;
    let mut launched = false;
    for _ in 0..80 {
        killers.update_after_player(player, true);
        let launcher = &killers.launchers[0];
        reached_stand_by |= launcher.nerve == LauncherNerve::StandBy;
        launched |= launcher
            .killers
            .iter()
            .any(|killer| killer.velocity.length() > 0.);
    }
    assert!(reached_stand_by);
    assert!(launched);
}

#[cfg_attr(test, test)]
fn killer_flight_settles_to_the_damped_speed() {
    let mut killer = Killer::new();
    killer.stand_by(Vec3::ZERO, Vec3::X);
    killer.launch(600);
    for _ in 0..200 {
        killer.update(None);
    }
    let settled = ACCELERATION * DAMPING / (1. - DAMPING);
    assert!((killer.velocity.length() - settled).abs() < 1e-3);
    assert!(killer.velocity.x > 0. && killer.velocity.z.abs() < 1e-6);
}

impl super::Player {
    /// The start of a frame, for the Killers: builds the stage's launchers on the first one, then
    /// the hit sensor director's update (Update entry 6), which every later part of the frame
    /// reads, the head's look-at included.
    pub(super) fn init_killers(&mut self, scene: &super::Course) {
        let Some(mesh) = scene.mesh.as_deref() else {
            return;
        };
        if self.killers.is_empty() && !mesh.killer_launchers.is_empty() {
            self.killers = Killers::new(&mesh.killer_launchers, &mesh.switch_areas);
        }
        self.killers.update_sensors();
    }

    /// The rest of the frame a recording's probe placed the player in. The probe places him after
    /// his movement (`hook_movement`, `afterMovement`), so the enemies, the watchers and the
    /// switch areas of that frame already see him at the placement; the first recorded frame is
    /// the next one. A launcher whose area holds the placement therefore starts one frame
    /// sooner than if the area first saw him in the first recorded frame.
    pub fn run_placement_frame_tail(&mut self, scene: &super::Course) {
        self.init_killers(scene);
        self.tick_killers(scene);
    }

    /// What runs after the player's movement in a frame (see [`Killers::update_after_player`]).
    /// The head's look-at ran inside that movement, so it saw the Killers where the previous
    /// frame left them, as the executable's sensors are.
    pub(super) fn tick_killers(&mut self, scene: &super::Course) {
        let Some(mesh) = scene.mesh.as_deref() else {
            return;
        };
        if self.killers.is_empty() {
            return;
        }
        let feet = mesh.to_game_vec(self.position_cm());
        self.killers.update_after_player(feet, !self.model_2d());
    }

    /// The Killers' body sensors, one per Killer of every launcher in a fixed order (the last
    /// sensors of the list).
    pub(super) fn killer_look_sensors(
        &self,
        sensors: &mut Vec<super::look_at::LookSensor>,
        mesh: Option<&crate::geometry::TriangleMesh>,
    ) {
        let Some(mesh) = mesh else {
            return;
        };
        for sensor in self.killers.sensors() {
            sensors.push(super::look_at::LookSensor {
                host: "キラー".into(),
                name: "Body".into(),
                kind: 9,
                radius: SENSOR_RADIUS,
                position: mesh.relative_vec(sensor.position),
                stage_position: sensor.position,
            });
        }
    }

    /// How many of the list's last sensors are Killers' (see [`Self::killer_look_sensors`]).
    pub(super) fn killer_sensor_count(
        &self,
        mesh: Option<&crate::geometry::TriangleMesh>,
    ) -> usize {
        if mesh.is_some() {
            self.killers.sensor_count()
        } else {
            0
        }
    }
}
