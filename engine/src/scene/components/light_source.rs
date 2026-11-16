/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::context::RenderCtx;
use crate::core::color::Color;
use crate::render_world::{
    DEBUG_DRAW_LAYER_LIGHTS, DebugDrawableInstance, DebugShape, LightSource, SceneRenderData,
};
use crate::scene::components::{Component, ComponentTypeFlags};
use crate::scene::{EntityHandle, GlobalTransform, LocalTransform, Scene};
use glam::{Mat4, Vec2};
use std::f32::consts::TAU;

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct LightSourceComponent {
    pub position: Vec2,
    pub radius: f32,
    pub color: Color,
    pub intensity: f32,
    pub direction: Vec2,
    /// Full spotlight cone angle in radians.
    ///
    /// `TAU` means full circle (point light), `PI` means half circle.
    pub angle: f32,
    pub lighting_layer: u8,
}

impl LightSourceComponent {
    pub fn new_spotlight(
        position: Vec2,
        radius: f32,
        color: Color,
        intensity: f32,
        direction: Vec2,
        angle: f32,
    ) -> Self {
        debug_assert!(direction.is_normalized(), "direction should be normalized");
        Self {
            position,
            radius,
            color,
            intensity,
            direction,
            angle,
            lighting_layer: 0,
        }
    }

    pub fn new_pointlight(position: Vec2, radius: f32, color: Color, intensity: f32) -> Self {
        Self {
            position,
            radius,
            color,
            intensity,
            direction: Vec2::X,
            angle: TAU,
            lighting_layer: 0,
        }
    }

    pub fn with_lighting_layer(mut self, layer: u8) -> Self {
        self.lighting_layer = layer;
        self
    }
}

impl Component for LightSourceComponent {
    fn flags() -> ComponentTypeFlags {
        ComponentTypeFlags::NEEDS_DRAW
    }

    fn extract_render_data(
        &self,
        scene: &Scene,
        entity: EntityHandle,
        render_data: &mut SceneRenderData,
        ctx: &mut RenderCtx<'_>,
    ) {
        let Some(global) = scene.get_component::<GlobalTransform>(entity) else {
            return;
        };
        let position = global
            .object_to_world
            .transform_point3(self.position.extend(0.0))
            .truncate();
        let world_direction = global
            .object_to_world
            .transform_vector3(self.direction.extend(0.0))
            .truncate();
        let world_direction = if world_direction.length_squared() > 1e-10 {
            world_direction.normalize()
        } else {
            Vec2::X
        };
        let layer_index = self.lighting_layer as usize;
        if let Some(layer) = render_data.lights_by_layer.get_mut(layer_index) {
            layer.push(LightSource::new(
                position,
                self.radius,
                self.color,
                self.intensity,
                world_direction,
                self.angle,
                self.lighting_layer,
            ));
        }

        if *ctx.shown_debug_layers != 0 {
            let (depth, space) = scene
                .get_component::<LocalTransform>(entity)
                .map(|local| (local.position.z, local.space()))
                .unwrap_or((0.0, crate::render_world::SpriteSpace::World));
            let shape = if self.angle >= TAU - 1e-3 {
                DebugShape::Circle {
                    radius: self.radius,
                    segments: 32,
                }
            } else {
                DebugShape::Arc {
                    radius: self.radius,
                    direction: world_direction,
                    angle: self.angle,
                    segments: 24,
                }
            };

            render_data.debug_drawables.push(DebugDrawableInstance {
                object_to_world: Mat4::from_translation(position.extend(0.0)),
                depth,
                color: self.color.to_vec4(),
                vertices: shape.outline_vertices(),
                space,
                layer: DEBUG_DRAW_LAYER_LIGHTS,
            });
        }
    }
}
