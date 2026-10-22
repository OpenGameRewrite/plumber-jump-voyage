//! Rendered replay: the windowed game plays a replay script on the flat floor
//! and saves a screenshot of every frame the script lists with `shot`.
//!
//! ```text
//! plumber-jump-voyage --replay run.input --shots runs/name [--camera game.csv]
//! ```
//!
//! Every update advances exactly one 60 Hz simulation step (time is stepped
//! by hand), the orders come from the script instead of a device, the camera
//! looks along the scenario's yaw, and time stops while a capture is taken so
//! the picture is the state after the listed frame. Pictures are written as
//! `rewrite-<frame>.png`. The camera's smoothing is real-time, so it settles
//! during the pause; its pitch and distance are the game's defaults.
use crate::{
    input::{Orders, Settings},
    movement::{Course, ExactFrames, Player, UNIT, look_at::LookSensor},
    replay::{CameraTrack, SETTLE_FRAMES, Script, Span, orders_for},
    ui::Menu,
    world::{MainCamera, Orbit},
};
use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    time::TimeUpdateStrategy,
};
use std::{path::PathBuf, time::Duration};

/// Updates a paused capture waits for the camera to settle (a second and a half).
const SETTLE_UPDATES: u32 = 90;

#[derive(Resource)]
pub struct Driver {
    frames: Vec<Span>,
    camera: CameraTrack,
    shots: Vec<u32>,
    dir: PathBuf,
    start_yaw_deg: f32,
    bindings: Vec<[String; 2]>,
    previous: Vec<&'static str>,
    /// Simulation steps started and finished.
    started: usize,
    finished: usize,
    next_shot: usize,
    phase: Phase,
    /// Where the recorded stage placed the sensors the head looks at (replay frame, cm).
    look_sensors: Vec<LookSensor>,
}
#[derive(PartialEq)]
enum Phase {
    /// Time stands still while the models and shaders load.
    Loading,
    Running,
    /// Time stands still at the frame while the camera's smoothing settles.
    Settling(u32),
    /// The capture was requested; waiting for the file to land.
    Saving(u32),
}
impl Driver {
    pub fn new(
        script: &Script,
        camera: CameraTrack,
        dir: PathBuf,
        look_sensors: Vec<LookSensor>,
    ) -> Result<Self, String> {
        if script.shots.is_empty() {
            return Err("the script has no 'shot <frame>...' line".into());
        }
        let mut shots = script.shots.clone();
        shots.sort_unstable();
        shots.dedup();
        Ok(Self {
            frames: script.frames().cloned().collect(),
            camera,
            shots,
            dir,
            start_yaw_deg: script.start_yaw_deg,
            bindings: Settings::default().bindings,
            previous: Vec::new(),
            started: 0,
            finished: 0,
            next_shot: 0,
            phase: Phase::Loading,
            look_sensors,
        })
    }
    /// Where the picture of the frame being captured goes.
    fn file(&self) -> PathBuf {
        self.dir
            .join(format!("rewrite-{}.png", self.shots[self.next_shot]))
    }
    /// Frame the latest finished step belongs to.
    fn frame(&self) -> usize {
        self.finished.saturating_sub(SETTLE_FRAMES + 1)
    }
}

pub fn install(app: &mut App, driver: Driver) {
    app.insert_resource(driver)
        .insert_resource(ExactFrames)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1. / crate::movement::FPS as f64,
        )))
        .add_systems(PostStartup, begin)
        .add_systems(PreUpdate, aim_camera)
        // After the orbit camera has placed itself and before the transform
        // propagation of this frame: the game's camera wins when it is known.
        .add_systems(
            PostUpdate,
            follow_game_camera.before(bevy::transform::TransformSystems::Propagate),
        )
        .add_systems(FixedPreUpdate, feed)
        .add_systems(FixedPostUpdate, |mut d: ResMut<Driver>| d.finished += 1)
        .add_systems(Update, (capture, mark_look_sensors));
}

