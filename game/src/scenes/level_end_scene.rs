/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use crate::game_context::SharedGameContext;
use crate::level::LevelData;
use crate::managers::locale_manager::format_timer;
use crate::scenes::map_scene::MapSceneLogic;
use crate::scenes::title_scene::TitleSceneLogic;
use crate::scenes::win_scene::WinSceneLogic;
use crate::time::GameTime;
use engine::assets::font::Font;
use engine::context::UpdateCtx;
use engine::game_director::{GameSceneLogic, SceneTransition};
use engine::input::ControlType;
use engine::renderer::BASE_VIRTUAL_WIDTH;
use engine::scene::{
    EntityHandle, LocalTransform, Scene, TextAnchor, TextComponent, TextLayout, TextStyle,
};
use std::time::Duration;
use tracing::error;

pub struct LevelEndSceneLogic {
    game_context: SharedGameContext,
    level_id: String,
    time: GameTime,
    best_time: Option<GameTime>,
    next_level: Option<String>,
    last_control_type: Option<ControlType>,
    restart_entity: Option<EntityHandle>,
    next_entity: Option<EntityHandle>,
    title_entity: Option<EntityHandle>,
}

impl LevelEndSceneLogic {
    pub fn new(
        game_context: SharedGameContext,
        level_id: impl Into<String>,
        time: GameTime,
    ) -> Self {
        let level_id = level_id.into();
        let (next_level, best_time) = {
            let ctx = game_context.borrow();
            let next_level = ctx
                .levels
                .try_get_next_level(&level_id)
                .map(|it| it.to_string());
            let best_time = ctx
                .saves
                .save_data()
                .level_data(&level_id)
                .and_then(|it| it.best_time);
            (next_level, best_time)
        };
        Self {
            game_context,
            level_id,
            time,
            best_time,
            next_level,
            last_control_type: None,
            restart_entity: None,
            next_entity: None,
            title_entity: None,
        }
    }

    fn refresh_prompts(&mut self, scene: &mut Scene, ctx: &UpdateCtx<'_>) {
        let control_type = ctx.inputs.last_control_type();
        if self.last_control_type != Some(control_type)
            && let Some(restart_entity) = self.restart_entity
            && let Some(next_entity) = self.next_entity
            && let Some(title_entity) = self.title_entity
        {
            let locales = &self.game_context.borrow().locales;

            scene
                .get_component_mut::<TextComponent>(restart_entity)
                .unwrap()
                .set_text(format!(
                    "{} {}",
                    locales.get_prompt_for_action(ctx.inputs, "reset", control_type),
                    locales.get_string("completion.try_again"),
                ));

            scene
                .get_component_mut::<TextComponent>(next_entity)
                .unwrap()
                .set_text(format!(
                    "{} {}",
                    locales.get_prompt_for_action(ctx.inputs, "menu_select", control_type),
                    locales.get_string(if self.next_level.is_some() {
                        "completion.next_level"
                    } else {
                        "completion.end_game"
                    }),
                ));

            scene
                .get_component_mut::<TextComponent>(title_entity)
                .unwrap()
                .set_text(format!(
                    "{} {}",
                    locales.get_prompt_for_action(ctx.inputs, "cancel", control_type),
                    locales.get_string("completion.back_to_title"),
                ));
        }
    }
}

