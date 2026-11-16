/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use crate::game_context::{GameContext, SharedGameContext};
use crate::level::LevelData;
use crate::managers::locale_manager::format_timer;
use crate::scenes::map_scene::MapSceneLogic;
use crate::scenes::pause_scene::PauseSceneLogic;
use crate::time::GameTime;
use engine::assets::audio::Audio;
use engine::assets::font::Font;
use engine::assets::texture::Texture;
use engine::context::UpdateCtx;
use engine::game_director::{GameSceneLogic, SceneTransition};
use engine::renderer::{BASE_VIRTUAL_HEIGHT, BASE_VIRTUAL_WIDTH};
use engine::scene::{
    EntityHandle, LocalTransform, Material, Scene, SpriteComponent, TextAnchor, TextComponent,
    TextLayout, TextStyle,
};
use glam::{Vec2, vec2};
use std::time::Duration;
use tracing::{debug, error};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TitleMenuSelection {
    #[default]
    Play,
    Options,
}

pub struct TitleSceneLogic {
    background_entity: Option<EntityHandle>,
    title_text_entity: Option<EntityHandle>,
    subtitle_text_entity: Option<EntityHandle>,
    play_btn_entity: Option<EntityHandle>,
    options_btn_entity: Option<EntityHandle>,
    level_best_time_entity: Option<EntityHandle>,
    active_lang_id: Option<String>,
    selection: TitleMenuSelection,
    selected_level_index: u32,
    available_levels: u32,
    game_context: SharedGameContext,
}

impl TitleSceneLogic {
    pub fn new(game_context: SharedGameContext) -> Self {
        let available_levels = game_context.borrow().unlocked_levels();
        let default_level = if available_levels == game_context.borrow().levels.count() as u32 {
            0
        } else {
            available_levels - 1
        };
        Self {
            background_entity: None,
            title_text_entity: None,
            subtitle_text_entity: None,
            play_btn_entity: None,
            options_btn_entity: None,
            level_best_time_entity: None,
            active_lang_id: None,
            selection: Default::default(),
            selected_level_index: default_level,
            available_levels,
            game_context,
        }
    }

    fn menu_style(font: engine::assets::AssetHandle<Font>, selected: bool) -> TextStyle {
        TextStyle {
            font,
            line_height: None,
            scale: 1.0,
            color: if selected {
                [255, 255, 0, 255]
            } else {
                [255, 255, 255, 255]
            },
        }
    }

    fn update_menu_styles(&self, scene: &mut Scene) {
        let play_text_color = if self.selection == TitleMenuSelection::Play {
            [255, 255, 0, 255]
        } else {
            [255, 255, 255, 255]
        };
        let options_text_color = if self.selection == TitleMenuSelection::Options {
            [255, 255, 0, 255]
        } else {
            [255, 255, 255, 255]
        };

        scene
            .get_component_mut::<TextComponent>(self.play_btn_entity.unwrap())
            .unwrap()
            .update_style(|it| it.color = play_text_color);
        scene
            .get_component_mut::<TextComponent>(self.options_btn_entity.unwrap())
            .unwrap()
            .update_style(|it| it.color = options_text_color);
    }

    fn set_text(scene: &mut Scene, entity: Option<EntityHandle>, text: String) {
        let Some(entity) = entity else {
            return;
        };
        if let Some(component) = scene.get_component_mut::<TextComponent>(entity) {
            component.set_text(text);
        }
    }

    fn get_play_button_text(&self, ctx: &GameContext) -> String {
        if self.available_levels == 1 {
            ctx.locales.get_string("menu.play")
        } else {
            let left_arrow = if self.selected_level_index == 0 {
                "  "
            } else {
                "◀"
            };
            let right_arrow = if self.selected_level_index == self.available_levels - 1 {
                "  "
            } else {
                "▶"
            };
            let text = ctx.locales.get_string_ext(
                "menu.level",
                [("level".into(), (self.selected_level_index + 1).to_string())],
            );
            format!("{left_arrow}{text}{right_arrow}")
        }
    }