/// Show where the head's look-at sensors are (the actors themselves are not drawn).
fn mark_look_sensors(driver: Res<Driver>, mut gizmos: Gizmos) {
    for sensor in &driver.look_sensors {
        gizmos.sphere(
            Isometry3d::from_translation(sensor.position * UNIT),
            sensor.radius * UNIT,
            Color::srgb(1., 0.8, 0.1),
        );
    }
}

/// Start playing at once, standing at the origin facing the start yaw, and
/// keep time still until the scene has loaded.
fn begin(
    mut driver: ResMut<Driver>,
    mut menu: ResMut<Menu>,
    mut orbit: ResMut<Orbit>,
    mut time: ResMut<Time<Virtual>>,
    mut player: Single<&mut Player>,
    mut course: ResMut<Course>,
) {
    course.look_sensors = driver.look_sensors.clone();
    menu.open = false;
    menu.begun = true;
    orbit.locked = true;
    **player = Player::spawn_facing(Vec3::ZERO, driver.start_yaw_deg);
    driver.phase = Phase::Loading;
    time.pause();
}
fn aim_camera(driver: Res<Driver>, mut orbit: ResMut<Orbit>) {
    // The game's camera looks along its yaw; the orbit sits behind that.
    orbit.yaw = driver.camera.yaw_deg(driver.frame()).to_radians() + std::f32::consts::PI;
}
/// Put the camera exactly where the real game's was in the frame being
/// shown: its position, view direction and field of view. Without a game
/// trace the orbit camera stays as it is.
fn follow_game_camera(
    driver: Res<Driver>,
    mut camera: Single<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    let Some(view) = driver.camera.view(driver.frame()) else {
        return;
    };
    camera.0.translation = view.position;
    camera.0.look_to(view.forward, Vec3::Y);
    if let Projection::Perspective(perspective) = camera.1.as_mut() {
        perspective.fov = view.fov_deg.to_radians();
    }
}
/// The orders of the step about to run.
fn feed(mut driver: ResMut<Driver>, mut orders: ResMut<Orders>) {
    let tick = driver.started;
    driver.started += 1;
    let Some(frame) = tick.checked_sub(SETTLE_FRAMES) else {
        *orders = Orders::default();
        return;
    };
    let Some(span) = driver.frames.get(frame).cloned() else {
        *orders = Orders::default();
        return;
    };
    let (next, _) = orders_for(
        &span,
        &driver.previous,
        driver.camera.yaw_deg(frame),
        driver.camera.aim_deg(frame),
        driver.camera.aim_gain(frame),
        &driver.bindings,
    );
    *orders = next;
    driver.previous = span.buttons;
}
fn capture(
    mut commands: Commands,
    mut driver: ResMut<Driver>,
    mut time: ResMut<Time<Virtual>>,
    real: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    // Seconds of real time to let the game's models and shaders load.
    let warmup: f32 = std::env::var("PJV_REPLAY_WARMUP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(6.);
    match driver.phase {
        Phase::Loading => {
            if real.elapsed_secs() >= warmup {
                time.unpause();
                driver.phase = Phase::Running;
            }
        }
        Phase::Running => {
            let Some(&frame) = driver.shots.get(driver.next_shot) else {
                exit.write(AppExit::Success);
                return;
            };
            if driver.finished > SETTLE_FRAMES + frame as usize {
                time.pause();
                driver.phase = Phase::Settling(0);
            }
        }
        Phase::Settling(waited) => {
            if waited < SETTLE_UPDATES {
                driver.phase = Phase::Settling(waited + 1);
                return;
            }
            if let Err(error) = std::fs::create_dir_all(&driver.dir) {
                error!("{}: {error}", driver.dir.display());
                exit.write(AppExit::error());
                return;
            }
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(driver.file()));
            driver.phase = Phase::Saving(0);
        }
        Phase::Saving(waited) => {
            let written = driver.file().is_file();
            if written || waited >= 600 {
                if !written {
                    error!("{} was not written", driver.file().display());
                }
                driver.next_shot += 1;
                time.unpause();
                driver.phase = Phase::Running;
            } else {
                driver.phase = Phase::Saving(waited + 1);
            }
        }
    }
}
