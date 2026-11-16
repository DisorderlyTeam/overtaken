/*
 * Copyright (c) 2026. DisorderlyTeam
 */

use crate::assets::AssetManager;
use crate::assets::audio::{Audio, AudioLoader};
use crate::assets::font::{Font, FontLoader};
use crate::assets::texture::{Texture, TextureLoader};
use crate::audio::AudioManager;
use crate::context::{RenderCtx, UpdateCtx};
use crate::game_director::GameDirector;
use crate::input::{GamepadAxis, InputManager};
use crate::render_world::RenderWorld;
use crate::renderer::{RenderStats, Renderer};
use gilrs::Gilrs;
use gilrs::ff::{BaseEffect, BaseEffectType, Effect as FfEffect, EffectBuilder, Replay, Ticks};
use glam::{UVec2, Vec2};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::field::Empty;
use tracing::{
    Span, debug, error, event, info, info_span, instrument, record_all, span, trace, warn,
};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

const DEFAULT_DELTA_TIME: Duration = Duration::from_nanos(16_666_667);
const MAX_DELTA_TIME: Duration = Duration::from_nanos(66_666_667);
const MAX_UPDATE_SUBSTEP: Duration = Duration::from_nanos(8_333_334);

macro_rules! hook_trait {
    ($name:ident, $fn_trait:ident ( $($args:ty),* )) => {
        pub trait $name: $fn_trait($($args),*) + 'static {}

        impl<T> $name for T
        where
            T: $fn_trait($($args),*) + 'static,
        {}
    };
}

hook_trait!(RendererSetupHook, FnOnce(&mut Renderer));

pub trait Game: 'static {
    fn setup(
        &mut self,
        _director: &mut GameDirector,
        _renderer: &mut Renderer,
        _ctx: &mut UpdateCtx<'_>,
    ) {
    }

    fn update(&mut self, _director: &mut GameDirector, _dt: Duration, _ctx: &mut UpdateCtx) {}
}

#[derive(Debug, Copy, Clone)]
pub struct EngineTimings {
    pub frame_handle_time: Duration,
    pub render_time: Duration,
    pub total_frame_time: Duration,
}

/// Main engine context.
pub struct Engine {
    // Keep as Option and release it explicitly in shutdown so it's dropped
    // after renderer/context teardown.
    window: Option<Arc<Window>>,
    renderer: Renderer,
    asset_manager: AssetManager,
    game_director: GameDirector,
    input_manager: InputManager,
    audio_manager: AudioManager,
    render_world: RenderWorld,
    frame_index: u64,
    last_frame_start: Option<Instant>,
    renderer_setup_hook: Option<Box<dyn RendererSetupHook>>,
    game: Option<Box<dyn Game>>,
    gilrs: Option<Gilrs>,
    last_frame_timings: Option<EngineTimings>,
    last_frame_stats: Option<RenderStats>,
    shown_debug_layers: u32,
    timescale: f32,
    ff_effect_low: Option<FfEffect>,
    ff_effect_high: Option<FfEffect>,
    ff_effect_active_gamepad: Option<gilrs::GamepadId>,
    ff_effect_playing_low: bool,
    ff_effect_playing_high: bool,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// Creates a new game engine instance.
    ///
    /// It does not itself start the event loop. The caller must call [Engine::run] explicitly.
    #[instrument]
    pub fn new() -> Engine {
        let gilrs = Gilrs::new()
            .inspect(|g| {
                info!("Gilrs gamepad support initialized successfully!");
                let gamepads = g.gamepads().collect::<Vec<_>>();
                if gamepads.is_empty() {
                    debug!("No gamepads detected!");
                } else {
                    debug!("{} gamepads detected.", gamepads.len());
                    for (id, gamepad) in gamepads {
                        debug!(
                            os_name = gamepad.os_name(),
                            map_name = gamepad.map_name(),
                            ff_supported = gamepad.is_ff_supported(),
                            "Gamepad {id}"
                        );
                    }
                }
            })
            .inspect_err(|err| {
                error!("Failed to initialize Gilrs: {}", err);
            })
            .ok();

        let mut engine = Engine {
            window: None,
            renderer: Renderer::new(),
            asset_manager: AssetManager::new(),
            game_director: GameDirector::new(),
            input_manager: InputManager::new(),
            audio_manager: AudioManager::new(),
            render_world: RenderWorld::new(),
            frame_index: 0,
            last_frame_start: None,
            renderer_setup_hook: None,
            game: None,
            gilrs,
            last_frame_timings: None,
            last_frame_stats: None,
            shown_debug_layers: 0,
            timescale: 1.0,
            ff_effect_low: None,
            ff_effect_high: None,
            ff_effect_active_gamepad: None,
            ff_effect_playing_low: false,
            ff_effect_playing_high: false,
        };

        if let Some(gilrs) = &mut engine.gilrs {
            // Two independent effects, one per motor.
            engine.ff_effect_low = Self::build_ff_effect(
                gilrs,
                BaseEffectType::Weak {
                    magnitude: u16::MAX,
                },
            )
            .ok();
            engine.ff_effect_high = Self::build_ff_effect(
                gilrs,
                BaseEffectType::Strong {
                    magnitude: u16::MAX,
                },
            )
            .ok();
        }
        engine
    }

