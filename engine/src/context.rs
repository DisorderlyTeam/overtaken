/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::assets::AssetManager;
use crate::audio::AudioManager;
use crate::engine::EngineTimings;
use crate::input::InputManager;
use crate::renderer::RenderStats;
use std::time::Duration;

/// Update-phase dependencies exposed to scenes.
pub struct UpdateCtx<'a> {
    pub assets: &'a mut AssetManager,
    pub audio: &'a mut AudioManager,
    pub inputs: &'a mut InputManager,
    pub delta_time: Duration,
    pub frame_index: u64,
    pub last_frame_timings: Option<EngineTimings>,
    pub last_frame_stats: Option<RenderStats>,
    pub shown_debug_layers: &'a mut u32,
    pub timescale: &'a mut f32,
    pub is_top_scene: bool,
}

impl<'a> UpdateCtx<'a> {
    pub fn assets(&self) -> &AssetManager {
        self.assets
    }

    pub fn assets_mut(&mut self) -> &mut AssetManager {
        self.assets
    }

    pub fn delta_time(&self) -> Duration {
        self.delta_time
    }

    pub fn delta_time_seconds(&self) -> f32 {
        self.delta_time.as_secs_f32()
    }

    pub fn timescale(&self) -> f32 {
        *self.timescale
    }

    pub fn set_timescale(&mut self, timescale: f32) {
        *self.timescale = timescale.max(0.0);
    }

    pub fn is_top_scene(&self) -> bool {
        self.is_top_scene
    }
}

/// Render/extract-phase dependencies exposed to scenes and renderer.
pub struct RenderCtx<'a> {
    pub assets: &'a mut AssetManager,
    pub shown_debug_layers: &'a mut u32,
}

impl<'a> RenderCtx<'a> {
    pub fn assets(&self) -> &AssetManager {
        self.assets
    }

    pub fn assets_mut(&mut self) -> &mut AssetManager {
        self.assets
    }
}
