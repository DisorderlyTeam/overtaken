/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::assets::AssetHandle;
use crate::assets::texture::Texture;
use crate::context::RenderCtx;
use crate::core::color::Color;
use crate::render_world::{Camera2d, LightSource, RenderWorld, SceneRenderData, SpriteSpace};
use crate::renderer::postprocessing::PostProcess;
use crate::scene::{BlendMode, Material};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, UVec2, Vec2, Vec3, Vec3Swizzles, Vec4};
use std::cell::{Ref, RefCell};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::ops::{Not, Range};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::{RwLock, mpsc};
use tracing::{
    Span, debug, debug_span, error, info, instrument, record_all, trace, trace_span, warn,
};
use wgpu::util::DeviceExt;

pub mod postprocessing;

pub struct WgpuContext {
    pub main_sampler: wgpu::Sampler,
    pub linear_sampler: wgpu::Sampler,
    pub opaque_pipeline: wgpu::RenderPipeline,
    pub alpha_pipeline: wgpu::RenderPipeline,
    pub debug_pipeline: wgpu::RenderPipeline,
    pub shadow_mask_pipeline: wgpu::RenderPipeline,
    pub light_pipeline: wgpu::RenderPipeline,
    pub fullscreen_pipeline: wgpu::RenderPipeline,
    pub queue: wgpu::Queue,
    pub device: wgpu::Device,
    pub surface: wgpu::Surface<'static>,
    pub config: RefCell<wgpu::SurfaceConfiguration>,
    pub is_surface_configured: AtomicBool,
    pub gpu_timestamps: Option<GpuTimestampContext>,
    pub virtual_resolution: Vec2,

    pub(crate) font_system: Arc<RwLock<glyphon::FontSystem>>,
    swash_cache: RefCell<glyphon::SwashCache>,
    text_viewport: glyphon::Viewport,
    atlas: RefCell<glyphon::TextAtlas>,
    text_renderer: RefCell<glyphon::TextRenderer>,

    transparent_tex: wgpu::TextureView,
    black_tex: wgpu::TextureView,
    white_tex: wgpu::TextureView,

