//! Drawing the original effects' particles.
//!
//! Particles are not entities. Each frame their quads (and the few small
//! meshes some effects use) are written into one dynamic mesh per material:
//! the crisp alpha-tested smoke (depth-writing), blended effects, and glowing
//! additive ones. Colours stay in HDR so the camera's bloom lets them glow.
//!
//! A small material combines the texture with the particle's two colours the
//! way the original's particle shader does (see `Combiner`): colour 0 and
//! alpha ride in the vertex colour, colour 1 in the tangent attribute.
use crate::effect_data::{Blend, Emitter, Mesh as MeshData};
use crate::effect_sim::Draw;
use bevy::{
    asset::{RenderAssetUsages, uuid_handle},
    camera::visibility::NoFrustumCulling,
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology},
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
    shader::{Shader, ShaderRef},
};
use std::collections::{HashMap, HashSet};

const SHADER: Handle<Shader> = uuid_handle!("6d1c2e7a-47b0-4a53-9c3e-5a0f6d3b9e11");

/// Fragment shader: texture and two colours, then the blend mode's output.
const SHADER_SOURCE: &str = r#"
import bevy_pbr::render::forward_io::VertexOutput;

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var particle_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var particle_sampler: sampler;
// x: 0 cutout, 1 blended, 2 additive; y: colour combiner (2 blends colour 0 to colour 1 by the\n// texture, 1 multiplies the texture by colour 0, anything else uses colour 0 alone).
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var<uniform> settings: vec2<u32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(particle_texture, particle_sampler, in.uv);
    let first = in.color.rgb;
    let second = in.world_tangent.xyz;
    var rgb = first;
    if settings.y == 2u {
        rgb = mix(first, second, texel.r);
    } else if settings.y == 1u {
        rgb = first * texel.rgb;
    }
    let alpha = texel.a * in.color.a;
    if settings.x == 0u {
        if alpha < 0.5 {
            discard;
        }
        return vec4<f32>(rgb, 1.0);
    }
    if settings.x == 2u {
        // Premultiplied blending with zero alpha adds the colour.
        return vec4<f32>(rgb * alpha, 0.0);
    }
    return vec4<f32>(rgb, alpha);
}
"#;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ParticleMaterial {
    #[texture(0)]
    #[sampler(1)]
    texture: Handle<Image>,
    #[uniform(2)]
    settings: UVec2,
}

