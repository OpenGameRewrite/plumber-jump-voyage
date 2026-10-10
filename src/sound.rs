//! Local effects are loaded only by the optional asset build.
use crate::{
    input::Settings,
    movement::{Move, Player},
    ui::Menu,
};
use bevy::audio::Volume;
use bevy::prelude::*;
#[derive(Resource, Default)]
struct Sounds(std::collections::BTreeMap<String, Vec<Handle<AudioSource>>>);
pub fn plugin(app: &mut App) {
    app.init_resource::<Sounds>()
        .add_systems(Startup, load)
        .add_systems(Update, play);
}
fn load(mut sounds: ResMut<Sounds>, server: Res<AssetServer>) {
    if let Some(files) =
        std::fs::read(crate::local_assets::asset_dir().join("local/sounds/manifest.json"))
            .ok()
            .and_then(|b| {
                serde_json::from_slice::<std::collections::BTreeMap<String, Vec<String>>>(&b).ok()
            })
    {
        for (key, paths) in files {
            let paths = paths
                .into_iter()
                .filter(|p| {
                    p.starts_with("local/sounds/")
                        && p.ends_with(".wav")
                        && !p.contains("..")
                        && !p.contains('\\')
                        && !p.contains(':')
                })
                .map(|p| server.load(p))
                .collect();
            sounds.0.insert(key, paths);
        }
    }
}
#[derive(Default)]
struct Last {
    mode: Move,
    grounded: bool,
    cap: bool,
    distance: f32,
    serial: usize,
}
fn play(
    mut commands: Commands,
    p: Single<&Player>,
    sounds: Res<Sounds>,
    settings: Res<Settings>,
    menu: Res<Menu>,
    mut last: Local<Last>,
) {
    let mut events = Vec::new();
    if !menu.open && settings.outfit >= 3 {
        if last.mode != p.mode
            && let Some(key) = match p.mode {
                Move::Jump => Some("jump"),
                Move::DoubleJump => Some("double_jump"),
                Move::TripleJump => Some("triple_jump"),
                Move::LongJump => Some("long_jump"),
                Move::Backflip => Some("backflip"),
                Move::Sideflip => Some("sideflip"),
                Move::SpinJump => Some("spin_jump"),
                Move::Roll => Some("roll"),
                Move::RollJump => Some("roll_jump"),
                Move::Dive => Some("dive"),
                Move::GroundPound => Some("pound"),
                Move::PoundJump => Some("pound_jump"),
                Move::WallJump => Some("wall_jump"),
                Move::Ledge => Some("ledge"),
                Move::Pole => Some("pole"),
                Move::Swim => Some("swim"),
                Move::SwimDash => Some("swim_dash"),
                Move::CapBounce | Move::CapVault | Move::CatchJump => Some("cap_bounce"),
                _ => None,
            }
        {
            events.push(key);
        }
        if p.grounded && !last.grounded {
            events.push("land");
        }
        if p.cap.is_some() && !last.cap {
            events.push("throw");
        }
        if p.cap.is_none() && last.cap && !matches!(p.mode, Move::CapBounce | Move::CapVault) {
            events.push("catch");
        }
        if matches!(p.mode, Move::Run | Move::Walk | Move::Crawl)
            && p.grounded
            && p.distance - last.distance > 0.9
        {
            events.push("step");
            last.distance = p.distance;
        }
        for key in events {
            if let Some(choices) = sounds.0.get(key).filter(|v| !v.is_empty()) {
                commands.spawn((
                    AudioPlayer::new(choices[last.serial % choices.len()].clone()),
                    PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.65 * settings.volume)),
                ));
                last.serial = last.serial.wrapping_add(1);
            }
        }
    }
    if p.distance < last.distance {
        last.distance = p.distance;
    }
    last.mode = p.mode;
    last.grounded = p.grounded;
    last.cap = p.cap.is_some();
}
pub fn self_check() {
    let mut world = World::new();
    world.insert_resource(Settings {
        outfit: 3,
        ..default()
    });
    let mut menu = Menu::default();
    menu.open = false;
    world.insert_resource(menu);
    world.insert_resource(Sounds([("jump".into(), vec![Handle::default()])].into()));
    let player = world
        .spawn(Player {
            mode: Move::Jump,
            grounded: false,
            ..default()
        })
        .id();
    let system = world.register_system(play);
    world.run_system(system).unwrap();
    assert_eq!(world.query::<&AudioPlayer>().iter(&world).count(), 1);
    world.run_system(system).unwrap();
    assert_eq!(
        world.query::<&AudioPlayer>().iter(&world).count(),
        1,
        "held states must not replay every frame"
    );
    world.resource_mut::<Menu>().open = true;
    world.get_mut::<Player>(player).unwrap().mode = Move::Idle;
    world.run_system(system).unwrap();
    world.get_mut::<Player>(player).unwrap().mode = Move::Jump;
    world.run_system(system).unwrap();
    assert_eq!(
        world.query::<&AudioPlayer>().iter(&world).count(),
        1,
        "menus suppress movement audio"
    );
}
