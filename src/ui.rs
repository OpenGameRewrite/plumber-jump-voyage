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
#[derive(Clone, Copy, Debug, PartialEq)]
enum MainItem {
    Resume,
    Guide,
    Wardrobe,
    Controls,
    Sensitivity,
    Invert,
    Restart,
    Volume,
    Defaults,
    Quit,
}
const MAIN_ITEMS: [MainItem; 10] = [
    MainItem::Resume,
    MainItem::Guide,
    MainItem::Wardrobe,
    MainItem::Controls,
    MainItem::Sensitivity,
    MainItem::Invert,
    MainItem::Restart,
    MainItem::Volume,
    MainItem::Defaults,
    MainItem::Quit,
];
impl MainItem {
    fn index(self) -> usize {
        MAIN_ITEMS
            .iter()
            .position(|item| *item == self)
            .expect("menu item is listed")
    }
    fn label(self, m: &Menu, s: &Settings, outfit: &str) -> String {
        match self {
            Self::Resume => if m.begun {
                "Resume exploring"
            } else {
                "Let's explore"
            }
            .into(),
            Self::Guide => "Movement guide".into(),
            Self::Wardrobe => format!("Change outfit   /   {outfit}"),
            Self::Controls => "Rebind controls".into(),
            Self::Sensitivity => format!("Camera sensitivity   <  {:.2}  >", s.sensitivity),
            Self::Invert => format!("Invert camera Y   {}", if s.invert { "On" } else { "Off" }),
            Self::Restart => "Restart the course".into(),
            Self::Volume => format!("Sound volume   <  {:.0}%  >", s.volume * 100.),
            Self::Defaults => "Reset control settings".into(),
            Self::Quit => "Quit".into(),
        }
    }
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
        .add_systems(Update, (render, render_hud, cursor));
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
        Page::Main => MAIN_ITEMS.len(),
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
        Page::Main => match MAIN_ITEMS.get(index).copied() {
            Some(MainItem::Resume) => {
                m.open = false;
                m.begun = true;
            }
            Some(MainItem::Guide) => {
                m.page = Page::Guide;
                m.selected = 0;
                m.offset = 0;
            }
            Some(MainItem::Wardrobe) => {
                m.page = Page::Wardrobe;
                m.selected = assets.row(s.outfit);
                m.offset = m.selected.saturating_sub(8);
            }
            Some(MainItem::Controls) => {
                m.page = Page::Controls;
                m.selected = 0;
                m.offset = 0;
            }
            Some(MainItem::Sensitivity) => {
                s.sensitivity = (s.sensitivity + step as f32 * 0.25).clamp(0.25, 3.);
            }
            Some(MainItem::Invert) => s.invert = !s.invert,
            Some(MainItem::Restart) => {
                p.checkpoint = crate::movement::SPAWN;
                p.reset();
                *progress = Progress::default();
                m.open = false;
                m.begun = true;
            }
            Some(MainItem::Volume) => s.volume = (s.volume + step as f32 * 0.1).clamp(0., 1.),
            Some(MainItem::Defaults) => {
                *s = Settings {
                    outfit: s.outfit,
                    best: s.best,
                    ..default()
                };
            }
            Some(MainItem::Quit) => {
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
                m.selected = MainItem::Controls.index();
                m.offset = 0;
            }
        }
        Page::Guide => {
            m.page = Page::Main;
            m.selected = MainItem::Guide.index();
        }
        Page::Wardrobe => {
            if index < 3 + assets.outfits.len() {
                s.outfit = assets.slot(index);
                if !assets.outfits.is_empty() {
                    s.local_outfit = Some(s.outfit);
                }
            } else {
                m.page = Page::Main;
                m.selected = MainItem::Wardrobe.index();
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
    replay: Option<Res<crate::shots::Driver>>,
) {
    if m.cooldown > 0. {
        m.cooldown = (m.cooldown - time.delta_secs()).max(0.);
    }
    if (!window.focused
        || keys.any_pressed([
            KeyCode::SuperLeft,
            KeyCode::SuperRight,
            KeyCode::PrintScreen,
        ]))
        && std::env::var_os("PJV_SHOT").is_none()
        && replay.is_none()
    {
        m.open = true;
        *orders = Orders::default();
    }
    if frame.pad && !m.pad {
        m.pad = true;
    } else if m.pad && frame.raw.as_ref().is_some_and(|r| r.0 == 0) {
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
    assets: Res<crate::local_assets::Wardrobe>,
    // A rendered replay captures the scene without the course HUD.
    replay: Option<Res<crate::shots::Driver>>,
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
) {
    **panel = if m.open {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut v in &mut gameplay {
        *v = if m.open || replay.is_some() {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
    }
    let next_header = match m.page {
        Page::Main => "VOYAGE",
        Page::Controls => "CONTROLS",
        Page::Guide => "MOVE GUIDE",
        Page::Wardrobe => "WARDROBE",
    }
    .into();
    if header.0 != next_header {
        header.0 = next_header;
    }
    for mut color in &mut menu_ink {
        color.0 = if m.page == Page::Wardrobe { INK } else { PAPER };
    }
    let outfit = assets.name(s.outfit);
    for (row, mut color, mut node) in &mut rows {
        let index = row.0 + m.offset;
        let display = if index < count(m.page, assets.outfits.len()) {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
        let next = BackgroundColor(if m.selected == index {
            Color::WHITE
        } else {
            Color::NONE
        });
        color.set_if_neq(next);
    }
    for (row, mut text, mut color) in &mut texts {
        let i = row.0 + m.offset;
        color.0 = if i == m.selected || m.page == Page::Wardrobe {
            INK
        } else {
            PAPER
        };
        let next = match m.page {
            Page::Main => MAIN_ITEMS
                .get(i)
                .map(|item| item.label(&m, &s, &outfit))
                .unwrap_or_default(),
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
        if text.0 != next {
            text.0 = next;
        }
    }
    background.0 = if m.page == Page::Wardrobe {
        Color::srgb(0.98, 0.77, 0.02)
    } else {
        Color::srgb(0.93, 0.06, 0.16)
    };
    let next_help = if m.capture {
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
    if help.0 != next_help {
        help.0 = next_help;
    }
}

fn render_hud(
    m: Res<Menu>,
    s: Res<Settings>,
    p: Single<&Player>,
    progress: Res<Progress>,
    mut hud: Single<&mut Text, (With<Hud>, Without<Bottom>)>,
    mut bottom: Single<&mut Text, (With<Bottom>, Without<Hud>)>,
) {
    let next_hud = format!(
        "SUN SPARKS  {:02} / 15     CAP TARGETS  {} / 3     {:02}:{:04.1}\n{}",
        progress.gems.len(),
        progress.targets.len(),
        (progress.time / 60.) as u32,
        progress.time % 60.,
        p.mode.label().to_uppercase(),
    );
    if hud.0 != next_hud {
        hud.0 = next_hud;
    }
    let k = |a: usize| {
        if m.pad {
            label(&s.bindings[a][1], 1)
        } else {
            label(&s.bindings[a][0], 0)
        }
    };
    let next_bottom = if progress.message_time > 0. {
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
    if bottom.0 != next_bottom {
        bottom.0 = next_bottom;
    }
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
fn cursor(
    m: Res<Menu>,
    replay: Option<Res<crate::shots::Driver>>,
    mut window: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    let active =
        !m.open && window.0.focused && std::env::var_os("PJV_SHOT").is_none() && replay.is_none();
    window.1.visible = !active;
    window.1.grab_mode = if active {
        CursorGrabMode::Confined
    } else {
        CursorGrabMode::None
    };
}
#[path = "tests/ui.rs"]
mod checks;

pub fn self_check() {
    checks::run();
}
