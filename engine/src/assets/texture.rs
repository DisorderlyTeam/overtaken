/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::assets::{Asset, AssetError, AssetLoader};
use compact_str::CompactString;
use glam::Vec2;
use image::ImageError;
use std::any::Any;
use wgpu::util::DeviceExt;
use wgpu::wgt::TextureDataOrder;

pub struct Texture {
    #[allow(dead_code)]
    pub name: CompactString,
    pub size: Vec2,
    pub(crate) raw: wgpu::Texture,
}

impl Asset for Texture {
    fn type_name() -> CompactString {
        CompactString::from("texture")
    }
}

pub struct TextureLoader {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl TextureLoader {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> TextureLoader {
        TextureLoader { device, queue }
    }
}

impl AssetLoader for TextureLoader {
    fn load_asset(&self, name: &str) -> Result<Box<dyn Any>, AssetError> {
        let img = image::open(name).map_err(|e| match e {
            ImageError::IoError(io) => io.into(),
            _ => AssetError::LoadError,
        })?;
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();

        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let texture = self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: Some(name),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            TextureDataOrder::default(),
            rgba.as_raw(),
        );

        Ok(Box::new(Texture {
            name: CompactString::from(name),
            size: Vec2::new(width as f32, height as f32),
            raw: texture,
        }))
    }
}