    fn get_best_time(&self) -> Option<GameTime> {
        let ctx = self.game_context.borrow();
        let current_level = ctx.levels.level_of_index(self.selected_level_index)?;
        let data = ctx.saves.save_data().level_data(current_level)?;
        data.best_time
    }

    fn update_localized_texts(&mut self, scene: &mut Scene) {
        let (lang_id, _title_text, subtitle_text, play_text, options_text, best_time_text) = {
            let ctx = self.game_context.borrow();
            let locales = &ctx.locales;
            (
                ctx.config.lang_id().to_string(),
                locales.get_string("menu.title"),
                locales.get_string("menu.subtitle"),
                self.get_play_button_text(&ctx),
                locales.get_string("menu.options"),
                if let Some(best_time) = self.get_best_time() {
                    locales.get_string_ext(
                        "menu.best_time",
                        [("best_time".into(), format_timer(best_time.into()))],
                    )
                } else {
                    "".to_string()
                },
            )
        };

        Self::set_text(
            scene,
            self.subtitle_text_entity,
            format!("{subtitle_text} {}", env!("CARGO_PKG_VERSION")),
        );
        Self::set_text(scene, self.play_btn_entity, play_text);
        Self::set_text(scene, self.options_btn_entity, options_text);
        Self::set_text(scene, self.level_best_time_entity, best_time_text);
        self.active_lang_id = Some(lang_id);
    }

    fn refresh_localized_texts_if_needed(&mut self, scene: &mut Scene) {
        let lang_id = {
            let ctx = self.game_context.borrow();
            ctx.config.lang_id().to_string()
        };

        if self.active_lang_id.as_deref() != Some(lang_id.as_str()) {
            self.update_localized_texts(scene);
        }
    }
}

impl GameSceneLogic for TitleSceneLogic {
    fn on_enter(&mut self, scene: &mut Scene, ctx: &mut UpdateCtx<'_>) {
        let title_music = ctx
            .assets
            .get_handle::<Audio>("assets/audio/music/title.ogg")
            .unwrap();
        let title_music = ctx.assets.resolve_handle::<Audio>(title_music).unwrap();
        ctx.audio.play_music(title_music);

        let jersey_font = ctx.assets.get_handle::<Font>("Tiny5Duo").unwrap();
        let tiny_font = ctx.assets.get_handle::<Font>("Tiny5").unwrap();

        let title_style = TextStyle {
            font: jersey_font,
            line_height: None,
            scale: 2.0,
            color: [245, 245, 245, 255],
        };
        let subtitle_style = TextStyle {
            font: tiny_font,
            line_height: None,
            scale: 1.0,
            color: [245, 245, 245, 255],
        };
        let menu_style = Self::menu_style(tiny_font, false);

        let (lang_id, subtitle_text, play_text, options_text) = {
            let ctx = self.game_context.borrow();
            let locales = &ctx.locales;

            (
                ctx.config.lang_id().to_string(),
                locales.get_string("menu.subtitle").to_string(),
                locales.get_string("menu.play").to_string(),
                locales.get_string("menu.options").to_string(),
            )
        };

        let title_tex = ctx
            .assets
            .get_handle::<Texture>("assets/textures/overtaken_logo.png")
            .unwrap();
        let title_material = Material::new_opaque(title_tex);
        self.title_text_entity = Some(scene.spawn_sprite_entity(
            LocalTransform::from_xyz_screenspace(0.0, -50.0, 10.0),
            SpriteComponent::new(title_material, 2.0 * vec2(59.0, 13.0)),
        ));

        let subtitle_layout = TextLayout {
            max_width: Some(BASE_VIRTUAL_WIDTH as f32),
            max_height: None,
            anchor: TextAnchor::HorizonCenter,
        };
        self.subtitle_text_entity = Some(scene.spawn_text_entity(
            LocalTransform::from_xyz_screenspace(0.0, -20.0, 10.0),
            TextComponent::new(subtitle_text, subtitle_style, subtitle_layout),
        ));

        let menu_layout = TextLayout {
            max_width: Some(BASE_VIRTUAL_WIDTH as f32),
            max_height: Some(BASE_VIRTUAL_HEIGHT as f32),
            anchor: TextAnchor::HorizonCenter,
        };

        self.play_btn_entity = Some(scene.spawn_text_entity(
            LocalTransform::from_xyz_screenspace(0.0, 50.0, 10.0),
            TextComponent::new(play_text, menu_style, menu_layout),
        ));
        self.options_btn_entity = Some(scene.spawn_text_entity(
            LocalTransform::from_xyz_screenspace(0.0, 60.0, 10.0),
            TextComponent::new(options_text, menu_style, menu_layout),
        ));
        self.level_best_time_entity = Some(scene.spawn_text_entity(
            LocalTransform::from_xyz_screenspace(0.0, 80.0, 10.0),
            TextComponent::new(
                "",
                TextStyle {
                    font: tiny_font,
                    line_height: None,
                    scale: 1.0,
                    color: [192, 192, 192, 255],
                },
                menu_layout,
            ),
        ));

        let background_tex = ctx
            .assets
            .get_handle::<Texture>("assets/textures/background.png")
            .unwrap();
        let background_mat = Material::new_opaque(background_tex);
        let mut transform_background = LocalTransform::from_xyz(0.0, 0.0, -100.0);
        transform_background.screen_space = true;

        self.background_entity = Some(scene.spawn_sprite_entity(
            transform_background,
            SpriteComponent::new(
                background_mat,
                Vec2::new(
                    BASE_VIRTUAL_WIDTH as f32 * 2.0,
                    BASE_VIRTUAL_HEIGHT as f32 * 2.0,
                ),
            ),
        ));
        debug!("Background entity: {:?}", self.background_entity);

        self.active_lang_id = Some(lang_id);
        self.update_menu_styles(scene);
        self.update_localized_texts(scene);
    }

