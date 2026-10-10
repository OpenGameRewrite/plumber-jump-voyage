// Bevy system parameters declare ECS access directly; keep their query filters explicit.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod input;
mod local_assets;
mod movement;
#[cfg(feature = "local-assets")]
mod sound;
mod ui;
mod world;

use bevy::prelude::*;

fn main() {
    if std::env::args().any(|a| a == "--self-check") {
        movement::self_check();
        input::self_check();
        ui::self_check();
        local_assets::self_check();
        #[cfg(feature = "local-assets")]
        sound::self_check();
        println!("Movement, collision, input and local asset checks passed.");
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
                    focused: false,
                    ..default()
                }),
                ..default()
            }),
    )
    .insert_resource(Time::<Fixed>::from_hz(120.0))
    .insert_resource(ClearColor(Color::srgb(0.47, 0.76, 0.88)))
    .add_plugins((
        input::plugin,
        movement::plugin,
        world::plugin,
        ui::plugin,
        local_assets::plugin,
    ));
    #[cfg(feature = "local-assets")]
    app.add_plugins(sound::plugin);
    app.run();
}
