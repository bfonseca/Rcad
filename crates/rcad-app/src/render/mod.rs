//! GPU rendering of the solid mesh (foundation A2).
//!
//! This module owns the WGPU pipeline that replaces `egui::Painter` for the
//! solid: vertex buffers with positions/normals/colours, a depth-tested pipeline
//! that matches the framebuffer configured in `NativeOptions`, and a WGSL
//! Blinn-Phong shader. Grid, axes, constraints, sketch and the selection edges
//! stay in egui; only the solid moves here.
//!
//! The pipeline is created lazily inside `CallbackTrait::prepare`, where the
//! device is available. The per-frame data travels in the callback; the
//! persistent GPU resources live in `CallbackResources`.

use std::sync::Arc;

use eframe::egui_wgpu::{self, CallbackResources, CallbackTrait, RenderState};
use eframe::wgpu;
use egui::epaint::PaintCallbackInfo;
use egui::{Color32, Rect};
use glam::Vec3;
use rcad_doc::{Part, ProjectManifest};
use rcad_geom::TriangleMesh;

use crate::camera::OrbitCamera;
use crate::selection::{Selected, Selection};
use crate::viewport::{face_colour, HOVER_COLOUR, SELECTED_COLOUR};

/// Framebuffer configuration shared with `eframe::NativeOptions`.
///
/// The 2D painter does not need depth, but the solid pipeline does. 4x MSAA is
/// the starting point; Gate A compares it with 8x on hardware.
pub const MSAA_SAMPLES: u16 = 4;
pub const DEPTH_BITS: u8 = 24;
pub const STENCIL_BITS: u8 = 8;

const SHADER: &str = include_str!("../../shaders/cad_material.wgsl");
const EDGE_SHADER: &str = include_str!("../../shaders/cad_edges.wgsl");
/// Neutral base for a face the project did not colour.
const DEFAULT_FACE: Color32 = Color32::from_rgb(196, 204, 222);
/// B-Rep edges: a dark outline, the way Creo draws shaded-with-edges.
const EDGE_COLOUR: Color32 = Color32::from_rgb(34, 38, 46);
/// Tangent edges are dimmed instead of outlined.
const TANGENT_EDGE_COLOUR: Color32 = Color32::from_rgb(140, 146, 156);
/// How close to 180° a dihedral angle must be to count as tangent.
const TANGENT_TOLERANCE_DEG: f64 = 2.0;
const LIGHT_DIRECTION: [f32; 3] = [0.4, 0.8, 0.5];

/// One vertex of the solid buffer. Layout must match `VertexBufferLayout` below.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SolidVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
}

impl SolidVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3,
        2 => Float32x3,
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// One vertex of the B-Rep edge line list.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EdgeVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

impl EdgeVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3,
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Uniforms for one frame. 16-byte aligned for WGSL.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SolidUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub eye: [f32; 4],
    pub light: [f32; 4],
}

impl SolidUniforms {
    fn for_camera(camera: &OrbitCamera, rect: Rect) -> Self {
        let aspect = rect.width() / rect.height().max(1.0);
        let eye = camera.eye();
        let light = Vec3::from(LIGHT_DIRECTION).normalize_or_zero();
        Self {
            view_proj: camera.view_proj(aspect).to_cols_array_2d(),
            eye: [eye.x, eye.y, eye.z, 1.0],
            light: [light.x, light.y, light.z, 0.0],
        }
    }
}

/// CPU-side snapshot of one frame, ready to be uploaded by the callback.
struct FrameData {
    vertices: Vec<SolidVertex>,
    indices: Vec<u32>,
    /// Line-list vertices for the B-Rep edges (two per segment).
    edges: Vec<EdgeVertex>,
    uniforms: SolidUniforms,
}

/// The app-side handle. `None` when the renderer is glow (or no WGPU state),
/// in which case the old painter path is used.
#[derive(Clone)]
pub struct SolidRenderer {
    target_format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
    msaa_samples: u32,
}

