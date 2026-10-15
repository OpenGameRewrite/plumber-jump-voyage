// Bevy system parameters declare ECS access directly; keep their query filters explicit.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod asset_contract;
mod fx;
mod input;
#[cfg(feature = "local-assets")]
mod legacy_assets;
mod local_assets;
mod movement;
mod replay;
mod scripted_input;
#[cfg(feature = "local-assets")]
mod sound;
// Pure sound rules, shared by the audio and the headless trace.
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
mod sound_table;
mod trace;
mod ui;
mod world;

use bevy::prelude::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--replay") {
        // Headless fidelity replay: no window, no rendering.
        if let Err(error) = replay::run_cli(&args) {
            eprintln!("{error}");
            std::process::exit(2);
        }
        return;
    }
    if args.iter().any(|a| a == "--self-check") {
        movement::self_check();
        input::self_check();
        ui::self_check();
        local_assets::self_check();
        asset_contract::self_check();
        fx::self_check();
        sound_table::self_check();
        trace::self_check();
        replay::self_check();
        #[cfg(feature = "local-assets")]
        sound::self_check();
        println!("Movement, collision, input, effects and local asset checks passed.");
        return;
    }
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: local_assets::asset_dir().to_string_lossy().into_owned(),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Plumber Jump Voyage".into(),
                    resolution: (1440, 900).into(),
                    // Take focus so input works at once (scripted captures stay in the background).
                    focused: std::env::var_os("PJV_SHOT").is_none(),
                    ..default()
                }),
                ..default()
            }),
    )
    .insert_resource(ClearColor(Color::srgb(0.47, 0.76, 0.88)))
    .add_plugins((
        input::plugin,
        movement::plugin,
        world::plugin,
        ui::plugin,
        local_assets::plugin,
        fx::plugin,
    ));
    #[cfg(feature = "local-assets")]
    app.add_plugins(sound::plugin);
    app.run();
}
