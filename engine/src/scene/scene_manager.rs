/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::context::{RenderCtx, UpdateCtx};
use crate::render_world::RenderWorld;
use crate::scene::Scene;
use tracing::{instrument, warn};

pub struct SceneManager {
    stack: Vec<Scene>,
}

impl Default for SceneManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SceneManager {
    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(16),
        }
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn current_scene(&self) -> Option<&Scene> {
        self.stack.last()
    }

    pub fn current_scene_mut(&mut self) -> Option<&mut Scene> {
        self.stack.last_mut()
    }

    pub fn scene_mut(&mut self, index: usize) -> Option<&mut Scene> {
        self.stack.get_mut(index)
    }

    pub fn all_scenes(&self) -> &[Scene] {
        self.stack.as_slice()
    }

    pub fn push_scene(&mut self, scene: Scene) {
        self.stack.push(scene);
    }

    pub fn pop_scene(&mut self) -> Option<Scene> {
        self.stack.pop()
    }

    pub fn replace_scene(&mut self, new_scene: Scene) -> Option<Scene> {
        let previous = self.pop_scene();
        self.push_scene(new_scene);
        previous
    }

    #[instrument(skip_all)]
    pub fn update(&mut self, ctx: &mut UpdateCtx<'_>) {
        if self.stack.is_empty() {
            return;
        }

        let mut selected_indices = Vec::with_capacity(self.stack.len());
        for (index, scene) in self.stack.iter().enumerate().rev() {
            selected_indices.push(index);
            if !scene.should_update_below() {
                break;
            }
        }

        for index in selected_indices.into_iter().rev() {
            let scene = &mut self.stack[index];
            scene.update(ctx);
        }
    }

    #[instrument(skip_all)]
    pub fn extract(&self, render_world: &mut RenderWorld, ctx: &mut RenderCtx<'_>) {
        if self.stack.is_empty() {
            #[cfg(debug_assertions)]
            warn!("No scene left to extract! {} scenes on stack.", self.len());
            return;
        }

        let mut selected_indices = Vec::with_capacity(self.stack.len());
        let mut blocked = false;
        for (index, scene) in self.stack.iter().enumerate().rev() {
            selected_indices.push(index);
            if !scene.should_draw_below() {
                blocked = true;
                break;
            }
        }

        for index in selected_indices {
            let scene = &self.stack[index];
            scene.extract_render_data(render_world, ctx);
        }

        #[cfg(debug_assertions)]
        if !blocked {
            warn!("No scene left to extract! {} scenes on stack.", self.len());
        }
    }
}