impl SolidRenderer {
    pub fn new(render_state: Option<&RenderState>) -> Option<Self> {
        let render_state = render_state?;
        Some(Self {
            target_format: render_state.target_format,
            depth_format: egui_wgpu::depth_format_from_bits(DEPTH_BITS, STENCIL_BITS),
            msaa_samples: MSAA_SAMPLES.max(1) as u32,
        })
    }

    /// Add the solid's paint callback at the viewport rect.
    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        mesh: &TriangleMesh,
        camera: &OrbitCamera,
        selection: &Selection,
        hover: Option<&Selected>,
        part: Option<&Part>,
        manifest: &ProjectManifest,
    ) {
        let data = build_frame_data(mesh, camera, rect, selection, hover, part, manifest);
        if data.indices.is_empty() {
            return;
        }
        let callback = SolidCallback {
            vertices: Arc::new(data.vertices),
            indices: Arc::new(data.indices),
            edges: Arc::new(data.edges),
            uniforms: data.uniforms,
            target_format: self.target_format,
            depth_format: self.depth_format,
            msaa_samples: self.msaa_samples,
        };
        painter.add(egui::Shape::Callback(
            egui_wgpu::Callback::new_paint_callback(rect, callback),
        ));
    }
}

/// CPU-side preparation shared by the painter fallback and the GPU path tests.
fn build_frame_data(
    mesh: &TriangleMesh,
    camera: &OrbitCamera,
    rect: Rect,
    selection: &Selection,
    hover: Option<&Selected>,
    part: Option<&Part>,
    manifest: &ProjectManifest,
) -> FrameData {
    let mut vertices = Vec::with_capacity(mesh.indices.len());
    let mut indices: Vec<u32> = Vec::with_capacity(mesh.indices.len());

    for (triangle_index, triangle) in mesh.indices.chunks_exact(3).enumerate() {
        let [i0, i1, i2] = [triangle[0] as usize, triangle[1] as usize, triangle[2] as usize];
        let (Some(p0), Some(p1), Some(p2)) = (
            mesh.positions.get(i0),
            mesh.positions.get(i1),
            mesh.positions.get(i2),
        ) else {
            continue;
        };
        let face_normal = (Vec3::from(*p1) - Vec3::from(*p0))
            .cross(Vec3::from(*p2) - Vec3::from(*p0))
            .normalize_or_zero();
        let color = triangle_colour(mesh, triangle_index, part, manifest, selection, hover);

        for (vertex_index, position) in [i0, i1, i2].into_iter().zip([p0, p1, p2]) {
            let normal = mesh
                .normals
                .get(vertex_index)
                .copied()
                .unwrap_or(face_normal.to_array());
            vertices.push(SolidVertex {
                position: *position,
                normal,
                color: [
                    color.r() as f32 / 255.0,
                    color.g() as f32 / 255.0,
                    color.b() as f32 / 255.0,
                ],
            });
        }
        let base = indices.len() as u32;
        indices.extend([base, base + 1, base + 2]);
    }

    FrameData {
        vertices,
        indices,
        edges: build_edge_vertices(mesh, selection, hover),
        uniforms: SolidUniforms::for_camera(camera, rect),
    }
}