    // Window is last so that it drops AFTER wgpu surface.
    // This avoids SIGSEGV when the program closes.
    // Probably a bug in wgpu.
    pub window: Arc<winit::window::Window>,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct FrameUniform {
    world_to_clip: Mat4,
    render_resolution: Vec2,
    _pad: Vec2,
}

impl FrameUniform {
    fn new(view_proj: Mat4, render_resolution: UVec2) -> Self {
        Self {
            world_to_clip: view_proj,
            render_resolution: render_resolution.as_vec2(),
            _pad: Vec2::ZERO,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct SpriteGpuInstance {
    model: Mat4,
    uv_min: Vec2,
    uv_max: Vec2,
}

const SPRITE_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    0 => Float32x4,
    1 => Float32x4,
    2 => Float32x4,
    3 => Float32x4,
    4 => Float32x2,
    5 => Float32x2
];

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct LightUniform {
    color: Vec3,
    intensity: f32,
    center: Vec2,
    radius: f32,
    _pad: f32,
    direction_and_cos_half_angle: Vec4,
}

impl From<LightSource> for LightUniform {
    fn from(light: LightSource) -> Self {
        Self {
            color: light.color.to_vec3(),
            intensity: light.intensity,
            center: light.position,
            radius: light.radius,
            _pad: 0.0,
            direction_and_cos_half_angle: Vec4::new(
                light.direction.x,
                light.direction.y,
                (light.angle * 0.5).cos(),
                light.color.a(),
            ),
        }
    }
}

impl From<&LightSource> for LightUniform {
    fn from(light: &LightSource) -> Self {
        Self {
            color: light.color.to_vec3(),
            intensity: light.intensity,
            center: light.position,
            radius: light.radius,
            _pad: 0.0,
            direction_and_cos_half_angle: Vec4::new(
                light.direction.x,
                light.direction.y,
                (light.angle * 0.5).cos(),
                light.color.a(),
            ),
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct DebugVertex {
    position: Vec4,
    color: Vec4,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct DebugUniform {
    view_proj: Mat4,
}

const DEBUG_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4];

pub struct RendererTargets {
    pub width: u32,
    pub height: u32,
    /// Last drawn to texture.
    pub drawn_texture: wgpu::Texture,
    pub drawn_texture_view: wgpu::TextureView,
    /// Texture that might be swapped with [`drawn_texture`] for postprocessing purposes.
    pub next_texture: wgpu::Texture,
    pub next_texture_view: wgpu::TextureView,
    /// Depth texture.
    pub depth_texture: wgpu::Texture,
    pub depth_texture_view: wgpu::TextureView,
    /// Texture with a mask for a specific light.
    pub shadow_mask_texture: wgpu::Texture,
    pub shadow_mask_texture_view: wgpu::TextureView,
    /// Final texture containing lighting colors.
    pub lighting_textures: Vec<wgpu::Texture>,
    pub lighting_texture_views: Vec<wgpu::TextureView>,
}

impl RendererTargets {
    pub fn swap(&mut self) {
        std::mem::swap(&mut self.drawn_texture, &mut self.next_texture);
        std::mem::swap(&mut self.drawn_texture_view, &mut self.next_texture_view);
    }
}

const GPU_TIMESTAMP_QUERY_COUNT: u32 = 3;
const GPU_QUERY_SCENE_START: u32 = 0;
const GPU_QUERY_SCENE_END: u32 = 1;
const GPU_QUERY_FRAME_START: u32 = 0;
const GPU_QUERY_FRAME_END: u32 = 2;
pub const BASE_VIRTUAL_WIDTH: u32 = 320;
pub const BASE_VIRTUAL_HEIGHT: u32 = 240;

#[derive(Copy, Clone, Debug)]
struct PresentationInfo {
    render_resolution: UVec2,
    output_viewport: (UVec2, UVec2),
}

pub struct GpuTimestampContext {
    pub query_set: wgpu::QuerySet,
    pub resolve_buffer: wgpu::Buffer,
    pub readback_buffer: wgpu::Buffer,
    pub timestamp_period_ns: f32,
}

pub struct Renderer {
    targets: Option<RendererTargets>,
    pub(crate) context: Option<WgpuContext>,
    light_reuse_buffers: RefCell<LightReuseBuffers>,
    post_processes: RefCell<Vec<Box<dyn PostProcess>>>,
}

#[derive(Default)]
struct LightReuseBuffers {
    shadow_quad_buffer: Option<wgpu::Buffer>,
    shadow_quad_capacity_quads: usize,
    light_triangle_buffer: Option<wgpu::Buffer>,
    light_triangle_capacity_lights: usize,
    light_uniform_buffer: Option<wgpu::Buffer>,
    light_uniform_capacity_lights: usize,
    shadow_quads_scratch: Vec<[Vec2; 4]>,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("Surface error: {0}")]
    Surface(#[from] wgpu::SurfaceError),
    #[error("WGPU error: {0}")]
    Wgpu(#[from] wgpu::Error),
}

#[derive(Debug, Default, Copy, Clone)]
pub struct RenderStats {
    pub draw_calls: u32,
    pub render_passes: u32,
    pub buffers_created: u32,
    pub bind_groups_created: u32,
    pub texture_views_created: u32,
    pub world_sprite_instances_drawn: u32,
    pub world_sprite_batches_drawn: u32,
    pub screen_sprite_instances_drawn: u32,
    pub screen_sprite_batches_drawn: u32,
    pub gpu_scene_ms: Option<f64>,
    pub gpu_frame_ms: Option<f64>,
}

struct RenderFrame<'a> {
    ctx: &'a WgpuContext,
    stats: &'a mut RenderStats,
}

struct SceneRenderPass<'a> {
    render_resolution: UVec2,
    scene_data: &'a SceneRenderData,
    targets: &'a RendererTargets,
}

impl<'a> RenderFrame<'a> {
    fn new(ctx: &'a WgpuContext, stats: &'a mut RenderStats) -> Self {
        Self { ctx, stats }
    }

    fn create_buffer_init(&mut self, desc: &wgpu::util::BufferInitDescriptor<'_>) -> wgpu::Buffer {
        self.stats.buffers_created += 1;
        self.ctx.device.create_buffer_init(desc)
    }

    fn create_bind_group(&mut self, desc: &wgpu::BindGroupDescriptor<'_>) -> wgpu::BindGroup {
        self.stats.bind_groups_created += 1;
        self.ctx.device.create_bind_group(desc)
    }

    fn create_buffer(&mut self, desc: &wgpu::BufferDescriptor<'_>) -> wgpu::Buffer {
        self.stats.buffers_created += 1;
        self.ctx.device.create_buffer(desc)
    }

    fn create_texture_view(&mut self, texture: &wgpu::Texture) -> wgpu::TextureView {
        self.stats.texture_views_created += 1;
        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    fn begin_render_pass<'encoder>(
        &mut self,
        encoder: &'encoder mut wgpu::CommandEncoder,
        descriptor: &wgpu::RenderPassDescriptor<'encoder>,
    ) -> wgpu::RenderPass<'encoder> {
        self.stats.render_passes += 1;
        encoder.begin_render_pass(descriptor)
    }

    fn draw(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        vertices: Range<u32>,
        instances: Range<u32>,
    ) {
        render_pass.draw(vertices, instances);
        self.stats.draw_calls += 1;
    }
}

impl Renderer {
    fn ensure_light_reuse_buffers(
        &self,
        frame: &mut RenderFrame<'_>,
        light_count: usize,
        shadow_segment_count: usize,
        uniform_stride_bytes: u64,
    ) {
        let mut light_reuse_buffers = self.light_reuse_buffers.borrow_mut();
        let required_shadow_quads = light_count.saturating_mul(shadow_segment_count).max(1);
        if light_reuse_buffers.shadow_quad_capacity_quads < required_shadow_quads {
            light_reuse_buffers.shadow_quad_buffer =
                Some(frame.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Reusable light shadow quad buffer"),
                    size: (required_shadow_quads as u64) * (size_of::<[Vec2; 4]>() as u64),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
            light_reuse_buffers.shadow_quad_capacity_quads = required_shadow_quads;
        }

        let required_light_triangles = light_count.max(1);
        if light_reuse_buffers.light_triangle_capacity_lights < required_light_triangles {
            light_reuse_buffers.light_triangle_buffer =
                Some(frame.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Reusable light triangle buffer"),
                    size: (required_light_triangles as u64) * 3 * (size_of::<Vec2>() as u64),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
            light_reuse_buffers.light_triangle_capacity_lights = required_light_triangles;
        }

        let required_uniform_lights = light_count.max(1);
        if light_reuse_buffers.light_uniform_capacity_lights < required_uniform_lights {
            light_reuse_buffers.light_uniform_buffer =
                Some(frame.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Reusable light uniform buffer"),
                    size: (required_uniform_lights as u64) * uniform_stride_bytes,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
            light_reuse_buffers.light_uniform_capacity_lights = required_uniform_lights;
        }

        let required_scratch = shadow_segment_count.max(1);
        if light_reuse_buffers.shadow_quads_scratch.capacity() < required_scratch {
            let current_capacity = light_reuse_buffers.shadow_quads_scratch.capacity();
            light_reuse_buffers
                .shadow_quads_scratch
                .reserve(required_scratch - current_capacity);
        }
    }

    fn snapped_world_camera(camera: Camera2d) -> Camera2d {
        Camera2d {
            center: camera.center.round(),
            ..camera
        }
    }

    fn snapped_world_transform(transform: Mat4) -> Mat4 {
        let mut cols = transform.to_cols_array_2d();
        cols[3][0] = cols[3][0].round();
        cols[3][1] = cols[3][1].round();
        Mat4::from_cols_array_2d(&cols)
    }

    pub fn new() -> Renderer {
        Renderer {
            context: None,
            targets: None,
            light_reuse_buffers: RefCell::new(LightReuseBuffers::default()),
            post_processes: RefCell::new(Vec::new()),
        }
    }

    pub fn add_post_process(&mut self, post_process: Box<dyn PostProcess>) {
        self.post_processes.borrow_mut().push(post_process);
    }

    pub fn init_wgpu(&mut self, window: Arc<winit::window::Window>) {
        pollster::block_on(self.init_wgpu_async(window));
    }

    pub async fn get_adapter(
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> wgpu::Adapter {
        let adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;
        let adapters = adapters
            .into_iter()
            .filter(|adapter| {
                let caps = surface.get_capabilities(adapter);
                !caps.formats.is_empty()
                    && !caps.present_modes.is_empty()
                    && !caps.alpha_modes.is_empty()
            })
            .collect::<Vec<_>>();

        assert!(
            !adapters.is_empty(),
            "No surface-compatible adapters found for current backend/window"
        );

        // 1. Prefer Vulkan
        let adapter = adapters.iter().find(|adapter| {
            adapter.get_info().device_type == wgpu::DeviceType::DiscreteGpu
                && adapter.get_info().backend == wgpu::Backend::Vulkan
        });
        if let Some(adapter) = adapter {
            info!(info = ?adapter.get_info(), "Using discrete Vulkan adapter");
            return adapter.clone();
        }

        // 2. Try any discrete GPU
        let adapter = adapters
            .iter()
            .find(|adapter| adapter.get_info().device_type == wgpu::DeviceType::DiscreteGpu);
        if let Some(adapter) = adapter {
            info!(info = ?adapter.get_info(), "Using discrete GPU adapter");
            return adapter.clone();
        }

        // 3. Try get any Vulkan adapter
        let adapter = adapters
            .iter()
            .find(|adapter| adapter.get_info().backend == wgpu::Backend::Vulkan);
        if let Some(adapter) = adapter {
            info!(info = ?adapter.get_info(), "Using Vulkan adapter");
            return adapter.clone();
        }

        // 4. Try get any adapter
        let adapter = adapters.first().expect("No adapters found");
        info!(info = ?adapter.get_info(), "Using first found adapter");
        adapter.clone()
    }

    #[instrument(skip_all)]
    pub async fn init_wgpu_async(&mut self, window: Arc<winit::window::Window>) {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());

        let surface = instance
            .create_surface(window.clone())
            .expect("Couldn't create surface");

        // let adapter = instance.request_adapter(
        //     &wgpu::RequestAdapterOptions {
        //         power_preference: wgpu::PowerPreference::HighPerformance,
        //         compatible_surface: Some(&surface),
        //         force_fallback_adapter: false,
        //     }
        // ).await.unwrap();

        let adapter = Self::get_adapter(&instance, &surface).await;

        let adapter_features = adapter.features();
        let supports_gpu_timestamps = adapter_features.contains(wgpu::Features::TIMESTAMP_QUERY)
            && adapter_features.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);

        let mut device_desc = wgpu::DeviceDescriptor {
            // Use adapter-reported limits to avoid requesting unsupported defaults on downlevel
            // backends (e.g. OpenGL with no compute support).
            required_limits: adapter.limits(),
            ..Default::default()
        };
        if supports_gpu_timestamps {
            device_desc.required_features |=
                wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        }

        let (device, queue) = match adapter.request_device(&device_desc).await {
            Ok(pair) => pair,
            Err(err) => {
                warn!(
                    ?err,
                    "Primary device request failed, retrying without optional timestamp features"
                );
                let fallback_desc = wgpu::DeviceDescriptor {
                    required_limits: adapter.limits(),
                    ..Default::default()
                };
                adapter
                    .request_device(&fallback_desc)
                    .await
                    .expect("Failed to create device with adapter limits")
            }
        };
        device.on_uncaptured_error(Arc::new(|err| {
            error!("Uncaptured wgpu error: {err}");
        }));

        let gpu_timestamps = if device.features().contains(wgpu::Features::TIMESTAMP_QUERY)
            && device
                .features()
                .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS)
        {
            let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("Frame Timestamp Query Set"),
                ty: wgpu::QueryType::Timestamp,
                count: GPU_TIMESTAMP_QUERY_COUNT,
            });
            let byte_size = (GPU_TIMESTAMP_QUERY_COUNT as u64) * (size_of::<u64>() as u64);
            let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Frame Timestamp Resolve Buffer"),
                size: byte_size,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Frame Timestamp Readback Buffer"),
                size: byte_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            Some(GpuTimestampContext {
                query_set,
                resolve_buffer,
                readback_buffer,
                timestamp_period_ns: queue.get_timestamp_period(),
            })
        } else {
            None
        };

        let surface_caps = surface.get_capabilities(&adapter);
        assert!(
            !surface_caps.formats.is_empty(),
            "Adapter reported no surface formats after selection"
        );

        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            // present_mode: wgpu::PresentMode::AutoVsync,
            present_mode: wgpu::PresentMode::AutoNoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        let main_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Main Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Linear Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let sprite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sprite Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/sprite.wgsl").into(),
            ),
        });

