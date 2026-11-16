/*
 * Copyright (c) 2026. DisorderlyTeam
 */
pub mod entities;
mod game_context;
pub mod level;
pub mod managers;
pub mod scenes;
pub mod time;

use crate::game_context::{GameContext, SharedGameContext};
use crate::level::LevelData;
use crate::managers::save_manager::SaveManager;
use engine::context::UpdateCtx;
use engine::engine::{Engine, Game};
use engine::game_director::GameDirector;
use engine::input::{ActionMapping, AxisMapping, GamepadAxis, GamepadButton, KeyboardKey};
use engine::renderer::postprocessing::color_grading::{
    ColorGradingParams, ColorGradingPostProcess,
};
use engine::scene::LocalTransform;
use entities::player::PlayerComponent;
use scenes::map_scene::MapSceneLogic;
use scenes::title_scene::TitleSceneLogic;
use scenes::win_scene::WinSceneLogic;
use std::env::args;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{error, info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt::Layer as FmtLayer};

struct OvertakenGame {
    color_grading_params: Option<Arc<Mutex<ColorGradingParams>>>,
    death_filter_mix: f32,
    game_context: SharedGameContext,
    auto_load_level: Option<String>,
}

const DEATH_FILTER_BLEND_SPEED: f32 = 5.0;
const NORMAL_COLOR_GRADING: ColorGradingParams = ColorGradingParams {
    exposure: 1.0,
    contrast: 1.0,
    saturation: 1.0,
    brightness: 0.0,
    tint: [1.0, 1.0, 1.0, 1.0],
};
const DEAD_COLOR_GRADING: ColorGradingParams = ColorGradingParams {
    exposure: 0.8,
    contrast: 1.0,
    saturation: 0.85,
    brightness: 0.0,
    tint: [1.0, 0.0, 0.0, 1.0],
};

impl OvertakenGame {
    fn new(auto_load_level: Option<String>) -> Self {
        Self {
            color_grading_params: None,
            death_filter_mix: 0.0,
            game_context: GameContext::shared(),
            auto_load_level,
        }
    }

    fn set_color_grading_params(&self, params: ColorGradingParams) {
        let Some(shared) = &self.color_grading_params else {
            return;
        };
        if let Ok(mut guard) = shared.lock() {
            *guard = params;
        }
    }

    fn update_death_filter(
        &mut self,
        director: &mut GameDirector,
        dt: Duration,
        ctx: &mut UpdateCtx<'_>,
    ) {
        if ctx.inputs.is_action_just_pressed("reset") {
            self.death_filter_mix = 0.0;
            self.set_color_grading_params(NORMAL_COLOR_GRADING);
            return;
        }

        let mut death_effect_target = 0.0;
        let mut player_death_state: Option<bool> = None;
        if let Some(scene) = director.current_scene_mut()
            && let Some(player) = scene
                .entities_with_component::<LocalTransform>()
                .into_iter()
                .find(|entity| scene.entity_name(*entity) == Some("player"))
        {
            player_death_state = scene
                .get_component::<PlayerComponent>(player)
                .map(PlayerComponent::is_dead);
            if player_death_state == Some(true) {
                death_effect_target = 1.0;
            }
        }

        if player_death_state == Some(false) {
            self.death_filter_mix = 0.0;
            self.set_color_grading_params(NORMAL_COLOR_GRADING);
            return;
        }

        let blend = 1.0 - (-DEATH_FILTER_BLEND_SPEED * dt.as_secs_f32()).exp();
        self.death_filter_mix += (death_effect_target - self.death_filter_mix) * blend;
        self.death_filter_mix = self.death_filter_mix.clamp(0.0, 1.0);

        let mix = self.death_filter_mix;
        let lerp = |a: f32, b: f32| a + (b - a) * mix;
        let params = ColorGradingParams {
            exposure: lerp(NORMAL_COLOR_GRADING.exposure, DEAD_COLOR_GRADING.exposure),
            contrast: lerp(NORMAL_COLOR_GRADING.contrast, DEAD_COLOR_GRADING.contrast),
            saturation: lerp(
                NORMAL_COLOR_GRADING.saturation,
                DEAD_COLOR_GRADING.saturation,
            ),
            brightness: lerp(
                NORMAL_COLOR_GRADING.brightness,
                DEAD_COLOR_GRADING.brightness,
            ),
            tint: [
                lerp(NORMAL_COLOR_GRADING.tint[0], DEAD_COLOR_GRADING.tint[0]),
                lerp(NORMAL_COLOR_GRADING.tint[1], DEAD_COLOR_GRADING.tint[1]),
                lerp(NORMAL_COLOR_GRADING.tint[2], DEAD_COLOR_GRADING.tint[2]),
                lerp(NORMAL_COLOR_GRADING.tint[3], DEAD_COLOR_GRADING.tint[3]),
            ],
        };
        self.set_color_grading_params(params);
    }
}