/// Turn the B-Rep edge polylines into a line list.
///
/// Edges are classified by the dihedral angle the kernel already computed:
/// tangent edges (angle through the material ≈ 180°) are dimmed, ordinary
/// ones are dark. Selected/hovered edges take the highlight colours; the
/// painter still draws a thicker line over them for easy targeting.
fn build_edge_vertices(
    mesh: &TriangleMesh,
    selection: &Selection,
    hover: Option<&Selected>,
) -> Vec<EdgeVertex> {
    let hovered = match hover {
        Some(Selected::Edge { reference, .. }) => Some(reference.as_str()),
        _ => None,
    };
    let mut vertices = Vec::new();
    for edge in &mesh.edges {
        if edge.points.len() < 2 {
            continue;
        }
        let selected = selection.has_edge(&edge.name);
        let is_hovered = hovered == Some(edge.name.as_str());
        let color = if selected {
            SELECTED_COLOUR
        } else if is_hovered {
            HOVER_COLOUR
        } else if (edge.dihedral_degrees - 180.0).abs() <= TANGENT_TOLERANCE_DEG {
            TANGENT_EDGE_COLOUR
        } else {
            EDGE_COLOUR
        };
        let rgb = [
            color.r() as f32 / 255.0,
            color.g() as f32 / 255.0,
            color.b() as f32 / 255.0,
        ];
        for pair in edge.points.windows(2) {
            vertices.push(EdgeVertex {
                position: pair[0],
                color: rgb,
            });
            vertices.push(EdgeVertex {
                position: pair[1],
                color: rgb,
            });
        }
        // Closed curves can come back without repeating the first point.
        if edge.closed && edge.points.first() != edge.points.last() {
            if let (Some(first), Some(last)) = (edge.points.first(), edge.points.last()) {
                vertices.push(EdgeVertex {
                    position: *last,
                    color: rgb,
                });
                vertices.push(EdgeVertex {
                    position: *first,
                    color: rgb,
                });
            }
        }
    }
    vertices
}

fn triangle_colour(
    mesh: &TriangleMesh,
    triangle: usize,
    part: Option<&Part>,
    manifest: &ProjectManifest,
    selection: &Selection,
    hover: Option<&Selected>,
) -> Color32 {
    let face_name = mesh.face_name(triangle);
    let mut color = face_colour(face_name, part, manifest)
        .map(|[r, g, b]| Color32::from_rgb(r, g, b))
        .unwrap_or(DEFAULT_FACE);

    if let Some(name) = face_name {
        let hovered = matches!(hover, Some(Selected::Face { reference, .. }) if reference == name);
        if hovered {
            color = blend(color, HOVER_COLOUR, 0.35);
        }
        if selection.has_face(name) {
            color = blend(color, SELECTED_COLOUR, 0.55);
        }
    }
    color
}

fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (x as f32 * (1.0 - t) + y as f32 * t).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// Persistent GPU resources, stored in `CallbackResources`.
struct SolidResources {
    pipeline: wgpu::RenderPipeline,
    edge_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: u64,
    index_buffer: wgpu::Buffer,
    index_capacity: u64,
    index_count: u32,
    edge_buffer: wgpu::Buffer,
    edge_capacity: u64,
    edge_count: u32,
}