    fn build_ff_effect(
        gilrs: &mut Gilrs,
        kind: BaseEffectType,
    ) -> Result<FfEffect, gilrs::ff::Error> {
        // Use a long-ish replay window to avoid any perceptible "pulsing".
        let scheduling = Replay {
            after: Ticks::from_ms(0),
            play_for: Ticks::from_ms(1_000),
            with_delay: Ticks::from_ms(0),
        };

        let mut builder = EffectBuilder::new();
        builder
            .add_effect(BaseEffect {
                kind,
                scheduling,
                envelope: Default::default(),
            })
            .finish(gilrs)
    }

    fn apply_rumble(&mut self, desired_strength: f32) {
        let Some(gilrs) = &mut self.gilrs else {
            return;
        };
        let effect_weak = self.ff_effect_low.as_ref();
        let effect_strong = self.ff_effect_high.as_ref();

        // Only rumble while the gamepad is the active control type.
        let allow_rumble = matches!(
            self.input_manager.last_control_type(),
            crate::input::ControlType::Gamepad(_)
        );
        let strength = if allow_rumble {
            desired_strength.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let active_gamepad = if allow_rumble {
            self.input_manager.last_gamepad_id()
        } else {
            None
        };

        // Rebind effect if active gamepad changed. This also prevents "stuck rumble" on the old pad.
        if active_gamepad != self.ff_effect_active_gamepad {
            // Best-effort stop before rebinding.
            if let Some(effect_weak) = effect_weak
                && let Err(err) = effect_weak.stop()
            {
                warn!(?err, "Force feedback stop (weak) failed");
            }
            if let Some(effect_strong) = effect_strong
                && let Err(err) = effect_strong.stop()
            {
                warn!(?err, "Force feedback stop (strong) failed");
            }
            self.ff_effect_playing_low = false;
            self.ff_effect_playing_high = false;
            self.ff_effect_active_gamepad = active_gamepad;

            if let Some(id) = active_gamepad {
                if let Some(effect_weak) = effect_weak
                    && let Err(err) = effect_weak.set_gamepads(&[id], gilrs)
                {
                    warn!(
                        ?err,
                        "Failed to bind force feedback (weak) to active gamepad"
                    );
                }
                if let Some(effect_strong) = effect_strong
                    && let Err(err) = effect_strong.set_gamepads(&[id], gilrs)
                {
                    warn!(
                        ?err,
                        "Failed to bind force feedback (strong) to active gamepad"
                    );
                }
            }
        }

        let Some(effect_weak) = effect_weak else {
            return;
        };
        let Some(effect_strong) = effect_strong else {
            return;
        };

        // No gain: only motor selection + duration. Any non-zero strength means "on".
        let (want_weak, want_strong) = if strength > 0.0 {
            self.input_manager.rumble_motors_active()
        } else {
            (false, false)
        };

        if want_weak {
            if !self.ff_effect_playing_low {
                if let Err(err) = effect_weak.play() {
                    warn!(?err, "Force feedback play (weak) failed");
                } else {
                    self.ff_effect_playing_low = true;
                }
            }
        } else if self.ff_effect_playing_low {
            if let Err(err) = effect_weak.stop() {
                warn!(?err, "Force feedback stop (weak) failed");
            }
            self.ff_effect_playing_low = false;
        }

        if want_strong {
            if !self.ff_effect_playing_high {
                if let Err(err) = effect_strong.play() {
                    warn!(?err, "Force feedback play (strong) failed");
                } else {
                    self.ff_effect_playing_high = true;
                }
            }
        } else if self.ff_effect_playing_high {
            if let Err(err) = effect_strong.stop() {
                warn!(?err, "Force feedback stop (strong) failed");
            }
            self.ff_effect_playing_high = false;
        }
    }

    /// Runs the game engine event loop
    #[instrument(skip(self))]
    pub fn run(&mut self) {
        let event_loop = EventLoop::with_user_event().build().unwrap();
        event_loop.run_app(self).unwrap();
    }

    pub fn set_renderer_setup_hook(&mut self, hook: impl RendererSetupHook) {
        self.renderer_setup_hook = Some(Box::new(hook));
    }

    pub fn with_renderer_setup_hook(mut self, hook: impl RendererSetupHook) -> Self {
        self.set_renderer_setup_hook(hook);
        self
    }

    pub fn set_game(&mut self, game: impl Game) {
        self.game = Some(Box::new(game));
    }

    pub fn with_game(mut self, game: impl Game) -> Self {
        self.set_game(game);
        self
    }

    pub fn timescale(&self) -> f32 {
        self.timescale
    }

    pub fn set_timescale(&mut self, timescale: f32) {
        self.timescale = timescale.max(0.0);
    }

    fn request_exit(&self, event_loop: &ActiveEventLoop) {
        event_loop.exit();
    }

    fn shutdown(&mut self) {
        let mut shutdown_ctx = UpdateCtx {
            assets: &mut self.asset_manager,
            audio: &mut self.audio_manager,
            inputs: &mut self.input_manager,
            delta_time: Duration::ZERO,
            frame_index: self.frame_index,
            last_frame_timings: self.last_frame_timings,
            last_frame_stats: self.last_frame_stats,
            shown_debug_layers: &mut self.shown_debug_layers,
            timescale: &mut self.timescale,
            is_top_scene: true,
        };
        while self.game_director.pop_scene(&mut shutdown_ctx).is_some() {}
        self.game = None;
        self.renderer_setup_hook = None;

        self.asset_manager.clear();
        self.renderer.destroy();
        self.window = None;
    }

    fn duration_ms(duration: Duration) -> f64 {
        duration.as_secs_f64() * 1000.0
    }

    fn compute_delta_time(total_frame_time: Option<Duration>) -> Duration {
        total_frame_time
            .unwrap_or(DEFAULT_DELTA_TIME)
            .min(MAX_DELTA_TIME)
    }

    fn split_update_steps(delta_time: Duration) -> Vec<Duration> {
        let step_count = (delta_time
            .as_nanos()
            .div_ceil(MAX_UPDATE_SUBSTEP.as_nanos()))
        .max(1)
        .min(u32::MAX as u128) as u32;
        let base_nanos = delta_time.as_nanos() / step_count as u128;
        let remainder = delta_time.as_nanos() % step_count as u128;

        let mut steps = Vec::with_capacity(step_count as usize);
        for i in 0..step_count {
            let nanos = base_nanos + u128::from(i < remainder as u32);
            steps.push(Duration::from_nanos(nanos as u64));
        }
        steps
    }
}

impl ApplicationHandler<Renderer> for Engine {
    #[instrument(level = "debug", skip_all)]
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let mut window_attr = Window::default_attributes();
        window_attr.fullscreen = Some(Fullscreen::Borderless(None));

