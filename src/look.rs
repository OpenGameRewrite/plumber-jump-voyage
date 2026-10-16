//! How the scene is lit and exposed.
//!
//! The ROMless build keeps its own sun and sky. The game-data build follows
//! the real game's daylight setup for the Cascade Kingdom (the stage the
//! visual comparison runs in): the sun's direction and colour, the
//! light rig the game uses for the player, and a tone curve with a soft
//! shoulder. Numbers are taken from the game's graphics preset for that stage
//! ("plains blue sky") where it states them and fitted against the game's own
//! screenshots where it does not; each constant says which.
use crate::tone::ToneCurve;
use bevy::prelude::*;

/// Render layer the sun sees and no camera draws: sprites' shadow casters.
pub const SPRITE_SHADOW_LAYER: usize = 2;

/// Direction a light comes from, as the game's graphics files give it:
/// a longitude around the vertical axis (0 toward +Z, 90 toward +X) and a
/// latitude above the horizon, both in degrees.
fn from_angles(longitude: f32, latitude: f32) -> Vec3 {
    let (lon, lat) = (longitude.to_radians(), latitude.to_radians());
    Vec3::new(lon.sin() * lat.cos(), lat.sin(), lon.cos() * lat.cos())
}

/// One lobe of the game's light rig for the player: a light shining from
/// `from` with a colour, an intensity and the half-angle (degrees) it spreads
/// over.
struct Lobe {
    longitude: f32,
    latitude: f32,
    color: [f32; 3],
    intensity: f32,
    spread: f32,
}
/// The game's light rig for the player in the "plains blue sky" preset: a
/// bluish fill from the camera's side, a green-tinted bounce from the ground,
/// a pale sky from above and a few white lights around Mario. Narrow lobes
/// (small spread) only show in reflections on rough cloth.
const PLAYER_LOBES: [Lobe; 8] = [
    Lobe {
        longitude: 180.,
        latitude: 0.,
        color: [0.548, 0.711, 1.],
        intensity: 22.1,
        spread: 45.,
    },
    Lobe {
        longitude: -0.72,
        latitude: 23.04,
        color: [0.598, 0.831, 1.],
        intensity: 5.2,
        spread: 6.12,
    },
    Lobe {
        longitude: 0.,
        latitude: -90.36,
        color: [0.365, 0.878, 0.706],
        intensity: 13.8,
        spread: 45.,
    },
    Lobe {
        longitude: 0.,
        latitude: 91.8,
        color: [0.485, 0.632, 1.],
        intensity: 18.1,
        spread: 8.37,
    },
    Lobe {
        longitude: -37.8,
        latitude: -19.44,
        color: [1., 1., 1.],
        intensity: 19.3,
        spread: 8.28,
    },
    Lobe {
        longitude: -140.76,
        latitude: 0.,
        color: [1., 1., 1.],
        intensity: 11.,
        spread: 6.03,
    },
    Lobe {
        longitude: 144.,
        latitude: 0.,
        color: [1., 1., 1.],
        intensity: 7.8,
        spread: 10.44,
    },
    Lobe {
        longitude: 35.64,
        latitude: 23.76,
        color: [1., 1., 1.],
        intensity: 15.,
        spread: 45.,
    },
];

/// Everything `world::setup` needs to light and expose a scene.
pub struct Rig {
    /// Where the sun stands (direction toward it).
    pub sun_from: Vec3,
    pub sun_color: Color,
    pub sun_lux: f32,
    pub ambient: Color,
    pub ambient_brightness: f32,
    /// Cubemap the scene's diffuse and reflected light come from.
    /// `None` lights the scene with the sky itself.
    pub environment: Option<Image>,
    pub environment_intensity: f32,
    pub skybox_brightness: f32,
    pub tonemapping: bevy::core_pipeline::tonemapping::Tonemapping,
    pub bloom: f32,
    /// The game's tone curve, where the build uses it.
    pub tone: Option<ToneCurve>,
    /// Shadows with a hard edge: the game's are not blurred.
    pub hard_shadows: bool,
}