impl SolidResources {
    fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        msaa_samples: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rcad solid shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rcad solid uniforms layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rcad solid pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rcad solid pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[SolidVertex::layout()],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // The kernel does not promise a winding; depth testing keeps the
                // visible front faces. A4 can revisit back-face culling.
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
                format,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: msaa_samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        // A4: second pipeline for the B-Rep edges. It shares the camera
        // uniforms; the depth test keeps hidden edges hidden, and a small
        // negative bias pulls the line off the face it bounds (no z-fighting).
        let edge_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rcad edge shader"),
            source: wgpu::ShaderSource::Wgsl(EDGE_SHADER.into()),
        });
        let edge_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rcad edge pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &edge_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[EdgeVertex::layout()],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
                format,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: -2,
                    slope_scale: -1.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState {
                count: msaa_samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &edge_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        let uniform_size = std::mem::size_of::<SolidUniforms>() as wgpu::BufferAddress;
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rcad solid uniforms"),
            size: uniform_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rcad solid uniforms bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            edge_pipeline,
            bind_group,
            uniform_buffer,
            vertex_buffer: empty_vertex_buffer(device),
            vertex_capacity: 4,
            index_buffer: empty_index_buffer(device),
            index_capacity: 4,
            index_count: 0,
            edge_buffer: empty_vertex_buffer(device),
            edge_capacity: 4,
            edge_count: 0,
        }
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[SolidVertex],
        indices: &[u32],
        edges: &[EdgeVertex],
        uniforms: SolidUniforms,
    ) {
        let vertex_bytes: &[u8] = bytemuck::cast_slice(vertices);
        if vertex_bytes.len() as u64 > self.vertex_capacity {
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rcad solid vertices"),
                size: vertex_bytes.len() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.vertex_capacity = vertex_bytes.len() as u64;
        }
        if !vertex_bytes.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, vertex_bytes);
        }

        let index_bytes: &[u8] = bytemuck::cast_slice(indices);
        if index_bytes.len() as u64 > self.index_capacity {
            self.index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rcad solid indices"),
                size: index_bytes.len() as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.index_capacity = index_bytes.len() as u64;
        }
        if !index_bytes.is_empty() {
            queue.write_buffer(&self.index_buffer, 0, index_bytes);
        }

        let edge_bytes: &[u8] = bytemuck::cast_slice(edges);
        if edge_bytes.len() as u64 > self.edge_capacity {
            self.edge_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rcad edge vertices"),
                size: edge_bytes.len() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.edge_capacity = edge_bytes.len() as u64;
        }
        if !edge_bytes.is_empty() {
            queue.write_buffer(&self.edge_buffer, 0, edge_bytes);
        }
        self.edge_count = edges.len() as u32;

        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        self.index_count = indices.len() as u32;
    }
}

fn empty_vertex_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rcad solid vertices (empty)"),
        size: 4,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn empty_index_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rcad solid indices (empty)"),
        size: 4,
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

struct SolidCallback {
    vertices: Arc<Vec<SolidVertex>>,
    indices: Arc<Vec<u32>>,
    edges: Arc<Vec<EdgeVertex>>,
    uniforms: SolidUniforms,
    target_format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
    msaa_samples: u32,
}