        let window = Arc::new(event_loop.create_window(window_attr).unwrap());

        self.renderer.init_wgpu(window.clone());
        if let Some(renderer_setup) = self.renderer_setup_hook.take() {
            let _span = info_span!("renderer_setup_hook").entered();
            renderer_setup(&mut self.renderer);
        }

        self.asset_manager
            .register_loader::<Texture, _>(TextureLoader::new(
                self.renderer.get_context().unwrap().device.clone(),
                self.renderer.get_context().unwrap().queue.clone(),
            ));

        self.asset_manager
            .register_loader::<Font, _>(FontLoader::new(
                self.renderer.get_context().unwrap().font_system.clone(),
            ));

        self.asset_manager
            .register_loader::<Audio, _>(AudioLoader::new());

        if let Some(game) = &mut self.game {
            let mut setup_ctx = UpdateCtx {
                assets: &mut self.asset_manager,
                audio: &mut self.audio_manager,
                inputs: &mut self.input_manager,
                delta_time: Duration::ZERO,
                frame_index: self.frame_index,
                last_frame_timings: self.last_frame_timings,
                last_frame_stats: self.last_frame_stats,
                shown_debug_layers: &mut self.shown_debug_layers,
                timescale: &mut self.timescale,
                is_top_scene: true,
            };
            game.setup(&mut self.game_director, &mut self.renderer, &mut setup_ctx);
        }

