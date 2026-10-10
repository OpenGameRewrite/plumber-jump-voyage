use crate::{
    input::{ACTIONS, FrameInput, Orders, Settings, act, label},
    movement::Player,
    world::Progress,
};
use bevy::{
    prelude::*,
    ui_widgets::{Activate, Button},
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

const INK: Color = Color::srgb(0.09, 0.18, 0.22);
const PAPER: Color = Color::srgb(0.99, 0.97, 0.92);
#[derive(Clone, Copy, Default, PartialEq)]
enum Page {
    #[default]
    Main,
    Controls,
    Guide,
    Wardrobe,
}
#[derive(Resource)]
pub struct Menu {
    pub open: bool,
    pub begun: bool,
    page: Page,
    selected: usize,
    offset: usize,
    capture: bool,
    cooldown: f32,
    pub notice: String,
    /// Last input device, so hints show the right buttons.
    pub pad: bool,
}
impl Default for Menu {
    fn default() -> Self {
        Self {
            open: std::env::var_os("PJV_PLAY").is_none(),
            begun: std::env::var_os("PJV_PLAY").is_some(),
            page: if std::env::var_os("PJV_WARDROBE").is_some() {
                Page::Wardrobe
            } else {
                Page::Main
            },
            selected: 0,
            offset: 0,
            capture: false,
            cooldown: 0.,
            notice: String::new(),
            pad: false,
        }
    }
}
#[derive(Component)]
struct Panel;
#[derive(Component)]
struct Header;
#[derive(Component)]
struct Row(usize);
#[derive(Component)]
struct RowText(usize);
#[derive(Component)]
struct Help;
#[derive(Component)]
struct Hud;
#[derive(Component)]
struct Bottom;
#[derive(Component)]
struct GameplayUi;
#[derive(Component)]
struct MenuInk;
#[derive(Message)]
struct Click(usize);
pub fn plugin(app: &mut App) {
    app.init_resource::<Menu>()
        .add_message::<Click>()
        .add_systems(Startup, (spawn, system_font).chain())
        .add_systems(PreUpdate, handle.after(crate::input::read))
        .add_systems(Update, (render, cursor));
}
fn text(t: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(t),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}
fn spawn(mut commands: Commands) {
    commands
        .spawn((
            GameplayUi,
            Node {
                position_type: PositionType::Absolute,
                left: px(36),
                top: px(28),
                padding: UiRect::all(px(12)),
                border: UiRect::bottom(px(4)),
                ..default()
            },
            BorderColor::all(Color::WHITE),
            BackgroundColor(Color::srgba(0., 0., 0., 0.3)),
        ))
        .with_children(|p| {
            p.spawn((
                text("SUN SPARKS  00 / 15     CAP TARGETS  0 / 3", 22., PAPER),
                Hud,
            ));
        });
    commands
        .spawn((
            GameplayUi,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(0),
                left: px(0),
                right: px(0),
                padding: UiRect::axes(px(36), px(14)),
                border: UiRect::top(px(3)),
                ..default()
            },
            BorderColor::all(Color::srgb(0.66, 0.59, 0.19)),
            BackgroundColor(PAPER),
        ))
        .with_children(|p| {
            p.spawn((text("", 20., INK), Bottom));
        });
    commands
        .spawn((
            Panel,
            Node {
                position_type: PositionType::Absolute,
                top: px(0),
                left: px(0),
                width: percent(100),
                height: percent(100),
                ..default()
            },
            BackgroundColor(Color::srgba(0.93, 0.06, 0.16, 0.82)),
        ))
        .with_children(|p| {
            // Original cartographic lines; no game artwork is embedded.
            for i in 0..7 {
                p.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: percent(48. - i as f32 * 5.),
                        top: percent(-30. + i as f32 * 5.),
                        width: percent(40. + i as f32 * 10.),
                        height: percent(130. - i as f32 * 10.),
                        border: UiRect::all(px(1.)),
                        border_radius: BorderRadius::all(percent(50.)),
                        ..default()
                    },
                    BorderColor::all(Color::srgba(1., 0.65, 0.65, 0.24)),
                ));
            }
            p.spawn((Node {
                position_type: PositionType::Absolute,
                left: percent(4.),
                top: percent(4.),
                width: percent(48.),
                flex_direction: FlexDirection::Column,
                row_gap: px(5.),
                ..default()
            },))
                .with_children(|p| {
                    p.spawn((text("PLUMBER JUMP", 18., PAPER), MenuInk));
                    p.spawn((text("VOYAGE", 62., PAPER), Header, MenuInk));
                    p.spawn((
                        text("SUNBREAK TERRACE   /   MOVEMENT PLAYGROUND", 13., PAPER),
                        MenuInk,
                    ));
                    for i in 0..9 {
                        p.spawn((
                            Button,
                            Row(i),
                            Node {
                                width: percent(100),
                                min_height: px(38),
                                padding: UiRect::axes(px(18), px(7)),
                                ..default()
                            },
                            BackgroundColor(Color::WHITE),
                        ))
                        .observe(move |_: On<Activate>, mut clicks: MessageWriter<Click>| {
                            clicks.write(Click(i));
                        })
                        .with_children(|b| {
                            b.spawn((text("", 23., INK), RowText(i)));
                        });
                    }
                    p.spawn((text("", 17., PAPER), Help, MenuInk));
                });
            p.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: px(0.),
                    left: px(0.),
                    width: percent(100.),
                    padding: UiRect::axes(px(36.), px(16.)),
                    border: UiRect::top(px(3.)),
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                },
                BackgroundColor(PAPER),
                BorderColor::all(Color::srgb(0.66, 0.59, 0.19)),
            ))
            .with_children(|p| {
                p.spawn(text("SUNBREAK TERRACE", 19., INK));
                p.spawn(text(
                    "Stick / arrows / wheel: select     A / Enter: confirm     B / Esc: back",
                    19.,
                    Color::srgb(0.58, 0.50, 0.13),
                ));
            });
        });
}
fn system_font(mut fonts: ResMut<Assets<Font>>, mut texts: Query<&mut TextFont>) {
    let paths = [
        "C:/Windows/Fonts/seguisb.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
    ];
    if let Some(font) = paths
        .iter()
        .find_map(|p| std::fs::read(p).ok().map(Font::from_bytes))
    {
        let handle = fonts.add(font);
        for mut text in &mut texts {
            text.font = bevy::text::FontSource::Handle(handle.clone());
        }
    }
}
fn count(page: Page, outfits: usize) -> usize {
    match page {
        Page::Main => 10,
        Page::Controls => ACTIONS.len() + 2,
        Page::Guide => 1,
        Page::Wardrobe => outfits + 4,
    }
}
fn execute(
    index: usize,
    step: i32,
    m: &mut Menu,
    s: &mut Settings,
    p: &mut Player,
    progress: &mut Progress,
    assets: &crate::local_assets::Wardrobe,
    exit: &mut MessageWriter<AppExit>,
) {
    let previous = s.clone();
    match m.page {
        Page::Main => match index {
            0 => {
                m.open = false;
                m.begun = true;
            }
            1 => {
                m.page = Page::Guide;
                m.selected = 0;
                m.offset = 0;
            }
            2 => {
                m.page = Page::Wardrobe;
                m.selected = assets.row(s.outfit);
                m.offset = m.selected.saturating_sub(8);
            }
            3 => {
                m.page = Page::Controls;
                m.selected = 0;
                m.offset = 0;
            }
            4 => {
                s.sensitivity = (s.sensitivity + step as f32 * 0.25).clamp(0.25, 3.);
            }
            5 => s.invert = !s.invert,
            6 => {
                p.checkpoint = crate::movement::SPAWN;
                p.reset();
                *progress = Progress::default();
                m.open = false;
                m.begun = true;
            }
            7 => s.volume = (s.volume + step as f32 * 0.1).clamp(0., 1.),
            8 => {
                *s = Settings {
                    outfit: s.outfit,
                    best: s.best,
                    ..default()
                };
            }
            9 => {
                exit.write(AppExit::Success);
            }
            _ => {}
        },
        Page::Controls => {
            if index < ACTIONS.len() {
                m.capture = true;
                m.cooldown = 0.2;
            } else if index == ACTIONS.len() {
                s.bindings = Settings::default().bindings;
            } else {
                m.page = Page::Main;
                m.selected = 3;
                m.offset = 0;
            }
        }
        Page::Guide => {
            m.page = Page::Main;
            m.selected = 1;
        }
        Page::Wardrobe => {
            if index < 3 + assets.outfits.len() {
                s.outfit = assets.slot(index);
                if !assets.outfits.is_empty() {
                    s.local_outfit = Some(s.outfit);
                }
            } else {
                m.page = Page::Main;
                m.selected = 2;
                m.offset = 0;
            }
        }
    }
    if *s != previous
        && let Err(e) = s.save()
    {
        m.notice = format!("Could not save settings: {e}");
    }
}
fn handle(
    time: Res<Time<Real>>,
    window: Single<&Window>,
    keys: Res<ButtonInput<KeyCode>>,
    frame: Res<FrameInput>,
    mut clicks: MessageReader<Click>,
    mut m: ResMut<Menu>,
    mut s: ResMut<Settings>,
    mut p: Single<&mut Player>,
    mut progress: ResMut<Progress>,
    assets: Res<crate::local_assets::Wardrobe>,
    mut orders: ResMut<Orders>,
    mut exit: MessageWriter<AppExit>,
) {
    m.cooldown = (m.cooldown - time.delta_secs()).max(0.);
    if (!window.focused
        || keys.any_pressed([
            KeyCode::SuperLeft,
            KeyCode::SuperRight,
            KeyCode::PrintScreen,
        ]))
        && std::env::var_os("PJV_SHOT").is_none()
    {
        m.open = true;
        *orders = Orders::default();
    }
    if frame.pad {
        m.pad = true;
    } else if frame.raw.as_ref().is_some_and(|r| r.0 == 0) {
        m.pad = false;
    }
    if m.capture {
        // Only the keyboard Escape cancels, so every pad button can be bound.
        if frame
            .raw
            .as_ref()
            .is_some_and(|r| r.0 == 0 && r.1 == "Escape")
        {
            m.capture = false;
        } else if m.cooldown == 0.
            && let Some((device, ref name)) = frame.raw
        {
            let selected = m.selected;
            s.rebind(selected, device, name.clone());
            m.capture = false;
            m.cooldown = 0.2;
            if let Err(e) = s.save() {
                m.notice = e;
            }
        }
        *orders = Orders::default();
        clicks.clear();
        return;
    }
    if m.cooldown == 0.
        && (frame.edge[act::PAUSE] || m.open && frame.back || keys.just_pressed(KeyCode::Escape))
    {
        if m.open && m.page != Page::Main {
            m.page = Page::Main;
            m.selected = 0;
            m.offset = 0;
        } else if m.begun {
            m.open = !m.open;
        }
        m.cooldown = 0.2;
        *orders = Orders::default();
    }
    if !m.open {
        clicks.clear();
        return;
    }
    if frame.nav != 0 {
        m.selected = (m.selected as i32 + frame.nav)
            .rem_euclid(count(m.page, assets.outfits.len()) as i32) as usize;
        if m.selected < m.offset {
            m.offset = m.selected;
        }
        if m.selected >= m.offset + 9 {
            m.offset = m.selected - 8;
        }
    }
    let click = clicks.read().last().map(|c| c.0 + m.offset);
    if m.cooldown == 0. && (frame.confirm || frame.step != 0 || click.is_some()) {
        let index = click.unwrap_or(m.selected);
        m.selected = index;
        execute(
            index,
            if frame.step == 0 { 1 } else { frame.step },
            &mut m,
            &mut s,
            &mut p,
            &mut progress,
            &assets,
            &mut exit,
        );
        m.cooldown = 0.15;
        *orders = Orders::default();
    }
}
fn render(
    m: Res<Menu>,
    s: Res<Settings>,
    p: Single<&Player>,
    progress: Res<Progress>,
    assets: Res<crate::local_assets::Wardrobe>,
    mut panel: Single<&mut Visibility, With<Panel>>,
    mut background: Single<&mut BackgroundColor, (With<Panel>, Without<Row>)>,
    mut gameplay: Query<&mut Visibility, (With<GameplayUi>, Without<Panel>)>,
    mut menu_ink: Query<&mut TextColor, (With<MenuInk>, Without<RowText>)>,
    mut header: Single<
        &mut Text,
        (
            With<Header>,
            Without<Hud>,
            Without<Bottom>,
            Without<Help>,
            Without<RowText>,
        ),
    >,
    mut rows: Query<(&Row, &mut BackgroundColor, &mut Node), Without<Panel>>,
    mut texts: Query<
        (&RowText, &mut Text, &mut TextColor),
        (
            Without<Header>,
            Without<Hud>,
            Without<Bottom>,
            Without<Help>,
        ),
    >,
    mut help: Single<
        &mut Text,
        (
            With<Help>,
            Without<Header>,
            Without<Hud>,
            Without<Bottom>,
            Without<RowText>,
        ),
    >,
    mut hud: Single<
        &mut Text,
        (
            With<Hud>,
            Without<Header>,
            Without<Help>,
            Without<Bottom>,
            Without<RowText>,
        ),
    >,
    mut bottom: Single<
        &mut Text,
        (
            With<Bottom>,
            Without<Header>,
            Without<Help>,
            Without<Hud>,
            Without<RowText>,
        ),
    >,
) {
    **panel = if m.open {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut v in &mut gameplay {
        *v = if m.open {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
    }
    header.0 = match m.page {
        Page::Main => "VOYAGE",
        Page::Controls => "CONTROLS",
        Page::Guide => "MOVE GUIDE",
        Page::Wardrobe => "WARDROBE",
    }
    .into();
    for mut color in &mut menu_ink {
        color.0 = if m.page == Page::Wardrobe { INK } else { PAPER };
    }
    let outfit = assets.name(s.outfit);
    let main = vec![
        if m.begun {
            "Resume exploring".into()
        } else {
            "Let's explore".into()
        },
        "Movement guide".into(),
        format!("Change outfit   /   {outfit}"),
        "Rebind controls".into(),
        format!("Camera sensitivity   <  {:.2}  >", s.sensitivity),
        format!("Invert camera Y   {}", if s.invert { "On" } else { "Off" }),
        "Restart the course".into(),
        format!("Sound volume   <  {:.0}%  >", s.volume * 100.),
        "Reset control settings".into(),
        "Quit".into(),
    ];
    for (row, mut color, mut node) in &mut rows {
        let index = row.0 + m.offset;
        node.display = if index < count(m.page, assets.outfits.len()) {
            Display::Flex
        } else {
            Display::None
        };
        *color = BackgroundColor(if m.selected == index {
            Color::WHITE
        } else {
            Color::NONE
        });
    }
    for (row, mut text, mut color) in &mut texts {
        let i = row.0 + m.offset;
        color.0 = if i == m.selected || m.page == Page::Wardrobe {
            INK
        } else {
            PAPER
        };
        text.0 = match m.page {
            Page::Main => main.get(i).cloned().unwrap_or_default(),
            Page::Controls => {
                if i < ACTIONS.len() {
                    format!(
                        "{}   |   {}   |   {}",
                        ACTIONS[i].0,
                        label(&s.bindings[i][0], 0),
                        label(&s.bindings[i][1], 1)
                    )
                } else if i == ACTIONS.len() {
                    "Restore default bindings".into()
                } else {
                    "Back".into()
                }
            }
            Page::Guide => "Back to voyage".into(),
            Page::Wardrobe => {
                if i < 3 + assets.outfits.len() {
                    let slot = assets.slot(i);
                    format!(
                        "{} {}",
                        if slot == s.outfit { "[ON]" } else { "    " },
                        assets.name(slot)
                    )
                } else {
                    "Back to voyage".into()
                }
            }
        };
    }
    background.0 = if m.page == Page::Wardrobe {
        Color::srgb(0.98, 0.77, 0.02)
    } else {
        Color::srgb(0.93, 0.06, 0.16)
    };
    help.0 = if m.capture {
        format!(
            "Press a key, mouse button or gamepad button for {}. Escape cancels. A binding already in use moves to the old one.",
            ACTIONS[m.selected].0
        )
    } else if m.page == Page::Guide {
        guide(&s, m.pad)
    } else if !m.notice.is_empty() {
        m.notice.clone()
    } else if m.page == Page::Controls {
        format!(
            "Action   |   keyboard & mouse   |   gamepad (Switch names by position: B = bottom, A = right)\nConfirm to rebind. {}-{} of {}",
            m.offset + 1,
            (m.offset + 9).min(count(m.page, assets.outfits.len())),
            count(m.page, assets.outfits.len())
        )
    } else if m.page == Page::Wardrobe {
        format!(
            "{}\nOriginal outfits come from your own game data; explorers are the ROMless originals.",
            assets.status
        )
    } else {
        format!(
            "Scroll / arrows / D-pad: select | Enter / A: choose\nLeft / Right: change | Mouse: choose\n{}",
            assets.status
        )
    };
    hud.0 = format!(
        "SUN SPARKS  {:02} / 15     CAP TARGETS  {} / 3     {:02}:{:04.1}\n{}",
        progress.gems.len(),
        progress.targets.len(),
        (progress.time / 60.) as u32,
        progress.time % 60.,
        p.mode.label().to_uppercase(),
    );
    let k = |a: usize| {
        if m.pad {
            label(&s.bindings[a][1], 1)
        } else {
            label(&s.bindings[a][0], 0)
        }
    };
    bottom.0 = if progress.message_time > 0. {
        progress.message.clone()
    } else if p.pos.z > 19. {
        format!(
            "TIDAL POOL  |  {}: swim up   {}: dive down   {}: swim dash",
            k(act::JUMP),
            k(act::CROUCH),
            k(act::THROW)
        )
    } else if p.pos.z < -19. {
        format!(
            "SKY STEPS  |  Cap jump: jump, {}, then {} + {} into the cap",
            k(act::THROW),
            k(act::CROUCH),
            k(act::THROW)
        )
    } else if p.pos.x > 7. {
        format!(
            "WALL GARDEN  |  Slide down a wall, {} to kick off, grab ledges",
            k(act::JUMP)
        )
    } else if p.pos.x < -20. {
        "FLOATING WALK  |  Moving platforms; the far island is a checkpoint".into()
    } else {
        format!(
            "SUNBREAK TERRACE  |  Jump {}   Crouch {}   Cap {}   Menu {}",
            k(act::JUMP),
            k(act::CROUCH),
            k(act::THROW),
            k(act::PAUSE)
        )
    };
}
fn guide(s: &Settings, pad: bool) -> String {
    let k = |a: usize| {
        if pad {
            label(&s.bindings[a][1], 1)
        } else {
            label(&s.bindings[a][0], 0)
        }
    };
    let (j, c, t) = (k(act::JUMP), k(act::CROUCH), k(act::THROW));
    format!(
        "Jump again right as you land for a double, then a triple jump.\n\
         {c} + {j}: long jump while running, backflip standing still.\n\
         Reverse at speed to skid, then {j}: side somersault. Circle the stick, then {j}: spin jump.\n\
         {c} in the air: ground pound. {j} on impact: pound jump. {t} during a pound: dive.\n\
         {c} + {t}: roll. Tap {t} on the roll's rhythm to boost; keep {c} held or it stops. {j}: roll jump.\n\
         {t}, hold it, then dive into the cap ({c} + {t} in the air) for a cap jump.\n\
         {}: up throw. {}: down throw. {}: spin throw. {}: home the cap.\n\
         Slide down walls and {j} to kick off. Grab ledges; push toward the wall or {j} to climb.\n\
         Collect 15 sun sparks and hit all 3 cap targets.",
        k(act::UP_THROW),
        k(act::DOWN_THROW),
        k(act::SPIN_THROW),
        k(act::HOMING)
    )
}
fn cursor(m: Res<Menu>, mut window: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>) {
    let active = !m.open && window.0.focused && std::env::var_os("PJV_SHOT").is_none();
    window.1.visible = !active;
    window.1.grab_mode = if active {
        CursorGrabMode::Confined
    } else {
        CursorGrabMode::None
    };
}
pub fn self_check() {
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
