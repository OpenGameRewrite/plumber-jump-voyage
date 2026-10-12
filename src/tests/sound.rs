use super::*;

pub(super) fn run() {
    animation_sound_triggers();
}

#[cfg_attr(test, test)]
fn animation_sound_triggers() {
    let mut world = World::new();
    world.insert_resource(Settings {
        outfit: 3,
        ..default()
    });
    let mut menu = Menu::default();
    menu.open = false;
    world.insert_resource(menu);
    world.insert_resource(Time::<()>::default());
    let layer = |r: &str| Layer {
        resource: r.into(),
        ..default()
    };
    let single = |r: &str| Play {
        layers: vec![layer(r)],
        ..default()
    };
    let mut table = Table::default();
    table.mario.actions.insert(
        "Jump".into(),
        vec![
            Trigger {
                play: "Jump".into(),
                frame: -1.,
                once: false,
            },
            Trigger {
                play: "vJump".into(),
                frame: 1.,
                once: false,
            },
        ],
    );
    table
        .mario
        .plays
        .insert("Jump".into(), single("SePfJumpSoil"));
    table
        .mario
        .plays
        .insert("vJump".into(), single("WsPvJump00"));
    for r in ["SePfJumpSoil", "WsPvJump00"] {
        table.resources.insert(
            r.into(),
            Resource {
                files: vec![format!("local/sounds/{r}.wav")],
                volume: 0.5,
                random_semitones: None,
            },
        );
    }
    let handles = table
        .resources
        .keys()
        .map(|k| (k.clone(), vec![Handle::default()]))
        .collect();
    world.insert_resource(Sounds { table, handles });
    world.insert_resource(bevy::ecs::message::Messages::<Fx>::default());
    let mut p = Player::default();
    p.anim.name = "Jump";
    p.anim.serial = 1;
    p.anim.frame = 0.;
    let e = world.spawn(p).id();
    let system = world.register_system(play);
    let count = |w: &mut World| w.query::<&AudioPlayer>().iter(w).count();
    world.run_system(system).unwrap();
    assert_eq!(count(&mut world), 1, "the foot sound starts with the clip");
    world.run_system(system).unwrap();
    assert_eq!(count(&mut world), 1, "each trigger plays once");
    world.get_mut::<Player>(e).unwrap().anim.frame = 2.;
    world.run_system(system).unwrap();
    assert_eq!(count(&mut world), 2, "the voice waits for its frame");
    world.resource_mut::<Menu>().open = true;
    world.get_mut::<Player>(e).unwrap().anim.serial = 2;
    world.run_system(system).unwrap();
    assert_eq!(count(&mut world), 2, "menus suppress movement audio");
    assert!((curve(Some([0.1, 6., 0.4, 1.]), 14.) - 1.).abs() < 1e-6);
    assert!((curve(Some([0.1, 6., 0.4, 1.]), 0.1) - 0.4).abs() < 1e-6);
}
