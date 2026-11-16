/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::renderer::WgpuContext;
use crate::renderer::postprocessing::PostProcess;
use bytemuck::{Pod, Zeroable};
use std::sync::{Arc, Mutex};
use wgpu::util::DeviceExt;

const COLOR_GRADING_SHADER: &str =
    include_str!("../../../../assets/shaders/postprocessing/color_grading.wgsl");

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ColorGradingParams {
    pub exposure: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub brightness: f32,
    pub tint: [f32; 4],
}

impl Default for ColorGradingParams {
    fn default() -> Self {
        Self {
            exposure: 1.0,
            contrast: 1.0,
            saturation: 1.0,
            brightness: 0.0,
            tint: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ColorGradingPostProcess {
    pipeline: Option<wgpu::RenderPipeline>,
    sampler_bind_group: Option<wgpu::BindGroup>,
    params_bind_group: Option<wgpu::BindGroup>,
    params_buffer: Option<wgpu::Buffer>,
    output_format: Option<wgpu::TextureFormat>,

    shared_params: Arc<Mutex<ColorGradingParams>>,
}

impl ColorGradingPostProcess {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn params_handle(&self) -> Arc<Mutex<ColorGradingParams>> {
        self.shared_params.clone()
    }

    fn ensure_pipeline(&mut self, ctx: &WgpuContext, output_format: wgpu::TextureFormat) {
        if self.output_format == Some(output_format) && self.pipeline.is_some() {
            return;
        }

        let device = &ctx.device;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Color Grading Shader"),
            source: wgpu::ShaderSource::Wgsl(COLOR_GRADING_SHADER.into()),
        });

        let sampler_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Color grading sampler layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            }],
        });

        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Color grading texture layout"),
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
        });

        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Color grading params layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Color grading params buffer"),
            contents: bytemuck::bytes_of(&ColorGradingParams::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let params_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Color grading params bind group"),
            layout: &params_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });

        let sampler_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Color grading sampler bind group"),
            layout: &sampler_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(&ctx.main_sampler),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Color grading pipeline layout"),
            bind_group_layouts: &[&sampler_layout, &texture_layout, &params_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Color grading pipeline"),
            layout: Some(&pipeline_layout),

            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },

            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),

            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        self.pipeline = Some(pipeline);
        self.params_buffer = Some(params_buffer);
        self.params_bind_group = Some(params_bind_group);
        self.sampler_bind_group = Some(sampler_bind_group);
        self.output_format = Some(output_format);
    }
}

impl PostProcess for ColorGradingPostProcess {
    fn name(&self) -> &str {
        "Color Grading"
    }

    fn render(
        &mut self,
        ctx: &WgpuContext,
        encoder: &mut wgpu::CommandEncoder,
        _drawn_texture: &wgpu::Texture,
        drawn_texture_view: &wgpu::TextureView,
        _next_texture: &wgpu::Texture,
        next_texture_view: &wgpu::TextureView,
    ) {
        let format = ctx.config.borrow().format;

        self.ensure_pipeline(ctx, format);

        let params = self
            .shared_params
            .lock()
            .map(|guard| *guard)
            .unwrap_or_else(|poisoned| *poisoned.into_inner());

        if let Some(buffer) = &self.params_buffer {
            ctx.queue
                .write_buffer(buffer, 0, bytemuck::bytes_of(&params));
        }

        let pipeline = self.pipeline.as_ref().unwrap();

        let texture_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Color grading texture bind group"),
            layout: &pipeline.get_bind_group_layout(1),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(drawn_texture_view),
            }],
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Color grading"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: next_texture_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });

        pass.set_pipeline(pipeline);

        pass.set_bind_group(0, self.sampler_bind_group.as_ref().unwrap(), &[]);

        pass.set_bind_group(1, &texture_bind_group, &[]);

        pass.set_bind_group(2, self.params_bind_group.as_ref().unwrap(), &[]);

        pass.draw(0..3, 0..1);
    }
}