impl Drop for OvertakenGame {
    fn drop(&mut self) {
        if let Ok(mut context) = self.game_context.try_borrow_mut() {
            context.ensure_config_saved();
        }
    }
}

impl Game for OvertakenGame {
    fn setup(
        &mut self,
        director: &mut GameDirector,
        renderer: &mut engine::renderer::Renderer,
        ctx: &mut UpdateCtx,
    ) {
        let grading = ColorGradingPostProcess::new();
        self.color_grading_params = Some(grading.params_handle());
        self.death_filter_mix = 0.0;
        self.set_color_grading_params(NORMAL_COLOR_GRADING);

        renderer.add_post_process(Box::new(grading));

        self.game_context
            .borrow()
            .apply_to_audio_input(ctx.audio, ctx.inputs);

        ctx.inputs
            .add_action_mapping("menu_start", ActionMapping::KeyboardKey(KeyboardKey::Z));
        ctx.inputs
            .add_action_mapping("menu_start", ActionMapping::KeyboardKey(KeyboardKey::Enter));
        ctx.inputs
            .add_action_mapping("menu_start", ActionMapping::KeyboardKey(KeyboardKey::Space));
        ctx.inputs.add_action_mapping(
            "menu_start",
            ActionMapping::GamepadButton(GamepadButton::Start),
        );
        ctx.inputs.add_action_mapping(
            "menu_start",
            ActionMapping::GamepadButton(GamepadButton::South),
        );
        ctx.inputs
            .add_action_mapping("overtake", ActionMapping::KeyboardKey(KeyboardKey::Z));
        ctx.inputs
            .add_action_mapping("overtake", ActionMapping::KeyboardKey(KeyboardKey::E));
        ctx.inputs.add_action_mapping(
            "overtake",
            ActionMapping::GamepadButton(GamepadButton::West),
        );
        ctx.inputs
            .add_action_mapping("cancel", ActionMapping::KeyboardKey(KeyboardKey::X));
        ctx.inputs
            .add_action_mapping("cancel", ActionMapping::KeyboardKey(KeyboardKey::Q));
        ctx.inputs
            .add_action_mapping("cancel", ActionMapping::GamepadButton(GamepadButton::East));
        ctx.inputs
            .add_action_mapping("reset", ActionMapping::KeyboardKey(KeyboardKey::R));
        ctx.inputs
            .add_action_mapping("reset", ActionMapping::GamepadButton(GamepadButton::Select));

        ctx.inputs.add_action_mapping(
            "menu_select",
            ActionMapping::KeyboardKey(KeyboardKey::Enter),
        );
        ctx.inputs.add_action_mapping(
            "menu_select",
            ActionMapping::KeyboardKey(KeyboardKey::Space),
        );
        ctx.inputs
            .add_action_mapping("menu_select", ActionMapping::KeyboardKey(KeyboardKey::E));
        ctx.inputs.add_action_mapping(
            "menu_select",
            ActionMapping::GamepadButton(GamepadButton::South),
        );
        ctx.inputs.add_action_mapping(
            "menu_pause",
            ActionMapping::KeyboardKey(KeyboardKey::Escape),
        );
        ctx.inputs
            .add_action_mapping("menu_pause", ActionMapping::KeyboardKey(KeyboardKey::P));
        ctx.inputs.add_action_mapping(
            "menu_pause",
            ActionMapping::GamepadButton(GamepadButton::Start),
        );

        ctx.inputs
            .add_action_mapping("menu_up", ActionMapping::KeyboardKey(KeyboardKey::W));
        ctx.inputs
            .add_action_mapping("menu_up", ActionMapping::KeyboardKey(KeyboardKey::Up));
        ctx.inputs.add_action_mapping(
            "menu_up",
            ActionMapping::GamepadButton(GamepadButton::DPadUp),
        );
        ctx.inputs.add_action_mapping(
            "menu_up",
            ActionMapping::GamepadAxis {
                axis: GamepadAxis::LeftStickY,
                deadzone: 0.5,
            },
        );
        ctx.inputs
            .add_action_mapping("menu_down", ActionMapping::KeyboardKey(KeyboardKey::S));
        ctx.inputs
            .add_action_mapping("menu_down", ActionMapping::KeyboardKey(KeyboardKey::Down));
        ctx.inputs.add_action_mapping(
            "menu_down",
            ActionMapping::GamepadButton(GamepadButton::DPadDown),
        );
        ctx.inputs.add_action_mapping(
            "menu_down",
            ActionMapping::GamepadAxis {
                axis: GamepadAxis::LeftStickY,
                deadzone: -0.5,
            },
        );
        ctx.inputs
            .add_action_mapping("menu_left", ActionMapping::KeyboardKey(KeyboardKey::A));
        ctx.inputs
            .add_action_mapping("menu_left", ActionMapping::KeyboardKey(KeyboardKey::Left));
        ctx.inputs.add_action_mapping(
            "menu_left",
            ActionMapping::GamepadButton(GamepadButton::DPadLeft),
        );
        ctx.inputs.add_action_mapping(
            "menu_left",
            ActionMapping::GamepadAxis {
                axis: GamepadAxis::LeftStickX,
                deadzone: -0.5,
            },
        );
        ctx.inputs
            .add_action_mapping("menu_right", ActionMapping::KeyboardKey(KeyboardKey::D));
        ctx.inputs
            .add_action_mapping("menu_right", ActionMapping::KeyboardKey(KeyboardKey::Right));
        ctx.inputs.add_action_mapping(
            "menu_right",
            ActionMapping::GamepadButton(GamepadButton::DPadRight),
        );
        ctx.inputs.add_action_mapping(
            "menu_right",
            ActionMapping::GamepadAxis {
                axis: GamepadAxis::LeftStickX,
                deadzone: 0.5,
            },
        );

        ctx.inputs.add_axis_mapping(
            "move_x",
            AxisMapping::KeyboardButtons {
                negative: KeyboardKey::Left,
                positive: KeyboardKey::Right,
            },
        );
        ctx.inputs.add_axis_mapping(
            "move_x",
            AxisMapping::KeyboardButtons {
                negative: KeyboardKey::A,
                positive: KeyboardKey::D,
            },
        );
        ctx.inputs.add_axis_mapping(
            "move_y",
            AxisMapping::KeyboardButtons {
                negative: KeyboardKey::W,
                positive: KeyboardKey::S,
            },
        );
        ctx.inputs.add_axis_mapping(
            "move_y",
            AxisMapping::KeyboardButtons {
                negative: KeyboardKey::Up,
                positive: KeyboardKey::Down,
            },
        );

        ctx.inputs.add_axis_mapping(
            "move_x",
            AxisMapping::GamepadAxis {
                axis: GamepadAxis::LeftStickX,
                inverted: false,
            },
        );
        ctx.inputs.add_axis_mapping(
            "move_y",
            AxisMapping::GamepadAxis {
                axis: GamepadAxis::LeftStickY,
                inverted: true,
            },
        );

        ctx.inputs.add_axis_mapping(
            "move_x",
            AxisMapping::GamepadButtons {
                negative: GamepadButton::DPadLeft,
                positive: GamepadButton::DPadRight,
            },
        );
        ctx.inputs.add_axis_mapping(
            "move_y",
            AxisMapping::GamepadButtons {
                negative: GamepadButton::DPadUp,
                positive: GamepadButton::DPadDown,
            },
        );

        {
            let mut context = self.game_context.borrow_mut();
            info!("Languages available: {:?}", context.locales.languages());

            let mut lang_id = if let Ok(lang_env) = std::env::var("OVERTAKEN_LANG") {
                lang_env
            } else {
                context.config.lang_id().to_string()
            };
            if context.locales.get_language(&lang_id).is_none() {
                warn!("Language of ID {} not found", &lang_id);
                warn!("Falling back to English");
                lang_id = "en".to_string();
                context.config.set_lang_id("en");
                context.config.ensure_saved();
            }

            context.locales.load_language(&lang_id);
        }

        if let Some(level_name) = self.auto_load_level.as_ref() {
            if level_name == "win" {
                director.push_scene(Box::new(WinSceneLogic::new(self.game_context.clone())), ctx);
                return;
            }
            let level_path = format!("assets/data/{}.json", level_name);
            let level_data = match LevelData::from_file(&level_path) {
                Ok(data) => data,
                Err(err) => {
                    panic!("Failed to load level: {}", err);
                }
            };
            let level = match level_data.load(ctx.assets) {
                Ok(level) => level,
                Err(err) => {
                    panic!("Failed to build level {level_name}: {err}")
                }
            };
            director.push_scene(
                Box::new(MapSceneLogic::new(
                    level,
                    level_path.clone(),
                    self.game_context.clone(),
                )),
                ctx,
            );
        } else {
            director.push_scene(
                Box::new(TitleSceneLogic::new(self.game_context.clone())),
                ctx,
            );
        }
    }

    fn update(&mut self, director: &mut GameDirector, dt: Duration, ctx: &mut UpdateCtx) {
        self.update_death_filter(director, dt, ctx);
    }
}

fn main() {
    #[cfg(feature = "tracy")]
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(tracing_tracy::TracyLayer::default())
        .with(FmtLayer::new())
        .init();
    #[cfg(not(feature = "tracy"))]
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(FmtLayer::new())
        .init();

    info!(version = env!("CARGO_PKG_VERSION"), "Starting Overtaken!");

    #[cfg(debug_assertions)]
    let level = {
        let args = args();

        if args.len() >= 2 {
            let level = args.skip(1).next().unwrap();

            info!("Debug starting level: {}", level);

            Some(level)
        } else {
            None
        }
    };
    #[cfg(not(debug_assertions))]
    let level = None;

    let mut engine = Engine::new().with_game(OvertakenGame::new(level));

    engine.run();

    info!("Engine exited.");
}
