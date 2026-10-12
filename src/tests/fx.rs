use super::*;

pub(super) fn run() {
    pound_effects();
}

#[cfg_attr(test, test)]
fn pound_effects() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<crate::world::Shake>()
        .add_message::<Fx>()
        .add_message::<Burst>();
    let mut menu = crate::ui::Menu::default();
    menu.open = false;
    app.insert_resource(menu);
    app.add_systems(Startup, setup)
        .add_systems(Update, (react, simulate).chain());
    app.world_mut().spawn(Player::default());
    app.update();
    app.world_mut().write_message(Fx::PoundLand);
    app.update();
    let n = app
        .world_mut()
        .query::<&Particle>()
        .iter(app.world())
        .count();
    assert!(n > 10, "a pound kicks up dust and sparks");
    assert!(app.world().resource::<crate::world::Shake>().0 > 0.5);
}