impl Material for ParticleMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(SHADER)
    }
    fn alpha_mode(&self) -> AlphaMode {
        match self.settings.x {
            0 => AlphaMode::Mask(0.5),
            1 => AlphaMode::Blend,
            _ => AlphaMode::Premultiplied,
        }
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Quads are drawn from both sides.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

pub fn plugin(app: &mut App) {
    app.world_mut()
        .resource_mut::<Assets<Shader>>()
        .insert(
            SHADER.id(),
            Shader::from_wesl(SHADER_SOURCE, "effect_particle.wesl"),
        )
        .expect("the particle shader registers once");
    app.add_plugins(MaterialPlugin::<ParticleMaterial>::default())
        .init_resource::<Painter>();
}

/// Where the camera is and which way it looks, for facing particles.
#[derive(Clone, Copy)]
pub struct View {
    pub position: Vec3,
    pub right: Vec3,
    pub up: Vec3,
}

/// Vertex data of one dynamic mesh.
#[derive(Default)]
pub struct Geometry {
    pub positions: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    /// Second colour, in the tangent attribute.
    pub second: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

/// The colours and texture rectangle one particle is drawn with.
#[derive(Clone, Copy)]
pub struct Paint {
    pub color: [f32; 4],
    pub color1: [f32; 3],
    pub uv: [f32; 4],
}

impl From<&Draw> for Paint {
    fn from(draw: &Draw) -> Self {
        Self {
            color: draw.color,
            color1: draw.color1,
            uv: draw.uv,
        }
    }
}

impl Geometry {
    fn vertex(&mut self, position: Vec3, paint: &Paint, uv: [f32; 2]) -> u32 {
        self.positions.push(position.to_array());
        self.colors.push(paint.color);
        self.second
            .push([paint.color1[0], paint.color1[1], paint.color1[2], 1.]);
        self.uvs.push(uv);
        self.positions.len() as u32 - 1
    }
    /// A quad around `center` spanning `right` and `up` (half extents) over a texture rectangle.
    pub fn quad(&mut self, center: Vec3, right: Vec3, up: Vec3, paint: &Paint) {
        let uv = paint.uv;
        let corners = [
            (center - right + up, [uv[0], uv[1]]),
            (center + right + up, [uv[2], uv[1]]),
            (center + right - up, [uv[2], uv[3]]),
            (center - right - up, [uv[0], uv[3]]),
        ];
        let first = corners.map(|(p, t)| self.vertex(p, paint, t));
        self.indices.extend([0, 1, 2, 0, 2, 3].map(|i| first[i]));
    }
    /// A mesh placed with a rotation and a per-axis scale (metres).
    pub fn mesh(
        &mut self,
        mesh: &MeshData,
        center: Vec3,
        rotation: Quat,
        scale: Vec3,
        paint: &Paint,
    ) {
        let first = self.positions.len() as u32;
        let uv = paint.uv;
        for (i, p) in mesh.position.iter().enumerate() {
            let at = center + rotation * (Vec3::from_array(*p) * scale);
            let t = mesh.uv.get(i).copied().unwrap_or([0., 0.]);
            self.vertex(
                at,
                paint,
                [
                    uv[0] + (uv[2] - uv[0]) * t[0],
                    uv[1] + (uv[3] - uv[1]) * t[1],
                ],
            );
        }
        for t in &mesh.triangles {
            self.indices.extend(t.map(|i| first + i));
        }
    }
}

/// Half-extent axes of a flat particle's quad, by the original's billboard kind:
/// 0, 1, 2, 7 face the camera; 3 lies in the emitter's XY plane; 4 on its XZ plane;
/// 5 and 6 stretch along the direction of travel (kinds inferred from the data).
pub fn quad_axes(draw: &Draw, billboard: u8, view: &View) -> (Vec3, Vec3) {
    let half = draw.size * 0.5;
    let turn = Quat::from_euler(
        EulerRot::XYZ,
        draw.rotation.x,
        draw.rotation.y,
        draw.rotation.z,
    );
    match billboard {
        3 => {
            let q = draw.frame * turn;
            (q * Vec3::X * half.x, q * Vec3::Y * half.y)
        }
        4 => {
            let q = draw.frame * turn;
            (q * Vec3::X * half.x, q * Vec3::Z * half.y)
        }
        5 | 6 => {
            let along = draw.velocity.normalize_or(Vec3::Y);
            let toward = (draw.position - view.position).normalize_or(Vec3::Z);
            let side = along.cross(toward).normalize_or(view.right);
            (along * half.x, side * half.y)
        }
        _ => {
            let (sin, cos) = draw.rotation.z.sin_cos();
            (
                (view.right * cos + view.up * sin) * half.x,
                (view.up * cos - view.right * sin) * half.y,
            )
        }
    }
}

/// Which dynamic mesh a particle is written to.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct BatchKey {
    texture: Option<String>,
    blend: u8,
    combine: u8,
}

struct Batch {
    entity: Entity,
    mesh: Handle<Mesh>,
}

#[derive(Resource, Default)]
pub struct Painter {
    batches: HashMap<BatchKey, Batch>,
    materials: HashMap<BatchKey, Handle<ParticleMaterial>>,
    textures: HashMap<String, Handle<Image>>,
    soft: Option<Handle<Image>>,
}

fn blend_id(blend: Blend) -> u8 {
    match blend {
        Blend::Cutout => 0,
        Blend::Alpha => 1,
        Blend::Add => 2,
    }
}

/// A soft round dot for emitters that have no texture of their own.
fn soft_dot() -> Image {
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let size = 64u32;
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 + 0.5) / size as f32 * 2. - 1.;
            let dy = (y as f32 + 0.5) / size as f32 * 2. - 1.;
            let alpha = (1. - (dx * dx + dy * dy).sqrt()).clamp(0., 1.);
            data.extend([255, 255, 255, (alpha * alpha * 255.) as u8]);
        }
    }
    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

