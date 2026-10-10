//! Local effects are loaded only by the optional asset build.
use crate::{
    input::Settings,
    movement::{Fx, Move},
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
fn key(fx: Fx) -> Option<&'static str> {
    Some(match fx {
        Fx::Jump(kind) => match kind {
            Move::DoubleJump => "double_jump",
            Move::TripleJump => "triple_jump",
            Move::LongJump => "long_jump",
            Move::Backflip => "backflip",
            Move::Sideflip => "sideflip",
            Move::SpinJump => "spin_jump",
            Move::RollJump => "roll_jump",
            Move::PoundJump => "pound_jump",
            Move::WallJump => "wall_jump",
            Move::CapBounce | Move::CapVault => return None,
            _ => "jump",
        },
        Fx::Land { .. } | Fx::PoundLand => "land",
        Fx::Step => "step",
        Fx::PoundStart => "pound",
        Fx::RollStart | Fx::RollBoost { .. } => "roll",
        Fx::Dive => "dive",
        Fx::Throw(_) => "throw",
        Fx::Catch => "catch",
        Fx::CapBounce => "cap_bounce",
        Fx::Ledge => "ledge",
        Fx::Pole => "pole",
        Fx::Splash => "swim",
        Fx::SwimStroke => "swim_dash",
        Fx::Skid | Fx::WallSlide => return None,
    })
}
fn play(
    mut commands: Commands,
    mut events: MessageReader<Fx>,
    sounds: Res<Sounds>,
    settings: Res<Settings>,
    menu: Res<Menu>,
    mut serial: Local<usize>,
) {
    for fx in events.read() {
        if menu.open || settings.outfit < 3 {
            continue;
        }
        if let Some(choices) = key(*fx)
            .and_then(|k| sounds.0.get(k))
            .filter(|v| !v.is_empty())
        {
            commands.spawn((
                AudioPlayer::new(choices[*serial % choices.len()].clone()),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(0.65 * settings.volume)),
            ));
            *serial = serial.wrapping_add(1);
        }
    }
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
    world.insert_resource(bevy::ecs::message::Messages::<Fx>::default());
    let system = world.register_system(play);
    world.write_message(Fx::Jump(Move::Jump));
    world.run_system(system).unwrap();
    assert_eq!(world.query::<&AudioPlayer>().iter(&world).count(), 1);
    world.run_system(system).unwrap();
    assert_eq!(
        world.query::<&AudioPlayer>().iter(&world).count(),
        1,
        "each event plays once"
    );
    world.resource_mut::<Menu>().open = true;
    world.write_message(Fx::Jump(Move::Jump));
    world.run_system(system).unwrap();
    assert_eq!(
        world.query::<&AudioPlayer>().iter(&world).count(),
        1,
        "menus suppress movement audio"
    );
}
