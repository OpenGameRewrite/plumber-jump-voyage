use super::*;

pub(super) fn run() {
    bindings_and_settings();
    device_routing();
}

#[cfg_attr(test, test)]
fn bindings_and_settings() {
    assert!((rotate(Vec2::NEG_Y, 0.) - Vec2::NEG_Y).length() < 0.001);
    let mut o = Orders {
        jump: true,
        throw_pressed: true,
        jump_held: true,
        ..default()
    };
    o.clear_edges();
    assert!(!o.jump && !o.throw_pressed && o.jump_held);
    let mut s = Settings {
        bindings: vec![],
        sensitivity: f32::NAN,
        ..default()
    };
    s.sanitize();
    assert_eq!(s.bindings.len(), ACTIONS.len());
    assert_eq!(s.sensitivity, 1.);
    s.rebind(act::JUMP, 0, "KeyW".into());
    assert_eq!(s.bindings[act::FORWARD][0], "Space");
    assert_eq!(s.bindings[act::JUMP][0], "KeyW");
    // Taking one of two alternatives leaves the other in place.
    s.rebind(act::RECENTER, 1, "East".into());
    assert_eq!(s.bindings[act::JUMP][1], "South");
    assert_eq!(s.bindings[act::RECENTER][1], "East");
    assert_eq!(label("South|East", 1), "B / A");
    let mut old = Settings {
        layout: 1,
        ..default()
    };
    old.bindings[act::JUMP][0] = "KeyX".into();
    old.sanitize();
    assert_eq!(old.bindings[act::JUMP][0], "Space");
}

#[cfg_attr(test, test)]
fn device_routing() {
    // Exercise the actual Bevy input routing, including gamepad-only movement.
    let mut world = World::new();
    world.insert_resource(ButtonInput::<KeyCode>::default());
    world.insert_resource(ButtonInput::<MouseButton>::default());
    world.insert_resource(AccumulatedMouseMotion::default());
    world.insert_resource(AccumulatedMouseScroll::default());
    world.insert_resource(Settings::default());
    world.insert_resource(Time::<Real>::default());
    world.insert_resource(FrameInput::default());
    world.insert_resource(Orders::default());
    world.insert_resource(crate::world::Orbit::default());
    let mut menu = crate::ui::Menu::default();
    menu.open = false;
    world.insert_resource(menu);
    let window = world
        .spawn(Window {
            focused: true,
            ..default()
        })
        .id();
    let mut gamepad = Gamepad::default();
    gamepad.digital_mut().press(GamepadButton::East);
    gamepad.digital_mut().press(GamepadButton::RightTrigger2);
    gamepad.analog_mut().set(GamepadAxis::LeftStickY, 1.);
    let pad = world.spawn(gamepad).id();
    let system = world.register_system(read);
    world.run_system(system).unwrap();
    let o = world.resource::<Orders>();
    assert!(o.direction.y < -0.99, "stick up moves forward");
    assert!(o.jump && o.jump_held, "Switch A jumps");
    assert!(o.crouch, "ZR crouches");
    world.despawn(pad);
    world.resource_mut::<Orders>().clear_edges();
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyD);
    world
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    world.run_system(system).unwrap();
    let o = world.resource::<Orders>();
    assert_eq!(o.direction, Vec2::X);
    assert!(!o.jump_held && o.throw_pressed && o.throw_held);
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ControlLeft);
    world.run_system(system).unwrap();
    assert!(
        world.resource::<Orders>().direction.length() < 0.4,
        "walk key slows"
    );
    world.resource_mut::<crate::ui::Menu>().open = true;
    world.run_system(system).unwrap();
    assert_eq!(world.resource::<FrameInput>().nav, 0);
    world.resource_mut::<AccumulatedMouseScroll>().delta.y = -1.;
    world.run_system(system).unwrap();
    assert_eq!(world.resource::<FrameInput>().nav, 1);
    assert_eq!(world.resource::<Orders>().direction, Vec2::ZERO);
    assert!(!world.resource::<Orders>().throw_pressed);
    world.resource_mut::<crate::ui::Menu>().open = false;
    world.get_mut::<Window>(window).unwrap().focused = false;
    if std::env::var_os("PJV_SHOT").is_none() {
        world.run_system(system).unwrap();
        assert!(!world.resource::<Orders>().throw_held);
    }
}