        self.window = Some(window);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        // self.renderer = Some(pollster::block_on(Renderer::init_wgpu(&window)));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.request_exit(event_loop);
            }
            WindowEvent::Resized(size) => self.renderer.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                let _span = info_span!("frame", id = self.frame_index).entered();

                let frame_start = Instant::now();
                let total_frame_time = self
                    .last_frame_start
                    .map(|last| frame_start.duration_since(last));
                self.last_frame_start = Some(frame_start);
                let delta_time = Self::compute_delta_time(total_frame_time);
                let scaled_delta_time =
                    Duration::from_secs_f32(delta_time.as_secs_f32() * self.timescale);

                if let Some(gilrs) = &mut self.gilrs {
                    while let Some(gilrs_event) = gilrs.next_event() {
                        self.input_manager
                            .handle_gamepad_event(&*gilrs, gilrs_event);
                    }
                }

                {
                    for step_dt in Self::split_update_steps(scaled_delta_time) {
                        let mut update_ctx = UpdateCtx {
                            assets: &mut self.asset_manager,
                            audio: &mut self.audio_manager,
                            inputs: &mut self.input_manager,
                            delta_time: step_dt,
                            frame_index: self.frame_index,
                            last_frame_timings: self.last_frame_timings,
                            last_frame_stats: self.last_frame_stats,
                            shown_debug_layers: &mut self.shown_debug_layers,
                            timescale: &mut self.timescale,
                            is_top_scene: true,
                        };

                        if let Some(game) = &mut self.game {
                            let _span = info_span!("game_update_hook").entered();
                            game.update(&mut self.game_director, step_dt, &mut update_ctx);
                        }

                        self.game_director
                            .update_active_scene_logic(step_dt, &mut update_ctx);
                        self.game_director
                            .scene_manager_mut()
                            .update(&mut update_ctx);

                        // Apply rumble after logic updates so newly requested rumble is immediate.
                        // Any active request enables rumble for at least one motor.
                        let (weak, strong) = self.input_manager.rumble_motors_active();
                        let rumble_on = weak || strong;
                        self.apply_rumble(if rumble_on { 1.0 } else { 0.0 });
                        self.input_manager.tick_rumble(step_dt);
                        self.input_manager.end_step();
                    }
                }

                let mut render_ctx = RenderCtx {
                    assets: &mut self.asset_manager,
                    shown_debug_layers: &mut self.shown_debug_layers,
                };

                {
                    let render_resolution = self
                        .renderer
                        .context
                        .as_ref()
                        .map(|ctx| ctx.virtual_resolution.max(Vec2::ONE).as_uvec2())
                        .unwrap_or(UVec2::ONE);
                    self.render_world
                        .begin_frame(self.frame_index, render_resolution);
                    self.game_director
                        .scene_manager()
                        .extract(&mut self.render_world, &mut render_ctx);
                }

                let render_start = Instant::now();
                let render_result = {
                    let (renderer) = &mut self.renderer;
                    renderer.render(&mut render_ctx, &self.render_world)
                };
                let render_time = render_start.elapsed();
                let frame_handle_time = frame_start.elapsed();

                match render_result {
                    Err(err) => {
                        error!("Render failed: {err}");
                        self.request_exit(event_loop);
                    }
                    Ok(render_stats) => {
                        self.last_frame_stats = Some(render_stats);
                        self.last_frame_timings = Some(EngineTimings {
                            frame_handle_time,
                            render_time,
                            total_frame_time: total_frame_time.unwrap_or_default(),
                        });

                        if cfg!(debug_assertions) {
                            trace!(?render_stats, "Render stats");
                            trace!(
                                ?frame_handle_time,
                                ?render_time,
                                ?total_frame_time,
                                ?delta_time,
                                "Frame timings",
                            );
                        }

                        if let Some(window) = &self.window {
                            self.frame_index += 1;
                            window.request_redraw();
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.input_manager.handle_keyboard_event(event);
            }
            WindowEvent::Focused(focused) => {
                if let Some(window) = &self.window {
                    window.set_cursor_visible(!focused);

                    // if(focused) {
                    //     let cursor_lock = window.set_cursor_grab(CursorGrabMode::Locked)
                    //         .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
                    //
                    //     if let Err(err) = cursor_lock {
                    //         error!("Failed to lock/confine cursor: {}", err);
                    //     }
                    // } else {
                    //     let cursor_lock = window.set_cursor_grab(CursorGrabMode::None);
                    //
                    //     if let Err(err) = cursor_lock {
                    //         error!("Failed to unlock cursor: {}", err);
                    //     }
                    // }
                }
            }
            _ => {}
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.shutdown();
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}
