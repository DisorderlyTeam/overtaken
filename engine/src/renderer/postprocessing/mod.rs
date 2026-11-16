/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::renderer::WgpuContext;

pub mod color_grading;
pub mod grayscale;

pub trait PostProcess {
    fn name(&self) -> &str;

    fn render(
        &mut self,
        ctx: &WgpuContext,
        encoder: &mut wgpu::CommandEncoder,
        drawn_texture: &wgpu::Texture,
        drawn_texture_view: &wgpu::TextureView,
        next_texture: &wgpu::Texture,
        next_texture_view: &wgpu::TextureView,
    );
}
