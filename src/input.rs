use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    prelude::*,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Action indices into `ACTIONS` and `Settings::bindings`.
pub mod act {
    pub const LEFT: usize = 0;
    pub const RIGHT: usize = 1;
    pub const FORWARD: usize = 2;
    pub const BACK: usize = 3;
    pub const JUMP: usize = 4;
    pub const CROUCH: usize = 5;
    pub const THROW: usize = 6;
    pub const WALK: usize = 7;
    pub const HOMING: usize = 8;
    pub const UP_THROW: usize = 9;
    pub const DOWN_THROW: usize = 10;
    pub const SPIN_THROW: usize = 11;
    pub const RECENTER: usize = 12;
    pub const RESET: usize = 13;
    pub const PAUSE: usize = 14;
    pub const CAM_LEFT: usize = 15;
    pub const CAM_RIGHT: usize = 16;
    pub const CAM_UP: usize = 17;
    pub const CAM_DOWN: usize = 18;
}
/// Label, keyboard/mouse default, gamepad default. `|` separates alternatives.
/// Gamepad defaults follow the Switch layout by position: both bottom/right
/// face buttons jump, both left/top throw, both ZL/ZR crouch, L/R recenter.
/// Motion-flick moves (homing, upward and downward throws) get buttons.
pub const ACTIONS: [(&str, &str, &str); 19] = [
    ("Move left", "KeyA", ""),
    ("Move right", "KeyD", ""),
    ("Move forward", "KeyW", ""),
    ("Move back", "KeyS", ""),
    ("Jump", "Space", "South|East"),
    (
        "Crouch / ground pound",
        "ShiftLeft",
        "LeftTrigger2|RightTrigger2",
    ),
    ("Cap throw / roll / dive", "MouseLeft", "West|North"),
    ("Walk", "ControlLeft", ""),
    ("Homing throw (flick)", "MouseRight", "RightTrigger"),
    ("Upward throw (flick)", "KeyE", "DPadUp"),
    ("Downward throw (flick)", "KeyQ", "DPadDown"),
    ("Spin throw", "KeyC", "LeftThumb"),
    ("Recenter camera", "KeyR", "LeftTrigger"),
    ("Return to checkpoint", "Backspace", "Select"),
    ("Pause", "Escape", "Start"),
    ("Camera left", "ArrowLeft", ""),
    ("Camera right", "ArrowRight", ""),
    ("Camera up", "ArrowUp", ""),
    ("Camera down", "ArrowDown", ""),
];
/// Bump when default bindings change meaning so old saves do not keep them.
const LAYOUT: u32 = 2;
#[derive(Resource, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub layout: u32,
    pub bindings: Vec<[String; 2]>,
    pub sensitivity: f32,
    pub invert: bool,
    /// Wardrobe slot: 0..3 are the original explorers, 3.. local outfits.
    pub outfit: usize,
    /// Wardrobe slot chosen in the local-asset build, kept separately so the
    /// ROMless build never inherits it and the asset build defaults to an
    /// original outfit.
    pub local_outfit: Option<usize>,
    pub best: u32,
    pub volume: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            layout: LAYOUT,
            bindings: ACTIONS.iter().map(|a| [a.1.into(), a.2.into()]).collect(),
            sensitivity: 1.,
            invert: false,
            outfit: 0,
            local_outfit: None,
            best: 0,
            volume: 1.,
        }
    }
}
pub fn save_path() -> PathBuf {
    std::env::var_os("PJV_SAVE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("APPDATA")
                .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("local"))
                .join("PlumberJumpVoyage/settings.json")
        })
}
impl Settings {
    pub fn load() -> Self {
        let mut s = std::fs::read(save_path())
            .ok()
            .and_then(|b| serde_json::from_slice::<Self>(&b).ok())
            .unwrap_or_default();
        s.sanitize();
        s
    }
    pub fn sanitize(&mut self) {
        if self.bindings.len() != ACTIONS.len() || self.layout != LAYOUT {
            self.bindings = Self::default().bindings;
            self.layout = LAYOUT;
        }
        if !self.sensitivity.is_finite() {
            self.sensitivity = 1.;
        }
        self.sensitivity = self.sensitivity.clamp(0.25, 3.);
        self.volume = if self.volume.is_finite() {
            self.volume.clamp(0., 1.)
        } else {
            1.
        };
        self.outfit = self.outfit.min(2);
        self.local_outfit = self.local_outfit.map(|o| o.min(200));
    }
    pub fn save(&self) -> Result<(), String> {
        let p = save_path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let mut stored = self.clone();
        // The ROMless build reads `outfit`; keep it an explorer.
        if stored.outfit > 2 {
            stored.outfit = 0;
        }
        let bytes = serde_json::to_vec_pretty(&stored).map_err(|e| e.to_string())?;
        std::fs::write(p, bytes).map_err(|e| e.to_string())
    }
    /// Bind one input; whichever action already used it takes the old binding.
    pub fn rebind(&mut self, index: usize, device: usize, name: String) {
        let old = self.bindings[index][device].clone();
        for (i, b) in self.bindings.iter_mut().enumerate() {
            if i != index && bound(&b[device], &name) {
                let rest: Vec<&str> = b[device].split('|').filter(|n| *n != name).collect();
                b[device] = if rest.is_empty() {
                    old.clone()
                } else {
                    rest.join("|")
                };
            }
        }
        self.bindings[index][device] = name;
    }
}
pub fn bound(binding: &str, name: &str) -> bool {
    binding.split('|').any(|b| b == name)
}
/// Human-readable binding, using Switch button names by position for pads.
pub fn label(binding: &str, device: usize) -> String {
    if binding.is_empty() {
        return if device == 0 {
            "-".into()
        } else {
            "stick".into()
        };
    }
    binding
        .split('|')
        .map(|b| {
            if device == 1 {
                match b {
                    "South" => "B",
                    "East" => "A",
                    "West" => "Y",
                    "North" => "X",
                    "LeftTrigger" => "L",
                    "RightTrigger" => "R",
                    "LeftTrigger2" => "ZL",
                    "RightTrigger2" => "ZR",
                    "Select" => "-",
                    "Start" => "+",
                    "LeftThumb" => "L-stick press",
                    "RightThumb" => "R-stick press",
                    "DPadUp" => "D-pad up",
                    "DPadDown" => "D-pad down",
                    "DPadLeft" => "D-pad left",
                    "DPadRight" => "D-pad right",
                    other => other,
                }
                .to_owned()
            } else {
                match b {
                    "MouseLeft" => "Left click".into(),
                    "MouseRight" => "Right click".into(),
                    "MouseMiddle" => "Middle click".into(),
                    "ShiftLeft" => "Left Shift".into(),
                    "ShiftRight" => "Right Shift".into(),
                    "ControlLeft" => "Left Ctrl".into(),
                    "ControlRight" => "Right Ctrl".into(),
                    "AltLeft" => "Left Alt".into(),
                    "ArrowLeft" => "Left arrow".into(),
                    "ArrowRight" => "Right arrow".into(),
                    "ArrowUp" => "Up arrow".into(),
                    "ArrowDown" => "Down arrow".into(),
                    other => other
                        .trim_start_matches("Key")
                        .trim_start_matches("Digit")
                        .to_owned(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" / ")
}
#[derive(Resource, Default, Clone, Debug)]
pub struct Orders {
    /// World-space XZ direction; length is analog tilt (walk < 0.5 < run).
    pub direction: Vec2,
    /// The stick as held, relative to the camera (y negative is up), and the
    /// camera yaw: climbing uses screen directions rather than world ones.
    pub stick: Vec2,
    pub camera_yaw: f32,
    pub look: Vec2,
    pub jump: bool,
    pub jump_held: bool,
    pub crouch: bool,
    pub crouch_pressed: bool,
    pub throw_pressed: bool,
    pub throw_held: bool,
    pub homing: bool,
    pub up_throw: bool,
    pub down_throw: bool,
    pub spin_throw: bool,
    pub recenter: bool,
    pub reset: bool,
}
impl Orders {
    pub fn clear_edges(&mut self) {
        self.jump = false;
        self.crouch_pressed = false;
        self.throw_pressed = false;
        self.homing = false;
        self.up_throw = false;
        self.down_throw = false;
        self.spin_throw = false;
        self.recenter = false;
        self.reset = false;
    }
}
#[derive(Resource, Default)]
pub struct FrameInput {
    pub held: [bool; ACTIONS.len()],
    pub edge: [bool; ACTIONS.len()],
    pub raw: Option<(usize, String)>,
    pub nav: i32,
    pub step: i32,
    pub confirm: bool,
    pub back: bool,
    pub pad: bool,
}
pub fn plugin(app: &mut App) {
    app.insert_resource(Settings::load())
        .init_resource::<Orders>()
        .init_resource::<FrameInput>()
        .add_systems(PreUpdate, read.after(bevy::input::InputSystems));
}
pub fn read(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    pads: Query<(Entity, &Gamepad)>,
    settings: Res<Settings>,
    time: Res<Time<Real>>,
    mut frame: ResMut<FrameInput>,
    mut orders: ResMut<Orders>,
    menu: Res<crate::ui::Menu>,
    camera: Res<crate::world::Orbit>,
    window: Single<&Window>,
    mut active: Local<Option<Entity>>,
    mut repeat: Local<f32>,
    mut smoke_phase: Local<u8>,
) {
    *repeat = (*repeat - time.delta_secs()).max(0.);
    let mut f = FrameInput::default();
    let held_keys: Vec<String> = keys
        .get_pressed()
        .map(|k| format!("{k:?}"))
        .chain(mouse.get_pressed().map(|k| format!("Mouse{k:?}")))
        .collect();
    let new_keys: Vec<String> = keys
        .get_just_pressed()
        .map(|k| format!("{k:?}"))
        .chain(mouse.get_just_pressed().map(|k| format!("Mouse{k:?}")))
        .collect();
    for (i, b) in settings.bindings.iter().enumerate() {
        f.held[i] = held_keys.iter().any(|k| bound(&b[0], k));
        f.edge[i] = new_keys.iter().any(|k| bound(&b[0], k));
    }
    f.raw = new_keys.first().map(|k| (0, k.clone()));
    f.nav =
        keys.just_pressed(KeyCode::ArrowDown) as i32 - keys.just_pressed(KeyCode::ArrowUp) as i32;
    if menu.open && scroll.delta.y != 0. {
        f.nav -= scroll.delta.y.signum() as i32;
    }
    f.step = keys.just_pressed(KeyCode::ArrowRight) as i32
        - keys.just_pressed(KeyCode::ArrowLeft) as i32;
    f.confirm = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space);
    f.back = keys.just_pressed(KeyCode::Escape);
    let mut stick = Vec2::ZERO;
    let mut look = motion.delta * 0.0025;
    // One active gamepad; another controller cannot add a second movement command.
    for (entity, p) in &pads {
        if p.get_just_pressed().next().is_some() || p.left_stick().length() > 0.2 {
            *active = Some(entity);
        }
    }
    if active.is_none() {
        *active = pads.iter().next().map(|p| p.0);
    }
    if let Some((_, p)) = active.and_then(|e| pads.get(e).ok()) {
        let held: Vec<String> = p.get_pressed().map(|k| format!("{k:?}")).collect();
        let new: Vec<String> = p.get_just_pressed().map(|k| format!("{k:?}")).collect();
        for (i, b) in settings.bindings.iter().enumerate() {
            f.held[i] |= held.iter().any(|k| bound(&b[1], k));
            f.edge[i] |= new.iter().any(|k| bound(&b[1], k));
        }
        let ls = p.left_stick();
        if menu.open && ls.length() > 0.55 {
            if *repeat == 0. {
                if ls.y.abs() > ls.x.abs() {
                    f.nav -= ls.y.signum() as i32;
                } else {
                    f.step += ls.x.signum() as i32;
                }
                *repeat = 0.22;
            }
        } else if ls.length() < 0.2 {
            *repeat = 0.;
        }
        // Radial dead zone with rescale keeps full analog walk-to-run range.
        if ls.length() > 0.16 {
            stick += ls.normalize() * Vec2::new(1., -1.) * ((ls.length() - 0.16) / 0.8).min(1.);
            f.pad = true;
        }
        let rs = p.right_stick();
        if rs.length() > 0.16 {
            let amount = ((rs.length() - 0.16) / 0.84).min(1.);
            // Squared response: precise small adjustments, fast full tilt.
            look += rs.normalize()
                * Vec2::new(1., -1.)
                * amount
                * amount
                * time.delta_secs().min(0.05)
                * 3.2;
            f.pad = true;
        }
        if let Some(k) = new.first() {
            f.raw = Some((1, k.clone()));
            f.pad = true;
        }
        f.nav += p.just_pressed(GamepadButton::DPadDown) as i32
            - p.just_pressed(GamepadButton::DPadUp) as i32;
        f.step += p.just_pressed(GamepadButton::DPadRight) as i32
            - p.just_pressed(GamepadButton::DPadLeft) as i32;
        // Menus follow the Switch: right face button (A) confirms, bottom (B) backs out.
        f.confirm |= p.just_pressed(GamepadButton::East);
        f.back |= p.just_pressed(GamepadButton::South);
    } else {
        *active = None;
    }
    let keys_dir = Vec2::new(
        f.held[act::RIGHT] as i32 as f32 - f.held[act::LEFT] as i32 as f32,
        f.held[act::BACK] as i32 as f32 - f.held[act::FORWARD] as i32 as f32,
    );
    stick += keys_dir.normalize_or_zero();
    if f.held[act::WALK] {
        stick = stick.clamp_length_max(0.35);
    }
    if menu.open || (!window.focused && std::env::var_os("PJV_SHOT").is_none()) {
        *orders = Orders::default();
    } else {
        orders.direction = rotate(stick.clamp_length_max(1.), camera.yaw);
        orders.stick = stick.clamp_length_max(1.);
        orders.camera_yaw = camera.yaw;
        orders.look =
            look * settings.sensitivity * Vec2::new(1., if settings.invert { -1. } else { 1. });
        let dt = time.delta_secs().min(0.05);
        orders.look.x +=
            (f.held[act::CAM_RIGHT] as i32 - f.held[act::CAM_LEFT] as i32) as f32 * dt * 2.4;
        orders.look.y += (f.held[act::CAM_UP] as i32 - f.held[act::CAM_DOWN] as i32) as f32 * dt;
        orders.jump |= f.edge[act::JUMP];
        orders.jump_held = f.held[act::JUMP];
        orders.crouch = f.held[act::CROUCH];
        orders.crouch_pressed |= f.edge[act::CROUCH];
        orders.throw_pressed |= f.edge[act::THROW];
        orders.throw_held = f.held[act::THROW]
            || f.held[act::UP_THROW]
            || f.held[act::DOWN_THROW]
            || f.held[act::SPIN_THROW]
            || f.held[act::HOMING];
        orders.homing |= f.edge[act::HOMING];
        orders.up_throw |= f.edge[act::UP_THROW];
        orders.down_throw |= f.edge[act::DOWN_THROW];
        orders.spin_throw |= f.edge[act::SPIN_THROW];
        orders.recenter |= f.edge[act::RECENTER];
        orders.reset |= f.edge[act::RESET];
    }
    // Optional graphical check drives the same orders through the real controller.
    if !menu.open && std::env::var_os("PJV_SMOKE").is_some() {
        // PJV_SMOKE=<seconds> delays the script, e.g. until shaders are ready.
        let delay: f32 = std::env::var("PJV_SMOKE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.);
        let started = time.elapsed_secs() >= delay;
        let phase = ((time.elapsed_secs() - delay).max(0.) / 0.7) as u8;
        let edge = phase != *smoke_phase;
        *smoke_phase = phase;
        let script = std::env::var("PJV_SCRIPT").unwrap_or_default();
        if script == "walk" {
            // A slow walk, then a full run, for close inspection.
            *orders = Orders {
                direction: match phase {
                    _ if !started => Vec2::ZERO,
                    0..=3 => Vec2::Y * 0.3,
                    _ => Vec2::Y,
                },
                ..default()
            };
            *frame = f;
            return;
        }
        if script == "pole" || script == "ledge" {
            // From PJV_SPAWN: catch a pole and climb then circle it, or catch an
            // edge (forward is -Z), hang, then climb up.
            let pole = script == "pole";
            *orders = Orders {
                reset: started && time.elapsed_secs() - delay < 0.04,
                direction: match phase {
                    _ if !started => Vec2::ZERO,
                    0 if !pole && time.elapsed_secs() - delay < 0.25 => Vec2::NEG_Y,
                    1..=3 if pole => Vec2::NEG_Y,
                    4 | 5 if pole => Vec2::X,
                    4 if !pole => Vec2::NEG_Y,
                    _ => Vec2::ZERO,
                },
                ..default()
            };
            *frame = f;
            return;
        }
        *orders = Orders {
            // Across open ground: run, jump, throw mid-run, then roll back.
            direction: match phase {
                _ if !started => Vec2::ZERO,
                0..=2 => Vec2::X,
                3 | 4 => Vec2::NEG_X,
                _ => Vec2::ZERO,
            },
            jump: phase == 1 && edge,
            jump_held: phase == 1,
            throw_pressed: matches!(phase, 2 | 3 | 6) && edge,
            throw_held: matches!(phase, 2..=4 | 6..=9),
            crouch: phase == 3,
            reset: phase == 5 && edge,
            ..default()
        };
    }
    *frame = f;
}
pub fn rotate(v: Vec2, yaw: f32) -> Vec2 {
    Vec2::new(
        v.x * yaw.cos() + v.y * yaw.sin(),
        -v.x * yaw.sin() + v.y * yaw.cos(),
    )
}
pub fn self_check() {
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