    fn update(
        &mut self,
        scene: &mut Scene,
        _dt: std::time::Duration,
        ctx: &mut UpdateCtx<'_>,
    ) -> SceneTransition {
        self.refresh_localized_texts_if_needed(scene);
        self.update_menu_styles(scene);

        if !ctx.is_top_scene() {
            return SceneTransition::None;
        }

        if ctx.inputs.is_action_just_pressed("menu_select") {
            match self.selection {
                TitleMenuSelection::Play => {
                    let Some(level_id) = ({
                        let context = self.game_context.borrow();
                        context
                            .levels
                            .level_of_index(self.selected_level_index)
                            .map(|it| it.to_string())
                    }) else {
                        error!(
                            level.index = self.selected_level_index,
                            "Couldn't find level of index"
                        );
                        return SceneTransition::None;
                    };

                    let level_data = match LevelData::from_id(&level_id) {
                        Ok(level_data) => level_data,
                        Err(err) => {
                            error!(level.id = level_id, ?err, "Failed to load level data");
                            return SceneTransition::None;
                        }
                    };
                    let level = match level_data.load(ctx.assets) {
                        Ok(level) => level,
                        Err(err) => {
                            error!(level.id = level_id, ?err, "Failed to build level");
                            return SceneTransition::None;
                        }
                    };

                    return SceneTransition::Replace(Box::new(MapSceneLogic::new(
                        level,
                        level_id,
                        self.game_context.clone(),
                    )));
                }
                TitleMenuSelection::Options => {
                    return SceneTransition::Push(Box::new(PauseSceneLogic::new(
                        self.game_context.clone(),
                        true,
                    )));
                }
            }
        }

        if ctx.inputs.is_action_just_pressed("menu_up")
            || ctx.inputs.is_action_just_pressed("menu_down")
        {
            self.selection = match self.selection {
                TitleMenuSelection::Play => TitleMenuSelection::Options,
                TitleMenuSelection::Options => TitleMenuSelection::Play,
            };
            self.update_menu_styles(scene);
        }

        if self.selection == TitleMenuSelection::Play {
            if ctx.inputs.is_action_just_pressed("menu_left") {
                self.selected_level_index = self.selected_level_index.saturating_sub(1);
                self.update_localized_texts(scene);
            } else if ctx.inputs.is_action_just_pressed("menu_right") {
                self.selected_level_index = self
                    .selected_level_index
                    .saturating_add(1)
                    .min(self.available_levels - 1);
                self.update_localized_texts(scene);
            }
        }

        SceneTransition::None
    }
}
