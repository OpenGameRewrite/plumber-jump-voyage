use super::*;

pub(super) fn run() {
    pause_and_click_routing();
    menu_definition();
}

#[cfg_attr(test, test)]
fn pause_and_click_routing() {
    let mut world = World::new();
    world.insert_resource(Time::<Real>::default());
    world.insert_resource(ButtonInput::<KeyCode>::default());
    world.insert_resource(FrameInput {
        back: true,
        ..default()
    });
    let menu = Menu {
        open: false,
        begun: true,
        page: Page::Main,
        ..default()
    };
    world.insert_resource(menu);
    world.insert_resource(Settings::default());
    world.insert_resource(Progress::default());
    world.insert_resource(crate::local_assets::Wardrobe {
        outfits: vec![],
        status: String::new(),
    });
    world.insert_resource(Orders {
        down_throw: true,
        ..default()
    });
    world.insert_resource(bevy::ecs::message::Messages::<Click>::default());
    world.insert_resource(bevy::ecs::message::Messages::<AppExit>::default());
    world.spawn(Window {
        focused: true,
        ..default()
    });
    world.spawn(Player::default());
    let system = world.register_system(handle);
    world.run_system(system).unwrap();
    assert!(
        !world.resource::<Menu>().open,
        "gamepad B throws downward during play"
    );
    assert!(world.resource::<Orders>().down_throw);
    world.resource_mut::<FrameInput>().edge[act::PAUSE] = true;
    world.run_system(system).unwrap();
    assert!(world.resource::<Menu>().open);
    assert!(!world.resource::<Orders>().down_throw);
    world
        .resource_mut::<Time<Real>>()
        .advance_by(std::time::Duration::from_millis(300));
    world.resource_mut::<FrameInput>().edge[act::PAUSE] = false;
    world.resource_mut::<FrameInput>().back = false;
    world
        .resource_mut::<bevy::ecs::message::Messages<Click>>()
        .write(Click(0));
    world.run_system(system).unwrap();
    assert!(
        !world.resource::<Menu>().open,
        "mouse activation resumes play"
    );
}

#[cfg_attr(test, test)]
fn menu_definition() {
    let m = Menu::default();
    let s = Settings::default();
    assert_eq!(count(Page::Main, 0), MAIN_ITEMS.len());
    for (i, item) in MAIN_ITEMS.iter().enumerate() {
        assert_eq!(item.index(), i);
        assert!(!item.label(&m, &s, "Explorer").is_empty());
    }
    assert_eq!(MainItem::Sensitivity.index(), 4);
    assert_eq!(MainItem::Quit.index(), 9);
}