impl GameSceneLogic for LevelEndSceneLogic {
    fn on_enter(&mut self, scene: &mut Scene, ctx: &mut UpdateCtx<'_>) {
        let game_ctx = self.game_context.borrow();
        let text_font = ctx.assets.get_handle::<Font>("Tiny5").unwrap();
        let heading_font = ctx.assets.get_handle::<Font>("Tiny5Duo").unwrap();

        scene.spawn_text_entity(
            LocalTransform::from_xyz_screenspace(0.0, -70.0, 10.0),
            TextComponent::new(
                game_ctx.locales.get_string("completion.title"),
                TextStyle {
                    font: heading_font,
                    line_height: None,
                    scale: 2.0,
                    color: [255, 255, 255, 255],
                },
                TextLayout {
                    max_width: Some(BASE_VIRTUAL_WIDTH as f32),
                    max_height: None,
                    anchor: TextAnchor::HorizonCenter,
                },
            ),
        );

        scene.spawn_text_entity(
            LocalTransform::from_xyz_screenspace(0.0, 0.0, 10.0),
            TextComponent::new(
                game_ctx.locales.get_string_ext(
                    "completion.text",
                    [
                        ("time".into(), format_timer(self.time.into())),
                        (
                            "best_time".into(),
                            self.best_time
                                .map(|it| format_timer(it.into()))
                                .unwrap_or_default(),
                        ),
                    ],
                ),
                TextStyle {
                    font: text_font,
                    line_height: None,
                    scale: 1.0,
                    color: [255, 255, 255, 255],
                },
                TextLayout {
                    max_width: Some(BASE_VIRTUAL_WIDTH as f32),
                    max_height: None,
                    anchor: TextAnchor::HorizonCenter,
                },
            ),
        );

        self.restart_entity = Some(scene.spawn_text_entity(
            LocalTransform::from_xyz_screenspace(0.0, 60.0, 10.0),
            TextComponent::new(
                game_ctx.locales.get_string("completion.try_again"),
                TextStyle {
                    font: text_font,
                    line_height: None,
                    scale: 1.0,
                    color: [255, 255, 255, 255],
                },
                TextLayout {
                    max_width: Some(BASE_VIRTUAL_WIDTH as f32),
                    max_height: None,
                    anchor: TextAnchor::HorizonCenter,
                },
            ),
        ));

        self.next_entity = Some(scene.spawn_text_entity(
            LocalTransform::from_xyz_screenspace(0.0, 70.0, 10.0),
            TextComponent::new(
                game_ctx.locales.get_string(if self.next_level.is_some() {
                    "completion.next_level"
                } else {
                    "completion.end_game"
                }),
                TextStyle {
                    font: text_font,
                    line_height: None,
                    scale: 1.0,
                    color: [255, 255, 255, 255],
                },
                TextLayout {
                    max_width: Some(BASE_VIRTUAL_WIDTH as f32),
                    max_height: None,
                    anchor: TextAnchor::HorizonCenter,
                },
            ),
        ));

        self.title_entity = Some(scene.spawn_text_entity(
            LocalTransform::from_xyz_screenspace(0.0, 80.0, 10.0),
            TextComponent::new(
                game_ctx.locales.get_string("completion.back_to_title"),
                TextStyle {
                    font: text_font,
                    line_height: None,
                    scale: 1.0,
                    color: [255, 255, 255, 192],
                },
                TextLayout {
                    max_width: Some(BASE_VIRTUAL_WIDTH as f32),
                    max_height: None,
                    anchor: TextAnchor::HorizonCenter,
                },
            ),
        ));
    }

    fn update(
        &mut self,
        scene: &mut Scene,
        _dt: Duration,
        ctx: &mut UpdateCtx<'_>,
    ) -> SceneTransition {
        self.refresh_prompts(scene, ctx);

        if ctx.inputs.is_action_just_pressed("reset") {
            let level_data = match LevelData::from_id(&self.level_id) {
                Ok(level_data) => level_data,
                Err(err) => {
                    error!(
                        level.id = self.level_id,
                        ?err,
                        "Failed to reload level data"
                    );
                    return SceneTransition::None;
                }
            };
            let level = match level_data.load(ctx.assets) {
                Ok(level) => level,
                Err(err) => {
                    error!(
                        level.id = self.level_id,
                        ?err,
                        "Failed to rebuild level for reset"
                    );
                    return SceneTransition::None;
                }
            };

            SceneTransition::Replace(Box::new(MapSceneLogic::new(
                level,
                self.level_id.clone(),
                self.game_context.clone(),
            )))
        } else if ctx.inputs.is_action_just_pressed("menu_select") {
            if let Some(level_id) = &self.next_level {
                let level_data = match LevelData::from_id(level_id) {
                    Ok(level_data) => level_data,
                    Err(err) => {
                        error!(
                            level.id = self.level_id,
                            ?err,
                            "Failed to reload level data"
                        );
                        return SceneTransition::None;
                    }
                };
                let level = match level_data.load(ctx.assets) {
                    Ok(level) => level,
                    Err(err) => {
                        error!(
                            level.id = level_id,
                            ?err,
                            "Failed to rebuild level for reset"
                        );
                        return SceneTransition::None;
                    }
                };

                SceneTransition::Replace(Box::new(MapSceneLogic::new(
                    level,
                    level_id,
                    self.game_context.clone(),
                )))
            } else {
                SceneTransition::Replace(Box::new(WinSceneLogic::new(self.game_context.clone())))
            }
        } else if ctx.inputs.is_action_just_pressed("cancel") {
            SceneTransition::Replace(Box::new(TitleSceneLogic::new(self.game_context.clone())))
        } else {
            SceneTransition::None
        }
    }
}
