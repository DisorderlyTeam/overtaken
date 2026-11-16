/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::context::UpdateCtx;
use crate::scene::{Scene, SceneManager};
use std::time::Duration;
use tracing::{debug, warn};

pub enum SceneTransition {
    None,
    Push(Box<dyn GameSceneLogic>),
    Replace(Box<dyn GameSceneLogic>),
    PopAndReplace(Box<dyn GameSceneLogic>),
    Pop,
}

pub trait GameSceneLogic: 'static {
    fn debug_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn on_enter(&mut self, _scene: &mut Scene, _ctx: &mut UpdateCtx<'_>) {}

    fn on_exit(&mut self, _scene: &mut Scene, _ctx: &mut UpdateCtx<'_>) {}

    fn update(
        &mut self,
        _scene: &mut Scene,
        _dt: Duration,
        _ctx: &mut UpdateCtx<'_>,
    ) -> SceneTransition {
        SceneTransition::None
    }
}

pub struct GameDirector {
    scene_manager: SceneManager,
    scene_logic_stack: Vec<Box<dyn GameSceneLogic>>,
}

impl Default for GameDirector {
    fn default() -> Self {
        Self::new()
    }
}

impl GameDirector {
    pub fn new() -> Self {
        Self {
            scene_manager: SceneManager::new(),
            scene_logic_stack: Vec::with_capacity(16),
        }
    }

    pub fn scene_manager(&self) -> &SceneManager {
        &self.scene_manager
    }

    pub fn scene_manager_mut(&mut self) -> &mut SceneManager {
        &mut self.scene_manager
    }

    pub fn current_scene(&self) -> Option<&Scene> {
        self.scene_manager.current_scene()
    }

    pub fn current_scene_mut(&mut self) -> Option<&mut Scene> {
        self.scene_manager.current_scene_mut()
    }

    pub fn push_scene(&mut self, mut logic: Box<dyn GameSceneLogic>, ctx: &mut UpdateCtx<'_>) {
        let scene_name = logic.debug_name();
        let mut scene = Scene::new();
        logic.on_enter(&mut scene, ctx);
        // Ensure freshly spawned entities have valid globals before first logic update.
        scene.update_transforms();
        self.scene_manager.push_scene(scene);
        self.scene_logic_stack.push(logic);
        debug!(
            scene = scene_name,
            stack_size = self.scene_logic_stack.len(),
            "Pushed scene logic"
        );
    }

    pub fn replace_scene(&mut self, logic: Box<dyn GameSceneLogic>, ctx: &mut UpdateCtx<'_>) {
        debug!("Replacing current scene logic");
        self.pop_scene(ctx);
        self.push_scene(logic, ctx);
    }

    pub fn pop_scene(&mut self, ctx: &mut UpdateCtx<'_>) -> Option<Box<dyn GameSceneLogic>> {
        let mut logic = self.scene_logic_stack.pop()?;
        let scene_name = logic.debug_name();
        if let Some(scene) = self.scene_manager.current_scene_mut() {
            logic.on_exit(scene, ctx);
        }
        let _ = self.scene_manager.pop_scene();
        debug!(
            scene = scene_name,
            stack_size = self.scene_logic_stack.len(),
            "Popped scene logic"
        );
        Some(logic)
    }

    pub fn update_active_scene_logic(&mut self, dt: Duration, ctx: &mut UpdateCtx<'_>) {
        if self.scene_logic_stack.is_empty() {
            return;
        }

        let mut selected_indices = Vec::with_capacity(self.scene_logic_stack.len());
        for (index, scene) in self.scene_manager.all_scenes().iter().enumerate().rev() {
            selected_indices.push(index);
            if !scene.should_update_below() {
                break;
            }
        }
        selected_indices.reverse();

        let Some(top_index) = selected_indices.last().copied() else {
            return;
        };

        let mut transition = SceneTransition::None;
        {
            let scene_manager = &mut self.scene_manager;
            let scene_logic_stack = &mut self.scene_logic_stack;

            for index in selected_indices {
                let Some(scene) = scene_manager.scene_mut(index) else {
                    continue;
                };
                let Some(logic) = scene_logic_stack.get_mut(index) else {
                    continue;
                };

                let is_top_scene = index == top_index;
                ctx.is_top_scene = is_top_scene;
                let scene_transition = logic.update(scene, dt, ctx);

                if is_top_scene {
                    transition = scene_transition;
                } else if !matches!(scene_transition, SceneTransition::None) {
                    warn!(
                        scene = logic.debug_name(),
                        "Ignoring transition from non-top scene logic"
                    );
                }
            }
        }
        ctx.is_top_scene = true;

        let current_scene_name = self
            .scene_logic_stack
            .last()
            .map(|logic| logic.debug_name())
            .unwrap_or("<none>");

        match transition {
            SceneTransition::None => {}
            SceneTransition::Push(next_logic) => {
                debug!(
                    from = current_scene_name,
                    to = next_logic.debug_name(),
                    "Scene transition: push"
                );
                self.push_scene(next_logic, ctx);
            }
            SceneTransition::Replace(next_logic) => {
                debug!(
                    from = current_scene_name,
                    to = next_logic.debug_name(),
                    "Scene transition: replace"
                );
                self.replace_scene(next_logic, ctx);
            }
            SceneTransition::Pop => {
                debug!(scene = current_scene_name, "Scene transition: pop");
                let _ = self.pop_scene(ctx);
            }
            SceneTransition::PopAndReplace(next_logic) => {
                debug!(
                    from = current_scene_name,
                    to = next_logic.debug_name(),
                    "Scene transition: pop and replace below"
                );
                let _ = self.pop_scene(ctx);
                self.replace_scene(next_logic, ctx);
            }
        }
    }
}