        let sprite_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Sprite Pipeline Layout"),
                bind_group_layouts: &[
                    &device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some("Sprite Frame Bind Group Layout"),
                        entries: &[
                            wgpu::BindGroupLayoutEntry {
                                binding: 0,
                                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                                ty: wgpu::BindingType::Buffer {
                                    ty: wgpu::BufferBindingType::Uniform,
                                    has_dynamic_offset: false,
                                    min_binding_size: wgpu::BufferSize::new(
                                        size_of::<FrameUniform>() as u64,
                                    ),
                                },
                                count: None,
                            },
                            wgpu::BindGroupLayoutEntry {
                                binding: 1,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                                count: None,
                            },
                            wgpu::BindGroupLayoutEntry {
                                binding: 2,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Texture {
                                    multisampled: false,
                                    sample_type: wgpu::TextureSampleType::Float {
                                        filterable: true,
                                    },
                                    view_dimension: wgpu::TextureViewDimension::D2,
                                },
                                count: None,
                            },
                        ],
                    }),
                    &device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some("Sprite Texture Bind Group Layout"),
                        entries: &[
                            wgpu::BindGroupLayoutEntry {
                                binding: 0,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Texture {
                                    multisampled: false,
                                    sample_type: wgpu::TextureSampleType::Float {
                                        filterable: true,
                                    },
                                    view_dimension: wgpu::TextureViewDimension::D2,
                                },
                                count: None,
                            },
                            wgpu::BindGroupLayoutEntry {
                                binding: 1,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Texture {
                                    multisampled: false,
                                    sample_type: wgpu::TextureSampleType::Float {
                                        filterable: true,
                                    },
                                    view_dimension: wgpu::TextureViewDimension::D2,
                                },
                                count: None,
                            },
                        ],
                    }),
                ],
                immediate_size: 0,
            });

        let sprite_vertex_state = wgpu::VertexState {
            module: &sprite_shader,
            entry_point: Some("vs_entry"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: size_of::<SpriteGpuInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &SPRITE_INSTANCE_ATTRIBUTES,
            }],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        };

        let opaque_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Opaque Pipeline"),
            layout: Some(&sprite_pipeline_layout),
            vertex: sprite_vertex_state.clone(),
            fragment: Some(wgpu::FragmentState {
                module: &sprite_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let alpha_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Alpha Pipeline"),
            layout: Some(&sprite_pipeline_layout),
            vertex: sprite_vertex_state,
            fragment: Some(wgpu::FragmentState {
                module: &sprite_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: false, // No depth writing for alpha blending
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let debug_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Debug Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/debug.wgsl").into(),
            ),
        });

        let debug_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Debug Pipeline Layout"),
                bind_group_layouts: &[&device.create_bind_group_layout(
                    &wgpu::BindGroupLayoutDescriptor {
                        label: Some("Debug Bind Group Layout"),
                        entries: &[wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: wgpu::BufferSize::new(
                                    size_of::<DebugUniform>() as u64
                                ),
                            },
                            count: None,
                        }],
                    },
                )],
                immediate_size: 0,
            });

        let debug_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Debug Pipeline"),
            layout: Some(&debug_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &debug_shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: size_of::<DebugVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &DEBUG_VERTEX_ATTRIBUTES,
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &debug_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        let shadow_mask_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shadow mask shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/shadow.wgsl").into(),
            ),
        });

        let shadow_mask_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Shadow mask pipeline layout"),
                bind_group_layouts: &[],
                immediate_size: 0,
            });

        let shadow_mask_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shadow mask pipeline"),
            layout: Some(&shadow_mask_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shadow_mask_shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: size_of::<[Vec2; 4]>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2,
                        1 => Float32x2,
                        2 => Float32x2,
                        3 => Float32x2,
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shadow_mask_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let light_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Light shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/light.wgsl").into(),
            ),
        });

        let light_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Light pipeline layout"),
                bind_group_layouts: &[&device.create_bind_group_layout(
                    &wgpu::BindGroupLayoutDescriptor {
                        label: Some("Light bind group layout"),
                        entries: &[
                            wgpu::BindGroupLayoutEntry {
                                binding: 0,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Buffer {
                                    ty: wgpu::BufferBindingType::Uniform,
                                    has_dynamic_offset: true,
                                    min_binding_size: wgpu::BufferSize::new(
                                        size_of::<LightUniform>() as u64,
                                    ),
                                },
                                count: None,
                            },
                            wgpu::BindGroupLayoutEntry {
                                binding: 1,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Texture {
                                    multisampled: false,
                                    sample_type: wgpu::TextureSampleType::Float {
                                        filterable: false,
                                    },
                                    view_dimension: wgpu::TextureViewDimension::D2,
                                },
                                count: None,
                            },
                        ],
                    },
                )],
                immediate_size: 0,
            });

        let light_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Light pipeline"),
            layout: Some(&light_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &light_shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vec2>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &light_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            operation: wgpu::BlendOperation::Add,
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                        },
                        alpha: wgpu::BlendComponent {
                            operation: wgpu::BlendOperation::Add,
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let fullscreen_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fullscreen Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/fullscreen.wgsl").into(),
            ),
        });

        let fullscreen_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Fullscreen Pipeline Layout"),
                bind_group_layouts: &[
                    &device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some("Global fullscreen bind group layout"),
                        entries: &[wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        }],
                    }),
                    &device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some("Fullscreen texture bind group layout"),
                        entries: &[wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        }],
                    }),
                ],
                immediate_size: 0,
            });

        let fullscreen_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Fullscreen Pipeline"),
            layout: Some(&fullscreen_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &fullscreen_shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    // Vertex::desc(),
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &fullscreen_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        // Prepare text renderer
        let font_system = Arc::new(RwLock::new(glyphon::FontSystem::new_with_locale_and_db(
            "".to_string(),
            glyphon::fontdb::Database::new(),
        )));
        let swash_cache = glyphon::SwashCache::new();
        let cache = glyphon::Cache::new(&device);
        let mut text_viewport = glyphon::Viewport::new(&device, &cache);
        let mut atlas = glyphon::TextAtlas::new(&device, &queue, &cache, config.format);

        let text_renderer = glyphon::TextRenderer::new(
            &mut atlas,
            &device,
            wgpu::MultisampleState::default(),
            Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
        );
        let initial_presentation = Self::compute_presentation(size.width, size.height);
        let initial_text_resolution = initial_presentation.render_resolution;
        text_viewport.update(
            &queue,
            glyphon::Resolution {
                width: initial_text_resolution[0],
                height: initial_text_resolution[1],
            },
        );

        let transparent_tex = Renderer::create_solid_texture(
            &device,
            &queue,
            Color::TRANSPARENT,
            "Transparent solid texture",
        );
        let black_tex =
            Renderer::create_solid_texture(&device, &queue, Color::BLACK, "Black solid texture");
        let white_tex =
            Renderer::create_solid_texture(&device, &queue, Color::WHITE, "White solid texture");

        self.context = Some(WgpuContext {
            window,
            surface,
            device,
            queue,
            config: RefCell::new(config),
            opaque_pipeline,
            alpha_pipeline,
            debug_pipeline,
            shadow_mask_pipeline,
            light_pipeline,
            fullscreen_pipeline,
            is_surface_configured: AtomicBool::new(false),
            main_sampler,
            linear_sampler,
            gpu_timestamps,

            transparent_tex,
            black_tex,
            white_tex,

            font_system,
            swash_cache: RefCell::new(swash_cache),
            text_viewport,
            atlas: RefCell::new(atlas),
            text_renderer: RefCell::new(text_renderer),
            virtual_resolution: glam::vec2(
                initial_text_resolution[0] as f32,
                initial_text_resolution[1] as f32,
            ),
        });
    }

    pub fn get_context(&self) -> Option<&WgpuContext> {
        self.context.as_ref()
    }

    #[instrument(level = "debug", skip(self))]
    pub fn resize(&mut self, width: u32, height: u32) {
        if width < 1 || height < 1 {
            return;
        }

        {
            let Some(ctx) = self.context.as_mut() else {
                return;
            };
            {
                let mut config = ctx.config.borrow_mut();
                config.width = width;
                config.height = height;
            }
            ctx.surface.configure(&ctx.device, &ctx.config.borrow());
            ctx.is_surface_configured
                .store(true, std::sync::atomic::Ordering::Release);
        }

        let presentation = Self::compute_presentation(width, height);
        let Some(ctx) = self.context.as_mut() else {
            return;
        };
        let virtual_resolution = presentation.render_resolution.as_vec2();
        if ctx.virtual_resolution != virtual_resolution {
            Self::update_text_resolution(ctx, presentation.render_resolution);
            ctx.virtual_resolution = virtual_resolution;
        }
        self.targets = Some(Self::make_targets(&*ctx, presentation.render_resolution));
    }

    #[instrument(level = "debug", skip_all, fields(frame_index, render_stats))]
    pub fn render(
        &mut self,
        render_ctx: &mut RenderCtx<'_>,
        render_world: &RenderWorld,
    ) -> Result<RenderStats, RenderError> {
        let span = Span::current();

        let mut stats = RenderStats::default();

        let output = {
            let Some(ctx) = self.context.as_mut() else {
                return Ok(stats);
            };
            if !ctx
                .is_surface_configured
                .load(std::sync::atomic::Ordering::Acquire)
            {
                return Ok(stats);
            }

            match ctx.surface.get_current_texture() {
                Ok(output) => output,
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    ctx.surface.configure(&ctx.device, &ctx.config.borrow());
                    return Ok(stats);
                }
                Err(wgpu::SurfaceError::Timeout) => {
                    warn!("Surface timeout while acquiring next frame");
                    return Ok(stats);
                }
                Err(err @ wgpu::SurfaceError::OutOfMemory) => return Err(err.into()),
                Err(err) => return Err(err.into()),
            }
        };

        let surface_size = {
            let Some(ctx) = self.context.as_ref() else {
                return Ok(stats);
            };
            [ctx.config.borrow().width, ctx.config.borrow().height]
        };
        let presentation = Self::compute_presentation(surface_size[0], surface_size[1]);
        {
            let Some(ctx) = self.context.as_mut() else {
                return Ok(stats);
            };
            let virtual_resolution = presentation.render_resolution.as_vec2();
            if ctx.virtual_resolution != virtual_resolution {
                Self::update_text_resolution(ctx, presentation.render_resolution);
                ctx.virtual_resolution = virtual_resolution;
            }
        }
        let max_lighting_layers = render_world
            .scene_passes
            .iter()
            .map(|pass| pass.lighting_layer_count as usize)
            .max()
            .unwrap_or(0);
        let Some(ctx) = self.context.as_ref() else {
            return Ok(stats);
        };
        Self::ensure_targets(&mut self.targets, ctx, presentation.render_resolution);
        Self::ensure_lighting_textures(
            &mut self.targets,
            ctx,
            presentation.render_resolution,
            max_lighting_layers,
        );
        let Some(ctx) = self.context.as_ref() else {
            return Ok(stats);
        };
        let error_scope = ctx.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        let mut frame = RenderFrame::new(ctx, &mut stats);
        let output_view = frame.create_texture_view(&output.texture);
        let uses_downscaling = presentation.output_viewport.1.x < presentation.render_resolution.x
            || presentation.output_viewport.1.y < presentation.render_resolution.y;
        let presentation_sampler = if uses_downscaling {
            &ctx.linear_sampler
        } else {
            &ctx.main_sampler
        };

        let global_bind_group = frame.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Global Bind Group"),
            layout: &ctx.fullscreen_pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(presentation_sampler),
            }],
        });

        if let Some(ts) = &ctx.gpu_timestamps {
            encoder.write_timestamp(&ts.query_set, GPU_QUERY_SCENE_START);
        }

        {
            let targets = self.targets.as_ref().unwrap();
            self.clear_scene_target(&mut frame, &mut encoder, targets);
            if !render_world.scene_passes.is_empty() {
                let scene_count = render_world.scene_passes.len();
                for (scene_index, scene_data) in render_world.scene_passes.iter().rev().enumerate()
                {
                    let pass = SceneRenderPass {
                        render_resolution: presentation.render_resolution,
                        scene_data,
                        targets,
                    };
                    self.render_scene(&mut frame, &mut encoder, render_ctx, &pass);
                    Self::prepare_scene_text_areas(
                        self.context.as_ref().unwrap(),
                        render_ctx,
                        scene_data,
                        presentation.render_resolution,
                    );
                    self.render_text_pass(&mut frame, &mut encoder, targets);

                    // Submit this scene before preparing text for the next one.
                    if scene_index + 1 < scene_count {
                        ctx.queue.submit(std::iter::once(encoder.finish()));
                        encoder =
                            ctx.device
                                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                    label: Some("Render Encoder"),
                                });
                    }
                }
            }
            if let Some(ts) = &ctx.gpu_timestamps {
                encoder.write_timestamp(&ts.query_set, GPU_QUERY_SCENE_END);
            }
        }
        {
            let post_processes = &self.post_processes;
            let targets = self.targets.as_mut().unwrap();
            Self::render_intermediate_passes(
                &mut frame,
                &mut encoder,
                render_ctx,
                render_world,
                post_processes,
                targets,
            );
        }
        let targets = self.targets.as_ref().unwrap();
        self.render_final_pass(
            &mut frame,
            &mut encoder,
            &output_view,
            targets,
            &global_bind_group,
            presentation.output_viewport,
        );
        if let Some(ts) = &ctx.gpu_timestamps {
            encoder.write_timestamp(&ts.query_set, GPU_QUERY_FRAME_END);
            encoder.resolve_query_set(
                &ts.query_set,
                0..GPU_TIMESTAMP_QUERY_COUNT,
                &ts.resolve_buffer,
                0,
            );
            encoder.copy_buffer_to_buffer(
                &ts.resolve_buffer,
                0,
                &ts.readback_buffer,
                0,
                (GPU_TIMESTAMP_QUERY_COUNT as u64) * (size_of::<u64>() as u64),
            );
        }

        // drop(frame);
        ctx.queue.submit(std::iter::once(encoder.finish()));

        if let Some(err) = pollster::block_on(error_scope.pop()) {
            return Err(err.into());
        }

        output.present();

        if let Some(ts) = &ctx.gpu_timestamps {
            if let Some((scene_ms, frame_ms)) = Self::read_gpu_timings(ctx, ts) {
                stats.gpu_scene_ms = Some(scene_ms);
                stats.gpu_frame_ms = Some(frame_ms);
            }
        }

        record_all!(
            span,
            frame_index = render_world.frame_index,
            render_stats = ?stats,
        );

        Ok(stats)
    }

    fn read_gpu_timings(ctx: &WgpuContext, ts: &GpuTimestampContext) -> Option<(f64, f64)> {
        let slice = ts.readback_buffer.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });

        let _ = ctx.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        // Two-stage failure handling:
        // 1) Channel receive can fail if callback delivery fails.
        // 2) Callback can arrive with a map error from wgpu.
        let map_result = match rx.recv() {
            Ok(callback_result) => callback_result,
            Err(_) => {
                ts.readback_buffer.unmap();
                return None;
            }
        };
        if map_result.is_err() {
            ts.readback_buffer.unmap();
            return None;
        }

        let data = slice.get_mapped_range();
        let timestamps: &[u64] = bytemuck::cast_slice(&data);
        if timestamps.len() < GPU_TIMESTAMP_QUERY_COUNT as usize {
            drop(data);
            ts.readback_buffer.unmap();
            return None;
        }

        let scene_ticks = timestamps[GPU_QUERY_SCENE_END as usize]
            .saturating_sub(timestamps[GPU_QUERY_SCENE_START as usize]);
        let frame_ticks = timestamps[GPU_QUERY_FRAME_END as usize]
            .saturating_sub(timestamps[GPU_QUERY_FRAME_START as usize]);

        let ns_per_tick = ts.timestamp_period_ns as f64;
        let scene_ms = (scene_ticks as f64 * ns_per_tick) / 1_000_000.0;
        let frame_ms = (frame_ticks as f64 * ns_per_tick) / 1_000_000.0;

        drop(data);
        ts.readback_buffer.unmap();
        Some((scene_ms, frame_ms))
    }

    #[instrument(level = "trace", skip_all)]
    fn render_scene(
        &self,
        frame: &mut RenderFrame<'_>,
        encoder: &mut wgpu::CommandEncoder,
        render_ctx: &mut RenderCtx<'_>,
        pass: &SceneRenderPass<'_>,
    ) {
        trace!(
            lighting_layer_count = pass.scene_data.lighting_layer_count,
            light_counts = ?pass.scene_data.lights_by_layer.iter().map(|layer| layer.len()).collect::<Vec<_>>(),
            shadow_counts = ?pass.scene_data.shadow_segments_by_layer.iter().map(|layer| layer.len()).collect::<Vec<_>>(),
            "scene lighting stats"
        );
        let ctx = self.context.as_ref().unwrap();
        let targets = self.targets.as_ref().unwrap();

        let snapped_camera = Self::snapped_world_camera(pass.scene_data.camera);
        let world_to_camera = snapped_camera.world_to_camera();
        let camera_to_clip = snapped_camera.camera_to_clip(pass.render_resolution.as_vec2());
        let world_to_clip = camera_to_clip * world_to_camera;
        let uniform_size = size_of::<LightUniform>() as u64;
        let uniform_alignment =
            (ctx.device.limits().min_uniform_buffer_offset_alignment as u64).max(1);
        let uniform_stride = uniform_size.div_ceil(uniform_alignment) * uniform_alignment;

        let total_light_count: usize = pass
            .scene_data
            .lights_by_layer
            .iter()
            .map(|layer| layer.len())
            .sum();
        let max_shadow_quads_per_light = pass
            .scene_data
            .shadow_segments_by_layer
            .iter()
            .map(Vec::len)
            .max()
            .unwrap_or(0);
        self.ensure_light_reuse_buffers(
            frame,
            total_light_count,
            max_shadow_quads_per_light,
            uniform_stride,
        );
        let mut light_reuse_buffers = self.light_reuse_buffers.borrow_mut();
        let shadow_quad_buffer = light_reuse_buffers
            .shadow_quad_buffer
            .as_ref()
            .expect("shadow quad buffer ensured")
            .clone();
        let light_triangle_buffer = light_reuse_buffers
            .light_triangle_buffer
            .as_ref()
            .expect("light triangle buffer ensured")
            .clone();
        let light_uniform_buffer = light_reuse_buffers
            .light_uniform_buffer
            .as_ref()
            .expect("light uniform buffer ensured")
            .clone();
        let quad_size_bytes = size_of::<[Vec2; 4]>() as u64;
        let light_triangle_bytes = (3 * size_of::<Vec2>()) as u64;
        let light_bind_group = frame.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Light bind group"),
            layout: &ctx.light_pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &light_uniform_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(uniform_size),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&targets.shadow_mask_texture_view),
                },
            ],
        });

        // === Lights ===
        // Process lights per lighting layer so each layer generates its own lighting texture.
        let layer_count = pass.scene_data.lighting_layer_count as usize;
        let mut global_light_index = 0usize;
        for layer_index in 0..layer_count {
            let lights = pass
                .scene_data
                .lights_by_layer
                .get(layer_index)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let lighting_view = match targets.lighting_texture_views.get(layer_index) {
                Some(view) => view,
                None => continue,
            };
            let shadow_segments = pass
                .scene_data
                .shadow_segments_by_layer
                .get(layer_index)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let shadow_quads_per_light = shadow_segments.len();

            if lights.is_empty() {
                // Clear the lighting texture for this layer even if no lights exist.
                let lighting_pass_descriptor = wgpu::RenderPassDescriptor {
                    label: Some("Lighting pass (empty layer)"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: lighting_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                };
                let _ = frame.begin_render_pass(encoder, &lighting_pass_descriptor);
                continue;
            }

            let layer_base_index = global_light_index;
            let mut first_light = true;
            for (local_index, light) in lights.iter().enumerate() {
                let light_index = layer_base_index + local_index;

                // Shadow mask pass
                {
                    let _span = trace_span!(
                        "Lighting layer shadow mask pass",
                        layer_index,
                        light_count = lights.len(),
                        light_index = light_index
                    )
                    .entered();
                    let shadow_mask_pass_descriptor = wgpu::RenderPassDescriptor {
                        label: Some("Shadow mask pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &targets.shadow_mask_texture_view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    };
                    let mut shadow_mask_pass =
                        frame.begin_render_pass(encoder, &shadow_mask_pass_descriptor);
                    shadow_mask_pass.set_pipeline(&ctx.shadow_mask_pipeline);

                    if shadow_quads_per_light != 0 {
                        let light_radius_clip = world_to_clip
                            .transform_vector3(Vec3::new(light.radius, 0.0, 0.0))
                            .xy()
                            .length();
                        let light_center_clip = world_to_clip
                            .project_point3(light.position.extend(0.0))
                            .xy();
                        light_reuse_buffers.shadow_quads_scratch.clear();
                        light_reuse_buffers.shadow_quads_scratch.extend(
                            shadow_segments.iter().map(|s| {
                                s.extrude(world_to_clip, light_center_clip, 2.0 * light_radius_clip)
                            }),
                        );

                        let shadow_quad_offset_bytes = (light_index as u64)
                            * (max_shadow_quads_per_light as u64)
                            * quad_size_bytes;
                        ctx.queue.write_buffer(
                            &shadow_quad_buffer,
                            shadow_quad_offset_bytes,
                            bytemuck::cast_slice(&light_reuse_buffers.shadow_quads_scratch),
                        );

                        let shadow_quad_len_bytes = (light_reuse_buffers.shadow_quads_scratch.len()
                            as u64)
                            * quad_size_bytes;
                        shadow_mask_pass.set_vertex_buffer(
                            0,
                            shadow_quad_buffer.slice(
                                shadow_quad_offset_bytes
                                    ..(shadow_quad_offset_bytes + shadow_quad_len_bytes),
                            ),
                        );

                        frame.draw(
                            &mut shadow_mask_pass,
                            0..6,
                            0..(light_reuse_buffers.shadow_quads_scratch.len() as u32),
                        );
                    }
                }

                // Lighting pass
                {
                    let _span = trace_span!(
                        "Lighting layer pass",
                        layer_index,
                        light_count = lights.len(),
                        light_index = light_index
                    )
                    .entered();
                    let lighting_pass_descriptor = wgpu::RenderPassDescriptor {
                        label: Some("Lighting pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: lighting_view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: if first_light {
                                    wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                                } else {
                                    wgpu::LoadOp::Load
                                },
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    };
                    let mut lighting_pass =
                        frame.begin_render_pass(encoder, &lighting_pass_descriptor);
                    lighting_pass.set_pipeline(&ctx.light_pipeline);

                    let light_triangle_clip =
                        Self::triangle_around_circle(light.position, light.radius)
                            .map(|vertex| world_to_clip.project_point3(vertex.extend(0.0)).xy());
                    let light_triangle_offset_bytes = (light_index as u64) * light_triangle_bytes;
                    ctx.queue.write_buffer(
                        &light_triangle_buffer,
                        light_triangle_offset_bytes,
                        bytemuck::cast_slice(&light_triangle_clip),
                    );

                    let mut light_uniform = LightUniform::from(*light);
                    let resolution = pass.render_resolution.as_vec2();
                    let center_clip = world_to_clip
                        .project_point3(light.position.extend(0.0))
                        .xy();
                    let center_px = Vec2::new(
                        (center_clip.x * 0.5 + 0.5) * resolution.x,
                        (-center_clip.y * 0.5 + 0.5) * resolution.y,
                    );
                    let radius_edge_clip = world_to_clip
                        .project_point3((light.position + Vec2::X * light.radius).extend(0.0));
                    let radius_edge_px = Vec2::new(
                        (radius_edge_clip.x * 0.5 + 0.5) * resolution.x,
                        (-radius_edge_clip.y * 0.5 + 0.5) * resolution.y,
                    );
                    light_uniform.center = center_px;
                    light_uniform.radius = center_px.distance(radius_edge_px);
                    let dir_world = if light.direction.length_squared() > 1e-10 {
                        light.direction.normalize()
                    } else {
                        Vec2::X
                    };
                    let dir_end_clip = world_to_clip
                        .project_point3((light.position + dir_world).extend(0.0))
                        .xy();
                    let dir_end_px = Vec2::new(
                        (dir_end_clip.x * 0.5 + 0.5) * resolution.x,
                        (-dir_end_clip.y * 0.5 + 0.5) * resolution.y,
                    );
                    let dir_px = dir_end_px - center_px;
                    if dir_px.length_squared() > 1e-10 {
                        let normalized = dir_px.normalize();
                        light_uniform.direction_and_cos_half_angle.x = normalized.x;
                        light_uniform.direction_and_cos_half_angle.y = normalized.y;
                    } else {
                        light_uniform.direction_and_cos_half_angle.x = Vec2::X.x;
                        light_uniform.direction_and_cos_half_angle.y = Vec2::X.y;
                    }
                    light_uniform.direction_and_cos_half_angle.z = (light.angle * 0.5).cos();
                    let light_uniform_offset_bytes = (light_index as u64) * uniform_stride;
                    ctx.queue.write_buffer(
                        &light_uniform_buffer,
                        light_uniform_offset_bytes,
                        bytemuck::bytes_of(&light_uniform),
                    );

                    lighting_pass.set_bind_group(
                        0,
                        &light_bind_group,
                        &[light_uniform_offset_bytes as u32],
                    );
                    lighting_pass.set_vertex_buffer(
                        0,
                        light_triangle_buffer.slice(
                            light_triangle_offset_bytes
                                ..(light_triangle_offset_bytes + light_triangle_bytes),
                        ),
                    );
                    frame.draw(&mut lighting_pass, 0..3, 0..1);
                }

                first_light = false;
            }

            global_light_index += lights.len();
        }

        // === Object rendering ===
        // First, prepare all batches and sprites for rendering
        type BatchKey = (Material, SpriteSpace, Option<u8>);
        let mut opaque_batches: HashMap<BatchKey, Vec<SpriteGpuInstance>> = HashMap::new();
        let mut alpha_sprites: Vec<(BatchKey, f32, SpriteGpuInstance)> = Vec::new();

        for sprite in &pass.scene_data.sprites {
            let object_to_world = if sprite.space == SpriteSpace::World {
                Self::snapped_world_transform(sprite.object_to_world)
            } else {
                sprite.object_to_world
            };
            let instance = SpriteGpuInstance {
                model: object_to_world,
                uv_min: sprite.uv_min,
                uv_max: sprite.uv_max,
            };
            let key = (sprite.material, sprite.space, sprite.lighting_layer);
            match sprite.material.blend_mode {
                BlendMode::Alpha => {
                    alpha_sprites.push((key, sprite.depth, instance));
                }
                BlendMode::Opaque => {
                    opaque_batches.entry(key).or_default().push(instance);
                }
            }
        }
        alpha_sprites.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));

        // Calculate some useful things
        // Initialize buffers
        // TODO Store buffers across frames. Idk why it's not the case already...
        let world_frame_uniform_buffer =
            frame.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Sprite World Frame Uniform Buffer"),
                contents: bytemuck::cast_slice(&[FrameUniform::new(
                    world_to_clip,
                    pass.render_resolution,
                )]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let screen_frame_uniform_buffer =
            frame.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Sprite Screen Frame Uniform Buffer"),
                contents: bytemuck::cast_slice(&[FrameUniform::new(
                    camera_to_clip,
                    pass.render_resolution,
                )]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let lighting_view_for_layer = |layer: Option<u8>| -> &'_ wgpu::TextureView {
            match layer {
                Some(layer_idx) => targets
                    .lighting_texture_views
                    .get(layer_idx as usize)
                    .unwrap_or(&frame.ctx.white_tex),
                None => &frame.ctx.white_tex,
            }
        };

        let scene_color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: &pass.targets.drawn_texture_view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];

        let frame_bind_group_layout = frame.ctx.opaque_pipeline.get_bind_group_layout(0);
        let sprite_sampler = &frame.ctx.main_sampler;

        {
            let opaque_scene_pass_descriptor = wgpu::RenderPassDescriptor {
                label: Some("Opaque Scene Render Pass"),
                color_attachments: &scene_color_attachments,
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &pass.targets.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            };
            let mut opaque_render_pass =
                frame.begin_render_pass(encoder, &opaque_scene_pass_descriptor);
            opaque_render_pass.set_pipeline(&frame.ctx.opaque_pipeline);

            for (key, instances) in opaque_batches.iter() {
                let (material, space, lighting_layer) = *key;
                let instance_count = instances.len() as u32;
                if instance_count == 0 {
                    continue;
                }

                let instance_buffer = frame.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Sprite Instance Buffer"),
                    contents: bytemuck::cast_slice(instances),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let Some(texture) = render_ctx
                    .assets()
                    .resolve_handle::<Texture>(material.texture)
                else {
                    continue;
                };
                let emissive_tex = material
                    .emissive_texture
                    .map(|t| render_ctx.assets().resolve_handle::<Texture>(t))
                    .flatten();
                let texture_view = frame.create_texture_view(&texture.raw);
                let emissive_view = emissive_tex.map(|t| frame.create_texture_view(&t.raw));
                let sprite_texture_bind_group =
                    frame.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Sprite Texture Bind Group"),
                        layout: &frame.ctx.opaque_pipeline.get_bind_group_layout(1),
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&texture_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(
                                    emissive_view.as_ref().unwrap_or(&frame.ctx.black_tex),
                                ),
                            },
                        ],
                    });

                let lighting_view = lighting_view_for_layer(lighting_layer);
                let uniform_buffer = if space == SpriteSpace::World {
                    &world_frame_uniform_buffer
                } else {
                    &screen_frame_uniform_buffer
                };
                let frame_bind_group = frame.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Sprite Frame Bind Group"),
                    layout: &frame_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sprite_sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(lighting_view),
                        },
                    ],
                });

                opaque_render_pass.set_bind_group(0, &frame_bind_group, &[]);
                opaque_render_pass.set_bind_group(1, &sprite_texture_bind_group, &[]);
                opaque_render_pass.set_vertex_buffer(0, instance_buffer.slice(..));
                frame.draw(&mut opaque_render_pass, 0..6, 0..instance_count);
                match space {
                    SpriteSpace::World => {
                        frame.stats.world_sprite_batches_drawn += 1;
                        frame.stats.world_sprite_instances_drawn += instance_count;
                    }
                    SpriteSpace::Screen => {
                        frame.stats.screen_sprite_batches_drawn += 1;
                        frame.stats.screen_sprite_instances_drawn += instance_count;
                    }
                }
            }
        }

        {
            let alpha_scene_pass_descriptor = wgpu::RenderPassDescriptor {
                label: Some("Alpha Scene Render Pass"),
                color_attachments: &scene_color_attachments,
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &pass.targets.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            };
            let mut alpha_render_pass =
                frame.begin_render_pass(encoder, &alpha_scene_pass_descriptor);
            alpha_render_pass.set_pipeline(&frame.ctx.alpha_pipeline);

            let texture_layout = frame.ctx.alpha_pipeline.get_bind_group_layout(1);
            let mut current_batch: Option<BatchKey> = None;
            let mut instances_to_render: Vec<SpriteGpuInstance> = Vec::with_capacity(8);

            let flush_batch = |instances: &mut Vec<SpriteGpuInstance>,
                               batch: BatchKey,
                               frame: &mut RenderFrame<'_>,
                               render_pass: &mut wgpu::RenderPass<'_>,
                               render_ctx: &RenderCtx| {
                if instances.is_empty() {
                    return;
                }
                let (material, space, lighting_layer) = batch;
                let instance_count = instances.len() as u32;
                let instance_buffer = frame.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Sprite Instance Buffer"),
                    contents: bytemuck::cast_slice(instances),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let Some(texture) = render_ctx
                    .assets()
                    .resolve_handle::<Texture>(material.texture)
                else {
                    instances.clear();
                    return;
                };
                let emissive_tex = material
                    .emissive_texture
                    .map(|t| render_ctx.assets().resolve_handle::<Texture>(t))
                    .flatten();

                let texture_view = frame.create_texture_view(&texture.raw);
                let emissive_view = emissive_tex.map(|t| frame.create_texture_view(&t.raw));
                let sprite_texture_bind_group =
                    frame.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Sprite Texture Bind Group"),
                        layout: &texture_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&texture_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(
                                    emissive_view.as_ref().unwrap_or(&frame.ctx.black_tex),
                                ),
                            },
                        ],
                    });

                let lighting_view = lighting_view_for_layer(lighting_layer);
                let uniform_buffer = if space == SpriteSpace::World {
                    &world_frame_uniform_buffer
                } else {
                    &screen_frame_uniform_buffer
                };
                let frame_bind_group = frame.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Sprite Frame Bind Group"),
                    layout: &frame_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sprite_sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(lighting_view),
                        },
                    ],
                });

                render_pass.set_bind_group(0, &frame_bind_group, &[]);
                render_pass.set_bind_group(1, &sprite_texture_bind_group, &[]);
                render_pass.set_vertex_buffer(0, instance_buffer.slice(..));
                frame.draw(render_pass, 0..6, 0..instance_count);
                match space {
                    SpriteSpace::World => {
                        frame.stats.world_sprite_batches_drawn += 1;
                        frame.stats.world_sprite_instances_drawn += instance_count;
                    }
                    SpriteSpace::Screen => {
                        frame.stats.screen_sprite_batches_drawn += 1;
                        frame.stats.screen_sprite_instances_drawn += instance_count;
                    }
                }
                instances.clear();
            };

            for value in alpha_sprites.iter().map(Some).chain(std::iter::once(None)) {
                let batch = value.map(|(key, _, _)| *key);
                if current_batch != batch {
                    if let Some(batch) = current_batch {
                        flush_batch(
                            &mut instances_to_render,
                            batch,
                            frame,
                            &mut alpha_render_pass,
                            render_ctx,
                        );
                    }
                    current_batch = batch;
                }

                if let Some(&(_, _, instance)) = value {
                    instances_to_render.push(instance);
                }
            }
        }

        if *render_ctx.shown_debug_layers != 0 {
            let debug_pass_descriptor = wgpu::RenderPassDescriptor {
                label: Some("Debug Render Pass"),
                color_attachments: &scene_color_attachments,
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &pass.targets.depth_texture_view,
                    depth_ops: None,
                    stencil_ops: None,
                }),
                ..Default::default()
            };
            let mut debug_render_pass = frame.begin_render_pass(encoder, &debug_pass_descriptor);
            debug_render_pass.set_pipeline(&frame.ctx.debug_pipeline);
            let mut world_vertices = Vec::new();
            let mut screen_vertices = Vec::new();

            for debug_draw in &pass.scene_data.debug_drawables {
                if debug_draw.vertices.len() < 2 {
                    continue;
                }
                if (debug_draw.layer & pass.scene_data.shown_debug_layers) == 0 {
                    continue;
                }

                let target_vertices = if debug_draw.space == SpriteSpace::World {
                    &mut world_vertices
                } else {
                    &mut screen_vertices
                };

                target_vertices.extend(debug_draw.vertices.iter().map(|&position| {
                    let object_to_world = if debug_draw.space == SpriteSpace::World {
                        Self::snapped_world_transform(debug_draw.object_to_world)
                    } else {
                        debug_draw.object_to_world
                    };
                    let transformed = object_to_world.transform_point3(position.extend(0.0));
                    DebugVertex {
                        position: Vec4::new(transformed.x, transformed.y, debug_draw.depth, 1.0),
                        color: debug_draw.color,
                    }
                }));
            }

            let debug_bind_group_layout = frame.ctx.debug_pipeline.get_bind_group_layout(0);

            let draw_debug_batch = |frame: &mut RenderFrame<'_>,
                                    render_pass: &mut wgpu::RenderPass<'_>,
                                    label: &str,
                                    vertices: &[DebugVertex],
                                    batch_view_proj: Mat4| {
                if vertices.is_empty() {
                    return;
                }

                let vertex_buffer = frame.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytemuck::cast_slice(vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                let debug_uniform = DebugUniform {
                    view_proj: batch_view_proj,
                };
                let debug_uniform_buffer =
                    frame.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Debug Uniform Buffer"),
                        contents: bytemuck::cast_slice(&[debug_uniform]),
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    });
                let debug_bind_group = frame.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Debug Bind Group"),
                    layout: &debug_bind_group_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: debug_uniform_buffer.as_entire_binding(),
                    }],
                });

                render_pass.set_bind_group(0, &debug_bind_group, &[]);
                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                frame.draw(render_pass, 0..vertices.len() as u32, 0..1);
            };

            draw_debug_batch(
                frame,
                &mut debug_render_pass,
                "Debug World Vertex Buffer",
                &world_vertices,
                world_to_clip,
            );
            draw_debug_batch(
                frame,
                &mut debug_render_pass,
                "Debug Screen Vertex Buffer",
                &screen_vertices,
                camera_to_clip,
            );
        }
    }

    #[instrument(level = "trace", skip_all)]
    fn render_text_pass(
        &self,
        frame: &mut RenderFrame<'_>,
        encoder: &mut wgpu::CommandEncoder,
        targets: &RendererTargets,
    ) {
        let text_color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: &targets.drawn_texture_view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let text_pass_descriptor = wgpu::RenderPassDescriptor {
            label: Some("Text Render Pass"),
            color_attachments: &text_color_attachments,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &targets.depth_texture_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        };
        let mut render_pass = frame.begin_render_pass(encoder, &text_pass_descriptor);
        let atlas = frame.ctx.atlas.borrow();
        let mut text_renderer = frame.ctx.text_renderer.borrow_mut();
        if let Err(err) = text_renderer.render(&atlas, &frame.ctx.text_viewport, &mut render_pass) {
            warn!("Text render failed: {err}");
        }
    }

    fn clear_scene_target(
        &self,
        frame: &mut RenderFrame<'_>,
        encoder: &mut wgpu::CommandEncoder,
        targets: &RendererTargets,
    ) {
        let clear_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: &targets.drawn_texture_view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })];
        let clear_pass_descriptor = wgpu::RenderPassDescriptor {
            label: Some("Scene Clear Pass"),
            color_attachments: &clear_attachments,
            ..Default::default()
        };
        let _ = frame.begin_render_pass(encoder, &clear_pass_descriptor);
    }

    #[instrument(level = "trace", skip_all)]
    fn render_intermediate_passes(
        frame: &mut RenderFrame<'_>,
        encoder: &mut wgpu::CommandEncoder,
        _render_ctx: &mut RenderCtx<'_>,
        _render_world: &RenderWorld,
        post_processes: &RefCell<Vec<Box<dyn PostProcess>>>,
        targets: &mut RendererTargets,
    ) {
        let mut post_processes = post_processes.borrow_mut();
        for post_process in post_processes.iter_mut() {
            post_process.render(
                frame.ctx,
                encoder,
                &targets.drawn_texture,
                &targets.drawn_texture_view,
                &targets.next_texture,
                &targets.next_texture_view,
            );
            frame.stats.render_passes += 1;
            frame.stats.draw_calls += 1;
            targets.swap();
        }
    }

    #[instrument(level = "trace", skip_all)]
    fn render_final_pass(
        &self,
        frame: &mut RenderFrame<'_>,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
        targets: &RendererTargets,
        global_bind_group: &wgpu::BindGroup,
        output_viewport: (UVec2, UVec2),
    ) {
        let final_color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output_view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })];
        let final_pass_descriptor = wgpu::RenderPassDescriptor {
            label: Some("Final Pass"),
            color_attachments: &final_color_attachments,
            ..Default::default()
        };
        let mut final_pass = frame.begin_render_pass(encoder, &final_pass_descriptor);

        let texture_bind_group = frame.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture Bind Group"),
            layout: &frame.ctx.fullscreen_pipeline.get_bind_group_layout(1),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&targets.drawn_texture_view),
            }],
        });

        let (offset, size) = output_viewport;
        final_pass.set_pipeline(&frame.ctx.fullscreen_pipeline);
        final_pass.set_bind_group(0, global_bind_group, &[]);
        final_pass.set_bind_group(1, &texture_bind_group, &[]);
        final_pass.set_viewport(
            offset.x as f32,
            offset.y as f32,
            size.x as f32,
            size.y as f32,
            0.0,
            1.0,
        );
        frame.draw(&mut final_pass, 0..3, 0..1);
    }

    fn create_target_texture(
        ctx: &WgpuContext,
        name: &str,
        size: UVec2,
        format: Option<wgpu::TextureFormat>,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(name),
            size: wgpu::Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: format.unwrap_or(ctx.config.borrow().format),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        (texture, view)
    }

    fn create_depth_texture(ctx: &WgpuContext, size: UVec2) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Scene Depth Texture"),
            size: wgpu::Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    fn make_targets(ctx: &WgpuContext, size: UVec2) -> RendererTargets {
        let (a_texture, a_texture_view) = Self::create_target_texture(ctx, "Target A", size, None);
        let (b_texture, b_texture_view) = Self::create_target_texture(ctx, "Target B", size, None);
        let (depth_texture, depth_texture_view) = Self::create_depth_texture(ctx, size);
        let (shadow_mask_texture, shadow_mask_texture_view) = Self::create_target_texture(
            ctx,
            "Shadow mask texture",
            size,
            Some(wgpu::TextureFormat::Rgba8Unorm),
        );

        RendererTargets {
            width: size.x,
            height: size.y,
            drawn_texture: a_texture,
            drawn_texture_view: a_texture_view,
            next_texture: b_texture,
            next_texture_view: b_texture_view,
            depth_texture,
            depth_texture_view,
            shadow_mask_texture,
            shadow_mask_texture_view,
            lighting_textures: Vec::new(),
            lighting_texture_views: Vec::new(),
        }
    }

    fn ensure_targets(targets: &mut Option<RendererTargets>, ctx: &WgpuContext, size: UVec2) {
        let needs_recreate = targets
            .as_ref()
            .map(|t| t.width != size.x || t.height != size.y)
            .unwrap_or(true);
        if needs_recreate {
            *targets = Some(Self::make_targets(ctx, size));
        }
    }

    fn ensure_lighting_textures(
        targets: &mut Option<RendererTargets>,
        ctx: &WgpuContext,
        size: UVec2,
        count: usize,
    ) {
        if targets.is_none() {
            return;
        }

        if count == 0 {
            if let Some(targets) = targets.as_mut() {
                targets.lighting_textures.clear();
                targets.lighting_texture_views.clear();
            }
            return;
        }

        loop {
            let current_layers = targets
                .as_ref()
                .map(|t| t.lighting_texture_views.len())
                .unwrap_or(0);
            if current_layers >= count {
                break;
            }

            let (lighting_texture, lighting_texture_view) = Self::create_target_texture(
                ctx,
                "Lighting texture",
                size,
                Some(wgpu::TextureFormat::Rgba8Unorm),
            );
            if let Some(targets) = targets.as_mut() {
                targets.lighting_textures.push(lighting_texture);
                targets.lighting_texture_views.push(lighting_texture_view);
            }
        }

        if let Some(targets) = targets.as_mut() {
            if targets.lighting_texture_views.len() > count {
                targets.lighting_textures.truncate(count);
                targets.lighting_texture_views.truncate(count);
            }
        }
    }

    fn update_text_resolution(ctx: &mut WgpuContext, text_resolution: UVec2) {
        let virtual_resolution = text_resolution.as_vec2();
        if ctx.virtual_resolution == virtual_resolution {
            return;
        }
        ctx.text_viewport.update(
            &mut ctx.queue,
            glyphon::Resolution {
                width: text_resolution.x,
                height: text_resolution.y,
            },
        );
        ctx.virtual_resolution = virtual_resolution;
    }

    #[instrument(level = "trace", skip_all)]
    fn prepare_scene_text_areas(
        ctx: &WgpuContext,
        render_ctx: &RenderCtx,
        scene: &crate::render_world::SceneRenderData,
        render_resolution: UVec2,
    ) {
        struct PlannedTextArea {
            buffer_index: usize,
            text_left: f32,
            text_top: f32,
            scale: f32,
            bounds: glyphon::TextBounds,
            color: [u8; 4],
        }

        let half_width = render_resolution.x.max(1) as f32 * 0.5;
        let half_height = render_resolution.y.max(1) as f32 * 0.5;
        let projection = glam::Mat4::orthographic_rh(
            -half_width,
            half_width,
            half_height,
            -half_height,
            -1000.0,
            1000.0,
        );
        let screen_view_proj = projection;

        let planning_span = trace_span!("planning_text_areas").entered();
        let mut text_buffer_refs: Vec<Ref<'_, crate::scene::TextBufferState>> = Vec::new();
        let mut planned_areas = Vec::new();
        let snapped_camera = Self::snapped_world_camera(scene.camera);
        let camera_translate = glam::Mat4::from_translation(Vec3::new(
            -snapped_camera.center.x,
            -snapped_camera.center.y,
            0.0,
        ));
        let camera_rotate = glam::Mat4::from_rotation_z(-snapped_camera.rotation);
        let camera_zoom =
            glam::Mat4::from_scale(Vec3::new(snapped_camera.zoom, snapped_camera.zoom, 1.0));
        let world_view_proj = projection * camera_zoom * camera_rotate * camera_translate;
        for text in &scene.texts {
            let model = if text.space == SpriteSpace::World {
                Self::snapped_world_transform(text.object_to_world)
            } else {
                text.object_to_world
            };
            let origin = model.transform_point3(Vec3::ZERO).truncate();
            let x_axis = model.transform_vector3(Vec3::X).truncate();
            let y_axis = model.transform_vector3(Vec3::Y).truncate();
            let scale = ((x_axis.length() + y_axis.length()) * 0.5).max(0.001);
            let view_proj = if text.space == SpriteSpace::World {
                world_view_proj
            } else {
                screen_view_proj
            };
            let Some((screen_pos, depth)) = Self::world_point_to_target(
                view_proj,
                render_resolution,
                glam::vec3(origin.x, origin.y, text.depth),
            ) else {
                continue;
            };
            let depth_key = depth.to_bits() as usize;

            {
                let mut state = text.buffer.borrow_mut();
                state.sync(
                    render_ctx.assets(),
                    &mut ctx.font_system.write().unwrap(),
                    text.text.as_str(),
                    &text.style,
                    &text.layout,
                    depth_key,
                );
            }

            let buffer_state = text.buffer.borrow();
            if buffer_state.buffer().is_none() {
                continue;
            }
            text_buffer_refs.push(buffer_state);
            let buffer_index = text_buffer_refs.len() - 1;
            let borrowed = text_buffer_refs
                .last()
                .and_then(|state| state.buffer())
                .expect("text buffer exists");

            let text_size = Self::measure_buffer_size(borrowed);

            let width = text.layout.max_width.unwrap_or(text_size.x.max(1.0)) * scale;
            let height = text.layout.max_height.unwrap_or(text_size.y.max(1.0)) * scale;
            let anchor_x = text.layout.anchor.horizontal_align_factor();
            let anchor_y = text.layout.anchor.vertical_align_factor();
            let rect_left = screen_pos.x - (width * anchor_x);
            let rect_top = screen_pos.y - (height * anchor_y);
            let text_left = rect_left.floor();
            let text_top = (rect_top + (height - text_size.y * scale) * anchor_y).floor();
            let bounded_left = rect_left.floor() as i32;
            let bounded_top = rect_top.floor() as i32;
            let bounded_right = (rect_left + width).ceil() as i32;
            let bounded_bottom = (rect_top + height).ceil() as i32;
            let bounds = glyphon::TextBounds {
                left: bounded_left,
                top: bounded_top,
                right: bounded_right,
                bottom: bounded_bottom,
            };

            planned_areas.push(PlannedTextArea {
                buffer_index,
                text_left,
                text_top,
                scale,
                bounds,
                color: text.style.color,
            });
        }
        planning_span.exit();

        let mut text_areas = Vec::with_capacity(planned_areas.len());
        for area in &planned_areas {
            let buffer = text_buffer_refs[area.buffer_index]
                .buffer()
                .expect("text buffer exists");
            text_areas.push(glyphon::TextArea {
                buffer,
                left: area.text_left,
                top: area.text_top,
                scale: area.scale,
                bounds: area.bounds,
                default_color: glyphon::Color::rgba(
                    area.color[0],
                    area.color[1],
                    area.color[2],
                    area.color[3],
                ),
                custom_glyphs: &[],
            });
        }

        let _span = trace_span!("glyphon_prepare").entered();
        let mut atlas = ctx.atlas.borrow_mut();
        let mut text_renderer = ctx.text_renderer.borrow_mut();
        let mut swash_cache = ctx.swash_cache.borrow_mut();
        if let Err(err) = text_renderer.prepare_with_depth(
            &ctx.device,
            &ctx.queue,
            &mut ctx.font_system.write().unwrap(),
            &mut atlas,
            &ctx.text_viewport,
            text_areas,
            &mut swash_cache,
            |metadata| f32::from_bits(metadata as u32),
        ) {
            warn!("Text prepare failed: {err}");
        }
    }

    fn measure_buffer_size(buffer: &glyphon::Buffer) -> glam::Vec2 {
        let mut width = 0.0_f32;
        let mut height = 0.0_f32;
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            height = height.max(run.line_top + run.line_height + 1.0);
        }
        glam::vec2(width, height)
    }

    /// Projects a point with the specified view-projection matrix and returns
    /// pixel coordinates based on the passed render resolution.
    ///
    /// Used mostly for text rendering as glyphon requires pixel coordinates.
    fn world_point_to_target(
        world_to_clip: Mat4,
        render_resolution: UVec2,
        point: Vec3,
    ) -> Option<(Vec2, f32)> {
        // project_point3 performs perspective divide, so we're getting NDC coordinates
        let ndc = world_to_clip.project_point3(point);
        let x = (ndc.x * 0.5 + 0.5) * render_resolution.x as f32;
        let y = (1.0 - (ndc.y * 0.5 + 0.5)) * render_resolution.y as f32;
        let depth = ndc.z.clamp(0.0, 1.0);
        Some((glam::vec2(x, y), depth))
    }

    fn compute_presentation(surface_width: u32, surface_height: u32) -> PresentationInfo {
        let safe_surface_width = surface_width.max(1);
        let safe_surface_height = surface_height.max(1);
        if safe_surface_width < BASE_VIRTUAL_WIDTH || safe_surface_height < BASE_VIRTUAL_HEIGHT {
            let surface_aspect = safe_surface_width as f32 / safe_surface_height as f32;
            let virtual_width = (BASE_VIRTUAL_HEIGHT as f32 * surface_aspect).ceil() as u32;
            let virtual_height = (BASE_VIRTUAL_WIDTH as f32 / surface_aspect).ceil() as u32;
            let virtual_width = virtual_width.clamp(BASE_VIRTUAL_WIDTH, 1000);
            let virtual_height = virtual_height.clamp(BASE_VIRTUAL_HEIGHT, 1000);

            return PresentationInfo {
                render_resolution: UVec2::new(virtual_width, virtual_height),
                output_viewport: (
                    UVec2::ZERO,
                    UVec2::new(safe_surface_width, safe_surface_height),
                ),
            };
        }

        let scale_x = safe_surface_width / BASE_VIRTUAL_WIDTH;
        let scale_y = safe_surface_height / BASE_VIRTUAL_HEIGHT;
        let scale = scale_x.min(scale_y).max(1);
        let virtual_width = (safe_surface_width / scale).max(1);
        let virtual_height = (safe_surface_height / scale).max(1);

        let viewport_width = virtual_width.saturating_mul(scale);
        let viewport_height = virtual_height.saturating_mul(scale);
        let viewport_x = (safe_surface_width - viewport_width) / 2;
        let viewport_y = (safe_surface_height - viewport_height) / 2;

        PresentationInfo {
            render_resolution: UVec2::new(virtual_width, virtual_height),
            output_viewport: (
                UVec2::new(viewport_x, viewport_y),
                UVec2::new(viewport_width, viewport_height),
            ),
        }
    }

    fn create_solid_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color: Color,
        label: &str,
    ) -> wgpu::TextureView {
        let size = wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let data: [u8; 4] = color.to_rgba_u8().into();

        queue.write_texture(
            texture.as_image_copy(),
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size_of::<Color>() as u32),
                rows_per_image: None,
            },
            size,
        );

        texture.create_view(&Default::default())
    }

    /// Generates an equilateral triangle circumscribing the given circle of specified `center` and `radius`.
    fn triangle_around_circle(center: Vec2, radius: f32) -> [Vec2; 3] {
        let (cx, cy) = center.into();
        let r = radius;
        let sr = 3f32.sqrt() * r;

        [
            Vec2::new(cx, cy + 2.0 * r),
            Vec2::new(cx - sr, cy - r),
            Vec2::new(cx + sr, cy - r),
        ]
    }

    pub fn destroy(&mut self) {
        // Avoid blocking poll during shutdown; some platform/driver combos can
        // crash here when the window/surface is already tearing down.
        // Drop GPU resources in explicit order instead.
        self.post_processes.borrow_mut().clear();
        self.light_reuse_buffers = RefCell::new(LightReuseBuffers::default());
        let targets = self.targets.take();
        let context = self.context.take();
        drop(targets);
        drop(context);
    }
}
