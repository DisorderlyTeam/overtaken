/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use crate::context::RenderCtx;
use crate::render_world::{
    DEBUG_DRAW_LAYER_GENERIC, DebugDrawable, DebugDrawableInstance, SceneRenderData,
};
use crate::scene::components::transforms::LocalTransform;
use crate::scene::components::{Component, ComponentTypeFlags};
use crate::scene::{EntityHandle, GlobalTransform, Scene};

#[derive(Debug, Clone, PartialEq)]
pub struct DebugDrawComponent {
    pub drawable: DebugDrawable,
}

impl Component for DebugDrawComponent {
    fn flags() -> ComponentTypeFlags {
        ComponentTypeFlags::NEEDS_DEBUG_DRAW
    }

    fn extract_render_data(
        &self,
        scene: &Scene,
        entity: EntityHandle,
        render_data: &mut SceneRenderData,
        ctx: &mut RenderCtx<'_>,
    ) {
        if *ctx.shown_debug_layers == 0 {
            return;
        }

        let Some(local) = scene.get_component::<LocalTransform>(entity) else {
            return;
        };
        let Some(global) = scene.get_component::<GlobalTransform>(entity) else {
            return;
        };

        let vertices = self.drawable.shape.outline_vertices();

        render_data.debug_drawables.push(DebugDrawableInstance {
            object_to_world: global.object_to_world,
            depth: local.position.z,
            color: self.drawable.color,
            vertices,
            space: local.space(),
            layer: DEBUG_DRAW_LAYER_GENERIC,
        });
    }
}