impl CallbackTrait for SolidCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if callback_resources.get::<SolidResources>().is_none() {
            callback_resources.insert(SolidResources::new(
                device,
                self.target_format,
                self.depth_format,
                self.msaa_samples,
            ));
        }
        if let Some(resources) = callback_resources.get_mut::<SolidResources>() {
            resources.upload(
                device,
                queue,
                &self.vertices,
                &self.indices,
                &self.edges,
                self.uniforms,
            );
        }
        Vec::new()
    }

    fn paint(
        &self,
        info: PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        let Some(resources) = callback_resources.get::<SolidResources>() else {
            return;
        };
        if resources.index_count == 0 {
            return;
        }

        // The pass viewport is set by egui; the scissor comes from the clip
        // rectangle so the solid cannot paint outside the viewport panel.
        let viewport = info.viewport_in_pixels();
        let clip = info.clip_rect_in_pixels();
        let left = viewport.left_px.max(clip.left_px).max(0);
        let top = viewport.top_px.max(clip.top_px).max(0);
        let right = (viewport.left_px + viewport.width_px).min(clip.left_px + clip.width_px);
        let bottom = (viewport.top_px + viewport.height_px).min(clip.top_px + clip.height_px);
        let width = (right - left).max(0) as u32;
        let height = (bottom - top).max(0) as u32;
        if width == 0 || height == 0 {
            return;
        }
        render_pass.set_scissor_rect(left as u32, top as u32, width, height);
        render_pass.set_pipeline(&resources.pipeline);
        render_pass.set_bind_group(0, &resources.bind_group, &[]);
        render_pass.set_vertex_buffer(0, resources.vertex_buffer.slice(..));
        render_pass.set_index_buffer(resources.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..resources.index_count, 0, 0..1);

        // Pass 2: B-Rep edges, with depth testing so hidden edges stay hidden.
        if resources.edge_count >= 2 {
            render_pass.set_pipeline(&resources.edge_pipeline);
            render_pass.set_bind_group(0, &resources.bind_group, &[]);
            render_pass.set_vertex_buffer(0, resources.edge_buffer.slice(..));
            render_pass.draw(0..resources.edge_count, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcad_geom::{BooleanOp, GeometryBackend, NamedFace, NO_FACE_ID};

    /// The shader cannot be compiled by a GPU in this environment, so validate
    /// its WGSL and uniform/layout types with naga instead.
    fn validate_shader(source: &str) {
        let module = naga::front::wgsl::parse_str(source).expect("shader parses");
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        validator.validate(&module).expect("shader validates");
    }

    #[test]
    fn the_solid_and_edge_shaders_parse_and_validate() {
        validate_shader(SHADER);
        validate_shader(EDGE_SHADER);
    }

    fn triangle_mesh() -> TriangleMesh {
        let mut mesh = TriangleMesh {
            positions: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            indices: vec![0, 1, 2],
            face_ids: vec![1],
            ..Default::default()
        };
        mesh.face_names.insert(
            1,
            NamedFace {
                name: "F:plane:a50.0000".to_string(),
                signature: "F:plane:a50.0000".to_string(),
                label: "plane".to_string(),
            },
        );
        mesh
    }

    fn rect() -> Rect {
        Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(800.0, 600.0))
    }

    fn colours(data: &FrameData) -> Vec<[f32; 3]> {
        data.vertices.iter().map(|vertex| vertex.color).collect()
    }

    #[test]
    fn frame_data_has_one_vertex_per_corner_and_finite_uniforms() {
        let mesh = triangle_mesh();
        let data = build_frame_data(
            &mesh,
            &OrbitCamera::default(),
            rect(),
            &Selection::default(),
            None,
            None,
            &ProjectManifest::default(),
        );

        assert_eq!(data.vertices.len(), 3);
        assert_eq!(data.indices, vec![0, 1, 2]);
        assert!(data
            .uniforms
            .view_proj
            .iter()
            .flatten()
            .all(|value| value.is_finite()));
        assert!(data.uniforms.eye.iter().all(|value| value.is_finite()));
        assert_eq!(data.vertices[0].position, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn a_project_face_colour_reaches_the_vertex_buffer() {
        let mesh = triangle_mesh();
        let mut part = Part::new_solid("P");
        part.set_face_colour("F:plane:a50.0000", "fit zone");
        let mut manifest = ProjectManifest::default();
        manifest.surface_colors.insert("fit zone".to_string(), [0, 128, 255]);

        let data = build_frame_data(
            &mesh,
            &OrbitCamera::default(),
            rect(),
            &Selection::default(),
            None,
            Some(&part),
            &manifest,
        );

        let expected = [0.0, 128.0 / 255.0, 1.0];
        for colour in colours(&data) {
            assert!((colour[0] - expected[0]).abs() < 1e-6, "{colour:?}");
            assert!((colour[1] - expected[1]).abs() < 1e-6, "{colour:?}");
            assert!((colour[2] - expected[2]).abs() < 1e-6, "{colour:?}");
        }
    }

    fn named_edge(name: &str, dihedral_degrees: f64) -> rcad_geom::NamedEdge {
        rcad_geom::NamedEdge {
            name: name.to_string(),
            points: vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]],
            dihedral_degrees,
            ..Default::default()
        }
    }

    #[test]
    fn tangent_edges_are_dimmed_and_selected_edges_take_the_highlight() {
        let mut mesh = triangle_mesh();
        mesh.edges = vec![named_edge("e-normal", 90.0), named_edge("e-tangent", 180.0)];

        let data = build_frame_data(
            &mesh,
            &OrbitCamera::default(),
            rect(),
            &Selection::default(),
            None,
            None,
            &ProjectManifest::default(),
        );
        assert_eq!(data.edges.len(), 4, "two segments, four line-list vertices");
        let normal = data.edges[0].color;
        let tangent = data.edges[2].color;
        assert_ne!(normal, tangent, "the tangent edge must be drawn dimmer");
        assert!(tangent[0] > normal[0], "the tangent colour is lighter");

        let mut selection = Selection::default();
        selection.replace(Selected::Edge {
            reference: "e-normal".to_string(),
            label: "edge".to_string(),
        });
        let selected = build_frame_data(
            &mesh,
            &OrbitCamera::default(),
            rect(),
            &selection,
            None,
            None,
            &ProjectManifest::default(),
        );
        let selected_rgb = selected.edges[0].color;
        let expected = [
            SELECTED_COLOUR.r() as f32 / 255.0,
            SELECTED_COLOUR.g() as f32 / 255.0,
            SELECTED_COLOUR.b() as f32 / 255.0,
        ];
        assert_eq!(selected_rgb, expected);
    }

    #[test]
    fn selection_and_hover_tint_the_face() {
        let mesh = triangle_mesh();
        let plain = build_frame_data(
            &mesh,
            &OrbitCamera::default(),
            rect(),
            &Selection::default(),
            None,
            None,
            &ProjectManifest::default(),
        );
        let selected = Selected::Face {
            reference: "F:plane:a50.0000".to_string(),
            label: "Face".to_string(),
            triangle: 0,
        };
        let mut selection = Selection::default();
        selection.replace(selected.clone());

        let tinted = build_frame_data(
            &mesh,
            &OrbitCamera::default(),
            rect(),
            &selection,
            Some(&selected),
            None,
            &ProjectManifest::default(),
        );

        assert_ne!(colours(&plain), colours(&tinted));
    }

    /// A2 pipeline smoke test. Ignored by default because it needs a GPU or a
    /// software Vulkan/GL adapter; skipped when the machine has none.
    ///
    /// It creates the real pipeline (shader, vertex layout, depth state, MSAA
    /// sample count), uploads a triangle and draws it into an offscreen target,
    /// so wgpu validation runs even without a window.
    #[test]
    #[ignore = "needs a GPU (or software Vulkan); run with --ignored --nocapture"]
    fn headless_pipeline_draws_a_triangle() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapters = instance.enumerate_adapters(wgpu::Backends::all());
        eprintln!("wgpu adapters: {}", adapters.len());
        let Some(adapter) = adapters.into_iter().next() else {
            eprintln!("no wgpu adapter available; skipping the pipeline smoke test");
            return;
        };
        let info = adapter.get_info();
        eprintln!("using adapter: {:?} {}", info.backend, info.name);

        let device_descriptor = wgpu::DeviceDescriptor {
            label: Some("rcad headless test device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&device_descriptor))
            .expect("headless device");

        let mut resources = SolidResources::new(
            &device,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            Some(wgpu::TextureFormat::Depth24PlusStencil8),
            1,
        );
        let vertices = [
            SolidVertex {
                position: [0.0, 0.5, 0.0],
                normal: [0.0, 0.0, 1.0],
                color: [1.0, 0.0, 0.0],
            },
            SolidVertex {
                position: [-0.5, -0.5, 0.0],
                normal: [0.0, 0.0, 1.0],
                color: [0.0, 1.0, 0.0],
            },
            SolidVertex {
                position: [0.5, -0.5, 0.0],
                normal: [0.0, 0.0, 1.0],
                color: [0.0, 0.0, 1.0],
            },
        ];
        resources.upload(
            &device,
            &queue,
            &vertices,
            &[0, 1, 2],
            &[],
            SolidUniforms {
                view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
                eye: [0.0, 0.0, 1.0, 1.0],
                light: [0.0, 0.0, 1.0, 0.0],
            },
        );

        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("headless color"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("headless depth"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("headless encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("headless pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&resources.pipeline);
            pass.set_bind_group(0, &resources.bind_group, &[]);
            pass.set_vertex_buffer(0, resources.vertex_buffer.slice(..));
            pass.set_index_buffer(resources.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..resources.index_count, 0, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        // A validation error would surface here (wgpu treats them as fatal by
        // default); a successful submission is the smoke test.
    }

    /// The real app configuration is 4x MSAA plus a resolve target. Validate
    /// that the pipeline's sample count, depth attachment and render pass agree;
    /// a mismatch here is exactly what would fail only in the window.
    #[test]
    #[ignore = "needs a GPU (or software Vulkan/GL); run with --ignored --nocapture"]
    fn headless_pipeline_accepts_the_apps_msaa_configuration() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let Some(adapter) = instance
            .enumerate_adapters(wgpu::Backends::all())
            .into_iter()
            .next()
        else {
            eprintln!("no wgpu adapter available; skipping the MSAA smoke test");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rcad headless MSAA device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        }))
        .expect("headless device");
        let (width, height) = (128u32, 128u32);
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let msaa = 4;
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("msaa color"),
            size,
            mip_level_count: 1,
            sample_count: msaa,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let resolve = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("msaa resolve"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("msaa depth"),
            size,
            mip_level_count: 1,
            sample_count: msaa,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let resolve_view = resolve.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

        let mut resources = SolidResources::new(
            &device,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            Some(wgpu::TextureFormat::Depth24PlusStencil8),
            msaa,
        );
        let vertices = [
            SolidVertex {
                position: [0.0, 0.5, 0.0],
                normal: [0.0, 0.0, 1.0],
                color: [1.0, 0.0, 0.0],
            },
            SolidVertex {
                position: [-0.5, -0.5, 0.0],
                normal: [0.0, 0.0, 1.0],
                color: [0.0, 1.0, 0.0],
            },
            SolidVertex {
                position: [0.5, -0.5, 0.0],
                normal: [0.0, 0.0, 1.0],
                color: [0.0, 0.0, 1.0],
            },
        ];
        resources.upload(
            &device,
            &queue,
            &vertices,
            &[0, 1, 2],
            &[],
            SolidUniforms {
                view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
                eye: [0.0, 0.0, 1.0, 1.0],
                light: [0.0, 0.0, 1.0, 0.0],
            },
        );

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("msaa encoder"),
        });
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("msaa pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &color_view,
                        resolve_target: Some(&resolve_view),
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Discard,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime();
            pass.set_pipeline(&resources.pipeline);
            pass.set_bind_group(0, &resources.bind_group, &[]);
            pass.set_vertex_buffer(0, resources.vertex_buffer.slice(..));
            pass.set_index_buffer(resources.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..resources.index_count, 0, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        // No readback needed: creating the pipeline at sample_count=4 and drawing
        // into a 4x MSAA attachment with a resolve target is the validation.
    }

    /// Render the real demo mesh with the A2 pipeline into an offscreen
    /// texture, read it back and save a PNG for inspection. Ignored by default:
    /// needs a GPU (llvmpipe works). This is the closest thing to a visual A2
    /// check that can run without a window.
    #[test]
    #[ignore = "needs a GPU (or software Vulkan/GL); run with --ignored --nocapture"]
    fn headless_render_of_the_demo_writes_a_png() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapters = instance.enumerate_adapters(wgpu::Backends::all());
        let Some(adapter) = adapters.into_iter().next() else {
            eprintln!("no wgpu adapter available; skipping the offscreen render");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rcad headless render device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        }))
        .expect("headless device");

        // Real kernel mesh: the demo box with a cylindrical hole.
        let mut backend = rcad_occt::OcctBackend::new();
        let base = backend.box_solid(glam::DVec3::new(40.0, 30.0, 8.0)).unwrap();
        let tool = backend.cylinder(5.0, 20.0).unwrap();
        let shape = backend.boolean(base, tool, BooleanOp::Subtract).unwrap();
        let mesh = backend.tessellate(shape).unwrap();

        let mut camera = OrbitCamera::default();
        if let Some((min, max)) = mesh.bounds() {
            camera.fit_to_bounds(min, max);
            camera.look_from(Vec3::new(1.0, 1.0, 0.8));
        }
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(800.0, 600.0));
        let data = build_frame_data(
            &mesh,
            &camera,
            rect,
            &Selection::default(),
            None,
            None,
            &ProjectManifest::default(),
        );

        let (width, height) = (800u32, 600u32);
        let mut callback_resources = egui_wgpu::CallbackResources::default();
        let callback = SolidCallback {
            vertices: Arc::new(data.vertices),
            indices: Arc::new(data.indices),
            edges: Arc::new(data.edges),
            uniforms: data.uniforms,
            target_format: wgpu::TextureFormat::Rgba8UnormSrgb,
            depth_format: Some(wgpu::TextureFormat::Depth24PlusStencil8),
            msaa_samples: 1,
        };
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: 1.0,
        };

        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen color"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        let bytes_per_pixel = 4u32;
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(256) * 256;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("offscreen readback"),
            size: (padded_bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("offscreen encoder"),
        });
        // Run the real callback path: `prepare` creates/updates the resources and
        // `paint` issues the draw into the pass.
        let _ = callback.prepare(
            &device,
            &queue,
            &screen,
            &mut encoder,
            &mut callback_resources,
        );
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("offscreen pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.11,
                            g: 0.12,
                            b: 0.13,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            })
                .forget_lifetime();
            let info = PaintCallbackInfo {
                viewport: rect,
                clip_rect: rect,
                pixels_per_point: 1.0,
                screen_size_px: [width, height],
            };
            callback.paint(info, &mut pass, &callback_resources);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        device
            .poll(wgpu::PollType::Wait)
            .expect("poll the headless device");
        receiver.recv().expect("map callback").expect("map buffer");

        let mapped = slice.get_mapped_range();
        let mut image = image::RgbaImage::new(width, height);
        for y in 0..height {
            let row = &mapped[(y * padded_bytes_per_row) as usize
                ..(y * padded_bytes_per_row + unpadded_bytes_per_row) as usize];
            for x in 0..width {
                let p = (x * bytes_per_pixel) as usize;
                image.put_pixel(
                    x,
                    y,
                    image::Rgba([row[p], row[p + 1], row[p + 2], row[p + 3]]),
                );
            }
        }
        drop(mapped);
        readback.unmap();

        let corner = *image.get_pixel(0, 0);
        let centre = *image.get_pixel(width / 2, height / 2);
        assert_ne!(centre, corner, "the solid did not paint over the background");
        let non_background = image
            .pixels()
            .filter(|pixel| {
                let d = |a: u8, b: u8| a.abs_diff(b) as u32;
                d(pixel.0[0], corner.0[0]) + d(pixel.0[1], corner.0[1]) + d(pixel.0[2], corner.0[2]) > 12
            })
            .count();
        assert!(
            non_background > 1_000,
            "the offscreen render is nearly empty ({non_background} non-background pixels)"
        );
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/a2-headless-demo.png");
        image.save(&path).expect("write the offscreen PNG");
        eprintln!("offscreen render: {} ({non_background} non-background pixels)", path.display());
    }

    #[test]
    fn a_triangle_without_a_face_name_keeps_the_default_colour() {
        let mut mesh = triangle_mesh();
        let data = build_frame_data(
            &mesh,
            &OrbitCamera::default(),
            rect(),
            &Selection::default(),
            None,
            None,
            &ProjectManifest::default(),
        );
        // Same mesh, but with the face id sentinel: no name can be resolved.
        mesh.face_ids = vec![NO_FACE_ID];
        let unnamed = build_frame_data(
            &mesh,
            &OrbitCamera::default(),
            rect(),
            &Selection::default(),
            None,
            None,
            &ProjectManifest::default(),
        );
        assert_eq!(colours(&data), colours(&unnamed));
    }
}
