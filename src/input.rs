use bevy::{input::mouse::AccumulatedMouseMotion, prelude::*};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const ACTIONS: [(&str, &str, &str); 18] = [
    ("Move left", "KeyA", "DPadLeft"),
    ("Move right", "KeyD", "DPadRight"),
    ("Move forward", "KeyW", "DPadUp"),
    ("Move back", "KeyS", "DPadDown"),
    ("Jump / swim up", "Space", "South"),
    ("Crouch / pound", "ControlLeft", "LeftTrigger2"),
    ("Throw / dive / roll", "MouseLeft", "West"),
    ("Walk slowly", "ShiftLeft", "RightTrigger2"),
    ("Roll boost / homing", "KeyE", "RightTrigger"),
    ("Upward throw", "KeyQ", "North"),
    ("Downward throw", "KeyF", "East"),
    ("Spin throw", "KeyC", "LeftTrigger"),
    ("Spin", "KeyV", "LeftThumb"),
    ("Recenter camera", "KeyR", "RightThumb"),
    ("Return to checkpoint", "Backspace", "Select"),
    ("Pause", "Escape", "Start"),
    ("Camera left", "KeyJ", "C"),
    ("Camera right", "KeyL", "Z"),
];
#[derive(Resource, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub bindings: Vec<[String; 2]>,
    pub sensitivity: f32,
    pub invert: bool,
    pub outfit: usize,
    pub best: u32,
    pub volume: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            bindings: ACTIONS.iter().map(|a| [a.1.into(), a.2.into()]).collect(),
            sensitivity: 1.,
            invert: false,
            outfit: 0,
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
        if self.bindings.len() != ACTIONS.len() {
            self.bindings = Self::default().bindings;
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
        self.outfit = self.outfit.min(100);
    }
    pub fn save(&self) -> Result<(), String> {
        let p = save_path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(p, bytes).map_err(|e| e.to_string())
    }
    pub fn rebind(&mut self, index: usize, device: usize, name: String) {
        let old = self.bindings[index][device].clone();
        if let Some(other) = self.bindings.iter().position(|b| b[device] == name) {
            self.bindings[other][device] = old;
        }
        self.bindings[index][device] = name;
    }
}
#[derive(Resource, Default, Clone, Debug)]
pub struct Orders {
    pub direction: Vec2,
    pub look: Vec2,
    pub jump: bool,
    pub jump_held: bool,
    pub crouch: bool,
    pub crouch_pressed: bool,
    pub throw_pressed: bool,
    pub throw_held: bool,
    pub walk: bool,
    pub boost: bool,
    pub homing: bool,
    pub up_throw: bool,
    pub down_throw: bool,
    pub spin_throw: bool,
    pub spin: bool,
    pub recenter: bool,
    pub reset: bool,
}
impl Orders {
    pub fn clear_edges(&mut self) {
        self.jump = false;
        self.crouch_pressed = false;
        self.throw_pressed = false;
        self.boost = false;
        self.homing = false;
        self.up_throw = false;
        self.down_throw = false;
        self.spin_throw = false;
        self.spin = false;
        self.recenter = false;
        self.reset = false;
    }
}
#[derive(Resource, Default)]
pub struct FrameInput {
    pub held: [bool; 18],
    pub edge: [bool; 18],
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
    for (i, b) in settings.bindings.iter().enumerate() {
        f.held[i] = keys.get_pressed().any(|k| format!("{k:?}") == b[0])
            || mouse.get_pressed().any(|k| format!("Mouse{k:?}") == b[0]);
        f.edge[i] = keys.get_just_pressed().any(|k| format!("{k:?}") == b[0])
            || mouse
                .get_just_pressed()
                .any(|k| format!("Mouse{k:?}") == b[0]);
    }
    f.raw = keys
        .get_just_pressed()
        .next()
        .map(|k| (0, format!("{k:?}")))
        .or_else(|| {
            mouse
                .get_just_pressed()
                .next()
                .map(|k| (0, format!("Mouse{k:?}")))
        });
    f.nav =
        keys.just_pressed(KeyCode::ArrowDown) as i32 - keys.just_pressed(KeyCode::ArrowUp) as i32;
    f.step = keys.just_pressed(KeyCode::ArrowRight) as i32
        - keys.just_pressed(KeyCode::ArrowLeft) as i32;
    f.confirm = keys.just_pressed(KeyCode::Enter);
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
        for (i, b) in settings.bindings.iter().enumerate() {
            f.held[i] |= p.get_pressed().any(|k| format!("{k:?}") == b[1]);
            f.edge[i] |= p.get_just_pressed().any(|k| format!("{k:?}") == b[1]);
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
        if ls.length() > 0.16 {
            stick += Vec2::new(ls.x, -ls.y) * (ls.length() - 0.16) / 0.84;
            f.pad = true;
        }
        let rs = p.right_stick();
        if rs.length() > 0.16 {
            look += Vec2::new(rs.x, -rs.y) * time.delta_secs().min(0.05) * 2.6;
            f.pad = true;
        }
        if let Some(k) = p.get_just_pressed().next() {
            f.raw = Some((1, format!("{k:?}")));
            f.pad = true;
        }
        f.nav += p.just_pressed(GamepadButton::DPadDown) as i32
            - p.just_pressed(GamepadButton::DPadUp) as i32;
        f.step += p.just_pressed(GamepadButton::DPadRight) as i32
            - p.just_pressed(GamepadButton::DPadLeft) as i32;
        f.confirm |= p.just_pressed(GamepadButton::South);
        f.back |= p.just_pressed(GamepadButton::East);
    } else {
        *active = None;
    }
    stick += Vec2::new(
        f.held[1] as i32 as f32 - f.held[0] as i32 as f32,
        f.held[3] as i32 as f32 - f.held[2] as i32 as f32,
    );
    if menu.open || (!window.focused && std::env::var_os("PJV_SHOT").is_none()) {
        *orders = Orders::default();
    } else {
        orders.direction = rotate(stick.clamp_length_max(1.), camera.yaw);
        orders.look =
            look * settings.sensitivity * Vec2::new(1., if settings.invert { -1. } else { 1. });
        orders.look.x += (f.held[17] as i32 - f.held[16] as i32) as f32 * time.delta_secs() * 2.;
        orders.jump |= f.edge[4];
        orders.jump_held = f.held[4];
        orders.crouch = f.held[5];
        orders.crouch_pressed |= f.edge[5];
        orders.throw_pressed |= f.edge[6];
        orders.throw_held = f.held[6] || f.held[9] || f.held[10] || f.held[11];
        orders.walk = f.held[7];
        orders.boost |= f.edge[8];
        orders.homing |= f.edge[8];
        orders.up_throw |= f.edge[9];
        orders.down_throw |= f.edge[10];
        orders.spin_throw |= f.edge[11];
        orders.spin |= f.edge[12];
        orders.recenter |= f.edge[13];
        orders.reset |= f.edge[14];
    }
    // Optional graphical check drives the same orders through the real controller.
    if !menu.open && std::env::var_os("PJV_SMOKE").is_some() {
        let phase = (time.elapsed_secs() / 0.7) as u8;
        let edge = phase != *smoke_phase;
        *smoke_phase = phase;
        *orders = Orders {
            direction: if phase < 5 { Vec2::NEG_Y } else { Vec2::ZERO },
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
    assert_eq!(s.bindings.len(), 18);
    assert_eq!(s.sensitivity, 1.);
    s.rebind(4, 0, "KeyW".into());
    assert_eq!(s.bindings[2][0], "Space");
    assert_eq!(s.bindings[4][0], "KeyW");
    assert_eq!(s.bindings[4][1], "South");

    // Exercise the actual Bevy input routing, including gamepad-only movement.
    let mut world = World::new();
    world.insert_resource(ButtonInput::<KeyCode>::default());
    world.insert_resource(ButtonInput::<MouseButton>::default());
    world.insert_resource(AccumulatedMouseMotion::default());
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
    gamepad.digital_mut().press(GamepadButton::DPadUp);
    gamepad.digital_mut().press(GamepadButton::South);
    let pad = world.spawn(gamepad).id();
    let system = world.register_system(read);
    world.run_system(system).unwrap();
    let o = world.resource::<Orders>();
    assert_eq!(o.direction, Vec2::NEG_Y);
    assert!(o.jump && o.jump_held);
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
    world.resource_mut::<crate::ui::Menu>().open = true;
    world.run_system(system).unwrap();
    assert_eq!(world.resource::<Orders>().direction, Vec2::ZERO);
    assert!(!world.resource::<Orders>().throw_pressed);
    world.resource_mut::<crate::ui::Menu>().open = false;
    world.get_mut::<Window>(window).unwrap().focused = false;
    if std::env::var_os("PJV_SHOT").is_none() {
        world.run_system(system).unwrap();
        assert!(!world.resource::<Orders>().throw_held);
    }
}
