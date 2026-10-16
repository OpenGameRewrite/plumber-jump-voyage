//! The game-data build's tone curve.
//!
//! The real game ends its frame with an "S curve" on each colour channel
//! (`alHdrCompose`, tone map type 8 in the Cascade preset): a gentle toe that
//! keeps the image's darker part as lit, then a shoulder that rolls highlights
//! off toward white instead of clipping or desaturating them. Because the curve
//! works on each channel alone, a saturated colour such as the cap's red keeps
//! its hue as it brightens, where Bevy's built-in tonemappers pull it toward
//! grey. This pass applies the same kind of curve before Bevy's own
//! (disabled) tonemapping step.
use bevy::{
    asset::uuid_handle,
    core_pipeline::fullscreen_material::{FullscreenMaterial, FullscreenMaterialPlugin},
    prelude::*,
    render::{RenderApp, extract_component::ExtractComponent, render_resource::ShaderType},
    shader::{Shader, ShaderRef},
};

const SHADER: Handle<Shader> = uuid_handle!("2f0a6c18-5b9e-4d52-8a73-0c1d4e9b7a26");

const SHADER_SOURCE: &str = r#"
import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;

struct Curve {
    // Below `cross.x` the curve is a straight line of slope `cross.y`.
    cross: vec2<f32>,
    // Above it: shoulder.x - shoulder.y / (x + shoulder.z).
    shoulder: vec3<f32>,
}
@group(0) @binding(2) var<uniform> curve: Curve;

fn shape(x: f32) -> f32 {
    if x < curve.cross.x {
        return max(x, 0.0) * curve.cross.y;
    }
    return curve.shoulder.x - curve.shoulder.y / (x + curve.shoulder.z);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let hdr = textureSample(screen_texture, screen_sampler, in.uv);
    return vec4<f32>(shape(hdr.r), shape(hdr.g), shape(hdr.b), hdr.a);
}
"#;

/// The curve of one camera, evaluated per channel.
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Default)]
#[extract_app(RenderApp)]
pub struct ToneCurve {
    /// x: where the toe ends and the shoulder starts; y: slope of the toe.
    cross: Vec2,
    /// A, B and k of the shoulder `A - B / (x + k)`.
    shoulder: Vec3,
}

impl ToneCurve {
    /// A straight toe of the given slope up to `cross`, then a shoulder that
    /// continues smoothly (same value, same slope) and reaches `white_out`
    /// when the input reaches `white_in`.
    pub fn new(cross: f32, slope: f32, white_in: f32, white_out: f32) -> Self {
        let joint = cross * slope;
        // Shoulder A - B/(x+k) with B = slope (cross+k)^2 keeps the slope at the
        // joint; A follows from the joint's value. Find k so the curve hits the
        // white point, by bisection (the output grows with k).
        let reach = |k: f32| {
            let b = slope * (cross + k).powi(2);
            joint + slope * (cross + k) - b / (white_in + k)
        };
        let (mut low, mut high) = (-cross + 1e-4, 1000.);
        for _ in 0..80 {
            let mid = 0.5 * (low + high);
            if reach(mid) < white_out {
                low = mid;
            } else {
                high = mid;
            }
        }
        let k = 0.5 * (low + high);
        let b = slope * (cross + k).powi(2);
        Self {
            cross: Vec2::new(cross, slope),
            shoulder: Vec3::new(joint + slope * (cross + k), b, k),
        }
    }
    /// The curve on one channel, as the shader computes it.
    fn apply(&self, x: f32) -> f32 {
        if x < self.cross.x {
            x.max(0.) * self.cross.y
        } else {
            self.shoulder.x - self.shoulder.y / (x + self.shoulder.z)
        }
    }
}

impl FullscreenMaterial for ToneCurve {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(SHADER)
    }
}

#[cfg_attr(not(feature = "local-assets"), allow(dead_code))]
pub fn plugin(app: &mut App) {
    app.world_mut()
        .resource_mut::<Assets<Shader>>()
        .insert(
            SHADER.id(),
            Shader::from_wesl(SHADER_SOURCE, "tone_curve.wesl"),
        )
        .expect("the tone curve shader registers once");
    app.add_plugins(FullscreenMaterialPlugin::<ToneCurve>::default());
}

pub fn self_check() {
    let curve = ToneCurve::new(0.7, 1., 12., 0.96);
    assert!((curve.apply(0.4) - 0.4).abs() < 1e-6, "the toe is a line");
    assert!(
        (curve.apply(0.7) - 0.7).abs() < 1e-4,
        "the shoulder starts where the toe ends"
    );
    assert!((curve.apply(12.) - 0.96).abs() < 1e-3, "the white point");
    let mut last = 0.;
    for step in 0..200 {
        let y = curve.apply(step as f32 * 0.07);
        assert!(y >= last, "the curve never falls");
        last = y;
    }
}
