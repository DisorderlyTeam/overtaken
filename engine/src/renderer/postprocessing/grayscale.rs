/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::renderer::WgpuContext;
use crate::renderer::postprocessing::PostProcess;

const GRAYSCALE_SHADER: &str =
    include_str!("../../../../assets/shaders/postprocessing/grayscale.wgsl");

#[derive(Default)]
pub struct GrayscalePostProcess {
    pipeline: Option<wgpu::RenderPipeline>,
    sampler_bind_group: Option<wgpu::BindGroup>,
    output_format: Option<wgpu::TextureFormat>,
}

impl GrayscalePostProcess {
    pub fn new() -> Self {
        Self::default()
    }

    fn ensure_pipeline(&mut self, ctx: &WgpuContext, output_format: wgpu::TextureFormat) {
        if self.output_format == Some(output_format) && self.pipeline.is_some() {
            return;
        }

        let device = &ctx.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Grayscale postprocess shader"),
            source: wgpu::ShaderSource::Wgsl(GRAYSCALE_SHADER.into()),
        });

        let sampler_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Grayscale sampler bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                }],
            });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Grayscale texture bind group layout"),
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

        let sampler_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Grayscale sampler bind group"),
            layout: &sampler_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(&ctx.main_sampler),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Grayscale postprocess pipeline layout"),
            bind_group_layouts: &[&sampler_bind_group_layout, &texture_bind_group_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Grayscale postprocess pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: Some(wgpu::BlendState::REPLACE),
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

        self.pipeline = Some(pipeline);
        self.sampler_bind_group = Some(sampler_bind_group);
        self.output_format = Some(output_format);
    }
}

impl PostProcess for GrayscalePostProcess {
    fn name(&self) -> &str {
        "Grayscale"
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
        let output_format = ctx.config.borrow().format;
        self.ensure_pipeline(ctx, output_format);

        let pipeline = self.pipeline.as_ref().unwrap();
        let sampler_bind_group = self.sampler_bind_group.as_ref().unwrap();
        let texture_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Grayscale texture bind group"),
            layout: &pipeline.get_bind_group_layout(1),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(drawn_texture_view),
            }],
        });

        let pass_label = format!("Postprocess Pass ({})", self.name());
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: next_texture_view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })];
        let pass_desc = wgpu::RenderPassDescriptor {
            label: Some(pass_label.as_str()),
            color_attachments: &color_attachments,
            ..Default::default()
        };

        let mut pass = encoder.begin_render_pass(&pass_desc);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, sampler_bind_group, &[]);
        pass.set_bind_group(1, &texture_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