/// The light rig as a radiance cubemap: each lobe is brightest in its own
/// direction and fades to nothing at its spread, linearly in the cosine.
fn player_rig_cubemap(size: u32) -> Image {
    use bevy::render::render_resource::{
        Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
    };
    let lobes: Vec<(Vec3, Vec3, f32, f32)> = PLAYER_LOBES
        .iter()
        .map(|l| {
            (
                from_angles(l.longitude, l.latitude),
                Vec3::from_array(l.color),
                l.intensity,
                l.spread.to_radians().cos(),
            )
        })
        .collect();
    let mut data = Vec::with_capacity((size * size * 6 * 4) as usize);
    // Radiance of 1.0 in the texture is this many units of lobe intensity.
    let scale = 1. / 40.;
    for face in 0..6 {
        for y in 0..size {
            for x in 0..size {
                let u = (x as f32 + 0.5) / size as f32 * 2. - 1.;
                let v = (y as f32 + 0.5) / size as f32 * 2. - 1.;
                let d = match face {
                    0 => Vec3::new(1., -v, -u),
                    1 => Vec3::new(-1., -v, u),
                    2 => Vec3::new(u, 1., v),
                    3 => Vec3::new(u, -1., -v),
                    4 => Vec3::new(u, -v, 1.),
                    _ => Vec3::new(-u, -v, -1.),
                }
                .normalize();
                let mut c = Vec3::ZERO;
                for (from, color, intensity, edge) in &lobes {
                    let k = ((d.dot(*from) - edge) / (1. - edge)).max(0.);
                    c += *color * (*intensity * k);
                }
                c *= scale;
                for k in 0..3 {
                    data.push((c[k].clamp(0., 1.).powf(1. / 2.2) * 255.) as u8);
                }
                data.push(255);
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image
}

/// The Cascade sun, as the graphics preset gives it: longitude 6.84 and
/// latitude 70 degrees (a little toward +Z, behind a Mario the camera follows).
fn game_sun_from() -> Vec3 {
    from_angles(0.119_381_f32.to_degrees(), 1.221_73_f32.to_degrees())
}
/// The preset's sun colour (302, 286, 274 in the game's units) as a ratio.
const GAME_SUN_COLOR: Color = Color::linear_rgb(1., 0.947, 0.907);
/// Lux that give the sun the game's strength on a white surface once the
/// curve is applied (fitted on the cap's red: the preset's own number, 302
/// times its exposure of 0.06, is a similar gain).
const GAME_SUN_LUX: f32 = 17_000.;

/// The game-data build's look, for a replay or for play.
pub fn game() -> Rig {
    use bevy::core_pipeline::tonemapping::Tonemapping;
    Rig {
        sun_from: game_sun_from(),
        sun_color: GAME_SUN_COLOR,
        sun_lux: GAME_SUN_LUX,
        ambient: Color::WHITE,
        ambient_brightness: 0.,
        environment: Some(player_rig_cubemap(64)),
        environment_intensity: 17_000.,
        skybox_brightness: 1800.,
        tonemapping: Tonemapping::None,
        bloom: 0.,
        hard_shadows: true,
        tone: Some(ToneCurve::new(0.7, 1., 12., 0.96)),
    }
}

/// The ROMless build's look: its own sun and the gradient sky. In a rendered
/// replay the sun is nearly overhead (about 80 degrees) and a little on the
/// camera's side, as the reference pictures' evenly lit back suggested.
pub fn romless(replay: bool) -> Rig {
    Rig {
        sun_from: if replay {
            Vec3::new(1., 20., -3.)
        } else {
            Vec3::new(10., 18., 8.)
        },
        sun_color: Color::srgb(1., 0.97, 0.9),
        sun_lux: if replay { 20000. } else { 14000. },
        ambient: Color::srgb(0.78, 0.88, 1.),
        ambient_brightness: if replay { 150. } else { 250. },
        environment: None,
        environment_intensity: if replay { 600. } else { 1400. },
        skybox_brightness: 1800.,
        tonemapping: bevy::core_pipeline::tonemapping::Tonemapping::TonyMcMapface,
        bloom: 0.12,
        tone: None,
        hard_shadows: false,
    }
}

/// Plain ground of a rendered replay (sRGB): the colour of the reference
/// plateau's soil as it shows in the game's pictures once this scene's own
/// lighting and tone curve are applied.
pub fn replay_soil() -> [f32; 3] {
    if cfg!(feature = "local-assets") {
        [0.38, 0.34, 0.24]
    } else {
        [0.70, 0.55, 0.22]
    }
}

/// The game lights its ground from a brighter, warmer sky than it lights Mario
/// with, so the ground's shadow is much lighter than Mario's own ambient light
/// would leave it. The replay's ground gets the difference as a glow.
pub fn replay_soil_glow() -> LinearRgba {
    if cfg!(feature = "local-assets") {
        LinearRgba::rgb(0.17, 0.09, 0.027)
    } else {
        LinearRgba::BLACK
    }
}

/// How much the course's own colours are dimmed under the game's daylight,
/// which is about twice as strong as the ROMless sun (1 keeps them as drawn).
pub fn course_albedo() -> f32 {
    if cfg!(feature = "local-assets") {
        0.45
    } else {
        1.
    }
}

/// Light for the original's lit particles (smoke), as the pre-curve radiance
/// a white surface would show: the sun, and the player rig's sky as an
/// "ambient cube" (what a surface facing +X, -X, +Y, -Y, +Z, -Z receives).
#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
pub struct ParticleLight {
    pub to_sun: Vec3,
    pub sun: Vec3,
    pub ambient: [Vec3; 6],
}

#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
pub fn particle_light() -> ParticleLight {
    let sun_from = game_sun_from().normalize();
    // Directional light of E lux gives E / pi / 1000 at the default exposure.
    let sun_gain = GAME_SUN_LUX / std::f32::consts::PI / 1000.;
    let sun = Vec3::from_array(GAME_SUN_COLOR.to_linear().to_f32_array_no_alpha()) * sun_gain;
    // Cosine-weighted sum of the lobes over the sphere (Fibonacci points).
    let lobes: Vec<(Vec3, Vec3, f32, f32)> = PLAYER_LOBES
        .iter()
        .map(|l| {
            (
                from_angles(l.longitude, l.latitude),
                Vec3::from_array(l.color),
                l.intensity,
                l.spread.to_radians().cos(),
            )
        })
        .collect();
    const POINTS: usize = 4000;
    let radiance = |d: Vec3| {
        lobes
            .iter()
            .map(|(from, color, intensity, edge)| {
                *color * (*intensity * ((d.dot(*from) - edge) / (1. - edge)).max(0.))
            })
            .sum::<Vec3>()
    };
    let samples: Vec<(Vec3, Vec3)> = (0..POINTS)
        .map(|i| {
            let y = 1. - 2. * (i as f32 + 0.5) / POINTS as f32;
            let ring = (1. - y * y).sqrt();
            let angle = i as f32 * 2.399_963_2;
            let d = Vec3::new(ring * angle.cos(), y, ring * angle.sin());
            (d, radiance(d))
        })
        .collect();
    let irradiance = |n: Vec3| {
        samples
            .iter()
            .map(|(d, l)| *l * d.dot(n).max(0.))
            .sum::<Vec3>()
            * (4. / POINTS as f32)
    };
    let axes = [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
    ];
    let raw = axes.map(irradiance);
    // Scale so a surface facing the camera's side (the strong fill lobe)
    // receives what the fit on Mario's gloves shows: about 1.45 of green.
    let scale = 1.45 / raw[5].y.max(1e-3);
    ParticleLight {
        to_sun: sun_from,
        sun,
        ambient: raw.map(|c| c * scale),
    }
}
