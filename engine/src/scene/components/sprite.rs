/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::assets::AssetHandle;
use crate::assets::texture::Texture;
use crate::context::RenderCtx;
use crate::render_world::{SceneRenderData, SpriteInstance};
use crate::scene::components::{Component, ComponentTypeFlags};
use crate::scene::{EntityHandle, GlobalTransform, LocalTransform, Scene};
use glam::{Mat4, Vec2};

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct SpriteComponent {
    pub material: Material,
    pub size: Vec2,
    pub uv_min: Vec2,
    pub uv_max: Vec2,
    pub lighting_layer: Option<u8>,
}

impl SpriteComponent {
    pub fn new(material: Material, size: Vec2) -> Self {
        Self {
            material,
            size,
            uv_min: Vec2::ZERO,
            uv_max: Vec2::ONE,
            lighting_layer: None,
        }
    }

    pub fn with_uv(mut self, uv_min: Vec2, uv_max: Vec2) -> Self {
        self.uv_min = uv_min;
        self.uv_max = uv_max;
        self
    }

    pub fn with_lighting_layer(mut self, layer: u8) -> Self {
        self.lighting_layer = Some(layer);
        self
    }

    pub fn set_lighting_layer(&mut self, layer: Option<u8>) {
        self.lighting_layer = layer;
    }
}

impl Component for SpriteComponent {
    fn flags() -> ComponentTypeFlags {
        ComponentTypeFlags::NEEDS_DRAW
    }

    fn extract_render_data(
        &self,
        scene: &Scene,
        entity: EntityHandle,
        render_data: &mut SceneRenderData,
        _ctx: &mut RenderCtx<'_>,
    ) {
        let Some(local) = scene.get_component::<LocalTransform>(entity) else {
            return;
        };
        let Some(global) = scene.get_component::<GlobalTransform>(entity) else {
            return;
        };

        let object_to_world = global.object_to_world * Mat4::from_scale(self.size.extend(1.0));

        render_data.sprites.push(SpriteInstance {
            material: self.material,
            object_to_world,
            depth: local.position.z,
            uv_min: self.uv_min,
            uv_max: self.uv_max,
            space: local.space(),
            lighting_layer: self.lighting_layer,
        });
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default, Hash)]
pub enum BlendMode {
    /// Blend mode for sprites that don't have partial transparency.
    /// Alpha values of exactly 0 and 1 are supported.
    #[default]
    Opaque,
    /// Blend mode for sprites that have partial transparency.
    Alpha,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Material {
    pub texture: AssetHandle<Texture>,
    pub emissive_texture: Option<AssetHandle<Texture>>,
    pub blend_mode: BlendMode,
}

impl Material {
    pub fn new(
        texture: AssetHandle<Texture>,
        emissive_texture: Option<AssetHandle<Texture>>,
        blend_mode: BlendMode,
    ) -> Self {
        Self {
            texture,
            emissive_texture,
            blend_mode,
        }
    }

    pub fn new_opaque(texture: AssetHandle<Texture>) -> Self {
        Self::new(texture, None, BlendMode::Opaque)
    }

    pub fn new_alpha(texture: AssetHandle<Texture>) -> Self {
        Self::new(texture, None, BlendMode::Alpha)
    }

    pub fn with_emissive(mut self, emissive: AssetHandle<Texture>) -> Self {
        self.emissive_texture = Some(emissive);
        self
    }
}