impl Painter {
    /// Write this frame's particles. `installed` lists texture files that exist.
    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &mut self,
        draws: &[(Draw, &Emitter)],
        view: &View,
        meshes_data: &HashMap<String, std::sync::Arc<MeshData>>,
        installed: &HashSet<String>,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<ParticleMaterial>,
        images: &mut Assets<Image>,
        server: &AssetServer,
    ) {
        if self.soft.is_none() {
            self.soft = Some(images.add(soft_dot()));
        }
        let mut groups: HashMap<BatchKey, Vec<usize>> = HashMap::new();
        for (index, (_, emitter)) in draws.iter().enumerate() {
            let slot = emitter.textures.first().and_then(|t| t.as_ref());
            let texture = match slot {
                Some(slot) if installed.contains(&slot.name) => Some(slot.name.clone()),
                // A texture that is not installed would draw as a white square.
                Some(_) => continue,
                None => None,
            };
            if let Some(name) = &texture {
                let handle = self.textures.entry(name.clone()).or_insert_with(|| {
                    server.load::<Image>(format!("local/effects/textures/{name}.png"))
                });
                if images.get(&*handle).is_none() {
                    continue;
                }
            }
            let key = BatchKey {
                texture,
                blend: blend_id(emitter.render.blend),
                combine: emitter.render.combiner.color_process,
            };
            groups.entry(key).or_default().push(index);
        }
        let mut used = HashSet::new();
        for (key, mut members) in groups {
            if key.blend != 0 {
                // Farthest first so blended particles overlap correctly.
                members.sort_by(|a, b| {
                    let da = draws[*a].0.position.distance_squared(view.position);
                    let db = draws[*b].0.position.distance_squared(view.position);
                    db.total_cmp(&da)
                });
            }
            let origin =
                members.iter().map(|i| draws[*i].0.position).sum::<Vec3>() / members.len() as f32;
            let mut geometry = Geometry::default();
            for i in members {
                let (draw, emitter) = &draws[i];
                let center = draw.position - origin;
                let paint = Paint::from(draw);
                let mesh = emitter
                    .render
                    .mesh
                    .as_ref()
                    .and_then(|id| meshes_data.get(id));
                if let Some(mesh) = mesh {
                    let turn = Quat::from_euler(
                        EulerRot::XYZ,
                        draw.rotation.x,
                        draw.rotation.y,
                        draw.rotation.z,
                    );
                    geometry.mesh(mesh, center, draw.frame * turn, draw.size, &paint);
                } else {
                    let (right, up) = quad_axes(draw, emitter.render.billboard, view);
                    geometry.quad(center, right, up, &paint);
                }
            }
            used.insert(key.clone());
            self.write(key, origin, geometry, commands, meshes, materials);
        }
        for (key, batch) in &self.batches {
            if !used.contains(key) {
                commands.entity(batch.entity).insert(Visibility::Hidden);
            }
        }
    }

    fn write(
        &mut self,
        key: BatchKey,
        origin: Vec3,
        geometry: Geometry,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<ParticleMaterial>,
    ) {
        let material = self
            .materials
            .entry(key.clone())
            .or_insert_with(|| {
                let texture = match &key.texture {
                    Some(name) => self.textures.get(name).cloned(),
                    None => self.soft.clone(),
                };
                materials.add(ParticleMaterial {
                    texture: texture.unwrap_or_default(),
                    settings: UVec2::new(key.blend as u32, key.combine as u32),
                })
            })
            .clone();
        let build = |geometry: Geometry| {
            Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, geometry.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, geometry.colors)
            .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, geometry.second)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, geometry.uvs)
            .with_inserted_indices(Indices::U32(geometry.indices))
        };
        if let Some(batch) = self.batches.get(&key) {
            if let Some(mut mesh) = meshes.get_mut(&batch.mesh) {
                *mesh = build(geometry);
            }
            commands
                .entity(batch.entity)
                .insert((Transform::from_translation(origin), Visibility::Inherited));
        } else {
            let handle = meshes.add(build(geometry));
            let entity = commands
                .spawn((
                    Mesh3d(handle.clone()),
                    MeshMaterial3d(material),
                    Transform::from_translation(origin),
                    NoFrustumCulling,
                ))
                .id();
            self.batches.insert(
                key,
                Batch {
                    entity,
                    mesh: handle,
                },
            );
        }
    }
}

pub fn self_check() {
    let view = View {
        position: Vec3::new(0., 0., 10.),
        right: Vec3::X,
        up: Vec3::Y,
    };
    let draw = Draw {
        emitter: 0,
        position: Vec3::ZERO,
        velocity: Vec3::new(0., 2., 0.),
        size: Vec3::new(2., 4., 0.),
        rotation: Vec3::ZERO,
        frame: Quat::IDENTITY,
        color: [1.; 4],
        color1: [1.; 3],
        uv: [0., 0., 1., 1.],
    };
    let (right, up) = quad_axes(&draw, 0, &view);
    assert!((right - Vec3::X).length() < 1e-5 && (up - Vec3::Y * 2.).length() < 1e-5);
    let (along, side) = quad_axes(&draw, 5, &view);
    assert!(
        (along - Vec3::Y).length() < 1e-5,
        "stretched quads run along the velocity"
    );
    assert!(side.dot(Vec3::Y).abs() < 1e-5 && (side.length() - 2.).abs() < 1e-5);
    let paint = Paint::from(&draw);
    let mut geometry = Geometry::default();
    geometry.quad(Vec3::ZERO, Vec3::X, Vec3::Y, &paint);
    assert_eq!((geometry.positions.len(), geometry.indices.len()), (4, 6));
    let mesh = MeshData {
        position: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        uv: vec![[0., 0.], [1., 0.], [0., 1.]],
        triangles: vec![[0, 1, 2]],
    };
    let tile = Paint {
        uv: [0.5, 0.5, 1., 1.],
        ..paint
    };
    geometry.mesh(&mesh, Vec3::ZERO, Quat::IDENTITY, Vec3::splat(2.), &tile);
    assert_eq!(geometry.positions.len(), 7);
    assert_eq!(geometry.indices[6..], [4, 5, 6]);
    assert_eq!(geometry.uvs[5], [1., 0.5], "mesh uvs map into the tile");
    assert_eq!(geometry.second.len(), 7);
}
