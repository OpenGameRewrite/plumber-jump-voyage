// Bevy system parameters declare ECS access directly; keep their query filters explicit.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod air_move;
mod asset_contract;
// The original's effects: their data and trigger rules also serve the headless trace.
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
mod effect_data;
#[cfg(feature = "local-assets")]
mod effect_render;
#[cfg(feature = "local-assets")]
mod effect_sim;
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
mod effect_triggers;
#[cfg(feature = "local-assets")]
mod effects;
mod fx;
mod game_columns;
mod game_names;
mod input;
#[cfg(feature = "local-assets")]
mod legacy_assets;
mod local_assets;
mod movement;
mod nerve;
mod replay;
mod scripted_input;
mod shots;
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
    // A headless replay (--trace) never opens a window; a rendered one
    // (--shots) plays the script in the game and saves pictures.
    let mut replay_shots = None;
    if args.iter().any(|a| a == "--replay") {
        match replay::run_cli(&args) {
            Ok(None) => return,
            Ok(driver) => replay_shots = driver,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        }
    }
    if args.iter().any(|a| a == "--self-check") {
        movement::self_check();
        input::self_check();
        ui::self_check();
        local_assets::self_check();
        asset_contract::self_check();
        fx::self_check();
        effect_data::self_check();
        effect_triggers::self_check();
        sound_table::self_check();
        game_names::self_check();
        game_columns::self_check();
        trace::self_check();
        replay::self_check();
        #[cfg(feature = "local-assets")]
        {
            sound::self_check();
            effect_sim::self_check();
            effects::self_check();
        }
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
                    focused: std::env::var_os("PJV_SHOT").is_none() && replay_shots.is_none(),
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
    app.add_plugins((sound::plugin, effects::plugin));
    if let Some(driver) = replay_shots {
        shots::install(&mut app, driver);
    }
    app.run();
}
