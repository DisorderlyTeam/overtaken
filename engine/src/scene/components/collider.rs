/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::context::RenderCtx;
use crate::core::OneOrMany;
use crate::render_world::SceneRenderData;
use crate::scene::collisions::Collider;
use crate::scene::components::{Component, ComponentTypeFlags};
use crate::scene::{EntityHandle, GlobalTransform, LocalTransform, Scene};

pub struct ColliderComponent {
    // Avoiding allocation for a single collider, but allowing multiple to avoid
    // too many game objects.
    // There may be a better way to do it.
    colliders: OneOrMany<Collider>,
}

impl ColliderComponent {
    pub fn new_single(collider: Collider) -> Self {
        Self {
            colliders: OneOrMany::One(collider),
        }
    }

    pub fn new_multiple(colliders: Vec<Collider>) -> Self {
        Self {
            colliders: OneOrMany::Many(colliders),
        }
    }

    pub fn colliders(&self) -> &[Collider] {
        self.colliders.as_slice()
    }

    pub fn colliders_mut(&mut self) -> &mut [Collider] {
        self.colliders.as_mut_slice()
    }
}

impl Component for ColliderComponent {
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

        for collider in self.colliders() {
            let drawable = collider.to_debug_drawable(global.object_to_world);

            render_data.debug_drawables.push(drawable);
        }
    }
}
